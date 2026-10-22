//! Ręczne spotkania (PLAN_BACKLOG.md, B12): spotkania spoza Outlooka (telefon,
//! spotkanie u klienta, kalendarz innej firmy, praca bez połączonego konta).
//!
//! Leżą w osobnej tabeli `manual_meetings` (`CREATE TABLE IF NOT EXISTS`
//! w `db::init`), nie w `calendar_events`, więc pobieranie z Microsoft 365
//! (`calendar::replace_range`) ani rozłączenie konta (`calendar::forget_account`)
//! ich nie kasują. Identyfikator ma prefiks `manual:`, zadanie jest opcjonalne
//! (`ON DELETE SET NULL`), a kopia SQLite obejmuje tabelę, bo kopiuje całą bazę.
//! Spotkania łączy z Outlookiem `calendar::meetings_between`. Bez cykli.
use crate::calendar::{self, CalendarEvent};
use crate::{db, stats, Database};
use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use tauri::Emitter;

/// Źródło ręcznych spotkań w `CalendarEvent::source`.
pub const MANUAL: &str = "manual";
/// Prefiks identyfikatora ręcznego spotkania.
pub const PREFIX: &str = "manual:";
/// Typy spotkania: Zajęte, Wstępnie, Poza biurem, Praca gdzie indziej.
pub const SHOW_AS: [&str; 4] = ["busy", "tentative", "oof", "workingElsewhere"];
/// Najdłuższy temat (znaki).
pub const MAX_SUBJECT: usize = 200;

/// Dane formularza `MeetingEditor.vue`; czasy w Unix ms.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInput {
    pub start: i64,
    pub end: i64,
    pub subject: String,
    #[serde(default = "busy")]
    pub show_as: String,
    #[serde(default)]
    pub all_day: bool,
    #[serde(default)]
    pub task_id: Option<i64>,
}
fn busy() -> String {
    "busy".into()
}

/// Spotkanie po walidacji: temat bez białych znaków na końcach, całodniowe
/// od lokalnej północy do następnej.
#[derive(Debug, PartialEq)]
struct Valid {
    start: i64,
    end: i64,
    subject: String,
    show_as: String,
    all_day: bool,
    task_id: Option<i64>,
}

fn local_date<Tz: TimeZone>(tz: &Tz, ms: i64) -> Option<NaiveDate> {
    tz.timestamp_millis_opt(ms).single().map(|d| d.date_naive())
}

/// Walidacja z B12: temat 1–200 znaków, typ z listy, koniec po początku i oba
/// w tym samym lokalnym dniu (koniec o północy kończy ten dzień) albo
/// „Cały dzień” (cały lokalny dzień początku), zadanie istniejące i nie w koszu.
fn validate<Tz: TimeZone>(db: &Connection, tz: &Tz, input: &MeetingInput) -> Result<Valid, String> {
    let subject = input.subject.trim();
    if subject.is_empty() {
        return Err("Podaj temat spotkania.".into());
    }
    if subject.chars().count() > MAX_SUBJECT {
        return Err(format!(
            "Temat spotkania może mieć najwyżej {MAX_SUBJECT} znaków."
        ));
    }
    if !SHOW_AS.contains(&input.show_as.as_str()) {
        return Err("Niepoprawny typ spotkania.".into());
    }
    let date = local_date(tz, input.start)
        .filter(|_| input.start >= 0)
        .ok_or("Niepoprawny początek spotkania.")?;
    let (start, end) = if input.all_day {
        stats::day_bounds(tz, date)
    } else {
        if input.end <= input.start {
            return Err("Koniec spotkania musi być po początku.".into());
        }
        if local_date(tz, input.end - 1) != Some(date) {
            return Err(
                "Początek i koniec spotkania muszą być tego samego dnia. Dla dłuższego zaznacz „Cały dzień”."
                    .into(),
            );
        }
        (input.start, input.end)
    };
    if let Some(task) = input.task_id {
        let exists: bool = db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM tasks t WHERE t.id=?1 AND NOT EXISTS(SELECT 1 FROM task_lifecycle l WHERE l.task_id=t.id AND l.deleted_at IS NOT NULL))",
                [task],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if !exists {
            return Err("Wybrane zadanie nie istnieje.".into());
        }
    }
    Ok(Valid {
        start,
        end,
        subject: subject.to_owned(),
        show_as: input.show_as.clone(),
        all_day: input.all_day,
        task_id: input.task_id,
    })
}

fn read_only(id: &str) -> Result<(), String> {
    if id.starts_with(PREFIX) {
        Ok(())
    } else {
        Err("Spotkania z Outlooka są tylko do odczytu.".into())
    }
}

const COLUMNS: &str = "id,start,\"end\",subject,show_as,all_day,task_id";
fn row(r: &rusqlite::Row) -> rusqlite::Result<CalendarEvent> {
    Ok(CalendarEvent {
        id: r.get(0)?,
        start: r.get(1)?,
        end: r.get(2)?,
        subject: r.get(3)?,
        show_as: r.get(4)?,
        all_day: r.get(5)?,
        cancelled: false,
        response: "organizer".into(),
        source: MANUAL.into(),
        task_id: r.get(6)?,
    })
}

/// Ręczne spotkanie o identyfikatorze `id`.
pub fn get(db: &Connection, id: &str) -> Result<Option<CalendarEvent>, String> {
    db.query_row(
        &format!("SELECT {COLUMNS} FROM manual_meetings WHERE id=?1"),
        [id],
        row,
    )
    .optional()
    .map_err(|e| e.to_string())
}

/// Ręczne spotkania nakładające się na `[from, to)`, według początku
/// (ten sam warunek co `calendar::events_between`).
pub fn between(db: &Connection, from: i64, to: i64) -> Result<Vec<CalendarEvent>, String> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT {COLUMNS} FROM manual_meetings
             WHERE start < ?2 AND (\"end\" > ?1 OR (\"end\" = start AND start >= ?1)) ORDER BY start, \"end\", id"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to], row)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

/// Dodaje ręczne spotkanie; identyfikator `manual:<czas>` (z dopiskiem przy
/// dwóch spotkaniach w tej samej milisekundzie).
pub fn add<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    input: &MeetingInput,
    now: i64,
) -> Result<CalendarEvent, String> {
    let v = validate(db, tz, input)?;
    let mut id = format!("{PREFIX}{now}");
    let mut n = 1;
    while get(db, &id)?.is_some() {
        n += 1;
        id = format!("{PREFIX}{now}-{n}");
    }
    db.execute(
        "INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,task_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)",
        params![id, v.start, v.end, v.subject, v.show_as, v.all_day, v.task_id, now],
    )
    .map_err(|e| e.to_string())?;
    get(db, &id)?.ok_or_else(|| "Nie udało się zapisać spotkania.".into())
}

/// Zmienia ręczne spotkanie; spotkania z Outlooka są tylko do odczytu.
pub fn update<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    id: &str,
    input: &MeetingInput,
    now: i64,
) -> Result<CalendarEvent, String> {
    read_only(id)?;
    let v = validate(db, tz, input)?;
    let changed = db
        .execute(
            "UPDATE manual_meetings SET start=?2,\"end\"=?3,subject=?4,show_as=?5,all_day=?6,task_id=?7,updated_at=?8 WHERE id=?1",
            params![id, v.start, v.end, v.subject, v.show_as, v.all_day, v.task_id, now],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Nie ma takiego spotkania.".into());
    }
    get(db, id)?.ok_or_else(|| "Nie ma takiego spotkania.".into())
}

/// Usuwa ręczne spotkanie; spotkania z Outlooka są tylko do odczytu.
pub fn delete(db: &Connection, id: &str) -> Result<(), String> {
    read_only(id)?;
    let changed = db
        .execute("DELETE FROM manual_meetings WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Nie ma takiego spotkania.".into());
    }
    // Nadpisanie przypomnienia (C3) znika razem ze spotkaniem.
    db.execute("DELETE FROM meeting_reminders WHERE meeting_id=?1", [id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// „Zapisz” w `MeetingEditor` dla nowego spotkania.
#[tauri::command]
pub fn meeting_add(app: tauri::AppHandle, meeting: MeetingInput) -> Result<CalendarEvent, String> {
    let saved = {
        let state = tauri::Manager::state::<Database>(&app);
        let db = state.0.lock().map_err(|e| e.to_string())?;
        add(&db, &Local, &meeting, db::now())?
    };
    let _ = app.emit(calendar::EVENT, ());
    Ok(saved)
}

/// „Zapisz” w `MeetingEditor` dla istniejącego ręcznego spotkania.
#[tauri::command]
pub fn meeting_update(
    app: tauri::AppHandle,
    id: String,
    meeting: MeetingInput,
) -> Result<CalendarEvent, String> {
    let saved = {
        let state = tauri::Manager::state::<Database>(&app);
        let db = state.0.lock().map_err(|e| e.to_string())?;
        update(&db, &Local, &id, &meeting, db::now())?
    };
    let _ = app.emit(calendar::EVENT, ());
    Ok(saved)
}

/// „Usuń” w `MeetingEditor` (po potwierdzeniu).
#[tauri::command]
pub fn meeting_delete(app: tauri::AppHandle, id: String) -> Result<(), String> {
    {
        let state = tauri::Manager::state::<Database>(&app);
        let db = state.0.lock().map_err(|e| e.to_string())?;
        delete(&db, &id)?;
    }
    let _ = app.emit(calendar::EVENT, ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;

    const MIN: i64 = 60_000;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db.execute_batch(
            "INSERT INTO tasks(title,created_at) VALUES('Analiza',0);
             INSERT INTO tasks(title,created_at) VALUES('W koszu',0);
             INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(2,5);",
        )
        .unwrap();
        db
    }
    fn at(s: &str) -> i64 {
        Warsaw
            .from_local_datetime(
                &chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap(),
            )
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    fn input(start: &str, end: &str, subject: &str) -> MeetingInput {
        MeetingInput {
            start: at(start),
            end: at(end),
            subject: subject.into(),
            show_as: "busy".into(),
            all_day: false,
            task_id: None,
        }
    }
    fn outlook(id: &str, start: i64, end: i64) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            start,
            end,
            subject: id.into(),
            show_as: "busy".into(),
            all_day: false,
            cancelled: false,
            response: "accepted".into(),
            source: calendar::OUTLOOK.into(),
            task_id: None,
        }
    }

    #[test]
    fn validates_subject_type_hours_and_task() {
        let db = db();
        let ok = input("2026-10-15 10:00", "2026-10-15 10:30", "  Telefon  ");
        let v = validate(&db, &Warsaw, &ok).unwrap();
        assert_eq!(v.subject, "Telefon");
        assert_eq!((v.start, v.end), (ok.start, ok.end));

        let err = |i: MeetingInput| validate(&db, &Warsaw, &i).unwrap_err();
        assert_eq!(
            err(input("2026-10-15 10:00", "2026-10-15 10:30", "   ")),
            "Podaj temat spotkania."
        );
        let long = "x".repeat(MAX_SUBJECT + 1);
        assert!(err(input("2026-10-15 10:00", "2026-10-15 10:30", &long)).contains("200"));
        // 200 znaków (także wielobajtowych) przechodzi.
        let max = "ż".repeat(MAX_SUBJECT);
        assert!(validate(
            &db,
            &Warsaw,
            &input("2026-10-15 10:00", "2026-10-15 10:30", &max)
        )
        .is_ok());
        for show_as in SHOW_AS {
            let mut i = ok.clone();
            i.show_as = show_as.into();
            assert!(validate(&db, &Warsaw, &i).is_ok(), "{show_as}");
        }
        for show_as in ["free", "unknown", ""] {
            let mut i = ok.clone();
            i.show_as = show_as.into();
            assert_eq!(err(i), "Niepoprawny typ spotkania.");
        }
        assert_eq!(
            err(input("2026-10-15 10:00", "2026-10-15 10:00", "A")),
            "Koniec spotkania musi być po początku."
        );
        assert_eq!(
            err(input("2026-10-15 11:00", "2026-10-15 10:00", "A")),
            "Koniec spotkania musi być po początku."
        );
        assert!(
            err(input("2026-10-15 23:00", "2026-10-16 01:00", "A")).contains("tego samego dnia")
        );
        // Koniec o północy kończy ten sam dzień.
        assert!(validate(
            &db,
            &Warsaw,
            &input("2026-10-15 23:00", "2026-10-16 00:00", "A")
        )
        .is_ok());
        // Zadanie: istniejące tak, nieistniejące i w koszu nie.
        let mut task = ok.clone();
        task.task_id = Some(1);
        assert_eq!(validate(&db, &Warsaw, &task).unwrap().task_id, Some(1));
        task.task_id = Some(99);
        assert_eq!(err(task.clone()), "Wybrane zadanie nie istnieje.");
        task.task_id = Some(2);
        assert_eq!(err(task), "Wybrane zadanie nie istnieje.");
    }

    #[test]
    fn all_day_covers_local_day_of_start() {
        let db = db();
        let mut i = input("2026-10-25 14:00", "2026-10-25 14:00", "Szkolenie");
        i.all_day = true;
        i.show_as = "oof".into();
        let v = validate(&db, &Warsaw, &i).unwrap();
        // 25.10.2026: zmiana czasu, doba ma 25 godzin.
        assert_eq!(
            (v.start, v.end),
            (at("2026-10-25 00:00"), at("2026-10-26 00:00"))
        );
        assert_eq!(v.end - v.start, 25 * 60 * MIN);
        assert!(v.all_day);
    }

    #[test]
    fn add_update_delete_and_outlook_is_read_only() {
        let db = db();
        let now = at("2026-10-15 08:00");
        let a = add(
            &db,
            &Warsaw,
            &input("2026-10-15 10:00", "2026-10-15 10:30", "Telefon"),
            now,
        )
        .unwrap();
        let b = add(
            &db,
            &Warsaw,
            &input("2026-10-15 12:00", "2026-10-15 13:00", "Klient"),
            now,
        )
        .unwrap();
        assert!(a.id.starts_with(PREFIX));
        assert_ne!(a.id, b.id);
        assert_eq!(a.source, MANUAL);
        assert_eq!(a.task_id, None);
        assert!(calendar::occupies(&a));

        let mut change = input("2026-10-15 10:15", "2026-10-15 11:00", "Telefon z klientem");
        change.task_id = Some(1);
        change.show_as = "tentative".into();
        let changed = update(&db, &Warsaw, &a.id, &change, now + 1).unwrap();
        assert_eq!(changed.subject, "Telefon z klientem");
        assert_eq!((changed.start, changed.end), (change.start, change.end));
        assert_eq!(changed.task_id, Some(1));
        assert_eq!(changed.show_as, "tentative");
        // Błąd walidacji niczego nie zmienia.
        assert!(update(
            &db,
            &Warsaw,
            &a.id,
            &input("2026-10-15 10:00", "2026-10-15 09:00", "X"),
            now
        )
        .is_err());
        assert_eq!(get(&db, &a.id).unwrap().unwrap(), changed);

        assert_eq!(
            update(&db, &Warsaw, "AAMkAD-outlook", &change, now).unwrap_err(),
            "Spotkania z Outlooka są tylko do odczytu."
        );
        assert_eq!(
            delete(&db, "AAMkAD-outlook").unwrap_err(),
            "Spotkania z Outlooka są tylko do odczytu."
        );
        assert_eq!(
            update(&db, &Warsaw, "manual:1", &change, now).unwrap_err(),
            "Nie ma takiego spotkania."
        );
        delete(&db, &b.id).unwrap();
        assert_eq!(delete(&db, &b.id).unwrap_err(), "Nie ma takiego spotkania.");
        let left = between(&db, 0, i64::MAX).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, a.id);
    }

    #[test]
    fn meetings_between_joins_sources_by_start_and_needs_account_for_outlook() {
        let mut db = db();
        let now = at("2026-10-15 08:00");
        calendar::replace_range(
            &mut db,
            at("2026-10-12 00:00"),
            at("2026-10-19 00:00"),
            &[
                outlook("o-daily", at("2026-10-15 09:15"), at("2026-10-15 09:30")),
                outlook("o-review", at("2026-10-15 13:00"), at("2026-10-15 14:00")),
            ],
            now,
        )
        .unwrap();
        add(
            &db,
            &Warsaw,
            &input("2026-10-15 11:00", "2026-10-15 11:30", "Telefon"),
            now,
        )
        .unwrap();
        add(
            &db,
            &Warsaw,
            &input("2026-10-15 08:00", "2026-10-15 08:30", "Wcześnie"),
            now,
        )
        .unwrap();
        add(
            &db,
            &Warsaw,
            &input("2026-10-16 08:00", "2026-10-16 08:30", "Jutro"),
            now,
        )
        .unwrap();
        let day = (at("2026-10-15 00:00"), at("2026-10-16 00:00"));

        // Bez połączonego konta: tylko ręczne.
        let list = calendar::meetings_between(&db, day.0, day.1).unwrap();
        let subjects: Vec<&str> = list.iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(subjects, ["Wcześnie", "Telefon"]);
        assert!(list.iter().all(|e| e.source == MANUAL));

        // Z kontem: oba źródła według początku.
        calendar::set_test_email(&db, "jan@firma.example");
        let list = calendar::meetings_between(&db, day.0, day.1).unwrap();
        let subjects: Vec<&str> = list.iter().map(|e| e.subject.as_str()).collect();
        assert_eq!(subjects, ["Wcześnie", "o-daily", "Telefon", "o-review"]);
        let sources: Vec<&str> = list.iter().map(|e| e.source.as_str()).collect();
        assert_eq!(
            sources,
            [MANUAL, calendar::OUTLOOK, MANUAL, calendar::OUTLOOK]
        );
    }

    #[test]
    fn manual_meetings_survive_outlook_sync_and_disconnect() {
        let mut db = db();
        let now = at("2026-10-15 08:00");
        calendar::set_test_email(&db, "jan@firma.example");
        let manual = add(
            &db,
            &Warsaw,
            &input("2026-10-15 11:00", "2026-10-15 11:30", "Telefon"),
            now,
        )
        .unwrap();
        let range = (at("2026-10-12 00:00"), at("2026-10-19 00:00"));
        calendar::replace_range(
            &mut db,
            range.0,
            range.1,
            &[outlook("o", at("2026-10-15 09:00"), at("2026-10-15 10:00"))],
            now,
        )
        .unwrap();
        // Ponowne pobranie zakresu bez spotkań kasuje Outlooka, nie ręczne.
        calendar::replace_range(&mut db, range.0, range.1, &[], now).unwrap();
        assert_eq!(between(&db, range.0, range.1).unwrap(), [manual.clone()]);
        calendar::replace_range(
            &mut db,
            range.0,
            range.1,
            &[outlook("o", at("2026-10-15 09:00"), at("2026-10-15 10:00"))],
            now,
        )
        .unwrap();
        assert_eq!(
            calendar::meetings_between(&db, range.0, range.1)
                .unwrap()
                .len(),
            2
        );
        // Rozłączenie: znika konto i Outlook, ręczne zostaje i dalej obowiązuje.
        calendar::forget_account(&db).unwrap();
        assert!(!calendar::connected(&db));
        assert!(calendar::events_between(&db, range.0, range.1)
            .unwrap()
            .is_empty());
        assert_eq!(
            calendar::meetings_between(&db, range.0, range.1).unwrap(),
            [manual]
        );
    }

    #[test]
    fn deleting_task_keeps_meeting_without_task() {
        let db = db();
        let mut i = input("2026-10-15 11:00", "2026-10-15 11:30", "Telefon");
        i.task_id = Some(1);
        let m = add(&db, &Warsaw, &i, 1).unwrap();
        assert_eq!(m.task_id, Some(1));
        db.execute("DELETE FROM tasks WHERE id=1", []).unwrap();
        let after = get(&db, &m.id).unwrap().unwrap();
        assert_eq!(after.task_id, None);
        assert_eq!(after.subject, "Telefon");
    }

    #[test]
    fn manual_meeting_reduces_capacity_without_connected_calendar() {
        let db = db();
        let s = crate::settings::load(&db);
        let date = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let now = at("2026-10-15 12:00");
        assert!(!calendar::connected(&db));
        assert_eq!(
            crate::capacity::goal_ms_in(&db, &Warsaw, &s, date, now).unwrap(),
            480 * MIN
        );
        add(
            &db,
            &Warsaw,
            &input("2026-10-15 09:00", "2026-10-15 10:30", "U klienta"),
            now,
        )
        .unwrap();
        assert_eq!(
            crate::capacity::goal_ms_in(&db, &Warsaw, &s, date, now).unwrap(),
            390 * MIN
        );
        // Całodniowe „Poza biurem” = dzień wolny (powiadomienia, tray).
        let friday = NaiveDate::from_ymd_opt(2026, 10, 16).unwrap();
        assert!(!crate::capacity::off_day_in(&db, &Warsaw, friday));
        let mut oof = input("2026-10-16 00:00", "2026-10-16 00:00", "Urlop");
        oof.all_day = true;
        oof.show_as = "oof".into();
        add(&db, &Warsaw, &oof, now).unwrap();
        assert!(crate::capacity::off_day_in(&db, &Warsaw, friday));
        assert_eq!(
            crate::capacity::goal_ms_in(&db, &Warsaw, &s, friday, now).unwrap(),
            0
        );
    }
}
