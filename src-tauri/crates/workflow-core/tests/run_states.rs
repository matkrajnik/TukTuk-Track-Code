use workflow_core::{consistent, ApprovalStatus, ExecutionStatus, FailureCode, RunStatus};
use RunStatus::*;

/// Pełna tablica dozwolonych przejść przebiegu (5.2); każda inna para jest zakazana.
const ALLOWED: &[(RunStatus, RunStatus)] = &[
    (Queued, Running),
    (Queued, Cancelled),
    (Queued, Failed),
    (Queued, Interrupted),
    (Queued, Paused),
    (Running, WaitingForApproval),
    (Running, Cancelling),
    (Running, Succeeded),
    (Running, Rejected),
    (Running, Failed),
    (Running, Interrupted),
    (Running, Paused),
    (WaitingForApproval, Running),
    (WaitingForApproval, Cancelling),
    (WaitingForApproval, Rejected),
    (WaitingForApproval, Interrupted),
    (Cancelling, Cancelled),
    (Cancelling, Failed),
    (Cancelling, Interrupted),
    // Decyzja po E7: wstrzymany po restarcie czeka na wznowienie, anulowanie,
    // błąd katalogu roboczego przy wznowieniu albo przywrócenie kopii.
    (Paused, Running),
    (Paused, Cancelled),
    (Paused, Failed),
    (Paused, Interrupted),
];

#[test]
fn every_run_transition_pair_matches_the_table() {
    for from in RunStatus::ALL {
        for to in RunStatus::ALL {
            assert_eq!(
                from.can_transition_to(to),
                ALLOWED.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
    }
}

#[test]
fn terminal_run_states_have_no_exits() {
    for status in RunStatus::ALL {
        if status.is_terminal() {
            for to in RunStatus::ALL {
                assert!(!status.can_transition_to(to), "{status:?} -> {to:?}");
            }
        }
    }
    assert!(Interrupted.is_terminal());
    assert!(!Cancelling.is_terminal());
}

#[test]
fn queued_can_be_cancelled_without_running() {
    assert!(Queued.can_transition_to(Cancelled));
    assert!(!Queued.can_transition_to(Succeeded));
}

#[test]
fn execution_transitions_follow_the_table() {
    use ExecutionStatus::*;
    const ALLOWED: &[(ExecutionStatus, ExecutionStatus)] = &[
        (Pending, Running),
        (Pending, Cancelled),
        (Pending, Skipped),
        (Running, Waiting),
        (Running, Completed),
        (Running, Failed),
        (Running, Cancelled),
        (Running, Interrupted),
        (Waiting, Running),
        (Waiting, Completed),
        (Waiting, Failed),
        (Waiting, Cancelled),
        (Waiting, Interrupted),
        (Waiting, Rejected),
    ];
    for from in ExecutionStatus::ALL {
        for to in ExecutionStatus::ALL {
            assert_eq!(
                from.can_transition_to(to),
                ALLOWED.contains(&(from, to)),
                "{from:?} -> {to:?}"
            );
        }
        if from.is_terminal() {
            assert!(ExecutionStatus::ALL
                .iter()
                .all(|t| !from.can_transition_to(*t)));
        }
    }
}

#[test]
fn consistent_rejects_succeeded_with_a_running_execution() {
    assert!(!consistent(
        Succeeded,
        &[ExecutionStatus::Completed, ExecutionStatus::Running]
    ));
    assert!(consistent(Succeeded, &[ExecutionStatus::Completed]));
    assert!(consistent(Running, &[ExecutionStatus::Running]));
    assert!(!consistent(
        Running,
        &[ExecutionStatus::Running, ExecutionStatus::Running]
    ));
    assert!(consistent(WaitingForApproval, &[ExecutionStatus::Waiting]));
    assert!(!consistent(
        WaitingForApproval,
        &[ExecutionStatus::Completed]
    ));
    assert!(!consistent(Queued, &[ExecutionStatus::Completed]));
}

fn json<T: serde::Serialize>(value: T) -> String {
    serde_json::to_string(&value).unwrap()
}

#[test]
fn names_serialize_in_camel_case() {
    assert_eq!(
        json(FailureCode::ReviewLimitExceeded),
        "\"reviewLimitExceeded\""
    );
    assert_eq!(json(WaitingForApproval), "\"waitingForApproval\"");
    assert_eq!(json(Paused), "\"paused\"");
    assert_eq!(json(ApprovalStatus::Invalidated), "\"invalidated\"");
    assert_eq!(json(ExecutionStatus::Interrupted), "\"interrupted\"");
    let back: FailureCode = serde_json::from_str("\"invalidOutput\"").unwrap();
    assert_eq!(back, FailureCode::InvalidOutput);
}

#[test]
fn every_failure_code_has_a_polish_message() {
    for code in [
        FailureCode::CliMissing,
        FailureCode::NotLoggedIn,
        FailureCode::UnsupportedVersion,
        FailureCode::SpawnFailed,
        FailureCode::ProtocolError,
        FailureCode::InvalidOutput,
        FailureCode::Timeout,
        FailureCode::Cancelled,
        FailureCode::PermissionViolation,
        FailureCode::StorageError,
        FailureCode::ReviewLimitExceeded,
        FailureCode::Interrupted,
        FailureCode::InvalidDefinition,
    ] {
        assert!(!code.message().is_empty());
    }
}

#[test]
fn approval_resolved_without_comment_still_replays() {
    use workflow_core::engine::RunEvent;
    let old = serde_json::json!({
        "type": "approvalResolved",
        "approvalId": "a1",
        "decision": "reject",
        "status": "rejected",
        "requestId": "r1"
    });
    let event: RunEvent = serde_json::from_value(old).unwrap();
    match event {
        RunEvent::ApprovalResolved { comment, .. } => assert_eq!(comment, None),
        other => panic!("{other:?}"),
    }
}

#[test]
fn old_tool_payload_without_ids_still_replays() {
    use workflow_core::engine::RunEvent;
    use workflow_core::ports::StoredEvent;
    let make = |sequence: u64, kind: &str, inner: serde_json::Value| StoredEvent {
        run_id: 1,
        sequence,
        execution_id: Some("e-1".into()),
        kind: kind.into(),
        timestamp: 0,
        payload: serde_json::json!({"type": "tool", "executionId": "e-1", "payload": inner}),
    };
    let started = make(
        1,
        "toolStarted",
        serde_json::json!({"phase": "started", "name": "Read"}),
    );
    let finished = make(
        2,
        "toolFinished",
        serde_json::json!({"phase": "finished", "name": "Read", "ok": true}),
    );
    for event in [&started, &finished] {
        let run_event = event.to_run_event().expect("stary kształt się odczytuje");
        assert!(matches!(run_event, RunEvent::Tool { .. }));
    }
    assert_eq!(started.to_run_event().unwrap().stream_type(), "toolStarted");
    assert_eq!(
        finished.to_run_event().unwrap().stream_type(),
        "toolFinished"
    );
}

#[test]
fn history_with_old_tool_events_replays_to_a_running_step() {
    use workflow_core::engine::{decide, Effect, Input, RunEvent, RunState, SequencedEvent};
    use workflow_core::ports::StoredEvent;
    use workflow_core::template::analysis_implementation_review;

    // Historia jak z bazy sprzed E8a: start kroku, potem zdarzenia narzędzi bez id.
    let graph = analysis_implementation_review();
    let created = RunEvent::RunCreated {
        run_id: 1,
        graph_hash: "g".into(),
    };
    let mut log = vec![created.clone()];
    let mut state = RunState::replay(
        &graph,
        &[SequencedEvent {
            sequence: 1,
            event: created,
        }],
    )
    .unwrap();
    let mut launched = None;
    for input in [Input::Start, Input::Continue] {
        let t = decide(&graph, &state, input).unwrap();
        for e in &t.events {
            state.apply(&graph, e).unwrap();
            log.push(e.clone());
        }
        if let Some(Effect::LaunchAgent { execution_id, .. }) = t.effects.first() {
            launched = Some(execution_id.clone());
        }
    }
    let execution_id = launched.expect("krok uruchomiony");
    let mut events: Vec<StoredEvent> = log
        .iter()
        .enumerate()
        .map(|(i, e)| StoredEvent::new(1, i as u64 + 1, 0, e))
        .collect();
    for (kind, inner) in [
        (
            "toolStarted",
            serde_json::json!({"phase": "started", "name": "Read"}),
        ),
        (
            "toolFinished",
            serde_json::json!({"phase": "finished", "name": "Read", "ok": true}),
        ),
    ] {
        events.push(StoredEvent {
            run_id: 1,
            sequence: events.len() as u64 + 1,
            execution_id: Some(execution_id.clone()),
            kind: kind.into(),
            timestamp: 0,
            payload: serde_json::json!({
                "type": "tool",
                "executionId": execution_id,
                "payload": inner
            }),
        });
    }

    let replayed = workflow_core::replay_stored(&graph, &events).expect("stara historia");
    assert_eq!(replayed.status, Running);
    let running: Vec<_> = replayed
        .executions
        .iter()
        .filter(|e| e.status == ExecutionStatus::Running)
        .collect();
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].id, execution_id);
}

#[test]
fn paused_is_not_terminal_and_has_no_running_step() {
    assert!(!Paused.is_terminal());
    assert!(RunStatus::ALL.contains(&Paused));
    assert!(consistent(Paused, &[ExecutionStatus::Completed]));
    assert!(consistent(Paused, &[]));
    assert!(!consistent(Paused, &[ExecutionStatus::Running]));
    assert!(!consistent(Paused, &[ExecutionStatus::Waiting]));
}
