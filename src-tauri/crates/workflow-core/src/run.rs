//! Stany przebiegu, kroków i zgód oraz kody błędów (PLAN_WDROZENIA.md, 5.2 i 5.3).
//!
//! Tablice przejść to wyczerpujące `match` bez gałęzi `_`: dodanie stanu nie
//! skompiluje się, dopóki ktoś nie zdecyduje, dokąd z niego wolno przejść.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Queued,
    Running,
    WaitingForApproval,
    Cancelling,
    Cancelled,
    Succeeded,
    Rejected,
    Failed,
    Interrupted,
}

impl RunStatus {
    pub const ALL: [RunStatus; 9] = [
        RunStatus::Queued,
        RunStatus::Running,
        RunStatus::WaitingForApproval,
        RunStatus::Cancelling,
        RunStatus::Cancelled,
        RunStatus::Succeeded,
        RunStatus::Rejected,
        RunStatus::Failed,
        RunStatus::Interrupted,
    ];

    /// Stan końcowy: przebieg nie ma już wyjść. `Interrupted` jest końcowy w E3;
    /// wznawianie przerwanych przebiegów dochodzi w E6.
    pub fn is_terminal(self) -> bool {
        match self {
            RunStatus::Queued
            | RunStatus::Running
            | RunStatus::WaitingForApproval
            | RunStatus::Cancelling => false,
            RunStatus::Cancelled
            | RunStatus::Succeeded
            | RunStatus::Rejected
            | RunStatus::Failed
            | RunStatus::Interrupted => true,
        }
    }

    pub fn can_transition_to(self, next: RunStatus) -> bool {
        use RunStatus::*;
        match self {
            Queued => matches!(next, Running | Cancelled),
            Running => matches!(
                next,
                WaitingForApproval | Cancelling | Succeeded | Rejected | Failed | Interrupted
            ),
            WaitingForApproval => matches!(next, Running | Cancelling | Rejected | Interrupted),
            Cancelling => matches!(next, Cancelled | Failed | Interrupted),
            Cancelled | Succeeded | Rejected | Failed | Interrupted => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionStatus {
    Pending,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
    Rejected,
    Skipped,
}

impl ExecutionStatus {
    pub const ALL: [ExecutionStatus; 9] = [
        ExecutionStatus::Pending,
        ExecutionStatus::Running,
        ExecutionStatus::Waiting,
        ExecutionStatus::Completed,
        ExecutionStatus::Failed,
        ExecutionStatus::Cancelled,
        ExecutionStatus::Interrupted,
        ExecutionStatus::Rejected,
        ExecutionStatus::Skipped,
    ];

    pub fn is_terminal(self) -> bool {
        match self {
            ExecutionStatus::Pending | ExecutionStatus::Running | ExecutionStatus::Waiting => false,
            ExecutionStatus::Completed
            | ExecutionStatus::Failed
            | ExecutionStatus::Cancelled
            | ExecutionStatus::Interrupted
            | ExecutionStatus::Rejected
            | ExecutionStatus::Skipped => true,
        }
    }

    pub fn can_transition_to(self, next: ExecutionStatus) -> bool {
        use ExecutionStatus::*;
        match self {
            Pending => matches!(next, Running | Cancelled | Skipped),
            Running => matches!(next, Waiting | Completed | Failed | Cancelled | Interrupted),
            Waiting => matches!(
                next,
                Running | Completed | Failed | Cancelled | Interrupted | Rejected
            ),
            Completed | Failed | Cancelled | Interrupted | Rejected | Skipped => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
    Invalidated,
}

/// Decyzja użytkownika w węźle Akceptacja.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    Approve,
    Reject,
}

/// Kody błędów z 5.3; te same nazwy trafiają do bazy i do IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureCode {
    CliMissing,
    NotLoggedIn,
    UnsupportedVersion,
    SpawnFailed,
    ProtocolError,
    InvalidOutput,
    Timeout,
    Cancelled,
    PermissionViolation,
    StorageError,
    ReviewLimitExceeded,
    Interrupted,
    InvalidDefinition,
}

impl FailureCode {
    pub fn message(self) -> &'static str {
        match self {
            FailureCode::CliMissing => "Nie znaleziono programu agenta (claude lub codex).",
            FailureCode::NotLoggedIn => "Agent nie jest zalogowany.",
            FailureCode::UnsupportedVersion => "Nieobsługiwana wersja programu agenta.",
            FailureCode::SpawnFailed => "Nie udało się uruchomić programu agenta.",
            FailureCode::ProtocolError => "Agent zwrócił niezgodną z protokołem odpowiedź.",
            FailureCode::InvalidOutput => "Wynik agenta nie pasuje do schematu roli.",
            FailureCode::Timeout => "Krok agenta przekroczył limit czasu.",
            FailureCode::Cancelled => "Krok został anulowany.",
            FailureCode::PermissionViolation => "Agent naruszył przydzielone uprawnienia.",
            FailureCode::StorageError => "Nie udało się zapisać stanu przebiegu.",
            FailureCode::ReviewLimitExceeded => {
                "Wyczerpano limit rund poprawek, a review nadal wskazuje problemy."
            }
            FailureCode::Interrupted => {
                "Przebieg został przerwany (zamknięcie aplikacji albo agent bez reakcji na \
                 anulowanie); koniec kroku nie jest potwierdzony."
            }
            FailureCode::InvalidDefinition => "Definicja procesu jest niepoprawna.",
        }
    }
}

/// Czy stan przebiegu nie przeczy stanom kroków: aktywny krok może istnieć
/// tylko w stanach `Running` i `Cancelling` (i najwyżej jeden), a krok
/// oczekujący na zgodę tylko przy `WaitingForApproval`, `Running` lub `Cancelling`.
pub fn consistent(run: RunStatus, executions: &[ExecutionStatus]) -> bool {
    let running = executions
        .iter()
        .filter(|s| **s == ExecutionStatus::Running)
        .count();
    let waiting = executions
        .iter()
        .filter(|s| **s == ExecutionStatus::Waiting)
        .count();
    let running_ok = match run {
        RunStatus::Running | RunStatus::Cancelling => running <= 1,
        RunStatus::Queued
        | RunStatus::WaitingForApproval
        | RunStatus::Cancelled
        | RunStatus::Succeeded
        | RunStatus::Rejected
        | RunStatus::Failed
        | RunStatus::Interrupted => running == 0,
    };
    let waiting_ok = match run {
        RunStatus::WaitingForApproval => waiting == 1,
        RunStatus::Cancelling | RunStatus::Running => waiting <= 1,
        RunStatus::Queued
        | RunStatus::Cancelled
        | RunStatus::Succeeded
        | RunStatus::Rejected
        | RunStatus::Failed
        | RunStatus::Interrupted => waiting == 0,
    };
    let queued_ok = run != RunStatus::Queued || executions.is_empty();
    running_ok && waiting_ok && queued_ok
}
