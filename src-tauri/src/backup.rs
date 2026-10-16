use crate::{db, Database};
use rusqlite::{backup::Backup, Connection, OpenFlags};
use serde::Serialize;
use std::{path::PathBuf, sync::Mutex, time::Duration};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

#[derive(Default)]
pub struct Pending(Mutex<Option<PathBuf>>);
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    name: String,
    tasks: usize,
    sessions: usize,
    trash: usize,
}
fn copy(source: &Connection, target: &mut Connection) -> Result<(), String> {
    Backup::new(source, target)
        .map_err(|e| e.to_string())?
        .run_to_completion(100, Duration::from_millis(10), None)
        .map_err(|e| e.to_string())
}
pub fn stage(path: &std::path::Path) -> Result<Connection, String> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "Nie można otworzyć kopii SQLite.")?;
    let valid:bool=source.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tasks') AND EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='sessions')",[],|r|r.get(0)).map_err(|_|"Niepoprawny plik kopii.")?;
    if !valid {
        return Err("To nie jest kopia Todo Track.".into());
    }
    let mut memory = Connection::open_in_memory().map_err(|e| e.to_string())?;
    copy(&source, &mut memory)?;
    if memory
        .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        != "ok"
    {
        return Err("Kopia bazy jest uszkodzona.".into());
    }
    db::init(&memory).map_err(|e| e.to_string())?;
    if memory
        .prepare("PRAGMA foreign_key_check")
        .map_err(|e| e.to_string())?
        .exists([])
        .map_err(|e| e.to_string())?
    {
        return Err("Kopia ma niespójne powiązania.".into());
    }
    let snap = db::snapshot(&memory).map_err(|_| "Kopia ma nieobsługiwany format.")?;
    if snap
        .sessions
        .iter()
        .chain(snap.trash_sessions.iter())
        .any(|s| s.started_at < 0 || s.ended_at.is_some_and(|e| e < s.started_at))
    {
        return Err("Kopia zawiera niepoprawne przedziały.".into());
    }
    // An exported running session is closed at the export instant, never resumed on import.
    let exported: i64 = memory
        .query_row(
            "SELECT value FROM settings WHERE key='backup_exported_at'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| {
            std::fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or_else(db::now)
        });
    db::stop(&memory, exported.min(db::now())).map_err(|e| e.to_string())?;
    Ok(memory)
}

#[tauri::command]
pub async fn export_backup(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move||{
        let Some(file)=app.dialog().file().set_file_name(format!("Todo-Track-{}.sqlite",chrono::Local::now().format("%Y-%m-%d"))).add_filter("Kopia Todo Track", &["sqlite"]).blocking_save_file() else{return Ok(None);};
        let path=file.into_path().map_err(|e|e.to_string())?;
        let own=app.path().app_data_dir().map_err(|e|e.to_string())?.join("todo-track.sqlite");
        if path.exists() && path.canonicalize().ok()==own.canonicalize().ok() { return Err("Wybierz plik inny niż bieżąca baza.".into()); }
        let state=app.state::<Database>(); let db=state.0.lock().map_err(|e|e.to_string())?;
        let mut snapshot=Connection::open_in_memory().map_err(|e|e.to_string())?; copy(&db,&mut snapshot)?;
        snapshot.execute("INSERT INTO settings(key,value) VALUES('backup_exported_at',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[db::now().to_string()]).map_err(|e|e.to_string())?;
        let mut target=Connection::open(&path).map_err(|e|e.to_string())?; copy(&snapshot,&mut target)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn inspect_backup(app: tauri::AppHandle) -> Result<Option<Preview>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        *app.state::<Pending>().0.lock().map_err(|e| e.to_string())? = None;
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("Kopia Todo Track", &["sqlite"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let staged = stage(&path)?;
        let snap = db::snapshot(&staged).map_err(|e| e.to_string())?;
        let preview = Preview {
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            tasks: snap.tasks.len(),
            sessions: snap.sessions.len(),
            trash: snap.trash.len(),
        };
        // Stage a stable snapshot so the confirmation refers to the inspected content.
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stable = dir.join(format!("restore-candidate-{}.sqlite", db::now()));
        let mut target = Connection::open(&stable).map_err(|e| e.to_string())?;
        copy(&staged, &mut target)?;
        *app.state::<Pending>().0.lock().map_err(|e| e.to_string())? = Some(stable);
        Ok(Some(preview))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn restore_backup(app: tauri::AppHandle) -> Result<db::Snapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .state::<Pending>()
            .0
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or("Najpierw wybierz i sprawdź kopię.")?;
        let staged = stage(&path)?;
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("backups");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let safety = dir.join(format!("before-restore-{}.sqlite", crate::db::now()));
        db.execute("VACUUM INTO ?1", [safety.to_string_lossy().as_ref()])
            .map_err(|e| e.to_string())?;
        // Carry current installation's Jira configuration; imported credentials are never assumed.
        let jira: Option<String> = db
            .query_row("SELECT value FROM settings WHERE key='jira'", [], |r| {
                r.get(0)
            })
            .ok();
        staged
            .execute("DELETE FROM settings WHERE key='jira'", [])
            .map_err(|e| e.to_string())?;
        if let Some(config) = jira {
            staged
                .execute(
                    "INSERT INTO settings(key,value) VALUES('jira',?1)",
                    [config],
                )
                .map_err(|e| e.to_string())?;
        }
        copy(&staged, &mut db)?;
        let result = db::snapshot(&db).map_err(|e| e.to_string())?;
        drop(db);
        *app.state::<Pending>().0.lock().map_err(|e| e.to_string())? = None;
        crate::idle::clear(&app);
        crate::reminder::reset(&app);
        let _ = app.emit("data-changed", ());
        // Kopia przynosi własne ustawienia; moduły reagują jak na zwykłą zmianę.
        let _ = app.emit("settings-changed", crate::settings::current(&app));
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_roundtrip_keeps_trash_and_stops_exported_timer() {
        let root = std::env::temp_dir().join(format!(
            "todo-track-roundtrip-{}-{}",
            std::process::id(),
            db::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("copy.sqlite");
        let mut source = Connection::open(&path).unwrap();
        db::init(&source).unwrap();
        db::change(&mut source, "add", None, Some("A".into()), 0).unwrap();
        db::change(&mut source, "start", Some(1), None, 100).unwrap();
        db::change(&mut source, "add", None, Some("Trash".into()), 0).unwrap();
        db::change(&mut source, "delete", Some(2), None, 200).unwrap();
        source
            .execute(
                "INSERT INTO settings(key,value) VALUES('backup_exported_at','500')",
                [],
            )
            .unwrap();
        drop(source);
        let staged = stage(&path).unwrap();
        assert_eq!(db::snapshot(&staged).unwrap().trash[0].title, "Trash");
        assert_eq!(
            db::snapshot(&staged).unwrap().sessions[0].ended_at,
            Some(500)
        );
        drop(staged);
        let invalid = root.join("invalid.sqlite");
        std::fs::write(&invalid, b"not a SQLite database").unwrap();
        assert!(stage(&invalid).is_err());
        assert!(root
            .canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap()));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Kopia niesie szablony, a przywrócenie (`copy` jak w `restore_backup`)
    /// zastępuje bieżące szablony szablonami z kopii.
    #[test]
    fn backup_carries_templates_and_restore_replaces_them() {
        use crate::templates;
        let root = std::env::temp_dir().join(format!(
            "todo-track-templates-{}-{}",
            std::process::id(),
            db::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("copy.sqlite");
        let mut source = Connection::open(&path).unwrap();
        db::init(&source).unwrap();
        templates::save(
            &mut source,
            None,
            "review",
            templates::Body {
                title: "Code review {klucz}: {tytuł}".into(),
                steps: vec!["Sprawdź testy".into()],
                ..templates::Body::default()
            },
        )
        .unwrap();
        drop(source);

        let mut current = Connection::open_in_memory().unwrap();
        db::init(&current).unwrap();
        templates::save(&mut current, None, "lokalny", templates::Body::default()).unwrap();
        let staged = stage(&path).unwrap();
        copy(&staged, &mut current).unwrap();
        let restored = templates::list(&current).unwrap();
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].name, "review");
        assert_eq!(restored[0].body.steps, ["Sprawdź testy"]);
        drop(staged);

        // Kopia sprzed szablonów: po przywróceniu tabela istnieje i jest pusta.
        let old = root.join("old.sqlite");
        let legacy = Connection::open(&old).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
                 CREATE TABLE sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, started_at INTEGER NOT NULL, ended_at INTEGER);",
            )
            .unwrap();
        drop(legacy);
        let staged = stage(&old).unwrap();
        copy(&staged, &mut current).unwrap();
        assert!(templates::list(&current).unwrap().is_empty());
        drop(staged);
        drop(current);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Kopia niesie czas agentów (`agent_sessions`, PLAN_1.7.md W1).
    #[test]
    fn backup_carries_agent_sessions() {
        let root = std::env::temp_dir().join(format!(
            "todo-track-agent-time-{}-{}",
            std::process::id(),
            db::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("copy.sqlite");
        let source = Connection::open(&path).unwrap();
        db::init(&source).unwrap();
        source
            .execute_batch(
                "INSERT INTO tasks(title,created_at) VALUES('Analiza',0);
                 INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES('claude','C:\repo',1,10,20);",
            )
            .unwrap();
        drop(source);
        let mut current = Connection::open_in_memory().unwrap();
        db::init(&current).unwrap();
        let staged = stage(&path).unwrap();
        copy(&staged, &mut current).unwrap();
        let time = crate::agent_time::query(&current, 0, 100, 100).unwrap();
        assert_eq!(time.periods.len(), 1);
        assert_eq!(time.totals[0].agent_ms, 10);
        drop(staged);
        drop(current);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Kopia niesie wnioski z przeglądu tygodnia (`week_notes`, PLAN_1.7.md W2).
    #[test]
    fn backup_carries_week_notes() {
        let root = std::env::temp_dir().join(format!(
            "todo-track-week-notes-{}-{}",
            std::process::id(),
            db::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("copy.sqlite");
        let source = Connection::open(&path).unwrap();
        db::init(&source).unwrap();
        crate::week_notes::save(&source, "2026-10-12", "Mniej przełączeń").unwrap();
        drop(source);
        let mut current = Connection::open_in_memory().unwrap();
        db::init(&current).unwrap();
        let staged = stage(&path).unwrap();
        copy(&staged, &mut current).unwrap();
        assert_eq!(
            crate::week_notes::load(&current, "2026-10-12").unwrap(),
            "Mniej przełączeń"
        );
        drop(staged);
        drop(current);
        std::fs::remove_dir_all(root).unwrap();
    }
}
