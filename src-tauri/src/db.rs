use rusqlite::{params, Connection};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub done: bool,
    pub created_at: i64,
    pub jira_key: Option<String>,
    pub jira_url: Option<String>,
    pub details: crate::planner::Details,
    pub archived: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub task_id: i64,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub tasks: Vec<Task>,
    pub sessions: Vec<Session>,
    pub trash: Vec<Task>,
    pub trash_sessions: Vec<Session>,
    pub closed_days: Vec<String>,
}
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}
pub fn init(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, done INTEGER NOT NULL DEFAULT 0, created_at INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, started_at INTEGER NOT NULL, ended_at INTEGER);
        CREATE UNIQUE INDEX IF NOT EXISTS one_active_session ON sessions((1)) WHERE ended_at IS NULL;
        CREATE TABLE IF NOT EXISTS task_details (task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE, json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS task_lifecycle (task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE, archived INTEGER NOT NULL DEFAULT 0, deleted_at INTEGER);
        CREATE TABLE IF NOT EXISTS closed_days (day TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS recurrence_state (task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE, last_day TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS recurrence_occurrences (source_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, day TEXT NOT NULL, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, PRIMARY KEY(source_id,day));
        CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS jira_tasks (task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE, site TEXT NOT NULL, issue_key TEXT NOT NULL, UNIQUE(site,issue_key));")
}
pub fn snapshot(db: &Connection) -> rusqlite::Result<Snapshot> {
    let tasks = db
        .prepare("SELECT t.id,t.title,t.done,t.created_at,j.issue_key,j.site || '/browse/' || j.issue_key,COALESCE(d.json,'{}'),COALESCE(l.archived,0),l.deleted_at IS NOT NULL FROM tasks t LEFT JOIN jira_tasks j ON j.task_id=t.id LEFT JOIN task_details d ON d.task_id=t.id LEFT JOIN task_lifecycle l ON l.task_id=t.id ORDER BY t.id DESC")?
        .query_map([], |r| {
            Ok((Task {
                id: r.get(0)?,
                title: r.get(1)?,
                done: r.get(2)?,
                created_at: r.get(3)?,
                jira_key: r.get(4)?,
                jira_url: r.get(5)?,
                details: serde_json::from_str(&r.get::<_, String>(6)?).map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e)))?,
                archived: r.get(7)?,
            },r.get::<_,bool>(8)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let sessions = db
        .prepare("SELECT id,task_id,started_at,ended_at FROM sessions ORDER BY id DESC")?
        .query_map([], |r| {
            Ok(Session {
                id: r.get(0)?,
                task_id: r.get(1)?,
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut live = Vec::new();
    let mut trash = Vec::new();
    for (task, deleted) in tasks {
        if deleted {
            trash.push(task);
        } else {
            live.push(task);
        }
    }
    let deleted_ids: std::collections::HashSet<_> = trash.iter().map(|t| t.id).collect();
    let (trash_sessions, sessions) = sessions
        .into_iter()
        .partition(|s| deleted_ids.contains(&s.task_id));
    let closed_days = db
        .prepare("SELECT day FROM closed_days ORDER BY day")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(Snapshot {
        tasks: live,
        sessions,
        trash,
        trash_sessions,
        closed_days,
    })
}
pub fn stop(db: &Connection, at: i64) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE ended_at IS NULL",
        [at],
    )?;
    Ok(())
}
pub fn change(
    db: &mut Connection,
    action: &str,
    id: Option<i64>,
    title: Option<String>,
    at: i64,
) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let apply = || -> Result<(), String> {
        if matches!(
            action,
            "rename" | "start" | "toggle" | "delete" | "archive" | "unarchive" | "restore"
        ) {
            let exists: bool = tx
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1)",
                    [id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if !exists {
                return Err("Nie znaleziono zadania.".into());
            }
            let deleted: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM task_lifecycle WHERE task_id=?1 AND deleted_at IS NOT NULL)",[id],|r|r.get(0)).map_err(|e|e.to_string())?;
            if deleted && action != "restore" {
                return Err("Najpierw przywróć zadanie z kosza.".into());
            }
        }
        match action {
            "add" | "rename" => {
                let title = title.unwrap_or_default().trim().to_owned();
                if title.is_empty() || title.chars().count() > 240 {
                    return Err("Tytuł musi mieć od 1 do 240 znaków.".into());
                }
                if action == "add" {
                    tx.execute(
                        "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
                        params![title, at],
                    )
                } else {
                    tx.execute("UPDATE tasks SET title=?1 WHERE id=?2", params![title, id])
                }
                .map_err(|e| e.to_string())?;
            }
            "start" => {
                tx.execute(
                    "UPDATE task_lifecycle SET archived=0 WHERE task_id=?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
                crate::planner::clear_waiting(&tx, id.unwrap()).map_err(|e| e.to_string())?;
                let done: bool = tx
                    .query_row("SELECT done FROM tasks WHERE id=?1", [id], |r| r.get(0))
                    .map_err(|e| e.to_string())?;
                if done {
                    return Err("Najpierw przywróć zadanie do zrobienia.".into());
                }
                let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE task_id=?1 AND ended_at IS NULL)", [id], |r|r.get(0)).map_err(|e|e.to_string())?;
                if !active {
                    stop(&tx, at).map_err(|e| e.to_string())?;
                    tx.execute(
                        "INSERT INTO sessions(task_id,started_at) VALUES(?1,?2)",
                        params![id, at],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
            "stop" => stop(&tx, at).map_err(|e| e.to_string())?,
            "toggle" => {
                tx.execute("UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE task_id=?2 AND ended_at IS NULL", params![at,id]).map_err(|e|e.to_string())?;
                tx.execute("UPDATE tasks SET done=NOT done WHERE id=?1", [id])
                    .map_err(|e| e.to_string())?;
            }
            "delete" => {
                tx.execute("UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE task_id=?2 AND ended_at IS NULL",params![at,id]).map_err(|e|e.to_string())?;
                tx.execute("INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET deleted_at=excluded.deleted_at",params![id,at]).map_err(|e|e.to_string())?;
            }
            "restore" => {
                tx.execute(
                    "UPDATE task_lifecycle SET deleted_at=NULL WHERE task_id=?1",
                    [id],
                )
                .map_err(|e| e.to_string())?;
            }
            "archive" | "unarchive" => {
                if action == "archive" {
                    tx.execute("UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE task_id=?2 AND ended_at IS NULL",params![at,id]).map_err(|e|e.to_string())?;
                }
                tx.execute("INSERT INTO task_lifecycle(task_id,archived) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET archived=excluded.archived",params![id,action=="archive"]).map_err(|e|e.to_string())?;
            }
            _ => return Err("Nieznana operacja.".into()),
        }
        Ok(())
    };
    apply()?;
    tx.commit().map_err(|e| e.to_string())?;
    snapshot(db).map_err(|e| e.to_string())
}

pub fn edit_session(
    db: &mut Connection,
    id: i64,
    started_at: i64,
    ended_at: i64,
    at: i64,
) -> Result<Snapshot, String> {
    if started_at < 0 || ended_at <= started_at || ended_at > at {
        return Err("Koniec musi być późniejszy niż początek i nie może być w przyszłości.".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let original_end: Option<i64> = tx
        .query_row("SELECT ended_at FROM sessions WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .map_err(|_| "Nie znaleziono przedziału.".to_owned())?;
    if original_end.is_none() {
        return Err("Zatrzymaj pomiar przed edycją przedziału.".into());
    }
    let overlaps: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE id<>?1 AND started_at<?3 AND (ended_at IS NULL OR ended_at>?2) AND (ended_at IS NULL OR ended_at>started_at))",
        params![id, started_at, ended_at], |r| r.get(0),
    ).map_err(|e| e.to_string())?;
    if overlaps {
        return Err("Ten przedział nakłada się na inny pomiar. Popraw godziny.".into());
    }
    tx.execute(
        "UPDATE sessions SET started_at=?1, ended_at=?2 WHERE id=?3",
        params![started_at, ended_at, id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    snapshot(db).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_edits_validate_and_preserve_other_measurements() {
        let mut db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        change(&mut db, "add", None, Some("A".into()), 0).unwrap();
        change(&mut db, "add", None, Some("B".into()), 0).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        change(&mut db, "start", Some(2), None, 3000).unwrap();
        assert!(edit_session(&mut db, 1, 500, 3500, 5000).is_err());
        assert!(edit_session(&mut db, 2, 3000, 4000, 5000).is_err());
        assert!(edit_session(&mut db, 1, 1000, 1000, 5000).is_err());
        assert!(edit_session(&mut db, 1, -1, 1000, 5000).is_err());
        assert!(edit_session(&mut db, 99, 0, 1000, 5000).is_err());
        let s = snapshot(&db).unwrap();
        assert_eq!(s.sessions[1].started_at, 1000);
        assert_eq!(s.sessions[1].ended_at, Some(3000));
        let s = edit_session(&mut db, 1, 500, 3000, 5000).unwrap();
        assert_eq!(s.sessions[1].started_at, 500);
        assert_eq!(s.sessions[0].ended_at, None);
        change(&mut db, "stop", None, None, 5000).unwrap();
        assert!(edit_session(&mut db, 2, 3000, 6000, 5000).is_err());
        assert!(edit_session(&mut db, 2, 2000, 4000, 5000).is_err());
        edit_session(&mut db, 2, 3000, 4500, 5000).unwrap();
    }
    #[test]
    fn switching_is_atomic_and_start_is_idempotent() {
        let mut db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        change(&mut db, "add", None, Some("A".into()), 0).unwrap();
        change(&mut db, "add", None, Some("B".into()), 0).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        change(&mut db, "start", Some(1), None, 2000).unwrap();
        assert!(change(&mut db, "start", Some(99), None, 3000).is_err());
        let s = change(&mut db, "start", Some(2), None, 4000).unwrap();
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[1].ended_at, Some(4000));
        let s = change(&mut db, "toggle", Some(2), None, 5000).unwrap();
        assert!(s.sessions.iter().all(|s| s.ended_at.is_some()));
        assert!(change(&mut db, "start", Some(2), None, 6000).is_err());
    }
    #[test]
    fn validates_titles_and_deletes_history() {
        let mut db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        assert!(change(&mut db, "add", None, Some("  ".into()), 0).is_err());
        change(&mut db, "add", None, Some(" A ".into()), 0).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        let s = change(&mut db, "delete", Some(1), None, 2000).unwrap();
        assert!(s.tasks.is_empty() && s.sessions.is_empty());
        assert_eq!(s.trash.len(), 1);
        assert_eq!(s.trash_sessions[0].ended_at, Some(2000));
        assert!(change(&mut db, "start", Some(1), None, 3000).is_err());
        let s = change(&mut db, "restore", Some(1), None, 3000).unwrap();
        assert_eq!(s.sessions[0].started_at, 1000);
        assert!(s.trash.is_empty());
        change(&mut db, "start", Some(1), None, 4000).unwrap();
        let s = change(&mut db, "archive", Some(1), None, 5000).unwrap();
        assert!(s.tasks[0].archived);
        assert_eq!(s.sessions.len(), 2);
        assert_eq!(s.sessions[0].ended_at, Some(5000));
        let s = change(&mut db, "unarchive", Some(1), None, 6000).unwrap();
        assert!(!s.tasks[0].archived);
    }
}
