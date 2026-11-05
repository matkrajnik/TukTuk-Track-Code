//! Sterownik na wątkach std z deterministycznym runnerem (cargo test --features testing).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use workflow_core::driver::{
    DecideError, DecideRequest, DriverConfig, Runtime, RuntimeHandle,
    MAX_RUNNER_EVENTS_PER_EXECUTION,
};
use workflow_core::engine::{Effect, Input, RunEvent, RunState};
use workflow_core::model::AgentConfig;
use workflow_core::ports::{
    CreatedRun, NewRun, RepoError, RunId, RunRepository, RunnerEvent, StoredEvent,
};
use workflow_core::template::analysis_implementation_review;
use workflow_core::testing::{
    apply_inputs, message, samples, FixedWorkspace, MemoryRepository, RecordingSink,
    ScriptedRunner, Step,
};
use workflow_core::{
    ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus, WorkflowGraph,
};

const WAIT: Duration = Duration::from_secs(10);

struct Env {
    repo: Arc<MemoryRepository>,
    sink: Arc<RecordingSink>,
    workspace: Arc<FixedWorkspace>,
}

impl Env {
    fn new() -> Self {
        Self {
            repo: Arc::new(MemoryRepository::new()),
            sink: Arc::new(RecordingSink::new()),
            workspace: Arc::new(FixedWorkspace::new("h1")),
        }
    }

    fn run(&self, runner: &Arc<ScriptedRunner>, config: DriverConfig) -> RuntimeHandle {
        self.run_on(self.repo.clone(), runner, config)
    }

    fn run_on(
        &self,
        repo: Arc<dyn RunRepository>,
        runner: &Arc<ScriptedRunner>,
        config: DriverConfig,
    ) -> RuntimeHandle {
        Runtime::start(
            repo,
            runner.clone(),
            self.sink.clone(),
            self.workspace.clone(),
            config,
        )
    }

    fn create(&self, request: &str) -> RunId {
        self.repo.create(&analysis_implementation_review(), request)
    }

    fn state(&self, run: RunId) -> RunState {
        self.repo.state(run)
    }

    fn decide(
        &self,
        handle: &RuntimeHandle,
        run: RunId,
        decision: Decision,
        request: &str,
    ) -> Result<workflow_core::driver::DecideResult, DecideError> {
        let state = self.state(run);
        let approval = state.approvals.last().expect("zgoda");
        handle.decide(DecideRequest {
            run_id: run,
            approval_id: approval.id.clone(),
            decision,
            request_id: request.into(),
            expected_revision: state.event_count,
            artifact_hash: approval.artifact_hash.clone(),
        })
    }
}

fn config() -> DriverConfig {
    DriverConfig {
        step_timeout: |_: &AgentConfig| Duration::from_secs(20),
        // Hojnie: krok reaguje na anulowanie od razu, a obciążona maszyna nie myli go
        // z runnerem bez reakcji.
        cancel_grace: Duration::from_secs(2),
        poll_interval: Duration::from_millis(5),
        now: || 1_000,
        on_error: |m| panic!("błąd sterownika: {m}"),
    }
}

fn runner_for(reviews: Vec<serde_json::Value>) -> Arc<ScriptedRunner> {
    Arc::new(ScriptedRunner::new().template_script(reviews))
}

fn settle(handle: &RuntimeHandle) {
    assert!(
        handle.wait_idle(WAIT),
        "sterownik nie uspokoił się w czasie"
    );
}

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < until, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn template_runs_to_approval_and_succeeds() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::WaitingForApproval);
    let result = env
        .decide(&handle, run, Decision::Approve, "d1")
        .expect("decyzja");
    assert_eq!(result.approval_status, ApprovalStatus::Approved);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Succeeded);
    assert_eq!(
        runner.call_order(),
        ["analysis", "implementation", "review"]
    );
}

#[test]
fn review_limit_is_enforced_through_the_driver() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_changes(); 4]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.failure, Some(FailureCode::ReviewLimitExceeded));
    assert_eq!(runner.calls("fix"), 3);
    assert_eq!(runner.calls("review"), 4);
}

#[test]
fn hung_step_times_out_and_receives_cancel() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script("analysis", vec![Step::hang()]));
    let handle = env.run(
        &runner,
        DriverConfig {
            step_timeout: |_| Duration::from_millis(100),
            ..config()
        },
    );
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.failure, Some(FailureCode::Timeout));
    assert!(runner.saw_cancel("analysis"));
    assert_eq!(runner.total_calls(), 1);
}

#[test]
fn cancel_during_a_hung_step_ends_cancelled_without_next_call() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script("analysis", vec![Step::hang()]));
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("start kroku", || runner.calls("analysis") == 1);
    handle.cancel(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Cancelled);
    assert!(runner.saw_cancel("analysis"));
    assert_eq!(runner.total_calls(), 1);
    assert!(state.cancel_requested);
}

#[test]
fn user_cancel_of_an_unresponsive_runner_ends_after_the_grace_not_the_step_timeout() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script(
        "analysis",
        vec![Step::stubborn(Duration::from_millis(3_000))],
    ));
    let handle = env.run(
        &runner,
        DriverConfig {
            step_timeout: |_| Duration::from_secs(60),
            cancel_grace: Duration::from_millis(200),
            ..quiet()
        },
    );
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("start kroku", || runner.calls("analysis") == 1);
    let cancelled_at = Instant::now();
    handle.cancel(run);
    wait_until("koniec czasu łaski po anulowaniu", || {
        env.state(run).status.is_terminal()
    });
    assert!(
        cancelled_at.elapsed() < Duration::from_millis(2_500),
        "przebieg czekał na limit kroku: {:?}",
        cancelled_at.elapsed()
    );
    assert_eq!(runner.running(), 1, "runner miał jeszcze działać");
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Interrupted);
    assert_eq!(state.executions[0].status, ExecutionStatus::Interrupted);
    assert!(state.cancel_requested);
    settle(&handle);
    assert_eq!(runner.total_calls(), 1);
}

#[test]
fn cancelling_a_queued_run_never_calls_the_runner() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.cancel(run);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Cancelled);
    assert_eq!(runner.total_calls(), 0);
}

#[test]
fn two_runs_never_hit_the_runner_concurrently() {
    let env = Env::new();
    let step = |value| Step::complete(value).delayed(Duration::from_millis(30));
    let queued_during_first = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let (repo, flag, counter) = (env.repo.clone(), queued_during_first.clone(), calls.clone());
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "analysis",
                vec![step(samples::analysis()), step(samples::analysis())],
            )
            .script(
                "implementation",
                vec![
                    step(samples::implementation()),
                    step(samples::implementation()),
                ],
            )
            .script(
                "review",
                vec![
                    step(samples::review_approved()),
                    step(samples::review_approved()),
                ],
            )
            .with_probe(move |_| {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    flag.store(repo.state(2).status == RunStatus::Queued, Ordering::SeqCst);
                }
            }),
    );
    let first = env.create("r1");
    let second = env.create("r2");
    let handle = env.run(&runner, config());
    handle.start_run(first);
    handle.start_run(second);
    settle(&handle);
    assert_eq!(runner.max_concurrency(), 1);
    assert!(
        queued_during_first.load(Ordering::SeqCst),
        "drugi nie czekał w Queued"
    );
    assert_eq!(env.state(first).status, RunStatus::WaitingForApproval);
    assert_eq!(env.state(second).status, RunStatus::WaitingForApproval);
    assert_eq!(runner.total_calls(), 6);
}

#[test]
fn restart_at_a_step_boundary_launches_the_next_step_once() {
    let env = Env::new();
    let run = env.create("r1");
    let effects = apply_inputs(&*env.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    apply_inputs(
        &*env.repo,
        run,
        vec![Input::ExecutionResult {
            execution_id,
            outcome: workflow_core::ports::RunnerOutcome::Completed {
                output: samples::analysis(),
                exit_code: 0,
                session_id: None,
            },
        }],
    )
    .unwrap();
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    );
    let handle = env.run(&runner, config());
    settle(&handle);
    assert_eq!(runner.calls("analysis"), 0);
    assert_eq!(runner.calls("implementation"), 1);
    assert_eq!(env.state(run).status, RunStatus::WaitingForApproval);
    drop(handle);
    // Kolejny restart niczego nie powtarza.
    let handle = env.run(&runner, config());
    settle(&handle);
    assert_eq!(runner.calls("implementation"), 1);
}

#[test]
fn restart_after_the_launch_intent_marks_the_run_interrupted() {
    let env = Env::new();
    let run = env.create("r1");
    apply_inputs(&*env.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Interrupted);
    assert_eq!(state.failure, Some(FailureCode::Interrupted));
    assert_eq!(runner.total_calls(), 0);
}

#[test]
fn same_responses_on_independent_repositories_give_the_same_path() {
    let reviews = || vec![samples::review_changes(), samples::review_approved()];
    let runs: Vec<(Vec<String>, Vec<RunEvent>)> = (0..2)
        .map(|_| {
            let env = Env::new();
            let runner = runner_for(reviews());
            let handle = env.run(&runner, config());
            let run = env.create("r1");
            handle.start_run(run);
            settle(&handle);
            let events: Vec<RunEvent> = env
                .repo
                .load_events(run, 0)
                .unwrap()
                .iter()
                .map(|e| e.to_run_event().unwrap())
                .collect();
            (runner.call_order(), events)
        })
        .collect();
    assert_eq!(runs[0].0, runs[1].0);
    assert_eq!(runs[0].0.len(), 5);
    assert!(runs[0].1.len() > 5);
    assert_eq!(runs[0].1, runs[1].1, "różne sekwencje zdarzeń");
}

#[test]
fn decision_is_idempotent_and_conflicting_decisions_are_rejected() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    let approval = state.approvals.last().unwrap().clone();
    let request = |decision, request_id: &str, revision| DecideRequest {
        run_id: run,
        approval_id: approval.id.clone(),
        decision,
        request_id: request_id.into(),
        expected_revision: revision,
        artifact_hash: approval.artifact_hash.clone(),
    };
    assert_eq!(
        handle.decide(request(Decision::Approve, "d1", state.event_count + 7)),
        Err(DecideError::Conflict)
    );
    let first = handle.decide(request(Decision::Approve, "d1", state.event_count));
    let again = handle.decide(request(Decision::Approve, "d1", 0));
    assert_eq!(
        first.map(|r| r.approval_status),
        again.map(|r| r.approval_status)
    );
    assert_eq!(
        handle.decide(request(Decision::Reject, "d2", 0)),
        Err(DecideError::Conflict)
    );
    settle(&handle);
    let resolved = env
        .repo
        .load_events(run, 0)
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == "approvalResolved")
        .count();
    assert_eq!(resolved, 1);
}

#[test]
fn changed_artifact_invalidates_the_approval() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    env.workspace.set_hash("h2");
    assert_eq!(
        env.decide(&handle, run, Decision::Approve, "d1"),
        Err(DecideError::Invalidated)
    );
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::WaitingForApproval);
    assert_eq!(state.approvals[0].status, ApprovalStatus::Invalidated);
    assert_eq!(state.approvals[1].artifact_hash, "h2");
    // Powtórka tego samego żądania daje ten sam wynik.
    assert_eq!(
        handle.decide(DecideRequest {
            run_id: run,
            approval_id: state.approvals[0].id.clone(),
            decision: Decision::Approve,
            request_id: "d1".into(),
            expected_revision: 0,
            artifact_hash: "h1".into(),
        }),
        Err(DecideError::Invalidated)
    );
    // Id żądania użyte przy unieważnionej zgodzie nie przechodzi na nową (jak UNIQUE w SQLite).
    assert_eq!(
        env.decide(&handle, run, Decision::Approve, "d1"),
        Err(DecideError::Conflict)
    );
    assert_eq!(env.state(run).approvals[1].status, ApprovalStatus::Pending);
    // Nowa zgoda dla aktualnego artefaktu przechodzi.
    assert!(env.decide(&handle, run, Decision::Approve, "d2").is_ok());
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Succeeded);
}

#[test]
fn storage_failure_while_preparing_approval_fails_the_run() {
    let env = Env::new();
    env.workspace.fail_next(true);
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.failure, Some(FailureCode::StorageError));
}

/// Repozytorium, które raz odrzuca zapis konfliktem albo odrzuca błędem bazy
/// `storage_failures` kolejnych zapisów kończących krok.
struct Flaky {
    inner: Arc<MemoryRepository>,
    fail_once: AtomicBool,
    storage_failures: AtomicUsize,
}

impl Flaky {
    fn new(inner: Arc<MemoryRepository>, fail_once: bool, storage_failures: usize) -> Self {
        Self {
            inner,
            fail_once: AtomicBool::new(fail_once),
            storage_failures: AtomicUsize::new(storage_failures),
        }
    }
}

impl RunRepository for Flaky {
    fn create_run(&self, new: &NewRun) -> Result<CreatedRun, RepoError> {
        self.inner.create_run(new)
    }
    fn graph(&self, run_id: RunId) -> Result<WorkflowGraph, RepoError> {
        self.inner.graph(run_id)
    }
    fn append(
        &self,
        run_id: RunId,
        expected_next_sequence: u64,
        events: &[RunEvent],
        now: i64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        if self.fail_once.swap(false, Ordering::SeqCst) {
            return Err(RepoError::Conflict);
        }
        let finishes_step = events
            .iter()
            .any(|e| matches!(e, RunEvent::ExecutionFinished { .. }));
        // Jedyny pisarz to wątek sterownika, więc odczyt i zmniejszenie się nie ścigają.
        if finishes_step && self.storage_failures.load(Ordering::SeqCst) > 0 {
            self.storage_failures.fetch_sub(1, Ordering::SeqCst);
            return Err(RepoError::Storage("dysk pełny".into()));
        }
        self.inner
            .append(run_id, expected_next_sequence, events, now)
    }
    fn load_events(&self, run_id: RunId, after: u64) -> Result<Vec<StoredEvent>, RepoError> {
        self.inner.load_events(run_id, after)
    }
    fn last_sequence(&self, run_id: RunId) -> Result<u64, RepoError> {
        self.inner.last_sequence(run_id)
    }
    fn active_runs(&self) -> Result<Vec<RunId>, RepoError> {
        self.inner.active_runs()
    }
}

#[test]
fn failed_append_emits_nothing_and_the_sink_matches_the_database() {
    let env = Env::new();
    let run = env.create("r1");
    let flaky = Arc::new(Flaky::new(env.repo.clone(), true, 0));
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run_on(flaky, &runner, config());
    handle.start_run(run);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::WaitingForApproval);
    let stored = env.repo.load_events(run, 0).unwrap();
    let emitted: Vec<StoredEvent> = env
        .sink
        .events()
        .into_iter()
        .filter(|e| e.run_id == run)
        .collect();
    // Zdarzenie 1 (RunCreated) powstało poza sterownikiem i nie trafia do magistrali.
    assert_eq!(emitted, stored[1..]);
    let sequences: Vec<u64> = emitted.iter().map(|e| e.sequence).collect();
    assert_eq!(sequences, (2..=stored.len() as u64).collect::<Vec<_>>());
}

#[test]
fn runner_messages_are_stored_with_an_explicit_truncation_marker() {
    let env = Env::new();
    let many: Vec<RunnerEvent> = (0..MAX_RUNNER_EVENTS_PER_EXECUTION + 100)
        .map(|i| message(&format!("linia {i}")))
        .collect();
    let huge = message(&"x".repeat(20_000));
    let runner = Arc::new(ScriptedRunner::new().script(
        "analysis",
        vec![Step::complete(samples::analysis()).with_events([vec![huge], many].concat())],
    ));
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("koniec kroku", || env.state(run).executions.len() >= 2);
    settle(&handle);
    let events = env.repo.load_events(run, 0).unwrap();
    let messages: Vec<&StoredEvent> = events.iter().filter(|e| e.kind == "message").collect();
    assert_eq!(messages.len(), MAX_RUNNER_EVENTS_PER_EXECUTION + 1);
    let truncated = messages
        .iter()
        .filter(|e| e.payload["payload"]["truncated"] == true)
        .count();
    assert_eq!(truncated, 2, "znacznik rozmiaru i znacznik limitu");
    assert_eq!(
        messages[0].payload["payload"]["originalBytes"]
            .as_u64()
            .is_some(),
        true
    );
}

fn quiet() -> DriverConfig {
    DriverConfig {
        on_error: |_| {},
        ..config()
    }
}

#[test]
fn storage_error_on_a_step_result_fails_the_run_instead_of_leaving_it_running() {
    let env = Env::new();
    let run = env.create("r1");
    let flaky = Arc::new(Flaky::new(env.repo.clone(), false, 1));
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run_on(flaky, &runner, quiet());
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.failure, Some(FailureCode::StorageError));
    assert_eq!(state.executions[0].status, ExecutionStatus::Failed);
    assert_eq!(runner.total_calls(), 1);
    assert!(env.sink.faults().is_empty());
}

#[test]
fn storage_error_that_also_blocks_the_failure_is_reported_on_the_bus() {
    let env = Env::new();
    let run = env.create("r1");
    let flaky = Arc::new(Flaky::new(env.repo.clone(), false, 2));
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run_on(flaky, &runner, quiet());
    handle.start_run(run);
    settle(&handle);
    let faults = env.sink.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].0, run);
    assert!(faults[0].1.contains("dysk pełny"), "{}", faults[0].1);
}

/// Krótki limit kroku i czas łaski; runner ignoruje anulowanie dłużej niż oba razem.
fn unresponsive_config() -> DriverConfig {
    DriverConfig {
        step_timeout: |_| Duration::from_millis(300),
        cancel_grace: Duration::from_millis(500),
        ..quiet()
    }
}

#[test]
fn unresponsive_runner_after_timeout_is_not_recorded_as_a_confirmed_end() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script(
        "analysis",
        vec![Step::stubborn(Duration::from_millis(3_000))],
    ));
    let handle = env.run(&runner, unresponsive_config());
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("koniec czasu łaski", || {
        env.state(run).status.is_terminal()
    });
    assert_eq!(runner.running(), 1, "runner miał jeszcze działać");
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Interrupted);
    assert_eq!(state.failure, Some(FailureCode::Interrupted));
    assert_eq!(state.executions[0].status, ExecutionStatus::Interrupted);
    let ends: Vec<(ExecutionStatus, Option<String>)> = env
        .repo
        .load_events(run, 0)
        .unwrap()
        .iter()
        .filter_map(|e| match e.to_run_event().unwrap() {
            RunEvent::ExecutionFinished {
                status, message, ..
            } => Some((status, message)),
            _ => None,
        })
        .collect();
    assert_eq!(
        ends,
        [(
            ExecutionStatus::Interrupted,
            Some(workflow_core::engine::UNRESPONSIVE_MESSAGE.to_owned())
        )]
    );
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Interrupted);
}

#[test]
fn queued_run_does_not_start_while_an_unresponsive_runner_is_alive() {
    let env = Env::new();
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "analysis",
                vec![
                    Step::stubborn(Duration::from_millis(3_000)),
                    Step::complete(samples::analysis()),
                ],
            )
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    );
    let first = env.create("r1");
    let second = env.create("r2");
    let handle = env.run(&runner, unresponsive_config());
    handle.start_run(first);
    handle.start_run(second);
    wait_until("pierwszy przerwany", || {
        env.state(first).status == RunStatus::Interrupted
    });
    assert!(runner.running() <= 1);
    assert_eq!(
        runner.calls("analysis"),
        1,
        "drugi przebieg ruszył za wcześnie"
    );
    assert_eq!(env.state(second).status, RunStatus::Queued);
    settle(&handle);
    assert_eq!(runner.max_concurrency(), 1);
    let st = env.state(second);
    assert_eq!(st.status, RunStatus::WaitingForApproval, "{:?}", st.failure);
}
