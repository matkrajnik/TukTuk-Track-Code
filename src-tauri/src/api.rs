//! Zewnętrzne sterowanie pomiarem: dodaj zadanie (z opcjonalnym startem),
//! zatrzymaj pomiar, pobierz stan.
//!
//! Inny program (albo `tuktuk-track-ctl.exe`, `src/bin/ctl.rs`) wysyła żądanie
//! nazwanym potokiem `\\.\pipe\tuktuk-track-api-<użytkownik>` (protokół:
//! `api_wire.rs`). Potok przyjmuje tylko klientów z tego komputera, a domyślne
//! uprawnienia potoku nie dają zapisu innym kontom Windows. Serwer odpowiada
//! wyłącznie przy włączonym ustawieniu `external_api` (domyślnie wyłączone);
//! bez niego każde żądanie dostaje `DISABLED` i nic się nie zmienia.
//!
//! Zapisy idą tą samą drogą co w oknie: `quickadd::create_linked` (start
//! zatrzymuje poprzedni pomiar w tej samej transakcji) i `db::stop`. Po zmianie
//! aplikacja wysyła `data-changed` i potwierdzenie Start/Stop jak skrót
//! klawiszowy, więc okno, tray i widget pokazują ją od razu.
//!
//! Start po zakończonym dniu (B13) wznawia dzień bez pytania; odpowiedź ma
//! wtedy `dayResumed: true`.
//!
//! Treść żądań nie trafia do logów.
use crate::api_wire::{self, code};
use crate::{days, db, quickadd};
use chrono::{SecondsFormat, TimeZone};
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Display;
use tauri::{Emitter, Manager};

pub const DISABLED: &str =
    "Zewnętrzne sterowanie jest wyłączone. Włącz je w Ustawieniach → Agenci → Zewnętrzne aplikacje.";
const UNKNOWN_ACTION: &str = "Nieznana akcja. Dostępne: add, stop, status.";
const STORAGE_ERROR: &str = "Błąd zapisu albo odczytu danych TukTuk Track.";

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
struct AddRequest {
    action: String,
    title: String,
    jira_key: Option<String>,
    redmine_id: Option<u64>,
    start: bool,
}

/// Połączone konta do powiązania zadania ze zgłoszeniem (jak `quick_add`).
/// `None` jako adres = konto niepołączone, klucz albo numer jest odrzucany.
pub struct Links<'a> {
    pub jira_site: Option<String>,
    pub redmine_site: Option<String>,
    pub summary: &'a dyn Fn(&str) -> Option<String>,
    pub subject: &'a dyn Fn(u64) -> Option<String>,
}

impl Links<'_> {
    /// Bez połączonych kont (stop, status, testy).
    pub fn none() -> Links<'static> {
        Links {
            jira_site: None,
            redmine_site: None,
            summary: &|_| None,
            subject: &|_| None,
        }
    }
}

/// Co aplikacja ma zrobić po odpowiedzi.
#[derive(Debug, Default, PartialEq)]
pub struct Effect {
    /// Dane się zmieniły: `data-changed` i przypomnienie od nowa.
    pub changed: bool,
    /// Potwierdzenie „Start: X” / „Stop: X · 01:02:03” (tray, widget, powiadomienie).
    pub feedback: Option<String>,
    /// Dzień był zakończony i został wznowiony.
    pub day_resumed: bool,
}

pub struct Outcome {
    pub reply: Value,
    pub effect: Effect,
}

impl Outcome {
    fn error(code: &str, message: impl Into<String>) -> Self {
        Self {
            reply: api_wire::error(code, message),
            effect: Effect::default(),
        }
    }
}

/// Trwający pomiar.
struct Active {
    task_id: i64,
    title: String,
    jira_key: Option<String>,
    redmine_id: Option<i64>,
    started_at: i64,
}

fn active(db: &Connection) -> rusqlite::Result<Option<Active>> {
    db.query_row(
        "SELECT s.task_id, s.started_at, t.title, j.issue_key, r.issue_id
         FROM sessions s JOIN tasks t ON t.id=s.task_id
         LEFT JOIN jira_tasks j ON j.task_id=t.id
         LEFT JOIN redmine_tasks r ON r.task_id=t.id
         WHERE s.ended_at IS NULL ORDER BY s.id DESC LIMIT 1",
        [],
        |r| {
            Ok(Active {
                task_id: r.get(0)?,
                started_at: r.get(1)?,
                title: r.get(2)?,
                jira_key: r.get(3)?,
                redmine_id: r.get(4)?,
            })
        },
    )
    .optional()
}

/// Chwila w ISO 8601 z przesunięciem strefy (`2026-11-02T09:15:00+01:00`).
fn iso<Tz: TimeZone>(tz: &Tz, ms: i64) -> Option<String>
where
    Tz::Offset: Display,
{
    tz.timestamp_millis_opt(ms)
        .earliest()
        .map(|d| d.to_rfc3339_opts(SecondsFormat::Secs, false))
}

fn status_json<Tz: TimeZone>(tz: &Tz, active: Option<&Active>, now: i64) -> Value
where
    Tz::Offset: Display,
{
    match active {
        None => json!({
            "running": false,
            "task": null,
            "startedAt": null,
            "startedAtMs": null,
            "elapsedSeconds": 0,
        }),
        Some(a) => json!({
            "running": true,
            "task": {
                "id": a.task_id,
                "title": a.title,
                "jiraKey": a.jira_key,
                "redmineId": a.redmine_id,
            },
            "startedAt": iso(tz, a.started_at),
            "startedAtMs": a.started_at,
            "elapsedSeconds": (now - a.started_at).max(0) / 1000,
        }),
    }
}

/// Obsługa jednego żądania. `enabled` = ustawienie `external_api`.
pub fn handle<Tz: TimeZone>(
    enabled: bool,
    db: &mut Connection,
    tz: &Tz,
    links: &Links,
    request: &Value,
    now: i64,
) -> Outcome
where
    Tz::Offset: Display,
{
    if !enabled {
        return Outcome::error(code::DISABLED, DISABLED);
    }
    let result = match request["action"].as_str() {
        Some("status") => active(db)
            .map_err(storage)
            .map(|a| (status_json(tz, a.as_ref(), now), Effect::default())),
        Some("stop") => stop(db, tz, now),
        Some("add") => match serde_json::from_value::<AddRequest>(request.clone()) {
            Ok(add_request) => add(db, tz, links, add_request, now),
            Err(e) => Err((code::BAD_REQUEST, format!("Niepoprawne pola żądania: {e}."))),
        },
        _ => Err((code::BAD_REQUEST, UNKNOWN_ACTION.to_string())),
    };
    match result {
        Ok((data, effect)) => Outcome {
            reply: api_wire::ok(data),
            effect,
        },
        Err((code, message)) => Outcome::error(code, message),
    }
}

type Handled = Result<(Value, Effect), (&'static str, String)>;

/// Szczegół błędu zostaje lokalnie (stderr); klient dostaje stały komunikat.
fn storage(e: impl Display) -> (&'static str, String) {
    eprintln!("Zewnętrzne sterowanie: błąd bazy ({e}).");
    (code::STORAGE, STORAGE_ERROR.to_string())
}

fn stop<Tz: TimeZone>(db: &Connection, tz: &Tz, now: i64) -> Handled
where
    Tz::Offset: Display,
{
    let before = active(db).map_err(storage)?;
    let Some(stopped) = before else {
        return Ok((
            json!({ "stopped": null, "status": status_json(tz, None, now) }),
            Effect::default(),
        ));
    };
    db::stop(db, now).map_err(storage)?;
    let elapsed = (now - stopped.started_at).max(0);
    let data = json!({
        "stopped": {
            "taskId": stopped.task_id,
            "title": stopped.title,
            "durationSeconds": elapsed / 1000,
        },
        "status": status_json(tz, None, now),
    });
    let effect = Effect {
        changed: true,
        feedback: Some(format!(
            "Stop: {} · {}",
            stopped.title,
            crate::tray::hms(elapsed)
        )),
        day_resumed: false,
    };
    Ok((data, effect))
}

fn add<Tz: TimeZone>(
    db: &mut Connection,
    tz: &Tz,
    links: &Links,
    request: AddRequest,
    now: i64,
) -> Handled
where
    Tz::Offset: Display,
{
    let start = request.start;
    let day_ended = start && days::ended_at(db, tz, now).is_some();
    quickadd::create_linked(
        db,
        links.jira_site.as_deref(),
        |key| (links.summary)(key),
        links.redmine_site.as_deref(),
        |id| (links.subject)(id),
        quickadd::NewTask {
            title: request.title,
            jira_key: request.jira_key,
            redmine_id: request.redmine_id,
            start,
            ..Default::default()
        },
        now,
    )
    .map_err(|e| (code::INVALID, e))?;
    // Baza jest zablokowana przez wywołującego, więc najnowsze zadanie to to nowe.
    let (task_id, title): (i64, String) = db
        .query_row(
            "SELECT id, title FROM tasks ORDER BY id DESC LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(storage)?;
    // Zadanie i pomiar są już zapisane: nieudane wznowienie dnia nie cofa ich
    // i nie zamienia odpowiedzi w błąd (okna i tak muszą się odświeżyć).
    let day_ended = day_ended
        && match resume_today(db, tz, now) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("Zewnętrzne sterowanie: nie udało się wznowić dnia ({e}).");
                false
            }
        };
    let current = active(db).map_err(storage)?;
    let data = json!({
        "taskId": task_id,
        "title": title,
        "started": start,
        "dayResumed": day_ended,
        "status": status_json(tz, current.as_ref(), now),
    });
    let effect = Effect {
        changed: true,
        feedback: start.then(|| format!("Start: {title}")),
        day_resumed: day_ended,
    };
    Ok((data, effect))
}

/// „Wznów dzień” dla dzisiejszego dnia w strefie `tz` (B13).
fn resume_today<Tz: TimeZone>(db: &Connection, tz: &Tz, now: i64) -> Result<(), String> {
    let today = tz
        .timestamp_millis_opt(now)
        .earliest()
        .ok_or("Niepoprawna data.")?
        .date_naive();
    days::resume(db, &today.format("%Y-%m-%d").to_string(), today)
}

// ---------------------------------------------------------------------------
// Aplikacja

fn on_request(app: &tauri::AppHandle, body: &[u8]) -> Value {
    let Ok(request @ Value::Object(_)) = serde_json::from_slice::<Value>(body) else {
        return api_wire::error(code::BAD_REQUEST, "Żądanie musi być obiektem JSON.");
    };
    let enabled = crate::settings::current(app).external_api;
    // Konfiguracja kont czyta bazę sama, więc przed zablokowaniem bazy na czas żądania.
    let (jira_site, redmine_site) = if enabled && request["action"] == "add" {
        let jira = match crate::jira::config(app) {
            Ok(c) => c,
            Err(e) => return api_wire::error(code::STORAGE, storage(e).1),
        };
        let redmine = match crate::redmine::config(app) {
            Ok(c) => c,
            Err(e) => return api_wire::error(code::STORAGE, storage(e).1),
        };
        (
            (!jira.email.is_empty()).then_some(jira.site),
            (!redmine.login.is_empty()).then_some(redmine.site),
        )
    } else {
        (None, None)
    };
    let summary = |key: &str| crate::jira::cached_summary(app, key);
    let subject = |id: u64| crate::redmine::cached_subject(app, id);
    let links = Links {
        jira_site,
        redmine_site,
        summary: &summary,
        subject: &subject,
    };
    let outcome = {
        let state = app.state::<crate::Database>();
        let Ok(mut db) = state.0.lock() else {
            return api_wire::error(code::STORAGE, "Baza danych jest niedostępna.");
        };
        handle(
            enabled,
            &mut db,
            &chrono::Local,
            &links,
            &request,
            db::now(),
        )
    };
    apply(app, outcome.effect);
    outcome.reply
}

/// Powiadamia resztę aplikacji o zmianie (jak `shortcuts::run_toggle`).
fn apply(app: &tauri::AppHandle, effect: Effect) {
    if effect.changed {
        crate::reminder::reset(app);
        let _ = app.emit("data-changed", ());
    }
    if effect.day_resumed {
        crate::suggest::refresh(app, db::now());
    }
    if let Some(text) = effect.feedback {
        crate::tray::show_feedback(app, &text);
        crate::notify::toggle_feedback(app, &text);
        let _ = app.emit("toggle-feedback", text);
    }
}

/// Serwer nazwanego potoku (jak host rozszerzenia i MCP): jedno połączenie to
/// jedna ramka i odpowiedź. Działa zawsze; wyłączone ustawienie daje `DISABLED`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) =
            crate::browser::pipe::serve(&api_wire::pipe_name(), move |body| on_request(&app, body))
        {
            eprintln!("Zewnętrzne sterowanie: potok niedostępny ({e}).");
        }
    });
}

/// Nazwa programu wiersza poleceń obok exe aplikacji.
pub const CTL_EXE: &str = "tuktuk-track-ctl.exe";

/// Stan dla Ustawień: ścieżka `tuktuk-track-ctl.exe` (`None`, gdy go nie ma).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub ctl_path: Option<String>,
}

#[tauri::command]
pub fn external_api_info() -> Info {
    let ctl_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(CTL_EXE)))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned());
    Info { ctl_path }
}

#[cfg(test)]
mod tests;
