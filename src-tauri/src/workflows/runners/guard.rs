//! Strażnik profilu dostępu (PLAN_WDROZENIA.md, 8.3, E4): dekorator `AgentRunner`.
//!
//! Dla kroków tylko do odczytu porównuje migawkę worktree przed krokiem i po
//! nim. To „kontrola CLI + wykrywanie zmian”, nie izolacja systemu plików.
//! Profil pochodzi wyłącznie z definicji grafu (`request.access`); tekst modelu
//! go nie zmienia.

use std::sync::Arc;

use workflow_core::model::{AccessProfile, AgentRole};
use workflow_core::ports::{
    AgentRunner, CancelToken, Capabilities, RunRequest, RunnerEvent, RunnerOutcome,
};
use workflow_core::FailureCode;

use crate::workflows::workspace::{GitWorkspace, WorkspaceError};

pub const READ_ONLY_VIOLATION: &str = "Krok tylko do odczytu zmienił pliki w katalogu roboczym (kontrola CLI + wykrywanie zmian, nie izolacja systemu plików).";
const HASH_MISMATCH: &str =
    "Review sprawdziło inny artefakt niż aktualny stan katalogu roboczego (niezgodny skrót).";

pub struct GuardedRunner {
    inner: Arc<dyn AgentRunner>,
    workspace: Arc<GitWorkspace>,
}

impl GuardedRunner {
    pub fn new(inner: Arc<dyn AgentRunner>, workspace: Arc<GitWorkspace>) -> Self {
        Self { inner, workspace }
    }
}

fn failed(code: FailureCode, message: impl Into<String>) -> RunnerOutcome {
    RunnerOutcome::Failed {
        code,
        message: message.into(),
    }
}

/// Naruszenie (np. podmieniony `.git`) zostaje naruszeniem; reszta to błąd zapisu.
fn workspace_failure(error: WorkspaceError) -> RunnerOutcome {
    match error {
        WorkspaceError::PermissionViolation(m) => failed(FailureCode::PermissionViolation, m),
        WorkspaceError::Cancelled => RunnerOutcome::Cancelled,
        other => failed(FailureCode::StorageError, other.to_string()),
    }
}

impl AgentRunner for GuardedRunner {
    /// Strażnik niczego nie egzekwuje na poziomie systemu plików: wykrywa zmiany
    /// po fakcie i odrzuca wynik. Dlatego `read_only_enforced` pochodzi z runnera
    /// wewnętrznego (dla CLI: `false`), a nie jest podnoszony do `true`.
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }

    fn run(
        &self,
        request: &RunRequest,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
    ) -> RunnerOutcome {
        if request.workspace.is_none() {
            return failed(
                FailureCode::StorageError,
                "Przebieg nie ma katalogu roboczego; krok nie zostanie uruchomiony.",
            );
        }
        // Skrót przed migawką: `git add -N` w skrócie zmienia indeks, więc robimy to
        // raz, zanim zaczniemy porównywać stan.
        let expected_hash = if request.role == AgentRole::Review {
            match self
                .workspace
                .artifact_hash_with(request.run_id, Some(cancel))
            {
                Ok(hash) => Some(hash),
                Err(e) => return workspace_failure(e),
            }
        } else {
            None
        };
        let watch = request.access == AccessProfile::ReadOnly;
        let before = if watch {
            match self.workspace.snapshot_with(request.run_id, cancel) {
                Ok(s) => Some(s),
                Err(e) => return workspace_failure(e),
            }
        } else {
            None
        };
        // Migawka i skrót to kilka poleceń Git; anulowanie w ich trakcie nie
        // uruchamia już agenta (sterownik czeka tylko krótki czas łaski).
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            return RunnerOutcome::Cancelled;
        }
        let outcome = self.inner.run(request, cancel, on_event);
        // Anulowany krok nie daje wyniku do akceptacji, a sterownik czeka na
        // odpowiedź tylko przez krótki czas łaski: migawka (kilka poleceń Git) po
        // anulowaniu mogłaby go przekroczyć i zamienić Cancelled w Interrupted.
        let before = before.filter(|_| outcome != RunnerOutcome::Cancelled);
        if let Some(before) = before {
            match self.workspace.snapshot_with(request.run_id, cancel) {
                Ok(after) if after == before => {}
                Ok(_) => return failed(FailureCode::PermissionViolation, READ_ONLY_VIOLATION),
                Err(e) => return workspace_failure(e),
            }
        }
        match (&outcome, expected_hash) {
            (RunnerOutcome::Completed { output, .. }, Some(expected))
                if output["checkedArtifactHash"].as_str() != Some(expected.as_str()) =>
            {
                failed(FailureCode::InvalidOutput, HASH_MISMATCH)
            }
            _ => outcome,
        }
    }
}
