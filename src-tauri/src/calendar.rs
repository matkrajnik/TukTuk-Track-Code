//! Kalendarz Microsoft 365 (Graph): logowanie OAuth 2.0 (kod autoryzacyjny z PKCE)
//! przez przeglądarkę systemową i pobieranie spotkań do tabeli `calendar_events`.
//!
//! Aplikacja jest publicznym klientem bez sekretu. Identyfikator aplikacji
//! (client ID) i dzierżawcę podaje użytkownik w Ustawieniach → Kalendarz.
//! Refresh token leży w magazynie poświadczeń Windows (`keyring`, jak token Jiry),
//! access token tylko w pamięci, a w SQLite są wyłącznie spotkania i ustawienia.
//! Nic tutaj nie zmienia pomiaru czasu.
use crate::{db, stats, Database};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Datelike, Duration as Days, Local, NaiveDate, NaiveDateTime, TimeZone};
use keyring::Entry;
use reqwest::{Client, StatusCode, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::{ErrorKind, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Uprawnienia delegowane: odświeżanie tokenu, adres e-mail i odczyt kalendarza.
const SCOPES: &str = "offline_access User.Read Calendars.Read";
const GRAPH: &str = "https://graph.microsoft.com/v1.0";
/// Limit czasu na zalogowanie w przeglądarce.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// Pobieranie spotkań co 10 minut (pętla sprawdza co 30 s).
const SYNC_EVERY: i64 = 10 * 60 * 1000;
const SYNC_TICK: Duration = Duration::from_secs(30);
/// Zabezpieczenie przed zapętlonym `@odata.nextLink`.
const MAX_PAGES: usize = 100;
pub const EVENT: &str = "calendar-changed";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub client_id: String,
    pub tenant: String,
    /// Zalogowane konto (pusty = niepołączone).
    pub email: String,
    /// Ostatnie udane pobranie spotkań (Unix ms).
    pub last_sync: Option<i64>,
    /// Błąd ostatniego pobrania; pusty po udanym pobraniu.
    pub last_error: String,
}
impl Config {
    fn tenant(&self) -> &str {
        if self.tenant.is_empty() {
            "organizations"
        } else {
            &self.tenant
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    client_id: String,
    tenant: String,
    email: String,
    connected: bool,
    /// Trwa logowanie w przeglądarce.
    connecting: bool,
    last_sync: Option<i64>,
    last_error: String,
}

/// Spotkanie z kalendarza; czasy w Unix ms, całodniowe od lokalnej północy.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub start: i64,
    pub end: i64,
    pub subject: String,
    pub show_as: String,
    pub all_day: bool,
    pub cancelled: bool,
    pub response: String,
}

/// Stan w pamięci: access token, trwające logowanie i pobieranie.
#[derive(Default)]
pub struct CalendarState {
    access: Mutex<Option<(String, i64)>>,
    connecting: AtomicBool,
    cancel: AtomicBool,
    syncing: AtomicBool,
}

pub fn setup(app: &tauri::AppHandle) {
    app.manage(CalendarState::default());
}

// ---------- PKCE i odpowiedź logowania ----------

/// Losowy tekst base64url z `bytes` bajtów (32 bajty = 43 znaki).
fn random_text(bytes: usize) -> Result<String, String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|_| "Nie udało się wylosować danych logowania.")?;
    Ok(URL_SAFE_NO_PAD.encode(buf))
}
/// `code_verifier` z RFC 7636: 43 znaki z [A-Za-z0-9-._~].
pub fn pkce_verifier() -> Result<String, String> {
    random_text(32)
}
/// `code_challenge` metodą S256: base64url(SHA-256(verifier)) bez dopełnienia.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[derive(Debug, PartialEq)]
pub enum Callback {
    /// Inne żądanie (np. favicon) — czekamy dalej.
    Ignore,
    Code(String),
    Error(String),
}

/// Rozpoznaje pierwszą linię żądania HTTP z przekierowania logowania.
pub fn parse_callback(request_line: &str, expected_state: &str) -> Callback {
    let mut parts = request_line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return Callback::Ignore;
    };
    let Ok(url) = Url::parse(&format!("http://localhost{target}")) else {
        return Callback::Ignore;
    };
    if url.path() != "/" {
        return Callback::Ignore;
    }
    let get = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    let (code, error, state) = (get("code"), get("error"), get("state"));
    if code.is_none() && error.is_none() {
        return Callback::Ignore;
    }
    if state.as_deref() != Some(expected_state) {
        return Callback::Error(
            "Odpowiedź logowania nie pasuje do tego logowania (state). Spróbuj ponownie.".into(),
        );
    }
    if let Some(error) = error {
        let description = get("error_description").unwrap_or_default();
        let first = description.lines().next().unwrap_or("").trim();
        return Callback::Error(if error == "access_denied" {
            "Logowanie anulowane albo odmówiono zgody na dostęp do kalendarza.".into()
        } else if first.is_empty() {
            format!("Microsoft odrzucił logowanie ({error}).")
        } else {
            format!("Microsoft odrzucił logowanie: {first}")
        });
    }
    match code {
        Some(code) if !code.is_empty() && code.len() <= 8192 => Callback::Code(code),
        _ => Callback::Error("Microsoft nie zwrócił kodu logowania.".into()),
    }
}

fn respond(stream: &mut TcpStream, status: &str, message: &str) {
    let body = format!(
        "<!doctype html><html lang=\"pl\"><meta charset=\"utf-8\"><title>Todo Track</title>\
         <body style=\"font-family:Segoe UI,sans-serif;padding:40px\"><h1>Todo Track</h1><p>{message}</p></body></html>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}

/// Czyta pierwszą linię żądania (najwyżej 16 KB, 5 s).
fn request_line(stream: &mut TcpStream) -> Option<String> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut data = Vec::new();
    let mut buf = [0u8; 2048];
    while !data.windows(2).any(|w| w == b"\r\n") && data.len() < 16 * 1024 {
        let n = stream.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&data);
    text.split("\r\n").next().map(str::to_owned)
}

/// Czeka na przekierowanie z kodem na jednym z gniazd (127.0.0.1 i ewentualnie ::1).
/// Zły `state` albo błąd z Microsoftu kończy logowanie; inne żądania są pomijane.
pub fn wait_for_code(
    listeners: &[TcpListener],
    state: &str,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Result<String, String> {
    for l in listeners {
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Logowanie przerwane.".into());
        }
        if Instant::now() >= deadline {
            return Err("Minął limit 5 minut na zalogowanie. Spróbuj ponownie.".into());
        }
        let mut idle = true;
        for listener in listeners {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    idle = false;
                    let line = request_line(&mut stream).unwrap_or_default();
                    match parse_callback(&line, state) {
                        Callback::Ignore => {
                            respond(&mut stream, "404 Not Found", "Nie znaleziono.")
                        }
                        Callback::Code(code) => {
                            respond(
                                &mut stream,
                                "200 OK",
                                "Zalogowano. Możesz zamknąć tę kartę i wrócić do Todo Track.",
                            );
                            return Ok(code);
                        }
                        Callback::Error(message) => {
                            respond(&mut stream, "400 Bad Request", &message);
                            return Err(message);
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(_) => {}
            }
        }
        if idle {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// Gniazdo na 127.0.0.1 z losowym portem i, jeśli się da, ::1 na tym samym porcie,
/// bo przeglądarka może rozwiązać `localhost` na IPv6.
fn bind() -> Result<(Vec<TcpListener>, u16), String> {
    let v4 = TcpListener::bind("127.0.0.1:0")
        .map_err(|_| "Nie udało się otworzyć lokalnego portu dla logowania.".to_owned())?;
    let port = v4.local_addr().map_err(|e| e.to_string())?.port();
    let mut listeners = vec![v4];
    if let Ok(v6) = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], port))) {
        listeners.push(v6);
    }
    Ok((listeners, port))
}

// ---------- Ustawienia i token ----------

fn load(db: &Connection) -> Config {
    db.query_row("SELECT value FROM settings WHERE key='calendar'", [], |r| {
        r.get::<_, String>(0)
    })
    .optional()
    .ok()
    .flatten()
    .and_then(|raw| serde_json::from_str(&raw).ok())
    .unwrap_or_default()
}
fn store(db: &Connection, config: &Config) -> Result<(), String> {
    let raw = serde_json::to_string(config).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO settings(key,value) VALUES('calendar',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [raw],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
fn config(app: &tauri::AppHandle) -> Result<Config, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    Ok(load(&db))
}
/// Zmienia zapisaną konfigurację (czyta świeżą wersję pod blokadą bazy).
fn update(app: &tauri::AppHandle, change: impl FnOnce(&mut Config)) -> Result<Config, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let mut c = load(&db);
    change(&mut c);
    store(&db, &c)?;
    Ok(c)
}

/// GUID w postaci 8-4-4-4-12 znaków szesnastkowych.
fn guid(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}
/// Dzierżawca: `organizations`, `common`, `consumers`, GUID albo domena.
pub fn valid_tenant(value: &str) -> bool {
    matches!(value, "organizations" | "common" | "consumers")
        || guid(value)
        || (value.len() <= 253
            && value.contains('.')
            && value.split('.').all(|l| {
                !l.is_empty() && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            }))
}
pub fn valid_client_id(value: &str) -> bool {
    guid(value)
}

fn entry(app: &tauri::AppHandle, c: &Config) -> Result<Entry, String> {
    Entry::new(
        &format!("{}.calendar", app.config().identifier),
        &format!("{}|{}", c.tenant(), c.client_id.to_ascii_lowercase()),
    )
    .map_err(|_| "Nie można otworzyć magazynu poświadczeń Windows.".into())
}
fn has_token(app: &tauri::AppHandle, c: &Config) -> bool {
    !c.email.is_empty()
        && !c.client_id.is_empty()
        && entry(app, c).is_ok_and(|e| e.get_password().is_ok())
}

fn status(app: &tauri::AppHandle) -> Result<Status, String> {
    let c = config(app)?;
    let connected = has_token(app, &c);
    Ok(Status {
        connected,
        connecting: app
            .state::<CalendarState>()
            .connecting
            .load(Ordering::SeqCst),
        tenant: c.tenant().to_owned(),
        client_id: c.client_id,
        email: if connected { c.email } else { String::new() },
        last_sync: c.last_sync,
        last_error: c.last_error,
    })
}

fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Nie udało się utworzyć połączenia HTTPS.".into())
}

struct Tokens {
    access: String,
    refresh: Option<String>,
    expires_in: i64,
}

/// Błąd z punktu tokenów Microsoft: `error` i pierwsza linia opisu (bez identyfikatorów śledzenia).
fn token_error(value: &Value) -> (String, String) {
    let code = value["error"].as_str().unwrap_or("").to_owned();
    let description = value["error_description"]
        .as_str()
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_owned();
    (code, description)
}

async fn token_request(c: &Config, form: &[(&str, &str)]) -> Result<Tokens, (String, String)> {
    let url = format!(
        "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
        c.tenant()
    );
    let mut body = vec![("client_id", c.client_id.as_str()), ("scope", SCOPES)];
    body.extend_from_slice(form);
    let response = client()
        .map_err(|e| (String::new(), e))?
        .post(url)
        .form(&body)
        .send()
        .await
        .map_err(|_| {
            (
                String::new(),
                "Nie można połączyć się z Microsoft. Sprawdź Internet.".to_owned(),
            )
        })?;
    let ok = response.status().is_success();
    let value: Value = response.json().await.unwrap_or(Value::Null);
    if !ok {
        let (code, description) = token_error(&value);
        let message = if description.is_empty() {
            format!("Microsoft odrzucił żądanie tokenu ({code}).")
        } else {
            format!("Microsoft odrzucił żądanie tokenu: {description}")
        };
        return Err((code, message));
    }
    let access = value["access_token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or((String::new(), "Microsoft nie zwrócił tokenu.".to_owned()))?;
    Ok(Tokens {
        access: access.to_owned(),
        refresh: value["refresh_token"].as_str().map(str::to_owned),
        expires_in: value["expires_in"].as_i64().unwrap_or(3600),
    })
}

fn remember(app: &tauri::AppHandle, tokens: &Tokens) {
    // Minuta zapasu przed wygaśnięciem.
    let until = db::now() + (tokens.expires_in - 60).max(0) * 1000;
    if let Ok(mut access) = app.state::<CalendarState>().access.lock() {
        *access = Some((tokens.access.clone(), until));
    }
}
fn forget(app: &tauri::AppHandle) {
    if let Ok(mut access) = app.state::<CalendarState>().access.lock() {
        *access = None;
    }
}

/// Ważny access token: z pamięci albo odświeżony z refresh tokenu.
async fn access_token(app: &tauri::AppHandle, c: &Config) -> Result<String, String> {
    if let Some((token, until)) = app
        .state::<CalendarState>()
        .access
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
    {
        if until > db::now() {
            return Ok(token);
        }
    }
    let entry = entry(app, c)?;
    let refresh = entry
        .get_password()
        .map_err(|_| "Połącz konto Microsoft w Ustawieniach → Kalendarz.".to_owned())?;
    match token_request(
        c,
        &[("grant_type", "refresh_token"), ("refresh_token", &refresh)],
    )
    .await
    {
        Ok(tokens) => {
            if let Some(next) = tokens.refresh.as_deref().filter(|r| *r != refresh) {
                let _ = entry.set_password(next);
            }
            remember(app, &tokens);
            Ok(tokens.access)
        }
        Err((code, _)) if code == "invalid_grant" => {
            // Sesja wygasła albo ją odwołano: konto trzeba połączyć ponownie.
            let _ = entry.delete_credential();
            Err("Sesja Microsoft wygasła. Połącz konto ponownie.".into())
        }
        Err((_, message)) => Err(message),
    }
}

// ---------- Graph ----------

/// Zakres pobrania: od poniedziałku poprzedniego tygodnia do 21 dni po dziś (włącznie).
pub fn sync_range(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let monday = today - Days::days(today.weekday().num_days_from_monday() as i64 + 7);
    (monday, today + Days::days(22))
}

fn graph_time(value: &Value) -> Result<NaiveDateTime, String> {
    let zone = value["timeZone"].as_str().unwrap_or("UTC");
    if !matches!(zone, "UTC" | "Etc/UTC" | "tzone://Microsoft/Utc" | "") {
        return Err(format!(
            "Kalendarz zwrócił czas w strefie {zone} zamiast UTC."
        ));
    }
    let raw = value["dateTime"].as_str().unwrap_or("");
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S%.f")
        .map_err(|_| "Kalendarz zwrócił niepoprawny czas spotkania.".to_owned())
}

/// Jedna strona `calendarView` (z `Prefer: outlook.timezone="UTC"`): spotkania
/// i adres następnej strony. Całodniowe zaczynają się o lokalnej północy dnia z Graph.
pub fn parse_page<Tz: TimeZone>(
    value: &Value,
    tz: &Tz,
) -> Result<(Vec<CalendarEvent>, Option<String>), String> {
    let items = value["value"]
        .as_array()
        .ok_or("Kalendarz zwrócił niepoprawną odpowiedź.")?;
    let mut events = Vec::with_capacity(items.len());
    for item in items {
        let Some(id) = item["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 1024)
        else {
            continue;
        };
        let all_day = item["isAllDay"].as_bool().unwrap_or(false);
        let (start, end) = (graph_time(&item["start"])?, graph_time(&item["end"])?);
        let (start, end) = if all_day {
            (
                stats::day_bounds(tz, start.date()).0,
                stats::day_bounds(tz, end.date()).0,
            )
        } else {
            (
                start.and_utc().timestamp_millis(),
                end.and_utc().timestamp_millis(),
            )
        };
        events.push(CalendarEvent {
            id: id.to_owned(),
            start,
            end: end.max(start),
            subject: item["subject"]
                .as_str()
                .unwrap_or("")
                .trim()
                .chars()
                .take(500)
                .collect(),
            show_as: item["showAs"].as_str().unwrap_or("busy").to_owned(),
            all_day,
            cancelled: item["isCancelled"].as_bool().unwrap_or(false),
            response: item["responseStatus"]["response"]
                .as_str()
                .unwrap_or("none")
                .to_owned(),
        });
    }
    let next = value["@odata.nextLink"].as_str().map(str::to_owned);
    Ok((events, next))
}

/// Czy spotkanie zajmuje czas: `busy`, `oof`, `workingElsewhere` albo `tentative`,
/// nieanulowane i nieodrzucone.
pub fn occupies(event: &CalendarEvent) -> bool {
    matches!(
        event.show_as.as_str(),
        "busy" | "oof" | "workingElsewhere" | "tentative"
    ) && !event.cancelled
        && event.response != "declined"
}

/// Zastępuje spotkania z zakresu `[from, to)` w jednej transakcji, więc usunięte
/// w kalendarzu znikają, a spotkania spoza zakresu zostają.
pub fn replace_range(
    db: &mut Connection,
    from: i64,
    to: i64,
    events: &[CalendarEvent],
    now: i64,
) -> Result<(), String> {
    let tx = db.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM calendar_events WHERE start < ?2 AND \"end\" > ?1",
        params![from, to],
    )
    .map_err(|e| e.to_string())?;
    {
        let mut insert = tx
            .prepare(
                "INSERT OR REPLACE INTO calendar_events(id,start,\"end\",subject,show_as,all_day,cancelled,response,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            )
            .map_err(|e| e.to_string())?;
        for e in events {
            insert
                .execute(params![
                    e.id,
                    e.start,
                    e.end,
                    e.subject,
                    e.show_as,
                    e.all_day,
                    e.cancelled,
                    e.response,
                    now
                ])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Spotkania nakładające się na `[from, to)`, według początku.
pub fn events_between(db: &Connection, from: i64, to: i64) -> Result<Vec<CalendarEvent>, String> {
    let mut stmt = db
        .prepare(
            "SELECT id,start,\"end\",subject,show_as,all_day,cancelled,response FROM calendar_events
             WHERE start < ?2 AND (\"end\" > ?1 OR (\"end\" = start AND start >= ?1)) ORDER BY start, \"end\", id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to], |r| {
            Ok(CalendarEvent {
                id: r.get(0)?,
                start: r.get(1)?,
                end: r.get(2)?,
                subject: r.get(3)?,
                show_as: r.get(4)?,
                all_day: r.get(5)?,
                cancelled: r.get(6)?,
                response: r.get(7)?,
            })
        })
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| e.to_string())
}

fn graph_error(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "Microsoft odrzucił token. Połącz konto ponownie.".into(),
        StatusCode::FORBIDDEN => {
            "Brak zgody na odczyt kalendarza (Calendars.Read). Poproś administratora o zgodę."
                .into()
        }
        StatusCode::TOO_MANY_REQUESTS => {
            "Microsoft ograniczył liczbę zapytań. Spróbuj ponownie za chwilę.".into()
        }
        _ => format!("Microsoft Graph zwrócił błąd HTTP {}.", status.as_u16()),
    }
}

async fn graph_get(client: &Client, token: &str, url: &str) -> Result<Value, (StatusCode, String)> {
    let response = client
        .get(url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header("Prefer", "outlook.timezone=\"UTC\"")
        .send()
        .await
        .map_err(|_| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "Nie można połączyć się z Microsoft Graph. Sprawdź Internet.".to_owned(),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err((status, graph_error(status)));
    }
    response.json().await.map_err(|_| {
        (
            status,
            "Microsoft Graph zwrócił niepoprawną odpowiedź.".to_owned(),
        )
    })
}

/// Wszystkie strony `calendarView` dla zakresu.
async fn fetch(
    client: &Client,
    token: &str,
    from: i64,
    to: i64,
) -> Result<Vec<CalendarEvent>, (StatusCode, String)> {
    let iso = |ms: i64| {
        chrono::DateTime::from_timestamp_millis(ms)
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    };
    let mut url = Url::parse_with_params(
        &format!("{GRAPH}/me/calendarView"),
        &[
            ("startDateTime", iso(from)),
            ("endDateTime", iso(to)),
            (
                "$select",
                "id,subject,start,end,showAs,isAllDay,isCancelled,responseStatus".into(),
            ),
            ("$orderby", "start/dateTime".into()),
            ("$top", "100".into()),
        ],
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    .to_string();
    let mut events = Vec::new();
    for _ in 0..MAX_PAGES {
        let value = graph_get(client, token, &url).await?;
        let (page, next) = parse_page(&value, &Local).map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
        events.extend(page);
        match next {
            // Tylko adresy Graph: token nie wychodzi gdzie indziej.
            Some(next) if next.starts_with(GRAPH) => url = next,
            Some(_) => {
                return Err((
                    StatusCode::BAD_GATEWAY,
                    "Microsoft Graph zwrócił nieoczekiwany adres strony.".into(),
                ))
            }
            None => return Ok(events),
        }
    }
    Err((
        StatusCode::BAD_GATEWAY,
        "Za dużo stron spotkań w kalendarzu.".into(),
    ))
}

/// Pobiera spotkania i zapisuje wynik (czas albo błąd) w konfiguracji.
/// `None`, gdy kalendarz nie jest połączony albo pobieranie już trwa.
async fn sync(app: &tauri::AppHandle) -> Option<Result<(), String>> {
    let c = config(app).ok()?;
    if !has_token(app, &c) {
        return None;
    }
    let state = app.state::<CalendarState>();
    if state.syncing.swap(true, Ordering::SeqCst) {
        return None;
    }
    let result = sync_inner(app, &c).await;
    state.syncing.store(false, Ordering::SeqCst);
    let now = db::now();
    let _ = update(app, |c| match &result {
        Ok(()) => {
            c.last_sync = Some(now);
            c.last_error.clear();
        }
        Err(e) => c.last_error = e.clone(),
    });
    let _ = app.emit(EVENT, ());
    Some(result)
}
async fn sync_inner(app: &tauri::AppHandle, c: &Config) -> Result<(), String> {
    let (first, last) = sync_range(Local::now().date_naive());
    let (from, to) = (
        stats::day_bounds(&Local, first).0,
        stats::day_bounds(&Local, last).0,
    );
    let client = client()?;
    let mut token = access_token(app, c).await?;
    let events = match fetch(&client, &token, from, to).await {
        Err((StatusCode::UNAUTHORIZED, _)) => {
            // Token mógł zostać unieważniony: jedna próba z odświeżonym.
            forget(app);
            token = access_token(app, c).await?;
            fetch(&client, &token, from, to).await.map_err(|(_, e)| e)?
        }
        other => other.map_err(|(_, e)| e)?,
    };
    let state = app.state::<Database>();
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    // Konto mogło zostać rozłączone w trakcie pobierania.
    if load(&db).email.is_empty() {
        return Err("Konto Microsoft zostało rozłączone.".into());
    }
    replace_range(&mut db, from, to, &events, db::now())
}

// ---------- Komendy ----------

#[tauri::command]
pub fn calendar_status(app: tauri::AppHandle) -> Result<Status, String> {
    status(&app)
}

/// Logowanie w przeglądarce systemowej: zapisuje client ID i dzierżawcę, czeka
/// najwyżej 5 min na przekierowanie, wymienia kod na tokeny i pobiera spotkania.
#[tauri::command]
pub async fn calendar_connect(
    app: tauri::AppHandle,
    client_id: String,
    tenant: String,
) -> Result<Status, String> {
    let client_id = client_id.trim().to_ascii_lowercase();
    let tenant = match tenant.trim() {
        "" => "organizations".to_owned(),
        t => t.to_ascii_lowercase(),
    };
    if !valid_client_id(&client_id) {
        return Err(
            "Podaj identyfikator aplikacji (client ID) z rejestracji w Entra ID, np. 00000000-0000-0000-0000-000000000000."
                .into(),
        );
    }
    if !valid_tenant(&tenant) {
        return Err("Podaj dzierżawcę: organizations, identyfikator albo domenę firmy.".into());
    }
    let state = app.state::<CalendarState>();
    if state.connecting.swap(true, Ordering::SeqCst) {
        return Err("Logowanie już trwa w przeglądarce.".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let result = connect(&app, client_id, tenant).await;
    state.connecting.store(false, Ordering::SeqCst);
    let _ = app.emit(EVENT, ());
    result?;
    // Pierwsze pobranie od razu; jego błąd widać w statusie.
    let _ = sync(&app).await;
    status(&app)
}

async fn connect(app: &tauri::AppHandle, client_id: String, tenant: String) -> Result<(), String> {
    let old = config(app)?;
    let mut c = Config {
        client_id,
        tenant,
        ..Config::default()
    };
    // Inne konto albo inna aplikacja: stary token i spotkania przestają obowiązywać.
    let changed = old.client_id != c.client_id || old.tenant() != c.tenant();
    if changed && !old.client_id.is_empty() {
        if let Ok(e) = entry(app, &old) {
            let _ = e.delete_credential();
        }
    }
    update(app, |saved| {
        saved.client_id = c.client_id.clone();
        saved.tenant = c.tenant.clone();
    })?;
    let verifier = pkce_verifier()?;
    let state = random_text(16)?;
    let (listeners, port) = bind()?;
    let redirect = format!("http://localhost:{port}");
    let url = Url::parse_with_params(
        &format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            c.tenant()
        ),
        &[
            ("client_id", c.client_id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect.as_str()),
            ("response_mode", "query"),
            ("scope", SCOPES),
            ("state", state.as_str()),
            ("code_challenge", pkce_challenge(&verifier).as_str()),
            ("code_challenge_method", "S256"),
            ("prompt", "select_account"),
        ],
    )
    .map_err(|e| e.to_string())?;
    app.opener()
        .open_url(url.as_str(), None::<String>)
        .map_err(|_| "Nie można otworzyć przeglądarki.".to_owned())?;
    let handle = app.clone();
    let code = tauri::async_runtime::spawn_blocking(move || {
        let state_ref = handle.state::<CalendarState>();
        wait_for_code(&listeners, &state, LOGIN_TIMEOUT, &state_ref.cancel)
    })
    .await
    .map_err(|e| e.to_string())??;
    let tokens = token_request(
        &c,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
        ],
    )
    .await
    .map_err(|(_, e)| e)?;
    let refresh = tokens
        .refresh
        .clone()
        .ok_or("Microsoft nie zwrócił refresh tokenu (brak uprawnienia offline_access).")?;
    let me = graph_get(&client()?, &tokens.access, &format!("{GRAPH}/me"))
        .await
        .map_err(|(_, e)| e)?;
    c.email = me["mail"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or(me["userPrincipalName"].as_str())
        .unwrap_or("konto Microsoft")
        .chars()
        .take(254)
        .collect();
    if app.state::<CalendarState>().cancel.load(Ordering::SeqCst) {
        return Err("Logowanie przerwane.".into());
    }
    entry(app, &c)?
        .set_password(&refresh)
        .map_err(|_| "Nie udało się zapisać tokenu w magazynie poświadczeń Windows.".to_owned())?;
    remember(app, &tokens);
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    if changed {
        db.execute("DELETE FROM calendar_events", [])
            .map_err(|e| e.to_string())?;
    }
    let mut saved = load(&db);
    saved.email = c.email;
    saved.last_error.clear();
    if changed {
        saved.last_sync = None;
    }
    store(&db, &saved)
}

/// Rozłącza konto (albo przerywa trwające logowanie): usuwa token z magazynu
/// i spotkania z bazy. Client ID i dzierżawca zostają.
#[tauri::command]
pub fn calendar_disconnect(app: tauri::AppHandle) -> Result<Status, String> {
    let state = app.state::<CalendarState>();
    state.cancel.store(true, Ordering::SeqCst);
    forget(&app);
    let c = config(&app)?;
    if !c.client_id.is_empty() {
        match entry(&app, &c)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {
                return Err("Nie udało się usunąć tokenu z magazynu poświadczeń Windows.".into())
            }
        }
    }
    {
        let db_state = app.state::<Database>();
        let db = db_state.0.lock().map_err(|e| e.to_string())?;
        db.execute("DELETE FROM calendar_events", [])
            .map_err(|e| e.to_string())?;
        let mut saved = load(&db);
        saved.email.clear();
        saved.last_sync = None;
        saved.last_error.clear();
        store(&db, &saved)?;
    }
    let _ = app.emit(EVENT, ());
    status(&app)
}

/// „Odśwież”: pobiera spotkania teraz.
#[tauri::command]
pub async fn calendar_refresh(app: tauri::AppHandle) -> Result<Status, String> {
    let c = config(&app)?;
    if c.client_id.is_empty() {
        return Err("Kalendarz jest wyłączony: podaj client ID w Ustawieniach → Kalendarz.".into());
    }
    if !has_token(&app, &c) {
        return Err("Połącz konto Microsoft w Ustawieniach → Kalendarz.".into());
    }
    if let Some(Err(e)) = sync(&app).await {
        return Err(e);
    }
    status(&app)
}

/// Spotkania nakładające się na `[from, to)` (Unix ms).
#[tauri::command]
pub fn calendar_events(
    app: tauri::AppHandle,
    from: i64,
    to: i64,
) -> Result<Vec<CalendarEvent>, String> {
    if to < from {
        return Err("Niepoprawny zakres dat.".into());
    }
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    events_between(&db, from, to)
}

/// Pobieranie przy starcie i co 10 minut, gdy konto jest połączone.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut first = true;
        let mut attempted = 0i64;
        loop {
            if app.get_webview_window("main").is_none() {
                break;
            }
            let now = db::now();
            let stale = match config(&app).ok().and_then(|c| c.last_sync) {
                Some(at) => now - at >= SYNC_EVERY || at > now,
                None => true,
            };
            // Przy starcie od razu, potem gdy ostatnie pobranie (także z „Odśwież”)
            // jest starsze niż 10 min; po błędzie następna próba też za 10 min.
            if first || (stale && now - attempted >= SYNC_EVERY) {
                if tauri::async_runtime::block_on(sync(&app)).is_some() {
                    attempted = now;
                }
                first = false;
            }
            std::thread::sleep(SYNC_TICK);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, Utc};
    use serde_json::json;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db
    }

    #[test]
    fn pkce_matches_rfc_7636_and_verifier_is_valid() {
        // RFC 7636, dodatek B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let a = pkce_verifier().unwrap();
        let b = pkce_verifier().unwrap();
        assert_ne!(a, b);
        for v in [&a, &b] {
            assert!((43..=128).contains(&v.len()), "{v}");
            assert!(v
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-._~".contains(&c)));
        }
        let challenge = pkce_challenge(&a);
        assert_eq!(challenge.len(), 43);
        assert!(!challenge.contains('=') && !challenge.contains('+') && !challenge.contains('/'));
    }

    #[test]
    fn callback_checks_state_and_reads_code_or_error() {
        assert_eq!(
            parse_callback("GET /?code=abc%2Fd&state=s1 HTTP/1.1", "s1"),
            Callback::Code("abc/d".into())
        );
        assert!(matches!(
            parse_callback("GET /?code=abc&state=zly HTTP/1.1", "s1"),
            Callback::Error(e) if e.contains("state")
        ));
        assert!(matches!(
            parse_callback("GET /?code=abc HTTP/1.1", "s1"),
            Callback::Error(e) if e.contains("state")
        ));
        // Błąd z innym state też jest odrzucany jako obcy.
        assert!(matches!(
            parse_callback("GET /?error=access_denied&state=x HTTP/1.1", "s1"),
            Callback::Error(e) if e.contains("state")
        ));
        assert!(matches!(
            parse_callback("GET /?error=access_denied&state=s1 HTTP/1.1", "s1"),
            Callback::Error(e) if e.contains("anulowane")
        ));
        assert!(matches!(
            parse_callback(
                "GET /?error=invalid_request&error_description=AADSTS50011%3A+Redirect+mismatch%0D%0ATrace+ID%3A+1&state=s1 HTTP/1.1",
                "s1"
            ),
            Callback::Error(e) if e == "Microsoft odrzucił logowanie: AADSTS50011: Redirect mismatch"
        ));
        assert_eq!(
            parse_callback("GET /favicon.ico HTTP/1.1", "s1"),
            Callback::Ignore
        );
        assert_eq!(parse_callback("GET / HTTP/1.1", "s1"), Callback::Ignore);
        assert_eq!(
            parse_callback("POST /?code=a&state=s1 HTTP/1.1", "s1"),
            Callback::Ignore
        );
        assert_eq!(parse_callback("", "s1"), Callback::Ignore);
    }

    fn send(port: u16, path: &str) -> String {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
        let mut out = String::new();
        let _ = s.read_to_string(&mut out);
        out
    }

    #[test]
    fn local_server_rejects_wrong_state_and_accepts_code() {
        let cancel = AtomicBool::new(false);
        // Zły state kończy logowanie błędem, favicon jest pomijany.
        let (listeners, port) = bind().unwrap();
        let client = std::thread::spawn(move || {
            let favicon = send(port, "/favicon.ico");
            let wrong = send(port, "/?code=abc&state=obcy");
            (favicon, wrong)
        });
        let result = wait_for_code(&listeners, "moj", Duration::from_secs(10), &cancel);
        let (favicon, wrong) = client.join().unwrap();
        assert!(favicon.starts_with("HTTP/1.1 404"));
        assert!(wrong.starts_with("HTTP/1.1 400"));
        assert!(result.unwrap_err().contains("state"));
        // Poprawny state zwraca kod.
        let (listeners, port) = bind().unwrap();
        let client = std::thread::spawn(move || send(port, "/?code=kod-123&state=moj"));
        let code = wait_for_code(&listeners, "moj", Duration::from_secs(10), &cancel).unwrap();
        assert_eq!(code, "kod-123");
        assert!(client.join().unwrap().contains("Zalogowano"));
        // Limit czasu i przerwanie.
        let (listeners, _) = bind().unwrap();
        let err =
            wait_for_code(&listeners, "moj", Duration::from_millis(100), &cancel).unwrap_err();
        assert!(err.contains("limit"));
        cancel.store(true, Ordering::SeqCst);
        let err = wait_for_code(&listeners, "moj", Duration::from_secs(10), &cancel).unwrap_err();
        assert!(err.contains("przerwane"));
    }

    #[test]
    fn validates_client_id_and_tenant() {
        assert!(valid_client_id("0a1b2c3d-4e5f-6789-abcd-ef0123456789"));
        assert!(!valid_client_id("0a1b2c3d-4e5f-6789-abcd-ef012345678"));
        assert!(!valid_client_id(""));
        assert!(!valid_client_id("x/../y"));
        for ok in [
            "organizations",
            "common",
            "contoso.onmicrosoft.com",
            "asiston.pl",
            "0a1b2c3d-4e5f-6789-abcd-ef0123456789",
        ] {
            assert!(valid_tenant(ok), "{ok}");
        }
        for bad in ["", "organisation", "a/b.pl", "a..pl", "x?y.pl"] {
            assert!(!valid_tenant(bad), "{bad}");
        }
    }

    #[test]
    fn sync_range_starts_on_previous_monday() {
        let d = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        // Czwartek 15.10.2026 -> od poniedziałku 5.10 do 6.11 (bez), czyli 21 dni po dziś.
        assert_eq!(
            sync_range(d("2026-10-15")),
            (d("2026-10-05"), d("2026-11-06"))
        );
        // Poniedziałek -> poniedziałek tydzień wcześniej.
        assert_eq!(sync_range(d("2026-10-12")).0, d("2026-10-05"));
        // Niedziela należy do tygodnia od 12.10.
        assert_eq!(sync_range(d("2026-10-18")).0, d("2026-10-05"));
    }

    fn item(id: &str, start: &str, end: &str) -> Value {
        json!({
            "id": id,
            "subject": format!("Spotkanie {id}"),
            "start": {"dateTime": start, "timeZone": "UTC"},
            "end": {"dateTime": end, "timeZone": "UTC"},
            "showAs": "busy",
            "isAllDay": false,
            "isCancelled": false,
            "responseStatus": {"response": "accepted", "time": "0001-01-01T00:00:00Z"}
        })
    }

    #[test]
    fn parses_graph_pages_all_day_cancelled_and_declined() {
        let warsaw = FixedOffset::east_opt(2 * 3600).unwrap();
        let mut all_day = item(
            "c",
            "2026-10-16T00:00:00.0000000",
            "2026-10-17T00:00:00.0000000",
        );
        all_day["isAllDay"] = json!(true);
        all_day["showAs"] = json!("oof");
        let mut cancelled = item(
            "d",
            "2026-10-15T12:00:00.0000000",
            "2026-10-15T13:00:00.0000000",
        );
        cancelled["isCancelled"] = json!(true);
        let mut declined = item(
            "e",
            "2026-10-15T14:00:00.0000000",
            "2026-10-15T15:00:00.0000000",
        );
        declined["responseStatus"]["response"] = json!("declined");
        let mut tentative = item("f", "2026-10-15T15:00:00", "2026-10-15T15:30:00");
        tentative["showAs"] = json!("tentative");
        let mut free = item("g", "2026-10-15T16:00:00", "2026-10-15T16:30:00");
        free["showAs"] = json!("free");
        let page1 = json!({
            "value": [item("a", "2026-10-15T06:30:00.0000000", "2026-10-15T07:00:00.0000000"), all_day],
            "@odata.nextLink": "https://graph.microsoft.com/v1.0/me/calendarView?$skiptoken=X"
        });
        let page2 = json!({"value": [cancelled, declined, tentative, free, {"id": ""}]});
        let (first, next) = parse_page(&page1, &warsaw).unwrap();
        assert_eq!(
            next.as_deref(),
            Some("https://graph.microsoft.com/v1.0/me/calendarView?$skiptoken=X")
        );
        let (second, last) = parse_page(&page2, &warsaw).unwrap();
        assert_eq!(last, None);
        let all: Vec<CalendarEvent> = first.into_iter().chain(second).collect();
        assert_eq!(all.len(), 6, "puste id jest pomijane");
        // Czas w UTC: 06:30Z = 08:30 w Warszawie.
        let a = &all[0];
        assert_eq!(
            a.start,
            Utc.with_ymd_and_hms(2026, 10, 15, 6, 30, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(a.end - a.start, 30 * 60 * 1000);
        assert_eq!(a.subject, "Spotkanie a");
        assert!(occupies(a));
        // Całodniowe: od lokalnej północy 16.10 do lokalnej północy 17.10.
        let c = &all[1];
        assert!(c.all_day && c.show_as == "oof");
        assert_eq!(
            c.start,
            warsaw
                .with_ymd_and_hms(2026, 10, 16, 0, 0, 0)
                .unwrap()
                .timestamp_millis()
        );
        assert_eq!(c.end - c.start, 24 * 3600 * 1000);
        assert!(occupies(c));
        assert!(all[2].cancelled && !occupies(&all[2]));
        assert!(all[3].response == "declined" && !occupies(&all[3]));
        assert!(occupies(&all[4]), "tentative zajmuje czas");
        assert!(!occupies(&all[5]), "free nie zajmuje czasu");
        // Inna strefa niż UTC jest błędem, nie cichym przesunięciem.
        let mut other = item("x", "2026-10-15T08:00:00", "2026-10-15T09:00:00");
        other["start"]["timeZone"] = json!("Central European Standard Time");
        assert!(parse_page(&json!({"value": [other]}), &warsaw).is_err());
        assert!(parse_page(&json!({"error": {}}), &warsaw).is_err());
    }

    fn ev(id: &str, start: i64, end: i64) -> CalendarEvent {
        CalendarEvent {
            id: id.into(),
            start,
            end,
            subject: id.into(),
            show_as: "busy".into(),
            all_day: false,
            cancelled: false,
            response: "accepted".into(),
        }
    }

    #[test]
    fn replace_range_drops_deleted_events_and_keeps_outside() {
        let mut db = db();
        replace_range(
            &mut db,
            0,
            10_000,
            &[
                ev("stare", -5000, -4000),
                ev("usuniete", 1000, 2000),
                ev("zostaje", 3000, 4000),
            ],
            1,
        )
        .unwrap();
        // Drugie pobranie zakresu [0, 10000): „usuniete” zniknęło z kalendarza.
        let mut changed = ev("zostaje", 3000, 4500);
        changed.subject = "Nowy tytuł".into();
        replace_range(&mut db, 0, 10_000, &[changed, ev("nowe", 5000, 6000)], 2).unwrap();
        let all = events_between(&db, -10_000, 20_000).unwrap();
        let ids: Vec<&str> = all.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["stare", "zostaje", "nowe"]);
        assert_eq!(all[1].subject, "Nowy tytuł");
        assert_eq!(all[1].end, 4500);
        // Zakres zapytania: tylko nakładające się.
        let some = events_between(&db, 4500, 5500).unwrap();
        assert_eq!(some.len(), 1);
        assert_eq!(some[0].id, "nowe");
        // Błąd w trakcie zapisu cofa całą transakcję: nic nie znika.
        db.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON calendar_events WHEN NEW.id='zly' BEGIN SELECT RAISE(ABORT,'x'); END;")
            .unwrap();
        assert!(replace_range(&mut db, 0, 10_000, &[ev("zly", 1, 2)], 3).is_err());
        assert_eq!(events_between(&db, 0, 10_000).unwrap().len(), 2);
    }

    #[test]
    fn config_is_stored_in_settings_table() {
        let db = db();
        assert_eq!(load(&db), Config::default());
        assert_eq!(load(&db).tenant(), "organizations");
        let c = Config {
            client_id: "0a1b2c3d-4e5f-6789-abcd-ef0123456789".into(),
            tenant: "contoso.pl".into(),
            email: "a@contoso.pl".into(),
            last_sync: Some(5),
            last_error: String::new(),
        };
        store(&db, &c).unwrap();
        assert_eq!(load(&db), c);
        db.execute(
            "UPDATE settings SET value='zepsute' WHERE key='calendar'",
            [],
        )
        .unwrap();
        assert_eq!(load(&db), Config::default());
    }
}
