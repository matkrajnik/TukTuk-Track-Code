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
    /// Ostatni znany status zgłoszenia w Jirze (`jira_issues`).
    pub jira_status: Option<String>,
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
        CREATE TABLE IF NOT EXISTS jira_tasks (task_id INTEGER PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE, site TEXT NOT NULL, issue_key TEXT NOT NULL, UNIQUE(site,issue_key));
        CREATE TABLE IF NOT EXISTS jira_issues (site TEXT NOT NULL, issue_key TEXT NOT NULL, status TEXT NOT NULL DEFAULT '', assigned INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(site,issue_key));
        CREATE INDEX IF NOT EXISTS sessions_task ON sessions(task_id);
        CREATE INDEX IF NOT EXISTS sessions_started ON sessions(started_at);
        CREATE TABLE IF NOT EXISTS notifications_sent (key TEXT PRIMARY KEY, at INTEGER);
        CREATE TABLE IF NOT EXISTS task_templates (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE COLLATE NOCASE, json TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS agent_bindings (folder TEXT PRIMARY KEY COLLATE NOCASE, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE);
        CREATE TABLE IF NOT EXISTS focus_blocks (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, completed INTEGER);
        CREATE TABLE IF NOT EXISTS focus_queue (id INTEGER PRIMARY KEY AUTOINCREMENT, key TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS calendar_events (id TEXT PRIMARY KEY, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL, subject TEXT NOT NULL DEFAULT '', show_as TEXT NOT NULL DEFAULT 'busy', all_day INTEGER NOT NULL DEFAULT 0, cancelled INTEGER NOT NULL DEFAULT 0, response TEXT NOT NULL DEFAULT 'none', updated_at INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS calendar_events_start ON calendar_events(start);
        CREATE TABLE IF NOT EXISTS agent_sessions (id INTEGER PRIMARY KEY AUTOINCREMENT, agent TEXT NOT NULL, folder TEXT NOT NULL, task_id INTEGER REFERENCES tasks(id) ON DELETE SET NULL, start INTEGER NOT NULL, \"end\" INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS agent_sessions_start ON agent_sessions(start);
        CREATE INDEX IF NOT EXISTS agent_sessions_end ON agent_sessions(\"end\");
        CREATE INDEX IF NOT EXISTS agent_sessions_task ON agent_sessions(task_id);
        CREATE TABLE IF NOT EXISTS week_notes (week TEXT PRIMARY KEY, text TEXT NOT NULL DEFAULT '');
        CREATE TABLE IF NOT EXISTS jira_board_columns (board_id INTEGER NOT NULL, position INTEGER NOT NULL, name TEXT NOT NULL, status_ids TEXT NOT NULL DEFAULT '[]', PRIMARY KEY(board_id, position));
        CREATE TABLE IF NOT EXISTS jira_statuses (site TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL, category TEXT NOT NULL DEFAULT '', PRIMARY KEY(site, id));
        CREATE TABLE IF NOT EXISTS jira_board_meta (board_id INTEGER PRIMARY KEY, site TEXT NOT NULL, project TEXT NOT NULL DEFAULT '', fetched_at INTEGER NOT NULL);")
}
pub fn snapshot(db: &Connection) -> rusqlite::Result<Snapshot> {
    let tasks = db
        .prepare("SELECT t.id,t.title,t.done,t.created_at,j.issue_key,j.site || '/browse/' || j.issue_key,COALESCE(d.json,'{}'),COALESCE(l.archived,0),l.deleted_at IS NOT NULL,NULLIF(ji.status,'') FROM tasks t LEFT JOIN jira_tasks j ON j.task_id=t.id LEFT JOIN jira_issues ji ON ji.site=j.site AND ji.issue_key=j.issue_key LEFT JOIN task_details d ON d.task_id=t.id LEFT JOIN task_lifecycle l ON l.task_id=t.id ORDER BY t.id DESC")?
        .query_map([], |r| {
            Ok((Task {
                id: r.get(0)?,
                title: r.get(1)?,
                done: r.get(2)?,
                created_at: r.get(3)?,
                jira_key: r.get(4)?,
                jira_url: r.get(5)?,
                jira_status: r.get(9)?,
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

/// Dzieli przedział w chwili `at`. Pierwsza część zostaje przy dotychczasowym
/// zadaniu i kończy się na `at`, druga należy do `target_task` (domyślnie to
/// samo zadanie). Przedział aktywny pozostaje aktywny w drugiej części, więc
/// obowiązują dla niej reguły Startu.
pub fn split_session(
    db: &mut Connection,
    id: i64,
    at: i64,
    target_task: Option<i64>,
    now: i64,
) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let (task_id, started_at, ended_at, deleted): (i64, i64, Option<i64>, bool) = tx
        .query_row(
            "SELECT s.task_id,s.started_at,s.ended_at,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=s.task_id AND l.deleted_at IS NOT NULL) FROM sessions s WHERE s.id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| "Nie znaleziono przedziału.".to_owned())?;
    if deleted {
        return Err("Przedział należy do zadania w koszu.".into());
    }
    let end = ended_at.unwrap_or(now);
    if !(started_at < at && at < end) {
        return Err("Moment podziału musi leżeć wewnątrz przedziału.".into());
    }
    let target = target_task.unwrap_or(task_id);
    let (done, target_deleted): (bool, bool) = tx
        .query_row(
            "SELECT t.done,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=t.id AND l.deleted_at IS NOT NULL) FROM tasks t WHERE t.id=?1",
            [target],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "Nie znaleziono zadania docelowego.".to_owned())?;
    if target_deleted {
        return Err("Zadanie docelowe jest w koszu. Najpierw je przywróć.".into());
    }
    if ended_at.is_none() {
        if done {
            return Err("Najpierw przywróć zadanie docelowe do zrobienia.".into());
        }
        tx.execute(
            "UPDATE task_lifecycle SET archived=0 WHERE task_id=?1",
            [target],
        )
        .map_err(|e| e.to_string())?;
        crate::planner::clear_waiting(&tx, target).map_err(|e| e.to_string())?;
    }
    // Najpierw zamknięcie, potem wstawienie: indeks one_active_session
    // dopuszcza tylko jeden otwarty przedział.
    tx.execute(
        "UPDATE sessions SET ended_at=?1 WHERE id=?2",
        params![at, id],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
        params![target, at, ended_at],
    )
    .map_err(|e| e.to_string())?;
    if ended_at.is_none() {
        // Pomiar trwa dalej w nowym wierszu: przenosimy klucz „długi pomiar”,
        // żeby podział nie wywołał drugiego powiadomienia dla tego samego pomiaru.
        let new_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT OR IGNORE INTO notifications_sent(key,at)
             SELECT ?1, at FROM notifications_sent WHERE key=?2",
            params![format!("long:{new_id}"), format!("long:{id}")],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    snapshot(db).map_err(|e| e.to_string())
}

/// Przenosi cały przedział na zadanie `target` (np. „Przypisz przerwę do X”, gdy
/// przerwa zaczęła się razem z pomiarem i nie ma czego dzielić). Reguły celu jak
/// w `split_session`: zadanie poza koszem, a dla trwającego pomiaru do zrobienia.
pub fn move_session(db: &mut Connection, id: i64, target: i64) -> Result<Snapshot, String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let (ended_at, deleted): (Option<i64>, bool) = tx
        .query_row(
            "SELECT s.ended_at,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=s.task_id AND l.deleted_at IS NOT NULL) FROM sessions s WHERE s.id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "Nie znaleziono przedziału.".to_owned())?;
    if deleted {
        return Err("Przedział należy do zadania w koszu.".into());
    }
    let (done, target_deleted): (bool, bool) = tx
        .query_row(
            "SELECT t.done,EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=t.id AND l.deleted_at IS NOT NULL) FROM tasks t WHERE t.id=?1",
            [target],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| "Nie znaleziono zadania docelowego.".to_owned())?;
    if target_deleted {
        return Err("Zadanie docelowe jest w koszu. Najpierw je przywróć.".into());
    }
    if ended_at.is_none() {
        if done {
            return Err("Najpierw przywróć zadanie docelowe do zrobienia.".into());
        }
        tx.execute(
            "UPDATE task_lifecycle SET archived=0 WHERE task_id=?1",
            [target],
        )
        .map_err(|e| e.to_string())?;
        crate::planner::clear_waiting(&tx, target).map_err(|e| e.to_string())?;
    }
    tx.execute(
        "UPDATE sessions SET task_id=?1 WHERE id=?2",
        params![target, id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    snapshot(db).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn split_fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        change(&mut db, "add", None, Some("A".into()), 0).unwrap();
        change(&mut db, "add", None, Some("B".into()), 0).unwrap();
        db
    }
    #[test]
    fn split_finished_session_keeps_total_range() {
        let mut db = split_fixture();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        change(&mut db, "stop", None, None, 5000).unwrap();
        change(&mut db, "toggle", Some(2), None, 6000).unwrap();
        // Zakończony przedział można przypisać także ukończonemu zadaniu.
        let s = split_session(&mut db, 1, 3000, Some(2), 9000).unwrap();
        assert_eq!(s.sessions.len(), 2);
        let first = s.sessions.iter().find(|x| x.id == 1).unwrap();
        assert_eq!(
            (first.task_id, first.started_at, first.ended_at),
            (1, 1000, Some(3000))
        );
        let second = s.sessions.iter().find(|x| x.id != 1).unwrap();
        assert_eq!(
            (second.task_id, second.started_at, second.ended_at),
            (2, 3000, Some(5000))
        );
        // Bez celu druga część zostaje przy tym samym zadaniu.
        let s = split_session(&mut db, 1, 2000, None, 9000).unwrap();
        assert_eq!(s.sessions.iter().filter(|x| x.task_id == 1).count(), 2);
    }
    #[test]
    fn split_active_session_moves_timer_to_target() {
        let mut db = split_fixture();
        db.execute(
            "INSERT INTO task_details(task_id,json) VALUES(2,'{\"waiting\":\"klient\"}')",
            [],
        )
        .unwrap();
        change(&mut db, "archive", Some(2), None, 500).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        let s = split_session(&mut db, 1, 4000, Some(2), 6000).unwrap();
        let first = s.sessions.iter().find(|x| x.id == 1).unwrap();
        assert_eq!(first.ended_at, Some(4000));
        let active: Vec<_> = s.sessions.iter().filter(|x| x.ended_at.is_none()).collect();
        assert_eq!(active.len(), 1);
        assert_eq!((active[0].task_id, active[0].started_at), (2, 4000));
        let b = s.tasks.iter().find(|t| t.id == 2).unwrap();
        assert!(!b.archived);
        assert_eq!(b.details.waiting, "");
    }
    #[test]
    fn split_rejects_moment_outside_range() {
        let mut db = split_fixture();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        assert!(split_session(&mut db, 1, 1000, None, 5000).is_err());
        assert!(split_session(&mut db, 1, 5000, None, 5000).is_err());
        assert!(split_session(&mut db, 1, 7000, None, 5000).is_err());
        assert!(split_session(&mut db, 99, 2000, None, 5000).is_err());
        change(&mut db, "stop", None, None, 3000).unwrap();
        assert!(split_session(&mut db, 1, 4000, None, 5000).is_err());
        let s = snapshot(&db).unwrap();
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].ended_at, Some(3000));
    }
    #[test]
    fn split_active_rejects_done_target() {
        let mut db = split_fixture();
        change(&mut db, "toggle", Some(2), None, 500).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        assert!(split_session(&mut db, 1, 2000, Some(2), 5000).is_err());
        let s = snapshot(&db).unwrap();
        assert_eq!(s.sessions.len(), 1);
        assert_eq!(s.sessions[0].ended_at, None);
        assert_eq!(s.sessions[0].task_id, 1);
    }
    #[test]
    fn split_rejects_trashed_target_and_session() {
        let mut db = split_fixture();
        change(&mut db, "delete", Some(2), None, 500).unwrap();
        change(&mut db, "start", Some(1), None, 1000).unwrap();
        assert!(split_session(&mut db, 1, 2000, Some(2), 5000).is_err());
        assert!(split_session(&mut db, 1, 2000, Some(99), 5000).is_err());
        change(&mut db, "delete", Some(1), None, 3000).unwrap();
        assert!(split_session(&mut db, 1, 2000, None, 5000).is_err());
        let s = snapshot(&db).unwrap();
        assert!(s.sessions.is_empty());
        assert_eq!(s.trash_sessions.len(), 1);
    }
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
    fn init_is_idempotent_and_creates_indexes_and_notifications_table() {
        let db = Connection::open_in_memory().unwrap();
        init(&db).unwrap();
        db.execute(
            "INSERT INTO notifications_sent(key,at) VALUES('due:1:2026-10-14',1)",
            [],
        )
        .unwrap();
        init(&db).unwrap();
        let names: Vec<String> = db
            .prepare("SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='sessions' ORDER BY name")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert!(names.contains(&"sessions_task".to_owned()));
        assert!(names.contains(&"sessions_started".to_owned()));
        let kept: i64 = db
            .query_row("SELECT COUNT(*) FROM notifications_sent", [], |r| r.get(0))
            .unwrap();
        assert_eq!(kept, 1);
        assert!(db
            .execute(
                "INSERT INTO notifications_sent(key,at) VALUES('due:1:2026-10-14',2)",
                [],
            )
            .is_err());
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
