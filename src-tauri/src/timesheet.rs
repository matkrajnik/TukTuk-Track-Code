//! Ideo Timesheet (PLAN_1.8.md, T1): logowanie, sesja, formularz dnia, parser i zapis.
//!
//! Port konektora `ideo-ts-api/connector` (Node) na Rust. Konfiguracja (adres API,
//! login, identyfikator urządzenia) leży w `settings` pod kluczem `timesheet`, hasło
//! w magazynie poświadczeń Windows. Token, refresh token i ciasteczko sesji są tylko
//! w pamięci (`TsState`): nigdy w SQLite, w logach ani w komunikatach błędów.
//!
//! Zapis w TS zastępuje cały dzień (formularz `wypelnij`), dlatego `ts_send` w trybie
//! `keep` sam dokleja wpisy TS spoza wysyłanej listy. Nic nie wysyła się samo:
//! komendę wywołuje dopiero „Zapisz w TS” w podglądzie.
use crate::Database;
use base64::Engine;
use reqwest::{Client, RequestBuilder, Response, StatusCode, Url};
use rusqlite::{params, Connection, OptionalExtension};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{future::Future, time::Duration};
use tauri::{async_runtime::Mutex, Manager};

/// Domyślny adres API Ideo Timesheet.
pub const DEFAULT_API: &str = "https://api.ts.ideo.pl";
/// Nazwa urządzenia w logowaniu (`deviceName`).
pub const DEVICE_NAME: &str = "Todo Track";
/// Odświeżenie tokenu, gdy do wygaśnięcia zostaje mniej niż minuta.
const REFRESH_MARGIN_MS: i64 = 60_000;
/// Ważność tokenu, gdy serwer jej nie podał.
const DEFAULT_LIFETIME_MS: i64 = 30 * 60_000;
/// Najwięcej wpisów w jednym zapisie dnia.
pub const MAX_ENTRIES: usize = 50;
/// Najdłuższy opis wpisu (w znakach).
pub const MAX_DESCRIPTION: usize = 2000;

pub const NOT_CONNECTED: &str = "Połącz konto TS: podaj login i hasło.";
pub const NO_COOKIE: &str = "Serwer TS nie zwrócił ciasteczka sesji po zalogowaniu.";
pub const FORM_ERROR: &str =
    "Nie rozpoznano formularza timesheetu. Strona TS mogła się zmienić albo sesja wygasła.";
pub const SEND_DISABLED: &str = "Wysyłka do TS jest wyłączona w Ustawieniach → Timesheet.";
pub const NETWORK_ERROR: &str = "Nie udało się połączyć z TS. Sprawdź połączenie z siecią firmową.";
pub const API_URL_ERROR: &str =
    "Podaj główny adres API TS z https, bez ścieżki i parametrów, np. https://api.ts.ideo.pl.";
const DATE_ERROR: &str = "Niepoprawny dzień (oczekiwany format RRRR-MM-DD).";
const BAD_RESPONSE: &str = "TS zwrócił niepoprawną odpowiedź.";
/// Zapis przyjęty, ale ponowny odczyt dnia się nie udał.
pub const READ_BACK_WARNING: &str =
    "Zapisano w TS, ale nie udało się odczytać dnia — sprawdź w przeglądarce.";
/// Odczyt po zapisie nie zawiera wysłanych wpisów.
pub const NOT_CONFIRMED: &str = "TS nie potwierdził zapisu — sprawdź dzień w przeglądarce.";

// Pola formularza `wypelnij` jak w konektorze `ideo-ts-api` (`createTimesheetForm`
// przy zapisie, `timesheet-html.parser.ts` przy odczycie). Zapis wysyła
// `<pole>[i]`, odczyt szuka `.jq-<pole z myślnikami> input` w wierszu.
const FIELD_TASK: &str = "zadanie";
const FIELD_HOURS: &str = "zadanie_godziny";
/// Nadgodziny wpisu (`timeOvertime` w konektorze): `zadanie_nadgodziny[i]`.
pub const FIELD_OVERTIME: &str = "zadanie_nadgodziny";
const FIELD_DESCRIPTION: &str = "zadanie_opis";
/// Selektor pola wiersza (`zadanie_nadgodziny` → `.jq-zadanie-nadgodziny input`).
fn row_input(field: &str) -> String {
    format!(".jq-{} input", field.replace('_', "-"))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub api_url: String,
    pub login: String,
    /// 32 znaki szesnastkowe, losowane raz przy pierwszym połączeniu, żeby TS
    /// widział jedno urządzenie.
    pub device_id: String,
    pub device_name: String,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: DEFAULT_API.into(),
            login: String::new(),
            device_id: String::new(),
            device_name: DEVICE_NAME.into(),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    config: Config,
    connected: bool,
    send_enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TsTask {
    pub id: String,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Project {
    pub name: String,
    pub tasks: Vec<TsTask>,
}
/// Wpis wypełniony w TS; wartości jako tekst, bez przeliczania („1,5” albo „1.5”).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Filled {
    pub id: String,
    pub hours: String,
    pub overtime: String,
    pub description: String,
}
/// Wpis wysyłany do TS.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub task_id: String,
    pub hours: String,
    pub overtime: String,
    pub description: String,
}
impl From<&Filled> for Entry {
    fn from(f: &Filled) -> Self {
        Self {
            task_id: f.id.clone(),
            hours: f.hours.clone(),
            overtime: f.overtime.clone(),
            description: f.description.clone(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Replace,
    Keep,
}
impl Mode {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "replace" => Ok(Self::Replace),
            "keep" => Ok(Self::Keep),
            _ => Err("Niepoprawny tryb zapisu w TS.".into()),
        }
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    filled: Vec<Filled>,
    /// Lista faktycznie wysłana w POST (przy `keep` z wpisami TS doklejonymi
    /// z odczytu tuż przed zapisem), z którą porównano odczyt po zapisie.
    sent: Vec<Entry>,
    /// Odczyt po zapisie różni się od `sent` (liczba wpisów, suma godzin albo
    /// nadgodzin o więcej niż 0,01 h); `false`, gdy odczytu brak.
    differs: bool,
    sent_at: i64,
    /// Ostrzeżenie, gdy zapis przyjęto, ale odczyt dnia po zapisie się nie udał
    /// (`filled` jest wtedy puste).
    #[serde(skip_serializing_if = "Option::is_none")]
    warning: Option<String>,
}
/// Ostatnia wysyłka dnia (`ts_sent_days`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sent {
    pub day: String,
    pub sent_at: i64,
    pub hours: String,
}

/// Sesja TS w pamięci. `Debug` celowo nie jest wyprowadzony: pola to sekrety.
#[derive(Clone, Default)]
pub struct Session {
    /// `adres API|login`, dla którego sesja powstała.
    owner: String,
    token: String,
    refresh: String,
    /// Wygaśnięcie tokenu (Unix ms).
    expires_at: i64,
    /// Sam fragment `nazwa=wartość` z pierwszego `Set-Cookie`.
    cookie: String,
}
pub struct TsState(Mutex<Session>);

pub fn setup(app: &tauri::AppHandle) {
    app.manage(TsState(Mutex::new(Session::default())));
}

// ---------------------------------------------------------------------------
// Konfiguracja, magazyn poświadczeń, walidacja

fn read_config(db: &Connection) -> Result<Config, String> {
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='timesheet'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|s| serde_json::from_str(&s).map_err(|_| "Nie można odczytać ustawień TS.".to_owned()))
        .unwrap_or_else(|| Ok(Config::default()))
}
fn write_config(db: &Connection, config: &Config) -> Result<(), String> {
    let raw = serde_json::to_string(config).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO settings(key,value) VALUES('timesheet',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [raw],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
pub fn config(app: &tauri::AppHandle) -> Result<Config, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    read_config(&db)
}
fn save(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    write_config(&db, config)
}
fn entry(app: &tauri::AppHandle, c: &Config) -> Result<keyring::Entry, String> {
    keyring::Entry::new(
        &format!("{}.timesheet", app.config().identifier),
        &format!("{}|{}", c.api_url, c.login),
    )
    .map_err(|_| "Nie można otworzyć magazynu poświadczeń Windows.".into())
}
fn password(app: &tauri::AppHandle, c: &Config) -> Result<String, String> {
    if c.login.is_empty() {
        return Err(NOT_CONNECTED.into());
    }
    entry(app, c)?
        .get_password()
        .map_err(|_| NOT_CONNECTED.into())
}
fn connected(app: &tauri::AppHandle, c: &Config) -> Result<bool, String> {
    if c.login.is_empty() {
        return Ok(false);
    }
    match entry(app, c)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("Nie można odczytać hasła TS z magazynu poświadczeń Windows.".into()),
    }
}

/// Adres API: tylko `https`, bez ścieżki, parametrów i danych logowania; bez `/`
/// na końcu.
pub fn normalize_api(value: &str) -> Result<String, String> {
    let url = Url::parse(value.trim()).map_err(|_| API_URL_ERROR.to_owned())?;
    if url.scheme() != "https"
        || url.host_str().is_none_or(str::is_empty)
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(API_URL_ERROR.into());
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
/// Dzień `RRRR-MM-DD` istniejący w kalendarzu (`isIsoDate` konektora).
pub fn check_date(value: &str) -> Result<(), String> {
    if crate::settings::valid_day(value) {
        Ok(())
    } else {
        Err(DATE_ERROR.into())
    }
}
/// Godziny jak `^\d+([.,]\d{1,2})?$`.
pub fn valid_hours(value: &str) -> bool {
    let (whole, fraction) = match value.find(['.', ',']) {
        Some(at) => (&value[..at], Some(&value[at + 1..])),
        None => (value, None),
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    !whole.is_empty()
        && whole.len() <= 6
        && digits(whole)
        && fraction.is_none_or(|f| (1..=2).contains(&f.len()) && digits(f))
}
pub fn validate_entries(entries: &[Entry]) -> Result<(), String> {
    if entries.is_empty() {
        return Err("Brak wpisów do zapisania w TS.".into());
    }
    if entries.len() > MAX_ENTRIES {
        return Err(format!(
            "Jednorazowo można zapisać w TS najwyżej {MAX_ENTRIES} wpisów."
        ));
    }
    for e in entries {
        let id = e.task_id.trim();
        if id.is_empty() || id.chars().count() > 20 || id.chars().any(char::is_control) {
            return Err("Niepoprawny identyfikator zadania TS.".into());
        }
        if !valid_hours(&e.hours) || !valid_hours(&e.overtime) {
            return Err(
                "Godziny w TS podaj jako liczbę z najwyżej dwoma miejscami po przecinku, np. 1,25."
                    .into(),
            );
        }
        if e.description.chars().count() > MAX_DESCRIPTION {
            return Err(format!(
                "Opis wpisu TS może mieć najwyżej {MAX_DESCRIPTION} znaków."
            ));
        }
        if e.description.chars().any(char::is_control) {
            return Err("Opis wpisu TS nie może zawierać znaków sterujących.".into());
        }
    }
    Ok(())
}
pub fn validate_login(login: &str, password: &str) -> Result<(), String> {
    let n = login.chars().count();
    if n == 0 || n > 100 || login.chars().any(char::is_control) {
        return Err("Podaj login TS (do 100 znaków).".into());
    }
    if password.is_empty() || password.chars().count() > 1024 {
        return Err("Podaj hasło TS.".into());
    }
    Ok(())
}
pub fn ensure_send(s: &crate::settings::AppSettings) -> Result<(), String> {
    if s.ts_send {
        Ok(())
    } else {
        Err(SEND_DISABLED.into())
    }
}
/// `keep`: nasze wpisy pierwsze, za nimi wpisy TS o zadaniach spoza naszej listy
/// (w kolejności z TS, bez zmian); wpisy TS o tych samych zadaniach są zastąpione.
pub fn merge_keep(entries: &[Entry], filled: &[Filled]) -> Vec<Entry> {
    let mut out = entries.to_vec();
    out.extend(
        filled
            .iter()
            .filter(|f| !entries.iter().any(|e| e.task_id.trim() == f.id.trim()))
            .map(Entry::from),
    );
    out
}
/// Godziny z TS w setnych („1,5”, „1.25”); nieczytelne liczą się jako 0.
fn hundredths(value: &str) -> i64 {
    value
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .map_or(0, |v| (v * 100.0).round() as i64)
}
/// Setne części godziny jako „7,50”.
fn hours_text(hundredths: i64) -> String {
    format!("{},{:02}", hundredths / 100, (hundredths % 100).abs())
}
/// Czy odczyt po zapisie zawiera każde wysłane zadanie z niezerowymi godzinami
/// albo nadgodzinami (wiersz z 0 h TS może pominąć).
pub fn confirmed(sent: &[Entry], filled: &[Filled]) -> bool {
    sent.iter()
        .filter(|e| hundredths(&e.hours) + hundredths(&e.overtime) > 0)
        .all(|e| filled.iter().any(|f| f.id.trim() == e.task_id.trim()))
}
fn as_filled(entries: &[Entry]) -> Vec<Filled> {
    entries
        .iter()
        .map(|e| Filled {
            id: e.task_id.trim().to_owned(),
            hours: e.hours.clone(),
            overtime: e.overtime.clone(),
            description: e.description.clone(),
        })
        .collect()
}
/// Suma godzin i nadgodzin wysłanych wpisów (gdy odczytu po zapisie brak).
pub fn sent_total(entries: &[Entry]) -> String {
    work_total(&as_filled(entries))
}
/// Porównanie odczytu po zapisie z listą faktycznie wysłaną: inna liczba
/// wpisów albo suma godzin lub nadgodzin różna o więcej niż 0,01 h.
pub fn saved_differs(sent: &[Entry], read: &[Filled]) -> bool {
    let sent = as_filled(sent);
    let hours = |rows: &[Filled]| rows.iter().map(|r| hundredths(&r.hours)).sum::<i64>();
    let overtime = |rows: &[Filled]| rows.iter().map(|r| hundredths(&r.overtime)).sum::<i64>();
    sent.len() != read.len()
        || (hours(&sent) - hours(read)).abs() > 1
        || (overtime(&sent) - overtime(read)).abs() > 1
}
/// Przekierowanie po zapisie oznacza przyjęcie tylko wtedy, gdy wraca na formularz
/// dnia (albo nie ma `Location`); inne (np. strona logowania) to wygasła sesja.
pub fn save_redirect_ok(location: Option<&str>) -> bool {
    let Some(location) = location.map(str::trim).filter(|l| !l.is_empty()) else {
        return true;
    };
    let path = match Url::parse(location) {
        Ok(url) => url.path().to_owned(),
        Err(_) => location
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_owned(),
    };
    path.trim_end_matches('/').ends_with("/timesheet/wypelnij")
}
/// Wynik po przyjętym zapisie: odczyt bez wysłanych wpisów → błąd (bez znacznika
/// wysyłki); nieudany odczyt → znacznik z sumą wysłanych godzin i ostrzeżenie.
pub fn sent_outcome(
    sent: &[Entry],
    read: Result<Vec<Filled>, String>,
) -> Result<(Vec<Filled>, String, Option<String>), String> {
    match read {
        Ok(filled) if confirmed(sent, &filled) => {
            // Znacznik wysyłki: godziny razem z nadgodzinami.
            let hours = work_total(&filled);
            Ok((filled, hours, None))
        }
        Ok(_) => Err(NOT_CONFIRMED.into()),
        Err(_) => Ok((Vec::new(), sent_total(sent), Some(READ_BACK_WARNING.into()))),
    }
}
fn valid_device_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
fn new_device_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| "Nie udało się wylosować identyfikatora urządzenia.")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

// ---------------------------------------------------------------------------
// Czyste funkcje protokołu

/// Fragment `nazwa=wartość` z nagłówka `Set-Cookie` (bez atrybutów).
pub fn cookie_pair(set_cookie: &str) -> Option<String> {
    let pair = set_cookie.split(';').next()?.trim();
    let (name, _) = pair.split_once('=')?;
    (!name.trim().is_empty()).then(|| pair.to_owned())
}
/// Wygaśnięcie tokenu (Unix ms) jak `AuthService.tokenExpiry`: `exp` z ładunku JWT
/// (bez weryfikacji podpisu), inaczej `expires` jako ms, sekundy epoki albo sekundy
/// od teraz, inaczej 30 minut.
pub fn token_expiry(token: &str, expires: Option<&Value>, now: i64) -> i64 {
    let exp = token.split('.').nth(1).and_then(|payload| {
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload.trim_end_matches('='))
            .ok()?;
        serde_json::from_slice::<Value>(&bytes).ok()?["exp"].as_f64()
    });
    if let Some(exp) = exp.filter(|e| e.is_finite()) {
        return (exp * 1000.0) as i64;
    }
    match expires.and_then(Value::as_f64).filter(|e| e.is_finite()) {
        Some(e) if e > 1e12 => e as i64,
        Some(e) if e > 1e9 => (e * 1000.0) as i64,
        Some(e) if e > 0.0 && e <= 86_400.0 => now + (e * 1000.0) as i64,
        _ => now + DEFAULT_LIFETIME_MS,
    }
}
/// Pola formularza zapisu w kolejności z konektora (`createTimesheetForm`).
pub fn form_fields(date: &str, entries: &[Entry]) -> Vec<(String, String)> {
    let mut fields = vec![
        ("akcja".to_owned(), "zapisz".to_owned()),
        ("dzien".to_owned(), date.to_owned()),
        ("status".to_owned(), "0".to_owned()),
        ("submit".to_owned(), "Zapisz Timesheet".to_owned()),
    ];
    for (i, e) in entries.iter().enumerate() {
        fields.push((format!("{FIELD_TASK}[{i}]"), e.task_id.trim().to_owned()));
        fields.push((format!("{FIELD_HOURS}[{i}]"), e.hours.clone()));
        fields.push((format!("{FIELD_OVERTIME}[{i}]"), e.overtime.clone()));
        fields.push((format!("{FIELD_DESCRIPTION}[{i}]"), e.description.clone()));
    }
    fields
}
/// Suma godzin z odczytu TS jak `decimalHours` („7,50”).
pub fn hours_total(filled: &[Filled]) -> String {
    hours_text(filled.iter().map(|f| hundredths(&f.hours)).sum())
}
/// Suma nadgodzin z odczytu TS („1,25”).
pub fn overtime_total(filled: &[Filled]) -> String {
    hours_text(filled.iter().map(|f| hundredths(&f.overtime)).sum())
}
/// Suma godzin i nadgodzin („8,75”): czas dnia zapisany w TS.
pub fn work_total(filled: &[Filled]) -> String {
    hours_text(
        filled
            .iter()
            .map(|f| hundredths(&f.hours) + hundredths(&f.overtime))
            .sum(),
    )
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("poprawny selektor CSS")
}
fn text(element: ElementRef) -> String {
    element.text().collect::<String>().trim().to_owned()
}
fn first_select(document: &Html) -> Result<ElementRef<'_>, String> {
    document
        .select(&selector(".jq-wiersz-ts .jq-zadanie select"))
        .next()
        .ok_or_else(|| FORM_ERROR.to_owned())
}
/// Projekty (`optgroup`) i zadania z pierwszej listy zadań formularza dnia.
pub fn parse_tasks(html: &str) -> Result<Vec<Project>, String> {
    let document = Html::parse_document(html);
    let select = first_select(&document)?;
    let mut projects = Vec::new();
    for group in select.child_elements() {
        if group.value().name() != "optgroup" {
            continue;
        }
        let tasks = group
            .child_elements()
            .filter(|o| o.value().name() == "option")
            .filter_map(|o| {
                let id = o.value().attr("value")?.trim();
                (!id.is_empty()).then(|| TsTask {
                    id: id.to_owned(),
                    name: text(o),
                })
            })
            .collect();
        projects.push(Project {
            name: group.value().attr("label").unwrap_or("").trim().to_owned(),
            tasks,
        });
    }
    Ok(projects)
}
/// Wiersze już wypełnione w TS; wiersze bez wybranego zadania są pomijane.
pub fn parse_filled(html: &str) -> Result<Vec<Filled>, String> {
    let document = Html::parse_document(html);
    first_select(&document)?;
    let selected = selector(".jq-zadanie select option[selected]");
    let hours = selector(&row_input(FIELD_HOURS));
    let overtime = selector(&row_input(FIELD_OVERTIME));
    let description = selector(&row_input(FIELD_DESCRIPTION));
    let value = |row: ElementRef, s: &Selector| {
        row.select(s)
            .next()
            .and_then(|e| e.value().attr("value"))
            .unwrap_or("")
            .to_owned()
    };
    Ok(document
        .select(&selector("tr.jq-wiersz-ts"))
        .map(|row| Filled {
            id: value(row, &selected).trim().to_owned(),
            hours: value(row, &hours).trim().to_owned(),
            overtime: value(row, &overtime).trim().to_owned(),
            description: value(row, &description),
        })
        .filter(|f| !f.id.is_empty())
        .collect())
}

// ---------------------------------------------------------------------------
// HTTP

/// Błąd żądania: kod HTTP (do rozpoznania 401) albo gotowy komunikat.
#[derive(Debug, PartialEq)]
pub enum TsError {
    Status(StatusCode),
    Message(String),
}
impl TsError {
    fn text(self) -> String {
        match self {
            Self::Status(s) => status_error(s),
            Self::Message(m) => m,
        }
    }
    fn unauthorized(&self) -> bool {
        matches!(self, Self::Status(s) if *s == StatusCode::UNAUTHORIZED || s.is_redirection())
    }
}
/// Komunikaty błędów HTTP TS po polsku. Nigdy nie zawierają sekretów.
pub fn status_error(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "TS odrzucił logowanie. Sprawdź login i hasło.".into(),
        StatusCode::FORBIDDEN => "Brak uprawnień do timesheetu.".into(),
        StatusCode::NOT_FOUND => "Nie znaleziono strony timesheetu. Sprawdź adres API.".into(),
        StatusCode::TOO_MANY_REQUESTS => {
            "TS ograniczył liczbę zapytań. Spróbuj ponownie za chwilę.".into()
        }
        s if s.is_redirection() => {
            "TS przekierował żądanie (sesja mogła wygasnąć). Spróbuj ponownie.".into()
        }
        _ => format!(
            "TS zwrócił błąd HTTP {}. Spróbuj ponownie.",
            status.as_u16()
        ),
    }
}
fn client() -> Result<Client, String> {
    let builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    // Testy rozmawiają wyłącznie z lokalnym serwerem na 127.0.0.1, bez proxy
    // systemowego, żeby żadne żądanie nie opuściło maszyny.
    #[cfg(test)]
    let builder = builder.no_proxy();
    builder
        .build()
        .map_err(|_| "Nie udało się utworzyć połączenia HTTPS.".into())
}
/// Wysyła żądanie; błąd sieci (z adresem, w którym może być refresh token) nigdy
/// nie trafia do komunikatu.
async fn send(request: RequestBuilder) -> Result<Response, TsError> {
    request
        .send()
        .await
        .map_err(|_| TsError::Message(NETWORK_ERROR.into()))
}
#[derive(Deserialize)]
struct Tokens {
    #[serde(default)]
    token: String,
    #[serde(default)]
    refresh: String,
    #[serde(default)]
    expires: Option<Value>,
}
/// Połączenie z TS: klient, adres i dane logowania (hasło tylko na czas operacji).
pub struct Api {
    http: Client,
    base: String,
    login: String,
    password: String,
    device_id: String,
    device_name: String,
}
impl Api {
    fn new(base: &str, c: &Config, password: String) -> Result<Self, String> {
        // W testach dozwolony jest tylko lokalny serwer z fixtures, nigdy prawdziwy TS.
        #[cfg(test)]
        assert!(
            base.starts_with("http://127.0.0.1:"),
            "testy nie mogą łączyć się z serwerem TS: {base}"
        );
        Ok(Self {
            http: client()?,
            base: base.to_owned(),
            login: c.login.clone(),
            password,
            device_id: c.device_id.clone(),
            device_name: c.device_name.clone(),
        })
    }
    fn owner(&self) -> String {
        format!("{}|{}", self.base, self.login)
    }
    fn authed(&self, request: RequestBuilder, s: &Session) -> RequestBuilder {
        let request = request.header("Authorization", format!("Bearer {}", s.token));
        if s.cookie.is_empty() {
            request
        } else {
            request.header("Cookie", &s.cookie)
        }
    }
    async fn tokens(response: Response) -> Result<Tokens, TsError> {
        if !response.status().is_success() {
            return Err(TsError::Status(response.status()));
        }
        let tokens: Tokens = response
            .json()
            .await
            .map_err(|_| TsError::Message(BAD_RESPONSE.into()))?;
        if tokens.token.is_empty() {
            return Err(TsError::Message(BAD_RESPONSE.into()));
        }
        Ok(tokens)
    }
    fn apply(s: &mut Session, tokens: Tokens) {
        s.expires_at = token_expiry(&tokens.token, tokens.expires.as_ref(), crate::db::now());
        s.token = tokens.token;
        s.refresh = tokens.refresh;
    }
    /// `GET /api/user/identity` → ciasteczko sesji stron timesheetu.
    async fn identity(&self, s: &Session) -> Result<Option<String>, TsError> {
        let response =
            send(self.authed(self.http.get(format!("{}/api/user/identity", self.base)), s)).await?;
        if !response.status().is_success() {
            return Err(TsError::Status(response.status()));
        }
        Ok(response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .next()
            .and_then(|v| v.to_str().ok())
            .and_then(cookie_pair))
    }
    /// Pełne logowanie hasłem: `POST /api/login`, potem identity.
    pub async fn login(&self, s: &mut Session) -> Result<(), TsError> {
        *s = Session::default();
        let response = send(self.http.post(format!("{}/api/login", self.base)).json(
            &serde_json::json!({
                "login": self.login,
                "password": self.password,
                "deviceId": self.device_id,
                "deviceName": self.device_name,
            }),
        ))
        .await?;
        let mut next = Session {
            owner: self.owner(),
            ..Session::default()
        };
        Self::apply(&mut next, Self::tokens(response).await?);
        next.cookie = self
            .identity(&next)
            .await?
            .ok_or_else(|| TsError::Message(NO_COOKIE.into()))?;
        *s = next;
        Ok(())
    }
    /// Odświeżenie tokenu i ciasteczka; 401 (albo brak refresh tokenu) → logowanie.
    async fn refresh(&self, s: &mut Session) -> Result<(), TsError> {
        if s.refresh.is_empty() || s.owner != self.owner() {
            return self.login(s).await;
        }
        let response = send(
            self.authed(
                self.http
                    .get(format!("{}/api/token/refresh", self.base))
                    .query(&[
                        ("id", "api-ts"),
                        ("name", "api-ts"),
                        ("refresh", &s.refresh),
                    ]),
                s,
            ),
        )
        .await?;
        if response.status() == StatusCode::UNAUTHORIZED {
            return self.login(s).await;
        }
        let mut next = s.clone();
        Self::apply(&mut next, Self::tokens(response).await?);
        // Jak `refreshOnce` w konektorze: odrzucone identity (401 albo przekierowanie
        // na logowanie) po odświeżeniu też kończy się pełnym logowaniem hasłem.
        match self.identity(&next).await {
            Ok(Some(cookie)) => next.cookie = cookie,
            Ok(None) => {}
            Err(e) if e.unauthorized() => return self.login(s).await,
            Err(e) => return Err(e),
        }
        *s = next;
        Ok(())
    }
    async fn ensure_fresh(&self, s: &mut Session) -> Result<(), TsError> {
        if s.token.is_empty() || s.owner != self.owner() {
            self.login(s).await
        } else if crate::db::now() >= s.expires_at - REFRESH_MARGIN_MS {
            self.refresh(s).await
        } else {
            Ok(())
        }
    }
    /// Odpowiednik `withAuthentication`: świeża sesja, operacja, a przy 401
    /// jednorazowe odświeżenie i powtórka. Przekierowanie (strona logowania przy
    /// ważnym tokenie = wygasło ciasteczko) → pełne logowanie po nowe ciasteczko
    /// i jedna powtórka.
    pub async fn with_auth<T, F, Fut>(&self, s: &mut Session, op: F) -> Result<T, TsError>
    where
        F: Fn(Session) -> Fut,
        Fut: Future<Output = Result<T, TsError>>,
    {
        self.ensure_fresh(s).await?;
        match op(s.clone()).await {
            Err(TsError::Status(status)) if status.is_redirection() => {
                self.login(s).await?;
                op(s.clone()).await
            }
            Err(e) if e.unauthorized() => {
                self.refresh(s).await?;
                op(s.clone()).await
            }
            other => other,
        }
    }
    fn page_url(&self) -> String {
        format!("{}/timesheet/wypelnij", self.base)
    }
    /// `GET /timesheet/wypelnij?dzien=` → HTML formularza dnia.
    pub async fn fill_page(&self, s: &Session, date: &str) -> Result<String, TsError> {
        let response =
            send(self.authed(self.http.get(self.page_url()).query(&[("dzien", date)]), s)).await?;
        if !response.status().is_success() {
            return Err(TsError::Status(response.status()));
        }
        response
            .text()
            .await
            .map_err(|_| TsError::Message(BAD_RESPONSE.into()))
    }
    /// `POST /timesheet/wypelnij?dzien=` multipartem; 2xx albo 3xx z powrotem na
    /// formularz dnia = przyjęte. Inne przekierowanie (logowanie) to błąd 3xx, po
    /// którym `with_auth` loguje się ponownie i powtarza zapis raz.
    pub async fn save_day(
        &self,
        s: &Session,
        date: &str,
        entries: &[Entry],
    ) -> Result<(), TsError> {
        let form = form_fields(date, entries)
            .into_iter()
            .fold(reqwest::multipart::Form::new(), |form, (k, v)| {
                form.text(k, v)
            });
        let response = send(
            self.authed(
                self.http
                    .post(self.page_url())
                    .query(&[("dzien", date)])
                    .multipart(form),
                s,
            ),
        )
        .await?;
        let status = response.status();
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok());
        if status.is_success() || (status.is_redirection() && save_redirect_ok(location)) {
            Ok(())
        } else {
            Err(TsError::Status(status))
        }
    }
    /// Zapis dnia z odczytem przed (`keep`: wpisy TS spoza listy doklejone z
    /// aktualnej strony, nie z podglądu) i po zapisie. Zwraca listę faktycznie
    /// wysłaną i odczyt po zapisie (błąd odczytu nie jest błędem zapisu).
    pub async fn send_day(
        &self,
        session: &mut Session,
        date: &str,
        entries: Vec<Entry>,
        mode: Mode,
    ) -> Result<(Vec<Entry>, Result<Vec<Filled>, String>), String> {
        let to_send = if mode == Mode::Keep {
            let html = self
                .with_auth(session, |s| async move { self.fill_page(&s, date).await })
                .await
                .map_err(TsError::text)?;
            merge_keep(&entries, &parse_filled(&html)?)
        } else {
            entries
        };
        let sent = &to_send;
        self.with_auth(
            session,
            |s| async move { self.save_day(&s, date, sent).await },
        )
        .await
        .map_err(TsError::text)?;
        // Zapis przyjęty: od tej chwili dane są w TS, więc nieudany odczyt nie może
        // wyglądać jak nieudana wysyłka.
        let read = self
            .with_auth(session, |s| async move { self.fill_page(&s, date).await })
            .await
            .map_err(TsError::text)
            .and_then(|html| parse_filled(&html));
        Ok((to_send, read))
    }
}

// ---------------------------------------------------------------------------
// ts_sent_days

pub fn save_sent(db: &Connection, day: &str, sent_at: i64, hours: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO ts_sent_days(day,sent_at,hours) VALUES(?1,?2,?3) ON CONFLICT(day) DO UPDATE SET sent_at=excluded.sent_at, hours=excluded.hours",
        params![day, sent_at, hours],
    )?;
    Ok(())
}
pub fn load_sent(db: &Connection, day: &str) -> rusqlite::Result<Option<Sent>> {
    db.query_row(
        "SELECT day,sent_at,hours FROM ts_sent_days WHERE day=?1",
        [day],
        |r| {
            Ok(Sent {
                day: r.get(0)?,
                sent_at: r.get(1)?,
                hours: r.get(2)?,
            })
        },
    )
    .optional()
}

// ---------------------------------------------------------------------------
// ts_project_map (T2): domyślne zadanie TS dla projektu Todo Track

/// Domyślne zadanie TS projektu. Nazwa projektu porównywana bez wielkości liter
/// (`COLLATE NOCASE`, jak w SQLite: tylko litery ASCII).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMap {
    pub project: String,
    pub ts_task_id: String,
    pub ts_task_name: String,
}
/// Identyfikator zadania TS jak w `validate_entries`.
pub fn valid_task_id(id: &str) -> bool {
    !id.is_empty() && id.chars().count() <= 20 && !id.chars().any(char::is_control)
}
/// Nazwa zadania TS zapamiętywana do pokazania bez połączenia.
pub fn valid_task_name(name: &str) -> bool {
    name.chars().count() <= 300 && !name.chars().any(char::is_control)
}
pub fn load_project_map(db: &Connection) -> rusqlite::Result<Vec<ProjectMap>> {
    db.prepare(
        "SELECT project,ts_task_id,ts_task_name FROM ts_project_map ORDER BY project COLLATE NOCASE",
    )?
    .query_map([], |r| {
        Ok(ProjectMap {
            project: r.get(0)?,
            ts_task_id: r.get(1)?,
            ts_task_name: r.get(2)?,
        })
    })?
    .collect()
}
/// Ustawia domyślne zadanie TS projektu; puste `ts_task_id` usuwa mapowanie.
pub fn set_project_map(
    db: &Connection,
    project: &str,
    ts_task_id: &str,
    ts_task_name: &str,
) -> Result<Vec<ProjectMap>, String> {
    let project = project.trim();
    let id = ts_task_id.trim();
    let name = ts_task_name.trim();
    if project.is_empty() || project.chars().count() > 80 || project.chars().any(char::is_control) {
        return Err("Niepoprawna nazwa projektu.".into());
    }
    if id.is_empty() {
        db.execute("DELETE FROM ts_project_map WHERE project=?1", [project])
            .map_err(|e| e.to_string())?;
    } else {
        if !valid_task_id(id) {
            return Err("Niepoprawny identyfikator zadania TS.".into());
        }
        if !valid_task_name(name) {
            return Err("Niepoprawna nazwa zadania TS.".into());
        }
        db.execute(
            "INSERT INTO ts_project_map(project,ts_task_id,ts_task_name) VALUES(?1,?2,?3) ON CONFLICT(project) DO UPDATE SET project=excluded.project, ts_task_id=excluded.ts_task_id, ts_task_name=excluded.ts_task_name",
            params![project, id, name],
        )
        .map_err(|e| e.to_string())?;
    }
    load_project_map(db).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Komendy

fn settings_of(app: &tauri::AppHandle, config: Config) -> Result<Settings, String> {
    Ok(Settings {
        connected: connected(app, &config)?,
        send_enabled: crate::settings::current(app).ts_send,
        config,
    })
}
/// Połączenie z TS dla bieżącej konfiguracji (hasło z magazynu).
fn api(app: &tauri::AppHandle) -> Result<Api, String> {
    let c = config(app)?;
    let secret = password(app, &c)?;
    Api::new(&normalize_api(&c.api_url)?, &c, secret)
}
async fn clear_session(app: &tauri::AppHandle) {
    *app.state::<TsState>().0.lock().await = Session::default();
}

#[tauri::command]
pub fn ts_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    settings_of(&app, c)
}
#[tauri::command]
pub async fn ts_connect(
    app: tauri::AppHandle,
    api_url: String,
    login: String,
    password: String,
) -> Result<Settings, String> {
    let old = config(&app)?;
    let login = login.trim().to_owned();
    validate_login(&login, &password)?;
    let c = Config {
        api_url: normalize_api(&api_url)?,
        login,
        device_id: if valid_device_id(&old.device_id) {
            old.device_id.clone()
        } else {
            new_device_id()?
        },
        device_name: DEVICE_NAME.into(),
    };
    let api = Api::new(&c.api_url, &c, password)?;
    let state = app.state::<TsState>();
    let mut session = state.0.lock().await;
    let mut fresh = Session::default();
    api.login(&mut fresh).await.map_err(TsError::text)?;
    entry(&app, &c)?
        .set_password(&api.password)
        .map_err(|_| "Nie udało się zapisać hasła w magazynie poświadczeń Windows.".to_owned())?;
    save(&app, &c)?;
    if !old.login.is_empty() && (old.api_url != c.api_url || old.login != c.login) {
        let _ = entry(&app, &old)?.delete_credential();
    }
    *session = fresh;
    drop(session);
    settings_of(&app, c)
}
#[tauri::command]
pub async fn ts_disconnect(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    if !c.login.is_empty() {
        match entry(&app, &c)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {
                return Err("Nie udało się usunąć hasła z magazynu poświadczeń Windows.".into())
            }
        }
    }
    clear_session(&app).await;
    settings_of(&app, c)
}
/// HTML formularza dnia przez sesję z `TsState`.
async fn day_page(app: &tauri::AppHandle, api: &Api, date: &str) -> Result<String, String> {
    let state = app.state::<TsState>();
    let mut session = state.0.lock().await;
    api.with_auth(
        &mut session,
        |s| async move { api.fill_page(&s, date).await },
    )
    .await
    .map_err(TsError::text)
}
#[tauri::command]
pub async fn ts_tasks(app: tauri::AppHandle, date: String) -> Result<Vec<Project>, String> {
    check_date(&date)?;
    let api = api(&app)?;
    let projects = parse_tasks(&day_page(&app, &api, &date).await?)?;
    // Serwer MCP (A2): `ts_tasks` zwraca ostatnio pobraną listę z pamięci.
    crate::mcp::remember_ts_tasks(&app, &projects);
    Ok(projects)
}
#[tauri::command]
pub async fn ts_filled(app: tauri::AppHandle, date: String) -> Result<Vec<Filled>, String> {
    check_date(&date)?;
    let api = api(&app)?;
    parse_filled(&day_page(&app, &api, &date).await?)
}
/// Zapis dnia w TS (zastępuje cały dzień). Wywoływany tylko z podglądu po
/// kliknięciu „Zapisz w TS”; po zapisie odczytuje dzień z TS i zapamiętuje wysyłkę.
#[tauri::command]
pub async fn ts_send(
    app: tauri::AppHandle,
    date: String,
    entries: Vec<Entry>,
    mode: String,
) -> Result<SendResult, String> {
    ensure_send(&crate::settings::current(&app))?;
    check_date(&date)?;
    let mode = Mode::parse(&mode)?;
    validate_entries(&entries)?;
    let api = api(&app)?;
    let state = app.state::<TsState>();
    let mut session = state.0.lock().await;
    let (sent, read) = api.send_day(&mut session, &date, entries, mode).await?;
    drop(session);
    let (filled, hours, warning) = sent_outcome(&sent, read)?;
    let differs = warning.is_none() && saved_differs(&sent, &filled);
    let sent_at = crate::db::now();
    {
        let db_state = app.state::<Database>();
        let db = db_state.0.lock().map_err(|e| e.to_string())?;
        save_sent(&db, &date, sent_at, &hours).map_err(|e| e.to_string())?;
    }
    Ok(SendResult {
        filled,
        sent,
        differs,
        sent_at,
        warning,
    })
}
#[tauri::command]
pub fn ts_sent(app: tauri::AppHandle, day: String) -> Result<Option<Sent>, String> {
    check_date(&day)?;
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    load_sent(&db, &day).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn ts_project_map(app: tauri::AppHandle) -> Result<Vec<ProjectMap>, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    load_project_map(&db).map_err(|e| e.to_string())
}
/// Domyślne zadanie TS projektu (puste `ts_task_id` usuwa); zwraca całą listę.
#[tauri::command]
pub fn ts_set_project_map(
    app: tauri::AppHandle,
    project: String,
    ts_task_id: String,
    ts_task_name: String,
) -> Result<Vec<ProjectMap>, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    set_project_map(&db, &project, &ts_task_id, &ts_task_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const PAGE: &str = include_str!("../tests/fixtures/ts_wypelnij.html");

    fn entry(id: &str, hours: &str, description: &str) -> Entry {
        Entry {
            task_id: id.into(),
            hours: hours.into(),
            overtime: "0".into(),
            description: description.into(),
        }
    }

    #[test]
    fn parses_projects_and_tasks_from_first_select() {
        let projects = parse_tasks(PAGE).unwrap();
        assert_eq!(
            projects,
            vec![
                Project {
                    name: "Projekt Alfa".into(),
                    tasks: vec![
                        TsTask {
                            id: "101".into(),
                            name: "Analiza wymagań".into()
                        },
                        TsTask {
                            id: "102".into(),
                            name: "Programowanie".into()
                        },
                    ],
                },
                Project {
                    name: "Projekt Beta".into(),
                    tasks: vec![TsTask {
                        id: "205".into(),
                        name: "Spotkania zespołu".into()
                    }],
                },
            ]
        );
    }

    #[test]
    fn parses_filled_rows_as_text_and_skips_rows_without_task() {
        let filled = parse_filled(PAGE).unwrap();
        assert_eq!(
            filled,
            vec![
                Filled {
                    id: "101".into(),
                    hours: "1,5".into(),
                    overtime: "0".into(),
                    description: "Przegląd zgłoszeń; poprawki w module wysyłki żółć".into(),
                },
                Filled {
                    id: "205".into(),
                    hours: "0.25".into(),
                    overtime: "0".into(),
                    description: "Daily".into(),
                },
            ]
        );
        assert_eq!(hours_total(&filled), "1,75");
    }

    #[test]
    fn page_without_form_is_reported() {
        let login =
            "<html><body><form><input name=login><input type=password></form></body></html>";
        assert_eq!(parse_tasks(login).unwrap_err(), FORM_ERROR);
        assert_eq!(parse_filled(login).unwrap_err(), FORM_ERROR);
        assert_eq!(parse_filled("").unwrap_err(), FORM_ERROR);
    }

    #[test]
    fn multipart_fields_follow_connector_order() {
        let fields = form_fields(
            "2026-10-15",
            &[entry("101", "1,50", "Opis; ą"), entry(" 205 ", "0,25", "")],
        );
        let names: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "akcja",
                "dzien",
                "status",
                "submit",
                "zadanie[0]",
                "zadanie_godziny[0]",
                "zadanie_nadgodziny[0]",
                "zadanie_opis[0]",
                "zadanie[1]",
                "zadanie_godziny[1]",
                "zadanie_nadgodziny[1]",
                "zadanie_opis[1]",
            ]
        );
        let value = |name: &str| {
            fields
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
                .unwrap()
        };
        assert_eq!(value("akcja"), "zapisz");
        assert_eq!(value("dzien"), "2026-10-15");
        assert_eq!(value("status"), "0");
        assert_eq!(value("submit"), "Zapisz Timesheet");
        assert_eq!(value("zadanie[1]"), "205");
        assert_eq!(value("zadanie_godziny[0]"), "1,50");
        assert_eq!(value("zadanie_nadgodziny[1]"), "0");
        assert_eq!(value("zadanie_opis[0]"), "Opis; ą");
        assert_eq!(form_fields("2026-10-15", &[]).len(), 4);
    }

    fn jwt(payload: &Value) -> String {
        let b64 = base64::engine::general_purpose::URL_SAFE_NO_PAD;
        format!(
            "{}.{}.podpis",
            b64.encode(br#"{"alg":"HS256"}"#),
            b64.encode(payload.to_string())
        )
    }

    #[test]
    fn token_expiry_variants() {
        let now = 1_760_000_000_000;
        // exp z JWT ma pierwszeństwo przed `expires`.
        assert_eq!(
            token_expiry(&jwt(&json!({"exp": 1_760_000_900})), Some(&json!(5)), now),
            1_760_000_900_000
        );
        // `expires` w milisekundach.
        assert_eq!(
            token_expiry("bez-jwt", Some(&json!(1_760_000_123_456_i64)), now),
            1_760_000_123_456
        );
        // `expires` w sekundach epoki.
        assert_eq!(
            token_expiry("bez-jwt", Some(&json!(1_760_000_600)), now),
            1_760_000_600_000
        );
        // `expires` w sekundach od teraz (JWT bez `exp`).
        assert_eq!(
            token_expiry(&jwt(&json!({"sub": "x"})), Some(&json!(1800)), now),
            now + 1_800_000
        );
        // Brak danych → 30 minut.
        assert_eq!(
            token_expiry("a.%%%.c", None, now),
            now + DEFAULT_LIFETIME_MS
        );
        assert_eq!(
            token_expiry("x", Some(&json!(-5)), now),
            now + DEFAULT_LIFETIME_MS
        );
        assert_eq!(
            token_expiry("x", Some(&json!("1800")), now),
            now + DEFAULT_LIFETIME_MS
        );
    }

    #[test]
    fn cookie_pair_drops_attributes() {
        assert_eq!(
            cookie_pair("PHPSESSID=abc123; path=/; HttpOnly; Secure; SameSite=Lax").as_deref(),
            Some("PHPSESSID=abc123")
        );
        assert_eq!(cookie_pair("sid=a=b").as_deref(), Some("sid=a=b"));
        assert_eq!(cookie_pair("  x=1  ").as_deref(), Some("x=1"));
        assert_eq!(cookie_pair("bez-rownasie; path=/"), None);
        assert_eq!(cookie_pair("=wartosc"), None);
        assert_eq!(cookie_pair(""), None);
    }

    #[test]
    fn date_validation() {
        assert!(check_date("2026-10-15").is_ok());
        assert!(check_date("2024-02-29").is_ok());
        for bad in [
            "2026-02-30",
            "2026-13-01",
            "2026-1-05",
            "15.10.2026",
            "",
            "2026-10-15x",
        ] {
            assert!(check_date(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn entry_validation() {
        assert!(validate_entries(&[entry("101", "1,37", "Opis; ąę")]).is_ok());
        assert!(validate_entries(&[entry("101", "2", "")]).is_ok());
        assert!(validate_entries(&[entry("101", "0.5", "")]).is_ok());
        for hours in ["1,375", "abc", "", ",5", "1,", "1.2.3", "-1", "1 ,5", "1e2"] {
            assert!(
                validate_entries(&[entry("101", hours, "")]).is_err(),
                "{hours}"
            );
        }
        let mut overtime = entry("101", "1", "");
        overtime.overtime = "x".into();
        assert!(validate_entries(&[overtime]).is_err());
        assert!(validate_entries(&[entry("", "1", "")]).is_err());
        assert!(validate_entries(&[entry("   ", "1", "")]).is_err());
        assert!(validate_entries(&[entry(&"1".repeat(21), "1", "")]).is_err());
        assert!(validate_entries(&[entry(&"1".repeat(20), "1", "")]).is_ok());
        assert!(validate_entries(&[entry("1", "1", &"ż".repeat(2000))]).is_ok());
        assert!(validate_entries(&[entry("1", "1", &"ż".repeat(2001))]).is_err());
        assert!(validate_entries(&[entry("1", "1", "a\nb")]).is_err());
        assert!(validate_entries(&[entry("1", "1", "a\u{7}")]).is_err());
        assert!(validate_entries(&[]).is_err());
        let many: Vec<Entry> = (0..51).map(|i| entry(&i.to_string(), "1", "")).collect();
        assert!(validate_entries(&many).is_err());
        assert!(validate_entries(&many[..50]).is_ok());
        assert!(Mode::parse("replace").is_ok() && Mode::parse("keep").is_ok());
        assert!(Mode::parse("merge").is_err());
    }

    #[test]
    fn login_validation() {
        assert!(validate_login("jan.kowalski", "tajne").is_ok());
        assert!(validate_login("", "tajne").is_err());
        assert!(validate_login(&"a".repeat(101), "tajne").is_err());
        assert!(validate_login("jan", "").is_err());
        assert!(validate_login("jan", &"x".repeat(1025)).is_err());
    }

    #[test]
    fn keep_appends_other_ts_entries_after_ours() {
        let filled = parse_filled(PAGE).unwrap();
        let ours = vec![
            entry("205", "1,00", "Spotkanie"),
            entry("102", "2,50", "Kod"),
        ];
        let merged = merge_keep(&ours, &filled);
        assert_eq!(
            merged,
            vec![
                entry("205", "1,00", "Spotkanie"),
                entry("102", "2,50", "Kod"),
                Entry {
                    task_id: "101".into(),
                    hours: "1,5".into(),
                    overtime: "0".into(),
                    description: "Przegląd zgłoszeń; poprawki w module wysyłki żółć".into(),
                },
            ]
        );
        // Wszystkie zadania TS w naszej liście → nic nie dochodzi.
        let all = vec![entry("101", "1", ""), entry("205", "1", "")];
        assert_eq!(merge_keep(&all, &filled), all);
        // Pusty dzień w TS → tylko nasze.
        assert_eq!(merge_keep(&ours, &[]), ours);
    }

    #[test]
    fn api_url_is_normalized() {
        assert_eq!(normalize_api(DEFAULT_API).unwrap(), DEFAULT_API);
        assert_eq!(
            normalize_api(" https://api.ts.ideo.pl/ ").unwrap(),
            "https://api.ts.ideo.pl"
        );
        assert_eq!(
            normalize_api("https://TS-test.example.com:8443").unwrap(),
            "https://ts-test.example.com:8443"
        );
        for bad in [
            "http://api.ts.ideo.pl",
            "https://api.ts.ideo.pl/api",
            "https://api.ts.ideo.pl/?a=1",
            "https://api.ts.ideo.pl/#x",
            "https://user:pass@api.ts.ideo.pl",
            "api.ts.ideo.pl",
            "",
        ] {
            assert!(normalize_api(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sending_is_blocked_when_switched_off() {
        let mut s = crate::settings::AppSettings::default();
        assert!(s.ts_send);
        assert!(ensure_send(&s).is_ok());
        s.ts_send = false;
        assert_eq!(ensure_send(&s).unwrap_err(), SEND_DISABLED);
    }

    #[test]
    fn sent_days_are_stored_and_replaced() {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        assert_eq!(load_sent(&db, "2026-10-15").unwrap(), None);
        save_sent(&db, "2026-10-15", 1000, "7,50").unwrap();
        save_sent(&db, "2026-10-16", 2000, "1,00").unwrap();
        save_sent(&db, "2026-10-15", 3000, "8,00").unwrap();
        assert_eq!(
            load_sent(&db, "2026-10-15").unwrap(),
            Some(Sent {
                day: "2026-10-15".into(),
                sent_at: 3000,
                hours: "8,00".into()
            })
        );
        assert_eq!(load_sent(&db, "2026-10-16").unwrap().unwrap().sent_at, 2000);
        // `ts_project_map` (T2) istnieje i ma porównanie bez wielkości liter.
        db.execute(
            "INSERT INTO ts_project_map(project,ts_task_id) VALUES('WMS','101')",
            [],
        )
        .unwrap();
        assert!(db
            .execute(
                "INSERT INTO ts_project_map(project,ts_task_id) VALUES('wms','102')",
                [],
            )
            .is_err());
    }

    #[test]
    fn project_map_insert_replace_delete_nocase() {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        assert!(load_project_map(&db).unwrap().is_empty());
        let list = set_project_map(&db, " WMS ", " 1101 ", "Rozwój").unwrap();
        assert_eq!(
            list,
            vec![ProjectMap {
                project: "WMS".into(),
                ts_task_id: "1101".into(),
                ts_task_name: "Rozwój".into()
            }]
        );
        set_project_map(&db, "Zespół", "2201", "Spotkania").unwrap();
        // Nadpisanie tego samego projektu w innej wielkości liter (NOCASE).
        let list = set_project_map(&db, "wms", "1103", "Code review").unwrap();
        assert_eq!(list.len(), 2);
        let wms = list.iter().find(|m| m.ts_task_id == "1103").unwrap();
        assert_eq!(
            (wms.project.as_str(), wms.ts_task_name.as_str()),
            ("wms", "Code review")
        );
        assert!(!list.iter().any(|m| m.ts_task_id == "1101"));
        // Usunięcie pustym id, też bez względu na wielkość liter.
        let list = set_project_map(&db, "WMS", "  ", "").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].project, "Zespół");
        // Usunięcie nieistniejącego mapowania niczego nie psuje.
        assert_eq!(set_project_map(&db, "Inny", "", "").unwrap().len(), 1);
        // Walidacja: projekt, identyfikator i nazwa.
        assert!(set_project_map(&db, "  ", "1", "x").is_err());
        assert!(set_project_map(&db, &"p".repeat(81), "1", "x").is_err());
        assert!(set_project_map(&db, "WMS", &"1".repeat(21), "x").is_err());
        assert!(set_project_map(&db, "WMS", "1", "a\nb").is_err());
        assert_eq!(load_project_map(&db).unwrap().len(), 1);
    }

    #[test]
    fn config_defaults_and_storage() {
        let db = Connection::open_in_memory().unwrap();
        crate::db::init(&db).unwrap();
        let c = read_config(&db).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.api_url, DEFAULT_API);
        assert_eq!(c.device_name, DEVICE_NAME);
        let saved = Config {
            login: "jan".into(),
            device_id: new_device_id().unwrap(),
            ..Config::default()
        };
        assert!(valid_device_id(&saved.device_id));
        assert_ne!(saved.device_id, new_device_id().unwrap());
        write_config(&db, &saved).unwrap();
        assert_eq!(read_config(&db).unwrap(), saved);
        // Konfiguracja bez sekretów: tylko adres, login i urządzenie.
        let raw: String = db
            .query_row(
                "SELECT value FROM settings WHERE key='timesheet'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let keys: Vec<String> = serde_json::from_str::<serde_json::Map<String, Value>>(&raw)
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["apiUrl", "deviceId", "deviceName", "login"]);
    }

    #[test]
    fn errors_are_polish_and_never_carry_secrets() {
        assert_eq!(
            status_error(StatusCode::UNAUTHORIZED),
            "TS odrzucił logowanie. Sprawdź login i hasło."
        );
        assert_eq!(
            status_error(StatusCode::FORBIDDEN),
            "Brak uprawnień do timesheetu."
        );
        assert!(status_error(StatusCode::NOT_FOUND).contains("adres API"));
        assert!(status_error(StatusCode::TOO_MANY_REQUESTS).contains("ograniczył"));
        assert_eq!(
            status_error(StatusCode::BAD_GATEWAY),
            "TS zwrócił błąd HTTP 502. Spróbuj ponownie."
        );
        assert!(TsError::Status(StatusCode::UNAUTHORIZED).unauthorized());
        assert!(TsError::Status(StatusCode::FOUND).unauthorized());
        assert!(!TsError::Status(StatusCode::FORBIDDEN).unauthorized());
    }

    /// Lokalny serwer na 127.0.0.1 (nie prawdziwy TS): odpowiada kolejno podanymi
    /// odpowiedziami i zwraca odebrane żądania (nagłówki i treść).
    fn fake_server(responses: Vec<String>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0; 4096];
                let header_end = loop {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    raw.extend_from_slice(&buffer[..n]);
                    if let Some(at) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break at + 4;
                    }
                };
                let head = String::from_utf8_lossy(&raw[..header_end]).to_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .map(|v| v.trim().parse().unwrap())
                    .unwrap_or(0);
                while raw.len() < header_end + length {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    raw.extend_from_slice(&buffer[..n]);
                }
                seen.push(String::from_utf8_lossy(&raw).into_owned());
                socket.write_all(response.as_bytes()).unwrap();
            }
            seen
        });
        (format!("http://{address}"), handle)
    }
    fn reply(status: &str, headers: &[&str], body: &str) -> String {
        let mut out = format!("HTTP/1.1 {status}\r\nConnection: close\r\n");
        for h in headers {
            out.push_str(h);
            out.push_str("\r\n");
        }
        out.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
        out
    }

    #[test]
    fn session_logs_in_retries_after_401_and_saves_multipart() {
        let tokens =
            |t: &str, r: &str| json!({"token": t, "refresh": r, "expires": 1800}).to_string();
        let json_type = "Content-Type: application/json";
        let (base, server) = fake_server(vec![
            reply("200 OK", &[json_type], &tokens("t1", "r1")),
            reply(
                "200 OK",
                &[
                    "Set-Cookie: sid=s1; path=/; HttpOnly",
                    "Set-Cookie: other=x",
                ],
                "{}",
            ),
            reply("401 Unauthorized", &[], ""),
            reply("200 OK", &[json_type], &tokens("t2", "r2")),
            reply("200 OK", &["Set-Cookie: sid=s2; Secure"], "{}"),
            reply("200 OK", &["Content-Type: text/html; charset=utf-8"], PAGE),
            reply(
                "302 Found",
                &["Location: /timesheet/wypelnij?dzien=2026-10-15"],
                "",
            ),
        ]);
        let c = Config {
            login: "jan".into(),
            device_id: "0123456789abcdef0123456789abcdef".into(),
            ..Config::default()
        };
        let api = Api::new(&base, &c, "tajne-haslo".into()).unwrap();
        let mut session = Session::default();
        let html = tauri::async_runtime::block_on(async {
            let html = api
                .with_auth(&mut session, |s| {
                    let api = &api;
                    async move { api.fill_page(&s, "2026-10-15").await }
                })
                .await
                .unwrap();
            api.save_day(
                &session,
                "2026-10-15",
                &[entry("101", "1,50", "Opis; żółć")],
            )
            .await
            .unwrap();
            html
        });
        assert_eq!(parse_filled(&html).unwrap().len(), 2);
        assert_eq!(session.token, "t2");
        assert_eq!(session.refresh, "r2");
        assert_eq!(session.cookie, "sid=s2");
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 7);
        assert!(requests[0].starts_with("POST /api/login "));
        let body = &requests[0][requests[0].find("\r\n\r\n").unwrap() + 4..];
        assert_eq!(
            serde_json::from_str::<Value>(body).unwrap(),
            json!({"login":"jan","password":"tajne-haslo","deviceId":"0123456789abcdef0123456789abcdef","deviceName":"Todo Track"})
        );
        assert!(requests[1].starts_with("GET /api/user/identity "));
        assert!(requests[1].contains("authorization: Bearer t1"));
        assert!(requests[2].starts_with("GET /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(requests[2].contains("authorization: Bearer t1"));
        assert!(requests[2].contains("cookie: sid=s1"));
        assert!(requests[3].starts_with("GET /api/token/refresh?id=api-ts&name=api-ts&refresh=r1 "));
        assert!(requests[4].starts_with("GET /api/user/identity "));
        assert!(requests[4].contains("authorization: Bearer t2"));
        assert!(requests[5].contains("authorization: Bearer t2"));
        assert!(requests[5].contains("cookie: sid=s2"));
        let post = &requests[6];
        assert!(post.starts_with("POST /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(post.contains("content-type: multipart/form-data; boundary="));
        assert!(post.contains("authorization: Bearer t2"));
        assert!(post.contains("cookie: sid=s2"));
        let positions: Vec<usize> = [
            "name=\"akcja\"\r\n\r\nzapisz",
            "name=\"dzien\"\r\n\r\n2026-10-15",
            "name=\"status\"\r\n\r\n0",
            "name=\"submit\"\r\n\r\nZapisz Timesheet",
            "name=\"zadanie[0]\"\r\n\r\n101",
            "name=\"zadanie_godziny[0]\"\r\n\r\n1,50",
            "name=\"zadanie_nadgodziny[0]\"\r\n\r\n0",
            "name=\"zadanie_opis[0]\"\r\n\r\nOpis; żółć",
        ]
        .iter()
        .map(|part| post.find(part).unwrap_or_else(|| panic!("brak {part}")))
        .collect();
        assert!(positions.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn login_without_cookie_and_rejected_refresh() {
        let (base, server) = fake_server(vec![
            reply(
                "200 OK",
                &["Content-Type: application/json"],
                r#"{"token":"t1","refresh":"r1","expires":1800}"#,
            ),
            reply("200 OK", &[], "{}"),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = Session::default();
        let error = tauri::async_runtime::block_on(api.login(&mut session)).unwrap_err();
        assert_eq!(error, TsError::Message(NO_COOKIE.into()));
        assert!(session.token.is_empty());
        server.join().unwrap();

        // Odrzucony refresh (401) → pełne logowanie hasłem.
        let (base, server) = fake_server(vec![
            reply("401 Unauthorized", &[], ""),
            reply(
                "200 OK",
                &["Content-Type: application/json"],
                r#"{"token":"t9","refresh":"r9"}"#,
            ),
            reply("200 OK", &["Set-Cookie: sid=s9; path=/"], "{}"),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = Session {
            owner: api.owner(),
            token: "stary".into(),
            refresh: "r0".into(),
            expires_at: 0,
            cookie: "sid=s0".into(),
        };
        tauri::async_runtime::block_on(api.ensure_fresh(&mut session)).unwrap();
        assert_eq!(session.token, "t9");
        assert_eq!(session.cookie, "sid=s9");
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("GET /api/token/refresh?"));
        assert!(requests[1].starts_with("POST /api/login "));

        // Odrzucone logowanie → komunikat bez hasła.
        let (base, server) = fake_server(vec![reply("401 Unauthorized", &[], "")]);
        let api = Api::new(&base, &Config::default(), "sekret-123".into()).unwrap();
        let message = tauri::async_runtime::block_on(api.login(&mut Session::default()))
            .unwrap_err()
            .text();
        assert_eq!(message, "TS odrzucił logowanie. Sprawdź login i hasło.");
        assert!(!message.contains("sekret-123"));
        server.join().unwrap();
    }

    #[test]
    fn save_redirect_is_accepted_only_back_to_the_form() {
        assert!(save_redirect_ok(None));
        assert!(save_redirect_ok(Some("")));
        assert!(save_redirect_ok(Some(
            "/timesheet/wypelnij?dzien=2026-10-15"
        )));
        assert!(save_redirect_ok(Some(
            "https://api.ts.ideo.pl/timesheet/wypelnij/?dzien=2026-10-15"
        )));
        assert!(!save_redirect_ok(Some("/login")));
        assert!(!save_redirect_ok(Some(
            "https://api.ts.ideo.pl/logowanie?powrot=/timesheet/wypelnij"
        )));
    }

    #[test]
    fn read_back_confirms_or_warns() {
        let sent = vec![entry("101", "1,50", "a"), entry("102", "0", "b")];
        let row = |id: &str, hours: &str| Filled {
            id: id.into(),
            hours: hours.into(),
            overtime: "0".into(),
            description: String::new(),
        };
        // Wiersz z 0 h może zniknąć; wiersz z godzinami musi być w odczycie.
        assert!(confirmed(&sent, &[row(" 101 ", "1.5")]));
        assert!(!confirmed(&sent, &[row("102", "0")]));
        assert!(!confirmed(&sent, &[]));

        let (filled, hours, warning) =
            sent_outcome(&sent, Ok(vec![row("101", "1,5"), row("900", "2")])).unwrap();
        assert_eq!(filled.len(), 2);
        assert_eq!(hours, "3,50");
        assert_eq!(warning, None);

        assert_eq!(
            sent_outcome(&sent, Ok(vec![row("900", "8")])).unwrap_err(),
            NOT_CONFIRMED
        );

        let (filled, hours, warning) = sent_outcome(&sent, Err(NETWORK_ERROR.into())).unwrap();
        assert!(filled.is_empty());
        assert_eq!(hours, "1,50");
        assert_eq!(warning.as_deref(), Some(READ_BACK_WARNING));
        assert_eq!(
            sent_total(&[entry("1", "1.25", ""), entry("2", "2,5", "")]),
            "3,75"
        );
    }

    fn with_overtime(mut e: Entry, overtime: &str) -> Entry {
        e.overtime = overtime.into();
        e
    }
    /// Strona formularza dnia z podanymi wierszami (id, godziny, nadgodziny, opis).
    fn day_page(rows: &[(&str, &str, &str, &str)]) -> String {
        let body: String = rows
            .iter()
            .enumerate()
            .map(|(i, (id, hours, overtime, description))| {
                format!(
                    r#"<tr class="jq-wiersz-ts">
<td class="jq-zadanie"><select name="zadanie[{i}]"><optgroup label="P"><option value="{id}" selected>Z</option></optgroup></select></td>
<td class="jq-zadanie-godziny"><input name="zadanie_godziny[{i}]" value="{hours}"></td>
<td class="jq-zadanie-nadgodziny"><input name="zadanie_nadgodziny[{i}]" value="{overtime}"></td>
<td class="jq-zadanie-opis"><input name="zadanie_opis[{i}]" value="{description}"></td>
</tr>"#
                )
            })
            .collect();
        format!("<html><body><form><table><tbody>{body}</tbody></table></form></body></html>")
    }

    #[test]
    fn overtime_field_constant_form_parser_and_totals() {
        assert_eq!(FIELD_OVERTIME, "zadanie_nadgodziny");
        assert_eq!(row_input(FIELD_OVERTIME), ".jq-zadanie-nadgodziny input");
        let fields = form_fields(
            "2026-10-15",
            &[with_overtime(entry("101", "6,00", ""), "1,25")],
        );
        assert!(fields.contains(&("zadanie_nadgodziny[0]".into(), "1,25".into())));
        let filled = parse_filled(&day_page(&[
            ("101", "6,00", "1,25", "a"),
            ("205", "0.5", "0.75", ""),
        ]))
        .unwrap();
        assert_eq!(filled[0].overtime, "1,25");
        assert_eq!(hours_total(&filled), "6,50");
        assert_eq!(overtime_total(&filled), "2,00");
        assert_eq!(work_total(&filled), "8,50");
        // Wpis z samymi nadgodzinami (0 h) też musi być w odczycie po zapisie.
        let only_overtime = [with_overtime(entry("300", "0", ""), "2")];
        assert!(!confirmed(&only_overtime, &filled));
        assert!(confirmed(
            &only_overtime,
            &parse_filled(&day_page(&[("300", "0", "2", "")])).unwrap()
        ));
        // Znacznik wysyłki i suma bez odczytu liczą godziny razem z nadgodzinami.
        let (_, hours, _) = sent_outcome(&only_overtime, Err(NETWORK_ERROR.into())).unwrap();
        assert_eq!(hours, "2,00");
        let sent = [
            with_overtime(entry("101", "6,00", "a"), "1,25"),
            with_overtime(entry("205", "0,50", ""), "0,75"),
        ];
        let (_, hours, _) = sent_outcome(&sent, Ok(filled.clone())).unwrap();
        assert_eq!(hours, "8,50");
    }

    #[test]
    fn saved_comparison_counts_hours_and_overtime() {
        let sent = [
            with_overtime(entry("101", "6,00", ""), "1,25"),
            entry("205", "0,5", ""),
        ];
        let read = |overtime: &str| {
            parse_filled(&day_page(&[
                ("101", "6", overtime, ""),
                ("205", "0.50", "0", ""),
            ]))
            .unwrap()
        };
        assert!(!saved_differs(&sent, &read("1.25")));
        // Inne nadgodziny (różnica > 0,01 h) to rozjazd; 0,01 h mieści się w zaokrągleniu.
        assert!(saved_differs(&sent, &read("0")));
        assert!(!saved_differs(&sent, &read("1,24")));
        // Inna liczba wpisów.
        assert!(saved_differs(&sent[..1], &read("1,25")));
    }

    #[test]
    fn keep_compares_with_entries_actually_sent_after_ts_changed_since_preview() {
        // Podgląd otwarty przy dniu z PAGE (101, 205); przed zapisem ktoś dopisał
        // w TS wpis 300. `keep` dokleja wpisy z aktualnej strony, a porównanie
        // idzie z listą faktycznie wysłaną, więc nie ma fałszywego rozjazdu.
        let at_save = day_page(&[
            ("101", "1,5", "0", "Analiza"),
            ("205", "0.25", "0", "Daily"),
            ("300", "1", "0", "Dopisane w TS"),
        ]);
        let after = day_page(&[
            ("102", "6,00", "1,50", "Programowanie"),
            ("101", "1,5", "0", "Analiza"),
            ("205", "0.25", "0", "Daily"),
            ("300", "1", "0", "Dopisane w TS"),
        ]);
        let html = "Content-Type: text/html; charset=utf-8";
        let (base, server) = fake_server(vec![
            reply("200 OK", &[html], &at_save),
            reply(
                "302 Found",
                &["Location: /timesheet/wypelnij?dzien=2026-10-15"],
                "",
            ),
            reply("200 OK", &[html], &after),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = live_session(&api);
        let ours = vec![with_overtime(entry("102", "6,00", "Programowanie"), "1,50")];
        let (sent, read) = tauri::async_runtime::block_on(api.send_day(
            &mut session,
            "2026-10-15",
            ours.clone(),
            Mode::Keep,
        ))
        .unwrap();
        let read = read.unwrap();
        assert_eq!(
            sent.iter().map(|e| e.task_id.as_str()).collect::<Vec<_>>(),
            ["102", "101", "205", "300"]
        );
        assert!(!saved_differs(&sent, &read));
        // Porównanie z listą zbudowaną z dnia przy otwarciu podglądu dałoby rozjazd.
        assert!(saved_differs(
            &merge_keep(&ours, &parse_filled(PAGE).unwrap()),
            &read
        ));
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 3);
        assert!(requests[0].starts_with("GET /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(requests[1].starts_with("POST /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(requests[1].contains("name=\"zadanie_nadgodziny[0]\"\r\n\r\n1,50"));
        assert!(requests[1].contains("name=\"zadanie[3]\"\r\n\r\n300"));
        assert!(requests[2].starts_with("GET /timesheet/wypelnij?dzien=2026-10-15 "));

        // `replace`: bez odczytu przed zapisem, wysłana lista = nasze wpisy.
        let (base, server) = fake_server(vec![
            reply("200 OK", &[], ""),
            reply("200 OK", &[html], &day_page(&[("102", "6,00", "1,50", "")])),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = live_session(&api);
        let (sent, read) = tauri::async_runtime::block_on(api.send_day(
            &mut session,
            "2026-10-15",
            ours.clone(),
            Mode::Replace,
        ))
        .unwrap();
        assert_eq!(sent, ours);
        assert!(!saved_differs(&sent, &read.unwrap()));
        let requests = server.join().unwrap();
        assert!(requests[0].starts_with("POST "));
    }

    fn live_session(api: &Api) -> Session {
        Session {
            owner: api.owner(),
            token: "t0".into(),
            refresh: "r0".into(),
            expires_at: crate::db::now() + 3_600_000,
            cookie: "sid=s0".into(),
        }
    }

    #[test]
    fn save_redirected_to_login_logs_in_again_and_repeats_once() {
        let (base, server) = fake_server(vec![
            reply("302 Found", &["Location: /login"], ""),
            reply(
                "200 OK",
                &["Content-Type: application/json"],
                r#"{"token":"t1","refresh":"r1","expires":1800}"#,
            ),
            reply("200 OK", &["Set-Cookie: sid=s1; path=/"], "{}"),
            reply(
                "302 Found",
                &["Location: /timesheet/wypelnij?dzien=2026-10-15"],
                "",
            ),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = live_session(&api);
        let entries = [entry("101", "1,50", "Opis")];
        tauri::async_runtime::block_on(api.with_auth(&mut session, |s| {
            let api = &api;
            let entries = &entries;
            async move { api.save_day(&s, "2026-10-15", entries).await }
        }))
        .unwrap();
        assert_eq!(session.token, "t1");
        assert_eq!(session.cookie, "sid=s1");
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests[0].starts_with("POST /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(requests[0].contains("cookie: sid=s0"));
        assert!(requests[1].starts_with("POST /api/login "));
        assert!(requests[2].starts_with("GET /api/user/identity "));
        assert!(requests[3].starts_with("POST /timesheet/wypelnij?dzien=2026-10-15 "));
        assert!(requests[3].contains("cookie: sid=s1"));

        // Druga odmowa po ponownym logowaniu → błąd, bez kolejnych powtórek.
        let (base, server) = fake_server(vec![
            reply("302 Found", &["Location: /login"], ""),
            reply(
                "200 OK",
                &["Content-Type: application/json"],
                r#"{"token":"t1","refresh":"r1"}"#,
            ),
            reply("200 OK", &["Set-Cookie: sid=s1"], "{}"),
            reply("302 Found", &["Location: /login"], ""),
        ]);
        let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
        let mut session = live_session(&api);
        let error = tauri::async_runtime::block_on(api.with_auth(&mut session, |s| {
            let api = &api;
            let entries = &entries;
            async move { api.save_day(&s, "2026-10-15", entries).await }
        }))
        .unwrap_err();
        assert_eq!(error, TsError::Status(StatusCode::FOUND));
        assert_eq!(server.join().unwrap().len(), 4);
    }

    #[test]
    fn rejected_identity_after_refresh_falls_back_to_login() {
        for rejected in [
            reply("401 Unauthorized", &[], ""),
            reply("302 Found", &["Location: /login"], ""),
        ] {
            let (base, server) = fake_server(vec![
                reply(
                    "200 OK",
                    &["Content-Type: application/json"],
                    r#"{"token":"t1","refresh":"r1"}"#,
                ),
                rejected,
                reply(
                    "200 OK",
                    &["Content-Type: application/json"],
                    r#"{"token":"t2","refresh":"r2"}"#,
                ),
                reply("200 OK", &["Set-Cookie: sid=s2"], "{}"),
            ]);
            let api = Api::new(&base, &Config::default(), "x".into()).unwrap();
            let mut session = Session {
                expires_at: 0,
                ..live_session(&api)
            };
            tauri::async_runtime::block_on(api.ensure_fresh(&mut session)).unwrap();
            assert_eq!(session.token, "t2");
            assert_eq!(session.cookie, "sid=s2");
            let requests = server.join().unwrap();
            assert!(requests[0].starts_with("GET /api/token/refresh?"));
            assert!(requests[1].starts_with("GET /api/user/identity "));
            assert!(requests[2].starts_with("POST /api/login "));
            assert!(requests[3].starts_with("GET /api/user/identity "));
        }
    }
}
