//! Przebiegi na SQLite: migracja 2, idempotencja, historia tylko do dopisywania,
//! sterownik z `ScriptedRunner` i blokada bazy (PLAN_WDROZENIA.md, 11).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use workflow_core::driver::{
    DecideError, DecideRequest, DriverConfig, Runtime, RuntimeHandle,
    MAX_RUNNER_EVENTS_PER_EXECUTION,
};
use workflow_core::engine::{Effect, Input, RunEvent};
use workflow_core::model::AgentConfig;
use workflow_core::ports::RunnerOutcome;
use workflow_core::template::analysis_implementation_review;
use workflow_core::testing::{
    apply_inputs, message, samples, FixedWorkspace, RecordingSink, ScriptedRunner, Step,
};
use workflow_core::{
    consistent, ApprovalStatus, Decision, ExecutionStatus, FailureCode, RunStatus,
};

use super::*;
use crate::workflows::store;

const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    db: Arc<Mutex<Connection>>,
    repo: SqliteRunRepository,
    version_id: i64,
    sink: Arc<RecordingSink>,
    workspace: Arc<FixedWorkspace>,
}

impl Fixture {
    fn new() -> Self {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::init(&conn).unwrap();
        let id = store::create(&conn, "Proces", "", &analysis_implementation_review(), 1).unwrap();
        store::publish(&conn, id, 1, 2).unwrap();
        let version_id = conn
            .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
            .unwrap();
        let db = Arc::new(Mutex::new(conn));
        Self {
            repo: SqliteRunRepository::new(db.clone()),
            db,
            version_id,
            sink: Arc::new(RecordingSink::new()),
            workspace: Arc::new(FixedWorkspace::new("h1")),
        }
    }

    fn new_run(&self, request_id: &str) -> NewRun {
        NewRun {
            version_id: self.version_id,
            request_id: request_id.into(),
            now: 10,
            ..NewRun::default()
        }
    }

    fn create(&self, request_id: &str) -> RunId {
        self.repo
            .create_run(&self.new_run(request_id))
            .unwrap()
            .run_id
    }

    fn runtime(&self, runner: &Arc<ScriptedRunner>) -> RuntimeHandle {
        Runtime::start(
            Arc::new(self.repo.clone()),
            runner.clone(),
            self.sink.clone(),
            self.workspace.clone(),
            DriverConfig {
                step_timeout: |_: &AgentConfig| Duration::from_secs(20),
                cancel_grace: Duration::from_millis(300),
                poll_interval: Duration::from_millis(5),
                now: || 1_000,
                on_error: |m| panic!("błąd sterownika: {m}"),
            },
        )
    }

    fn scalar<T: rusqlite::types::FromSql>(&self, sql: &str) -> T {
        self.db
            .lock()
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }

    fn column(&self, sql: &str) -> Vec<String> {
        let db = self.db.lock().unwrap();
        let mut statement = db.prepare(sql).unwrap();
        statement
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn run_status(&self, run: RunId) -> RunStatus {
        let text: String = self
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM workflow_runs WHERE id = ?1",
                [run],
                |r| r.get(0),
            )
            .unwrap();
        serde_json::from_value(serde_json::Value::String(text)).unwrap()
    }

    fn execution_statuses(&self, run: RunId) -> Vec<(String, ExecutionStatus)> {
        let db = self.db.lock().unwrap();
        let mut statement = db
            .prepare("SELECT node_id, status FROM workflow_node_executions WHERE run_id = ?1 ORDER BY rowid")
            .unwrap();
        statement
            .query_map([run], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .unwrap()
            .map(|row| {
                let (node, status) = row.unwrap();
                (
                    node,
                    serde_json::from_value(serde_json::Value::String(status)).unwrap(),
                )
            })
            .collect()
    }

    fn assert_projection_consistent(&self, run: RunId) {
        let statuses: Vec<ExecutionStatus> = self
            .execution_statuses(run)
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        assert!(
            consistent(self.run_status(run), &statuses),
            "{:?} {:?}",
            self.run_status(run),
            statuses
        );
    }

    fn decide(
        &self,
        handle: &RuntimeHandle,
        run: RunId,
        decision: Decision,
        request: &str,
    ) -> Result<workflow_core::driver::DecideResult, DecideError> {
        let events = self.repo.load_events(run, 0).unwrap();
        let graph = self.repo.graph(run).unwrap();
        let state = workflow_core::testing::replay_stored(&graph, &events).unwrap();
        let approval = state.approvals.last().unwrap();
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

fn template_runner(reviews: Vec<serde_json::Value>) -> Arc<ScriptedRunner> {
    Arc::new(ScriptedRunner::new().template_script(reviews))
}

fn settle(handle: &RuntimeHandle) {
    assert!(
        handle.wait_idle(WAIT),
        "sterownik nie uspokoił się w czasie"
    );
}

#[test]
fn migration_of_a_v1_database_keeps_data_and_adds_run_tables() {
    let f = Fixture::new();
    {
        let mut db = f.db.lock().unwrap();
        crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
        db.execute_batch(
            "DROP TABLE workflow_approvals; DROP TABLE workflow_node_executions;
             DROP TABLE workflow_events; DROP TABLE workflow_runs;
             DELETE FROM workflow_schema_migrations WHERE version = 2;",
        )
        .unwrap();
        crate::db::init(&db).unwrap();
        crate::db::init(&db).unwrap();
    }
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM tasks"), 1);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_versions"), 1);
    assert_eq!(
        f.column("SELECT CAST(version AS TEXT) FROM workflow_schema_migrations ORDER BY version"),
        ["1", "2", "3"]
    );
    f.create("po-migracji");
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_runs"), 1);
}

#[test]
fn migration_2_to_3_keeps_runs_and_definitions_and_is_idempotent() {
    let f = Fixture::new();
    // Zadanie z dwoma pomiarami (zakończonym i trwającym) i przebieg przypięty do zadania.
    let task_id = {
        let db = f.db.lock().unwrap();
        db.execute(
            "INSERT INTO tasks(title, done, created_at) VALUES('Zadanie przed migracją', 0, 100)",
            [],
        )
        .unwrap();
        let task_id = db.last_insert_rowid();
        db.execute(
            "INSERT INTO sessions(task_id, started_at, ended_at) VALUES(?1, 1000, 4600), (?1, 5000, NULL)",
            [task_id],
        )
        .unwrap();
        task_id
    };
    let run = f
        .repo
        .create_run(&NewRun {
            task_id: Some(task_id),
            ..f.new_run("przed-migracja")
        })
        .unwrap()
        .run_id;
    let sessions = |f: &Fixture| -> Vec<(i64, i64, Option<i64>)> {
        let db = f.db.lock().unwrap();
        let mut stmt = db
            .prepare("SELECT task_id, started_at, ended_at FROM sessions ORDER BY id")
            .unwrap();
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows
    };
    let sessions_before = sessions(&f);
    {
        let db = f.db.lock().unwrap();
        db.execute_batch(
            "DROP TABLE workflow_workspaces; DELETE FROM workflow_schema_migrations WHERE version = 3;",
        )
        .unwrap();
        crate::db::init(&db).unwrap();
        crate::db::init(&db).unwrap();
    }
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_runs"), 1);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflows"), 1);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_versions"), 1);
    assert_eq!(
        f.scalar::<i64>("SELECT COUNT(*) FROM workflow_workspaces"),
        0
    );
    assert_eq!(
        f.scalar::<i64>("SELECT COUNT(*) FROM workflow_schema_migrations"),
        3
    );
    assert_eq!(
        sessions_before,
        vec![(task_id, 1000, Some(4600)), (task_id, 5000, None)]
    );
    assert_eq!(sessions(&f), sessions_before);
    assert_eq!(
        f.scalar::<String>("SELECT title FROM tasks"),
        "Zadanie przed migracją"
    );
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM tasks"), 1);
    assert_eq!(
        f.scalar::<i64>(&format!(
            "SELECT task_id FROM workflow_runs WHERE id = {run}"
        )),
        task_id
    );
}

#[test]
fn duplicate_request_id_returns_the_same_run() {
    let f = Fixture::new();
    let first = f.repo.create_run(&f.new_run("r1")).unwrap();
    let second = f.repo.create_run(&f.new_run("r1")).unwrap();
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.run_id, second.run_id);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_runs"), 1);
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_events"), 1);
}

#[test]
fn same_request_id_with_another_version_conflicts() {
    let f = Fixture::new();
    f.create("r1");
    {
        let db = f.db.lock().unwrap();
        db.execute(
            "INSERT INTO workflow_versions(workflow_id, version, schema_version, definition_json, hash, created_at)
             SELECT workflow_id, 2, schema_version, definition_json, 'inny', 3 FROM workflow_versions",
            [],
        )
        .unwrap();
    }
    let other = f.scalar::<i64>("SELECT MAX(id) FROM workflow_versions");
    let new = NewRun {
        version_id: other,
        ..f.new_run("r1")
    };
    assert_eq!(f.repo.create_run(&new), Err(RepoError::Conflict));
}

#[test]
fn missing_version_empty_request_id_and_invalid_definition_are_rejected() {
    let f = Fixture::new();
    let missing = NewRun {
        version_id: 999,
        ..f.new_run("r1")
    };
    assert_eq!(f.repo.create_run(&missing), Err(RepoError::NotFound));
    assert!(matches!(
        f.repo.create_run(&f.new_run("  ")),
        Err(RepoError::BadInput(_))
    ));
    assert!(matches!(
        f.repo
            .create_run(&f.new_run(&"x".repeat(MAX_REQUEST_ID_CHARS + 1))),
        Err(RepoError::BadInput(_))
    ));
    {
        let db = f.db.lock().unwrap();
        db.execute(
            "INSERT INTO workflow_versions(workflow_id, version, schema_version, definition_json, hash, created_at)
             SELECT workflow_id, 2, schema_version, '{\"schemaVersion\":1,\"nodes\":[],\"edges\":[]}', 'zly', 3
             FROM workflow_versions",
            [],
        )
        .unwrap();
    }
    let invalid = NewRun {
        version_id: f.scalar("SELECT MAX(id) FROM workflow_versions"),
        ..f.new_run("r2")
    };
    assert!(matches!(
        f.repo.create_run(&invalid),
        Err(RepoError::InvalidDefinition(_))
    ));
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_runs"), 0);
}

#[test]
fn events_are_append_only() {
    let f = Fixture::new();
    f.create("r1");
    let db = f.db.lock().unwrap();
    assert!(db
        .execute("UPDATE workflow_events SET type = 'x'", [])
        .is_err());
    assert!(db.execute("DELETE FROM workflow_events", []).is_err());
}

#[test]
fn stale_sequence_conflicts_and_failed_batch_rolls_back() {
    let f = Fixture::new();
    let run = f.create("r1");
    let planned = |id: &str| RunEvent::ExecutionPlanned {
        execution_id: id.into(),
        node_id: "analysis".into(),
        iteration: 1,
        attempt: 1,
        inputs: vec![],
    };
    assert_eq!(
        f.repo.append(run, 5, &[planned("x")], 1),
        Err(RepoError::Conflict)
    );
    // Druga pozycja łamie klucz główny: cała partia ma się wycofać.
    let result = f.repo.append(run, 2, &[planned("x"), planned("x")], 1);
    assert!(matches!(result, Err(RepoError::Storage(_))));
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_events"), 1);
    assert_eq!(
        f.scalar::<i64>("SELECT COUNT(*) FROM workflow_node_executions"),
        0
    );
    assert_eq!(f.repo.last_sequence(run).unwrap(), 1);
    assert!(f.repo.append(run, 2, &[planned("x")], 1).is_ok());
    assert_eq!(f.repo.append(404, 1, &[], 1), Err(RepoError::NotFound));
}

#[test]
fn run_completes_through_sqlite_with_consistent_projections() {
    let f = Fixture::new();
    let runner = template_runner(vec![samples::review_approved()]);
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    assert_eq!(f.run_status(run), RunStatus::WaitingForApproval);
    assert_eq!(
        f.execution_statuses(run).last(),
        Some(&("approval".to_owned(), ExecutionStatus::Waiting))
    );
    f.assert_projection_consistent(run);
    assert_eq!(
        f.column("SELECT status FROM workflow_approvals"),
        ["pending"]
    );

    assert!(f.decide(&handle, run, Decision::Approve, "d1").is_ok());
    settle(&handle);
    assert_eq!(f.run_status(run), RunStatus::Succeeded);
    f.assert_projection_consistent(run);
    assert_eq!(
        f.column("SELECT status FROM workflow_approvals"),
        ["approved"]
    );
    assert!(f
        .execution_statuses(run)
        .iter()
        .all(|(_, s)| *s == ExecutionStatus::Completed));
    assert!(f
        .scalar::<Option<i64>>("SELECT finished_at FROM workflow_runs")
        .is_some());
    assert!(f
        .scalar::<Option<String>>("SELECT error_code FROM workflow_runs")
        .is_none());
    // Wynik kroku jest zapisany w kolumnie JSON.
    assert!(f
        .scalar::<String>(
            "SELECT output_json FROM workflow_node_executions WHERE node_id = 'review'"
        )
        .contains("approved"));
}

#[test]
fn review_limit_is_recorded_in_the_run_row() {
    let f = Fixture::new();
    let runner = template_runner(vec![samples::review_changes(); 4]);
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    assert_eq!(f.run_status(run), RunStatus::Failed);
    assert_eq!(
        f.scalar::<String>("SELECT error_code FROM workflow_runs"),
        "reviewLimitExceeded"
    );
    f.assert_projection_consistent(run);
}

#[test]
fn repeated_decision_writes_one_resolution_and_other_decision_conflicts() {
    let f = Fixture::new();
    let runner = template_runner(vec![samples::review_approved()]);
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    let first = f.decide(&handle, run, Decision::Approve, "d1");
    assert!(first.is_ok());
    settle(&handle);
    let approval = f.column("SELECT id FROM workflow_approvals").remove(0);
    let again = handle.decide(DecideRequest {
        run_id: run,
        approval_id: approval.clone(),
        decision: Decision::Approve,
        request_id: "d1".into(),
        expected_revision: 0,
        artifact_hash: "h1".into(),
    });
    assert_eq!(
        first.map(|r| r.approval_status),
        again.map(|r| r.approval_status)
    );
    let other = handle.decide(DecideRequest {
        run_id: run,
        approval_id: approval,
        decision: Decision::Reject,
        request_id: "d2".into(),
        expected_revision: 0,
        artifact_hash: "h1".into(),
    });
    assert_eq!(other, Err(DecideError::Conflict));
    assert_eq!(
        f.scalar::<i64>("SELECT COUNT(*) FROM workflow_events WHERE type = 'approvalResolved'"),
        1
    );
}

#[test]
fn stale_artifact_hash_invalidates_the_approval() {
    let f = Fixture::new();
    let runner = template_runner(vec![samples::review_approved()]);
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    f.workspace.set_hash("h2");
    assert_eq!(
        f.decide(&handle, run, Decision::Approve, "d1"),
        Err(DecideError::Invalidated)
    );
    assert_eq!(
        f.column("SELECT status FROM workflow_approvals ORDER BY rowid"),
        ["invalidated", "pending"]
    );
    assert_eq!(f.run_status(run), RunStatus::WaitingForApproval);
    let _ = ApprovalStatus::Invalidated;
}

#[test]
fn request_id_reused_after_invalidation_conflicts_instead_of_a_storage_error() {
    let f = Fixture::new();
    let runner = template_runner(vec![samples::review_approved()]);
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    f.workspace.set_hash("h2");
    assert_eq!(
        f.decide(&handle, run, Decision::Approve, "d1"),
        Err(DecideError::Invalidated)
    );
    // Ponowienie z tym samym id na nowej zgodzie: konflikt, nie naruszenie UNIQUE.
    assert_eq!(
        f.decide(&handle, run, Decision::Approve, "d1"),
        Err(DecideError::Conflict)
    );
    assert_eq!(
        f.column("SELECT status FROM workflow_approvals ORDER BY rowid"),
        ["invalidated", "pending"]
    );
    assert!(f.decide(&handle, run, Decision::Approve, "d2").is_ok());
    settle(&handle);
    assert_eq!(f.run_status(run), RunStatus::Succeeded);
}

fn boundary_after_analysis(f: &Fixture, run: RunId) {
    let effects = apply_inputs(&f.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    apply_inputs(
        &f.repo,
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
fn restart_at_a_step_boundary_calls_the_next_step_once() {
    let f = Fixture::new();
    let run = f.create("r1");
    boundary_after_analysis(&f, run);
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    );
    let handle = f.runtime(&runner);
    settle(&handle);
    assert_eq!(runner.calls("analysis"), 0);
    assert_eq!(runner.calls("implementation"), 1);
    assert_eq!(f.run_status(run), RunStatus::WaitingForApproval);
    drop(handle);
    let handle = f.runtime(&runner);
    settle(&handle);
    assert_eq!(runner.calls("implementation"), 1);
    assert_eq!(runner.calls("review"), 1);
}

#[test]
fn restart_after_the_intent_marks_execution_and_run_interrupted() {
    let f = Fixture::new();
    let run = f.create("r1");
    apply_inputs(&f.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let runner = Arc::new(ScriptedRunner::new());
    let handle = f.runtime(&runner);
    settle(&handle);
    assert_eq!(runner.total_calls(), 0);
    assert_eq!(f.run_status(run), RunStatus::Interrupted);
    assert_eq!(
        f.execution_statuses(run),
        [("analysis".to_owned(), ExecutionStatus::Interrupted)]
    );
    assert_eq!(
        f.scalar::<String>("SELECT error_code FROM workflow_runs"),
        "interrupted"
    );
    f.assert_projection_consistent(run);
    let _ = FailureCode::Interrupted;
}

#[test]
fn database_lock_is_free_while_the_runner_works() {
    let f = Fixture::new();
    let lock_was_free = Arc::new(AtomicBool::new(false));
    let (db, flag) = (f.db.clone(), lock_was_free.clone());
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "analysis",
                vec![Step::complete(samples::analysis()).delayed(Duration::from_millis(30))],
            )
            .with_probe(move |_| flag.store(db.try_lock().is_ok(), Ordering::SeqCst)),
    );
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    // Test nie dotyka bazy, dopóki sterownik pracuje.
    assert!(handle.wait_idle(WAIT));
    assert!(runner.calls("analysis") >= 1);
    assert!(
        lock_was_free.load(Ordering::SeqCst),
        "blokada bazy była trzymana podczas kroku"
    );
}

#[test]
fn deleting_the_task_keeps_the_run_with_its_title() {
    let f = Fixture::new();
    {
        let mut db = f.db.lock().unwrap();
        crate::db::change(&mut db, "add", None, Some("Zadanie".into()), 0).unwrap();
    }
    let task_id: i64 = f.scalar("SELECT id FROM tasks");
    let new = NewRun {
        task_id: Some(task_id),
        task_title: Some("Zadanie".into()),
        ..f.new_run("r1")
    };
    f.repo.create_run(&new).unwrap();
    {
        let mut db = f.db.lock().unwrap();
        crate::db::change(&mut db, "delete", Some(task_id), None, 5).unwrap();
    }
    // Usunięcie w aplikacji jest miękkie; twarde skasowanie wiersza też nie rusza historii.
    f.db.lock()
        .unwrap()
        .execute("DELETE FROM tasks", [])
        .unwrap();
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM tasks"), 0);
    assert_eq!(
        f.scalar::<String>("SELECT task_title FROM workflow_runs"),
        "Zadanie"
    );
    assert_eq!(f.scalar::<i64>("SELECT COUNT(*) FROM workflow_events"), 1);
}

#[test]
fn sequences_are_dense_per_run_and_events_since_pages_without_duplicates() {
    let f = Fixture::new();
    let runner = Arc::new(
        ScriptedRunner::new()
            .template_script(vec![samples::review_approved()])
            .template_script(vec![samples::review_approved()]),
    );
    let handle = f.runtime(&runner);
    let (a, b) = (f.create("a"), f.create("b"));
    handle.start_run(a);
    handle.start_run(b);
    settle(&handle);
    for run in [a, b] {
        let events = f.repo.load_events(run, 0).unwrap();
        let sequences: Vec<u64> = events.iter().map(|e| e.sequence).collect();
        assert_eq!(sequences, (1..=events.len() as u64).collect::<Vec<_>>());
        let k = 4;
        let tail = f.repo.events_since(run, k, MAX_EVENTS_PAGE).unwrap();
        assert!(tail.iter().all(|e| e.sequence > k));
        assert_eq!(tail.len(), events.len() - k as usize);
        let page = f.repo.events_since(run, 0, 3).unwrap();
        assert_eq!(
            page.iter().map(|e| e.sequence).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        let next = f.repo.events_since(run, 3, 3).unwrap();
        assert_eq!(next[0].sequence, 4);
        // Sink dostał dokładnie to, co zapisano (poza RunCreated spoza sterownika), w tej kolejności.
        let emitted: Vec<_> = f
            .sink
            .events()
            .into_iter()
            .filter(|e| e.run_id == run)
            .collect();
        assert_eq!(emitted, events[1..]);
    }
    assert_eq!(
        f.repo.events_since(a, 0, 0).unwrap().len(),
        1,
        "limit 0 daje najmniejszą stronę"
    );
}

#[test]
fn runner_messages_over_the_limit_leave_one_truncation_marker() {
    let f = Fixture::new();
    let many: Vec<_> = (0..MAX_RUNNER_EVENTS_PER_EXECUTION + 50)
        .map(|i| message(&format!("m{i}")))
        .collect();
    let runner = Arc::new(
        ScriptedRunner::new()
            .script(
                "analysis",
                vec![Step::complete(samples::analysis()).with_events(many)],
            )
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("review", vec![Step::complete(samples::review_approved())]),
    );
    let handle = f.runtime(&runner);
    let run = f.create("r1");
    handle.start_run(run);
    settle(&handle);
    assert_eq!(
        f.scalar::<i64>("SELECT COUNT(*) FROM workflow_events WHERE type = 'message'"),
        MAX_RUNNER_EVENTS_PER_EXECUTION as i64 + 1
    );
    assert_eq!(
        f.scalar::<i64>(
            "SELECT COUNT(*) FROM workflow_events WHERE type = 'message' AND payload_json LIKE '%truncated%'"
        ),
        1
    );
}
