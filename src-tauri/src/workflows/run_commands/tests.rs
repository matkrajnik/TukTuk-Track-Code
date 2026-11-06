//! Rdzeń komend przebiegów: dostęp, komentarz przy odrzuceniu, idempotencja decyzji.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use workflow_core::driver::{DriverConfig, Runtime, RuntimeHandle};
use workflow_core::model::AgentConfig;
use workflow_core::ports::NewRun;
use workflow_core::template::analysis_implementation_review;
use workflow_core::testing::{samples, FixedWorkspace, RecordingSink, ScriptedRunner};

use super::*;
use crate::workflows::store;

const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    db: SharedDb,
    handle: RuntimeHandle,
    run: i64,
}

impl Fixture {
    fn new(unlocked: bool) -> Self {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let id = store::create(&conn, "P", "", &analysis_implementation_review(), 1).unwrap();
        store::publish(&conn, id, 1, 2).unwrap();
        let version_id: i64 = conn
            .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
            .unwrap();
        if unlocked {
            store::set_unlocked(&conn).unwrap();
        }
        let db: SharedDb = Arc::new(Mutex::new(conn));
        let repo = SqliteRunRepository::new(db.clone());
        let run = workflow_core::ports::RunRepository::create_run(
            &repo,
            &NewRun {
                version_id,
                request_id: "r".into(),
                now: 10,
                ..NewRun::default()
            },
        )
        .unwrap()
        .run_id;
        let runner =
            Arc::new(ScriptedRunner::new().template_script(vec![samples::review_approved()]));
        let handle = Runtime::start(
            Arc::new(repo),
            runner,
            Arc::new(RecordingSink::new()),
            Arc::new(FixedWorkspace::new("h1")),
            DriverConfig {
                step_timeout: |_: &AgentConfig| Duration::from_secs(20),
                cancel_grace: Duration::from_secs(2),
                poll_interval: Duration::from_millis(5),
                now: || 1_000,
                on_error: |m| panic!("błąd sterownika: {m}"),
            },
        );
        Self { db, handle, run }
    }

    fn to_approval(&self) -> (String, String, u64) {
        self.handle.start_run(self.run);
        assert!(self.handle.wait_idle(WAIT));
        let conn = self.db.lock().unwrap();
        let (id, hash): (String, String) = conn
            .query_row(
                "SELECT id, artifact_hash FROM workflow_approvals WHERE status = 'pending'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let revision: i64 = conn
            .query_row(
                "SELECT MAX(sequence) FROM workflow_events WHERE run_id = ?1",
                [self.run],
                |r| r.get(0),
            )
            .unwrap();
        (id, hash, revision as u64)
    }

    fn command(
        &self,
        decision: Decision,
        request_id: &str,
        comment: Option<&str>,
    ) -> DecideCommand {
        let (approval_id, artifact_hash, expected_revision) = self.to_approval_cached();
        DecideCommand {
            run_id: self.run,
            approval_id,
            decision,
            request_id: request_id.into(),
            expected_revision,
            artifact_hash,
            comment: comment.map(str::to_owned),
        }
    }

    fn to_approval_cached(&self) -> (String, String, u64) {
        let conn = self.db.lock().unwrap();
        let (id, hash): (String, String) = conn
            .query_row(
                "SELECT id, artifact_hash FROM workflow_approvals ORDER BY rowid DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let revision: i64 = conn
            .query_row(
                "SELECT MAX(sequence) FROM workflow_events WHERE run_id = ?1",
                [self.run],
                |r| r.get(0),
            )
            .unwrap();
        (id, hash, revision as u64)
    }

    fn status(&self) -> String {
        self.db
            .lock()
            .unwrap()
            .query_row("SELECT status FROM workflow_runs", [], |r| r.get(0))
            .unwrap()
    }
}

#[test]
fn decide_requires_comment_on_reject() {
    let f = Fixture::new(true);
    f.to_approval();
    for comment in [None, Some(""), Some("   ")] {
        let err = decide(&f.db, &f.handle, f.command(Decision::Reject, "r1", comment)).unwrap_err();
        assert!(err.contains("komentarza"), "{err}");
    }
    assert_eq!(f.status(), "waitingForApproval");
    let ok = decide(
        &f.db,
        &f.handle,
        f.command(Decision::Reject, "r2", Some("  Do poprawy  ")),
    )
    .unwrap();
    assert_eq!(ok.approval_status, ApprovalStatus::Rejected);
    let stored: Option<String> =
        f.db.lock()
            .unwrap()
            .query_row("SELECT comment FROM workflow_approvals", [], |r| r.get(0))
            .unwrap();
    assert_eq!(stored.as_deref(), Some("Do poprawy"));
}

#[test]
fn double_decide_same_request_id_has_one_effect() {
    let f = Fixture::new(true);
    f.to_approval();
    let command = f.command(Decision::Approve, "same", None);
    let first = decide(&f.db, &f.handle, command.clone()).unwrap();
    let second = decide(&f.db, &f.handle, command).unwrap();
    assert_eq!(first.approval_status, second.approval_status);
    assert!(f.handle.wait_idle(WAIT));
    let resolved: i64 =
        f.db.lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM workflow_events WHERE type = 'approvalResolved'",
                [],
                |r| r.get(0),
            )
            .unwrap();
    assert_eq!(resolved, 1);
    assert_eq!(f.status(), "succeeded");
}

#[test]
fn decide_with_stale_revision_is_reported_as_invalidated() {
    let f = Fixture::new(true);
    f.to_approval();
    let mut command = f.command(Decision::Approve, "stale", None);
    command.expected_revision -= 1;
    let err = decide(&f.db, &f.handle, command).unwrap_err();
    assert!(err.starts_with(INVALIDATED_PREFIX), "{err}");
    assert_eq!(f.status(), "waitingForApproval");
    // Po odświeżeniu (aktualna rewizja) decyzja przechodzi.
    let fresh = f.command(Decision::Approve, "fresh", None);
    assert!(decide(&f.db, &f.handle, fresh).is_ok());
}

#[test]
fn cancel_works_without_unlock() {
    let f = Fixture::new(false);
    f.to_approval();
    cancel(&f.db, &f.handle, f.run).unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.status(), "cancelled");
    // Zakończony przebieg: czytelny błąd, nie cichy sukces.
    assert!(cancel(&f.db, &f.handle, f.run)
        .unwrap_err()
        .contains("zakończył"));
    assert!(cancel(&f.db, &f.handle, 999).is_err());
}

#[test]
fn start_and_decide_require_unlock() {
    let f = Fixture::new(false);
    f.to_approval();
    let err = decide(&f.db, &f.handle, f.command(Decision::Approve, "x", None)).unwrap_err();
    assert!(err.contains("nie jest odblokowany"));
    let dir = std::env::temp_dir();
    let err = artifact_read(&f.db, &dir, f.run, &"0".repeat(64)).unwrap_err();
    assert!(err.contains("nie jest odblokowany"));
    // Start na poziomie komend: odrzucony przed walidacją, bez wiersza i bez kolejki.
    let enqueued = std::cell::Cell::new(0);
    let enqueue = |_: i64| enqueued.set(enqueued.get() + 1);
    let emit = |_: &StoredEvent| {};
    let detected = crate::ai::Detected::default();
    let err = start_run(
        &StartDeps {
            db: &f.db,
            app_data: &dir,
            detected: &detected,
            read_codex_config: || Ok(None),
            free_bytes: agent_cli::available_bytes,
            now: 1,
            enqueue: &enqueue,
            emit: &emit,
        },
        StartRequest {
            version_id: 1,
            request_id: "locked-start".into(),
            task_id: None,
            repo: dir.to_string_lossy().into_owned(),
            goal: None,
        },
    )
    .unwrap_err();
    assert!(err.contains("nie jest odblokowany"), "{err}");
    assert_eq!(enqueued.get(), 0);
    let runs: i64 =
        f.db.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM workflow_runs", [], |r| r.get(0))
            .unwrap();
    assert_eq!(runs, 1, "tylko przebieg z fixture");
    assert_eq!(f.status(), "waitingForApproval");
}

#[test]
fn cancel_and_history_work_without_unlock() {
    let f = Fixture::new(false);
    f.to_approval();
    let list = history_list(&f.db, &RunFilter::default()).unwrap();
    assert_eq!(list.iter().map(|r| r.id).collect::<Vec<_>>(), [f.run]);
    let detail = history_get(&f.db, f.run, 2_000).unwrap();
    assert_eq!(detail.summary.id, f.run);
    let events = history_events(&f.db, f.run, 0, 500).unwrap();
    assert!(!events.is_empty());
    let later = history_events(&f.db, f.run, events[0].sequence, 500).unwrap();
    assert_eq!(later.len(), events.len() - 1);
    assert!(history_get(&f.db, 999, 2_000).is_err());
    cancel(&f.db, &f.handle, f.run).unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.status(), "cancelled");
    assert_eq!(
        history_get(&f.db, f.run, 2_000).unwrap().summary.status,
        "cancelled"
    );
}

#[test]
fn artifact_read_requires_matching_run() {
    let f = Fixture::new(true);
    let dir = std::env::temp_dir().join(format!("tuktuk-wf-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let err = artifact_read(&f.db, &dir, f.run, &"a".repeat(64)).unwrap_err();
    assert!(err.contains("Nie znaleziono artefaktu"), "{err}");
    let err = artifact_read(&f.db, &dir, 12345, "..\\..\\sekret").unwrap_err();
    assert!(err.contains("Nie znaleziono artefaktu"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `workflow_recover` działa bez klucza modułu (odzyskiwanie dostępu), jak anulowanie.
#[test]
fn recover_works_without_unlock_key() {
    use workflow_core::engine::Input;
    let f = Fixture::new(false);
    f.handle.start_run(f.run);
    assert!(f.handle.wait_idle(WAIT));
    f.db.lock()
        .unwrap()
        .execute(
            "INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at)
             VALUES(?1, 'C:/repo', 'C:/wt/r', 'abc', 'reserved', 1)",
            [f.run],
        )
        .unwrap();
    let repo = SqliteRunRepository::new(f.db.clone());
    workflow_core::testing::apply_inputs(
        &repo,
        f.run,
        vec![Input::Abandon {
            message: "przywrócona kopia".into(),
        }],
    )
    .unwrap();
    let fake = crate::workflows::processes::fake::FakeProcesses::default();
    let outcome = recover_run(&f.db, &fake, &|_| {}, f.run, 50).unwrap();
    assert!(outcome.processes_confirmed && outcome.workspace_released);
    assert!(!outcome.already_recovered);
    // Bez klucza nadal nie da się startować ani decydować.
    assert!(unlocked(&f.db).is_err());
}

fn count(f: &Fixture, table: &str) -> i64 {
    f.db.lock()
        .unwrap()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

const MODULE_TABLES: [&str; 6] = [
    "workflows",
    "workflow_versions",
    "workflow_runs",
    "workflow_events",
    "workflow_approvals",
    "workflow_artifacts",
];

fn set_status(f: &Fixture, status: &str) {
    f.db.lock()
        .unwrap()
        .execute("UPDATE workflow_runs SET status = ?1", [status])
        .unwrap();
}

fn lock_now(f: &Fixture) -> Result<crate::workflows::Access, String> {
    let conn = f.db.lock().unwrap();
    crate::workflows::lock(&conn).map_err(|e| e.to_string())
}

#[test]
fn lock_removes_flag_and_keeps_definitions_runs_and_events() {
    let f = Fixture::new(true);
    f.to_approval();
    let app = std::env::temp_dir().join(format!("tuktuk-wf-lock-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    let diff = b"--- a/x
+++ b/x
+zmiana
";
    let hash = crate::workflows::artifacts::hex_of(diff);
    crate::workflows::artifacts::store_diff(&f.db, &app, f.run, diff, &hash, 3).unwrap();
    let file = app.join(crate::workflows::artifacts::rel_path(f.run, &hash));
    assert!(file.exists());
    let before: Vec<i64> = MODULE_TABLES.iter().map(|t| count(&f, t)).collect();
    assert!(
        before.iter().all(|&n| n > 0),
        "każda tabela modułu ma dane: {before:?}"
    );
    assert!(!lock_now(&f).unwrap().unlocked);
    assert!(!store::is_unlocked(&f.db.lock().unwrap()).unwrap());
    let after: Vec<i64> = MODULE_TABLES.iter().map(|t| count(&f, t)).collect();
    assert_eq!(before, after);
    assert!(file.exists(), "plik diffu w workflows/runs zostaje");
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn lock_is_refused_while_runs_are_active() {
    for status in ["queued", "running", "cancelling"] {
        let f = Fixture::new(true);
        set_status(&f, status);
        let err = lock_now(&f).unwrap_err();
        assert!(err.starts_with("Wyłączenie wstrzymane."), "{status}: {err}");
        // Ten sam tekst w podglądzie (`src/preview/workflows.ts`, `lockRefusal`) i fixture.
        assert_eq!(
            err,
            "Wyłączenie wstrzymane. Przebiegi Workflows w toku: 1. Poczekaj na ich koniec albo je zatrzymaj (przebieg czekający na zgodę nie blokuje).",
            "{status}"
        );
        assert!(
            store::is_unlocked(&f.db.lock().unwrap()).unwrap(),
            "{status}"
        );
    }
    // Bez procesu w toku wyłączenie jest dozwolone: czekający na zgodę i wstrzymany
    // po restarcie (decyzja po E7).
    for status in ["waitingForApproval", "paused"] {
        let f = Fixture::new(true);
        set_status(&f, status);
        assert!(lock_now(&f).is_ok(), "{status}");
    }
}

#[test]
fn start_after_lock_is_refused_with_locked_message() {
    let f = Fixture::new(true);
    f.to_approval();
    lock_now(&f).unwrap();
    let dir = std::env::temp_dir();
    let enqueued = std::cell::Cell::new(0);
    let enqueue = |_: i64| enqueued.set(enqueued.get() + 1);
    let emit = |_: &StoredEvent| {};
    let detected = crate::ai::Detected::default();
    let err = start_run(
        &StartDeps {
            db: &f.db,
            app_data: &dir,
            detected: &detected,
            read_codex_config: || Ok(None),
            free_bytes: agent_cli::available_bytes,
            now: 1,
            enqueue: &enqueue,
            emit: &emit,
        },
        StartRequest {
            version_id: 1,
            request_id: "after-lock".into(),
            task_id: None,
            repo: dir.to_string_lossy().into_owned(),
            goal: None,
        },
    )
    .unwrap_err();
    assert!(err.contains("nie jest odblokowany"), "{err}");
    assert_eq!(enqueued.get(), 0);
    assert_eq!(count(&f, "workflow_runs"), 1);
}

#[test]
fn decide_after_lock_is_refused() {
    let f = Fixture::new(true);
    f.to_approval();
    let command = f.command(Decision::Approve, "after-lock", None);
    lock_now(&f).unwrap();
    let err = decide(&f.db, &f.handle, command).unwrap_err();
    assert!(err.contains("nie jest odblokowany"), "{err}");
    assert_eq!(f.status(), "waitingForApproval");
}

#[test]
fn history_and_cancel_work_after_lock() {
    let f = Fixture::new(true);
    f.to_approval();
    lock_now(&f).unwrap();
    assert_eq!(history_list(&f.db, &RunFilter::default()).unwrap().len(), 1);
    assert_eq!(
        history_get(&f.db, f.run, 2_000).unwrap().summary.status,
        "waitingForApproval"
    );
    assert!(!history_events(&f.db, f.run, 0, 500).unwrap().is_empty());
    cancel(&f.db, &f.handle, f.run).unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.status(), "cancelled");
}

/// Odzyskanie przerwanego przebiegu działa po wyłączeniu modułu (E7).
#[test]
fn recover_works_after_lock() {
    use workflow_core::engine::Input;
    let f = Fixture::new(true);
    f.to_approval();
    f.db.lock()
        .unwrap()
        .execute(
            "INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at)
             VALUES(?1, 'C:/repo', 'C:/wt/r', 'abc', 'reserved', 1)",
            [f.run],
        )
        .unwrap();
    let repo = SqliteRunRepository::new(f.db.clone());
    workflow_core::testing::apply_inputs(
        &repo,
        f.run,
        vec![Input::Abandon {
            message: "awaria".into(),
        }],
    )
    .unwrap();
    assert_eq!(f.status(), "interrupted");
    lock_now(&f).unwrap();
    let fake = crate::workflows::processes::fake::FakeProcesses::default();
    let outcome = recover_run(&f.db, &fake, &|_| {}, f.run, 50).unwrap();
    assert!(outcome.processes_confirmed && outcome.workspace_released);
    assert!(!outcome.already_recovered);
    assert!(!store::is_unlocked(&f.db.lock().unwrap()).unwrap());
}

#[test]
fn newer_module_schema_blocks_mutations_but_not_history() {
    let f = Fixture::new(true);
    f.to_approval();
    let newer = store::MODULE_SCHEMA + 1;
    {
        let conn = f.db.lock().unwrap();
        conn.execute(
            "INSERT INTO workflow_schema_migrations(version, applied_at) VALUES(?1, 1)",
            [newer],
        )
        .unwrap();
        // Ponowna inicjalizacja przechodzi i niczego nie usuwa.
        store::init_tables(&conn).unwrap();
    }
    let before = count(&f, "workflow_runs");
    let command = f.command(Decision::Approve, "newer", None);
    let err = decide(&f.db, &f.handle, command).unwrap_err();
    assert!(
        err.starts_with(
            "Baza Workflows pochodzi z nowszej wersji aplikacji (schemat 6, obsługiwany 5)."
        ),
        "{err}"
    );
    assert!(err.ends_with("Zaktualizuj aplikację."), "{err}");
    assert!(unlocked(&f.db).is_err());
    assert_eq!(history_list(&f.db, &RunFilter::default()).unwrap().len(), 1);
    assert!(history_get(&f.db, f.run, 2_000).is_ok());
    cancel(&f.db, &f.handle, f.run).unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(count(&f, "workflow_runs"), before);
}

// ---------------------------------------------------------------------------
// Decyzja po E7: wznowienie przebiegu wstrzymanego po restarcie (`workflow_resume`).

/// Fixture z przebiegiem wstrzymanym na granicy kroków (po analizie), zanim
/// sterownik go zobaczy; sterownik po starcie niczego nie uruchamia.
struct PausedFixture {
    db: SharedDb,
    handle: RuntimeHandle,
    runner: Arc<ScriptedRunner>,
    run: i64,
}

impl PausedFixture {
    fn new(unlocked: bool) -> Self {
        use workflow_core::engine::{Effect, Input};
        use workflow_core::ports::RunnerOutcome;
        use workflow_core::testing::apply_inputs;
        let t = crate::workflows::test_db::TestDb::new();
        if unlocked {
            store::set_unlocked(&t.db.lock().unwrap()).unwrap();
        }
        let run = t.create("p1");
        let effects = apply_inputs(&t.repo, run, vec![Input::Start, Input::Continue]).unwrap();
        let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
            panic!("brak LaunchAgent");
        };
        apply_inputs(
            &t.repo,
            run,
            vec![
                Input::ExecutionResult {
                    execution_id,
                    outcome: RunnerOutcome::Completed {
                        output: samples::analysis(),
                        exit_code: 0,
                        session_id: None,
                    },
                },
                Input::PauseAfterRestart,
            ],
        )
        .unwrap();
        let runner = Arc::new(
            ScriptedRunner::new()
                .script(
                    "implementation",
                    vec![workflow_core::testing::Step::complete(
                        samples::implementation(),
                    )],
                )
                .script(
                    "review",
                    vec![workflow_core::testing::Step::complete(
                        samples::review_approved(),
                    )],
                ),
        );
        let handle = Runtime::start(
            Arc::new(t.repo.clone()),
            runner.clone(),
            Arc::new(RecordingSink::new()),
            Arc::new(FixedWorkspace::new("h1")),
            DriverConfig {
                step_timeout: |_: &AgentConfig| Duration::from_secs(20),
                cancel_grace: Duration::from_secs(2),
                poll_interval: Duration::from_millis(5),
                now: || 1_000,
                on_error: |m| panic!("błąd sterownika: {m}"),
            },
        );
        assert!(handle.wait_idle(WAIT));
        Self {
            db: t.db.clone(),
            handle,
            runner,
            run,
        }
    }

    fn status(&self) -> String {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [self.run],
                |r| r.get(0),
            )
            .unwrap()
    }
}

#[test]
fn paused_run_does_not_start_after_restart() {
    let f = PausedFixture::new(true);
    assert_eq!(f.status(), "paused");
    assert_eq!(f.runner.total_calls(), 0);
}

#[test]
fn resume_requires_unlock_and_leaves_the_run_paused() {
    let f = PausedFixture::new(false);
    let err = resume_run(&f.db, &f.handle, f.run, "w1").unwrap_err();
    assert!(err.contains("nie jest odblokowany"), "{err}");
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.status(), "paused");
    assert_eq!(f.runner.total_calls(), 0);
}

#[test]
fn resume_twice_with_the_same_request_id_launches_one_step() {
    let f = PausedFixture::new(true);
    assert_eq!(
        resume_run(&f.db, &f.handle, f.run, "w1").unwrap(),
        RunStatus::Running
    );
    resume_run(&f.db, &f.handle, f.run, "w1").unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.runner.calls("implementation"), 1);
    assert_eq!(f.runner.calls("analysis"), 0);
    assert_eq!(f.status(), "waitingForApproval");
    // Inne id po wznowieniu: czytelny błąd.
    let err = resume_run(&f.db, &f.handle, f.run, "w2").unwrap_err();
    assert_eq!(err, "Przebieg nie jest wstrzymany.");
    assert!(resume_run(&f.db, &f.handle, 999, "w3").is_err());
}

#[test]
fn cancel_of_a_paused_run_works_without_unlock() {
    let f = PausedFixture::new(false);
    cancel(&f.db, &f.handle, f.run).unwrap();
    assert!(f.handle.wait_idle(WAIT));
    assert_eq!(f.status(), "cancelled");
    assert_eq!(f.runner.total_calls(), 0);
}

#[test]
fn resume_after_shutdown_is_refused_with_a_message() {
    let f = PausedFixture::new(true);
    f.handle.shutdown_graceful(Duration::from_secs(2));
    let err = resume_run(&f.db, &f.handle, f.run, "w1").unwrap_err();
    assert_eq!(
        err,
        "Aplikacja się zamyka — wznów przebieg po ponownym uruchomieniu."
    );
    assert_eq!(f.status(), "paused");
    assert_eq!(f.runner.total_calls(), 0);
}
