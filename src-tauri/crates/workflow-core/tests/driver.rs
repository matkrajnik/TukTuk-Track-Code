//! Sterownik na wątkach std z deterministycznym runnerem (cargo test --features testing).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use workflow_core::driver::{
    DecideError, DecideRequest, DriverConfig, ResumeError, Runtime, RuntimeHandle,
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
            comment: None,
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
    // Bez tego wyścig: początkowe `Wake` sterownika mogło wczytać już utworzony
    // przebieg z bazy i uruchomić go, zanim dotarło anulowanie (raz zawiodło
    // pod obciążeniem). Po uspokojeniu przebieg czeka w kolejce bez `Start`.
    settle(&handle);
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

/// Historia do granicy kroków: analiza zakończona, implementacja nie zaplanowana.
fn boundary_history(env: &Env, run: RunId) {
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
}

fn after_analysis_runner() -> Arc<ScriptedRunner> {
    Arc::new(
        ScriptedRunner::new()
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    )
}

/// Kryterium E3 zmienione świadomie decyzją po E7 (PLAN_WDROZENIA.md, 15,
/// „2026-11-06 — decyzje po E7”): przebieg na granicy kroków po restarcie nie
/// rusza sam. Rekonsyliacja hosta (`recovery::reconcile_at_startup`) wstrzymuje go
/// (`Input::PauseAfterRestart`, tu odtworzone przez `apply_inputs`), sterownik po
/// starcie nie uruchamia żadnego agenta, a dopiero `resume` kontynuuje graf od
/// następnego kroku dokładnie raz; kolejny restart niczego nie powtarza.
#[test]
fn restart_at_a_step_boundary_waits_paused_until_resumed_once() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    assert_eq!(runner.total_calls(), 0, "wstrzymany przebieg nie rusza sam");
    assert_eq!(env.state(run).status, RunStatus::Paused);
    handle.wake();
    settle(&handle);
    assert_eq!(runner.total_calls(), 0, "Wake nie wznawia wstrzymanego");
    assert_eq!(handle.resume(run, "w1"), Ok(RunStatus::Running));
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
fn double_resume_with_the_same_request_id_launches_one_step() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    let first = handle.resume(run, "w1");
    let second = handle.resume(run, "w1");
    assert!(first.is_ok(), "{first:?}");
    assert!(second.is_ok(), "{second:?}");
    settle(&handle);
    assert_eq!(runner.calls("implementation"), 1);
    assert_eq!(env.state(run).resume_requests, ["w1"]);
    // Inny identyfikator po wznowieniu jest odrzucany.
    assert_eq!(handle.resume(run, "w2"), Err(ResumeError::NotPaused));
}

#[test]
fn resume_with_a_broken_workspace_fails_the_run_without_runner() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    env.workspace
        .set_verify_error(Some("Katalog roboczy nie jest worktree przebiegu."));
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    assert_eq!(handle.resume(run, "w1"), Ok(RunStatus::Failed));
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.failure, Some(FailureCode::WorkspaceUnavailable));
    assert_eq!(runner.total_calls(), 0);
}

/// Błąd samego sprawdzenia katalogu (np. baza) nie kończy przebiegu: wznowienie
/// jest odrzucone bez zdarzeń, a ponowienie z tym samym id po naprawie wznawia.
#[test]
fn verify_error_refuses_resume_without_events_and_retry_resumes() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    env.workspace.set_verify_failure(Some("baza niedostępna"));
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    let before = env.state(run).event_count;
    assert!(matches!(
        handle.resume(run, "w1"),
        Err(ResumeError::Storage(m)) if m.contains("baza niedostępna")
    ));
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Paused);
    assert_eq!(state.event_count, before, "bez zdarzeń");
    assert_eq!(runner.total_calls(), 0);
    env.workspace.set_verify_failure(None);
    assert_eq!(handle.resume(run, "w1"), Ok(RunStatus::Running));
    settle(&handle);
    assert_eq!(runner.calls("implementation"), 1);
}

/// Zatrzymany sterownik (wyjście z aplikacji) odrzuca wznowienie czytelnym
/// komunikatem, a przebieg zostaje wstrzymany do następnego uruchomienia.
#[test]
fn resume_after_graceful_shutdown_is_refused() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    handle.shutdown_graceful(Duration::from_secs(2));
    let err = handle.resume(run, "w1").unwrap_err();
    assert_eq!(err, ResumeError::ShuttingDown);
    assert_eq!(
        err.to_string(),
        "Aplikacja się zamyka — wznów przebieg po ponownym uruchomieniu."
    );
    assert_eq!(env.state(run).status, RunStatus::Paused);
    assert_eq!(runner.total_calls(), 0);
}

#[test]
fn cancel_of_a_paused_run_ends_cancelled_without_runner() {
    let env = Env::new();
    let run = env.create("r1");
    boundary_history(&env, run);
    apply_inputs(&*env.repo, run, vec![Input::PauseAfterRestart]).unwrap();
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    settle(&handle);
    handle.cancel(run);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Cancelled);
    assert_eq!(runner.total_calls(), 0);
    assert_eq!(handle.resume(run, "w1"), Err(ResumeError::NotPaused));
}

#[test]
fn resume_of_an_unknown_run_is_reported() {
    let env = Env::new();
    let runner = after_analysis_runner();
    let handle = env.run(&runner, config());
    assert!(matches!(
        handle.resume(999, "w1"),
        Err(ResumeError::Storage(_)) | Err(ResumeError::NotFound)
    ));
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
        comment: (decision == Decision::Reject).then(|| "Do poprawy".to_owned()),
    };
    // Nieaktualna rewizja: decyzja odrzucona jako nieaktualna, przebieg dalej czeka.
    assert_eq!(
        handle.decide(request(Decision::Approve, "d1", state.event_count + 7)),
        Err(DecideError::Stale)
    );
    assert_eq!(env.state(run).status, RunStatus::WaitingForApproval);
    assert_eq!(env.state(run).event_count, state.event_count);
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
            comment: None,
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
/// Limit dotyczy też zdrowych kroków drugiego przebiegu: 300 ms raz na ok. 35
/// przebiegów pakietu (przy równoległym pakiecie testów z Gitem) nie wystarczało
/// na zwykły krok i drugi przebieg kończył się `Timeout`. 1,5 s daje zapas, a krok
/// uparty (4 s) nadal przekracza limit razem z czasem łaski.
fn unresponsive_config() -> DriverConfig {
    DriverConfig {
        step_timeout: |_| Duration::from_millis(1_500),
        cancel_grace: Duration::from_millis(500),
        ..quiet()
    }
}

/// Dłużej niż limit kroku i czas łaski razem (`unresponsive_config`).
const STUBBORN: Duration = Duration::from_millis(4_000);

#[test]
fn unresponsive_runner_after_timeout_is_not_recorded_as_a_confirmed_end() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script("analysis", vec![Step::stubborn(STUBBORN)]));
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
                    Step::stubborn(STUBBORN),
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

#[test]
fn runner_receives_the_role_of_the_node() {
    use std::sync::Mutex;
    let env = Env::new();
    let seen: Arc<Mutex<Vec<(String, workflow_core::AgentRole)>>> = Arc::default();
    let sink = Arc::clone(&seen);
    let runner = Arc::new(
        ScriptedRunner::new()
            .template_script(vec![samples::review_approved()])
            .with_probe(move |request| {
                sink.lock()
                    .expect("blokada")
                    .push((request.node_id.clone(), request.role));
            }),
    );
    let handle = env.run(&runner, config());
    let run = env.create("roles");
    handle.start_run(run);
    settle(&handle);
    let seen = seen.lock().expect("blokada").clone();
    use workflow_core::AgentRole::{Analysis, Implementation, Review};
    assert_eq!(
        seen,
        [
            ("analysis".to_owned(), Analysis),
            ("implementation".to_owned(), Implementation),
            ("review".to_owned(), Review)
        ]
    );
}

#[test]
fn step_request_carries_the_run_goal() {
    use std::sync::Mutex;
    let env = Env::new();
    let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
    let sink = Arc::clone(&seen);
    let runner = Arc::new(
        ScriptedRunner::new()
            .template_script(vec![samples::review_approved()])
            .with_probe(move |request| {
                sink.lock().expect("blokada").push(request.goal.clone());
            }),
    );
    let handle = env.run(&runner, config());
    let run = env.repo.create_with_goal(
        &analysis_implementation_review(),
        "goal",
        "Napraw odejmowanie",
    );
    handle.start_run(run);
    settle(&handle);
    let seen = seen.lock().expect("blokada").clone();
    assert!(seen.len() >= 3);
    assert!(seen
        .iter()
        .all(|g| g.as_deref() == Some("Napraw odejmowanie")));
}

#[test]
fn decide_with_same_request_id_twice_records_one_resolution_with_comment() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("c1");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    let approval = state.approvals.last().unwrap().clone();
    let request = |revision| DecideRequest {
        run_id: run,
        approval_id: approval.id.clone(),
        decision: Decision::Reject,
        request_id: "same".into(),
        expected_revision: revision,
        artifact_hash: approval.artifact_hash.clone(),
        comment: Some("Do poprawy".into()),
    };
    let first = handle.decide(request(state.event_count));
    let again = handle.decide(request(0));
    assert_eq!(
        first.map(|r| r.approval_status),
        again.map(|r| r.approval_status)
    );
    settle(&handle);
    let after = env.state(run);
    assert_eq!(after.approvals.len(), 1);
    assert_eq!(after.approvals[0].comment.as_deref(), Some("Do poprawy"));
    assert_eq!(after.status, RunStatus::Rejected);
}

#[test]
fn comment_over_limit_is_rejected_without_events() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("c2");
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    let approval = state.approvals.last().unwrap().clone();
    let result = handle.decide(DecideRequest {
        run_id: run,
        approval_id: approval.id.clone(),
        decision: Decision::Reject,
        request_id: "long".into(),
        expected_revision: state.event_count,
        artifact_hash: approval.artifact_hash.clone(),
        comment: Some("x".repeat(2001)),
    });
    assert!(matches!(result, Err(DecideError::InvalidState(_))));
    assert_eq!(env.state(run).event_count, state.event_count);
}

fn waiting_run(env: &Env, handle: &RuntimeHandle, request: &str) -> (RunId, RunState) {
    let run = env.create(request);
    handle.start_run(run);
    settle(handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::WaitingForApproval);
    (run, state)
}

#[test]
fn reject_without_comment_is_rejected_by_the_driver_without_events() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let (run, state) = waiting_run(&env, &handle, "nc");
    let approval = state.approvals.last().unwrap().clone();
    for comment in [
        None,
        Some(String::new()),
        Some(
            "   
	"
            .to_owned(),
        ),
    ] {
        let result = handle.decide(DecideRequest {
            run_id: run,
            approval_id: approval.id.clone(),
            decision: Decision::Reject,
            request_id: "no-comment".into(),
            expected_revision: state.event_count,
            artifact_hash: approval.artifact_hash.clone(),
            comment,
        });
        assert!(matches!(result, Err(DecideError::InvalidState(_))));
    }
    let after = env.state(run);
    assert_eq!(after.event_count, state.event_count);
    assert_eq!(after.status, RunStatus::WaitingForApproval);
    assert_eq!(after.approvals[0].status, ApprovalStatus::Pending);
}

#[test]
fn comment_is_trimmed_in_the_core() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let (run, state) = waiting_run(&env, &handle, "trim");
    let approval = state.approvals.last().unwrap().clone();
    handle
        .decide(DecideRequest {
            run_id: run,
            approval_id: approval.id.clone(),
            decision: Decision::Approve,
            request_id: "trim-1".into(),
            expected_revision: state.event_count,
            artifact_hash: approval.artifact_hash.clone(),
            comment: Some("  Dobrze  ".into()),
        })
        .expect("decyzja");
    settle(&handle);
    assert_eq!(
        env.state(run).approvals[0].comment.as_deref(),
        Some("Dobrze")
    );
}

#[test]
fn stale_revision_is_reported_as_stale_and_the_run_keeps_waiting() {
    let env = Env::new();
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let (run, state) = waiting_run(&env, &handle, "stale");
    let approval = state.approvals.last().unwrap().clone();
    let result = handle.decide(DecideRequest {
        run_id: run,
        approval_id: approval.id.clone(),
        decision: Decision::Approve,
        request_id: "stale-1".into(),
        expected_revision: state.event_count - 1,
        artifact_hash: approval.artifact_hash.clone(),
        comment: None,
    });
    assert_eq!(result, Err(DecideError::Stale));
    let after = env.state(run);
    assert_eq!(after.status, RunStatus::WaitingForApproval);
    assert_eq!(after.event_count, state.event_count);
    // Ten sam request_id z aktualną rewizją przechodzi (nic nie zostało zapisane).
    assert!(handle
        .decide(DecideRequest {
            expected_revision: state.event_count,
            ..DecideRequest {
                run_id: run,
                approval_id: approval.id.clone(),
                decision: Decision::Approve,
                request_id: "stale-1".into(),
                expected_revision: 0,
                artifact_hash: approval.artifact_hash.clone(),
                comment: None,
            }
        })
        .is_ok());
}

/// Repozytorium, którego pierwsze `active_runs` (początkowe `Wake` sterownika)
/// czeka na sygnał testu; pozwala ustawić anulowanie w kanale, zanim przebieg
/// trafi do kolejki. Opcjonalnie `run_goal` zawodzi błędem bazy.
struct Gated {
    inner: Arc<MemoryRepository>,
    entered: std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>,
    release: std::sync::Mutex<Option<std::sync::mpsc::Receiver<()>>>,
    goal_fails: bool,
}

impl Gated {
    fn new(
        inner: Arc<MemoryRepository>,
    ) -> (
        Self,
        std::sync::mpsc::Receiver<()>,
        std::sync::mpsc::Sender<()>,
    ) {
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let gated = Self {
            inner,
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(Some(release_rx)),
            goal_fails: false,
        };
        (gated, entered_rx, release_tx)
    }

    fn failing_goal(inner: Arc<MemoryRepository>) -> Self {
        Self {
            inner,
            entered: std::sync::Mutex::new(None),
            release: std::sync::Mutex::new(None),
            goal_fails: true,
        }
    }
}

impl RunRepository for Gated {
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
        let runs = self.inner.active_runs();
        if let Some(entered) = self.entered.lock().unwrap().take() {
            let _ = entered.send(());
        }
        if let Some(release) = self.release.lock().unwrap().take() {
            let _ = release.recv_timeout(WAIT);
        }
        runs
    }
    fn run_goal(&self, run_id: RunId) -> Result<Option<String>, RepoError> {
        if self.goal_fails {
            return Err(RepoError::Storage("baza zablokowana".into()));
        }
        self.inner.run_goal(run_id)
    }
}

/// Rozstrzygnięcie wyścigu z E4 (`cancelling_a_queued_run_never_calls_the_runner`):
/// przebieg utworzony przed startem sterownika trafia do kolejki przez początkowe
/// `Wake`. Anulowanie, które jest już w kanale, gdy sterownik wybiera przebieg z
/// kolejki, jest rozliczane wcześniej: przebieg kończy się `Cancelled` bez runnera.
/// (Anulowanie wysłane dopiero po zdjęciu przebiegu z kolejki dotyczy przebiegu
/// `Running` i zatrzymuje uruchomiony krok; to zgodne z 5.2.)
#[test]
fn cancel_of_queued_run_is_processed_before_dequeue() {
    let env = Env::new();
    let run = env.create("q1");
    let (gated, entered, release) = Gated::new(env.repo.clone());
    let runner = Arc::new(ScriptedRunner::new().template_script(vec![samples::review_approved()]));
    let handle = env.run_on(Arc::new(gated), &runner, config());
    // Sterownik jest w `Wake` i już widzi przebieg Queued, ale jeszcze go nie wziął.
    entered.recv_timeout(WAIT).expect("początkowe Wake");
    handle.cancel(run);
    release.send(()).unwrap();
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Cancelled);
    assert_eq!(runner.total_calls(), 0, "runner nie może wystartować");
}

static GOAL_ERRORS: AtomicUsize = AtomicUsize::new(0);

/// Błąd bazy przy odczycie celu nie jest połykany: krok kończy się `StorageError`
/// bez startu agenta, a błąd trafia do `on_error`.
#[test]
fn goal_read_error_fails_the_step_without_starting_the_agent() {
    let env = Env::new();
    let run = env.create("goal-err");
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run_on(
        Arc::new(Gated::failing_goal(env.repo.clone())),
        &runner,
        DriverConfig {
            on_error: |m| {
                assert!(m.contains("cel przebiegu"), "{m}");
                GOAL_ERRORS.fetch_add(1, Ordering::SeqCst);
            },
            ..config()
        },
    );
    handle.start_run(run);
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Failed);
    assert_eq!(state.executions[0].status, ExecutionStatus::Failed);
    assert_eq!(state.executions[0].code, Some(FailureCode::StorageError));
    assert_eq!(runner.total_calls(), 0, "agent bez celu nie startuje");
    assert!(GOAL_ERRORS.load(Ordering::SeqCst) >= 1);
}

// ---- E6: odzyskiwanie po restarcie i kontrolowane zatrzymanie ----

use workflow_core::driver::ShutdownReport;
use workflow_core::ports::RunnerOutcome;

fn completed(output: serde_json::Value) -> RunnerOutcome {
    RunnerOutcome::Completed {
        output,
        exit_code: 0,
        session_id: None,
    }
}

/// Historia do kroku zgody bez przygotowanej zgody (awaria tuż po `ExecutionPlanned`).
fn history_to_unprepared_approval(env: &Env, run: RunId) -> Vec<Effect> {
    let outputs = [
        samples::analysis(),
        samples::implementation(),
        samples::review_approved(),
    ];
    apply_inputs(&*env.repo, run, vec![Input::Start]).unwrap();
    for output in outputs {
        let effects = apply_inputs(&*env.repo, run, vec![Input::Continue]).unwrap();
        let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
            panic!("brak LaunchAgent");
        };
        apply_inputs(
            &*env.repo,
            run,
            vec![Input::ExecutionResult {
                execution_id,
                outcome: completed(output),
            }],
        )
        .unwrap();
    }
    apply_inputs(&*env.repo, run, vec![Input::Continue]).unwrap()
}

#[test]
fn restart_with_cancelling_run_without_step_ends_cancelled_without_runner() {
    let env = Env::new();
    let run = env.create("r1");
    let effects = apply_inputs(&*env.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    apply_inputs(&*env.repo, run, vec![Input::CancelRequested]).unwrap();
    let state = env.state(run);
    env.repo
        .append(
            run,
            state.next_sequence(),
            &[RunEvent::ExecutionFinished {
                execution_id,
                status: ExecutionStatus::Cancelled,
                output: None,
                code: Some(FailureCode::Cancelled),
                message: None,
                exit_code: None,
                session_id: None,
            }],
            0,
        )
        .unwrap();
    assert_eq!(env.state(run).status, RunStatus::Cancelling);
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::Cancelled);
    assert_eq!(runner.total_calls(), 0);
}

#[test]
fn restart_with_unprepared_approval_prepares_it_and_never_calls_runner() {
    let env = Env::new();
    let run = env.create("r1");
    let effects = history_to_unprepared_approval(&env, run);
    assert!(matches!(
        effects.first(),
        Some(Effect::RequestApproval { .. })
    ));
    assert_eq!(env.state(run).status, RunStatus::Running);
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::WaitingForApproval);
    assert_eq!(state.approvals.len(), 1);
    assert_eq!(state.approvals[0].artifact_hash, "h1");
    assert_eq!(runner.total_calls(), 0);
    let result = env
        .decide(&handle, run, Decision::Approve, "d1")
        .expect("decyzja po restarcie");
    assert_eq!(result.approval_status, ApprovalStatus::Approved);
}

fn waiting_run_e6(env: &Env) -> (RunId, Arc<ScriptedRunner>) {
    let runner = runner_for(vec![samples::review_approved()]);
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    settle(&handle);
    assert_eq!(env.state(run).status, RunStatus::WaitingForApproval);
    drop(handle);
    (run, runner)
}

#[test]
fn waiting_approval_is_revalidated_once_on_start_and_unchanged_hash_keeps_ids() {
    let env = Env::new();
    let (run, runner) = waiting_run_e6(&env);
    let before = env.state(run);
    let handle = env.run(&runner, config());
    settle(&handle);
    let after = env.state(run);
    assert_eq!(after.event_count, before.event_count);
    assert_eq!(after.approvals, before.approvals);
    assert!(env.sink.faults().is_empty());
    assert!(env.decide(&handle, run, Decision::Approve, "d1").is_ok());
}

#[test]
fn changed_artifact_on_start_invalidates_and_requests_new_approval() {
    let env = Env::new();
    let (run, runner) = waiting_run_e6(&env);
    let old_id = env.state(run).approvals[0].id.clone();
    env.workspace.set_hash("h2");
    let handle = env.run(&runner, config());
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::WaitingForApproval);
    assert_eq!(state.approvals.len(), 2);
    assert_eq!(state.approvals[0].status, ApprovalStatus::Invalidated);
    assert_eq!(state.approvals[1].status, ApprovalStatus::Pending);
    assert_eq!(state.approvals[1].artifact_hash, "h2");
    assert_ne!(state.approvals[1].id, old_id);
}

#[test]
fn missing_artifact_on_start_reports_fault_and_keeps_waiting() {
    let env = Env::new();
    let (run, runner) = waiting_run_e6(&env);
    let before = env.state(run);
    env.workspace.fail_next(true);
    let handle = env.run(&runner, config());
    settle(&handle);
    let after = env.state(run);
    assert_eq!(after.status, RunStatus::WaitingForApproval);
    assert_eq!(after.event_count, before.event_count);
    let faults = env.sink.faults();
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].0, run);
}

#[test]
fn graceful_shutdown_cancels_step_waits_and_records_interrupted() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script("analysis", vec![Step::hang()]));
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("start kroku", || runner.calls("analysis") == 1);
    let report = handle.shutdown_graceful(Duration::from_secs(5));
    assert_eq!(
        report,
        ShutdownReport {
            interrupted: vec![run],
            unconfirmed: None,
            timed_out: false,
        }
    );
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Interrupted);
    assert_eq!(state.failure, Some(FailureCode::Interrupted));
    assert!(runner.saw_cancel("analysis"));
    assert_eq!(runner.total_calls(), 1);
    // Po restarcie nic nie rusza.
    let fresh = Arc::new(ScriptedRunner::new());
    let handle = env.run(&fresh, config());
    settle(&handle);
    assert_eq!(fresh.total_calls(), 0);
    assert_eq!(env.state(run).status, RunStatus::Interrupted);
}

#[test]
fn graceful_shutdown_with_stubborn_runner_returns_after_limit_and_reports_unconfirmed() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new().script(
        "analysis",
        vec![Step::stubborn(Duration::from_millis(1_500))],
    ));
    let handle = env.run(&runner, config());
    let run = env.create("r1");
    handle.start_run(run);
    wait_until("start kroku", || runner.calls("analysis") == 1);
    let started = Instant::now();
    let report = handle.shutdown_graceful(Duration::from_millis(100));
    assert!(started.elapsed() < Duration::from_millis(1_200));
    assert_eq!(report.unconfirmed, Some(run));
    assert!(report.interrupted.is_empty());
    // Nic nie zapisano: krok nadal bez wyniku, więc po restarcie Recover da Interrupted.
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Running);
    assert!(state.unfinished().is_some());
    assert_eq!(runner.total_calls(), 1);
}

#[test]
fn graceful_shutdown_when_idle_returns_immediately() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    settle(&handle);
    assert!(!handle.is_stopped());
    let started = Instant::now();
    let report = handle.shutdown_graceful(Duration::from_secs(5));
    assert_eq!(report, ShutdownReport::default());
    assert!(started.elapsed() < Duration::from_secs(2));
    // Po zatrzymaniu host widzi to i odrzuca nowe przebiegi (runtime_handle).
    assert!(handle.is_stopped());
}

/// Kryterium 3 E6: anulowanie z krokiem w toku, a potem awaria hosta. Po restarcie
/// krok i przebieg są `Interrupted` (koniec procesu nie jest potwierdzony), a runner
/// nie jest wołany ponownie.
#[test]
fn restart_during_cancel_with_step_ends_interrupted() {
    let env = Env::new();
    let run = env.create("r1");
    apply_inputs(&*env.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    apply_inputs(&*env.repo, run, vec![Input::CancelRequested]).unwrap();
    let before = env.state(run);
    assert_eq!(before.status, RunStatus::Cancelling);
    assert!(before.unfinished().is_some(), "krok nadal w toku");
    let runner = Arc::new(ScriptedRunner::new());
    let handle = env.run(&runner, config());
    settle(&handle);
    let state = env.state(run);
    assert_eq!(state.status, RunStatus::Interrupted);
    assert_eq!(state.executions[0].status, ExecutionStatus::Interrupted);
    assert_eq!(runner.total_calls(), 0);
}

/// Repozytorium, którego `active_runs` wisi: wątek roboczy nie odpowie na
/// kontrolowane zatrzymanie w limicie.
struct HangingRepo {
    inner: Arc<MemoryRepository>,
    hang: Duration,
}

impl RunRepository for HangingRepo {
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
        std::thread::sleep(self.hang);
        self.inner.active_runs()
    }
}

/// Uwaga 17: brak odpowiedzi wątku roboczego nie udaje „nic nie było w toku”.
#[test]
fn graceful_shutdown_without_worker_reply_reports_timed_out() {
    let env = Env::new();
    let runner = Arc::new(ScriptedRunner::new());
    let repo = Arc::new(HangingRepo {
        inner: env.repo.clone(),
        hang: Duration::from_secs(8),
    });
    let handle = env.run_on(repo, &runner, config());
    let started = Instant::now();
    let report = handle.shutdown_graceful(Duration::from_millis(50));
    assert!(report.timed_out, "{report:?}");
    assert!(report.interrupted.is_empty());
    assert!(started.elapsed() < Duration::from_secs(7));
    // Drugie wywołanie nie czeka ponownie.
    let again = Instant::now();
    let _ = handle.shutdown_graceful(Duration::from_millis(50));
    assert!(again.elapsed() < Duration::from_secs(1));
}
