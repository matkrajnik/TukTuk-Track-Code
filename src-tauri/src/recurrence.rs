use crate::{db, planner::Details};
use chrono::{Datelike, Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

pub fn generate(db: &mut Connection, today: NaiveDate) -> Result<(), String> {
    let sources=db.prepare("SELECT t.id,t.title,d.json FROM tasks t JOIN task_details d ON d.task_id=t.id LEFT JOIN task_lifecycle l ON l.task_id=t.id WHERE l.deleted_at IS NULL AND COALESCE(l.archived,0)=0").map_err(|e|e.to_string())?.query_map([],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    for (id, title, json) in sources {
        let details: Details = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        if details.repeat_days.is_empty() {
            continue;
        }
        let from = NaiveDate::parse_from_str(&details.repeat_from, "%Y-%m-%d")
            .map_err(|_| "Niepoprawny początek cyklu")?;
        let last: Option<String> = tx
            .query_row(
                "SELECT last_day FROM recurrence_state WHERE task_id=?1",
                [id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let mut date = last
            .and_then(|v| NaiveDate::parse_from_str(&v, "%Y-%m-%d").ok())
            .and_then(|d| d.succ_opt())
            .unwrap_or(from)
            .max(from)
            .max(today - Duration::days(365));
        while date <= today {
            let key = date.to_string();
            let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM recurrence_occurrences WHERE source_id=?1 AND day=?2)",params![id,key],|r|r.get(0)).map_err(|e|e.to_string())?;
            if details
                .repeat_days
                .contains(&date.weekday().number_from_monday())
                && !exists
            {
                let instance = if details.planned_day == key {
                    id
                } else {
                    let mut copy = details.clone();
                    copy.repeat_days.clear();
                    copy.repeat_from.clear();
                    copy.notes.clear();
                    copy.waiting.clear();
                    copy.planned_day = key.clone();
                    copy.due_day.clear();
                    for item in &mut copy.checklist {
                        item.done = false;
                    }
                    tx.execute(
                        "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
                        params![title, db::now()],
                    )
                    .map_err(|e| e.to_string())?;
                    let new_id = tx.last_insert_rowid();
                    tx.execute(
                        "INSERT INTO task_details(task_id,json) VALUES(?1,?2)",
                        params![
                            new_id,
                            serde_json::to_string(&copy).map_err(|e| e.to_string())?
                        ],
                    )
                    .map_err(|e| e.to_string())?;
                    new_id
                };
                tx.execute(
                    "INSERT INTO recurrence_occurrences(source_id,day,task_id) VALUES(?1,?2,?3)",
                    params![id, key, instance],
                )
                .map_err(|e| e.to_string())?;
            }
            date = date.succ_opt().ok_or("Data poza zakresem")?;
        }
        if from <= today {
            tx.execute("INSERT INTO recurrence_state(task_id,last_day) VALUES(?1,?2) ON CONFLICT(task_id) DO UPDATE SET last_day=excluded.last_day",params![id,today.to_string()]).map_err(|e|e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeats_once_per_selected_day_and_preserves_history() {
        let mut conn = Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        db::change(&mut conn, "add", None, Some("Daily".into()), 0).unwrap();
        crate::planner::save(
            &mut conn,
            1,
            Details {
                repeat_days: vec![1, 3, 5],
                repeat_from: "2026-10-12".into(),
                planned_day: "2026-10-12".into(),
                ..Details::default()
            },
        )
        .unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        generate(&mut conn, today).unwrap();
        generate(&mut conn, today).unwrap();
        let s = db::snapshot(&conn).unwrap();
        assert_eq!(s.tasks.len(), 3);
        assert_eq!(s.tasks[0].details.planned_day, "2026-10-16");
        assert!(s.tasks[0].details.repeat_days.is_empty());
        db::change(&mut conn, "archive", Some(1), None, 0).unwrap();
        generate(&mut conn, today + Duration::days(7)).unwrap();
        assert_eq!(db::snapshot(&conn).unwrap().tasks.len(), 3);
    }
}
