//! Czysty silnik przebiegu (PLAN_WDROZENIA.md, 5 i 6): bez I/O i bez zegara.
//!
//! `decide(graph, stan, wejście)` zwraca zdarzenia do zapisania i skutki dla
//! sterownika. Stan odtwarza się wyłącznie przez `RunState::replay`, więc
//! historia zdarzeń jest jedynym źródłem prawdy. Przebieg idzie jedną ścieżką
//! (kolejka gotowych węzłów bez równoległości); pozycja na canvasie nie ma znaczenia.

mod decide;
mod state;

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{AgentConfig, Port};
use crate::ports::{RunId, RunnerOutcome};
use crate::run::{ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus};

pub use decide::{decide, normalize_comment, UNRESPONSIVE_MESSAGE};
pub use state::{ApprovalRecord, ExecutionRecord, ReplayError, RunState, SequencedEvent};

/// Odwołanie do wyniku wcześniejszego kroku użytego jako wejście.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputRef {
    pub node_id: String,
    pub execution_id: String,
}

/// Zdarzenie przebiegu: trwałe źródło prawdy. Numer sekwencji nadaje repozytorium.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RunEvent {
    RunCreated {
        run_id: RunId,
        graph_hash: String,
    },
    RunStateChanged {
        from: RunStatus,
        to: RunStatus,
        code: Option<FailureCode>,
    },
    /// Zamiar uruchomienia kroku, zapisywany przed startem runnera (5.4).
    ExecutionPlanned {
        execution_id: String,
        node_id: String,
        iteration: u32,
        attempt: u32,
        inputs: Vec<InputRef>,
    },
    ExecutionFinished {
        execution_id: String,
        status: ExecutionStatus,
        output: Option<Value>,
        code: Option<FailureCode>,
        message: Option<String>,
        exit_code: Option<i32>,
        session_id: Option<String>,
    },
    ConditionEvaluated {
        node_id: String,
        execution_id: String,
        read_execution_id: String,
        result: bool,
        port: Port,
    },
    RepairRoundCounted {
        condition_id: String,
        round: u32,
    },
    ApprovalRequested {
        approval_id: String,
        execution_id: String,
        artifact_hash: String,
    },
    ApprovalResolved {
        approval_id: String,
        decision: Option<Decision>,
        status: ApprovalStatus,
        request_id: Option<String>,
        /// Komentarz użytkownika (wymagany przy odrzuceniu po stronie hosta).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        comment: Option<String>,
    },
    NodesSkipped {
        node_ids: Vec<String>,
    },
    CancelRequested,
    /// Komunikat runnera: informacyjny, pomijany przy odtwarzaniu stanu.
    Message {
        execution_id: String,
        payload: Value,
    },
    /// Narzędzie runnera (`payload.phase`: `started` albo `finished`).
    Tool {
        execution_id: String,
        payload: Value,
    },
}

impl RunEvent {
    pub fn execution_id(&self) -> Option<&str> {
        match self {
            RunEvent::ExecutionPlanned { execution_id, .. }
            | RunEvent::ExecutionFinished { execution_id, .. }
            | RunEvent::ConditionEvaluated { execution_id, .. }
            | RunEvent::ApprovalRequested { execution_id, .. }
            | RunEvent::Message { execution_id, .. }
            | RunEvent::Tool { execution_id, .. } => Some(execution_id),
            RunEvent::RunCreated { .. }
            | RunEvent::RunStateChanged { .. }
            | RunEvent::RepairRoundCounted { .. }
            | RunEvent::ApprovalResolved { .. }
            | RunEvent::NodesSkipped { .. }
            | RunEvent::CancelRequested => None,
        }
    }

    /// Typ w strumieniu `workflow-event` (7.3).
    pub fn stream_type(&self) -> &'static str {
        match self {
            RunEvent::RunCreated { .. }
            | RunEvent::RunStateChanged { .. }
            | RunEvent::RepairRoundCounted { .. }
            | RunEvent::CancelRequested => "runStateChanged",
            RunEvent::ExecutionPlanned { .. }
            | RunEvent::ConditionEvaluated { .. }
            | RunEvent::NodesSkipped { .. } => "nodeStateChanged",
            RunEvent::ExecutionFinished { status, .. } => match status {
                ExecutionStatus::Failed => "error",
                _ => "nodeStateChanged",
            },
            RunEvent::ApprovalRequested { .. } => "approvalRequested",
            RunEvent::ApprovalResolved { .. } => "approvalResolved",
            RunEvent::Message { .. } => "message",
            RunEvent::Tool { payload, .. } => {
                if payload.get("phase").and_then(Value::as_str) == Some("finished") {
                    "toolFinished"
                } else {
                    "toolStarted"
                }
            }
        }
    }
}

/// Zdarzenia wejściowe silnika.
#[derive(Debug, Clone)]
pub enum Input {
    /// `Queued` → `Running`.
    Start,
    /// Granica kroków: planuje następny węzeł od kursora.
    Continue,
    ExecutionResult {
        execution_id: String,
        outcome: RunnerOutcome,
    },
    CancelRequested,
    CancelConfirmed {
        execution_id: String,
    },
    /// Skrót artefaktu dla zgody pobrany przez sterownik.
    ApprovalPrepared {
        approval_id: String,
        artifact_hash: String,
    },
    ApprovalDecision {
        approval_id: String,
        decision: Decision,
        request_id: Option<String>,
        comment: Option<String>,
        /// Skrót, który zobaczył użytkownik.
        artifact_hash: String,
        /// Skrót artefaktu w chwili decyzji.
        current_hash: String,
    },
    /// Restart: krok bez wyniku oznacza przerwany przebieg (bez ponownego startu).
    Recover,
    /// Runner nie zakończył się po anulowaniu i czasie łaski: koniec kroku jest
    /// niepotwierdzony, więc przebieg kończy się `Interrupted`, nie `Cancelled` ani `Failed`
    /// (PLAN_WDROZENIA.md, 8.3).
    Unresponsive {
        execution_id: String,
    },
    /// Trwały błąd sterownika (zapis, silnik): aktywny przebieg kończy się `Failed`
    /// z podanym kodem, zamiast wisieć w `Running` bez wyniku kroku.
    Fault {
        code: FailureCode,
        message: String,
    },
}

/// Skutki dla sterownika; silnik sam niczego nie uruchamia.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    LaunchAgent {
        execution_id: String,
        node_id: String,
        config: AgentConfig,
        inputs: Vec<InputRef>,
    },
    CancelExecution {
        execution_id: String,
    },
    RequestApproval {
        approval_id: String,
        execution_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transition {
    pub events: Vec<RunEvent>,
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    InvalidState(String),
    UnknownExecution(String),
    UnknownApproval(String),
    InvalidDefinition(String),
    Replay(ReplayError),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::InvalidState(m) => write!(f, "Niedozwolone w tym stanie przebiegu: {m}"),
            EngineError::UnknownExecution(id) => write!(f, "Nieznany krok przebiegu: {id}"),
            EngineError::UnknownApproval(id) => write!(f, "Nieznana zgoda: {id}"),
            EngineError::InvalidDefinition(m) => write!(f, "Niepoprawna definicja procesu: {m}"),
            EngineError::Replay(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<ReplayError> for EngineError {
    fn from(e: ReplayError) -> Self {
        EngineError::Replay(e)
    }
}
