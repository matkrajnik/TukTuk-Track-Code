//! Aggregates computed in SQL, so the tray and notifications do not need a
//! full snapshot. Mirrors `inDay` from `src/time.ts`: sessions are split at
//! local midnight and an active session counts up to `now`.
use chrono::{Local, NaiveDate, NaiveTime, TimeZone};
use rusqlite::{params, Connection};

/// First instant (Unix ms) of `date` in `tz`. A midnight that falls into a DST
/// gap starts the day at the first existing local time after it.
fn day_start_ms<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> i64 {
    let midnight = date.and_time(NaiveTime::MIN);
    (0..=24 * 60)
        .find_map(|minutes| {
            tz.from_local_datetime(&(midnight + chrono::Duration::minutes(minutes)))
                .earliest()
        })
        .map(|start| start.timestamp_millis())
        .unwrap_or_else(|| midnight.and_utc().timestamp_millis())
}

/// Local-day bounds `[start, end)` in Unix ms.
pub fn day_bounds<Tz: TimeZone>(tz: &Tz, date: NaiveDate) -> (i64, i64) {
    let next = date.succ_opt().unwrap_or(date);
    (day_start_ms(tz, date), day_start_ms(tz, next))
}

/// Measured time on `date` in time zone `tz`, without tasks moved to trash.
pub fn day_total_ms_in<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    date: NaiveDate,
    now: i64,
) -> rusqlite::Result<i64> {
    let (start, end) = day_bounds(tz, date);
    db.query_row(
        "SELECT COALESCE(SUM(MAX(0, MIN(COALESCE(s.ended_at, ?3), ?2) - MAX(s.started_at, ?1))), 0)
         FROM sessions s
         WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
           AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)",
        params![start, end, now],
        |r| r.get(0),
    )
}

/// Measured time on the local calendar day `date`.
pub fn day_total_ms(db: &Connection, date: NaiveDate, now: i64) -> rusqlite::Result<i64> {
    day_total_ms_in(db, &Local, date, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono_tz::Europe::Warsaw;

    const H: i64 = 3_600_000;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn at<Tz: TimeZone>(tz: &Tz, s: &str) -> i64 {
        let local = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
        tz.from_local_datetime(&local)
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    fn setup() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db::change(&mut db, "add", None, Some("A".into()), 0).unwrap();
        db::change(&mut db, "add", None, Some("B".into()), 0).unwrap();
        db
    }
    fn session(db: &Connection, task: i64, start: i64, end: Option<i64>) {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            params![task, start, end],
        )
        .unwrap();
    }

    #[test]
    fn dst_day_has_25_hours_and_sessions_split_at_midnight() {
        let db = setup();
        let (start, end) = day_bounds(&Warsaw, date("2026-10-25"));
        assert_eq!(end - start, 25 * H);
        assert_eq!(
            day_bounds(&Warsaw, date("2026-03-29")).1 - day_bounds(&Warsaw, date("2026-03-29")).0,
            23 * H
        );
        // 22:00 on the 24th to 04:00 on the 25th: 2 h before midnight, then
        // 00:00–04:00 local, which is 5 real hours because 03:00 happens twice.
        session(
            &db,
            1,
            at(&Warsaw, "2026-10-24 22:00"),
            Some(at(&Warsaw, "2026-10-25 04:00")),
        );
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-24"), 0).unwrap(),
            2 * H
        );
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-25"), 0).unwrap(),
            5 * H
        );
        // Active session from 23:00 on the 25th counts until `now`, split at midnight.
        let late = at(&Warsaw, "2026-10-25 23:00");
        let now = at(&Warsaw, "2026-10-26 01:30");
        session(&db, 2, late, None);
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-25"), now).unwrap(),
            6 * H
        );
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-26"), now).unwrap(),
            H + H / 2
        );
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-27"), now).unwrap(),
            0
        );
    }

    #[test]
    fn whole_dst_day_and_trash_are_handled() {
        let db = setup();
        let (start, end) = day_bounds(&Warsaw, date("2026-10-25"));
        session(&db, 1, start - H, Some(end + H));
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-25"), end + 2 * H).unwrap(),
            25 * H
        );
        session(&db, 2, start + H, Some(start + 2 * H));
        let mut db = db;
        db::change(&mut db, "delete", Some(2), None, end + 2 * H).unwrap();
        assert_eq!(
            day_total_ms_in(&db, &Warsaw, date("2026-10-25"), end + 2 * H).unwrap(),
            25 * H
        );
    }

    #[test]
    fn local_wrapper_splits_at_local_midnight() {
        let db = setup();
        let day = date("2026-10-25");
        let (start, end) = day_bounds(&Local, day);
        assert!(end - start >= 23 * H && end - start <= 25 * H);
        session(&db, 1, start - H, Some(start + 2 * H));
        session(&db, 2, end - H, None);
        assert_eq!(day_total_ms(&db, day, end + H).unwrap(), 3 * H);
        assert_eq!(
            day_total_ms(&db, day.succ_opt().unwrap(), end + H).unwrap(),
            H
        );
        assert_eq!(
            day_total_ms(&db, day.pred_opt().unwrap(), end + H).unwrap(),
            H
        );
    }
}
