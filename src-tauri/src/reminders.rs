//! Przypomnienia (PLAN_BACKLOG.md, C3): własne na datę i godzinę (jednorazowe
//! albo powtarzalne) oraz przypomnienia przed spotkaniem. To nie jest
//! `reminder.rs` (przypomnienie o braku pomiaru).
//!
//! - Tabela `reminders` (`CREATE TABLE IF NOT EXISTS` w `db::init`): `at` to
//!   lokalna data i godzina następnego wystąpienia (`RRRR-MM-DDTGG:MM`), a
//!   `repeat` to `none` / `daily` / `workdays` (pon–pt bez świąt z `holidays.rs`)
//!   / `weekly` (dni z `weekdays`, 1 = poniedziałek). `snooze_until` trzyma
//!   „Odłóż” powtarzalnego przypomnienia, żeby nie przesuwać jego harmonogramu.
//!   Zadanie jest opcjonalne (`ON DELETE SET NULL`), a kopia SQLite obejmuje
//!   tabelę, bo kopiuje całą bazę.
//! - Wyzwalanie w `notify::NotifyClock::tick` (`fire_due`, `meeting_messages`):
//!   czas i strefa z zewnątrz, klucz w `notifications_sent` (`rem:<id>:<at>`,
//!   `meet:<id spotkania>:<start>`) powstaje przed wysłaniem, więc restart nie
//!   powtarza toastu. Po wystrzeleniu jednorazowe przechodzi na `enabled = 0`,
//!   powtarzalne na następne wystąpienie po teraz. Po starcie przegapione
//!   przypomnienia dostają jeden toast „Przegapione przypomnienie”.
//! - `meeting_reminders(meeting_id, minutes)`: nadpisanie dla jednego spotkania
//!   (`minutes` puste = bez przypomnienia, brak wiersza = ustawienie domyślne).
//!
//! Toast ma akcje „Odłóż 10 min” i „Pokaż”; żadna nigdy nie startuje pomiaru.
use crate::notify::{self, Action, Message, View};
use crate::{calendar, db, meetings, settings::AppSettings, Database};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, TimeZone, Timelike};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{Emitter, Manager};

/// Zdarzenie okna po zmianie przypomnień (komendy i „Odłóż” z toastu).
pub const EVENT: &str = "reminders-changed";
/// Format `at` i `snooze_until`: lokalnie, co do minuty.
pub const AT_FORMAT: &str = "%Y-%m-%dT%H:%M";
/// Najdłuższy tekst przypomnienia (znaki).
pub const MAX_TEXT: usize = 200;
/// Rodzaje powtarzania.
pub const REPEATS: [&str; 4] = ["none", "daily", "workdays", "weekly"];
/// Dozwolone „X minut przed” spotkaniem (ustawienie domyślne i nadpisanie).
pub const MEETING_MINUTES: [u32; 5] = [1, 5, 10, 15, 30];
/// Odłożenie z przycisku toastu.
pub const SNOOZE_MINUTES: i64 = 10;
/// Najdłuższe odłożenie z komendy (doba).
const MAX_SNOOZE: i64 = 1_440;
/// Od tylu minut spóźnienia po starcie przypomnienie jest „przegapione”.
const MISSED_MS: i64 = 60_000;

/// Przypomnienie jak z bazy.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub text: String,
    pub task_id: Option<i64>,
    /// Tytuł przypiętego zadania (tylko do wyświetlenia).
    pub task_title: Option<String>,
    pub at: String,
    pub repeat: String,
    /// Dni tygodnia dla `weekly` (1 = poniedziałek … 7 = niedziela).
    pub weekdays: Vec<u8>,
    pub enabled: bool,
    /// Odłożone wystąpienie powtarzalnego przypomnienia.
    pub snooze_until: Option<String>,
    pub last_fired_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Dane formularza przypomnienia (`reminder_add` / `reminder_update`).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderInput {
    pub text: String,
    pub at: String,
    #[serde(default = "none")]
    pub repeat: String,
    #[serde(default)]
    pub weekdays: Vec<u8>,
    #[serde(default)]
    pub task_id: Option<i64>,
    #[serde(default = "yes")]
    pub enabled: bool,
}
fn none() -> String {
    "none".into()
}
fn yes() -> bool {
    true
}

/// Przypomnienie po walidacji.
#[derive(Debug, PartialEq)]
struct Valid {
    text: String,
    at: NaiveDateTime,
    repeat: String,
    weekdays: Vec<u8>,
    task_id: Option<i64>,
    enabled: bool,
}

/// Lokalny czas `ms` co do minuty (bez sekund).
fn local_minute<Tz: TimeZone>(tz: &Tz, ms: i64) -> Option<NaiveDateTime> {
    tz.timestamp_millis_opt(ms)
        .earliest()
        .and_then(|d| d.naive_local().with_second(0))
        .and_then(|d| d.with_nanosecond(0))
}
fn local_exact<Tz: TimeZone>(tz: &Tz, ms: i64) -> Option<NaiveDateTime> {
    tz.timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.naive_local())
}
/// `NaiveDateTime` → Unix ms w strefie `tz` (do porównań z czasem zapisu).
fn to_ms<Tz: TimeZone>(tz: &Tz, at: NaiveDateTime) -> Option<i64> {
    tz.from_local_datetime(&at)
        .earliest()
        .map(|d| d.timestamp_millis())
}
fn parse_at(at: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(at, AT_FORMAT).ok()
}
fn format_at(at: NaiveDateTime) -> String {
    at.format(AT_FORMAT).to_string()
}

/// Czy dzień pasuje do reguły powtarzania.
fn day_matches(repeat: &str, weekdays: &[u8], day: NaiveDate) -> bool {
    match repeat {
        "daily" => true,
        "workdays" => {
            day.weekday().number_from_monday() <= 5 && crate::holidays::holiday(day).is_none()
        }
        "weekly" => weekdays.contains(&(day.weekday().number_from_monday() as u8)),
        _ => false,
    }
}

/// Pierwsze wystąpienie powtarzalnego przypomnienia ściśle po `after`, o
/// godzinie z `at` i w dniu pasującym do reguły; `none` nie ma następnego.
pub fn next_occurrence(
    repeat: &str,
    weekdays: &[u8],
    at: NaiveDateTime,
    after: NaiveDateTime,
) -> Option<NaiveDateTime> {
    if repeat == "none" {
        return None;
    }
    let mut day = at.date().max(after.date());
    // Rok dni wystarcza: `weekly` bez dni jest odrzucane przy zapisie.
    for _ in 0..400 {
        if day_matches(repeat, weekdays, day) {
            let candidate = day.and_time(at.time());
            if candidate > after {
                return Some(candidate);
            }
        }
        day = day.succ_opt()?;
    }
    None
}

/// Pierwsze wystąpienie nie wcześniejsze niż `from`.
fn first_occurrence(
    repeat: &str,
    weekdays: &[u8],
    at: NaiveDateTime,
    from: NaiveDateTime,
) -> Option<NaiveDateTime> {
    next_occurrence(repeat, weekdays, at, from - Duration::minutes(1))
}

fn weekdays_text(days: &[u8]) -> Option<String> {
    (!days.is_empty()).then(|| days.iter().map(u8::to_string).collect::<Vec<_>>().join(","))
}
fn parse_weekdays(text: Option<String>) -> Vec<u8> {
    text.unwrap_or_default()
        .split(',')
        .filter_map(|d| d.trim().parse::<u8>().ok())
        .filter(|d| (1..=7).contains(d))
        .collect()
}

/// Walidacja z C3: tekst 1–200 znaków, poprawna data i godzina, powtarzanie
/// z listy, `weekly` z co najmniej jednym dniem, zadanie istniejące. Jednorazowe
/// i włączone nie może być w przeszłości; powtarzalne przesuwa się na pierwsze
/// pasujące wystąpienie nie wcześniejsze niż teraz.
fn validate<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    input: &ReminderInput,
    now: i64,
) -> Result<Valid, String> {
    let text = input.text.trim();
    if text.is_empty() {
        return Err("Podaj treść przypomnienia.".into());
    }
    if text.chars().count() > MAX_TEXT {
        return Err(format!(
            "Treść przypomnienia może mieć najwyżej {MAX_TEXT} znaków."
        ));
    }
    let at = parse_at(input.at.trim()).ok_or("Podaj poprawny dzień i godzinę przypomnienia.")?;
    if !REPEATS.contains(&input.repeat.as_str()) {
        return Err("Niepoprawny sposób powtarzania przypomnienia.".into());
    }
    let mut weekdays = input.weekdays.clone();
    if input.repeat == "weekly" {
        if weekdays.is_empty() {
            return Err("Zaznacz co najmniej jeden dzień tygodnia.".into());
        }
        if weekdays.iter().any(|d| !(1..=7).contains(d)) {
            return Err("Dni tygodnia muszą być liczbami od 1 do 7.".into());
        }
        weekdays.sort_unstable();
        weekdays.dedup();
    } else {
        weekdays.clear();
    }
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
    let current = local_minute(tz, now).ok_or("Niepoprawny czas systemowy.")?;
    let at = if input.repeat == "none" {
        if input.enabled && at < current {
            return Err("Przypomnienie jednorazowe musi być w przyszłości.".into());
        }
        at
    } else {
        first_occurrence(&input.repeat, &weekdays, at, at.max(current))
            .ok_or("Nie udało się wyznaczyć wystąpienia przypomnienia.")?
    };
    Ok(Valid {
        text: text.to_owned(),
        at,
        repeat: input.repeat.clone(),
        weekdays,
        task_id: input.task_id,
        enabled: input.enabled,
    })
}

const COLUMNS: &str = "r.id,r.text,r.task_id,t.title,r.at,r.repeat,r.weekdays,r.enabled,r.snooze_until,r.last_fired_at,r.created_at,r.updated_at";
const FROM: &str = "FROM reminders r LEFT JOIN tasks t ON t.id=r.task_id";
fn row(r: &rusqlite::Row) -> rusqlite::Result<Reminder> {
    Ok(Reminder {
        id: r.get(0)?,
        text: r.get(1)?,
        task_id: r.get(2)?,
        task_title: r.get(3)?,
        at: r.get(4)?,
        repeat: r.get(5)?,
        weekdays: parse_weekdays(r.get(6)?),
        enabled: r.get(7)?,
        snooze_until: r.get(8)?,
        last_fired_at: r.get(9)?,
        created_at: r.get(10)?,
        updated_at: r.get(11)?,
    })
}

/// Przypomnienie o identyfikatorze `id`.
pub fn get(db: &Connection, id: i64) -> Result<Option<Reminder>, String> {
    db.query_row(&format!("SELECT {COLUMNS} {FROM} WHERE r.id=?1"), [id], row)
        .optional()
        .map_err(|e| e.to_string())
}

/// Wszystkie przypomnienia: nadchodzące (włączone) według najbliższego
/// terminu, potem wyłączone i wykonane od najnowszych.
pub fn list(db: &Connection) -> Result<Vec<Reminder>, String> {
    let mut stmt = db
        .prepare(&format!(
            "SELECT {COLUMNS} {FROM}
             ORDER BY r.enabled DESC,
                      CASE WHEN r.enabled=1 THEN COALESCE(r.snooze_until, r.at) END,
                      r.at DESC, r.id"
        ))
        .map_err(|e| e.to_string())?;
    let rows = stmt.query_map([], row).map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

/// Dodaje przypomnienie.
pub fn add<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    input: &ReminderInput,
    now: i64,
) -> Result<Reminder, String> {
    let v = validate(db, tz, input, now)?;
    db.execute(
        "INSERT INTO reminders(text,task_id,at,repeat,weekdays,enabled,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?7)",
        params![
            v.text,
            v.task_id,
            format_at(v.at),
            v.repeat,
            weekdays_text(&v.weekdays),
            v.enabled,
            now
        ],
    )
    .map_err(|e| e.to_string())?;
    get(db, db.last_insert_rowid())?.ok_or_else(|| "Nie udało się zapisać przypomnienia.".into())
}

/// Zmienia przypomnienie (także włącza i wyłącza); zapis czyści odłożenie.
pub fn update<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    id: i64,
    input: &ReminderInput,
    now: i64,
) -> Result<Reminder, String> {
    let v = validate(db, tz, input, now)?;
    let changed = db
        .execute(
            "UPDATE reminders SET text=?2,task_id=?3,at=?4,repeat=?5,weekdays=?6,enabled=?7,snooze_until=NULL,updated_at=?8 WHERE id=?1",
            params![
                id,
                v.text,
                v.task_id,
                format_at(v.at),
                v.repeat,
                weekdays_text(&v.weekdays),
                v.enabled,
                now
            ],
        )
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Nie ma takiego przypomnienia.".into());
    }
    get(db, id)?.ok_or_else(|| "Nie ma takiego przypomnienia.".into())
}

/// Usuwa przypomnienie.
pub fn delete(db: &Connection, id: i64) -> Result<(), String> {
    let changed = db
        .execute("DELETE FROM reminders WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("Nie ma takiego przypomnienia.".into());
    }
    Ok(())
}

/// Odkłada przypomnienie o `minutes` od teraz. Jednorazowe dostaje nowy `at`
/// (i wraca na listę aktywnych), powtarzalne `snooze_until`, więc jego
/// harmonogram zostaje bez zmian. `Ok(None)`, gdy przypomnienia już nie ma
/// (stary toast).
pub fn snooze<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    id: i64,
    minutes: i64,
    now: i64,
) -> Result<Option<Reminder>, String> {
    if !(1..=MAX_SNOOZE).contains(&minutes) {
        return Err("Odłożenie musi mieć od 1 do 1440 minut.".into());
    }
    let Some(current) = get(db, id)? else {
        return Ok(None);
    };
    let until =
        local_minute(tz, now).ok_or("Niepoprawny czas systemowy.")? + Duration::minutes(minutes);
    let until = format_at(until);
    if current.repeat == "none" {
        db.execute(
            "UPDATE reminders SET at=?2,snooze_until=NULL,enabled=1,updated_at=?3 WHERE id=?1",
            params![id, until, now],
        )
    } else {
        db.execute(
            "UPDATE reminders SET snooze_until=?2,updated_at=?3 WHERE id=?1",
            params![id, until, now],
        )
    }
    .map_err(|e| e.to_string())?;
    get(db, id)
}

/// Nadpisanie przypomnienia dla jednego spotkania.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingOverride {
    pub meeting_id: String,
    /// `None` = bez przypomnienia.
    pub minutes: Option<u32>,
}

/// Wszystkie nadpisania (brak wiersza = ustawienie domyślne).
pub fn meeting_overrides(db: &Connection) -> Result<Vec<MeetingOverride>, String> {
    let mut stmt = db
        .prepare("SELECT meeting_id, minutes FROM meeting_reminders ORDER BY meeting_id")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(MeetingOverride {
                meeting_id: r.get(0)?,
                minutes: r.get::<_, Option<u32>>(1)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

/// Ustawia przypomnienie spotkania: liczba minut z `MEETING_MINUTES`, `null`
/// (bez przypomnienia) albo `"default"` (usuwa nadpisanie).
pub fn set_meeting_reminder(
    db: &Connection,
    meeting_id: &str,
    value: &Value,
) -> Result<(), String> {
    let id = meeting_id.trim();
    if id.is_empty() || id.chars().count() > 512 {
        return Err("Niepoprawne spotkanie.".into());
    }
    let exists: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM calendar_events WHERE id=?1) OR EXISTS(SELECT 1 FROM manual_meetings WHERE id=?1)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err("Nie ma takiego spotkania.".into());
    }
    match value {
        Value::String(s) if s == "default" => {
            db.execute("DELETE FROM meeting_reminders WHERE meeting_id=?1", [id])
                .map_err(|e| e.to_string())?;
        }
        Value::Null => {
            db.execute(
                "INSERT INTO meeting_reminders(meeting_id,minutes) VALUES(?1,NULL) ON CONFLICT(meeting_id) DO UPDATE SET minutes=NULL",
                [id],
            )
            .map_err(|e| e.to_string())?;
        }
        Value::Number(n) => {
            let minutes = n
                .as_u64()
                .and_then(|m| u32::try_from(m).ok())
                .filter(|m| MEETING_MINUTES.contains(m))
                .ok_or("Przypomnienie przed spotkaniem: 1, 5, 10, 15 albo 30 minut.")?;
            db.execute(
                "INSERT INTO meeting_reminders(meeting_id,minutes) VALUES(?1,?2) ON CONFLICT(meeting_id) DO UPDATE SET minutes=excluded.minutes",
                params![id, minutes],
            )
            .map_err(|e| e.to_string())?;
        }
        _ => return Err("Niepoprawne przypomnienie spotkania.".into()),
    }
    Ok(())
}

/// Widok po „Pokaż”: kafelek przypiętego zadania albo Ustawienia → Powiadomienia.
pub fn show_view(task: Option<i64>) -> View {
    match task {
        Some(task) => View {
            task: Some(task),
            ..View::tasks()
        },
        None => View {
            view: Some("settings"),
            section: Some("notifications"),
            ..View::default()
        },
    }
}

fn clock_text(at: NaiveDateTime, today: NaiveDate) -> String {
    if at.date() == today {
        at.format("%H:%M").to_string()
    } else {
        at.format("%d.%m %H:%M").to_string()
    }
}

/// Treść toastu przypomnienia; `missed` dla przypomnienia z czasu, gdy
/// aplikacja nie działała.
fn reminder_message(
    r: &Reminder,
    key: String,
    due: NaiveDateTime,
    today: NaiveDate,
    missed: bool,
) -> Message {
    let mut lines = Vec::new();
    if missed {
        lines.push(format!("Ustawione na {}", clock_text(due, today)));
    } else {
        lines.push(r.text.clone());
    }
    if let Some(title) = &r.task_title {
        lines.push(format!("Zadanie: {}", notify::short(title)));
    }
    Message {
        key,
        title: if missed {
            format!("Przegapione przypomnienie: {}", notify::short(&r.text))
        } else {
            "Przypomnienie".into()
        },
        body: lines.join("\n"),
        actions: vec![
            Action::new(notify::REMINDER_SNOOZE, "Odłóż 10 min", r.id),
            Action::new(notify::REMINDER_SHOW, "Pokaż", r.id),
        ],
        open: show_view(r.task_id),
    }
}

/// Przypomnienia własne w chwili `now`: wystrzeliwuje te, których `at` (albo
/// odłożenie) minęło. Klucz `rem:<id>:<termin>` jest zapisywany przed
/// wysyłką; stan przypomnienia przesuwa się zawsze, także gdy klucz już był
/// (restart po wysłaniu toastu, ale przed zapisem stanu). `startup` = pierwszy
/// przebieg po starcie aplikacji: spóźnione o minutę i więcej są „przegapione”.
pub fn fire_due<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    now: i64,
    startup: bool,
) -> rusqlite::Result<Vec<Message>> {
    fire_due_tracked(db, tz, now, startup).map(|(out, _)| out)
}

/// `fire_due` z flagą „zmieniono tabelę `reminders`” (wykonane jednorazowe,
/// przesunięte powtarzalne, zdjęte odłożenie) — także gdy toast nie wychodzi,
/// bo klucz już był. Pętla w `notify::start` wysyła wtedy `EVENT`, żeby lista
/// i znacznik 🔔 na kafelku odświeżyły się bez przeładowania.
pub fn fire_due_tracked<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    now: i64,
    startup: bool,
) -> rusqlite::Result<(Vec<Message>, bool)> {
    let (Some(current), Some(exact)) = (local_minute(tz, now), local_exact(tz, now)) else {
        return Ok((Vec::new(), false));
    };
    let mut changed = false;
    let today = current.date();
    let reminders = list(db).map_err(|e| rusqlite::Error::ToSqlConversionFailure(e.into()))?;
    let mut out = Vec::new();
    for r in reminders.into_iter().filter(|r| r.enabled) {
        let Some(at) = parse_at(&r.at) else { continue };
        let snoozed = r.snooze_until.as_deref().and_then(parse_at);
        // Termin własny ma pierwszeństwo (odłożenie, które i tak minęło, znika razem z nim).
        let (due, own) = if at <= current {
            (at, true)
        } else if let Some(s) = snoozed.filter(|s| *s <= current) {
            (s, false)
        } else {
            continue;
        };
        let key = format!("rem:{}:{}", r.id, format_at(due));
        let fresh = notify::claim(db, &key, now)?;
        changed = true;
        let weekdays = &r.weekdays;
        if r.repeat == "none" {
            db.execute(
                "UPDATE reminders SET enabled=0,snooze_until=NULL,last_fired_at=?2 WHERE id=?1",
                params![r.id, now],
            )?;
        } else if own {
            let next = next_occurrence(&r.repeat, weekdays, at, exact).unwrap_or(at);
            db.execute(
                "UPDATE reminders SET at=?2,snooze_until=NULL,last_fired_at=?3 WHERE id=?1",
                params![r.id, format_at(next), now],
            )?;
        } else {
            db.execute(
                "UPDATE reminders SET snooze_until=NULL,last_fired_at=?2 WHERE id=?1",
                params![r.id, now],
            )?;
        }
        if fresh {
            let late = (exact - due).num_milliseconds();
            out.push(reminder_message(
                &r,
                key,
                due,
                today,
                startup && late >= MISSED_MS,
            ));
        }
    }
    Ok((out, changed))
}

/// Przypomnienia przed spotkaniem w chwili `now`: spotkania z `meetings_between`
/// (Outlook przy połączonym koncie i ręczne), które zaczynają się za najwyżej
/// tyle minut, ile ustawiono (domyślnie `meeting_reminder_minutes`, nadpisane
/// w `meeting_reminders`). Bez całodniowych, `free`, anulowanych i odrzuconych;
/// przesunięte spotkanie ma nowy start, więc nowy klucz `meet:<id>:<start>`.
/// Tytuł spotkania z Outlooka ukrywa `calendar_show_titles`; ręczne wpisał
/// użytkownik. Działa także po „Zakończ dzień”.
pub fn meeting_messages<Tz: TimeZone>(
    db: &Connection,
    s: &AppSettings,
    tz: &Tz,
    now: i64,
) -> rusqlite::Result<Vec<Message>> {
    // Wyłącznik obejmuje wszystkie spotkania, także te z własnym ustawieniem
    // (plan C3 pkt 5); nadpisanie działa tylko przy włączonych przypomnieniach.
    if !s.meeting_reminder_enabled {
        return Ok(Vec::new());
    }
    let longest = i64::from(*MEETING_MINUTES.last().unwrap_or(&30)) * 60_000;
    let events = calendar::meetings_between(db, now, now + longest + 60_000).unwrap_or_default();
    if events.is_empty() {
        return Ok(Vec::new());
    }
    let overrides = meeting_overrides(db).unwrap_or_default();
    let mut out = Vec::new();
    for e in events {
        if e.all_day
            || e.show_as == "free"
            || e.cancelled
            || e.response == "declined"
            || e.start <= now
        {
            continue;
        }
        let lead = match overrides.iter().find(|o| o.meeting_id == e.id) {
            Some(o) => o.minutes,
            None => Some(s.meeting_reminder_minutes),
        };
        let Some(lead) = lead else { continue };
        if now < e.start - i64::from(lead) * 60_000 {
            continue;
        }
        let key = format!("meet:{}:{}", e.id, e.start);
        if !notify::claim(db, &key, now)? {
            continue;
        }
        let left = ((e.start - now + 59_999) / 60_000).max(1);
        let subject = if e.source == meetings::MANUAL {
            e.subject.trim().to_owned()
        } else if s.calendar_show_titles && !e.subject.trim().is_empty() {
            e.subject.trim().to_owned()
        } else {
            "spotkanie".to_owned()
        };
        let (Some(from), Some(to)) = (local_exact(tz, e.start), local_exact(tz, e.end)) else {
            continue;
        };
        out.push(Message {
            key,
            title: "Przypomnienie o spotkaniu".into(),
            body: format!(
                "Za {left} min: {} ({}–{})",
                notify::short(&subject),
                from.format("%H:%M"),
                to.format("%H:%M")
            ),
            actions: Vec::new(),
            open: View {
                view: Some("journal"),
                ..View::default()
            },
        });
    }
    Ok(out)
}

fn emit_changed(app: &tauri::AppHandle) {
    let _ = app.emit(EVENT, ());
}

/// Lista „Moje przypomnienia”.
#[tauri::command]
pub fn reminder_list(app: tauri::AppHandle) -> Result<Vec<Reminder>, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    list(&db)
}

/// „Zapisz” w formularzu nowego przypomnienia.
#[tauri::command]
pub fn reminder_add(app: tauri::AppHandle, reminder: ReminderInput) -> Result<Reminder, String> {
    let saved = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        add(&db, &chrono::Local, &reminder, db::now())?
    };
    emit_changed(&app);
    Ok(saved)
}

/// „Zapisz” w formularzu istniejącego przypomnienia i przełącznik włączenia.
#[tauri::command]
pub fn reminder_update(
    app: tauri::AppHandle,
    id: i64,
    reminder: ReminderInput,
) -> Result<Reminder, String> {
    let saved = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        update(&db, &chrono::Local, id, &reminder, db::now())?
    };
    emit_changed(&app);
    Ok(saved)
}

/// „Usuń” przypomnienie (po potwierdzeniu).
#[tauri::command]
pub fn reminder_delete(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        delete(&db, id)?;
    }
    emit_changed(&app);
    Ok(())
}

/// Odłożenie przypomnienia o `minutes` minut.
#[tauri::command]
pub fn reminder_snooze(app: tauri::AppHandle, id: i64, minutes: i64) -> Result<Reminder, String> {
    let saved = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        snooze(&db, &chrono::Local, id, minutes, db::now())?
            .ok_or("Nie ma takiego przypomnienia.")?
    };
    emit_changed(&app);
    Ok(saved)
}

/// Nadpisania przypomnień spotkań (formularz spotkania i okno przypomnienia).
#[tauri::command]
pub fn meeting_reminder_list(app: tauri::AppHandle) -> Result<Vec<MeetingOverride>, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    meeting_overrides(&db)
}

/// Przypomnienie jednego spotkania: minuty, `null` (bez przypomnienia) albo `"default"`.
#[tauri::command]
pub fn meeting_reminder_set(
    app: tauri::AppHandle,
    meeting_id: String,
    minutes: Value,
) -> Result<(), String> {
    {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        set_meeting_reminder(&db, &meeting_id, &minutes)?;
    }
    emit_changed(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;
    use serde_json::json;

    const MIN: i64 = 60_000;

    fn at(s: &str) -> i64 {
        Warsaw
            .from_local_datetime(&NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis()
    }
    fn naive(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }
    fn setup() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db.execute_batch(
            "INSERT INTO tasks(title,created_at) VALUES('Raport',0);
             INSERT INTO tasks(title,created_at) VALUES('W koszu',0);
             INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(2,5);",
        )
        .unwrap();
        db
    }
    fn input(text: &str, when: &str, repeat: &str) -> ReminderInput {
        ReminderInput {
            text: text.into(),
            at: when.into(),
            repeat: repeat.into(),
            weekdays: Vec::new(),
            task_id: None,
            enabled: true,
        }
    }
    fn sent(db: &Connection) -> Vec<String> {
        db.prepare("SELECT key FROM notifications_sent ORDER BY key")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }

    #[test]
    fn validates_text_time_repeat_weekdays_and_task() {
        let db = setup();
        let now = at("2026-10-15 09:00");
        let ok = add(
            &db,
            &Warsaw,
            &input("  Zadzwonić  ", "2026-10-15T10:00", "none"),
            now,
        )
        .unwrap();
        assert_eq!(
            (ok.text.as_str(), ok.at.as_str()),
            ("Zadzwonić", "2026-10-15T10:00")
        );
        assert!(ok.enabled && ok.weekdays.is_empty() && ok.task_id.is_none());

        let err = |i: ReminderInput| add(&db, &Warsaw, &i, now).unwrap_err();
        assert_eq!(
            err(input("   ", "2026-10-15T10:00", "none")),
            "Podaj treść przypomnienia."
        );
        let long = "x".repeat(MAX_TEXT + 1);
        assert!(err(input(&long, "2026-10-15T10:00", "none")).contains("200"));
        // 200 znaków (także wielobajtowych) przechodzi.
        add(
            &db,
            &Warsaw,
            &input(&"ż".repeat(MAX_TEXT), "2026-10-15T10:00", "none"),
            now,
        )
        .unwrap();
        for bad in [
            "",
            "jutro",
            "2026-10-15 10:00",
            "2026-13-01T10:00",
            "2026-10-15T25:00",
        ] {
            assert!(
                err(input("A", bad, "none")).contains("poprawny dzień i godzinę"),
                "{bad}"
            );
        }
        assert!(err(input("A", "2026-10-15T10:00", "hourly")).contains("powtarzania"));
        // Jednorazowe nie w przeszłości (bieżąca minuta jeszcze jest).
        assert!(err(input("A", "2026-10-15T08:59", "none")).contains("w przyszłości"));
        add(&db, &Warsaw, &input("A", "2026-10-15T09:00", "none"), now).unwrap();
        // Wyłączone jednorazowe może być w przeszłości (pozycja „wykonane”).
        let mut done = input("A", "2026-10-14T09:00", "none");
        done.enabled = false;
        add(&db, &Warsaw, &done, now).unwrap();
        // `weekly` wymaga dni z 1–7.
        let mut weekly = input("A", "2026-10-15T10:00", "weekly");
        assert!(err(weekly.clone()).contains("co najmniej jeden dzień"));
        weekly.weekdays = vec![0];
        assert!(err(weekly.clone()).contains("od 1 do 7"));
        weekly.weekdays = vec![8];
        assert!(err(weekly.clone()).contains("od 1 do 7"));
        weekly.weekdays = vec![5, 1, 5];
        let saved = add(&db, &Warsaw, &weekly, now).unwrap();
        assert_eq!(saved.weekdays, vec![1, 5]);
        // Dni zbędne dla innych rodzajów znikają.
        let mut daily = input("A", "2026-10-15T10:00", "daily");
        daily.weekdays = vec![2];
        assert!(add(&db, &Warsaw, &daily, now).unwrap().weekdays.is_empty());
        // Zadanie istnieje i nie jest w koszu.
        let mut with_task = input("A", "2026-10-15T10:00", "none");
        with_task.task_id = Some(99);
        assert_eq!(err(with_task.clone()), "Wybrane zadanie nie istnieje.");
        with_task.task_id = Some(2);
        assert_eq!(err(with_task.clone()), "Wybrane zadanie nie istnieje.");
        with_task.task_id = Some(1);
        let linked = add(&db, &Warsaw, &with_task, now).unwrap();
        assert_eq!(linked.task_title.as_deref(), Some("Raport"));
    }

    #[test]
    fn repeating_start_moves_to_first_matching_occurrence() {
        let db = setup();
        let now = at("2026-10-15 09:30"); // czwartek
                                          // Dzisiejsza godzina już minęła: pierwsze wystąpienie jutro.
        let daily = add(&db, &Warsaw, &input("A", "2026-10-15T09:00", "daily"), now).unwrap();
        assert_eq!(daily.at, "2026-10-16T09:00");
        // Dzisiejsza godzina jeszcze przed nami: dziś.
        let daily = add(&db, &Warsaw, &input("A", "2026-10-15T10:00", "daily"), now).unwrap();
        assert_eq!(daily.at, "2026-10-15T10:00");
        // Data z przeszłości dla powtarzalnego nie cofa harmonogramu.
        let old = add(&db, &Warsaw, &input("A", "2026-09-01T10:00", "daily"), now).unwrap();
        assert_eq!(old.at, "2026-10-15T10:00");
        // „W dni robocze” z początkiem w sobotę zaczyna w poniedziałek.
        let work = add(
            &db,
            &Warsaw,
            &input("A", "2026-10-17T08:00", "workdays"),
            now,
        )
        .unwrap();
        assert_eq!(work.at, "2026-10-19T08:00");
        // `weekly` w poniedziałki i piątki z początkiem w czwartek: piątek.
        let mut weekly = input("A", "2026-10-15T08:00", "weekly");
        weekly.weekdays = vec![1, 5];
        assert_eq!(
            add(&db, &Warsaw, &weekly, now).unwrap().at,
            "2026-10-16T08:00"
        );
    }

    #[test]
    fn next_occurrence_daily_workdays_and_weekly() {
        // Codziennie: następny dzień o tej samej godzinie.
        assert_eq!(
            next_occurrence(
                "daily",
                &[],
                naive("2026-10-15 09:00"),
                naive("2026-10-15 09:00")
            ),
            Some(naive("2026-10-16 09:00"))
        );
        // Po długiej przerwie: pierwsze wystąpienie po teraz, nie po `at`.
        assert_eq!(
            next_occurrence(
                "daily",
                &[],
                naive("2026-10-10 09:00"),
                naive("2026-10-15 09:30")
            ),
            Some(naive("2026-10-16 09:00"))
        );
        assert_eq!(
            next_occurrence(
                "daily",
                &[],
                naive("2026-10-10 09:00"),
                naive("2026-10-15 08:00")
            ),
            Some(naive("2026-10-15 09:00"))
        );
        // W dni robocze: piątek → poniedziałek (weekend pominięty).
        assert_eq!(
            next_occurrence(
                "workdays",
                &[],
                naive("2026-10-16 09:00"),
                naive("2026-10-16 09:00")
            ),
            Some(naive("2026-10-19 09:00"))
        );
        // Święto: 2026-11-11 (środa) pominięte, a 11.11 w poniedziałek–piątek nie jest roboczym.
        assert_eq!(
            next_occurrence(
                "workdays",
                &[],
                naive("2026-11-10 09:00"),
                naive("2026-11-10 09:00")
            ),
            Some(naive("2026-11-12 09:00"))
        );
        // Boże Narodzenie (piątek 25.12) i weekend: następny dzień roboczy to poniedziałek.
        assert_eq!(
            next_occurrence(
                "workdays",
                &[],
                naive("2026-12-24 09:00"),
                naive("2026-12-24 09:00")
            ),
            Some(naive("2026-12-28 09:00"))
        );
        // Co tydzień: dni z listy, kolejność niezależna od `at`.
        assert_eq!(
            next_occurrence(
                "weekly",
                &[1, 3, 5],
                naive("2026-10-14 10:00"),
                naive("2026-10-14 10:00")
            ),
            Some(naive("2026-10-16 10:00"))
        );
        assert_eq!(
            next_occurrence(
                "weekly",
                &[1, 3, 5],
                naive("2026-10-16 10:00"),
                naive("2026-10-16 10:00")
            ),
            Some(naive("2026-10-19 10:00"))
        );
        assert_eq!(
            next_occurrence(
                "weekly",
                &[7],
                naive("2026-10-16 10:00"),
                naive("2026-10-16 10:00")
            ),
            Some(naive("2026-10-18 10:00"))
        );
        // Jednorazowe nie ma następnego.
        assert_eq!(
            next_occurrence(
                "none",
                &[],
                naive("2026-10-16 10:00"),
                naive("2026-10-16 10:00")
            ),
            None
        );
    }

    #[test]
    fn update_and_delete_and_unknown_id() {
        let db = setup();
        let now = at("2026-10-15 09:00");
        let r = add(&db, &Warsaw, &input("A", "2026-10-15T10:00", "daily"), now).unwrap();
        let mut change = input("B", "2026-10-15T11:00", "daily");
        change.enabled = false;
        let saved = update(&db, &Warsaw, r.id, &change, now + MIN).unwrap();
        assert_eq!(
            (saved.text.as_str(), saved.at.as_str(), saved.enabled),
            ("B", "2026-10-15T11:00", false)
        );
        assert_eq!(saved.updated_at, now + MIN);
        assert!(update(&db, &Warsaw, 999, &change, now)
            .unwrap_err()
            .contains("Nie ma takiego"));
        delete(&db, r.id).unwrap();
        assert!(get(&db, r.id).unwrap().is_none());
        assert!(delete(&db, r.id).unwrap_err().contains("Nie ma takiego"));
    }

    #[test]
    fn list_orders_upcoming_then_disabled() {
        let db = setup();
        let now = at("2026-10-15 09:00");
        let late = add(
            &db,
            &Warsaw,
            &input("późno", "2026-10-15T18:00", "none"),
            now,
        )
        .unwrap();
        let soon = add(
            &db,
            &Warsaw,
            &input("wkrótce", "2026-10-15T10:00", "none"),
            now,
        )
        .unwrap();
        let mut off = input("wyłączone", "2026-10-15T09:30", "none");
        off.enabled = false;
        let off = add(&db, &Warsaw, &off, now).unwrap();
        let ids: Vec<i64> = list(&db).unwrap().iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![soon.id, late.id, off.id]);
    }

    #[test]
    fn fire_due_reports_changed_state() {
        let db = setup();
        let r = add(
            &db,
            &Warsaw,
            &input("Zmiana", "2026-10-15T10:00", "none"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        // Przed terminem: bez zmian.
        let (out, changed) = fire_due_tracked(&db, &Warsaw, at("2026-10-15 09:59"), false).unwrap();
        assert!(out.is_empty() && !changed);
        // Termin: toast i zmiana stanu (wykonane).
        let (out, changed) = fire_due_tracked(&db, &Warsaw, at("2026-10-15 10:00"), false).unwrap();
        assert!(out.len() == 1 && changed);
        // Kolejny przebieg: przypomnienie wyłączone, nic się nie zmienia.
        let (out, changed) = fire_due_tracked(&db, &Warsaw, at("2026-10-15 10:01"), false).unwrap();
        assert!(out.is_empty() && !changed);
        // Klucz już zapisany, a stan nie (restart po toaście): bez toastu, ale ze zmianą.
        db.execute(
            "UPDATE reminders SET enabled=1,at='2026-10-15T11:00' WHERE id=?1",
            [r.id],
        )
        .unwrap();
        assert!(notify::claim(&db, &format!("rem:{}:2026-10-15T11:00", r.id), 0).unwrap());
        let (out, changed) = fire_due_tracked(&db, &Warsaw, at("2026-10-15 11:00"), false).unwrap();
        assert!(out.is_empty() && changed);
        assert!(!get(&db, r.id).unwrap().unwrap().enabled);
    }

    #[test]
    fn deleting_task_keeps_reminder_without_task() {
        let db = setup();
        let mut with_task = input("A", "2026-10-15T10:00", "none");
        with_task.task_id = Some(1);
        let r = add(&db, &Warsaw, &with_task, at("2026-10-15 09:00")).unwrap();
        assert_eq!(r.task_id, Some(1));
        db.execute("DELETE FROM tasks WHERE id=1", []).unwrap();
        let after = get(&db, r.id).unwrap().unwrap();
        assert_eq!((after.task_id, after.task_title), (None, None));
    }

    #[test]
    fn one_off_fires_exactly_once_then_is_disabled() {
        let db = setup();
        let r = add(
            &db,
            &Warsaw,
            &input("Zadzwonić", "2026-10-15T10:00", "none"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        // Przed terminem nic.
        assert!(fire_due(&db, &Warsaw, at("2026-10-15 09:59"), false)
            .unwrap()
            .is_empty());
        let sent_now = fire_due(&db, &Warsaw, at("2026-10-15 10:00") + 10_000, false).unwrap();
        assert_eq!(sent_now.len(), 1);
        let m = &sent_now[0];
        assert_eq!(m.key, format!("rem:{}:2026-10-15T10:00", r.id));
        assert_eq!(
            (m.title.as_str(), m.body.as_str()),
            ("Przypomnienie", "Zadzwonić")
        );
        assert_eq!(
            m.actions
                .iter()
                .map(|a| (a.id, a.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (notify::REMINDER_SNOOZE, "Odłóż 10 min"),
                (notify::REMINDER_SHOW, "Pokaż")
            ]
        );
        let after = get(&db, r.id).unwrap().unwrap();
        assert!(!after.enabled);
        assert_eq!(after.last_fired_at, Some(at("2026-10-15 10:00") + 10_000));
        // Kolejne sprawdzenia i restart milczą.
        assert!(fire_due(&db, &Warsaw, at("2026-10-15 10:01"), false)
            .unwrap()
            .is_empty());
        assert!(fire_due(&db, &Warsaw, at("2026-10-15 10:02"), true)
            .unwrap()
            .is_empty());
        assert_eq!(sent(&db), vec![format!("rem:{}:2026-10-15T10:00", r.id)]);
    }

    #[test]
    fn key_prevents_repeat_after_restart_and_state_still_advances() {
        let db = setup();
        let r = add(
            &db,
            &Warsaw,
            &input("Raport", "2026-10-15T10:00", "daily"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        // Toast poszedł (klucz zapisany), ale stan nie został przesunięty.
        db.execute(
            "INSERT INTO notifications_sent(key,at) VALUES(?1,0)",
            [format!("rem:{}:2026-10-15T10:00", r.id)],
        )
        .unwrap();
        let out = fire_due(&db, &Warsaw, at("2026-10-15 10:00"), true).unwrap();
        assert!(out.is_empty(), "klucz już był");
        assert_eq!(get(&db, r.id).unwrap().unwrap().at, "2026-10-16T10:00");
    }

    #[test]
    fn repeating_moves_to_next_occurrence_and_skips_weekends_and_holidays() {
        let db = setup();
        // Piątek 2026-10-16: następne w poniedziałek.
        let work = add(
            &db,
            &Warsaw,
            &input("Stand-up", "2026-10-16T09:00", "workdays"),
            at("2026-10-16 08:00"),
        )
        .unwrap();
        assert_eq!(
            fire_due(&db, &Warsaw, at("2026-10-16 09:00"), false)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(get(&db, work.id).unwrap().unwrap().at, "2026-10-19T09:00");
        assert!(get(&db, work.id).unwrap().unwrap().enabled);
        // Po wystrzeleniu w poniedziałek: wtorek.
        assert_eq!(
            fire_due(&db, &Warsaw, at("2026-10-19 09:00"), false)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(get(&db, work.id).unwrap().unwrap().at, "2026-10-20T09:00");

        // Święto 1 listopada 2026 to niedziela; 11 listopada (środa) jest świętem.
        let holiday = add(
            &db,
            &Warsaw,
            &input("Święto", "2026-11-10T09:00", "workdays"),
            at("2026-11-10 08:00"),
        )
        .unwrap();
        fire_due(&db, &Warsaw, at("2026-11-10 09:00"), false).unwrap();
        assert_eq!(
            get(&db, holiday.id).unwrap().unwrap().at,
            "2026-11-12T09:00"
        );

        // Co tydzień: poniedziałek i piątek.
        let mut weekly = input("Tygodniowe", "2026-10-16T12:00", "weekly");
        weekly.weekdays = vec![1, 5];
        let weekly = add(&db, &Warsaw, &weekly, at("2026-10-16 08:00")).unwrap();
        fire_due(&db, &Warsaw, at("2026-10-16 12:00"), false).unwrap();
        assert_eq!(get(&db, weekly.id).unwrap().unwrap().at, "2026-10-19T12:00");
        fire_due(&db, &Warsaw, at("2026-10-19 12:00"), false).unwrap();
        assert_eq!(get(&db, weekly.id).unwrap().unwrap().at, "2026-10-23T12:00");
    }

    #[test]
    fn missed_reminders_fire_once_after_start_and_repeating_jump_to_future() {
        let db = setup();
        let once = add(
            &db,
            &Warsaw,
            &input("Raz", "2026-10-15T09:00", "none"),
            at("2026-10-15 08:00"),
        )
        .unwrap();
        let daily = add(
            &db,
            &Warsaw,
            &input("Codziennie", "2026-10-15T09:30", "daily"),
            at("2026-10-15 08:00"),
        )
        .unwrap();
        let mut with_task = input("Z zadaniem", "2026-10-15T09:45", "none");
        with_task.task_id = Some(1);
        let linked = add(&db, &Warsaw, &with_task, at("2026-10-15 08:00")).unwrap();
        // Aplikacja nie działała do 2026-10-17 12:00 (dwa dni później).
        let now = at("2026-10-17 12:00");
        let out = fire_due(&db, &Warsaw, now, true).unwrap();
        assert_eq!(out.len(), 3);
        let titles: Vec<&str> = out.iter().map(|m| m.title.as_str()).collect();
        assert!(
            titles.contains(&"Przegapione przypomnienie: Raz"),
            "{titles:?}"
        );
        assert!(titles.contains(&"Przegapione przypomnienie: Codziennie"));
        let with = out
            .iter()
            .find(|m| m.title.ends_with("Z zadaniem"))
            .unwrap();
        assert!(with.body.contains("Zadanie: Raport"), "{}", with.body);
        assert!(with.body.contains("15.10 09:45"), "{}", with.body);
        assert!(!get(&db, once.id).unwrap().unwrap().enabled);
        assert!(!get(&db, linked.id).unwrap().unwrap().enabled);
        // Powtarzalne: jedno powiadomienie, następne wystąpienie w przyszłości.
        assert_eq!(get(&db, daily.id).unwrap().unwrap().at, "2026-10-18T09:30");
        // Drugi przebieg niczego nie dubluje.
        assert!(fire_due(&db, &Warsaw, now + 30_000, false)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn startup_within_a_minute_is_not_called_missed() {
        let db = setup();
        add(
            &db,
            &Warsaw,
            &input("Tuż", "2026-10-15T10:00", "none"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        let out = fire_due(&db, &Warsaw, at("2026-10-15 10:00") + 20_000, true).unwrap();
        assert_eq!(out[0].title, "Przypomnienie");
    }

    #[test]
    fn snooze_one_off_moves_at_and_reenables() {
        let db = setup();
        let now = at("2026-10-15 10:00");
        let r = add(&db, &Warsaw, &input("A", "2026-10-15T10:00", "none"), now).unwrap();
        fire_due(&db, &Warsaw, now, false).unwrap();
        assert!(!get(&db, r.id).unwrap().unwrap().enabled);
        let snoozed = snooze(&db, &Warsaw, r.id, SNOOZE_MINUTES, now + 20_000)
            .unwrap()
            .unwrap();
        assert!(snoozed.enabled);
        assert_eq!(snoozed.at, "2026-10-15T10:10");
        assert!(fire_due(&db, &Warsaw, at("2026-10-15 10:09"), false)
            .unwrap()
            .is_empty());
        let out = fire_due(&db, &Warsaw, at("2026-10-15 10:10"), false).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].key, format!("rem:{}:2026-10-15T10:10", r.id));
        assert!(!get(&db, r.id).unwrap().unwrap().enabled);
    }

    #[test]
    fn snooze_repeating_keeps_schedule() {
        let db = setup();
        let r = add(
            &db,
            &Warsaw,
            &input("A", "2026-10-15T09:00", "daily"),
            at("2026-10-15 08:00"),
        )
        .unwrap();
        fire_due(&db, &Warsaw, at("2026-10-15 09:00"), false).unwrap();
        let snoozed = snooze(&db, &Warsaw, r.id, 10, at("2026-10-15 09:01"))
            .unwrap()
            .unwrap();
        assert_eq!(snoozed.at, "2026-10-16T09:00");
        assert_eq!(snoozed.snooze_until.as_deref(), Some("2026-10-15T09:11"));
        let out = fire_due(&db, &Warsaw, at("2026-10-15 09:11"), false).unwrap();
        assert_eq!(out.len(), 1);
        let after = get(&db, r.id).unwrap().unwrap();
        assert_eq!(
            (after.at.as_str(), after.snooze_until, after.enabled),
            ("2026-10-16T09:00", None, true)
        );
        // Następny dzień o 9:00 działa jak zwykle.
        assert_eq!(
            fire_due(&db, &Warsaw, at("2026-10-16 09:00"), false)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn snooze_validates_and_ignores_missing_reminder() {
        let db = setup();
        assert_eq!(snooze(&db, &Warsaw, 99, 10, 0).unwrap(), None);
        let r = add(
            &db,
            &Warsaw,
            &input("A", "2026-10-15T10:00", "none"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        assert!(snooze(&db, &Warsaw, r.id, 0, 0).is_err());
        assert!(snooze(&db, &Warsaw, r.id, 1_441, 0).is_err());
        // Edycja czyści odłożenie.
        let rep = add(
            &db,
            &Warsaw,
            &input("A", "2026-10-16T10:00", "daily"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        snooze(&db, &Warsaw, rep.id, 30, at("2026-10-15 09:00")).unwrap();
        let saved = update(
            &db,
            &Warsaw,
            rep.id,
            &input("A", "2026-10-16T10:00", "daily"),
            at("2026-10-15 09:00"),
        )
        .unwrap();
        assert_eq!(saved.snooze_until, None);
    }

    #[test]
    fn disabled_reminder_does_not_fire() {
        let db = setup();
        let mut off = input("A", "2026-10-15T10:00", "daily");
        off.enabled = false;
        add(&db, &Warsaw, &off, at("2026-10-15 09:00")).unwrap();
        assert!(fire_due(&db, &Warsaw, at("2026-10-15 10:00"), false)
            .unwrap()
            .is_empty());
        assert!(sent(&db).is_empty());
    }

    // --- przypomnienia przed spotkaniem ---

    fn manual(
        db: &Connection,
        subject: &str,
        start: &str,
        end: &str,
        show_as: &str,
        all_day: bool,
    ) -> String {
        let id = format!("manual:{}-{}", at(start), subject.len());
        db.execute(
            "INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,0,0)",
            params![id, at(start), at(end), subject, show_as, all_day],
        )
        .unwrap();
        id
    }
    fn outlook(db: &Connection, id: &str, subject: &str, start: &str, end: &str) {
        db.execute(
            "INSERT INTO calendar_events(id,start,\"end\",subject,show_as,all_day,cancelled,response,updated_at) VALUES(?1,?2,?3,?4,'busy',0,0,'accepted',0)",
            params![id, at(start), at(end), subject],
        )
        .unwrap();
        // Spotkania z Outlooka liczą się przy połączonym koncie.
        calendar::set_test_email(db, "a@b.pl");
    }
    fn meeting_settings() -> AppSettings {
        AppSettings::default()
    }
    fn texts(m: &[Message]) -> Vec<String> {
        m.iter().map(|m| m.body.clone()).collect()
    }

    #[test]
    fn meeting_reminder_uses_default_minutes_once() {
        let db = setup();
        let id = manual(
            &db,
            "Telefon z klientem",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let s = meeting_settings();
        assert!(s.meeting_reminder_enabled && s.meeting_reminder_minutes == 5);
        // 6 minut przed: jeszcze nie.
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:54"))
            .unwrap()
            .is_empty());
        let out = meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:55")).unwrap();
        assert_eq!(
            texts(&out),
            vec!["Za 5 min: Telefon z klientem (10:00–10:30)"]
        );
        assert_eq!(out[0].key, format!("meet:{id}:{}", at("2026-10-15 10:00")));
        assert!(out[0].actions.is_empty());
        // Drugi raz (i po restarcie) nie.
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:56"))
            .unwrap()
            .is_empty());
        // Po starcie spotkania już nic.
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 10:01"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn meeting_reminder_counts_remaining_minutes_when_late() {
        let db = setup();
        manual(
            &db,
            "Spotkanie",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        // Aplikacja wystartowała 3 minuty przed spotkaniem.
        let out = meeting_messages(
            &db,
            &meeting_settings(),
            &Warsaw,
            at("2026-10-15 09:57") + 10_000,
        )
        .unwrap();
        assert_eq!(texts(&out), vec!["Za 3 min: Spotkanie (10:00–10:30)"]);
    }

    #[test]
    fn meeting_reminder_setting_minutes_and_disabled() {
        let db = setup();
        manual(
            &db,
            "A",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let mut s = meeting_settings();
        s.meeting_reminder_minutes = 15;
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:44"))
            .unwrap()
            .is_empty());
        assert_eq!(
            meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:45"))
                .unwrap()
                .len(),
            1
        );
        // Wyłączone ustawienie domyślne: cisza.
        let db = setup();
        manual(
            &db,
            "A",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        s.meeting_reminder_enabled = false;
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:50"))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn meeting_override_replaces_default_and_can_disable() {
        let db = setup();
        let a = manual(
            &db,
            "Dłuższe",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let b = manual(
            &db,
            "Bez",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let mut s = meeting_settings();
        set_meeting_reminder(&db, &a, &json!(15)).unwrap();
        set_meeting_reminder(&db, &b, &Value::Null).unwrap();
        // 15 min przed: tylko spotkanie z nadpisaniem.
        let out = meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:45")).unwrap();
        assert_eq!(texts(&out), vec!["Za 15 min: Dłuższe (10:00–10:30)"]);
        // 5 min przed: „Bez” wyłączone, „Dłuższe” już wysłane.
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:55"))
            .unwrap()
            .is_empty());
        // Wyłączone przypomnienia o spotkaniach wyciszają też nadpisania.
        let db = setup();
        let c = manual(
            &db,
            "Mimo to",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        s.meeting_reminder_enabled = false;
        set_meeting_reminder(&db, &c, &json!(10)).unwrap();
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:50"))
            .unwrap()
            .is_empty());
        // Po ponownym włączeniu nadpisanie (10 min) znów obowiązuje.
        s.meeting_reminder_enabled = true;
        assert_eq!(
            meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:50"))
                .unwrap()
                .len(),
            1
        );
        // „default” usuwa nadpisanie.
        let db = setup();
        let d = manual(
            &db,
            "Powrót",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        set_meeting_reminder(&db, &d, &Value::Null).unwrap();
        set_meeting_reminder(&db, &d, &json!("default")).unwrap();
        assert!(meeting_overrides(&db).unwrap().is_empty());
        assert_eq!(
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 09:55"))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn meeting_override_validation() {
        let db = setup();
        let id = manual(
            &db,
            "A",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        for bad in [
            json!(0),
            json!(7),
            json!(31),
            json!(-5),
            json!(5.5),
            json!("5"),
            json!(true),
        ] {
            assert!(set_meeting_reminder(&db, &id, &bad).is_err(), "{bad}");
        }
        assert!(set_meeting_reminder(&db, "manual:nie-ma", &json!(5))
            .unwrap_err()
            .contains("Nie ma takiego"));
        assert!(set_meeting_reminder(&db, "  ", &json!(5)).is_err());
        for ok in MEETING_MINUTES {
            set_meeting_reminder(&db, &id, &json!(ok)).unwrap();
        }
        assert_eq!(
            meeting_overrides(&db).unwrap(),
            vec![MeetingOverride {
                meeting_id: id,
                minutes: Some(30)
            }]
        );
    }

    #[test]
    fn moved_meeting_gets_new_reminder() {
        let db = setup();
        let id = manual(
            &db,
            "Przesuwane",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let s = meeting_settings();
        assert_eq!(
            meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:55"))
                .unwrap()
                .len(),
            1
        );
        // Spotkanie przesunięte na 11:00.
        db.execute(
            "UPDATE manual_meetings SET start=?2,\"end\"=?3 WHERE id=?1",
            params![id, at("2026-10-15 11:00"), at("2026-10-15 11:30")],
        )
        .unwrap();
        assert!(meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:56"))
            .unwrap()
            .is_empty());
        let out = meeting_messages(&db, &s, &Warsaw, at("2026-10-15 10:55")).unwrap();
        assert_eq!(texts(&out), vec!["Za 5 min: Przesuwane (11:00–11:30)"]);
        assert_eq!(sent(&db).len(), 2);
    }

    #[test]
    fn all_day_free_cancelled_and_declined_meetings_are_skipped() {
        let db = setup();
        manual(
            &db,
            "Cały dzień",
            "2026-10-15 00:00",
            "2026-10-16 00:00",
            "busy",
            true,
        );
        // Pora tuż po północy nie wystarczy: całodniowe pomijamy zawsze.
        assert!(
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-14 23:57"))
                .unwrap()
                .is_empty()
        );
        let free = |db: &Connection, id: &str, show_as: &str, cancelled: bool, response: &str| {
            db.execute(
                "INSERT INTO calendar_events(id,start,\"end\",subject,show_as,all_day,cancelled,response,updated_at) VALUES(?1,?2,?3,'X',?4,0,?5,?6,0)",
                params![id, at("2026-10-15 10:00"), at("2026-10-15 10:30"), show_as, cancelled, response],
            )
            .unwrap();
        };
        outlook(
            &db,
            "o-base",
            "Podstawa",
            "2026-10-15 12:00",
            "2026-10-15 12:30",
        );
        free(&db, "o-free", "free", false, "accepted");
        free(&db, "o-cancel", "busy", true, "accepted");
        free(&db, "o-declined", "busy", false, "declined");
        let out =
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 09:55")).unwrap();
        assert!(out.is_empty(), "{:?}", texts(&out));
        let out =
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 11:55")).unwrap();
        assert_eq!(texts(&out), vec!["Za 5 min: Podstawa (12:00–12:30)"]);
    }

    #[test]
    fn outlook_title_follows_show_titles_setting_and_manual_is_always_visible() {
        let db = setup();
        outlook(
            &db,
            "o-1",
            "Tajny projekt",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
        );
        manual(
            &db,
            "Telefon",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        let mut s = meeting_settings();
        s.calendar_show_titles = false;
        let mut out = texts(&meeting_messages(&db, &s, &Warsaw, at("2026-10-15 09:55")).unwrap());
        out.sort();
        assert_eq!(
            out,
            vec![
                "Za 5 min: Telefon (10:00–10:30)",
                "Za 5 min: spotkanie (10:00–10:30)"
            ]
        );
        let db = setup();
        outlook(
            &db,
            "o-1",
            "Tajny projekt",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
        );
        let out =
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 09:55")).unwrap();
        assert_eq!(texts(&out), vec!["Za 5 min: Tajny projekt (10:00–10:30)"]);
    }

    #[test]
    fn outlook_meetings_need_connected_account() {
        let db = setup();
        db.execute(
            "INSERT INTO calendar_events(id,start,\"end\",subject,show_as,all_day,cancelled,response,updated_at) VALUES('o-1',?1,?2,'X','busy',0,0,'accepted',0)",
            params![at("2026-10-15 10:00"), at("2026-10-15 10:30")],
        )
        .unwrap();
        assert!(
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 09:55"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn meeting_reminder_ignores_day_end() {
        // Przypomnienia o spotkaniach nie zależą od „Zakończ dzień” (B13).
        let db = setup();
        db.execute(
            "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES('2026-10-15',1,2,0)",
            [],
        )
        .unwrap();
        manual(
            &db,
            "Po pracy",
            "2026-10-15 17:00",
            "2026-10-15 17:30",
            "busy",
            false,
        );
        let out =
            meeting_messages(&db, &meeting_settings(), &Warsaw, at("2026-10-15 16:55")).unwrap();
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn deleting_manual_meeting_drops_its_override() {
        let db = setup();
        let id = manual(
            &db,
            "A",
            "2026-10-15 10:00",
            "2026-10-15 10:30",
            "busy",
            false,
        );
        set_meeting_reminder(&db, &id, &json!(10)).unwrap();
        meetings::delete(&db, &id).unwrap();
        assert!(meeting_overrides(&db).unwrap().is_empty());
    }
}
