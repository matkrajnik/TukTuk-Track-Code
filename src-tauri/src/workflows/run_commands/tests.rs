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
