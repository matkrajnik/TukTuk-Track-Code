//! Historia powiadomień (dzwoneczek w górnym pasku).
//!
//! Każde powiadomienie z `notify::show` (terminy, spotkania, przypomnienia, nowe
//! zgłoszenia z Jiry i Redmine, agenci) trafia do tabeli `notification_log`, także
//! wtedy, gdy blok skupienia odkłada toast na później. Wpis ma tytuł, treść, cel
//! kliknięcia (`notify::View` jako JSON) i znacznik „przeczytane”. Okno czyta listę
//! komendą `notification_log`, zmiany ogłasza zdarzenie `notification-log-changed`.
//!
//! Lista jest ograniczona: najwyżej `KEEP` wpisów i nie starsze niż `KEEP_MS`.

use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::notify::Message;
use crate::Database;

/// Zdarzenie z nową listą po każdej zmianie historii.
pub const EVENT: &str = "notification-log-changed";
/// Najwięcej wpisów w historii.
const KEEP: i64 = 200;
/// Wpisy starsze niż 30 dni znikają.
const KEEP_MS: i64 = 30 * 24 * 3_600_000;

/// Wpis historii dla okna.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub at: i64,
    pub title: String,
    pub body: String,
    /// Ładunek `navigate` (`notify::View`); `null`, gdy zapisany JSON jest uszkodzony.
    pub open: serde_json::Value,
    pub read: bool,
}

/// Zapisuje powiadomienie i przycina historię. Puste tytuł i treść są pomijane.
pub fn record(db: &Connection, message: &Message, now: i64) -> rusqlite::Result<bool> {
    if message.title.trim().is_empty() && message.body.trim().is_empty() {
        return Ok(false);
    }
    let open = serde_json::to_string(&message.open).unwrap_or_else(|_| "{}".into());
    db.execute(
        "INSERT INTO notification_log(at, title, body, open, read) VALUES(?1, ?2, ?3, ?4, 0)",
        params![now, message.title, message.body, open],
    )?;
    prune(db, now)?;
    Ok(true)
}

/// Usuwa wpisy starsze niż `KEEP_MS` i wszystko ponad `KEEP` najnowszych.
fn prune(db: &Connection, now: i64) -> rusqlite::Result<usize> {
    db.execute(
        "DELETE FROM notification_log WHERE at < ?1 OR id NOT IN
            (SELECT id FROM notification_log ORDER BY at DESC, id DESC LIMIT ?2)",
        params![now - KEEP_MS, KEEP],
    )
}

/// Historia od najnowszych.
pub fn list(db: &Connection) -> rusqlite::Result<Vec<Entry>> {
    let mut stmt = db.prepare(
        "SELECT id, at, title, body, open, read FROM notification_log
         ORDER BY at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([KEEP], |r| {
        let open: String = r.get(4)?;
        Ok(Entry {
            id: r.get(0)?,
            at: r.get(1)?,
            title: r.get(2)?,
            body: r.get(3)?,
            open: serde_json::from_str(&open).unwrap_or(serde_json::Value::Null),
            read: r.get::<_, i64>(5)? != 0,
        })
    })?;
    rows.collect()
}

/// Oznacza jako przeczytany jeden wpis (`Some(id)`) albo wszystkie (`None`).
pub fn mark_read(db: &Connection, id: Option<i64>) -> rusqlite::Result<usize> {
    match id {
        Some(id) => db.execute(
            "UPDATE notification_log SET read=1 WHERE id=?1 AND read=0",
            [id],
        ),
        None => db.execute("UPDATE notification_log SET read=1 WHERE read=0", []),
    }
}

/// Czyści całą historię.
pub fn clear(db: &Connection) -> rusqlite::Result<usize> {
    db.execute("DELETE FROM notification_log", [])
}

fn with_db<R>(
    app: &tauri::AppHandle,
    f: impl FnOnce(&Connection) -> rusqlite::Result<R>,
) -> Result<R, String> {
    let database = app.state::<Database>();
    let db = database.0.lock().map_err(|e| e.to_string())?;
    f(&db).map_err(|e| format!("Nie udało się odczytać historii powiadomień: {e}"))
}

/// Zapis z `notify::show`. Błąd bazy nie blokuje toastu, trafia tylko do logu.
pub fn record_now(app: &tauri::AppHandle, message: &Message) {
    let Some(database) = app.try_state::<Database>() else {
        return;
    };
    let result = match database.0.lock() {
        Ok(db) => record(&db, message, crate::db::now()).and_then(|saved| {
            if saved {
                list(&db).map(Some)
            } else {
                Ok(None)
            }
        }),
        Err(e) => {
            eprintln!("Historia powiadomień: zablokowana baza: {e}");
            return;
        }
    };
    match result {
        Ok(Some(entries)) => {
            let _ = app.emit(EVENT, entries);
        }
        Ok(None) => {}
        Err(e) => eprintln!("Historia powiadomień: nie zapisano wpisu: {e}"),
    }
}

fn changed(app: &tauri::AppHandle) -> Result<Vec<Entry>, String> {
    let entries = with_db(app, list)?;
    let _ = app.emit(EVENT, entries.clone());
    Ok(entries)
}

#[tauri::command]
pub fn notification_log(app: tauri::AppHandle) -> Result<Vec<Entry>, String> {
    with_db(&app, list)
}

/// „Przeczytane”: jeden wpis (`id`) albo wszystkie (`id` puste).
#[tauri::command]
pub fn notification_log_read(app: tauri::AppHandle, id: Option<i64>) -> Result<Vec<Entry>, String> {
    with_db(&app, |db| mark_read(db, id))?;
    changed(&app)
}

#[tauri::command]
pub fn notification_log_clear(app: tauri::AppHandle) -> Result<Vec<Entry>, String> {
    with_db(&app, clear)?;
    changed(&app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::View;

    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        db
    }

    fn message(title: &str) -> Message {
        Message {
            key: format!("k:{title}"),
            title: title.into(),
            body: format!("Treść {title}"),
            actions: Vec::new(),
            open: View {
                task: Some(7),
                ..View::tasks()
            },
        }
    }

    #[test]
    fn records_newest_first_with_navigation_target_and_unread() {
        let db = setup();
        assert!(record(&db, &message("Spotkanie"), 1_000).unwrap());
        assert!(record(&db, &message("Nowe zadanie z Jiry"), 2_000).unwrap());
        let entries = list(&db).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            ["Nowe zadanie z Jiry", "Spotkanie"]
        );
        assert!(entries.iter().all(|e| !e.read));
        assert_eq!(entries[0].body, "Treść Nowe zadanie z Jiry");
        assert_eq!(entries[0].open["view"], "tasks");
        assert_eq!(entries[0].open["task"], 7);
    }

    #[test]
    fn skips_empty_messages() {
        let db = setup();
        let mut empty = message("");
        empty.body = " ".into();
        assert!(!record(&db, &empty, 1).unwrap());
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn marks_one_or_all_as_read_and_clears() {
        let db = setup();
        record(&db, &message("A"), 1).unwrap();
        record(&db, &message("B"), 2).unwrap();
        record(&db, &message("C"), 3).unwrap();
        let b = list(&db).unwrap()[1].id;
        assert_eq!(mark_read(&db, Some(b)).unwrap(), 1);
        assert_eq!(mark_read(&db, Some(b)).unwrap(), 0);
        let read: Vec<bool> = list(&db).unwrap().iter().map(|e| e.read).collect();
        assert_eq!(read, [false, true, false]);
        assert_eq!(mark_read(&db, None).unwrap(), 2);
        assert!(list(&db).unwrap().iter().all(|e| e.read));
        clear(&db).unwrap();
        assert!(list(&db).unwrap().is_empty());
    }

    #[test]
    fn keeps_limited_number_of_recent_entries() {
        let db = setup();
        let day = 24 * 3_600_000;
        record(&db, &message("stare"), 0).unwrap();
        let now = 31 * day;
        for i in 0..KEEP + 5 {
            record(&db, &message(&format!("n{i}")), now + i).unwrap();
        }
        let entries = list(&db).unwrap();
        assert_eq!(entries.len() as i64, KEEP);
        assert!(entries.iter().all(|e| e.title != "stare"));
        assert_eq!(entries[0].title, format!("n{}", KEEP + 4));
        assert_eq!(entries.last().unwrap().title, "n5");
    }

    #[test]
    fn corrupted_target_reads_as_null() {
        let db = setup();
        db.execute(
            "INSERT INTO notification_log(at, title, body, open, read) VALUES(1, 'x', '', '{', 0)",
            [],
        )
        .unwrap();
        assert_eq!(list(&db).unwrap()[0].open, serde_json::Value::Null);
    }
}
