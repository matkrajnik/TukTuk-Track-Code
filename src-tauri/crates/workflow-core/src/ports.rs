//! Porty silnika: runner agenta, repozytorium zdarzeń, ujście zdarzeń i dostęp
//! do przestrzeni roboczej (PLAN_WDROZENIA.md, 6 i 8.1). Rdzeń nie zna SQLite,
//! Tauri ani procesów; host dostarcza implementacje.

use std::fmt;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::engine::{InputRef, RunEvent};
use crate::model::{AccessProfile, Provider, WorkflowGraph};
use crate::run::FailureCode;

pub type RunId = i64;

/// Znacznik anulowania kroku; runner sprawdza go w pętli i kończy pracę.
pub type CancelToken = Arc<AtomicBool>;

/// Wynik wcześniejszego kroku przekazany agentowi jako wejście.
#[derive(Debug, Clone, PartialEq)]
pub struct RunInput {
    pub node_id: String,
    pub execution_id: String,
    pub output: Value,
}

/// Żądanie uruchomienia kroku agenta (8.1).
#[derive(Debug, Clone)]
pub struct RunRequest {
    pub run_id: RunId,
    pub execution_id: String,
    pub node_id: String,
    pub workspace: Option<String>,
    pub provider: Provider,
    pub model: Option<String>,
    pub instruction: String,
    pub inputs: Vec<RunInput>,
    pub access: AccessProfile,
    pub timeout: Duration,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub read_only_enforced: bool,
    pub resumable_sessions: bool,
}

/// Zdarzenie przekazywane przez runner w trakcie kroku (informacyjne).
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerEvent {
    Message { text: String },
    ToolStarted { name: String },
    ToolFinished { name: String, ok: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunnerOutcome {
    Completed {
        output: Value,
        exit_code: i32,
        session_id: Option<String>,
    },
    Failed {
        code: FailureCode,
        message: String,
    },
    Cancelled,
}

/// Blokujący runner jednego kroku. Wołany z osobnego wątku, więc może czekać
/// na proces; kończy pracę po ustawieniu `cancel`.
pub trait AgentRunner: Send + Sync {
    fn capabilities(&self) -> Capabilities;
    fn run(
        &self,
        request: &RunRequest,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
    ) -> RunnerOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoError {
    Conflict,
    NotFound,
    BadInput(String),
    InvalidDefinition(String),
    Storage(String),
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::Conflict => f.write_str("Konflikt zapisu przebiegu."),
            RepoError::NotFound => f.write_str("Nie znaleziono przebiegu lub wersji procesu."),
            RepoError::BadInput(m) | RepoError::InvalidDefinition(m) | RepoError::Storage(m) => {
                f.write_str(m)
            }
        }
    }
}

impl std::error::Error for RepoError {}

/// Dane nowego przebiegu. `request_id` czyni start idempotentnym.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewRun {
    pub version_id: i64,
    pub request_id: String,
    pub task_id: Option<i64>,
    pub task_title: Option<String>,
    pub repo: Option<String>,
    pub workspace: Option<String>,
    pub base_commit: Option<String>,
    pub now: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatedRun {
    pub run_id: RunId,
    /// `false`, gdy ten `request_id` już istniał i zwrócono stary przebieg.
    pub created: bool,
}

/// Zdarzenie w kształcie przyszłego `workflow-event` (7.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredEvent {
    pub run_id: RunId,
    pub sequence: u64,
    pub execution_id: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub timestamp: i64,
    pub payload: Value,
}

impl StoredEvent {
    pub fn new(run_id: RunId, sequence: u64, timestamp: i64, event: &RunEvent) -> Self {
        Self {
            run_id,
            sequence,
            execution_id: event.execution_id().map(str::to_owned),
            kind: event.stream_type().to_owned(),
            timestamp,
            payload: serde_json::to_value(event).unwrap_or(Value::Null),
        }
    }

    pub fn to_run_event(&self) -> Result<RunEvent, RepoError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|e| RepoError::Storage(format!("Uszkodzone zdarzenie przebiegu: {e}")))
    }
}

/// Trwała historia przebiegów. Implementacja nie może trzymać blokady
/// dłużej niż jedna krótka transakcja.
pub trait RunRepository: Send + Sync {
    fn create_run(&self, new: &NewRun) -> Result<CreatedRun, RepoError>;
    fn graph(&self, run_id: RunId) -> Result<WorkflowGraph, RepoError>;
    /// Dopisuje zdarzenia jedną transakcją. `expected_next_sequence` musi
    /// równać się dotychczasowa liczba zdarzeń + 1, inaczej `Conflict`.
    fn append(
        &self,
        run_id: RunId,
        expected_next_sequence: u64,
        events: &[RunEvent],
        now: i64,
    ) -> Result<Vec<StoredEvent>, RepoError>;
    /// Zdarzenia o sekwencji większej niż `after_sequence`, rosnąco.
    fn load_events(
        &self,
        run_id: RunId,
        after_sequence: u64,
    ) -> Result<Vec<StoredEvent>, RepoError>;
    /// Ostatnia nadana sekwencja (liczba zdarzeń przebiegu).
    fn last_sequence(&self, run_id: RunId) -> Result<u64, RepoError>;
    /// Przebiegi, które nie są w stanie końcowym, rosnąco po id.
    fn active_runs(&self) -> Result<Vec<RunId>, RepoError>;
}

/// Magistrala zdarzeń: dostaje zdarzenie dopiero po udanym zapisie.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &StoredEvent);
    /// Błąd, którego nie udało się zapisać w historii przebiegu (np. baza odrzuca
    /// zapis): interfejs ma się dowiedzieć, że przebieg utknął.
    fn fault(&self, _run_id: RunId, _message: &str) {}
}

/// Dostęp do przestrzeni roboczej przebiegu (w E3 tylko skrót artefaktu).
pub trait WorkspaceAccess: Send + Sync {
    /// Skrót tego, co użytkownik akceptuje (np. diff); zmiana unieważnia zgodę.
    fn artifact_hash(&self, run_id: RunId) -> Result<String, String>;
    fn workspace_path(&self, _run_id: RunId) -> Option<String> {
        None
    }
}

/// Odwołanie do wejścia kroku; trzymane tu, by `ports` nie zależało od szczegółów silnika.
pub fn resolve_inputs(refs: &[InputRef], lookup: impl Fn(&str) -> Option<Value>) -> Vec<RunInput> {
    refs.iter()
        .filter_map(|r| {
            lookup(&r.execution_id).map(|output| RunInput {
                node_id: r.node_id.clone(),
                execution_id: r.execution_id.clone(),
                output,
            })
        })
        .collect()
}
