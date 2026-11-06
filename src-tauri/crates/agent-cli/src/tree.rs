//! Kończenie całego drzewa procesów: Job Object (Windows) plus `taskkill /T /F`.

/// Wynik próby zakończenia procesu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KillResult {
    /// Koniec procesu potwierdzony przed upływem limitu.
    Confirmed,
    /// Nie udało się potwierdzić końca; wynik kroku nie powinien być zwracany.
    Unconfirmed,
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// Job z `KILL_ON_JOB_CLOSE`: zamknięcie uchwytu (także przy zamknięciu
    /// aplikacji) kończy wszystkie procesy w jobie.
    pub struct Job(HANDLE);

    // SAFETY: uchwyt jobu to identyfikator jądra; wywołania na nim są bezpieczne z dowolnego wątku.
    unsafe impl Send for Job {}
    // SAFETY: jak wyżej, wszystkie operacje są atomowe po stronie jądra.
    unsafe impl Sync for Job {}

    impl Job {
        /// Tworzy job i dołącza proces o danym uchwycie. Okno wyścigu: proces
        /// zdążył utworzyć potomka przed przypisaniem; drugie przejście
        /// `taskkill /T` w [`kill_tree_by_pid`] kończy takie drzewo. Błąd jest
        /// zwracany (nie połykany): bez jobu koniec drzewa potwierdza tylko `taskkill`.
        // `HANDLE` jest nieprzezroczystym uchwytem jądra, nie wskaźnikiem do pamięci:
        // funkcja go nie dereferuje, a zły uchwyt kończy się błędem API.
        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        pub fn attach(process: HANDLE) -> Result<Job, String> {
            // SAFETY: tworzenie anonimowego jobu bez atrybutów zabezpieczeń.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(last_error("CreateJobObjectW"));
            }
            let job = Job(job);
            // SAFETY: struktura składa się z liczb i wskaźników, zero to poprawna wartość początkowa.
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `job.0` to ważny uchwyt, `info` żyje przez całe wywołanie, a rozmiar zgadza się z typem.
            let set = unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    std::ptr::addr_of!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if set == 0 {
                return Err(last_error("SetInformationJobObject"));
            }
            // SAFETY: oba uchwyty są ważne; proces należy do tego wywołującego.
            let assigned = unsafe { AssignProcessToJobObject(job.0, process) };
            if assigned == 0 {
                return Err(last_error("AssignProcessToJobObject"));
            }
            Ok(job)
        }

        pub fn terminate(&self) -> bool {
            // SAFETY: `self.0` to ważny uchwyt jobu aż do `drop`.
            unsafe { TerminateJobObject(self.0, 1) != 0 }
        }

        /// Liczba żywych procesów w jobie; `None`, gdy zapytanie się nie udało.
        pub fn active_processes(&self) -> Option<u32> {
            // SAFETY: struktura składa się z liczb, zero to poprawna wartość początkowa.
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
            // SAFETY: `self.0` to ważny uchwyt, bufor ma rozmiar typu i żyje przez całe wywołanie.
            let ok = unsafe {
                QueryInformationJobObject(
                    self.0,
                    JobObjectBasicAccountingInformation,
                    std::ptr::addr_of_mut!(info).cast(),
                    std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                )
            };
            (ok != 0).then_some(info.ActiveProcesses)
        }
    }

    fn last_error(what: &str) -> String {
        format!("{what}: {}", std::io::Error::last_os_error())
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: uchwyt został utworzony w `attach` i jest zamykany dokładnie raz.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// `taskkill /T /F /PID`; `true`, gdy polecenie zakończyło się sukcesem.
    pub fn kill_tree_by_pid(pid: u32) -> bool {
        use std::os::windows::process::CommandExt;
        let program = std::env::var_os("SystemRoot")
            .map(|root| PathBuf::from(root).join("System32").join("taskkill.exe"))
            .unwrap_or_else(|| PathBuf::from("taskkill.exe"));
        Command::new(program)
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(windows)]
pub use imp::{kill_tree_by_pid, Job};

#[cfg(not(windows))]
pub struct Job;

#[cfg(not(windows))]
impl Job {
    pub fn terminate(&self) -> bool {
        false
    }

    pub fn active_processes(&self) -> Option<u32> {
        None
    }
}

#[cfg(not(windows))]
pub fn kill_tree_by_pid(_pid: u32) -> bool {
    false
}
