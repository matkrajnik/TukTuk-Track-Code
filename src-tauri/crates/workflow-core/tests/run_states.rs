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
fn paused_is_not_terminal_and_has_no_running_step() {
    assert!(!Paused.is_terminal());
    assert!(RunStatus::ALL.contains(&Paused));
    assert!(consistent(Paused, &[ExecutionStatus::Completed]));
    assert!(consistent(Paused, &[]));
    assert!(!consistent(Paused, &[ExecutionStatus::Running]));
    assert!(!consistent(Paused, &[ExecutionStatus::Waiting]));
}
