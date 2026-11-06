//! Zapis atomowy artefaktu i redakcja tekstu diffu w widoku (bez Gita).

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use workflow_core::ports::{NewRun, RunRepository};
use workflow_core::template::analysis_implementation_review;

use super::*;
use crate::workflows::runs::SqliteRunRepository;
use crate::workflows::store;

fn db_with_run() -> (SharedDb, RunId) {
    let conn = Connection::open_in_memory().unwrap();
    crate::db::init(&conn).unwrap();
    let id = store::create(&conn, "P", "", &analysis_implementation_review(), 1).unwrap();
    store::publish(&conn, id, 1, 2).unwrap();
    let version_id: i64 = conn
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    let db: SharedDb = Arc::new(Mutex::new(conn));
    let run = SqliteRunRepository::new(db.clone())
        .create_run(&NewRun {
            version_id,
            request_id: "art".into(),
            now: 1,
            ..NewRun::default()
        })
        .unwrap()
        .run_id;
    (db, run)
}

fn temp_app(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("tuktuk-wf-artifacts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn rows(db: &SharedDb) -> i64 {
    db.lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM workflow_artifacts", [], |r| r.get(0))
        .unwrap()
}

fn files_in(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn artifact_write_is_atomic_no_partial_file_on_error() {
    let (db, run) = db_with_run();
    let app = temp_app("atomic");
    let bytes = b"diff --git a/x b/x\n+zmiana\n";
    let hash = hex_of(bytes);
    let err = store_diff_with(&db, &app, run, bytes, &hash, 1, |_, _| {
        Err(std::io::Error::other("rename zawiódł"))
    })
    .unwrap_err();
    assert!(matches!(err, ArtifactError::Storage(_)), "{err:?}");
    let dir = app.join(format!("workflows/runs/{run}"));
    assert_eq!(files_in(&dir), Vec::<String>::new(), "ani .patch, ani .tmp");
    assert_eq!(rows(&db), 0, "bez wiersza w workflow_artifacts");
    // Ponowny zapis po błędzie się udaje i zostawia tylko plik docelowy.
    store_diff(&db, &app, run, bytes, &hash, 2).unwrap();
    assert_eq!(files_in(&dir), vec![format!("diff-{hash}.patch")]);
    assert_eq!(rows(&db), 1);
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn parallel_writes_of_the_same_hash_use_distinct_temp_files() {
    let (db, run) = db_with_run();
    let app = temp_app("parallel");
    let bytes = b"+ten sam diff\n".to_vec();
    let hash = hex_of(&bytes);
    let seen = Arc::new(Mutex::new(Vec::new()));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let (db, app, bytes, hash, seen) = (&db, &app, &bytes, &hash, seen.clone());
            scope.spawn(move || {
                store_diff_with(db, app, run, bytes, hash, 1, |from, to| {
                    seen.lock().unwrap().push(from.to_path_buf());
                    std::fs::rename(from, to)
                })
                .unwrap();
            });
        }
    });
    let mut temps = seen.lock().unwrap().clone();
    let count = temps.len();
    temps.sort();
    temps.dedup();
    assert_eq!(temps.len(), count, "każdy zapis ma własny plik tymczasowy");
    let dir = app.join(format!("workflows/runs/{run}"));
    assert_eq!(files_in(&dir), vec![format!("diff-{hash}.patch")]);
    assert_eq!(rows(&db), 1);
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn diff_view_masks_secrets_but_file_and_hash_stay_raw() {
    let (db, run) = db_with_run();
    let app = temp_app("redact");
    let raw = "diff --git a/.env b/.env\n\
               +API_KEY=abcdefghijklmnopqrstuvwxyz0123456789\n\
               +  \"auth\": \"Bearer sk-abcdefghijklmnopqrstuvwxyz012345\",\n\
               -const x = 1;\n\
               +const x = 2;\n";
    let hash = hex_of(raw.as_bytes());
    store_diff(&db, &app, run, raw.as_bytes(), &hash, 1).unwrap();
    let view = read_diff(&db, &app, run, &hash, MAX_DIFF_VIEW_BYTES).unwrap();
    assert!(!view.text.contains("abcdefghijklmnop"), "{}", view.text);
    assert_eq!(view.redacted, 2);
    assert!(view.text.contains("+[ukryte]\n"), "{}", view.text);
    assert!(view.text.contains("+  \"auth\": \"Bearer [ukryte]\","));
    assert!(view.text.contains("-const x = 1;\n+const x = 2;\n"));
    assert_eq!(view.hash, hash, "zgoda dotyczy surowych bajtów");
    let stored =
        std::fs::read(app.join(format!("workflows/runs/{run}/diff-{hash}.patch"))).unwrap();
    assert_eq!(stored, raw.as_bytes(), "plik na dysku bez redakcji");
    let _ = std::fs::remove_dir_all(&app);
}

#[test]
fn redaction_keeps_ordinary_code_and_paths() {
    let text = "+fn main() {\n+    let path = \"C:/Users/abcdefghijklmnopqrstuvwxyz\";\n+}\n";
    assert_eq!(redact_for_view(text), (text.to_owned(), 0));
}
