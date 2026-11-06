//! Wspólna baza testów modułu: opublikowany szablon i pomocnicy przebiegów.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use workflow_core::ports::{NewRun, RunId, RunRepository};
use workflow_core::template::analysis_implementation_review;
use workflow_core::RunStatus;

use super::runs::SqliteRunRepository;
use super::store;
use super::workspace::SharedDb;

pub struct TestDb {
    pub db: SharedDb,
    pub repo: SqliteRunRepository,
    pub version_id: i64,
}

impl TestDb {
    pub fn new() -> Self {
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
        }
    }

    pub fn create(&self, request_id: &str) -> RunId {
        self.repo
            .create_run(&NewRun {
                version_id: self.version_id,
                request_id: request_id.into(),
                now: 10,
                ..NewRun::default()
            })
            .unwrap()
            .run_id
    }

    pub fn scalar<T: rusqlite::types::FromSql>(&self, sql: &str, run: RunId) -> T {
        self.db
            .lock()
            .unwrap()
            .query_row(sql, [run], |r| r.get(0))
            .unwrap()
    }

    pub fn status(&self, run: RunId) -> RunStatus {
        let text: String = self.scalar("SELECT status FROM workflow_runs WHERE id = ?1", run);
        serde_json::from_value(serde_json::Value::String(text)).unwrap()
    }

    /// Rezerwacja katalogu bez prawdziwego gita.
    pub fn reserve(&self, run: RunId, path: &str) {
        self.db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO workflow_workspaces(run_id, source_repo, worktree_path, base_commit, state, created_at)
                 VALUES(?1, 'C:/repo', ?2, 'abc', 'reserved', 1)",
                rusqlite::params![run, path],
            )
            .unwrap();
    }

    /// (stan, powód zwolnienia) ostatniej rezerwacji przebiegu.
    pub fn reservation(&self, run: RunId) -> (String, Option<String>) {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT state, release_reason FROM workflow_workspaces WHERE run_id = ?1 ORDER BY id DESC",
                [run],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap()
    }

    pub fn process_state(&self, execution_id: &str) -> String {
        self.db
            .lock()
            .unwrap()
            .query_row(
                "SELECT state FROM workflow_processes WHERE execution_id = ?1",
                [execution_id],
                |r| r.get(0),
            )
            .unwrap()
    }
}
