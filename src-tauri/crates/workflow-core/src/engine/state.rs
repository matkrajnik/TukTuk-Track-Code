//! Stan przebiegu odtwarzany ze zdarzeń.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::{InputRef, RunEvent};
use crate::model::{NodeKind, Port, WorkflowGraph};
use crate::output::{parse_output, AgentOutput};
use crate::ports::RunId;
use crate::run::{ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionRecord {
    pub id: String,
    pub node_id: String,
    pub iteration: u32,
    pub attempt: u32,
    pub status: ExecutionStatus,
    pub inputs: Vec<InputRef>,
    pub output: Option<AgentOutput>,
    /// Port, którym przebieg opuszcza węzeł po tym kroku.
    pub port: Option<Port>,
    pub code: Option<FailureCode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApprovalRecord {
    pub id: String,
    pub execution_id: String,
    pub artifact_hash: String,
    pub status: ApprovalStatus,
    pub decision: Option<Decision>,
    pub request_id: Option<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunState {
    pub run_id: RunId,
    pub graph_hash: String,
    pub status: RunStatus,
    /// Liczba zastosowanych zdarzeń; sekwencje są gęste od 1, więc to też ostatnia sekwencja.
    pub event_count: u64,
    pub executions: Vec<ExecutionRecord>,
    pub approvals: Vec<ApprovalRecord>,
    pub repair_rounds: BTreeMap<String, u32>,
    pub visited: BTreeSet<String>,
    pub skipped: Vec<String>,
    pub failure: Option<FailureCode>,
    pub cancel_requested: bool,
    /// Identyfikatory żądań wznowienia (`ResumeRequested`), w kolejności.
    pub resume_requests: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    SequenceGap { expected: u64, found: u64 },
    MissingRunCreated,
    ForbiddenTransition { from: RunStatus, to: RunStatus },
    Contradiction(String),
}

impl fmt::Display for ReplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReplayError::SequenceGap { expected, found } => {
                write!(
                    f,
                    "Luka w historii przebiegu: oczekiwano {expected}, jest {found}."
                )
            }
            ReplayError::MissingRunCreated => {
                f.write_str("Historia przebiegu nie zaczyna się od utworzenia.")
            }
            ReplayError::ForbiddenTransition { from, to } => {
                write!(
                    f,
                    "Zabronione przejście stanu przebiegu: {from:?} -> {to:?}."
                )
            }
            ReplayError::Contradiction(m) => write!(f, "Sprzeczna historia przebiegu: {m}"),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Zdarzenie z numerem sekwencji nadanym przez repozytorium.
#[derive(Debug, Clone, PartialEq)]
pub struct SequencedEvent {
    pub sequence: u64,
    pub event: RunEvent,
}

impl RunState {
    fn empty() -> Self {
        Self {
            run_id: 0,
            graph_hash: String::new(),
            status: RunStatus::Queued,
            event_count: 0,
            executions: Vec::new(),
            approvals: Vec::new(),
            repair_rounds: BTreeMap::new(),
            visited: BTreeSet::new(),
            skipped: Vec::new(),
            failure: None,
            cancel_requested: false,
            resume_requests: Vec::new(),
        }
    }

    /// Odtwarza stan przez złożenie zdarzeń; odrzuca lukę w sekwencji i
    /// przejście zabronione przez `RunStatus::can_transition_to`.
    pub fn replay(graph: &WorkflowGraph, events: &[SequencedEvent]) -> Result<Self, ReplayError> {
        let mut state = Self::empty();
        for stored in events {
            let expected = state.event_count + 1;
            if stored.sequence != expected {
                return Err(ReplayError::SequenceGap {
                    expected,
                    found: stored.sequence,
                });
            }
            state.apply(graph, &stored.event)?;
        }
        Ok(state)
    }

    pub fn next_sequence(&self) -> u64 {
        self.event_count + 1
    }

    pub fn execution(&self, id: &str) -> Option<&ExecutionRecord> {
        self.executions.iter().find(|e| e.id == id)
    }

    pub fn approval(&self, id: &str) -> Option<&ApprovalRecord> {
        self.approvals.iter().find(|a| a.id == id)
    }

    /// Krok bez wyniku (uruchomiony albo czekający na zgodę).
    pub fn unfinished(&self) -> Option<&ExecutionRecord> {
        self.executions
            .iter()
            .rev()
            .find(|e| !e.status.is_terminal())
    }

    /// Ostatni zakończony krok węzła z wynikiem agenta.
    pub fn last_completed(&self, node_id: &str) -> Option<&ExecutionRecord> {
        self.executions
            .iter()
            .rev()
            .find(|e| e.node_id == node_id && e.status == ExecutionStatus::Completed)
    }

    fn execution_mut(&mut self, id: &str) -> Result<&mut ExecutionRecord, ReplayError> {
        self.executions
            .iter_mut()
            .find(|e| e.id == id)
            .ok_or_else(|| ReplayError::Contradiction(format!("brak kroku {id}")))
    }

    /// Stosuje zdarzenie. To jedyne miejsce zmieniające stan.
    pub fn apply(&mut self, graph: &WorkflowGraph, event: &RunEvent) -> Result<(), ReplayError> {
        if self.event_count == 0 && !matches!(event, RunEvent::RunCreated { .. }) {
            return Err(ReplayError::MissingRunCreated);
        }
        match event {
            RunEvent::RunCreated { run_id, graph_hash } => {
                if self.event_count != 0 {
                    return Err(ReplayError::Contradiction("podwójne utworzenie".into()));
                }
                self.run_id = *run_id;
                self.graph_hash = graph_hash.clone();
                if let Some(trigger) = graph.nodes.iter().find(|n| n.kind == NodeKind::Trigger) {
                    self.visited.insert(trigger.id.clone());
                }
            }
            RunEvent::RunStateChanged { from, to, code, .. } => {
                if *from != self.status {
                    return Err(ReplayError::Contradiction(format!(
                        "przejście z {from:?}, a stan to {:?}",
                        self.status
                    )));
                }
                if !from.can_transition_to(*to) {
                    return Err(ReplayError::ForbiddenTransition {
                        from: *from,
                        to: *to,
                    });
                }
                self.status = *to;
                if code.is_some() {
                    self.failure = *code;
                }
                // Idempotencja wznowienia dotyczy jednej pauzy: nowa pauza zaczyna od zera,
                // więc to samo id po kolejnym restarcie faktycznie wznawia.
                if *to == RunStatus::Paused {
                    self.resume_requests.clear();
                }
            }
            RunEvent::ExecutionPlanned {
                execution_id,
                node_id,
                iteration,
                attempt,
                inputs,
            } => {
                if self.execution(execution_id).is_some() {
                    return Err(ReplayError::Contradiction(format!(
                        "krok {execution_id} zaplanowany dwa razy"
                    )));
                }
                self.visited.insert(node_id.clone());
                self.executions.push(ExecutionRecord {
                    id: execution_id.clone(),
                    node_id: node_id.clone(),
                    iteration: *iteration,
                    attempt: *attempt,
                    status: ExecutionStatus::Running,
                    inputs: inputs.clone(),
                    output: None,
                    port: None,
                    code: None,
                });
            }
            RunEvent::ExecutionFinished {
                execution_id,
                status,
                output,
                code,
                ..
            } => {
                let role = self.agent_role(graph, execution_id);
                let record = self.execution_mut(execution_id)?;
                if !record.status.can_transition_to(*status) {
                    return Err(ReplayError::Contradiction(format!(
                        "krok {execution_id}: {:?} -> {status:?}",
                        record.status
                    )));
                }
                record.status = *status;
                record.code = *code;
                if let (Some(value), Some(role)) = (output, role) {
                    record.output = Some(parse_output(role, value).map_err(|e| {
                        ReplayError::Contradiction(format!("wynik kroku {execution_id}: {e}"))
                    })?);
                }
                if *status == ExecutionStatus::Completed && role.is_some() {
                    record.port = Some(Port::Out);
                }
            }
            RunEvent::ConditionEvaluated {
                execution_id, port, ..
            } => {
                self.execution_mut(execution_id)?.port = Some(*port);
            }
            RunEvent::RepairRoundCounted {
                condition_id,
                round,
            } => {
                let current = self.repair_rounds.get(condition_id).copied().unwrap_or(0);
                if *round != current + 1 {
                    return Err(ReplayError::Contradiction(format!(
                        "runda {round} warunku {condition_id} po rundzie {current}"
                    )));
                }
                self.repair_rounds.insert(condition_id.clone(), *round);
            }
            RunEvent::ApprovalRequested {
                approval_id,
                execution_id,
                artifact_hash,
            } => {
                let record = self.execution_mut(execution_id)?;
                if record.status == ExecutionStatus::Running {
                    record.status = ExecutionStatus::Waiting;
                }
                self.approvals.push(ApprovalRecord {
                    id: approval_id.clone(),
                    execution_id: execution_id.clone(),
                    artifact_hash: artifact_hash.clone(),
                    status: ApprovalStatus::Pending,
                    decision: None,
                    request_id: None,
                    comment: None,
                });
            }
            RunEvent::ApprovalResolved {
                approval_id,
                decision,
                status,
                request_id,
                comment,
            } => {
                let approval = self
                    .approvals
                    .iter_mut()
                    .find(|a| a.id == *approval_id)
                    .ok_or_else(|| {
                        ReplayError::Contradiction(format!("brak zgody {approval_id}"))
                    })?;
                if approval.status != ApprovalStatus::Pending {
                    return Err(ReplayError::Contradiction(format!(
                        "zgoda {approval_id} rozstrzygnięta dwa razy"
                    )));
                }
                approval.status = *status;
                approval.decision = *decision;
                approval.request_id = request_id.clone();
                approval.comment = comment.clone();
                let execution_id = approval.execution_id.clone();
                let port = match (status, decision) {
                    (ApprovalStatus::Approved, _) => Some(Port::Approve),
                    (ApprovalStatus::Rejected, _) => Some(Port::Reject),
                    (ApprovalStatus::Pending | ApprovalStatus::Invalidated, _) => None,
                };
                if let Some(port) = port {
                    self.execution_mut(&execution_id)?.port = Some(port);
                }
            }
            RunEvent::NodesSkipped { node_ids } => self.skipped = node_ids.clone(),
            RunEvent::CancelRequested => self.cancel_requested = true,
            RunEvent::ResumeRequested { request_id } => {
                if self.status != RunStatus::Paused {
                    return Err(ReplayError::Contradiction(
                        "wznowienie dotyczy tylko wstrzymanego przebiegu".into(),
                    ));
                }
                self.resume_requests.push(request_id.clone());
            }
            RunEvent::Reconciled { .. } => {
                if self.status != RunStatus::Interrupted {
                    return Err(ReplayError::Contradiction(
                        "odzyskanie dotyczy tylko przerwanego przebiegu".into(),
                    ));
                }
            }
            RunEvent::Message { .. } | RunEvent::Tool { .. } => {}
        }
        self.event_count += 1;
        Ok(())
    }

    fn agent_role(&self, graph: &WorkflowGraph, execution_id: &str) -> Option<crate::AgentRole> {
        let record = self.execution(execution_id)?;
        match &graph.node(&record.node_id)?.kind {
            NodeKind::Agent(config) => Some(config.role),
            NodeKind::Trigger
            | NodeKind::Condition(_)
            | NodeKind::Approval(_)
            | NodeKind::End(_) => None,
        }
    }
}
