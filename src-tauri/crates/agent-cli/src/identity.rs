//! Tożsamość procesu odporna na ponowne użycie PID (PLAN_WDROZENIA.md, E6).
//!
//! PID sam nie wystarcza: po awarii aplikacji system może nadać ten numer innemu
//! procesowi. Dlatego razem z PID zapisujemy czas utworzenia procesu (FILETIME);
//! inny czas oznacza, że to nie jest proces, który uruchomiliśmy, i nie wolno go zabijać.
//!
//! Zabijanie trzyma jeden uchwyt procesu od sprawdzenia czasu utworzenia do
//! potwierdzenia końca: otwarty uchwyt nie pozwala systemowi nadać PID innemu
//! procesowi (brak TOCTOU między sprawdzeniem a `taskkill`), a koniec potwierdza
//! `WaitForSingleObject` na tym samym uchwycie.

use std::time::Duration;

use crate::tree::{kill_tree_by_pid, KillResult};

/// Identyfikacja procesu zapisywana w rejestrze procesów.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Czas utworzenia procesu (FILETIME, 100 ns od 1601); 0 poza Windows.
    pub created_ft: u64,
    /// Czy proces trafił do Job Object (informacyjnie; po awarii hosta job jest
    /// zamknięty, więc nie dowodzi końca potomków).
    pub job_attached: bool,
}

/// Wynik porównania zapisanej tożsamości z tym, co działa w systemie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// Procesu o tym PID nie ma albo już się zakończył.
    Gone,
    /// Ten sam proces nadal działa.
    SameAlive,
    /// PID należy już do innego procesu (inny czas utworzenia).
    Reused,
}

#[cfg(windows)]
mod imp {
    use super::{Probe, ProcessIdentity};
    use crate::tree::KillResult;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, GetProcessTimes, OpenProcess, TerminateProcess, WaitForSingleObject,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    /// Kod wyjścia raportowany dla procesu, który jeszcze działa.
    const STILL_ACTIVE: u32 = 259;

    /// Uchwyt procesu zamykany dokładnie raz (Drop).
    struct Handle(HANDLE);

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: uchwyt pochodzi z udanego `OpenProcess` i jest zamykany raz.
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Handle {
        fn open(pid: u32, rights: u32) -> Option<Handle> {
            // SAFETY: otwarcie uchwytu procesu; zero oznacza błąd.
            let handle = unsafe { OpenProcess(rights, 0, pid) };
            (!handle.is_null()).then_some(Handle(handle))
        }

        fn created_ft(&self) -> Option<u64> {
            let zero = FILETIME {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            };
            let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
            // SAFETY: uchwyt jest ważny, wskaźniki wskazują na zmienne żyjące przez wywołanie.
            let ok = unsafe {
                GetProcessTimes(self.0, &mut created, &mut exited, &mut kernel, &mut user)
            };
            (ok != 0).then(|| {
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime)
            })
        }

        fn alive(&self) -> Option<bool> {
            let mut code = 0_u32;
            // SAFETY: jak wyżej; `code` żyje przez wywołanie.
            let ok = unsafe { GetExitCodeProcess(self.0, &mut code) };
            (ok != 0).then_some(code == STILL_ACTIVE)
        }

        /// Kończy proces tym uchwytem (nie po PID) i czeka na jego koniec.
        fn terminate_and_wait(&self, timeout: Duration) -> bool {
            if self.alive() == Some(true) {
                // SAFETY: uchwyt ma prawo PROCESS_TERMINATE; błąd sprawdza oczekiwanie niżej.
                unsafe { TerminateProcess(self.0, 1) };
            }
            let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
            // SAFETY: uchwyt ma prawo SYNCHRONIZE i jest ważny przez całe wywołanie.
            unsafe { WaitForSingleObject(self.0, millis) == WAIT_OBJECT_0 }
        }
    }

    pub fn capture(pid: u32, job_attached: bool) -> Option<ProcessIdentity> {
        let handle = Handle::open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        Some(ProcessIdentity {
            pid,
            created_ft: handle.created_ft()?,
            job_attached,
        })
    }

    pub fn probe(identity: &ProcessIdentity) -> Probe {
        let Some(handle) = Handle::open(identity.pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
            return Probe::Gone;
        };
        match (handle.created_ft(), handle.alive()) {
            (Some(ft), _) if ft != identity.created_ft => Probe::Reused,
            (Some(_), Some(true)) => Probe::SameAlive,
            _ => Probe::Gone,
        }
    }

    pub fn kill_tree_if_same(
        identity: &ProcessIdentity,
        confirm_timeout: Duration,
        kill_tree: &dyn Fn(u32) -> bool,
    ) -> KillResult {
        let rights = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE;
        let Some(handle) = Handle::open(identity.pid, rights) else {
            // Bez uchwytu z prawem zakończenia: albo procesu nie ma, albo nie wolno
            // go zakończyć. Rozstrzyga zwykłe sprawdzenie.
            return match probe(identity) {
                Probe::Gone | Probe::Reused => KillResult::Confirmed,
                Probe::SameAlive => KillResult::Unconfirmed,
            };
        };
        match (handle.created_ft(), handle.alive()) {
            // Ponownie użyty PID: oryginał już nie istnieje, cudzego procesu nie ruszamy.
            (Some(ft), _) if ft != identity.created_ft => return KillResult::Confirmed,
            (Some(_), Some(true)) => {}
            (Some(_), Some(false)) => return KillResult::Confirmed,
            _ => return KillResult::Unconfirmed,
        }
        // Uchwyt jest otwarty, więc PID nie może trafić do innego procesu: `taskkill /T`
        // dotyczy naszego drzewa. Potomków bez działającego jobu (awaria hosta zamknęła
        // uchwyt jobu, a proces przeżył) kończy tylko `taskkill`.
        let tree_killed = kill_tree(identity.pid);
        let root_ended = handle.terminate_and_wait(confirm_timeout);
        if root_ended && tree_killed {
            KillResult::Confirmed
        } else {
            KillResult::Unconfirmed
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Probe, ProcessIdentity};
    use crate::tree::KillResult;
    use std::time::Duration;

    pub fn capture(pid: u32, job_attached: bool) -> Option<ProcessIdentity> {
        Some(ProcessIdentity {
            pid,
            created_ft: 0,
            job_attached,
        })
    }

    pub fn probe(_identity: &ProcessIdentity) -> Probe {
        Probe::Gone
    }

    pub fn kill_tree_if_same(
        _identity: &ProcessIdentity,
        _confirm_timeout: Duration,
        _kill_tree: &dyn Fn(u32) -> bool,
    ) -> KillResult {
        KillResult::Confirmed
    }
}

impl ProcessIdentity {
    /// Odczytuje tożsamość działającego (lub dopiero co zakończonego) procesu.
    pub fn capture(pid: u32, job_attached: bool) -> Option<ProcessIdentity> {
        imp::capture(pid, job_attached)
    }
}

/// Porównuje zapisaną tożsamość z procesem w systemie.
pub fn probe(identity: &ProcessIdentity) -> Probe {
    imp::probe(identity)
}

/// Zabija drzewo procesu tylko wtedy, gdy to nadal ten sam proces, i potwierdza
/// jego koniec. Proces z innym czasem utworzenia (ponownie użyty PID) nie jest
/// ruszany: oryginał już nie istnieje, więc wynik to `Confirmed`. `Confirmed` po
/// zabijaniu wymaga zarówno udanego `taskkill /T` (potomkowie), jak i potwierdzonego
/// końca procesu głównego; `job_attached` niczego tu nie zastępuje.
pub fn kill_tree_if_same(identity: &ProcessIdentity, confirm_timeout: Duration) -> KillResult {
    imp::kill_tree_if_same(identity, confirm_timeout, &kill_tree_by_pid)
}

/// Jak [`kill_tree_if_same`], z podstawionym kończeniem drzewa (testy wstrzykują
/// niepowodzenie `taskkill`).
#[doc(hidden)]
pub fn kill_tree_if_same_with(
    identity: &ProcessIdentity,
    confirm_timeout: Duration,
    kill_tree: &dyn Fn(u32) -> bool,
) -> KillResult {
    imp::kill_tree_if_same(identity, confirm_timeout, kill_tree)
}
