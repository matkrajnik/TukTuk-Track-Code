use crate::db::{self, Snapshot};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Details {
    pub priority: i32,
    pub planned_day: String,
    pub due_day: String,
    pub project: String,
    pub waiting: String,
    pub notes: BTreeMap<String, String>,
    pub estimate_minutes: u32,
    pub checklist: Vec<CheckItem>,
    pub repeat_days: Vec<u32>,
    pub repeat_from: String,
    /// Manual order within a priority band (stage 3); 0 for older records.
    pub order: i64,
    /// Optional fixed start of the plan block: empty or `HH:MM`.
    pub planned_start: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct CheckItem {
    pub text: String,
    pub done: bool,
}

pub fn date_valid(value: &str) -> bool {
    if value.len() != 10 {
        return false;
    }
    let parts: Vec<_> = value.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return false;
    }
    let nums: Option<Vec<u32>> = parts.iter().map(|p| p.parse().ok()).collect();
    let Some(n) = nums else {
        return false;
    };
    if n[0] < 1970 || n[1] < 1 || n[1] > 12 {
        return false;
    }
    let leap = n[0] % 4 == 0 && (n[0] % 100 != 0 || n[0] % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    n[2] >= 1 && n[2] <= days[(n[1] - 1) as usize]
}

pub fn save(db: &mut Connection, id: i64, mut details: Details) -> Result<Snapshot, String> {
    details.project = details.project.trim().into();
    details.waiting = details.waiting.trim().into();
    if !(-1..=1).contains(&details.priority)
        || details.estimate_minutes > 600_000
        || details.checklist.len() > 100
        || details
            .checklist
            .iter()
            .any(|item| item.text.trim().is_empty() || item.text.chars().count() > 240)
        || details.repeat_days.iter().any(|day| *day < 1 || *day > 7)
        || (!details.repeat_from.is_empty() && !date_valid(&details.repeat_from))
        || [&details.planned_day, &details.due_day]
            .iter()
            .any(|d| !d.is_empty() && !date_valid(d))
        || (!details.planned_start.is_empty()
            && !crate::settings::valid_time(&details.planned_start))
        || details.project.chars().count() > 80
        || details.waiting.chars().count() > 300
        || details
            .notes
            .iter()
            .any(|(day, text)| !date_valid(day) || text.chars().count() > 4000)
    {
        return Err("Niepoprawne daty lub zbyt długa treść planu/notatki.".into());
    }
    details.notes.retain(|_, note| !note.trim().is_empty());
    if !details.repeat_days.is_empty() && details.repeat_from.is_empty() {
        details.repeat_from = chrono::Local::now().format("%Y-%m-%d").to_string();
    }
    if !details.repeat_days.is_empty() && details.planned_day.is_empty() {
        details.planned_day = details.repeat_from.clone();
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL))",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Nie znaleziono zadania.".into());
    }
    // Blocking an actively measured task closes its session at the same time.
    if !details.waiting.is_empty() {
        tx.execute(
            "UPDATE sessions SET ended_at=MAX(started_at,?1) WHERE task_id=?2 AND ended_at IS NULL",
            params![db::now(), id],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.execute("INSERT INTO task_details(task_id,json) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET json=excluded.json", params![id, serde_json::to_string(&details).map_err(|e| e.to_string())?]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

/// Entry of `reorder_tasks`: new manual position and priority band of a task.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ReorderEntry {
    pub id: i64,
    pub order: i64,
    pub priority: i32,
}

/// Sets only `$.order` and `$.priority` of each task in one transaction, like
/// `set_priority`; the rest of the details JSON (an unsaved planning form included)
/// stays untouched.
pub fn reorder(db: &mut Connection, entries: &[ReorderEntry]) -> Result<Snapshot, String> {
    if entries.len() > 10_000
        || entries
            .iter()
            .any(|e| !(-1..=1).contains(&e.priority) || !(0..=1_000_000).contains(&e.order))
    {
        return Err("Niepoprawna kolejność zadań.".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for entry in entries {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL))",
                [entry.id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("Nie znaleziono zadania.".into());
        }
        tx.execute(
            "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('priority',?2,'order',?3)) ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.priority',?2,'$.order',?3)",
            params![entry.id, entry.priority, entry.order],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

/// Moves a task on the week board: sets only `$.plannedDay` (empty = back to
/// "Do zaplanowania") and `$.order`, like `reorder`; the rest of the details
/// JSON (an unsaved planning form included) stays untouched.
pub fn plan_task(db: &mut Connection, id: i64, day: &str, order: i64) -> Result<Snapshot, String> {
    if (!day.is_empty() && !date_valid(day)) || !(0..=1_000_000).contains(&order) {
        return Err("Niepoprawny dzień planu.".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks WHERE id=?1 AND id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL))",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Nie znaleziono zadania.".into());
    }
    tx.execute(
        "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('plannedDay',?2,'order',?3)) ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.plannedDay',?2,'$.order',?3)",
        params![id, day, order],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

pub fn clear_waiting(db: &Connection, id: i64) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE task_details SET json=json_set(json,'$.waiting','') WHERE task_id=?1",
        [id],
    )?;
    Ok(())
}

pub fn add_session(
    db: &mut Connection,
    task_id: i64,
    start: i64,
    end: i64,
    now: i64,
) -> Result<Snapshot, String> {
    if start < 0 || end <= start || end > now {
        return Err("Koniec musi być późniejszy niż początek i nie może być w przyszłości.".into());
    }
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let overlaps: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE started_at<?2 AND (ended_at IS NULL OR ended_at>?1) AND (ended_at IS NULL OR ended_at>started_at))", params![start,end], |r| r.get(0)).map_err(|e| e.to_string())?;
    if overlaps {
        return Err("Ten przedział nakłada się na inny pomiar. Popraw godziny.".into());
    }
    tx.execute(
        "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
        params![task_id, start, end],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(db).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plans_preserve_history_and_manual_sessions_validate() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db::change(&mut db, "add", None, Some("Old task".into()), 0).unwrap();
        add_session(&mut db, 1, 100, 200, 1000).unwrap();
        assert!(add_session(&mut db, 1, 150, 250, 1000).is_err());
        assert!(add_session(&mut db, 1, 200, 2000, 1000).is_err());
        assert!(add_session(&mut db, 99, 200, 300, 1000).is_err());
        add_session(&mut db, 1, 200, 300, 1000).unwrap();
        let mut d = Details {
            priority: 1,
            planned_day: "2026-10-14".into(),
            project: "WMS".into(),
            ..Details::default()
        };
        d.notes.insert("2026-10-14".into(), "Import".into());
        save(&mut db, 1, d.clone()).unwrap();
        db::init(&db).unwrap();
        let snap = db::snapshot(&db).unwrap();
        assert_eq!(snap.tasks[0].details.priority, 1);
        assert_eq!(snap.tasks[0].details.notes["2026-10-14"], "Import");
        assert_eq!(snap.sessions.len(), 2);
        d.planned_day = "2026-02-30".into();
        assert!(save(&mut db, 1, d).is_err());
        db::change(&mut db, "start", Some(1), None, 1000).unwrap();
        save(
            &mut db,
            1,
            Details {
                waiting: "Klient".into(),
                ..Details::default()
            },
        )
        .unwrap();
        assert!(db::snapshot(&db)
            .unwrap()
            .sessions
            .iter()
            .all(|s| s.ended_at.is_some()));
        db::change(&mut db, "start", Some(1), None, db::now()).unwrap();
        assert!(db::snapshot(&db).unwrap().tasks[0]
            .details
            .waiting
            .is_empty());
    }
    #[test]
    fn old_details_json_reads_and_new_fields_round_trip() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db::change(&mut db, "add", None, Some("Old".into()), 0).unwrap();
        db::change(&mut db, "add", None, Some("New".into()), 0).unwrap();
        // A 0.3 record: no `order` and no `plannedStart`.
        db.execute(
            "INSERT INTO task_details(task_id,json) VALUES(1,?1)",
            [r#"{"priority":1,"plannedDay":"2026-10-14","dueDay":"","project":"WMS","waiting":"","notes":{},"estimateMinutes":30,"checklist":[],"repeatDays":[],"repeatFrom":""}"#],
        )
        .unwrap();
        let snap = db::snapshot(&db).unwrap();
        let old = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!(old.details.priority, 1);
        assert_eq!(old.details.order, 0);
        assert_eq!(old.details.planned_start, "");
        let mut d = Details {
            order: 3,
            planned_start: "09:30".into(),
            ..Details::default()
        };
        save(&mut db, 2, d.clone()).unwrap();
        let snap = db::snapshot(&db).unwrap();
        let new = snap.tasks.iter().find(|t| t.id == 2).unwrap();
        assert_eq!(new.details.order, 3);
        assert_eq!(new.details.planned_start, "09:30");
        let json: String = db
            .query_row("SELECT json FROM task_details WHERE task_id=2", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(json.contains(r#""order":3"#) && json.contains(r#""plannedStart":"09:30""#));
        for bad in ["9:30", "24:00", "12:60", "ab:cd"] {
            d.planned_start = bad.into();
            assert!(save(&mut db, 2, d.clone()).is_err(), "{bad}");
        }
    }
    #[test]
    fn reorder_sets_only_order_and_priority() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["A", "B", "C"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        let mut d = Details {
            planned_day: "2026-10-14".into(),
            project: "WMS".into(),
            waiting: "Klient".into(),
            estimate_minutes: 90,
            order: 7,
            planned_start: "09:30".into(),
            ..Details::default()
        };
        d.notes.insert("2026-10-14".into(), "Notatka".into());
        d.checklist.push(CheckItem {
            text: "Krok".into(),
            done: true,
        });
        save(&mut db, 1, d).unwrap();
        // Keys unknown to this version must survive as well.
        db.execute(
            "UPDATE task_details SET json=json_set(json,'$.future','x') WHERE task_id=1",
            [],
        )
        .unwrap();
        let entry = |id, order, priority| ReorderEntry {
            id,
            order,
            priority,
        };
        let snap = reorder(&mut db, &[entry(1, 2, 1), entry(3, 1, 1)]).unwrap();
        let one = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!((one.details.order, one.details.priority), (2, 1));
        assert_eq!(one.details.planned_day, "2026-10-14");
        assert_eq!(one.details.project, "WMS");
        assert_eq!(one.details.waiting, "Klient");
        assert_eq!(one.details.estimate_minutes, 90);
        assert_eq!(one.details.planned_start, "09:30");
        assert_eq!(one.details.notes["2026-10-14"], "Notatka");
        assert!(one.details.checklist[0].done);
        let json: String = db
            .query_row("SELECT json FROM task_details WHERE task_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(json.contains(r#""future":"x""#));
        // A task without a details row gets one with just the two fields.
        let three = snap.tasks.iter().find(|t| t.id == 3).unwrap();
        assert_eq!((three.details.order, three.details.priority), (1, 1));
        let two = snap.tasks.iter().find(|t| t.id == 2).unwrap();
        assert_eq!((two.details.order, two.details.priority), (0, 0));
        // Invalid input and tasks in the trash are rejected without partial writes.
        assert!(reorder(&mut db, &[entry(2, 1, 2)]).is_err());
        assert!(reorder(&mut db, &[entry(2, -1, 0)]).is_err());
        assert!(reorder(&mut db, &[entry(99, 1, 0)]).is_err());
        db::change(&mut db, "delete", Some(2), None, 0).unwrap();
        assert!(reorder(&mut db, &[entry(1, 5, 0), entry(2, 1, 0)]).is_err());
        let snap = db::snapshot(&db).unwrap();
        let one = snap.tasks.iter().find(|t| t.id == 1).unwrap();
        assert_eq!((one.details.order, one.details.priority), (2, 1));
    }
    #[test]
    fn plan_task_sets_only_planned_day_and_order() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["A", "B"] {
            db::change(&mut db, "add", None, Some(title.into()), 0).unwrap();
        }
        let mut d = Details {
            priority: 1,
            planned_day: "2026-10-12".into(),
            due_day: "2026-10-20".into(),
            project: "WMS".into(),
            waiting: "Klient".into(),
            estimate_minutes: 90,
            order: 7,
            planned_start: "09:30".into(),
            repeat_days: vec![3],
            repeat_from: "2026-10-01".into(),
            ..Details::default()
        };
        d.notes.insert("2026-10-12".into(), "Notatka".into());
        d.checklist.push(CheckItem {
            text: "Krok".into(),
            done: true,
        });
        save(&mut db, 1, d).unwrap();
        // Keys unknown to this version must survive as well.
        db.execute(
            "UPDATE task_details SET json=json_set(json,'$.future','x') WHERE task_id=1",
            [],
        )
        .unwrap();
        let snap = plan_task(&mut db, 1, "2026-10-14", 3).unwrap();
        let one = &snap.tasks.iter().find(|t| t.id == 1).unwrap().details;
        assert_eq!((one.planned_day.as_str(), one.order), ("2026-10-14", 3));
        assert_eq!(one.priority, 1);
        assert_eq!(one.due_day, "2026-10-20");
        assert_eq!(one.project, "WMS");
        assert_eq!(one.waiting, "Klient");
        assert_eq!(one.estimate_minutes, 90);
        assert_eq!(one.planned_start, "09:30");
        assert_eq!(one.repeat_days, vec![3]);
        assert_eq!(one.repeat_from, "2026-10-01");
        assert_eq!(one.notes["2026-10-12"], "Notatka");
        assert!(one.checklist[0].done);
        let json: String = db
            .query_row("SELECT json FROM task_details WHERE task_id=1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(json.contains(r#""future":"x""#));
        // Back to "Do zaplanowania": an empty day.
        let snap = plan_task(&mut db, 1, "", 3).unwrap();
        let one = &snap.tasks.iter().find(|t| t.id == 1).unwrap().details;
        assert_eq!(one.planned_day, "");
        assert_eq!(one.project, "WMS");
        // A task without a details row gets one with just the two fields.
        let snap = plan_task(&mut db, 2, "2026-10-15", 1).unwrap();
        let two = &snap.tasks.iter().find(|t| t.id == 2).unwrap().details;
        assert_eq!(
            (two.planned_day.as_str(), two.order, two.priority),
            ("2026-10-15", 1, 0)
        );
        // Invalid input and tasks in the trash are rejected without writes.
        assert!(plan_task(&mut db, 2, "2026-02-30", 1).is_err());
        assert!(plan_task(&mut db, 2, "14.10.2026", 1).is_err());
        assert!(plan_task(&mut db, 2, "2026-10-16", -1).is_err());
        assert!(plan_task(&mut db, 99, "2026-10-16", 1).is_err());
        db::change(&mut db, "delete", Some(2), None, 0).unwrap();
        assert!(plan_task(&mut db, 2, "2026-10-16", 1).is_err());
        let day: String = db
            .query_row(
                "SELECT json_extract(json,'$.plannedDay') FROM task_details WHERE task_id=2",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(day, "2026-10-15");
    }
}
