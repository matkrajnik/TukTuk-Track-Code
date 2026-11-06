//! Rekonsyliacja po starcie i `workflow_recover` na SQLite, z `ScriptedRunner`
//! i atrapą procesów systemu (PLAN_WDROZENIA.md, 11: Recovery).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use agent_cli::{KillResult, Probe, ProcessIdentity};
use workflow_core::driver::{DriverConfig, Runtime, RuntimeHandle};
use workflow_core::engine::{Effect, Input};
use workflow_core::model::AgentConfig;
use workflow_core::ports::RunnerOutcome;
use workflow_core::testing::{
    apply_inputs, samples, FixedWorkspace, RecordingSink, ScriptedRunner, Step,
};

use super::*;
use crate::workflows::processes::fake::FakeProcesses;
use crate::workflows::processes::{ProcessLedger, SqliteLedger};
use crate::workflows::test_db::TestDb;

const WAIT: Duration = Duration::from_secs(10);
const PID: u32 = 4242;

fn identity() -> ProcessIdentity {
    ProcessIdentity {
        pid: PID,
        created_ft: 99,
        job_attached: true,
    }
}

fn runtime(t: &TestDb, runner: &Arc<ScriptedRunner>) -> RuntimeHandle {
    Runtime::start(
        Arc::new(t.repo.clone()),
        runner.clone(),
        Arc::new(RecordingSink::new()),
        Arc::new(FixedWorkspace::new("h1")),
        DriverConfig {
            step_timeout: |_: &AgentConfig| Duration::from_secs(20),
            cancel_grace: Duration::from_millis(300),
            poll_interval: Duration::from_millis(5),
            now: || 1_000,
            on_error: |m| panic!("błąd sterownika: {m}"),
        },
    )
}

fn settle_runtime(handle: &RuntimeHandle) {
    assert!(handle.wait_idle(WAIT), "sterownik nie uspokoił się");
}

fn reconcile(t: &TestDb, control: &FakeProcesses) -> StartupReport {
    reconcile_at_startup(&t.db, control, &|_| {}, 500).unwrap()
}

/// Przebieg z krokiem analizy w toku (intencja zapisana, brak wyniku).
fn launched(t: &TestDb, request: &str) -> (RunId, String) {
    let run = t.create(request);
    t.reserve(run, &format!("C:/wt/{request}"));
    let effects = apply_inputs(&t.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    (run, execution_id)
}

fn record(t: &TestDb, run: RunId, execution_id: &str) -> SqliteLedger {
    let ledger = SqliteLedger::new(t.db.clone(), || 5);
    ledger.spawned(run, execution_id, &identity()).unwrap();
    ledger
}

fn assert_interrupted_without_rerun(t: &TestDb, run: RunId) {
    let runner = Arc::new(ScriptedRunner::new());
    let handle = runtime(t, &runner);
    settle_runtime(&handle);
    assert_eq!(runner.total_calls(), 0, "krok nie może ruszyć ponownie");
    assert_eq!(t.status(run), RunStatus::Interrupted);
    let status: String = t.scalar(
        "SELECT status FROM workflow_node_executions WHERE run_id = ?1",
        run,
    );
    assert_eq!(status, "interrupted");
}

// ---------------------------------------------------------------------------
// Kryterium 1: awaria w trakcie kroku

#[test]
fn crash_before_spawn_marks_interrupted_and_never_calls_runner() {
    let t = TestDb::new();
    let (run, _) = launched(&t, "r1");
    let fake = FakeProcesses::default();
    let report = reconcile(&t, &fake);
    assert_eq!(report, StartupReport::default());
    assert_interrupted_without_rerun(&t, run);
    assert!(fake.killed().is_empty());
}

#[test]
fn crash_after_spawn_before_ledger_write_is_interrupted_and_keeps_reservation() {
    let t = TestDb::new();
    let (run, _) = launched(&t, "r1");
    // Proces mógł wystartować, ale wpisu w rejestrze nie ma: niepewny stan.
    reconcile(&t, &FakeProcesses::default());
    assert_interrupted_without_rerun(&t, run);
    assert_eq!(t.reservation(run), ("reserved".to_owned(), None));
}

#[test]
fn crash_after_exit_before_commit_is_interrupted_not_rerun() {
    let t = TestDb::new();
    let (run, execution_id) = launched(&t, "r1");
    let ledger = record(&t, run, &execution_id);
    ledger
        .ended(
            &execution_id,
            crate::workflows::processes::ProcState::Exited,
        )
        .unwrap();
    let fake = FakeProcesses::with(PID, Probe::SameAlive, KillResult::Confirmed);
    reconcile(&t, &fake);
    assert!(
        fake.killed().is_empty(),
        "zakończonego procesu nie sprawdzamy"
    );
    assert_interrupted_without_rerun(&t, run);
    let executions = t.scalar::<i64>(
        "SELECT COUNT(*) FROM workflow_node_executions WHERE run_id = ?1",
        run,
    );
    assert_eq!(executions, 1, "wynik nie jest odtwarzany drugim krokiem");
}

// ---------------------------------------------------------------------------
// Kryterium 5: rejestr procesów i ponowne użycie PID

#[test]
fn reused_pid_is_not_killed_and_marked_exited() {
    let t = TestDb::new();
    let (run, execution_id) = launched(&t, "r1");
    record(&t, run, &execution_id);
    let fake = FakeProcesses::with(PID, Probe::Reused, KillResult::Confirmed);
    let report = reconcile(&t, &fake);
    assert!(
        fake.killed().is_empty(),
        "cudzy proces nie może zostać zabity"
    );
    assert_eq!(report.killed, 0);
    assert_eq!(t.process_state(&execution_id), "exited");
    assert_interrupted_without_rerun(&t, run);
}

#[test]
fn alive_same_process_is_killed_before_driver_starts() {
    let t = TestDb::new();
    let (run, execution_id) = launched(&t, "r1");
    record(&t, run, &execution_id);
    let fake = FakeProcesses::with(PID, Probe::SameAlive, KillResult::Confirmed);
    let report = reconcile(&t, &fake);
    // Przed startem sterownika: proces już zakończony, przebieg jeszcze nie ruszony.
    assert_eq!(fake.killed(), [PID]);
    assert_eq!(report.killed, 1);
    assert_eq!(t.process_state(&execution_id), "killed");
    assert_eq!(t.status(run), RunStatus::Running);
    assert_interrupted_without_rerun(&t, run);
}

#[test]
fn unconfirmed_kill_marks_unknown_and_keeps_reservation() {
    let t = TestDb::new();
    let (run, execution_id) = launched(&t, "r1");
    record(&t, run, &execution_id);
    let fake = FakeProcesses::with(PID, Probe::SameAlive, KillResult::Unconfirmed);
    let report = reconcile(&t, &fake);
    assert_eq!(report.unknown, 1);
    assert_eq!(report.unknown_processes, [(run, PID)]);
    // Interfejs dostaje błąd z PID przez FAULT_EVENT.
    let faults = startup_faults(&report, &TmpReport::default());
    assert_eq!(faults.len(), 1);
    assert_eq!(faults[0].0, run);
    assert!(faults[0].1.contains("PID 4242"), "{}", faults[0].1);
    assert_eq!(t.process_state(&execution_id), "unknown");
    assert_interrupted_without_rerun(&t, run);
    assert_eq!(t.reservation(run), ("reserved".to_owned(), None));
    // Odzyskanie też nie zwalnia katalogu, dopóki koniec nie jest potwierdzony.
    let outcome = recover(&t.db, &fake, &|_| {}, run, 600).unwrap();
    assert!(!outcome.processes_confirmed && !outcome.workspace_released);
    assert_eq!(t.reservation(run), ("reserved".to_owned(), None));
}

// ---------------------------------------------------------------------------
// Kryterium 6: rezerwacje pozostawione przez E5

#[test]
fn terminal_run_reservation_left_by_e5_is_released() {
    let t = TestDb::new();
    let done = t.create("done");
    t.reserve(done, "C:/wt/done");
    // Stan końcowy zapisany bez zwolnienia (zachowanie sprzed E6).
    t.db.lock()
        .unwrap()
        .execute(
            "UPDATE workflow_runs SET status = 'succeeded' WHERE id = ?1",
            [done],
        )
        .unwrap();
    let interrupted = t.create("int");
    t.reserve(interrupted, "C:/wt/int");
    apply_inputs(
        &t.repo,
        interrupted,
        vec![Input::Abandon {
            message: "x".into(),
        }],
    )
    .unwrap();
    let report = reconcile(&t, &FakeProcesses::default());
    assert_eq!(report.released, 1);
    assert_eq!(
        t.reservation(done),
        ("released".to_owned(), Some("terminal".to_owned()))
    );
    assert_eq!(t.reservation(interrupted), ("reserved".to_owned(), None));
}

// ---------------------------------------------------------------------------
// Kryterium 4: kolejka bez rezerwacji i granica kroków z naruszonym worktree

#[test]
fn queued_run_without_workspace_fails_workspace_unavailable() {
    let t = TestDb::new();
    let run = t.create("r1");
    let report = reconcile(&t, &FakeProcesses::default());
    assert_eq!(report.failed_runs, [run]);
    assert_eq!(t.status(run), RunStatus::Failed);
    let code: String = t.scalar("SELECT error_code FROM workflow_runs WHERE id = ?1", run);
    assert_eq!(code, "workspaceUnavailable");
    let runner = Arc::new(ScriptedRunner::new());
    let handle = runtime(&t, &runner);
    settle_runtime(&handle);
    assert_eq!(runner.total_calls(), 0);
}

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct GitSandbox {
    root: PathBuf,
    _permit: crate::workflows::test_git::GitPermit,
}

impl GitSandbox {
    fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root =
            std::env::temp_dir().join(format!("tuktuk recovery zolc {}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self {
            root,
            _permit: crate::workflows::test_git::GitPermit::acquire(),
        }
    }

    fn repo(&self) -> PathBuf {
        let dir = self.root.join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        let g = |args: &[&str]| {
            let mut full = vec!["-c", "user.name=t", "-c", "user.email=t@example.com"];
            full.extend_from_slice(args);
            crate::workflows::workspace::git(&dir, &full).unwrap();
        };
        g(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "a\n").unwrap();
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "start"]);
        dir
    }
}

impl Drop for GitSandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn at_boundary(t: &TestDb, run: RunId) {
    let effects = apply_inputs(&t.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    apply_inputs(
        &t.repo,
        run,
        vec![Input::ExecutionResult {
            execution_id,
            outcome: RunnerOutcome::Completed {
                output: samples::analysis(),
                exit_code: 0,
                session_id: None,
            },
        }],
    )
    .unwrap();
}

#[test]
fn boundary_run_with_tampered_gitfile_fails_instead_of_launching() {
    let sandbox = GitSandbox::new();
    let source = sandbox.repo();
    let app_data = sandbox.root.join("app data");
    let t = TestDb::new();
    let tampered = t.create("tampered");
    let intact = t.create("intact");
    for run in [tampered, intact] {
        crate::workflows::workspace::prepare_workspace(&t.db, &source, &app_data, run, 1).unwrap();
        at_boundary(&t, run);
    }
    let worktree: String = t.scalar(
        "SELECT worktree_path FROM workflow_workspaces WHERE run_id = ?1",
        tampered,
    );
    // Agent podmienił `.git`: Git czytałby konfigurację spoza rezerwacji.
    std::fs::write(
        PathBuf::from(&worktree).join(".git"),
        format!("gitdir: {}", sandbox.root.join("obcy").display()),
    )
    .unwrap();
    let report = reconcile(&t, &FakeProcesses::default());
    assert_eq!(report.failed_runs, [tampered], "{:?}", report.errors);
    assert_eq!(t.status(tampered), RunStatus::Failed);
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    );
    let handle = runtime(&t, &runner);
    settle_runtime(&handle);
    // Tylko nienaruszony przebieg ruszył dalej.
    assert_eq!(runner.calls("implementation"), 1);
    assert_eq!(t.status(tampered), RunStatus::Failed);
    assert_eq!(t.status(intact), RunStatus::WaitingForApproval);
    drop(handle);
    for run in [tampered, intact] {
        let path: String = t.scalar(
            "SELECT worktree_path FROM workflow_workspaces WHERE run_id = ?1",
            run,
        );
        let _ =
            crate::workflows::workspace::git(&source, &["worktree", "remove", "--force", &path]);
    }
}

// ---------------------------------------------------------------------------
// Kryterium 7: workflow_recover

fn interrupted_with_process(t: &TestDb) -> (RunId, String) {
    let (run, execution_id) = launched(t, "r1");
    record(t, run, &execution_id);
    apply_inputs(&t.repo, run, vec![Input::Recover]).unwrap();
    assert_eq!(t.status(run), RunStatus::Interrupted);
    (run, execution_id)
}

fn last_event(t: &TestDb, run: RunId) -> StoredEvent {
    t.repo.load_events(run, 0).unwrap().pop().unwrap()
}

#[test]
fn recover_releases_workspace_after_confirmed_end_and_records_event() {
    let t = TestDb::new();
    let (run, execution_id) = interrupted_with_process(&t);
    let fake = FakeProcesses::default();
    let emitted = std::sync::Mutex::new(Vec::new());
    let outcome = recover(
        &t.db,
        &fake,
        &|e| emitted.lock().unwrap().push(e.kind.clone()),
        run,
        600,
    )
    .unwrap();
    assert_eq!(
        outcome,
        RecoverOutcome {
            processes_confirmed: true,
            workspace_released: true,
            already_recovered: false,
            unconfirmed_pids: Vec::new(),
        }
    );
    assert_eq!(
        t.reservation(run),
        ("released".to_owned(), Some("recovered".to_owned()))
    );
    assert_eq!(t.process_state(&execution_id), "exited");
    let event = last_event(&t, run);
    assert_eq!(event.kind, "recovery");
    assert_eq!(event.payload["processesConfirmed"], true);
    assert_eq!(event.payload["workspaceReleased"], true);
    assert_eq!(*emitted.lock().unwrap(), ["recovery"]);
    assert_eq!(
        t.status(run),
        RunStatus::Interrupted,
        "graf nie jest wznawiany"
    );
}

#[test]
fn recover_keeps_reservation_when_process_still_alive() {
    let t = TestDb::new();
    let (run, execution_id) = interrupted_with_process(&t);
    let fake = FakeProcesses::with(PID, Probe::SameAlive, KillResult::Unconfirmed);
    let outcome = recover(&t.db, &fake, &|_| {}, run, 600).unwrap();
    assert!(!outcome.processes_confirmed);
    assert!(!outcome.workspace_released);
    assert_eq!(outcome.unconfirmed_pids, [PID]);
    assert_eq!(fake.killed(), [PID]);
    assert_eq!(t.reservation(run), ("reserved".to_owned(), None));
    assert_eq!(t.process_state(&execution_id), "unknown");
    let event = last_event(&t, run);
    assert_eq!(event.kind, "recovery");
    assert_eq!(event.payload["processesConfirmed"], false);
    let note = event.payload["note"].as_str().expect("note");
    assert!(note.contains("PID 4242"), "{note}");
    assert!(
        note.starts_with("Nie potwierdzono końca procesu agenta"),
        "{note}"
    );
    let view = crate::workflows::runs::run_detail(&t.db, run, 1)
        .unwrap()
        .unwrap()
        .recovery
        .unwrap();
    assert_eq!(view.unconfirmed_pids, [PID]);
    // Gdy proces jednak się skończy, kolejne odzyskanie zwalnia katalog.
    fake.probes.lock().unwrap().insert(PID, Probe::Gone);
    let second = recover(&t.db, &fake, &|_| {}, run, 700).unwrap();
    assert!(second.processes_confirmed && second.workspace_released);
    assert_eq!(t.reservation(run).0, "released");
}

#[test]
fn recover_rejects_non_interrupted_run() {
    let t = TestDb::new();
    let queued = t.create("q");
    assert_eq!(
        recover(&t.db, &FakeProcesses::default(), &|_| {}, queued, 1).unwrap_err(),
        ERR_NOT_INTERRUPTED
    );
    assert!(recover(&t.db, &FakeProcesses::default(), &|_| {}, 999, 1).is_err());
    let (running, _) = launched(&t, "run");
    assert!(recover(&t.db, &FakeProcesses::default(), &|_| {}, running, 1).is_err());
}

#[test]
fn recover_twice_is_noop() {
    let t = TestDb::new();
    let (run, _) = interrupted_with_process(&t);
    let fake = FakeProcesses::default();
    recover(&t.db, &fake, &|_| {}, run, 600).unwrap();
    let count = t.repo.load_events(run, 0).unwrap().len();
    let again = recover(&t.db, &fake, &|_| {}, run, 700).unwrap();
    assert!(again.already_recovered);
    assert_eq!(t.repo.load_events(run, 0).unwrap().len(), count);
}

#[test]
fn recover_never_calls_runner() {
    let t = TestDb::new();
    let (run, _) = interrupted_with_process(&t);
    let runner = Arc::new(ScriptedRunner::new());
    let handle = runtime(&t, &runner);
    settle_runtime(&handle);
    recover(&t.db, &FakeProcesses::default(), &|_| {}, run, 600).unwrap();
    handle.wake();
    settle_runtime(&handle);
    assert_eq!(runner.total_calls(), 0);
    assert_eq!(t.status(run), RunStatus::Interrupted);
}

/// Szczegóły przerwanego przebiegu pokazują stan odzyskania (RunDetail.recovery).
#[test]
fn run_detail_shows_recovery_state_of_an_interrupted_run() {
    let t = TestDb::new();
    let (run, _) = interrupted_with_process(&t);
    let detail = crate::workflows::runs::run_detail(&t.db, run, 1)
        .unwrap()
        .unwrap();
    let view = detail.recovery.expect("recovery");
    assert!(view.workspace_reserved);
    assert_eq!(view.processes_unconfirmed, 1);
    assert_eq!(view.unconfirmed_pids, [PID]);
    assert_eq!(view.attempts, 0);
    recover(&t.db, &FakeProcesses::default(), &|_| {}, run, 600).unwrap();
    let view = crate::workflows::runs::run_detail(&t.db, run, 1)
        .unwrap()
        .unwrap()
        .recovery
        .unwrap();
    assert!(!view.workspace_reserved);
    assert_eq!(view.processes_unconfirmed, 0);
    assert!(view.unconfirmed_pids.is_empty());
    assert_eq!(view.attempts, 1);
    let queued = t.create("q");
    assert!(crate::workflows::runs::run_detail(&t.db, queued, 1)
        .unwrap()
        .unwrap()
        .recovery
        .is_none());
}

// ---------------------------------------------------------------------------
// Krok 7e: pliki tymczasowe, problemy przez FAULT_EVENT, brak pliku diffu

fn temp_app(name: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "tuktuk-wf-recovery-{name}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn orphan_tmp_artifact_is_removed_and_final_files_kept() {
    let app = temp_app("tmp");
    let run_dir = app.join("workflows/runs/7");
    std::fs::create_dir_all(&run_dir).unwrap();
    let hash = "b".repeat(64);
    let final_file = run_dir.join(format!("diff-{hash}.patch"));
    let orphan = run_dir.join(format!("diff-{hash}.123-1.tmp"));
    let other = run_dir.join("notatka.tmp");
    std::fs::write(
        &final_file,
        "+gotowy
",
    )
    .unwrap();
    std::fs::write(&orphan, "+urwany").unwrap();
    std::fs::write(&other, "x").unwrap();
    // Plik w katalogu runs (nie w katalogu przebiegu) też zostaje.
    std::fs::write(app.join("workflows/runs/diff-x.tmp"), "x").unwrap();
    let report = remove_orphan_tmp(&app);
    assert_eq!(
        report,
        TmpReport {
            removed: 1,
            errors: Vec::new()
        }
    );
    assert!(!orphan.exists());
    assert!(final_file.is_file() && other.is_file());
    assert!(app.join("workflows/runs/diff-x.tmp").is_file());
    // Bez katalogu artefaktów nic się nie dzieje.
    assert_eq!(remove_orphan_tmp(&app.join("brak")), TmpReport::default());
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn startup_faults_cover_failed_runs_and_errors_but_not_routine_cleanup() {
    let report = StartupReport {
        killed: 2,
        released: 3,
        failed_runs: vec![9],
        errors: vec!["Przebieg 4: x".into()],
        ..StartupReport::default()
    };
    let tmp = TmpReport {
        removed: 5,
        errors: vec!["plik".into()],
    };
    let faults = startup_faults(&report, &tmp);
    assert_eq!(faults.len(), 3);
    assert_eq!(faults[0].0, 9);
    assert!(faults[0].1.contains("katalog roboczy jest niedostępny"));
    assert_eq!((faults[1].0, faults[2].0), (0, 0));
    assert!(startup_faults(&StartupReport::default(), &TmpReport::default()).is_empty());
}

#[test]
fn missing_artifact_file_shows_artifact_unavailable_in_detail() {
    use crate::workflows::artifacts::{hex_of, rel_path, store_diff, with_file_availability};
    let t = TestDb::new();
    let app = temp_app("missing");
    let run = t.create("diff");
    let text = b"+zmiana
";
    let hash = hex_of(text);
    store_diff(&t.db, &app, run, text, &hash, 1).unwrap();
    t.db.lock()
        .unwrap()
        .execute(
            "INSERT INTO workflow_approvals(id, run_id, execution_id, artifact_hash, status)
             VALUES('a1', ?1, 'e1', ?2, 'pending')",
            rusqlite::params![run, hash],
        )
        .unwrap();
    let detail = || {
        crate::workflows::runs::run_detail(&t.db, run, 1)
            .unwrap()
            .unwrap()
    };
    let present = with_file_availability(detail(), &app);
    assert!(present.approvals[0].artifact_available);
    // Kopia zapasowa bez diffu albo usunięty plik: niedostępny, ale nie po retencji.
    std::fs::remove_file(app.join(rel_path(run, &hash))).unwrap();
    let missing = with_file_availability(detail(), &app);
    assert!(!missing.approvals[0].artifact_available);
    assert!(!missing.approvals[0].artifact_purged);
    let _ = std::fs::remove_dir_all(&app);
}
