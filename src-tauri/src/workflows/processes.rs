//! Rejestr procesów agentów (PLAN_WDROZENIA.md, 5.4 i 8.3; etap E6).
//!
//! Runner zapisuje PID razem z czasem utworzenia procesu zaraz po starcie i stan
//! końcowy po jego końcu. Po awarii hosta rekonsyliacja (`recovery`) sprawdza każdy
//! wpis bez potwierdzonego końca: ponownie użytego PID nie zabija, ten sam żywy
//! proces kończy razem z drzewem, a niepotwierdzony koniec zostawia jako `unknown`
//! (rezerwacja katalogu roboczego zostaje). Sprawdzanie i zabijanie procesów
//! odbywa się poza blokadą bazy; baza jest blokowana tylko na krótkie zapisy.

use std::time::Duration;

use agent_cli::{KillResult, Probe, ProcessIdentity};
use rusqlite::params;
use workflow_core::ports::RunId;

use super::workspace::SharedDb;

/// Czas na potwierdzenie końca procesu przy rekonsyliacji.
pub const RECONCILE_KILL_CONFIRM: Duration = Duration::from_secs(5);

/// Stan wpisu rejestru (kolumna `workflow_processes.state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Running,
    Exited,
    Killed,
    Unknown,
}

impl ProcState {
    pub fn as_str(self) -> &'static str {
        match self {
            ProcState::Running => "running",
            ProcState::Exited => "exited",
            ProcState::Killed => "killed",
            ProcState::Unknown => "unknown",
        }
    }

    fn parse(text: &str) -> ProcState {
        match text {
            "running" => ProcState::Running,
            "exited" => ProcState::Exited,
            "killed" => ProcState::Killed,
            _ => ProcState::Unknown,
        }
    }

    /// Koniec procesu potwierdzony (nie trzeba go już sprawdzać).
    pub fn is_settled(self) -> bool {
        matches!(self, ProcState::Exited | ProcState::Killed)
    }
}

/// Zapis procesów uruchamianych przez runner.
pub trait ProcessLedger: Send + Sync {
    /// Proces wystartował; wpis musi powstać, zanim runner zacznie czytać strumień.
    fn spawned(
        &self,
        run_id: RunId,
        execution_id: &str,
        identity: &ProcessIdentity,
    ) -> Result<(), String>;
    /// Proces się skończył (`Exited`, `Killed`) albo koniec jest niepotwierdzony (`Unknown`).
    fn ended(&self, execution_id: &str, state: ProcState) -> Result<(), String>;
}

/// Sprawdzanie i kończenie procesów systemu; testy podstawiają atrapę.
pub trait ProcessControl: Send + Sync {
    fn probe(&self, identity: &ProcessIdentity) -> Probe;
    fn kill_tree(&self, identity: &ProcessIdentity, confirm_timeout: Duration) -> KillResult;
}

/// Prawdziwe procesy systemu (`agent_cli`).
pub struct SystemProcesses;

impl ProcessControl for SystemProcesses {
    fn probe(&self, identity: &ProcessIdentity) -> Probe {
        agent_cli::probe(identity)
    }

    fn kill_tree(&self, identity: &ProcessIdentity, confirm_timeout: Duration) -> KillResult {
        agent_cli::kill_tree_if_same(identity, confirm_timeout)
    }
}

fn locked(db: &SharedDb) -> Result<std::sync::MutexGuard<'_, rusqlite::Connection>, String> {
    db.lock()
        .map_err(|_| "Blokada bazy jest zatruta.".to_owned())
}

/// Rejestr na SQLite (krótkie transakcje na wspólnej blokadzie bazy).
pub struct SqliteLedger {
    db: SharedDb,
    now: fn() -> i64,
}

impl SqliteLedger {
    pub fn new(db: SharedDb, now: fn() -> i64) -> Self {
        Self { db, now }
    }
}

impl ProcessLedger for SqliteLedger {
    fn spawned(
        &self,
        run_id: RunId,
        execution_id: &str,
        identity: &ProcessIdentity,
    ) -> Result<(), String> {
        let created = i64::try_from(identity.created_ft).map_err(|e| e.to_string())?;
        locked(&self.db)?
            .execute(
                "INSERT INTO workflow_processes(execution_id, run_id, pid, created_ft, job_attached, state, recorded_at)
                 VALUES(?1, ?2, ?3, ?4, ?5, 'running', ?6)
                 ON CONFLICT(execution_id) DO UPDATE SET pid = excluded.pid,
                   created_ft = excluded.created_ft, job_attached = excluded.job_attached,
                   state = 'running', recorded_at = excluded.recorded_at, ended_at = NULL",
                params![
                    execution_id,
                    run_id,
                    identity.pid,
                    created,
                    identity.job_attached,
                    (self.now)()
                ],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn ended(&self, execution_id: &str, state: ProcState) -> Result<(), String> {
        set_state(&self.db, execution_id, state, (self.now)())
    }
}

fn set_state(db: &SharedDb, execution_id: &str, state: ProcState, now: i64) -> Result<(), String> {
    let ended_at = state.is_settled().then_some(now);
    locked(db)?
        .execute(
            "UPDATE workflow_processes SET state = ?2, ended_at = ?3 WHERE execution_id = ?1",
            params![execution_id, state.as_str(), ended_at],
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Wpis rejestru bez potwierdzonego końca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenProcess {
    pub execution_id: String,
    pub run_id: RunId,
    pub identity: ProcessIdentity,
    pub state: ProcState,
}

/// Wpisy w stanie `running` albo `unknown` (wszystkie albo jednego przebiegu).
pub fn open_processes(db: &SharedDb, run_id: Option<RunId>) -> Result<Vec<OpenProcess>, String> {
    let conn = locked(db)?;
    let mut statement = conn
        .prepare(
            "SELECT execution_id, run_id, pid, created_ft, job_attached, state FROM workflow_processes
             WHERE state IN ('running', 'unknown') AND (?1 IS NULL OR run_id = ?1)
             ORDER BY recorded_at, execution_id",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([run_id], |r| {
            Ok(OpenProcess {
                execution_id: r.get(0)?,
                run_id: r.get(1)?,
                identity: ProcessIdentity {
                    pid: r.get(2)?,
                    created_ft: u64::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                    job_attached: r.get(4)?,
                },
                state: ProcState::parse(&r.get::<_, String>(5)?),
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

/// Rozstrzyga jeden wpis: proces, którego już nie ma, i ponownie użyty PID to
/// `Exited` (cudzego procesu nie ruszamy); ten sam żywy proces jest kończony z
/// drzewem: potwierdzony koniec to `Killed`, niepotwierdzony `Unknown`.
pub fn settle(
    db: &SharedDb,
    control: &dyn ProcessControl,
    process: &OpenProcess,
    now: i64,
) -> Result<ProcState, String> {
    // Sprawdzanie i zabijanie bez blokady bazy.
    let state = match control.probe(&process.identity) {
        Probe::Gone | Probe::Reused => ProcState::Exited,
        Probe::SameAlive => match control.kill_tree(&process.identity, RECONCILE_KILL_CONFIRM) {
            KillResult::Confirmed => ProcState::Killed,
            KillResult::Unconfirmed => ProcState::Unknown,
        },
    };
    set_state(db, &process.execution_id, state, now)?;
    Ok(state)
}

#[cfg(test)]
pub(crate) mod fake {
    //! Atrapa procesów systemu dla testów rekonsyliacji.

    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct FakeProcesses {
        /// Wynik `probe` dla PID; brak wpisu to `Gone`.
        pub probes: Mutex<HashMap<u32, Probe>>,
        /// Wynik `kill_tree` dla PID; brak wpisu to `Confirmed`.
        pub kills: Mutex<HashMap<u32, KillResult>>,
        pub killed: Mutex<Vec<u32>>,
    }

    impl FakeProcesses {
        pub fn with(pid: u32, probe: Probe, kill: KillResult) -> Self {
            let fake = Self::default();
            fake.probes.lock().unwrap().insert(pid, probe);
            fake.kills.lock().unwrap().insert(pid, kill);
            fake
        }

        pub fn killed(&self) -> Vec<u32> {
            self.killed.lock().unwrap().clone()
        }
    }

    impl ProcessControl for FakeProcesses {
        fn probe(&self, identity: &ProcessIdentity) -> Probe {
            *self
                .probes
                .lock()
                .unwrap()
                .get(&identity.pid)
                .unwrap_or(&Probe::Gone)
        }

        fn kill_tree(&self, identity: &ProcessIdentity, _timeout: Duration) -> KillResult {
            self.killed.lock().unwrap().push(identity.pid);
            let result = *self
                .kills
                .lock()
                .unwrap()
                .get(&identity.pid)
                .unwrap_or(&KillResult::Confirmed);
            if result == KillResult::Confirmed {
                self.probes
                    .lock()
                    .unwrap()
                    .insert(identity.pid, Probe::Gone);
            }
            result
        }
    }
}

#[cfg(test)]
mod tests;
