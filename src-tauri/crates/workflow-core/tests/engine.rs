//! Czysty silnik na szablonie „Analiza → implementacja → review” i małych grafach.

use std::collections::{HashMap, VecDeque};

use serde_json::{json, Value};
use workflow_core::engine::{
    decide, Effect, EngineError, Input, ReplayError, RunEvent, RunState, SequencedEvent,
};
use workflow_core::ports::RunnerOutcome;
use workflow_core::samples;
use workflow_core::template::analysis_implementation_review;
use workflow_core::{
    ApprovalStatus, CompareOp, ConditionConfig, Decision, Edge, ExecutionStatus, FailureCode,
    FieldRef, Node, NodeKind, Port, Position, RunStatus, WorkflowGraph, SCHEMA_VERSION,
};

const HASH: &str = "hash-1";

struct Sim {
    graph: WorkflowGraph,
    state: RunState,
    log: Vec<RunEvent>,
}

impl Sim {
    fn new(graph: WorkflowGraph) -> Self {
        let created = RunEvent::RunCreated {
            run_id: 1,
            graph_hash: "g".into(),
        };
        let state = RunState::replay(
            &graph,
            &[SequencedEvent {
                sequence: 1,
                event: created.clone(),
            }],
        )
        .unwrap();
        Self {
            graph,
            state,
            log: vec![created],
        }
    }

    fn try_feed(&mut self, input: Input) -> Result<workflow_core::engine::Transition, EngineError> {
        let t = decide(&self.graph, &self.state, input)?;
        for e in &t.events {
            self.state.apply(&self.graph, e).unwrap();
            self.log.push(e.clone());
        }
        Ok(t)
    }

    fn feed(&mut self, input: Input) -> workflow_core::engine::Transition {
        self.try_feed(input).unwrap()
    }

    fn sequenced(&self) -> Vec<SequencedEvent> {
        self.log
            .iter()
            .enumerate()
            .map(|(i, e)| SequencedEvent {
                sequence: i as u64 + 1,
                event: e.clone(),
            })
            .collect()
    }

    fn executions_of(&self, node: &str) -> Vec<&workflow_core::engine::ExecutionRecord> {
        self.state
            .executions
            .iter()
            .filter(|e| e.node_id == node)
            .collect()
    }

    fn path(&self) -> Vec<String> {
        self.state
            .executions
            .iter()
            .map(|e| e.node_id.clone())
            .collect()
    }
}

type Script = HashMap<&'static str, VecDeque<RunnerOutcome>>;

fn ok(output: Value) -> RunnerOutcome {
    RunnerOutcome::Completed {
        output,
        exit_code: 0,
        session_id: None,
    }
}

fn template_script(reviews: &[Value]) -> Script {
    let mut script: Script = HashMap::new();
    script.insert("analysis", VecDeque::from([ok(samples::analysis())]));
    script.insert(
        "implementation",
        VecDeque::from([ok(samples::implementation())]),
    );
    script.insert(
        "fix",
        reviews
            .iter()
            .map(|_| ok(samples::implementation()))
            .collect(),
    );
    script.insert("review", reviews.iter().cloned().map(ok).collect());
    script
}

/// Prowadzi przebieg do końca, do zgody albo do błędu; zwraca skutek ostatniego kroku.
fn drive(sim: &mut Sim, script: &mut Script) {
    if sim.state.status == RunStatus::Queued {
        sim.feed(Input::Start);
    }
    while sim.state.status == RunStatus::Running {
        let t = sim.feed(Input::Continue);
        match t.effects.first() {
            Some(Effect::LaunchAgent {
                execution_id,
                node_id,
                ..
            }) => {
                let outcome = script
                    .get_mut(node_id.as_str())
                    .and_then(VecDeque::pop_front)
                    .expect("skrypt dla węzła");
                sim.feed(Input::ExecutionResult {
                    execution_id: execution_id.clone(),
                    outcome,
                });
            }
            Some(Effect::RequestApproval { approval_id, .. }) => {
                sim.feed(Input::ApprovalPrepared {
                    approval_id: approval_id.clone(),
                    artifact_hash: HASH.into(),
                });
            }
            _ => {}
        }
    }
}

fn decision(sim: &mut Sim, decision: Decision, request: &str) -> Result<(), EngineError> {
    let approval = sim.state.approvals.last().expect("zgoda").id.clone();
    sim.try_feed(Input::ApprovalDecision {
        approval_id: approval,
        decision,
        request_id: Some(request.into()),
        comment: (decision == Decision::Reject).then(|| "Do poprawy".to_owned()),
        artifact_hash: HASH.into(),
        current_hash: HASH.into(),
    })
    .map(|_| ())
}

fn changes() -> Value {
    samples::review_changes()
}

fn approved() -> Value {
    samples::review_approved()
}

#[test]
fn happy_path_waits_for_approval_then_succeeds() {
    let mut sim = Sim::new(analysis_implementation_review());
    let mut script = template_script(&[approved()]);
    script.get_mut("fix").unwrap().clear();
    drive(&mut sim, &mut script);
    assert_eq!(sim.state.status, RunStatus::WaitingForApproval);
    let approval = &sim.state.approvals[0];
    assert_eq!(approval.status, ApprovalStatus::Pending);
    assert_eq!(approval.artifact_hash, HASH);
    assert_eq!(
        sim.state.unfinished().map(|e| e.status),
        Some(ExecutionStatus::Waiting)
    );
    decision(&mut sim, Decision::Approve, "r1").unwrap();
    drive(&mut sim, &mut script);
    assert_eq!(sim.state.status, RunStatus::Succeeded);
    assert_eq!(
        sim.path(),
        [
            "analysis",
            "implementation",
            "review",
            "decision",
            "approval",
            "done"
        ]
    );
    assert!(sim.state.skipped.contains(&"rejected".to_owned()));
}

#[test]
fn rejecting_ends_the_run_as_rejected() {
    let mut sim = Sim::new(analysis_implementation_review());
    let mut script = template_script(&[approved()]);
    drive(&mut sim, &mut script);
    decision(&mut sim, Decision::Reject, "r1").unwrap();
    drive(&mut sim, &mut script);
    assert_eq!(sim.state.status, RunStatus::Rejected);
    assert_eq!(
        sim.executions_of("approval")[0].status,
        ExecutionStatus::Rejected
    );
    assert!(sim.state.skipped.contains(&"done".to_owned()));
}

#[test]
fn repair_rounds_one_two_and_three_then_approved() {
    for repairs in 1..=3usize {
        let mut reviews = vec![changes(); repairs];
        reviews.push(approved());
        let mut sim = Sim::new(analysis_implementation_review());
        let mut script = template_script(&reviews);
        drive(&mut sim, &mut script);
        assert_eq!(sim.state.status, RunStatus::WaitingForApproval, "{repairs}");
        assert_eq!(sim.executions_of("fix").len(), repairs);
        assert_eq!(sim.executions_of("review").len(), repairs + 1);
        assert_eq!(sim.state.repair_rounds["decision"], repairs as u32);
    }
}

#[test]
fn fourth_changes_requested_fails_with_review_limit() {
    let mut sim = Sim::new(analysis_implementation_review());
    let mut script = template_script(&[changes(), changes(), changes(), changes()]);
    drive(&mut sim, &mut script);
    assert_eq!(sim.executions_of("fix").len(), 3);
    assert_eq!(sim.executions_of("review").len(), 4);
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::ReviewLimitExceeded));
    let last = sim.executions_of("decision").last().unwrap().status;
    assert_eq!(last, ExecutionStatus::Failed);
}

#[test]
fn every_visit_is_a_new_execution_and_fix_reads_the_last_review() {
    let mut sim = Sim::new(analysis_implementation_review());
    let mut script = template_script(&[changes(), changes(), approved()]);
    drive(&mut sim, &mut script);
    let reviews = sim.executions_of("review");
    assert_eq!(
        reviews
            .iter()
            .map(|e| (e.iteration, e.attempt))
            .collect::<Vec<_>>(),
        [(1, 1), (2, 1), (3, 1)]
    );
    // Wcześniejsze wyniki review nie zostały nadpisane.
    assert!(reviews.iter().all(|e| e.output.is_some()));
    let fixes = sim.executions_of("fix");
    for (fix, expected_review) in fixes.iter().zip([&reviews[0], &reviews[1]]) {
        let review_input = fix.inputs.iter().find(|i| i.node_id == "review").unwrap();
        assert_eq!(review_input.execution_id, expected_review.id);
    }
    let second_fix = fixes[1];
    assert_ne!(
        second_fix
            .inputs
            .iter()
            .find(|i| i.node_id == "review")
            .unwrap()
            .execution_id,
        reviews[0].id
    );
}

fn tiny_graph(op: CompareOp, value: Value, field: &str) -> WorkflowGraph {
    use workflow_core::template::analysis_implementation_review as t;
    let mut graph = t();
    graph
        .nodes
        .retain(|n| matches!(n.id.as_str(), "start" | "analysis" | "done" | "rejected"));
    graph.nodes.push(Node {
        id: "cond".into(),
        label: "w".into(),
        position: Position::default(),
        kind: NodeKind::Condition(ConditionConfig {
            field: FieldRef {
                node_id: "analysis".into(),
                field: field.into(),
            },
            op,
            value,
            repair_limit: None,
        }),
    });
    let edge = |s: &str, p: Port, t: &str| Edge {
        id: format!("{s}-{t}"),
        source: s.into(),
        source_port: p,
        target: t.into(),
    };
    graph.edges = vec![
        edge("start", Port::Out, "analysis"),
        edge("analysis", Port::Out, "cond"),
        edge("cond", Port::True, "done"),
        edge("cond", Port::False, "rejected"),
    ];
    assert_eq!(graph.schema_version, SCHEMA_VERSION);
    graph
}

fn analysis_script() -> Script {
    HashMap::from([("analysis", VecDeque::from([ok(samples::analysis())]))])
}

#[test]
fn both_condition_branches_are_taken() {
    // riskCount == 1 w próbce analizy.
    let mut yes = Sim::new(tiny_graph(CompareOp::GreaterThan, json!(0), "riskCount"));
    drive(&mut yes, &mut analysis_script());
    assert_eq!(yes.state.status, RunStatus::Succeeded);
    assert_eq!(yes.path(), ["analysis", "cond", "done"]);
    assert!(yes.state.skipped.contains(&"rejected".to_owned()));
    assert!(!yes.state.skipped.contains(&"done".to_owned()));

    let mut no = Sim::new(tiny_graph(CompareOp::GreaterThan, json!(5), "riskCount"));
    drive(&mut no, &mut analysis_script());
    assert_eq!(no.state.status, RunStatus::Rejected);
    assert_eq!(no.state.skipped, ["done"]);
}

#[test]
fn condition_records_which_execution_it_read() {
    let mut sim = Sim::new(tiny_graph(CompareOp::GreaterThan, json!(0), "riskCount"));
    drive(&mut sim, &mut analysis_script());
    let analysis_id = sim.executions_of("analysis")[0].id.clone();
    let read = sim.log.iter().find_map(|e| match e {
        RunEvent::ConditionEvaluated {
            read_execution_id, ..
        } => Some(read_execution_id.clone()),
        _ => None,
    });
    assert_eq!(read, Some(analysis_id));
}

#[test]
fn condition_type_mismatch_fails_the_run_instead_of_defaulting() {
    let mut sim = Sim::new(tiny_graph(CompareOp::GreaterThan, json!(0), "summary"));
    drive(&mut sim, &mut analysis_script());
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::InvalidDefinition));
    assert!(sim
        .log
        .iter()
        .all(|e| !matches!(e, RunEvent::ConditionEvaluated { .. })));
}

#[test]
fn condition_without_a_result_to_read_fails() {
    let mut graph = tiny_graph(CompareOp::GreaterThan, json!(0), "riskCount");
    // Warunek czyta węzeł, który nigdy się nie wykonał.
    if let NodeKind::Condition(c) = &mut graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "cond")
        .unwrap()
        .kind
    {
        c.field.node_id = "rejected".into();
    }
    let mut sim = Sim::new(graph);
    drive(&mut sim, &mut analysis_script());
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::InvalidOutput));
}

#[test]
fn timeout_fails_the_run_without_a_next_launch() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::Start);
    let t = sim.feed(Input::Continue);
    let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    let t = sim.feed(Input::ExecutionResult {
        execution_id,
        outcome: RunnerOutcome::Failed {
            code: FailureCode::Timeout,
            message: "limit".into(),
        },
    });
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::Timeout));
    assert!(sim.try_feed(Input::Continue).is_err());
}

#[test]
fn invalid_output_and_empty_success_fail_with_invalid_output() {
    for outcome in [
        ok(json!({"summary": "tylko"})),
        ok(Value::Null),
        RunnerOutcome::Completed {
            output: Value::Null,
            exit_code: 0,
            session_id: None,
        },
    ] {
        let mut sim = Sim::new(analysis_implementation_review());
        let mut script: Script = HashMap::from([("analysis", VecDeque::from([outcome]))]);
        drive(&mut sim, &mut script);
        assert_eq!(sim.state.status, RunStatus::Failed);
        assert_eq!(sim.state.failure, Some(FailureCode::InvalidOutput));
        assert_eq!(sim.executions_of("implementation").len(), 0);
    }
}

#[test]
fn cancelling_a_queued_run_has_no_effects() {
    let mut sim = Sim::new(analysis_implementation_review());
    let t = sim.feed(Input::CancelRequested);
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Cancelled);
    assert!(sim.state.executions.is_empty());
}

#[test]
fn cancelling_a_running_step_goes_through_cancelling() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::Start);
    let t = sim.feed(Input::Continue);
    let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    let t = sim.feed(Input::CancelRequested);
    assert_eq!(sim.state.status, RunStatus::Cancelling);
    assert_eq!(
        t.effects,
        [Effect::CancelExecution {
            execution_id: execution_id.clone()
        }]
    );
    let t = sim.feed(Input::CancelConfirmed { execution_id });
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Cancelled);
    assert_eq!(
        sim.executions_of("analysis")[0].status,
        ExecutionStatus::Cancelled
    );
    assert!(sim.try_feed(Input::Continue).is_err());
}

#[test]
fn result_after_cancel_is_recorded_but_starts_nothing() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::Start);
    let t = sim.feed(Input::Continue);
    let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    sim.feed(Input::CancelRequested);
    let t = sim.feed(Input::ExecutionResult {
        execution_id,
        outcome: ok(samples::analysis()),
    });
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Cancelled);
    let analysis = &sim.executions_of("analysis")[0];
    assert_eq!(analysis.status, ExecutionStatus::Completed);
    assert!(analysis.output.is_some());
    assert!(sim.executions_of("implementation").is_empty());
}

#[test]
fn cancelling_while_waiting_for_approval_invalidates_it() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    sim.feed(Input::CancelRequested);
    assert_eq!(sim.state.status, RunStatus::Cancelled);
    assert_eq!(sim.state.approvals[0].status, ApprovalStatus::Invalidated);
}

#[test]
fn stale_hash_invalidates_and_requests_a_new_approval() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    let first = sim.state.approvals[0].id.clone();
    sim.feed(Input::ApprovalDecision {
        approval_id: first.clone(),
        decision: Decision::Approve,
        request_id: Some("r1".into()),
        comment: None,
        artifact_hash: HASH.into(),
        current_hash: "changed".into(),
    });
    assert_eq!(sim.state.approvals[0].status, ApprovalStatus::Invalidated);
    assert_eq!(sim.state.approvals[1].status, ApprovalStatus::Pending);
    assert_eq!(sim.state.approvals[1].artifact_hash, "changed");
    assert_eq!(sim.state.status, RunStatus::WaitingForApproval);
    assert!(decision_on(&mut sim, &first).is_err());
}

fn decision_on(sim: &mut Sim, approval: &str) -> Result<(), EngineError> {
    sim.try_feed(Input::ApprovalDecision {
        approval_id: approval.into(),
        decision: Decision::Approve,
        request_id: Some("again".into()),
        comment: None,
        artifact_hash: HASH.into(),
        current_hash: HASH.into(),
    })
    .map(|_| ())
}

#[test]
fn same_outputs_replayed_twice_give_identical_events_and_path() {
    let reviews = [changes(), changes(), approved()];
    let run = || {
        let mut sim = Sim::new(analysis_implementation_review());
        drive(&mut sim, &mut template_script(&reviews));
        sim
    };
    let (a, b) = (run(), run());
    assert_eq!(a.log, b.log);
    assert_eq!(a.path(), b.path());
}

#[test]
fn replay_equals_the_state_built_step_by_step() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[changes(), approved()]));
    decision(&mut sim, Decision::Approve, "r").unwrap();
    drive(&mut sim, &mut template_script(&[]));
    let replayed = RunState::replay(&sim.graph, &sim.sequenced()).unwrap();
    assert_eq!(replayed, sim.state);
    assert_eq!(replayed.status, RunStatus::Succeeded);
}

#[test]
fn replay_rejects_a_sequence_gap_and_a_forbidden_transition() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    let mut gap = sim.sequenced();
    gap.remove(2);
    assert!(matches!(
        RunState::replay(&sim.graph, &gap),
        Err(ReplayError::SequenceGap { .. })
    ));

    let mut events = sim.sequenced();
    events.push(SequencedEvent {
        sequence: events.len() as u64 + 1,
        event: RunEvent::RunStateChanged {
            from: RunStatus::WaitingForApproval,
            to: RunStatus::Succeeded,
            code: None,
        },
    });
    assert!(matches!(
        RunState::replay(&sim.graph, &events),
        Err(ReplayError::ForbiddenTransition { .. })
    ));

    let headless = &sim.sequenced()[1..];
    assert!(RunState::replay(&sim.graph, headless).is_err());
}

#[test]
fn skipped_nodes_name_the_branch_that_was_not_chosen() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    decision(&mut sim, Decision::Approve, "r").unwrap();
    drive(&mut sim, &mut template_script(&[]));
    assert_eq!(sim.state.skipped, ["fix", "rejected"]);
}

#[test]
fn execution_ids_are_deterministic_and_unique() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[changes(), approved()]));
    let ids: Vec<&str> = sim.state.executions.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids[0], "1-1");
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn recover_marks_an_unfinished_step_interrupted_without_relaunch() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::Start);
    sim.feed(Input::Continue);
    let t = sim.feed(Input::Recover);
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Interrupted);
    assert_eq!(
        sim.executions_of("analysis")[0].status,
        ExecutionStatus::Interrupted
    );
    // Na granicy kroków nie ma czego przerywać.
    let mut boundary = Sim::new(analysis_implementation_review());
    boundary.feed(Input::Start);
    let t = boundary.feed(Input::Continue);
    let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    boundary.feed(Input::ExecutionResult {
        execution_id,
        outcome: ok(samples::analysis()),
    });
    assert!(boundary.feed(Input::Recover).events.is_empty());
    assert_eq!(boundary.state.status, RunStatus::Running);
}

fn launched(sim: &mut Sim) -> String {
    sim.feed(Input::Start);
    let t = sim.feed(Input::Continue);
    let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    execution_id
}

#[test]
fn unresponsive_runner_interrupts_the_run_and_its_step_once() {
    let mut sim = Sim::new(analysis_implementation_review());
    let execution_id = launched(&mut sim);
    let t = sim.feed(Input::Unresponsive {
        execution_id: execution_id.clone(),
    });
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Interrupted);
    assert_eq!(sim.state.failure, Some(FailureCode::Interrupted));
    assert_eq!(
        sim.executions_of("analysis")[0].status,
        ExecutionStatus::Interrupted
    );
    // Późny wynik runnera ani powtórka nie zmieniają zapisanego końca.
    assert!(sim
        .try_feed(Input::Unresponsive {
            execution_id: execution_id.clone()
        })
        .is_err());
    assert!(sim
        .try_feed(Input::ExecutionResult {
            execution_id,
            outcome: RunnerOutcome::Cancelled,
        })
        .is_err());
}

#[test]
fn fault_fails_an_active_run_and_is_refused_outside_running() {
    let mut sim = Sim::new(analysis_implementation_review());
    let fault = || Input::Fault {
        code: FailureCode::StorageError,
        message: "zapis".into(),
    };
    launched(&mut sim);
    let t = sim.feed(fault());
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::StorageError));
    assert_eq!(
        sim.executions_of("analysis")[0].status,
        ExecutionStatus::Failed
    );
    assert!(sim.try_feed(fault()).is_err());
}

#[test]
fn fault_on_a_queued_run_fails_it_without_executions() {
    let mut sim = Sim::new(analysis_implementation_review());
    let t = sim.feed(Input::Fault {
        code: FailureCode::WorkspaceUnavailable,
        message: "brak worktree".into(),
    });
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Failed);
    assert_eq!(sim.state.failure, Some(FailureCode::WorkspaceUnavailable));
    assert!(sim.state.executions.is_empty());
}

#[test]
fn fault_on_a_finished_run_is_rejected() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::CancelRequested);
    assert_eq!(sim.state.status, RunStatus::Cancelled);
    assert!(sim
        .try_feed(Input::Fault {
            code: FailureCode::StorageError,
            message: "x".into(),
        })
        .is_err());
}

#[test]
fn rejection_comment_is_kept_in_state() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    let approval = sim.state.approvals.last().unwrap().id.clone();
    sim.feed(Input::ApprovalDecision {
        approval_id: approval,
        decision: Decision::Reject,
        request_id: Some("r9".into()),
        comment: Some("Zła nazwa funkcji".into()),
        artifact_hash: HASH.into(),
        current_hash: HASH.into(),
    });
    let record = sim.state.approvals.last().unwrap();
    assert_eq!(record.comment.as_deref(), Some("Zła nazwa funkcji"));
    // Odtworzenie z zapisanych zdarzeń daje ten sam komentarz.
    let replayed = RunState::replay(&sim.graph, &sim.sequenced()).unwrap();
    assert_eq!(replayed.approvals.last().unwrap().comment, record.comment);
}

#[test]
fn reject_without_comment_is_rejected_by_engine() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    let approval = sim.state.approvals.last().unwrap().id.clone();
    let before = sim.log.len();
    for comment in [
        None,
        Some(String::new()),
        Some(
            "  
 "
            .to_owned(),
        ),
    ] {
        let result = sim.try_feed(Input::ApprovalDecision {
            approval_id: approval.clone(),
            decision: Decision::Reject,
            request_id: Some("r-empty".into()),
            comment,
            artifact_hash: HASH.into(),
            current_hash: HASH.into(),
        });
        assert!(matches!(result, Err(EngineError::InvalidState(_))));
    }
    assert_eq!(
        sim.log.len(),
        before,
        "odrzucenie bez komentarza dodało zdarzenia"
    );
    assert_eq!(sim.state.status, RunStatus::WaitingForApproval);
}

#[test]
fn approval_comment_is_trimmed_by_engine() {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    let approval = sim.state.approvals.last().unwrap().id.clone();
    sim.feed(Input::ApprovalDecision {
        approval_id: approval,
        decision: Decision::Reject,
        request_id: Some("r-trim".into()),
        comment: Some("  Zła nazwa  ".into()),
        artifact_hash: HASH.into(),
        current_hash: HASH.into(),
    });
    assert_eq!(
        sim.state.approvals.last().unwrap().comment.as_deref(),
        Some("Zła nazwa")
    );
}

// ---- E6: odzyskiwanie ----

/// Prowadzi do kroku zgody, którego zgoda nie została jeszcze przygotowana.
fn unprepared_approval(sim: &mut Sim) -> (String, String) {
    let mut script = template_script(&[approved()]);
    sim.feed(Input::Start);
    loop {
        let t = sim.feed(Input::Continue);
        match t.effects.first() {
            Some(Effect::LaunchAgent {
                execution_id,
                node_id,
                ..
            }) => {
                let outcome = script
                    .get_mut(node_id.as_str())
                    .and_then(VecDeque::pop_front)
                    .expect("skrypt dla węzła");
                sim.feed(Input::ExecutionResult {
                    execution_id: execution_id.clone(),
                    outcome,
                });
            }
            Some(Effect::RequestApproval {
                approval_id,
                execution_id,
            }) => return (approval_id.clone(), execution_id.clone()),
            other => panic!("nieoczekiwany skutek: {other:?}"),
        }
    }
}

fn waiting_sim() -> Sim {
    let mut sim = Sim::new(analysis_implementation_review());
    drive(&mut sim, &mut template_script(&[approved()]));
    assert_eq!(sim.state.status, RunStatus::WaitingForApproval);
    sim
}

#[test]
fn recover_of_unprepared_approval_requests_it_again_without_interrupt() {
    let mut sim = Sim::new(analysis_implementation_review());
    let (approval_id, execution_id) = unprepared_approval(&mut sim);
    let before = sim.state.event_count;
    let t = sim.feed(Input::Recover);
    assert!(t.events.is_empty());
    assert_eq!(
        t.effects,
        vec![Effect::RequestApproval {
            approval_id,
            execution_id
        }]
    );
    assert_eq!(sim.state.status, RunStatus::Running);
    assert_eq!(sim.state.event_count, before);
}

#[test]
fn recover_of_cancelling_run_without_step_ends_cancelled() {
    let mut sim = Sim::new(analysis_implementation_review());
    let execution_id = launched(&mut sim);
    sim.feed(Input::CancelRequested);
    assert_eq!(sim.state.status, RunStatus::Cancelling);
    // Awaria po zapisaniu końca kroku, a przed zapisaniem końca przebiegu.
    let finished = RunEvent::ExecutionFinished {
        execution_id,
        status: ExecutionStatus::Cancelled,
        output: None,
        code: Some(FailureCode::Cancelled),
        message: None,
        exit_code: None,
        session_id: None,
    };
    sim.state.apply(&sim.graph, &finished).unwrap();
    sim.log.push(finished);
    let t = sim.feed(Input::Recover);
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Cancelled);
}

/// Kryterium 3 E6: `Cancelling` z krokiem bez wyniku po restarcie daje `Interrupted`
/// (koniec procesu niepotwierdzony), a nie `Cancelled`, i nie startuje niczego.
#[test]
fn recover_of_cancelling_run_with_step_ends_interrupted() {
    let mut sim = Sim::new(analysis_implementation_review());
    launched(&mut sim);
    sim.feed(Input::CancelRequested);
    assert_eq!(sim.state.status, RunStatus::Cancelling);
    assert!(sim.state.unfinished().is_some());
    let t = sim.feed(Input::Recover);
    assert!(t.effects.is_empty());
    assert_eq!(sim.state.status, RunStatus::Interrupted);
    assert_eq!(sim.state.failure, Some(FailureCode::Interrupted));
    assert_eq!(
        sim.executions_of("analysis")[0].status,
        ExecutionStatus::Interrupted
    );
}

#[test]
fn revalidated_approval_with_same_hash_emits_nothing() {
    let mut sim = waiting_sim();
    let id = sim.state.approvals[0].id.clone();
    let t = sim.feed(Input::ApprovalRevalidated {
        approval_id: id,
        current_hash: HASH.into(),
    });
    assert!(t.events.is_empty() && t.effects.is_empty());
    assert_eq!(sim.state.approvals.len(), 1);
}

#[test]
fn revalidated_approval_with_changed_hash_invalidates_and_requests_new() {
    let mut sim = waiting_sim();
    let id = sim.state.approvals[0].id.clone();
    sim.feed(Input::ApprovalRevalidated {
        approval_id: id,
        current_hash: "changed".into(),
    });
    assert_eq!(sim.state.approvals[0].status, ApprovalStatus::Invalidated);
    assert_eq!(sim.state.approvals[0].decision, None);
    assert_eq!(sim.state.approvals[1].status, ApprovalStatus::Pending);
    assert_eq!(sim.state.approvals[1].artifact_hash, "changed");
    assert_eq!(sim.state.status, RunStatus::WaitingForApproval);
    let replayed = RunState::replay(&sim.graph, &sim.sequenced()).unwrap();
    assert_eq!(replayed, sim.state);
}

#[test]
fn revalidate_outside_waiting_is_invalid_state() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::Start);
    let err = sim
        .try_feed(Input::ApprovalRevalidated {
            approval_id: "x".into(),
            current_hash: HASH.into(),
        })
        .unwrap_err();
    assert!(matches!(err, EngineError::InvalidState(_)));
}

fn abandon() -> Input {
    Input::Abandon {
        message: "Przebieg z przywróconej kopii nie jest wznawiany".into(),
    }
}

#[test]
fn abandon_turns_every_active_status_into_interrupted() {
    let mut queued = Sim::new(analysis_implementation_review());
    let t = queued.feed(abandon());
    assert!(t.effects.is_empty());
    assert_eq!(queued.state.status, RunStatus::Interrupted);

    let mut running = Sim::new(analysis_implementation_review());
    launched(&mut running);
    running.feed(abandon());
    assert_eq!(running.state.status, RunStatus::Interrupted);
    assert_eq!(
        running.executions_of("analysis")[0].status,
        ExecutionStatus::Interrupted
    );
    assert_eq!(running.state.failure, Some(FailureCode::Interrupted));

    let mut waiting = waiting_sim();
    waiting.feed(abandon());
    assert_eq!(waiting.state.status, RunStatus::Interrupted);
    assert_eq!(
        waiting.state.approvals[0].status,
        ApprovalStatus::Invalidated
    );
    assert_eq!(
        waiting.executions_of("approval")[0].status,
        ExecutionStatus::Interrupted
    );

    let mut cancelling = Sim::new(analysis_implementation_review());
    launched(&mut cancelling);
    cancelling.feed(Input::CancelRequested);
    cancelling.feed(abandon());
    assert_eq!(cancelling.state.status, RunStatus::Interrupted);

    for sim in [&queued, &running, &waiting, &cancelling] {
        let replayed = RunState::replay(&sim.graph, &sim.sequenced()).unwrap();
        assert_eq!(replayed, sim.state);
    }
}

#[test]
fn abandon_of_terminal_run_changes_nothing() {
    let mut sim = Sim::new(analysis_implementation_review());
    sim.feed(Input::CancelRequested);
    let t = sim.feed(abandon());
    assert!(t.events.is_empty());
    assert_eq!(sim.state.status, RunStatus::Cancelled);
}

#[test]
fn reconciled_event_keeps_interrupted_and_replays() {
    let mut sim = Sim::new(analysis_implementation_review());
    launched(&mut sim);
    sim.feed(Input::Recover);
    assert_eq!(sim.state.status, RunStatus::Interrupted);
    let t = sim.feed(Input::Reconcile {
        processes_confirmed: true,
        workspace_released: true,
        note: None,
    });
    let event = t.events[0].clone();
    assert_eq!(event.stream_type(), "recovery");
    assert_eq!(event.execution_id(), None);
    assert_eq!(sim.state.status, RunStatus::Interrupted);
    let replayed = RunState::replay(&sim.graph, &sim.sequenced()).unwrap();
    assert_eq!(replayed, sim.state);
    // Poza Interrupted audyt jest niedozwolony.
    let mut running = Sim::new(analysis_implementation_review());
    running.feed(Input::Start);
    assert!(running
        .try_feed(Input::Reconcile {
            processes_confirmed: true,
            workspace_released: true,
            note: None
        })
        .is_err());
}
