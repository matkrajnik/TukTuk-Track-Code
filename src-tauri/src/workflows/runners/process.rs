//! Wstrzykiwany nadzorca procesu: prawdziwy (`agent_cli`) albo skryptowany w testach.

use std::time::Duration;

use agent_cli::{
    KillResult, Options, ProcessEvent, RunningProcess, SpawnError, Spec, StderrTail,
    StreamingProcess,
};

/// Uruchomiony proces widziany przez runner.
pub trait Proc: Send {
    /// Następne zdarzenie albo `None` po upływie `timeout`.
    fn next(&self, timeout: Duration) -> Option<ProcessEvent>;
    fn kill(&self, confirm_timeout: Duration) -> KillResult;
    fn stderr_tail(&self) -> StderrTail;
}

pub trait Spawner: Send + Sync {
    fn spawn(&self, spec: &Spec) -> Result<Box<dyn Proc>, SpawnError>;
}

/// Prawdziwy proces: czyste środowisko (`env_clear` + lista z `spec.env`).
pub struct RealSpawner;

pub(crate) struct RealProc(pub(crate) RunningProcess);

impl Proc for RealProc {
    fn next(&self, timeout: Duration) -> Option<ProcessEvent> {
        match self.0.events().recv_timeout(timeout) {
            Ok(event) => Some(event),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
            // Wątki nadzorcy zniknęły bez `Exited`: traktujemy jak koniec bez kodu.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Some(ProcessEvent::Exited(None))
            }
        }
    }

    fn kill(&self, confirm_timeout: Duration) -> KillResult {
        self.0.kill(confirm_timeout)
    }

    fn stderr_tail(&self) -> StderrTail {
        self.0.stderr_tail()
    }
}

impl RealSpawner {
    /// Opcje prawdziwego procesu: czyste środowisko, Job Object i `taskkill`.
    pub fn options() -> Options {
        Options {
            clear_env: true,
            ..Options::default()
        }
    }
}

impl Spawner for RealSpawner {
    fn spawn(&self, spec: &Spec) -> Result<Box<dyn Proc>, SpawnError> {
        StreamingProcess::spawn(spec, Self::options())
            .map(|p| Box::new(RealProc(p)) as Box<dyn Proc>)
    }
}
