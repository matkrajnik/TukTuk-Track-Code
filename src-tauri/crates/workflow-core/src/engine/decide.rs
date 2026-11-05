//! `decide`: wejście + stan → zdarzenia i skutki. Bez I/O i zegara.

use std::collections::HashSet;

use serde_json::Value;

use super::{Effect, EngineError, Input, InputRef, RunEvent, RunState, Transition};
use crate::model::{AgentConfig, ConditionConfig, EndOutcome, Node, NodeKind, Port, WorkflowGraph};
use crate::output::{evaluate, parse_output, ConditionError};
use crate::ports::RunnerOutcome;
use crate::run::{ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus};

/// Komunikat kroku, którego runner nie potwierdził końca po anulowaniu.
pub const UNRESPONSIVE_MESSAGE: &str =
    "Agent nie zakończył pracy po anulowaniu; koniec kroku nie jest potwierdzony.";

/// Rozstrzyga jedno wejście. Nie zmienia `state`: pracuje na kopii, stosując
/// własne zdarzenia tą samą funkcją `apply`, której używa odtwarzanie.
pub fn decide(
    graph: &WorkflowGraph,
    state: &RunState,
    input: Input,
) -> Result<Transition, EngineError> {
    let mut work = Work {
        graph,
        state: state.clone(),
        events: Vec::new(),
        effects: Vec::new(),
    };
    match input {
        Input::Start => work.start()?,
        Input::Continue => work.continue_run()?,
        Input::ExecutionResult {
            execution_id,
            outcome,
        } => work.execution_result(&execution_id, outcome)?,
        Input::CancelConfirmed { execution_id } => {
            work.execution_result(&execution_id, RunnerOutcome::Cancelled)?
        }
        Input::CancelRequested => work.cancel()?,
        Input::ApprovalPrepared {
            approval_id,
            artifact_hash,
        } => work.approval_prepared(&approval_id, &artifact_hash)?,
        Input::ApprovalDecision {
            approval_id,
            decision,
            request_id,
            artifact_hash,
            current_hash,
        } => work.approval_decision(
            &approval_id,
            decision,
            request_id,
            &artifact_hash,
            &current_hash,
        )?,
        Input::Recover => work.recover()?,
        Input::Unresponsive { execution_id } => work.unresponsive(&execution_id)?,
        Input::Fault { code, message } => work.fault(code, message)?,
    }
    Ok(Transition {
        events: work.events,
        effects: work.effects,
    })
}

struct Work<'a> {
    graph: &'a WorkflowGraph,
    state: RunState,
    events: Vec<RunEvent>,
    effects: Vec<Effect>,
}

fn invalid_state(message: &str) -> EngineError {
    EngineError::InvalidState(message.to_owned())
}

impl Work<'_> {
    fn emit(&mut self, event: RunEvent) -> Result<(), EngineError> {
        self.state.apply(self.graph, &event)?;
        self.events.push(event);
        Ok(())
    }

    fn set_status(&mut self, to: RunStatus, code: Option<FailureCode>) -> Result<(), EngineError> {
        let from = self.state.status;
        self.emit(RunEvent::RunStateChanged { from, to, code })
    }

    /// Zapisuje węzły, których przebieg nie odwiedził, i przechodzi do stanu końcowego.
    fn finish_run(&mut self, to: RunStatus, code: Option<FailureCode>) -> Result<(), EngineError> {
        let skipped: Vec<String> = self
            .graph
            .nodes
            .iter()
            .filter(|n| !self.state.visited.contains(&n.id))
            .map(|n| n.id.clone())
            .collect();
        if !skipped.is_empty() {
            self.emit(RunEvent::NodesSkipped { node_ids: skipped })?;
        }
        self.set_status(to, code)
    }

    fn fail_run(&mut self, code: FailureCode) -> Result<(), EngineError> {
        self.finish_run(RunStatus::Failed, Some(code))
    }

    fn finish_execution(
        &mut self,
        execution_id: &str,
        status: ExecutionStatus,
        code: Option<FailureCode>,
        message: Option<String>,
    ) -> Result<(), EngineError> {
        self.emit(RunEvent::ExecutionFinished {
            execution_id: execution_id.to_owned(),
            status,
            output: None,
            code,
            message,
            exit_code: None,
            session_id: None,
        })
    }

    fn start(&mut self) -> Result<(), EngineError> {
        if self.state.status != RunStatus::Queued {
            return Err(invalid_state("start możliwy tylko z kolejki"));
        }
        self.set_status(RunStatus::Running, None)
    }

    /// Ostatnie zakończone wyniki każdego węzła agenta, w kolejności wykonania.
    fn agent_inputs(&self) -> Vec<InputRef> {
        let mut seen = HashSet::new();
        let mut refs: Vec<InputRef> = self
            .state
            .executions
            .iter()
            .rev()
            .filter(|e| e.status == ExecutionStatus::Completed && e.output.is_some())
            .filter(|e| seen.insert(e.node_id.clone()))
            .map(|e| InputRef {
                node_id: e.node_id.clone(),
                execution_id: e.id.clone(),
            })
            .collect();
        refs.reverse();
        refs
    }

    fn plan(&mut self, node_id: &str, inputs: Vec<InputRef>) -> Result<String, EngineError> {
        let run_id = self.state.run_id;
        let id = format!("{run_id}-{}", self.state.executions.len() + 1);
        let iteration = self
            .state
            .executions
            .iter()
            .filter(|e| e.node_id == node_id)
            .count() as u32
            + 1;
        self.emit(RunEvent::ExecutionPlanned {
            execution_id: id.clone(),
            node_id: node_id.to_owned(),
            iteration,
            attempt: 1,
            inputs,
        })?;
        Ok(id)
    }

    fn target(&self, node_id: &str, port: Port) -> Result<String, EngineError> {
        self.graph
            .edges
            .iter()
            .find(|e| e.source == node_id && e.source_port == port)
            .map(|e| e.target.clone())
            .ok_or_else(|| {
                EngineError::InvalidDefinition(format!("węzeł {node_id} nie ma połączenia z portu"))
            })
    }

    /// Węzeł, od którego przebieg ma iść dalej (kursor wynika z ostatniego kroku).
    fn next_node(&self) -> Result<String, EngineError> {
        match self.state.executions.last() {
            None => {
                let trigger = self
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.kind == NodeKind::Trigger)
                    .ok_or_else(|| EngineError::InvalidDefinition("brak węzła Start".into()))?;
                self.target(&trigger.id, Port::Out)
            }
            Some(last) => {
                let port = last
                    .port
                    .ok_or_else(|| invalid_state("ostatni krok nie wskazuje następnego węzła"))?;
                self.target(&last.node_id, port)
            }
        }
    }

    /// Czy z portu da się wrócić do tego samego warunku (pętla poprawek).
    fn loops_back(&self, condition_id: &str, port: Port) -> bool {
        let Ok(start) = self.target(condition_id, port) else {
            return false;
        };
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(id) = stack.pop() {
            if id == condition_id {
                return true;
            }
            if seen.insert(id.clone()) {
                stack.extend(
                    self.graph
                        .edges
                        .iter()
                        .filter(|e| e.source == id)
                        .map(|e| e.target.clone()),
                );
            }
        }
        false
    }

    fn continue_run(&mut self) -> Result<(), EngineError> {
        if self.state.status != RunStatus::Running {
            return Err(invalid_state("kontynuacja możliwa tylko w stanie Running"));
        }
        if self.state.unfinished().is_some() {
            return Err(invalid_state("poprzedni krok nie ma jeszcze wyniku"));
        }
        let mut node_id = self.next_node()?;
        loop {
            let node = self
                .graph
                .node(&node_id)
                .ok_or_else(|| EngineError::InvalidDefinition(format!("brak węzła {node_id}")))?;
            match &node.kind {
                NodeKind::Trigger => {
                    return Err(EngineError::InvalidDefinition(
                        "połączenie prowadzi do węzła Start".into(),
                    ))
                }
                NodeKind::Agent(config) => return self.launch_agent(node, config),
                NodeKind::Approval(_) => {
                    let execution_id = self.plan(&node.id, Vec::new())?;
                    let approval_id = self.new_approval_id(&execution_id);
                    self.effects.push(Effect::RequestApproval {
                        approval_id,
                        execution_id,
                    });
                    return Ok(());
                }
                NodeKind::Condition(config) => match self.condition(node, config)? {
                    Some(next) => node_id = next,
                    None => return Ok(()),
                },
                NodeKind::End(config) => {
                    let execution_id = self.plan(&node.id, Vec::new())?;
                    self.finish_execution(&execution_id, ExecutionStatus::Completed, None, None)?;
                    let (to, code) = match config.outcome {
                        EndOutcome::Succeeded => (RunStatus::Succeeded, None),
                        EndOutcome::Rejected => (RunStatus::Rejected, None),
                        EndOutcome::Failed => (RunStatus::Failed, None),
                    };
                    return self.finish_run(to, code);
                }
            }
        }
    }

    fn launch_agent(&mut self, node: &Node, config: &AgentConfig) -> Result<(), EngineError> {
        let inputs = self.agent_inputs();
        let execution_id = self.plan(&node.id, inputs.clone())?;
        self.effects.push(Effect::LaunchAgent {
            execution_id,
            node_id: node.id.clone(),
            config: config.clone(),
            inputs,
        });
        Ok(())
    }

    fn new_approval_id(&self, execution_id: &str) -> String {
        format!("{execution_id}-a{}", self.state.approvals.len() + 1)
    }

    /// Wykonuje warunek od ręki. `None` oznacza, że przebieg zakończył się błędem.
    fn condition(
        &mut self,
        node: &Node,
        config: &ConditionConfig,
    ) -> Result<Option<String>, EngineError> {
        let execution_id = self.plan(&node.id, Vec::new())?;
        let read = self.state.last_completed(&config.field.node_id).cloned();
        let value: Option<Value> = read
            .as_ref()
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.field(&config.field.field));
        let result = match evaluate(config, value.as_ref()) {
            Ok(result) => result,
            Err(error) => {
                let code = match error {
                    ConditionError::MissingValue => FailureCode::InvalidOutput,
                    ConditionError::TypeMismatch => FailureCode::InvalidDefinition,
                };
                self.finish_execution(
                    &execution_id,
                    ExecutionStatus::Failed,
                    Some(code),
                    Some(error.to_string()),
                )?;
                self.fail_run(code)?;
                return Ok(None);
            }
        };
        let port = if result { Port::True } else { Port::False };
        let read_execution_id = read.map(|r| r.id).unwrap_or_default();
        self.emit(RunEvent::ConditionEvaluated {
            node_id: node.id.clone(),
            execution_id: execution_id.clone(),
            read_execution_id,
            result,
            port,
        })?;
        if let Some(limit) = config.repair_limit {
            if self.loops_back(&node.id, port) {
                let rounds = self.state.repair_rounds.get(&node.id).copied().unwrap_or(0);
                if rounds >= limit {
                    let code = FailureCode::ReviewLimitExceeded;
                    self.finish_execution(
                        &execution_id,
                        ExecutionStatus::Failed,
                        Some(code),
                        Some(code.message().to_owned()),
                    )?;
                    self.fail_run(code)?;
                    return Ok(None);
                }
                self.emit(RunEvent::RepairRoundCounted {
                    condition_id: node.id.clone(),
                    round: rounds + 1,
                })?;
            }
        }
        self.finish_execution(&execution_id, ExecutionStatus::Completed, None, None)?;
        self.target(&node.id, port).map(Some)
    }

    fn execution_result(
        &mut self,
        execution_id: &str,
        outcome: RunnerOutcome,
    ) -> Result<(), EngineError> {
        let record = self
            .state
            .execution(execution_id)
            .ok_or_else(|| EngineError::UnknownExecution(execution_id.to_owned()))?;
        if record.status != ExecutionStatus::Running {
            return Err(invalid_state("krok nie jest uruchomiony"));
        }
        if !matches!(
            self.state.status,
            RunStatus::Running | RunStatus::Cancelling
        ) {
            return Err(invalid_state("przebieg nie przyjmuje wyniku kroku"));
        }
        // Krok zgody może zakończyć się tylko błędem (np. brak skrótu artefaktu).
        let role = match self.graph.node(&record.node_id).map(|n| &n.kind) {
            Some(NodeKind::Agent(config)) => Some(config.role),
            Some(NodeKind::Approval(_)) if matches!(outcome, RunnerOutcome::Failed { .. }) => None,
            _ => return Err(invalid_state("wynik dotyczy kroku, który nie jest agentem")),
        };
        let (status, output, code, message, exit_code, session_id) = match outcome {
            RunnerOutcome::Completed {
                output,
                exit_code,
                session_id,
            } => {
                if exit_code != 0 {
                    let code = FailureCode::ProtocolError;
                    let message = format!("{} Kod wyjścia: {exit_code}.", code.message());
                    (
                        ExecutionStatus::Failed,
                        None,
                        Some(code),
                        Some(message),
                        Some(exit_code),
                        session_id,
                    )
                } else {
                    let Some(role) = role else {
                        return Err(invalid_state("wynik dotyczy kroku, który nie jest agentem"));
                    };
                    match parse_output(role, &output) {
                        Ok(parsed) => (
                            ExecutionStatus::Completed,
                            Some(parsed.to_value()),
                            None,
                            None,
                            Some(exit_code),
                            session_id,
                        ),
                        Err(error) => (
                            ExecutionStatus::Failed,
                            None,
                            Some(FailureCode::InvalidOutput),
                            Some(error.to_string()),
                            Some(exit_code),
                            session_id,
                        ),
                    }
                }
            }
            RunnerOutcome::Failed { code, message } => (
                ExecutionStatus::Failed,
                None,
                Some(code),
                Some(message),
                None,
                None,
            ),
            RunnerOutcome::Cancelled => (
                ExecutionStatus::Cancelled,
                None,
                Some(FailureCode::Cancelled),
                None,
                None,
                None,
            ),
        };
        self.emit(RunEvent::ExecutionFinished {
            execution_id: execution_id.to_owned(),
            status,
            output,
            code,
            message,
            exit_code,
            session_id,
        })?;
        if self.state.status == RunStatus::Cancelling {
            // Wynik po anulowaniu jest zapisany, ale nie uruchamia następnego kroku.
            return self.finish_run(RunStatus::Cancelled, None);
        }
        if status != ExecutionStatus::Completed {
            return self.fail_run(code.unwrap_or(FailureCode::ProtocolError));
        }
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), EngineError> {
        match self.state.status {
            RunStatus::Queued => {
                self.emit(RunEvent::CancelRequested)?;
                self.finish_run(RunStatus::Cancelled, None)
            }
            RunStatus::Running => {
                self.emit(RunEvent::CancelRequested)?;
                self.set_status(RunStatus::Cancelling, None)?;
                match self.state.unfinished().map(|e| e.id.clone()) {
                    Some(execution_id) => {
                        self.effects.push(Effect::CancelExecution { execution_id });
                        Ok(())
                    }
                    None => self.finish_run(RunStatus::Cancelled, None),
                }
            }
            RunStatus::WaitingForApproval => {
                self.emit(RunEvent::CancelRequested)?;
                self.set_status(RunStatus::Cancelling, None)?;
                let pending: Vec<String> = self
                    .state
                    .approvals
                    .iter()
                    .filter(|a| a.status == ApprovalStatus::Pending)
                    .map(|a| a.id.clone())
                    .collect();
                for approval_id in pending {
                    self.emit(RunEvent::ApprovalResolved {
                        approval_id,
                        decision: None,
                        status: ApprovalStatus::Invalidated,
                        request_id: None,
                    })?;
                }
                if let Some(id) = self.state.unfinished().map(|e| e.id.clone()) {
                    self.finish_execution(
                        &id,
                        ExecutionStatus::Cancelled,
                        Some(FailureCode::Cancelled),
                        None,
                    )?;
                }
                self.finish_run(RunStatus::Cancelled, None)
            }
            // Powtórne anulowanie i anulowanie zakończonego przebiegu nic nie zmieniają.
            RunStatus::Cancelling
            | RunStatus::Cancelled
            | RunStatus::Succeeded
            | RunStatus::Rejected
            | RunStatus::Failed
            | RunStatus::Interrupted => Ok(()),
        }
    }

    fn approval_prepared(
        &mut self,
        approval_id: &str,
        artifact_hash: &str,
    ) -> Result<(), EngineError> {
        if self.state.status != RunStatus::Running {
            return Err(invalid_state("zgodę przygotowuje się w stanie Running"));
        }
        let execution = self
            .state
            .unfinished()
            .filter(|e| e.status == ExecutionStatus::Running)
            .ok_or_else(|| invalid_state("brak kroku zgody do przygotowania"))?;
        let is_approval = matches!(
            self.graph.node(&execution.node_id).map(|n| &n.kind),
            Some(NodeKind::Approval(_))
        );
        if !is_approval || self.state.approval(approval_id).is_some() {
            return Err(invalid_state("krok nie czeka na przygotowanie zgody"));
        }
        let execution_id = execution.id.clone();
        self.emit(RunEvent::ApprovalRequested {
            approval_id: approval_id.to_owned(),
            execution_id,
            artifact_hash: artifact_hash.to_owned(),
        })?;
        self.set_status(RunStatus::WaitingForApproval, None)
    }

    fn approval_decision(
        &mut self,
        approval_id: &str,
        decision: Decision,
        request_id: Option<String>,
        artifact_hash: &str,
        current_hash: &str,
    ) -> Result<(), EngineError> {
        if self.state.status != RunStatus::WaitingForApproval {
            return Err(invalid_state("przebieg nie czeka na zgodę"));
        }
        let approval = self
            .state
            .approval(approval_id)
            .ok_or_else(|| EngineError::UnknownApproval(approval_id.to_owned()))?;
        if approval.status != ApprovalStatus::Pending {
            return Err(invalid_state("zgoda jest już rozstrzygnięta"));
        }
        let execution_id = approval.execution_id.clone();
        if artifact_hash != approval.artifact_hash || current_hash != approval.artifact_hash {
            // Artefakt zmienił się albo użytkownik widział inny: zgoda traci ważność,
            // a przebieg prosi o nową dla aktualnego artefaktu.
            self.emit(RunEvent::ApprovalResolved {
                approval_id: approval_id.to_owned(),
                decision: Some(decision),
                status: ApprovalStatus::Invalidated,
                request_id,
            })?;
            let new_id = self.new_approval_id(&execution_id);
            return self.emit(RunEvent::ApprovalRequested {
                approval_id: new_id,
                execution_id,
                artifact_hash: current_hash.to_owned(),
            });
        }
        let (status, execution_status) = match decision {
            Decision::Approve => (ApprovalStatus::Approved, ExecutionStatus::Completed),
            Decision::Reject => (ApprovalStatus::Rejected, ExecutionStatus::Rejected),
        };
        self.emit(RunEvent::ApprovalResolved {
            approval_id: approval_id.to_owned(),
            decision: Some(decision),
            status,
            request_id,
        })?;
        self.finish_execution(&execution_id, execution_status, None, None)?;
        self.set_status(RunStatus::Running, None)
    }

    fn recover(&mut self) -> Result<(), EngineError> {
        if !matches!(
            self.state.status,
            RunStatus::Running | RunStatus::Cancelling
        ) {
            return Ok(());
        }
        let Some(id) = self.state.unfinished().map(|e| e.id.clone()) else {
            return Ok(());
        };
        self.interrupt(&id, FailureCode::Interrupted.message())
    }

    fn unresponsive(&mut self, execution_id: &str) -> Result<(), EngineError> {
        let record = self
            .state
            .execution(execution_id)
            .ok_or_else(|| EngineError::UnknownExecution(execution_id.to_owned()))?;
        if record.status != ExecutionStatus::Running
            || !matches!(
                self.state.status,
                RunStatus::Running | RunStatus::Cancelling
            )
        {
            return Err(invalid_state("krok nie jest uruchomiony"));
        }
        self.interrupt(execution_id, UNRESPONSIVE_MESSAGE)
    }

    fn fault(&mut self, code: FailureCode, message: String) -> Result<(), EngineError> {
        if !matches!(
            self.state.status,
            RunStatus::Running | RunStatus::Cancelling
        ) {
            return Err(invalid_state("przebieg nie jest w toku"));
        }
        if let Some(id) = self.state.unfinished().map(|e| e.id.clone()) {
            self.finish_execution(&id, ExecutionStatus::Failed, Some(code), Some(message))?;
        }
        self.fail_run(code)
    }

    fn interrupt(&mut self, execution_id: &str, message: &str) -> Result<(), EngineError> {
        let code = FailureCode::Interrupted;
        self.finish_execution(
            execution_id,
            ExecutionStatus::Interrupted,
            Some(code),
            Some(message.to_owned()),
        )?;
        self.finish_run(RunStatus::Interrupted, Some(code))
    }
}
