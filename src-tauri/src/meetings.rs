//! Ręczne spotkania (PLAN_BACKLOG.md, B12): spotkania spoza Outlooka (telefon,
//! spotkanie u klienta, kalendarz innej firmy, praca bez połączonego konta).
//!
//! Leżą w osobnej tabeli `manual_meetings` (`CREATE TABLE IF NOT EXISTS`
//! w `db::init`), nie w `calendar_events`, więc pobieranie z Microsoft 365
//! (`calendar::replace_range`) ani rozłączenie konta (`calendar::forget_account`)
//! ich nie kasują. Identyfikator ma prefiks `manual:`, zadanie jest opcjonalne
//! (`ON DELETE SET NULL`), a kopia SQLite obejmuje tabelę, bo kopiuje całą bazę.
//! Spotkania łączy z Outlookiem `calendar::meetings_between`.
//!
//! Cykle (np. daily w dni robocze, planning co 2 tygodnie) są materializowane:
//! `add` z `repeat` wstawia w jednej transakcji osobny wiersz na każde
//! wystąpienie (do `MAX_OCCURRENCES`), wszystkie z `series_id` = identyfikator
//! pierwszego. Edycja i „Usuń tylko to” dotyczą jednego wystąpienia,
//! `delete_following` usuwa wystąpienie i kolejne z cyklu. Dni wystąpień liczy
//! `occurrences` (ta sama semantyka „co N tygodni” co `recurrence::occurs`).
//! Odpowiednik w podglądzie: `src/meetingRepeat.ts` i `src/preview/calendar.ts`.
use crate::calendar::{self, CalendarEvent};
use crate::{db, stats, Database};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;
use std::collections::BTreeSet;
use tauri::Emitter;

/// Źródło ręcznych spotkań w `CalendarEvent::source`.
pub const MANUAL: &str = "manual";
/// Prefiks identyfikatora ręcznego spotkania.
pub const PREFIX: &str = "manual:";
/// Typy spotkania: Zajęte, Wstępnie, Poza biurem, Praca gdzie indziej.
pub const SHOW_AS: [&str; 4] = ["busy", "tentative", "oof", "workingElsewhere"];
/// Najdłuższy temat (znaki).
pub const MAX_SUBJECT: usize = 200;
/// Najwięcej wystąpień jednego cyklu.
pub const MAX_OCCURRENCES: usize = 200;
/// Najdłuższy cykl: ostatni dzień najpóźniej tyle dni po pierwszym.
pub const MAX_REPEAT_SPAN_DAYS: i64 = 366;
/// Najdłuższy odstęp cyklu (tygodnie), jak `RULE_WEEKS` zadań cyklicznych.
pub const MAX_REPEAT_INTERVAL: u32 = 8;

/// Cykl nowego spotkania („Powtarzaj” w `MeetingEditor.vue`): dni tygodnia ISO
/// (1 = poniedziałek), co `interval` tygodni od tygodnia pierwszego spotkania,
/// do `until` (RRRR-MM-DD, włącznie).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRepeat {
    pub days: Vec<u32>,
    pub interval: u32,
    pub until: String,
}

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
    /// Cykl, tylko przy dodawaniu (`add`); `update` z cyklem zwraca błąd.
    #[serde(default)]
    pub repeat: Option<MeetingRepeat>,
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

/// Dni wystąpień cyklu: zawsze `first` (pierwsze spotkanie, niezależnie od
/// `days`), potem każdy dzień z `(first, until]` o dniu tygodnia z `days`
/// w tygodniu odległym od tygodnia `first` o wielokrotność `interval`
/// (jak `RULE_WEEKS` w `recurrence::occurs`).
fn occurrences(first: NaiveDate, until: NaiveDate, days: &[u32], interval: u32) -> Vec<NaiveDate> {
    let start_week = crate::recurrence::monday(first);
    let every = i64::from(interval.max(1));
    std::iter::once(first)
        .chain(
            first
                .iter_days()
                .skip(1)
                .take_while(|d| *d <= until)
                .filter(|d| {
                    let weeks = (crate::recurrence::monday(*d) - start_week)
                        .num_days()
                        .div_euclid(7);
                    days.contains(&d.weekday().number_from_monday()) && weeks.rem_euclid(every) == 0
                }),
        )
        .collect()
}

/// Walidacja cyklu i jego dni; komunikaty jak `meetingRepeatDays` w `src/meetingRepeat.ts`.
fn repeat_dates(first: NaiveDate, r: &MeetingRepeat) -> Result<Vec<NaiveDate>, String> {
    if r.days.is_empty() {
        return Err("Wybierz co najmniej jeden dzień tygodnia.".into());
    }
    let unique: BTreeSet<u32> = r.days.iter().copied().collect();
    if unique.len() != r.days.len() || r.days.iter().any(|d| !(1..=7).contains(d)) {
        return Err("Niepoprawne dni tygodnia cyklu.".into());
    }
    if !(1..=MAX_REPEAT_INTERVAL).contains(&r.interval) {
        return Err(format!(
            "Odstęp cyklu musi wynosić od 1 do {MAX_REPEAT_INTERVAL} tygodni."
        ));
    }
    let until = NaiveDate::parse_from_str(&r.until, "%Y-%m-%d")
        .ok()
        .filter(|d| d.format("%Y-%m-%d").to_string() == r.until)
        .ok_or("Podaj poprawny ostatni dzień cyklu.")?;
    if until < first {
        return Err("Ostatni dzień cyklu nie może być przed dniem spotkania.".into());
    }
    if (until - first).num_days() > MAX_REPEAT_SPAN_DAYS {
        return Err(format!(
            "Cykl może trwać najwyżej {MAX_REPEAT_SPAN_DAYS} dni."
        ));
    }
    let dates = occurrences(first, until, &r.days, r.interval);
    if dates.len() > MAX_OCCURRENCES {
        return Err(format!(
            "Cykl może mieć najwyżej {MAX_OCCURRENCES} spotkań. Skróć okres albo wybierz mniej dni."
        ));
    }
    Ok(dates)
}

/// Chwila lokalnej daty i godziny `at`; w luce zmiany czasu (wiosną) godzina
/// przesuwa się o godzinę do przodu (jak `new Date(r, m, d, g, min)` w JS),
/// przy podwójnej godzinie (jesienią) wybiera wcześniejszą.
fn local_ms<Tz: TimeZone>(tz: &Tz, at: NaiveDateTime) -> Option<i64> {
    tz.from_local_datetime(&at)
        .earliest()
        .or_else(|| {
            tz.from_local_datetime(&(at + Duration::hours(1)))
                .earliest()
        })
        .map(|d| d.timestamp_millis())
}

/// Granice wystąpienia cyklu w dniu `date`: ta sama lokalna godzina od–do co
/// pierwsze spotkanie `v` (koniec o północy kończy dzień), całodniowe = cały
/// lokalny dzień (`stats::day_bounds`). Gdy luka zmiany czasu zrówna koniec
/// z początkiem, wystąpienie trwa tyle co pierwsze.
fn occurrence_bounds<Tz: TimeZone>(
    tz: &Tz,
    v: &Valid,
    date: NaiveDate,
) -> Result<(i64, i64), String> {
    if v.all_day {
        return Ok(stats::day_bounds(tz, date));
    }
    const BAD_START: &str = "Niepoprawny początek spotkania.";
    const BAD_END: &str = "Niepoprawny koniec spotkania.";
    let local = |ms: i64, error: &'static str| {
        tz.timestamp_millis_opt(ms)
            .single()
            .map(|d| d.naive_local())
            .ok_or(error)
    };
    let (start_local, end_local) = (local(v.start, BAD_START)?, local(v.end, BAD_END)?);
    let end_days = (end_local.date() - start_local.date()).num_days();
    let start = local_ms(tz, date.and_time(start_local.time())).ok_or(BAD_START)?;
    let end = local_ms(
        tz,
        (date + Duration::days(end_days)).and_time(end_local.time()),
    )
    .ok_or(BAD_END)?;
    Ok((
        start,
        if end > start {
            end
        } else {
            start + (v.end - v.start)
        },
    ))
}

fn read_only(id: &str) -> Result<(), String> {
    if id.starts_with(PREFIX) {
        Ok(())
    } else {
        Err("Spotkania z Outlooka są tylko do odczytu.".into())
    }
}

const COLUMNS: &str = "id,start,\"end\",subject,show_as,all_day,task_id,series_id";
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
        series_id: r.get(7)?,
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

/// Pierwszy wolny identyfikator `manual:<now>` (n = 1) albo `manual:<now>-<n>`;
/// `n` rośnie między wywołaniami, więc kolejne wystąpienia cyklu dostają `-2`, `-3`…
fn fresh_id(db: &Connection, now: i64, n: &mut u32) -> Result<String, String> {
    loop {
        let id = if *n == 1 {
            format!("{PREFIX}{now}")
        } else {
            format!("{PREFIX}{now}-{n}")
        };
        if get(db, &id)?.is_none() {
            return Ok(id);
        }
        *n += 1;
    }
}

fn insert(
    db: &Connection,
    id: &str,
    (start, end): (i64, i64),
    v: &Valid,
    series: Option<&str>,
    now: i64,
) -> Result<(), String> {
    db.execute(
        "INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,task_id,series_id,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
        params![id, start, end, v.subject, v.show_as, v.all_day, v.task_id, series, now],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Dodaje ręczne spotkanie; identyfikator `manual:<czas>` (z dopiskiem przy
/// dwóch spotkaniach w tej samej milisekundzie). Z `repeat` dodaje cały cykl
/// w jednej transakcji i zwraca pierwsze wystąpienie.
pub fn add<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    input: &MeetingInput,
    now: i64,
) -> Result<CalendarEvent, String> {
    let v = validate(db, tz, input)?;
    let first = local_date(tz, v.start).ok_or("Niepoprawny początek spotkania.")?;
    let dates = match &input.repeat {
        Some(r) => repeat_dates(first, r)?,
        None => vec![first],
    };
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut n = 1;
    let first_id = fresh_id(&tx, now, &mut n)?;
    let series = input.repeat.as_ref().map(|_| first_id.as_str());
    insert(&tx, &first_id, (v.start, v.end), &v, series, now)?;
    for date in &dates[1..] {
        let id = fresh_id(&tx, now, &mut n)?;
        insert(&tx, &id, occurrence_bounds(tz, &v, *date)?, &v, series, now)?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    get(db, &first_id)?.ok_or_else(|| "Nie udało się zapisać spotkania.".into())
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
    if input.repeat.is_some() {
        return Err("Cykl można ustawić tylko przy dodawaniu spotkania.".into());
    }
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

/// „Usuń to i kolejne”: usuwa spotkanie `id` i późniejsze (początek ≥ jego
/// początku) z tego samego cyklu razem z nadpisaniami przypomnień (C3); bez
/// cyklu działa jak `delete`. Zwraca liczbę usuniętych spotkań.
pub fn delete_following(db: &Connection, id: &str) -> Result<usize, String> {
    read_only(id)?;
    let found: Option<(Option<String>, i64)> = db
        .query_row(
            "SELECT series_id,start FROM manual_meetings WHERE id=?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let (series, start) = found.ok_or("Nie ma takiego spotkania.")?;
    let Some(series) = series else {
        delete(db, id)?;
        return Ok(1);
    };
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM meeting_reminders WHERE meeting_id IN (SELECT id FROM manual_meetings WHERE series_id=?1 AND start>=?2)",
        params![series, start],
    )
    .map_err(|e| e.to_string())?;
    let n = tx
        .execute(
            "DELETE FROM manual_meetings WHERE series_id=?1 AND start>=?2",
            params![series, start],
        )
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(n)
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

/// „Usuń to i kolejne” w `MeetingEditor` dla spotkania z cyklu (po potwierdzeniu).
#[tauri::command]
pub fn meeting_delete_following(app: tauri::AppHandle, id: String) -> Result<(), String> {
    {
        let state = tauri::Manager::state::<Database>(&app);
        let db = state.0.lock().map_err(|e| e.to_string())?;
        delete_following(&db, &id)?;
    }
    let _ = app.emit(calendar::EVENT, ());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;

    const MIN: i64 = 60_000;

    #[test]
    fn occurrence_bounds_names_wrong_start_and_end_separately() {
        let v = |start, end| Valid {
            start,
            end,
            subject: "A".into(),
            show_as: "busy".into(),
            all_day: false,
            task_id: None,
        };
        let date = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        assert_eq!(
            occurrence_bounds(&Warsaw, &v(0, i64::MAX), date).unwrap_err(),
            "Niepoprawny koniec spotkania."
        );
        assert_eq!(
            occurrence_bounds(&Warsaw, &v(i64::MAX, 0), date).unwrap_err(),
            "Niepoprawny początek spotkania."
        );
    }

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
            repeat: None,
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
            series_id: None,
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

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn days(list: &[&str]) -> Vec<NaiveDate> {
        list.iter().map(|s| day(s)).collect()
    }
    fn repeat(days: &[u32], interval: u32, until: &str) -> MeetingRepeat {
        MeetingRepeat {
            days: days.to_vec(),
            interval,
            until: until.into(),
        }
    }
    fn series_input(
        start: &str,
        end: &str,
        subject: &str,
        days: &[u32],
        interval: u32,
        until: &str,
    ) -> MeetingInput {
        MeetingInput {
            repeat: Some(repeat(days, interval, until)),
            ..input(start, end, subject)
        }
    }
    fn all(db: &Connection) -> Vec<CalendarEvent> {
        between(db, 0, i64::MAX).unwrap()
    }

    #[test]
    fn occurrences_always_include_first_day_then_matching_weekdays() {
        // Czwartek startu wchodzi mimo braku 4 w dniach.
        assert_eq!(
            occurrences(day("2026-10-15"), day("2026-10-26"), &[1], 1),
            days(&["2026-10-15", "2026-10-19", "2026-10-26"])
        );
    }

    #[test]
    fn occurrences_every_second_week_count_from_week_of_first_day() {
        assert_eq!(
            occurrences(day("2026-10-15"), day("2026-11-12"), &[1, 4], 2),
            days(&[
                "2026-10-15",
                "2026-10-26",
                "2026-10-29",
                "2026-11-09",
                "2026-11-12"
            ])
        );
    }

    #[test]
    fn occurrences_include_until_day() {
        assert_eq!(
            occurrences(day("2026-10-15"), day("2026-10-26"), &[1], 1).last(),
            Some(&day("2026-10-26"))
        );
        assert_eq!(
            occurrences(day("2026-10-15"), day("2026-10-15"), &[1, 4], 1),
            days(&["2026-10-15"])
        );
    }

    #[test]
    fn repeat_dates_validates_days_interval_until_and_limit() {
        let first = day("2026-10-15");
        let err = |r: MeetingRepeat| repeat_dates(first, &r).unwrap_err();
        let ok = repeat(&[1, 4], 2, "2026-11-12");
        let with = |f: fn(&mut MeetingRepeat)| {
            let mut r = ok.clone();
            f(&mut r);
            r
        };
        assert_eq!(
            err(with(|r| r.days = vec![])),
            "Wybierz co najmniej jeden dzień tygodnia."
        );
        for bad in [vec![0], vec![8], vec![1, 1]] {
            let mut r = ok.clone();
            r.days = bad;
            assert_eq!(err(r), "Niepoprawne dni tygodnia cyklu.");
        }
        for bad in [0, 9] {
            let mut r = ok.clone();
            r.interval = bad;
            assert_eq!(err(r), "Odstęp cyklu musi wynosić od 1 do 8 tygodni.");
        }
        for bad in ["2026-02-30", "2026-1-5", "x"] {
            let mut r = ok.clone();
            r.until = bad.into();
            assert_eq!(err(r), "Podaj poprawny ostatni dzień cyklu.");
        }
        assert_eq!(
            err(repeat(&[4], 1, "2026-10-14")),
            "Ostatni dzień cyklu nie może być przed dniem spotkania."
        );
        assert_eq!(
            err(repeat(&[4], 1, "2027-10-17")),
            "Cykl może trwać najwyżej 366 dni."
        );
        assert_eq!(
            repeat_dates(first, &repeat(&[4], 1, "2027-10-16"))
                .unwrap()
                .len(),
            53
        );
        let every_day = [1, 2, 3, 4, 5, 6, 7];
        assert_eq!(
            err(repeat(&every_day, 1, "2027-05-03")),
            "Cykl może mieć najwyżej 200 spotkań. Skróć okres albo wybierz mniej dni."
        );
        assert_eq!(
            repeat_dates(first, &repeat(&every_day, 1, "2027-05-02"))
                .unwrap()
                .len(),
            200
        );
    }

    #[test]
    fn add_series_keeps_local_time_across_dst_change() {
        let db = db();
        let now = at("2026-10-20 08:00");
        let first = add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-23 09:00",
                "2026-10-23 09:30",
                "Daily",
                &[1, 5],
                1,
                "2026-10-27",
            ),
            now,
        )
        .unwrap();
        assert_eq!(first.start, at("2026-10-23 09:00"));
        assert_eq!(first.series_id.as_deref(), Some(first.id.as_str()));
        let list = all(&db);
        assert_eq!(list.len(), 2);
        let second = &list[1];
        // 25.10.2026 zmiana czasu: dalej 9:00 lokalnie, o godzinę później w UTC.
        assert_eq!(second.start, at("2026-10-26 09:00"));
        assert_eq!(second.end, at("2026-10-26 09:30"));
        assert_eq!(second.start - first.start, 3 * 24 * 60 * MIN + 60 * MIN);
        assert_eq!(second.series_id, first.series_id);
        assert_eq!(list[0].id, format!("manual:{now}"));
        assert_eq!(second.id, format!("manual:{now}-2"));
    }

    #[test]
    fn add_series_meeting_ending_at_midnight_and_all_day() {
        let db = db();
        let now = 1;
        add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-24 23:30",
                "2026-10-25 00:00",
                "Nocne",
                &[6, 7],
                1,
                "2026-10-25",
            ),
            now,
        )
        .unwrap();
        let list = all(&db);
        assert_eq!(list.len(), 2);
        assert_eq!(list[1].start, at("2026-10-25 23:30"));
        assert_eq!(list[1].end, at("2026-10-26 00:00"));

        let mut oof = series_input(
            "2026-10-24 00:00",
            "2026-10-24 00:00",
            "Urlop",
            &[6, 7],
            1,
            "2026-10-25",
        );
        oof.all_day = true;
        oof.show_as = "oof".into();
        add(&db, &Warsaw, &oof, 2).unwrap();
        let urlop: Vec<_> = all(&db)
            .into_iter()
            .filter(|e| e.subject == "Urlop")
            .collect();
        assert_eq!(urlop.len(), 2);
        assert_eq!(
            (urlop[1].start, urlop[1].end),
            (at("2026-10-25 00:00"), at("2026-10-26 00:00"))
        );
        assert_eq!(urlop[1].end - urlop[1].start, 25 * 60 * MIN);
    }

    #[test]
    fn add_series_in_spring_gap_moves_hour_forward() {
        let db = db();
        add(
            &db,
            &Warsaw,
            &series_input(
                "2027-03-27 02:30",
                "2027-03-27 03:30",
                "Wiosna",
                &[7],
                1,
                "2027-03-28",
            ),
            1,
        )
        .unwrap();
        let list = all(&db);
        assert_eq!(list.len(), 2);
        // 28.03.2027 02:30 nie istnieje: godzina przesuwa się na 03:30.
        assert_eq!(list[1].start, at("2027-03-28 03:30"));
        assert_eq!(list[1].end - list[1].start, 60 * MIN);
    }

    #[test]
    fn add_series_continues_ids_after_taken_and_rejects_whole_series() {
        let db = db();
        let now = 5;
        add(
            &db,
            &Warsaw,
            &input("2026-10-19 08:00", "2026-10-19 08:30", "Jedno"),
            now,
        )
        .unwrap();
        let first = add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-19 10:00",
                "2026-10-19 10:30",
                "Cykl",
                &[1, 2, 3],
                1,
                "2026-10-21",
            ),
            now,
        )
        .unwrap();
        assert_eq!(first.id, "manual:5-2");
        assert_eq!(first.series_id.as_deref(), Some("manual:5-2"));
        let ids: Vec<String> = all(&db)
            .into_iter()
            .filter(|e| e.subject == "Cykl")
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, ["manual:5-2", "manual:5-3", "manual:5-4"]);

        let before = all(&db).len();
        let too_many = series_input(
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "Za dużo",
            &[1, 2, 3, 4, 5, 6, 7],
            1,
            "2027-05-03",
        );
        assert!(add(&db, &Warsaw, &too_many, now)
            .unwrap_err()
            .contains("najwyżej 200 spotkań"));
        assert_eq!(all(&db).len(), before);
        // Walidacja spotkania idzie przed walidacją cyklu.
        let empty = series_input("2026-10-15 10:00", "2026-10-15 10:30", " ", &[], 1, "x");
        assert_eq!(
            add(&db, &Warsaw, &empty, now).unwrap_err(),
            "Podaj temat spotkania."
        );
    }

    #[test]
    fn update_with_repeat_is_rejected_and_keeps_series() {
        let db = db();
        let first = add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-19 10:00",
                "2026-10-19 10:30",
                "Cykl",
                &[1, 2],
                1,
                "2026-10-20",
            ),
            1,
        )
        .unwrap();
        let list = all(&db);
        let second = list[1].clone();
        let mut change = input("2026-10-20 11:00", "2026-10-20 11:45", "Zmienione");
        change.repeat = Some(repeat(&[1], 1, "2026-10-27"));
        assert_eq!(
            update(&db, &Warsaw, &second.id, &change, 2).unwrap_err(),
            "Cykl można ustawić tylko przy dodawaniu spotkania."
        );
        assert_eq!(get(&db, &second.id).unwrap().unwrap(), second);
        assert_eq!(
            update(&db, &Warsaw, "AAMkAD-outlook", &change, 2).unwrap_err(),
            "Spotkania z Outlooka są tylko do odczytu."
        );
        change.repeat = None;
        let changed = update(&db, &Warsaw, &second.id, &change, 2).unwrap();
        assert_eq!(changed.subject, "Zmienione");
        assert_eq!(changed.series_id, first.series_id);
        assert_eq!(get(&db, &first.id).unwrap().unwrap(), list[0]);
    }

    #[test]
    fn delete_following_removes_this_and_later_of_series_with_reminders() {
        let db = db();
        let a_all = [
            "2026-10-19",
            "2026-10-20",
            "2026-10-21",
            "2026-10-22",
            "2026-10-23",
        ];
        add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-19 10:00",
                "2026-10-19 10:30",
                "A",
                &[1, 2, 3, 4, 5],
                1,
                a_all[4],
            ),
            1,
        )
        .unwrap();
        add(
            &db,
            &Warsaw,
            &series_input(
                "2026-10-19 11:00",
                "2026-10-19 11:30",
                "B",
                &[1, 2, 3],
                1,
                "2026-10-21",
            ),
            2,
        )
        .unwrap();
        add(
            &db,
            &Warsaw,
            &input("2026-10-19 12:00", "2026-10-19 12:30", "C"),
            3,
        )
        .unwrap();
        let by = |subject: &str| -> Vec<CalendarEvent> {
            all(&db)
                .into_iter()
                .filter(|e| e.subject == subject)
                .collect()
        };
        let (a, b, c) = (by("A"), by("B"), by("C"));
        assert_eq!((a.len(), b.len(), c.len()), (5, 3, 1));
        for id in [&a[0].id, &a[2].id, &a[4].id, &b[2].id, &c[0].id] {
            db.execute(
                "INSERT INTO meeting_reminders(meeting_id,minutes) VALUES(?1,5)",
                [id],
            )
            .unwrap();
        }
        assert_eq!(delete_following(&db, &a[2].id).unwrap(), 3);
        assert_eq!(by("A"), a[..2].to_vec());
        assert_eq!(by("B"), b);
        assert_eq!(by("C"), c);
        let mut left: Vec<String> = db
            .prepare("SELECT meeting_id FROM meeting_reminders")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        left.sort();
        let mut expected = vec![a[0].id.clone(), b[2].id.clone(), c[0].id.clone()];
        expected.sort();
        assert_eq!(left, expected);

        // Bez cyklu działa jak `delete`, razem z nadpisaniem przypomnienia.
        assert_eq!(delete_following(&db, &c[0].id).unwrap(), 1);
        assert!(by("C").is_empty());
        let reminders: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM meeting_reminders WHERE meeting_id=?1",
                [&c[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(reminders, 0);
        assert_eq!(
            delete_following(&db, "AAMkAD-outlook").unwrap_err(),
            "Spotkania z Outlooka są tylko do odczytu."
        );
        assert_eq!(
            delete_following(&db, "manual:404").unwrap_err(),
            "Nie ma takiego spotkania."
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
