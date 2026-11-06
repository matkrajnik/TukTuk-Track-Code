//! Ręczne wznowienie przebiegu wstrzymanego po restarcie (`Paused`, decyzja po E7).
//!
//! Wznowienie idzie przez wątek roboczy (jak decyzja o zgodzie), więc dwa
//! wywołania nie ścigają się o sekwencję. Powtórka z tym samym `request_id` nie
//! zmienia stanu i nie uruchamia drugiego kroku. Przed wznowieniem katalog roboczy
//! jest sprawdzany (`WorkspaceAccess::verify`), poza blokadą repozytorium; naruszony
//! kończy przebieg `Failed`/`WorkspaceUnavailable` bez uruchomienia agenta.

use std::fmt;

use super::Worker;
use crate::engine::Input;
use crate::ports::{RepoError, RunId};
use crate::run::RunStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeError {
    NotFound,
    /// Przebieg nie jest wstrzymany, a ten `request_id` go nie wznawiał.
    NotPaused,
    /// Sterownik się zatrzymuje albo jest zatrzymany (wyjście z aplikacji).
    ShuttingDown,
    Storage(String),
}

impl fmt::Display for ResumeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResumeError::NotFound => f.write_str("Nie znaleziono przebiegu."),
            ResumeError::NotPaused => f.write_str("Przebieg nie jest wstrzymany."),
            ResumeError::ShuttingDown => {
                f.write_str("Aplikacja się zamyka — wznów przebieg po ponownym uruchomieniu.")
            }
            ResumeError::Storage(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ResumeError {}

fn storage(e: RepoError) -> ResumeError {
    match e {
        RepoError::NotFound => ResumeError::NotFound,
        other => ResumeError::Storage(other.to_string()),
    }
}

impl Worker {
    /// Wznawia `Paused` i kolejkuje przebieg; zwraca stan po wznowieniu.
    pub(super) fn resume_run(
        &mut self,
        run_id: RunId,
        request_id: &str,
    ) -> Result<RunStatus, ResumeError> {
        // Przy kontrolowanym zatrzymaniu nowy krok i tak by nie ruszył.
        if self.graceful.is_some() || self.stopping {
            return Err(ResumeError::ShuttingDown);
        }
        let (_, state) = self.load(run_id).map_err(storage)?;
        if state.resume_requests.iter().any(|id| id == request_id) {
            return Ok(state.status);
        }
        if state.status != RunStatus::Paused {
            return Err(ResumeError::NotPaused);
        }
        // Git poza blokadą repozytorium (jak skrót artefaktu zgody).
        let workspace_error = self
            .workspace
            .verify(run_id)
            .map_err(|e| ResumeError::Storage(format!("Nie sprawdzono katalogu roboczego: {e}")))?;
        let input = Input::Resume {
            request_id: request_id.to_owned(),
            workspace_error,
        };
        let committed = self
            .step_with(run_id, |state| {
                let repeated = state.resume_requests.iter().any(|id| id == request_id);
                (repeated || state.status == RunStatus::Paused).then(|| input.clone())
            })
            .map_err(|e| ResumeError::Storage(e.to_string()))?
            .ok_or(ResumeError::NotPaused)?;
        let status = committed.state.status;
        // Tylko faktyczne wznowienie kolejkuje przebieg (powtórka niczego nie dokłada).
        if status == RunStatus::Running && !committed.transition.events.is_empty() {
            self.enqueue(run_id);
        }
        Ok(status)
    }
}
