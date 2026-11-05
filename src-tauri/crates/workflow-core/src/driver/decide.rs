//! Decyzje użytkownika o zgodach: idempotencja po `request_id`, rewizja i skrót artefaktu.

use std::fmt;

use super::{DriverError, Worker};
use crate::engine::{Input, RunEvent};
use crate::ports::{RepoError, RunId};
use crate::run::{ApprovalStatus, Decision, RunStatus};

/// Decyzja użytkownika o zgodzie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecideRequest {
    pub run_id: RunId,
    pub approval_id: String,
    pub decision: Decision,
    /// Powtórzenie z tym samym id zwraca zapisany wynik. Id jest jednorazowe w całym
    /// przebiegu: użyte już przy innej zgodzie daje `Conflict` (tak jak UNIQUE w SQLite).
    pub request_id: String,
    /// Ostatnia sekwencja, jaką widział klient.
    pub expected_revision: u64,
    /// Skrót artefaktu, który zobaczył użytkownik.
    pub artifact_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecideResult {
    pub approval_status: ApprovalStatus,
    pub run_status: RunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecideError {
    NotFound,
    Conflict,
    /// Artefakt zmienił się: zgoda unieważniona, przebieg prosi o nową.
    Invalidated,
    InvalidState(String),
    Storage(String),
}

impl fmt::Display for DecideError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecideError::NotFound => f.write_str("Nie znaleziono zgody."),
            DecideError::Conflict => {
                f.write_str("Zgoda została już rozstrzygnięta albo przebieg się zmienił.")
            }
            DecideError::Invalidated => f.write_str(
                "Zmiany w katalogu roboczym unieważniły zgodę. Przejrzyj je jeszcze raz.",
            ),
            DecideError::InvalidState(m) | DecideError::Storage(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for DecideError {}

fn storage(e: RepoError) -> DecideError {
    match e {
        RepoError::NotFound => DecideError::NotFound,
        RepoError::Conflict => DecideError::Conflict,
        other => DecideError::Storage(other.to_string()),
    }
}

impl Worker {
    pub(super) fn decide_approval(
        &mut self,
        request: &DecideRequest,
    ) -> Result<DecideResult, DecideError> {
        let (_, state) = self.load(request.run_id).map_err(storage)?;
        let approval = state
            .approval(&request.approval_id)
            .ok_or(DecideError::NotFound)?;
        let reused_elsewhere = state.approvals.iter().any(|a| {
            a.id != request.approval_id && a.request_id.as_deref() == Some(&request.request_id)
        });
        if reused_elsewhere {
            return Err(DecideError::Conflict);
        }
        if approval.status != ApprovalStatus::Pending {
            // Powtórzone żądanie zwraca zapisany wynik; inna decyzja to konflikt.
            if approval.request_id.as_deref() != Some(request.request_id.as_str()) {
                return Err(DecideError::Conflict);
            }
            return match approval.status {
                ApprovalStatus::Invalidated => Err(DecideError::Invalidated),
                status => Ok(DecideResult {
                    approval_status: status,
                    run_status: state.status,
                }),
            };
        }
        if request.expected_revision != state.event_count {
            return Err(DecideError::Conflict);
        }
        let current_hash = self
            .workspace
            .artifact_hash(request.run_id)
            .map_err(DecideError::Storage)?;
        let committed = self
            .step_with(request.run_id, |_| {
                Some(Input::ApprovalDecision {
                    approval_id: request.approval_id.clone(),
                    decision: request.decision,
                    request_id: Some(request.request_id.clone()),
                    artifact_hash: request.artifact_hash.clone(),
                    current_hash: current_hash.clone(),
                })
            })
            .map_err(|e| match e {
                DriverError::Repo(e) => storage(e),
                DriverError::Engine(e) => DecideError::InvalidState(e.to_string()),
            })?
            .ok_or_else(|| DecideError::InvalidState("Brak zmiany stanu.".into()))?;
        let invalidated = committed.transition.events.iter().any(|e| {
            matches!(
                e,
                RunEvent::ApprovalResolved {
                    status: ApprovalStatus::Invalidated,
                    ..
                }
            )
        });
        if invalidated {
            return Err(DecideError::Invalidated);
        }
        self.enqueue(request.run_id);
        let approval_status = committed
            .state
            .approval(&request.approval_id)
            .map_or(ApprovalStatus::Pending, |a| a.status);
        Ok(DecideResult {
            approval_status,
            run_status: committed.state.status,
        })
    }
}
