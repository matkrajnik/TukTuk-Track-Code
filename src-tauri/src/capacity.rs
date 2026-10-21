//! Pojemność dnia ze spotkaniami. Odpowiednik w TypeScript: `src/capacity.ts`.
//!
//! - Pojemność = cel dnia minus minuty spotkań zajmujących czas (`calendar::occupies`),
//!   policzone bez nakładania (suma przedziałów), przycięte do lokalnej doby, nie mniej niż 0.
//! - Całodniowe `oof` oznacza dzień wolny: pojemność 0.
//! - Cel do osiągnięcia mierzonym czasem (tray, powiadomienie o celu) to cel dnia minus
//!   spotkania, których nie pokrywa zapisany pomiar. Spotkanie dodane jako czas
//!   („Dodaj jako czas”) nie zmniejsza więc celu drugi raz.
use crate::calendar::{self, CalendarEvent};
use crate::settings::AppSettings;
use crate::stats;
use chrono::{NaiveDate, TimeZone};
use rusqlite::{params, Connection};

/// Pojemność dnia (ms).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayCapacity {
    pub target_ms: i64,
    pub meetings_ms: i64,
    pub capacity_ms: i64,
    /// Całodniowe „poza biurem”.
    pub off: bool,
}

/// Posortowane, rozłączne przedziały `[start, end)`; puste są pomijane.
pub fn merge(mut spans: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    spans.retain(|(a, b)| b > a);
    spans.sort_unstable();
    let mut out: Vec<(i64, i64)> = Vec::with_capacity(spans.len());
    for (a, b) in spans {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

fn length(spans: &[(i64, i64)]) -> i64 {
    spans.iter().map(|(a, b)| b - a).sum()
}

/// Część wspólna dwóch list rozłącznych, posortowanych przedziałów.
fn intersection(a: &[(i64, i64)], b: &[(i64, i64)]) -> i64 {
    let (mut i, mut j, mut sum) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        let from = a[i].0.max(b[j].0);
        let to = a[i].1.min(b[j].1);
        if to > from {
            sum += to - from;
        }
        if a[i].1 < b[j].1 {
            i += 1;
        } else {
            j += 1;
        }
    }
    sum
}

/// Czy dzień `[from, to)` jest wolny przez całodniowe „poza biurem”.
pub fn off_day(events: &[CalendarEvent], (from, to): (i64, i64)) -> bool {
    events.iter().any(|e| {
        e.all_day && e.show_as == "oof" && calendar::occupies(e) && e.start < to && e.end > from
    })
}

/// Zajęte przedziały spotkań w dniu `[from, to)`: przycięte do doby i złączone.
/// Wydarzenia całodniowe nie są spotkaniami (np. „Praca zdalna”, konferencja);
/// całodniowe „poza biurem” obsługuje `off_day`.
pub fn meeting_spans(events: &[CalendarEvent], (from, to): (i64, i64)) -> Vec<(i64, i64)> {
    merge(
        events
            .iter()
            .filter(|e| !e.all_day && calendar::occupies(e))
            .map(|e| (e.start.max(from), e.end.min(to)))
            .collect(),
    )
}

/// Pojemność dnia z celu (`target_ms`, 0 dla dnia wolnego) i spotkań.
pub fn day_capacity(target_ms: i64, events: &[CalendarEvent], day: (i64, i64)) -> DayCapacity {
    let off = off_day(events, day);
    let meetings_ms = length(&meeting_spans(events, day));
    DayCapacity {
        target_ms,
        meetings_ms,
        capacity_ms: if off {
            0
        } else {
            (target_ms - meetings_ms).max(0)
        },
        off,
    }
}

/// Cel mierzonego czasu: cel dnia minus spotkania niepokryte pomiarem (`sessions`).
pub fn day_goal_ms(
    target_ms: i64,
    events: &[CalendarEvent],
    sessions: &[(i64, i64)],
    day: (i64, i64),
) -> i64 {
    if target_ms <= 0 || off_day(events, day) {
        return 0;
    }
    let meetings = meeting_spans(events, day);
    let worked = merge(
        sessions
            .iter()
            .map(|&(a, b)| (a.max(day.0), b.min(day.1)))
            .collect(),
    );
    let uncovered = length(&meetings) - intersection(&meetings, &worked);
    (target_ms - uncovered).max(0)
}

/// Czy dzień `date` jest wolny przez całodniowe „poza biurem” ze spotkań
/// (`calendar::meetings_between`: Outlook przy połączonym koncie i ręczne).
pub fn off_day_in<Tz: TimeZone>(db: &Connection, tz: &Tz, date: NaiveDate) -> bool {
    let day = stats::day_bounds(tz, date);
    calendar::meetings_between(db, day.0, day.1)
        .map(|events| off_day(&events, day))
        .unwrap_or(false)
}

/// Cel mierzonego czasu dnia `date` z bazy (tray, powiadomienie o celu).
/// Bez spotkań (ręcznych ani z połączonego kalendarza) równy celowi dnia.
pub fn goal_ms_in<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    s: &AppSettings,
    date: NaiveDate,
    now: i64,
) -> rusqlite::Result<i64> {
    let target = i64::from(crate::holidays::day_target_minutes(s, date)) * 60_000;
    if target <= 0 {
        return Ok(target);
    }
    let day = stats::day_bounds(tz, date);
    let events = calendar::meetings_between(db, day.0, day.1).unwrap_or_default();
    if events.is_empty() {
        return Ok(target);
    }
    let mut stmt = db.prepare(
        "SELECT s.started_at, COALESCE(s.ended_at, ?3) FROM sessions s
         WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
           AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)",
    )?;
    let sessions = stmt
        .query_map(params![day.0, day.1, now], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<(i64, i64)>>>()?;
    Ok(day_goal_ms(target, &events, &sessions, day))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use chrono_tz::Europe::Warsaw;

    const MIN: i64 = 60_000;

    fn at(s: &str) -> i64 {
        Warsaw
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap(),
            )
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    fn day(s: &str) -> (i64, i64) {
        stats::day_bounds(&Warsaw, NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap())
    }
    fn event(id: &str, start: &str, end: &str, show_as: &str) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            start: at(start),
            end: at(end),
            subject: id.into(),
            show_as: show_as.into(),
            all_day: false,
            cancelled: false,
            response: "accepted".into(),
            source: crate::calendar::OUTLOOK.into(),
            task_id: None,
        }
    }

    #[test]
    fn overlapping_meetings_count_once() {
        let events = vec![
            event("a", "2026-10-15 09:00", "2026-10-15 10:00", "busy"),
            event("b", "2026-10-15 09:30", "2026-10-15 11:00", "busy"),
            event(
                "c",
                "2026-10-15 13:00",
                "2026-10-15 13:30",
                "workingElsewhere",
            ),
            // Wolne, anulowane i odrzucone nie zajmują czasu.
            event("d", "2026-10-15 14:00", "2026-10-15 15:00", "free"),
            CalendarEvent {
                cancelled: true,
                ..event("e", "2026-10-15 15:00", "2026-10-15 16:00", "busy")
            },
            CalendarEvent {
                response: "declined".into(),
                ..event("f", "2026-10-15 16:00", "2026-10-15 17:00", "busy")
            },
        ];
        let c = day_capacity(480 * MIN, &events, day("2026-10-15"));
        assert_eq!(c.meetings_ms, 150 * MIN);
        assert_eq!(c.capacity_ms, 330 * MIN);
        assert!(!c.off);
        // Spotkania dłuższe niż cel: pojemność nie spada poniżej zera.
        assert_eq!(
            day_capacity(60 * MIN, &events, day("2026-10-15")).capacity_ms,
            0
        );
    }

    #[test]
    fn meeting_across_midnight_is_split_between_days() {
        let events = vec![event("n", "2026-10-15 22:00", "2026-10-16 02:00", "busy")];
        assert_eq!(
            day_capacity(480 * MIN, &events, day("2026-10-15")).meetings_ms,
            120 * MIN
        );
        assert_eq!(
            day_capacity(480 * MIN, &events, day("2026-10-16")).meetings_ms,
            120 * MIN
        );
        // Doba 25-godzinna (zmiana czasu 25.10.2026): przycięcie do lokalnej doby.
        let dst = vec![event("z", "2026-10-24 23:00", "2026-10-25 04:00", "busy")];
        assert_eq!(
            day_capacity(480 * MIN, &dst, day("2026-10-25")).meetings_ms,
            5 * 60 * MIN
        );
    }

    #[test]
    fn all_day_out_of_office_frees_the_day() {
        let (from, to) = day("2026-10-16");
        let oof = CalendarEvent {
            start: from,
            end: to,
            all_day: true,
            ..event("u", "2026-10-16 00:00", "2026-10-16 00:00", "oof")
        };
        let c = day_capacity(480 * MIN, &[oof.clone()], day("2026-10-16"));
        assert!(c.off);
        assert_eq!(c.capacity_ms, 0);
        assert_eq!(
            day_goal_ms(480 * MIN, &[oof.clone()], &[], day("2026-10-16")),
            0
        );
        // Dzień przed i po nie jest wolny.
        assert!(!day_capacity(480 * MIN, &[oof.clone()], day("2026-10-15")).off);
        assert!(!day_capacity(480 * MIN, &[oof.clone()], day("2026-10-17")).off);
        // Całodniowe „zajęty” to nie urlop; anulowany urlop nie zwalnia dnia.
        let busy = CalendarEvent {
            show_as: "busy".into(),
            ..oof.clone()
        };
        assert!(!day_capacity(480 * MIN, &[busy], day("2026-10-16")).off);
        let cancelled = CalendarEvent {
            cancelled: true,
            ..oof
        };
        assert_eq!(
            day_capacity(480 * MIN, &[cancelled], day("2026-10-16")).capacity_ms,
            480 * MIN
        );
    }

    #[test]
    fn tentative_occupies_time() {
        let events = vec![event(
            "t",
            "2026-10-15 13:00",
            "2026-10-15 14:00",
            "tentative",
        )];
        let c = day_capacity(480 * MIN, &events, day("2026-10-15"));
        assert_eq!(c.meetings_ms, 60 * MIN);
        assert_eq!(c.capacity_ms, 420 * MIN);
    }

    #[test]
    fn all_day_event_other_than_oof_is_not_a_meeting() {
        let (from, to) = day("2026-10-16");
        for show_as in ["busy", "workingElsewhere", "tentative"] {
            let all_day = CalendarEvent {
                start: from,
                end: to,
                all_day: true,
                ..event("z", "2026-10-16 00:00", "2026-10-16 00:00", show_as)
            };
            let events = [
                all_day,
                event("a", "2026-10-16 09:00", "2026-10-16 10:00", "busy"),
            ];
            let c = day_capacity(480 * MIN, &events, day("2026-10-16"));
            assert!(!c.off, "{show_as}");
            assert_eq!(c.meetings_ms, 60 * MIN, "{show_as}");
            assert_eq!(c.capacity_ms, 420 * MIN, "{show_as}");
            assert_eq!(
                day_goal_ms(480 * MIN, &events, &[], day("2026-10-16")),
                420 * MIN,
                "{show_as}"
            );
        }
    }

    #[test]
    fn goal_does_not_subtract_meetings_already_measured() {
        let events = vec![
            event("a", "2026-10-15 09:00", "2026-10-15 10:00", "busy"),
            event("b", "2026-10-15 13:00", "2026-10-15 14:00", "busy"),
        ];
        let d = day("2026-10-15");
        assert_eq!(day_goal_ms(480 * MIN, &events, &[], d), 360 * MIN);
        // Spotkanie 9–10 dodane jako czas: tylko 13–14 zmniejsza cel.
        let measured = [(at("2026-10-15 09:00"), at("2026-10-15 10:00"))];
        assert_eq!(day_goal_ms(480 * MIN, &events, &measured, d), 420 * MIN);
        // Dzień bez celu zostaje bez celu.
        assert_eq!(day_goal_ms(0, &events, &[], d), 0);
    }

    #[test]
    fn goal_from_database_requires_connected_calendar() {
        let mut db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        let s = crate::settings::load(&db);
        let date = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let now = at("2026-10-15 12:00");
        let meeting = event("a", "2026-10-15 09:00", "2026-10-15 11:30", "busy");
        calendar::replace_range(
            &mut db,
            at("2026-10-12 00:00"),
            at("2026-11-01 00:00"),
            &[meeting],
            now,
        )
        .unwrap();
        assert_eq!(goal_ms_in(&db, &Warsaw, &s, date, now).unwrap(), 480 * MIN);
        calendar::set_test_email(&db, "jan@firma.example");
        assert_eq!(goal_ms_in(&db, &Warsaw, &s, date, now).unwrap(), 330 * MIN);
        // Sobota: bez celu niezależnie od spotkań.
        let saturday = NaiveDate::from_ymd_opt(2026, 10, 17).unwrap();
        assert_eq!(goal_ms_in(&db, &Warsaw, &s, saturday, now).unwrap(), 0);
    }
}
