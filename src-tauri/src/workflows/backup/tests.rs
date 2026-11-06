//! Workflows w kopii zapasowej: blokada przywracania, neutralizacja przebiegów,
//! schemat i diffy osadzone w kopii (PLAN_WDROZENIA.md, 11: Backup).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::Connection;
use workflow_core::driver::{DriverConfig, Runtime};
use workflow_core::engine::{Effect, Input};
use workflow_core::model::AgentConfig;
use workflow_core::ports::{RunId, RunRepository};
use workflow_core::testing::{apply_inputs, FixedWorkspace, RecordingSink, ScriptedRunner};
use workflow_core::RunStatus;

use super::*;
use crate::workflows::artifacts::{read_diff, store_diff};
use crate::workflows::test_db::TestDb;

static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "tuktuk-wf-backup-{name}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Zapisuje bazę testu do pliku kopii (jak eksport bez osadzania).
fn save_copy(t: &TestDb, path: &Path) {
    t.db.lock()
        .unwrap()
        .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
        .unwrap();
}

fn status_of(conn: &Connection, run: RunId) -> String {
    conn.query_row(
        "SELECT status FROM workflow_runs WHERE id = ?1",
        [run],
        |r| r.get(0),
    )
    .unwrap()
}

fn has_table(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
        [BACKUP_ARTIFACTS_TABLE],
        |r| r.get(0),
    )
    .unwrap()
}

/// Przebieg na granicy kroków: po starcie sterownik wywołałby kolejny krok.
fn at_boundary(t: &TestDb, request: &str) -> RunId {
    let run = t.create(request);
    let effects = apply_inputs(&t.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    let Some(Effect::LaunchAgent { execution_id, .. }) = effects.first().cloned() else {
        panic!("brak LaunchAgent");
    };
    apply_inputs(
        &t.repo,
        run,
        vec![Input::ExecutionResult {
            execution_id,
            outcome: workflow_core::ports::RunnerOutcome::Completed {
                output: workflow_core::samples::analysis(),
                exit_code: 0,
                session_id: None,
            },
        }],
    )
    .unwrap();
    run
}

// ---------------------------------------------------------------------------
// Kryterium 8: przywracanie

#[test]
fn restore_is_refused_while_any_run_is_unfinished() {
    let dir = Dir::new("active");
    let copy_db = TestDb::new();
    let path = dir.0.join("kopia.sqlite");
    save_copy(&copy_db, &path);
    let live = TestDb::new();
    let run = live.create("w toku");
    let prepared = crate::backup::prepare_restore(&path, 1).unwrap();
    let cache = crate::settings::Cache::default();
    let mut conn = live.db.lock().unwrap();
    // Każdy nieskończony przebieg blokuje, także czekający na zgodę: kopia
    // zastąpiłaby go i odebrała rezerwację jego katalogu roboczego.
    for status in ["queued", "running", "cancelling", "waitingForApproval"] {
        conn.execute(
            "UPDATE workflow_runs SET status = ?2 WHERE id = ?1",
            rusqlite::params![run, status],
        )
        .unwrap();
        let err = crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache)
            .unwrap_err();
        assert!(
            err.contains("Nieskończone przebiegi Workflows: 1"),
            "{status}: {err}"
        );
        assert_eq!(status_of(&conn, run), status, "baza robocza bez zmian");
    }
    // Aktualizację blokują tylko przebiegi z procesem w toku.
    assert!(active_work(&conn).unwrap().is_none());
    assert!(restore_blocker(&conn).unwrap().is_some());
    conn.execute(
        "UPDATE workflow_runs SET status = 'succeeded' WHERE id = ?1",
        [run],
    )
    .unwrap();
    assert!(restore_blocker(&conn).unwrap().is_none());
    crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache).unwrap();
}

/// Przebieg doprowadzony przez sterownik do zgody (oczekująca zgoda w bazie).
fn waiting_for_approval(t: &TestDb, request: &str) -> RunId {
    let run = t.create(request);
    let runner = Arc::new(
        ScriptedRunner::new().template_script(vec![workflow_core::samples::review_approved()]),
    );
    let handle = Runtime::start(
        Arc::new(t.repo.clone()),
        runner,
        Arc::new(RecordingSink::new()),
        Arc::new(FixedWorkspace::new("h1")),
        DriverConfig {
            step_timeout: |_: &AgentConfig| Duration::from_secs(20),
            cancel_grace: Duration::from_millis(300),
            poll_interval: Duration::from_millis(5),
            now: || 1_000,
            on_error: |m| panic!("błąd sterownika: {m}"),
        },
    );
    handle.start_run(run);
    assert!(handle.wait_idle(Duration::from_secs(10)));
    handle.shutdown();
    assert_eq!(t.status(run), RunStatus::WaitingForApproval);
    run
}

/// Przebieg z krokiem w toku (intencja zapisana, brak wyniku).
fn with_step_in_progress(t: &TestDb, request: &str) -> RunId {
    let run = t.create(request);
    let effects = apply_inputs(&t.repo, run, vec![Input::Start, Input::Continue]).unwrap();
    assert!(matches!(effects.first(), Some(Effect::LaunchAgent { .. })));
    run
}

#[test]
fn restored_active_runs_become_interrupted_and_runtime_never_calls_runner() {
    let dir = Dir::new("neutralize");
    let source = TestDb::new();
    // Najpierw zgoda (sterownik źródła prowadzi tylko ten przebieg), potem reszta.
    let waiting = waiting_for_approval(&source, "zgoda");
    let boundary = at_boundary(&source, "granica");
    let in_step = with_step_in_progress(&source, "krok");
    let queued = source.create("kolejka");
    let path = dir.0.join("kopia.sqlite");
    save_copy(&source, &path);
    let prepared = crate::backup::prepare_restore(&path, 5).unwrap();
    for run in [waiting, boundary, in_step, queued] {
        assert_eq!(
            status_of(&prepared.db, run),
            "interrupted",
            "przebieg {run}"
        );
    }
    let live = TestDb::new();
    {
        let mut conn = live.db.lock().unwrap();
        let cache = crate::settings::Cache::default();
        crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache).unwrap();
    }
    let runner = Arc::new(ScriptedRunner::new());
    let handle = Runtime::start(
        Arc::new(live.repo.clone()),
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
    );
    assert!(handle.wait_idle(Duration::from_secs(10)));
    assert_eq!(
        runner.total_calls(),
        0,
        "przywrócona kopia nie wznawia poleceń"
    );
    for run in [waiting, boundary, in_step, queued] {
        assert_eq!(live.status(run), RunStatus::Interrupted, "przebieg {run}");
    }
    // Oczekująca zgoda jest unieważniona, krok w toku przerwany.
    let approval: String = live.scalar(
        "SELECT status FROM workflow_approvals WHERE run_id = ?1",
        waiting,
    );
    assert_eq!(approval, "invalidated");
    let open: i64 = live.scalar(
        "SELECT COUNT(*) FROM workflow_approvals WHERE run_id = ?1 AND status = 'pending'",
        waiting,
    );
    assert_eq!(open, 0);
    let execution: String = live.scalar(
        "SELECT status FROM workflow_node_executions WHERE run_id = ?1 ORDER BY rowid DESC LIMIT 1",
        in_step,
    );
    assert_eq!(execution, "interrupted");
    let message: String = live.scalar("SELECT error FROM workflow_runs WHERE id = ?1", boundary);
    assert!(!message.is_empty());
    // Historia zostaje: zdarzenia z kopii plus przerwanie.
    assert!(live.repo.load_events(boundary, 0).unwrap().len() > 3);
}

#[test]
fn restored_reservations_are_marked_restored() {
    let dir = Dir::new("reservations");
    let source = TestDb::new();
    let run = at_boundary(&source, "r");
    source.reserve(run, "C:/wt/r");
    let path = dir.0.join("kopia.sqlite");
    save_copy(&source, &path);
    let staged = crate::backup::prepare_restore(&path, 5).unwrap().db;
    let (state, reason): (String, Option<String>) = staged
        .query_row(
            "SELECT state, release_reason FROM workflow_workspaces WHERE run_id = ?1",
            [run],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        (state.as_str(), reason.as_deref()),
        ("released", Some("restored"))
    );
}

#[test]
fn restore_rejects_newer_workflow_schema() {
    let dir = Dir::new("schema");
    let source = TestDb::new();
    source
        .db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO workflow_schema_migrations(version, applied_at) VALUES(?1, 1)",
            [MODULE_SCHEMA + 1],
        )
        .unwrap();
    let path = dir.0.join("kopia.sqlite");
    save_copy(&source, &path);
    let err = crate::backup::stage(&path).unwrap_err();
    assert!(err.contains("nowszej wersji"), "{err}");
}

#[test]
fn restore_keeps_workflows_unlocked_key() {
    let dir = Dir::new("unlocked");
    let source = TestDb::new();
    let path = dir.0.join("kopia.sqlite");
    save_copy(&source, &path);
    let live = TestDb::new();
    let staged = crate::backup::prepare_restore(&path, 5).unwrap().db;
    let mut conn = live.db.lock().unwrap();
    crate::workflows::store::set_unlocked(&conn).unwrap();
    let cache = crate::settings::Cache::default();
    crate::backup::restore_locked(&mut conn, &staged, &dir.0.join("b"), &cache).unwrap();
    assert!(crate::workflows::store::is_unlocked(&conn).unwrap());
}

#[test]
fn restore_after_lock_keeps_module_locked() {
    let dir = Dir::new("locked");
    let source = TestDb::new();
    crate::workflows::store::set_unlocked(&source.db.lock().unwrap()).unwrap();
    let path = dir.0.join("kopia.sqlite");
    save_copy(&source, &path);
    let live = TestDb::new();
    let staged = crate::backup::prepare_restore(&path, 5).unwrap().db;
    let mut conn = live.db.lock().unwrap();
    assert!(!crate::workflows::store::is_unlocked(&conn).unwrap());
    let cache = crate::settings::Cache::default();
    crate::backup::restore_locked(&mut conn, &staged, &dir.0.join("b"), &cache).unwrap();
    assert!(!crate::workflows::store::is_unlocked(&conn).unwrap());
}

// ---------------------------------------------------------------------------
// Kryterium 9: diffy w kopii

/// Baza z dwoma diffami na dysku; zwraca (przebieg, skróty, katalog danych).
fn with_diffs(t: &TestDb, dir: &Dir) -> (RunId, Vec<String>, PathBuf) {
    let app = dir.0.join("app");
    let run = t.create("diffs");
    let hashes = ["+pierwszy\n", "+drugi diff\n"]
        .iter()
        .map(|text| {
            let hash = hex_of(text.as_bytes());
            store_diff(&t.db, &app, run, text.as_bytes(), &hash, 1).unwrap();
            hash
        })
        .collect();
    (run, hashes, app)
}

fn snapshot_of(t: &TestDb) -> Connection {
    let mut snapshot = Connection::open_in_memory().unwrap();
    let conn = t.db.lock().unwrap();
    rusqlite::backup::Backup::new(&conn, &mut snapshot)
        .unwrap()
        .run_to_completion(100, Duration::from_millis(1), None)
        .unwrap();
    snapshot
}

fn embedded(snapshot: &Connection) -> i64 {
    snapshot
        .query_row(
            &format!("SELECT COUNT(*) FROM {BACKUP_ARTIFACTS_TABLE}"),
            [],
            |r| r.get(0),
        )
        .unwrap()
}

#[test]
fn export_embeds_valid_artifacts_and_counts_missing() {
    let dir = Dir::new("embed");
    let t = TestDb::new();
    let (run, hashes, app) = with_diffs(&t, &dir);
    // Jeden plik zniknął, drugi ma podmienioną treść: oba pominięte.
    std::fs::remove_file(app.join(rel_path(run, &hashes[0]))).unwrap();
    let snapshot = snapshot_of(&t);
    let report = embed_artifacts(&snapshot, &app, MAX_BACKUP_ARTIFACT_BYTES).unwrap();
    assert_eq!(
        report,
        EmbedReport {
            embedded: 1,
            missing: 1,
            over_limit: 0
        }
    );
    std::fs::write(app.join(rel_path(run, &hashes[1])), "+podmiana\n").unwrap();
    let report = embed_artifacts(&snapshot, &app, MAX_BACKUP_ARTIFACT_BYTES).unwrap();
    assert_eq!(report.embedded, 0);
    assert_eq!(report.missing, 2);
    assert_eq!(embedded(&snapshot), 0, "tabela odtwarzana od zera");
}

#[test]
fn export_respects_artifact_size_limit() {
    let dir = Dir::new("limit");
    let t = TestDb::new();
    let (_, _, app) = with_diffs(&t, &dir);
    let snapshot = snapshot_of(&t);
    let first_len = "+pierwszy\n".len() as u64;
    let report = embed_artifacts(&snapshot, &app, first_len).unwrap();
    assert_eq!(report.embedded, 1);
    assert_eq!(report.over_limit, 1);
}

#[test]
fn export_leaves_live_database_without_backup_table() {
    let dir = Dir::new("live");
    let t = TestDb::new();
    let (_, _, app) = with_diffs(&t, &dir);
    let snapshot = snapshot_of(&t);
    embed_artifacts(&snapshot, &app, MAX_BACKUP_ARTIFACT_BYTES).unwrap();
    assert!(has_table(&snapshot));
    assert!(!has_table(&t.db.lock().unwrap()));
}

/// Plik kopii z osadzonymi diffami; zwraca (ścieżka kopii, przebieg, skróty).
fn exported(dir: &Dir) -> (PathBuf, RunId, Vec<String>) {
    let t = TestDb::new();
    let (run, hashes, app) = with_diffs(&t, dir);
    let snapshot = snapshot_of(&t);
    embed_artifacts(&snapshot, &app, MAX_BACKUP_ARTIFACT_BYTES).unwrap();
    let path = dir.0.join("kopia.sqlite");
    snapshot
        .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])
        .unwrap();
    (path, run, hashes)
}

/// Jak przywrócenie: weryfikacja wpisów i zapis plików (w kodzie zapis jest po zamianie bazy).
fn extract_artifacts(staged: &Connection, target: &Path) -> Result<ExtractReport, String> {
    take_artifacts(staged)?.write(target)
}

#[test]
fn restore_writes_verified_artifacts_only_after_replace() {
    let dir = Dir::new("extract");
    let (path, run, hashes) = exported(&dir);
    let target = dir.0.join("nowe dane");
    let prepared = crate::backup::prepare_restore(&path, 5).unwrap();
    assert!(
        !has_table(&prepared.db),
        "tabela kopii nie trafia do bazy roboczej"
    );
    assert!(
        !target.join("workflows").exists(),
        "przygotowanie niczego nie zapisuje"
    );
    let live = TestDb::new();
    {
        let mut conn = live.db.lock().unwrap();
        let cache = crate::settings::Cache::default();
        crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache).unwrap();
    }
    let report = prepared.write_artifacts(&target).unwrap();
    assert_eq!(report.written, 2);
    for hash in &hashes {
        let file = target.join(rel_path(run, hash));
        assert_eq!(hex_of(&std::fs::read(&file).unwrap()), *hash);
    }
}

#[test]
fn refused_restore_writes_no_artifact_files() {
    let dir = Dir::new("refused");
    let (path, run, hashes) = exported(&dir);
    let target = dir.0.join("dane");
    let prepared = crate::backup::prepare_restore(&path, 5).unwrap();
    let live = TestDb::new();
    let active = live.create("w toku");
    let mut conn = live.db.lock().unwrap();
    let cache = crate::settings::Cache::default();
    let refused = crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache);
    assert!(refused.is_err());
    assert_eq!(status_of(&conn, active), "queued");
    for hash in &hashes {
        assert!(!target.join(rel_path(run, hash)).exists());
    }
    assert!(!target.join("workflows").exists());
}

fn tamper(path: &Path, sql: &str) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(sql).unwrap();
}

#[test]
fn restore_skips_wrong_hash() {
    let dir = Dir::new("wrong-hash");
    let (path, run, hashes) = exported(&dir);
    tamper(
        &path,
        &format!(
            "UPDATE {BACKUP_ARTIFACTS_TABLE} SET content = X'2B7A6C650A' WHERE hash = '{}'",
            hashes[0]
        ),
    );
    let target = dir.0.join("dane");
    let staged = crate::backup::stage(&path).unwrap();
    let report = extract_artifacts(&staged, &target).unwrap();
    assert_eq!(
        report,
        ExtractReport {
            written: 1,
            rejected: 1
        }
    );
    assert!(!target.join(rel_path(run, &hashes[0])).exists());
    assert!(target.join(rel_path(run, &hashes[1])).is_file());
}

#[test]
fn restore_rejects_traversal_and_foreign_run_paths() {
    let dir = Dir::new("traversal");
    let (path, run, hashes) = exported(&dir);
    let foreign = rel_path(run + 1, &hashes[1]);
    tamper(
        &path,
        &format!(
            "UPDATE {BACKUP_ARTIFACTS_TABLE} SET rel_path = '../../poza.patch' WHERE hash = '{}';
             UPDATE {BACKUP_ARTIFACTS_TABLE} SET rel_path = '{foreign}' WHERE hash = '{}';",
            hashes[0], hashes[1]
        ),
    );
    let target = dir.0.join("dane");
    let staged = crate::backup::stage(&path).unwrap();
    let report = extract_artifacts(&staged, &target).unwrap();
    assert_eq!(
        report,
        ExtractReport {
            written: 0,
            rejected: 2
        }
    );
    assert!(!dir.0.join("poza.patch").exists());
    assert!(!target.join(&foreign).exists());
}

#[test]
fn restore_skips_artifacts_without_index_row() {
    let dir = Dir::new("no-index");
    let (path, run, hashes) = exported(&dir);
    tamper(
        &path,
        &format!(
            "DELETE FROM workflow_artifacts WHERE hash = '{}'",
            hashes[0]
        ),
    );
    let target = dir.0.join("dane");
    let staged = crate::backup::stage(&path).unwrap();
    let report = extract_artifacts(&staged, &target).unwrap();
    assert_eq!(
        report,
        ExtractReport {
            written: 1,
            rejected: 1
        }
    );
    assert!(!target.join(rel_path(run, &hashes[0])).exists());
}

#[test]
fn restored_artifact_is_readable_by_read_diff() {
    let dir = Dir::new("readable");
    let (path, run, hashes) = exported(&dir);
    let target = dir.0.join("dane");
    let prepared = crate::backup::prepare_restore(&path, 5).unwrap();
    let live = TestDb::new();
    {
        let mut conn = live.db.lock().unwrap();
        let cache = crate::settings::Cache::default();
        crate::backup::restore_locked(&mut conn, &prepared.db, &dir.0.join("b"), &cache).unwrap();
        assert!(!has_table(&conn));
    }
    prepared.write_artifacts(&target).unwrap();
    let view = read_diff(&live.db, &target, run, &hashes[1], 1024).unwrap();
    assert_eq!(view.text, "+drugi diff\n");
}
