//! Retencja diffów: stare artefakty zakończonych przebiegów znikają z dysku,
//! indeks zostaje z `purged_at`, a odczyt mówi o retencji.

use std::path::PathBuf;

use super::*;
use crate::workflows::artifacts::{hex_of, read_diff, store_diff, ArtifactError};
use crate::workflows::test_db::TestDb;

const NOW: i64 = 100 * DAY_MS;

fn temp_app(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tuktuk-wf-retention-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Zapisuje diff przebiegu z datą `created_at` i zwraca (skrót, ścieżka pliku).
fn diff(t: &TestDb, app: &Path, run: i64, text: &str, created_at: i64) -> (String, PathBuf) {
    let hash = hex_of(text.as_bytes());
    store_diff(&t.db, app, run, text.as_bytes(), &hash, created_at).unwrap();
    let path = app.join(format!("workflows/runs/{run}/diff-{hash}.patch"));
    assert!(path.is_file());
    (hash, path)
}

fn set_status(t: &TestDb, run: i64, status: &str) {
    t.db.lock()
        .unwrap()
        .execute(
            "UPDATE workflow_runs SET status = ?2 WHERE id = ?1",
            rusqlite::params![run, status],
        )
        .unwrap();
}

fn purged_at(t: &TestDb, hash: &str) -> Option<i64> {
    t.db.lock()
        .unwrap()
        .query_row(
            "SELECT purged_at FROM workflow_artifacts WHERE hash = ?1",
            [hash],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn retention_removes_old_terminal_artifacts_and_marks_purged() {
    let t = TestDb::new();
    let app = temp_app("old");
    let run = t.create("done");
    set_status(&t, run, "succeeded");
    let (old, old_path) = diff(&t, &app, run, "+stary\n", NOW - 31 * DAY_MS);
    let (fresh, fresh_path) = diff(&t, &app, run, "+nowy\n", NOW - 2 * DAY_MS);
    let report = purge_expired(&t.db, &app, NOW, 30).unwrap();
    assert_eq!(report.purged, 1, "{:?}", report.errors);
    assert!(!old_path.exists());
    assert_eq!(purged_at(&t, &old), Some(NOW));
    assert!(fresh_path.is_file());
    assert_eq!(purged_at(&t, &fresh), None);
    // Druga retencja nic już nie robi.
    assert_eq!(purge_expired(&t.db, &app, NOW, 30).unwrap().purged, 0);
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn retention_keeps_active_runs_and_pending_approvals() {
    let t = TestDb::new();
    let app = temp_app("keep");
    let active = t.create("active");
    let (active_hash, active_path) = diff(&t, &app, active, "+w toku\n", 0);
    let waiting = t.create("waiting");
    set_status(&t, waiting, "interrupted");
    let (pending_hash, pending_path) = diff(&t, &app, waiting, "+czeka\n", 0);
    t.db.lock()
        .unwrap()
        .execute(
            "INSERT INTO workflow_approvals(id, run_id, execution_id, artifact_hash, status)
             VALUES('a1', ?1, 'e1', ?2, 'pending')",
            rusqlite::params![waiting, pending_hash],
        )
        .unwrap();
    let report = purge_expired(&t.db, &app, NOW, 1).unwrap();
    assert_eq!(report.purged, 0);
    assert!(active_path.is_file() && pending_path.is_file());
    assert_eq!(purged_at(&t, &active_hash), None);
    assert_eq!(purged_at(&t, &pending_hash), None);
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn purged_artifact_read_reports_retention() {
    let t = TestDb::new();
    let app = temp_app("read");
    let run = t.create("done");
    set_status(&t, run, "rejected");
    let (hash, _) = diff(&t, &app, run, "+do usuniecia\n", 0);
    purge_expired(&t.db, &app, NOW, 30).unwrap();
    let err = read_diff(&t.db, &app, run, &hash, 1024).unwrap_err();
    assert_eq!(err, ArtifactError::Purged);
    assert!(err.to_string().contains("okresie przechowywania"), "{err}");
    let _ = std::fs::remove_dir_all(&app);
}
