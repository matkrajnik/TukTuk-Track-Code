//! Redmine obok Jiry (PLAN_1.9.md, R1): konfiguracja w `settings` pod kluczem
//! `redmine`, klucz API w magazynie poświadczeń Windows, klient HTTP, lista
//! zgłoszeń, projekty, statusy i powiązanie zgłoszenia z zadaniem
//! (`redmine_tasks`). Moduł jest niezależny od `jira.rs`: własne połączenie,
//! poświadczenie, stan i tabele. Klucz API nigdy nie trafia do SQLite, do
//! komunikatów ani do logów.

use crate::{db, jira_fields, Database};
use keyring::Entry;
use reqwest::{Client, StatusCode, Url};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::Mutex,
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

/// Śledzony projekt (odpowiednik `jira::Board`), wybierany w Ustawieniach (R5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub identifier: String,
    #[serde(default)]
    pub parent_id: Option<u64>,
}
/// Konfiguracja połączenia (bez klucza API). Brak domyślnego adresu: adres
/// firmowego Redmine nie jest znany.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub site: String,
    pub login: String,
    pub user_id: u64,
    pub display_name: String,
    pub accept_invalid_cert: bool,
    pub projects: Vec<Project>,
}
#[derive(Serialize)]
pub struct Settings {
    config: Config,
    connected: bool,
}
/// Zgłoszenie z `/issues.json` (pola na kartę widoku i kafelek zadania).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub id: u64,
    pub subject: String,
    pub project_id: u64,
    pub project: String,
    pub tracker: String,
    pub status: String,
    pub status_id: u64,
    pub closed: bool,
    pub priority: String,
    pub assigned_to_id: Option<u64>,
    pub assigned_to: String,
    /// Wersja docelowa (`fixed_version.name`).
    pub version: String,
    pub category: String,
    pub due_date: String,
    pub updated_on: String,
    pub url: String,
}
/// Status z `/issue_statuses.json` (pamięć podręczna w `redmine_statuses`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: u64,
    pub name: String,
    pub is_closed: bool,
}
/// Grupa statusu w widoku Redmine (R3): numery zgłoszeń w kolejności
/// wyświetlania (pasmo priorytetu, potem termin).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusGroup {
    pub status: String,
    pub closed: bool,
    pub ids: Vec<u64>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issues {
    issues: Vec<Issue>,
    warnings: Vec<String>,
    /// Zgłoszenia pogrupowane według statusu w kolejności z `redmine_statuses`.
    groups: Vec<StatusGroup>,
    /// Projekty śledzone razem z ich podprojektami (znanymi z listy projektów):
    /// zgłoszenia spoza nich trafiają do sekcji „spoza śledzonych projektów”.
    tracked: Vec<u64>,
}
/// Ostatnio pobrane zgłoszenia (tytuł do szybkiego dodawania i „Rozpocznij”)
/// oraz lista projektów do wyboru w Ustawieniach — bez tabel.
pub struct RedmineState {
    issues: Mutex<HashMap<u64, Issue>>,
    projects: Mutex<Vec<Project>>,
}

pub const NOT_CONNECTED: &str = "Połącz konto Redmine: podaj adres i klucz API.";
const NETWORK_ERROR: &str =
    "Nie udało się połączyć z Redmine. Sprawdź adres i połączenie z siecią firmową.";
pub const CERT_ERROR: &str = "Certyfikat serwera nie jest zaufany. Zainstaluj certyfikat firmowego urzędu w Windows albo włącz opcję w „Zaawansowane”.";
const HTML_ERROR: &str =
    "Redmine odpowiedział stroną zamiast danymi. Sprawdź, czy REST API jest włączone.";
const BAD_RESPONSE: &str = "Redmine zwrócił niepoprawną odpowiedź.";
const SITE_ERROR: &str = "Podaj adres Redmine, np. https://redmine.firma.pl albo https://firma.pl/redmine (bez parametrów).";
const KEY_ERROR: &str = "Podaj klucz API Redmine (1–128 znaków, bez spacji).";
const REFRESH_FIRST: &str = "Odśwież listę zgłoszeń przed rozpoczęciem.";
const BAD_PROJECT: &str = "Niepoprawny projekt Redmine.";
/// Maksimum Redmine na stronę.
const PAGE: u64 = 100;
/// Najwyżej 5 stron (500 zgłoszeń) z ostrzeżeniem, jak limit 300 na tablicy Jiry.
const MAX_ISSUE_PAGES: usize = 5;
const MAX_PROJECT_PAGES: usize = 10;
/// Statusy odświeżane raz na dobę (poza „Odśwież” i pierwszym użyciem).
const STATUS_TTL: i64 = 24 * 60 * 60_000;
const SUBJECT_LIMIT: usize = 1000;

pub fn setup(app: &tauri::AppHandle) {
    app.manage(RedmineState {
        issues: Mutex::new(HashMap::new()),
        projects: Mutex::new(Vec::new()),
    });
}
/// Temat zgłoszenia z ostatnio pobranej listy, jak `jira::cached_summary`
/// (szybkie dodawanie `#1234`, R2).
pub fn cached_subject(app: &tauri::AppHandle, id: u64) -> Option<String> {
    app.state::<RedmineState>()
        .issues
        .lock()
        .ok()?
        .get(&id)
        .map(|issue| issue.subject.clone())
}
fn clear_state(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<RedmineState>();
    state.issues.lock().map_err(|e| e.to_string())?.clear();
    state.projects.lock().map_err(|e| e.to_string())?.clear();
    Ok(())
}

// ---------------------------------------------------------------------------
// Konfiguracja, magazyn poświadczeń, walidacja

fn read_config(db: &Connection) -> Result<Config, String> {
    let raw: Option<String> = db
        .query_row("SELECT value FROM settings WHERE key='redmine'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|s| {
        serde_json::from_str(&s).map_err(|_| "Nie można odczytać ustawień Redmine.".to_owned())
    })
    .unwrap_or_else(|| Ok(Config::default()))
}
fn write_config(db: &Connection, config: &Config) -> Result<(), String> {
    let raw = serde_json::to_string(config).map_err(|e| e.to_string())?;
    db.execute(
        "INSERT INTO settings(key,value) VALUES('redmine',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
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
/// Wpis w magazynie poświadczeń Windows: usługa `<identifier>.redmine`,
/// użytkownik `<adres>|<login>`.
fn entry(app: &tauri::AppHandle, c: &Config) -> Result<Entry, String> {
    Entry::new(
        &format!("{}.redmine", app.config().identifier),
        &format!("{}|{}", c.site, c.login),
    )
    .map_err(|_| "Nie można otworzyć magazynu poświadczeń Windows.".into())
}
pub(crate) fn api_key(app: &tauri::AppHandle, c: &Config) -> Result<String, String> {
    if c.login.is_empty() || c.site.is_empty() {
        return Err(NOT_CONNECTED.into());
    }
    entry(app, c)?
        .get_password()
        .map_err(|_| NOT_CONNECTED.into())
}
fn connected(app: &tauri::AppHandle, c: &Config) -> Result<bool, String> {
    if c.login.is_empty() || c.site.is_empty() {
        return Ok(false);
    }
    match entry(app, c)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(_) => Err("Nie można odczytać klucza API z magazynu poświadczeń Windows.".into()),
    }
}
/// Adres Redmine: `http` albo `https`, host wymagany, dozwolony prefiks ścieżki
/// (np. `/redmine`) bez końcowego `/`; parametry, fragment i dane logowania
/// odrzucane. Port jest dozwolony (Redmine w sieci wewnętrznej bywa na porcie).
pub fn normalize_site(site: &str) -> Result<String, String> {
    let url = Url::parse(site.trim()).map_err(|_| SITE_ERROR.to_owned())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.cannot_be_a_base()
        || url.host_str().is_none_or(str::is_empty)
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(SITE_ERROR.into());
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
/// Numer zgłoszenia: od 1 do 10 cyfr.
pub fn valid_issue_id(id: u64) -> bool {
    (1..=9_999_999_999).contains(&id)
}
/// Klucz API: 1–128 znaków ASCII bez białych znaków (nagłówek HTTP).
pub fn valid_api_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && key.bytes().all(|b| b.is_ascii_graphic())
}
/// Link, który aplikacja otwiera w przeglądarce: zgłoszenie
/// (`<site>/issues/1234`) albo „Moje konto” (`<site>/my/account`) pod hostem
/// i prefiksem ścieżki skonfigurowanego adresu, bez parametrów, fragmentu i
/// danych logowania.
pub fn allowed_link(parsed: &Url, site: &str) -> bool {
    let Ok(base) = Url::parse(site) else {
        return false;
    };
    if parsed.scheme() != base.scheme()
        || parsed.host_str().is_none()
        || parsed.host_str() != base.host_str()
        || parsed.port_or_known_default() != base.port_or_known_default()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return false;
    }
    let prefix = base.path().trim_end_matches('/');
    let Some(rest) = parsed.path().strip_prefix(prefix) else {
        return false;
    };
    rest == "/my/account"
        || rest.strip_prefix("/issues/").is_some_and(|n| {
            !n.is_empty()
                && n.len() <= 10
                && n.bytes().all(|b| b.is_ascii_digit())
                && n.parse::<u64>().is_ok_and(valid_issue_id)
        })
}
/// Link „Gdzie znaleźć klucz API” z Ustawień przed połączeniem: `/my/account`
/// pod dowolnym poprawnym adresem Redmine wpisanym przez użytkownika.
fn account_link(parsed: &Url) -> bool {
    let Some(prefix) = parsed.path().strip_suffix("/my/account") else {
        return false;
    };
    parsed.query().is_none()
        && parsed.fragment().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.host_str().is_some_and(|host| {
            normalize_site(&format!("{}://{host}{prefix}", parsed.scheme())).is_ok()
        })
}
/// Host, jawny port i prefiks ścieżki adresu Redmine (bez schematu: ten sam
/// Redmine bywa otwierany przez `http` i `https`).
fn site_parts(url: &Url) -> Option<(String, Option<u16>, Vec<&str>)> {
    let host = url.host_str().filter(|h| !h.is_empty())?;
    let path = url
        .path_segments()
        .map(|s| s.filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    Some((
        host.trim_end_matches('.').to_ascii_lowercase(),
        url.port(),
        path,
    ))
}
/// Czy dwa adresy wskazują ten sam Redmine: host, port i prefiks ścieżki
/// (schemat bez znaczenia). Pusty albo niepoprawny adres nie pasuje do niczego.
pub fn same_site(a: &str, b: &str) -> bool {
    let (Ok(a), Ok(b)) = (Url::parse(a), Url::parse(b)) else {
        return false;
    };
    matches!((site_parts(&a), site_parts(&b)), (Some(x), Some(y)) if x == y)
}
/// Numer zgłoszenia ze strony Redmine pod adresem `site`: tylko ścieżka
/// `<prefiks>/issues/<numer>[/...]` na tym samym hoście i porcie (PLAN_1.9.md,
/// R3, rozszerzenie przeglądarki). Inne strony tego hosta (np. GitLab pod
/// `/gitlab/.../issues/42`) nie są zgłoszeniami Redmine.
pub fn issue_page(url: &Url, site: &str) -> Option<u64> {
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let base = Url::parse(site).ok()?;
    let (host, port, prefix) = site_parts(&base)?;
    let (page_host, page_port, path) = site_parts(url)?;
    if page_host != host || page_port != port || !path.starts_with(&prefix) {
        return None;
    }
    match &path[prefix.len()..] {
        ["issues", number, ..]
            if !number.is_empty()
                && number.len() <= 10
                && number.bytes().all(|b| b.is_ascii_digit()) =>
        {
            number.parse::<u64>().ok().filter(|id| valid_issue_id(*id))
        }
        _ => None,
    }
}
/// Skonfigurowany adres Redmine z bazy (pusty bez konfiguracji): dopasowanie
/// `#1234` z rozszerzenia przeglądarki tylko do zgłoszeń tego Redmine.
pub fn current_site(db: &Connection) -> String {
    read_config(db).map(|c| c.site).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Klient HTTP i komunikaty

/// Klient per konfiguracja: 30 s na żądanie, bez przekierowań, zaufanie do
/// certyfikatów z magazynu Windows (`rustls-tls-native-roots`), a
/// `danger_accept_invalid_certs` tylko przy włączonej opcji w Ustawieniach.
pub(crate) fn client(c: &Config) -> Result<Client, String> {
    let mut builder = Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none());
    if c.accept_invalid_cert {
        builder = builder.danger_accept_invalid_certs(true);
    }
    // Testy rozmawiają wyłącznie z lokalnym serwerem na 127.0.0.1, bez proxy
    // systemowego, żeby żadne żądanie nie opuściło maszyny.
    #[cfg(test)]
    let builder = builder.no_proxy();
    builder
        .build()
        .map_err(|_| "Nie udało się utworzyć połączenia z Redmine.".into())
}
/// Czy w łańcuchu przyczyn błędu jest błąd certyfikatu TLS (rustls:
/// `InvalidCertificate(UnknownIssuer)`, „invalid peer certificate” itp.).
fn certificate_in_chain(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut current: Option<&(dyn std::error::Error + 'static)> = Some(error);
    while let Some(e) = current {
        let text = format!("{e} {e:?}").to_ascii_lowercase();
        if text.contains("certificate") || text.contains("unknownissuer") {
            return true;
        }
        current = e.source();
    }
    false
}
/// Komunikat dla błędu sieci; treść `reqwest::Error` (z adresem) nigdy nie trafia
/// do komunikatu, tylko rozpoznanie błędu certyfikatu.
fn network_error(error: &reqwest::Error) -> String {
    if certificate_in_chain(error) {
        CERT_ERROR.into()
    } else {
        NETWORK_ERROR.into()
    }
}
/// Treść `errors` z odpowiedzi 422 (`{"errors":["Subject cannot be blank"]}`),
/// najwyżej 200 znaków; odpowiedź bez JSON daje pusty tekst.
fn error_details(body: &str) -> String {
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let parts: Vec<String> = match &value["errors"] {
        Value::Array(list) => list
            .iter()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect(),
        Value::String(s) => vec![s.clone()],
        Value::Object(map) => map
            .values()
            .filter_map(|m| m.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    };
    parts
        .join("; ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(200)
        .collect()
}
/// Polskie komunikaty błędów HTTP Redmine. Nigdy nie zawierają klucza API.
pub fn status_error(status: StatusCode, body: &str) -> String {
    match status {
        StatusCode::UNAUTHORIZED => {
            "Redmine odrzucił klucz API. Sprawdź klucz w „Moje konto” w Redmine.".into()
        }
        StatusCode::FORBIDDEN => "Brak uprawnień w Redmine do tej operacji.".into(),
        StatusCode::NOT_FOUND => {
            "Nie znaleziono zgłoszenia albo adres Redmine jest błędny (REST API może być wyłączone)."
                .into()
        }
        StatusCode::UNPROCESSABLE_ENTITY => {
            let details = error_details(body);
            if details.is_empty() {
                "Redmine odrzucił dane.".into()
            } else {
                format!("Redmine odrzucił dane: {details}")
            }
        }
        StatusCode::TOO_MANY_REQUESTS => {
            "Redmine ograniczył liczbę zapytań. Spróbuj ponownie za chwilę.".into()
        }
        s if s.is_redirection() => {
            "Redmine przekierował żądanie. Sprawdź adres (http czy https, prefiks ścieżki)."
                .into()
        }
        s if s.is_server_error() => format!(
            "Redmine zwrócił błąd HTTP {}. Spróbuj ponownie.",
            status.as_u16()
        ),
        _ => format!("Redmine zwrócił błąd HTTP {}.", status.as_u16()),
    }
}
fn looks_like_html(text: &str) -> bool {
    let head: String = text
        .trim_start()
        .chars()
        .take(64)
        .collect::<String>()
        .to_ascii_lowercase();
    head.starts_with("<!doctype")
        || head.starts_with("<html")
        || head.starts_with("<head")
        || head.starts_with("<?xml")
}
/// Odpowiedź Redmine: błąd HTTP po polsku, JSON przy powodzeniu; strona HTML
/// (np. logowanie przy wyłączonym API) ma osobny komunikat.
pub(crate) fn parse_response(status: StatusCode, html: bool, text: &str) -> Result<Value, String> {
    if !status.is_success() {
        return Err(status_error(status, text));
    }
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() || value.is_array() => Ok(value),
        _ if html || looks_like_html(text) => Err(HTML_ERROR.into()),
        _ => Err(BAD_RESPONSE.into()),
    }
}
/// `GET <site><path>?query` z nagłówkami `X-Redmine-API-Key` i `Accept: application/json`.
pub(crate) async fn get(
    client: &Client,
    c: &Config,
    key: &str,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, String> {
    // W testach dozwolony jest tylko lokalny serwer, nigdy prawdziwy Redmine.
    #[cfg(test)]
    assert!(
        c.site.starts_with("http://127.0.0.1:"),
        "testy nie mogą łączyć się z serwerem Redmine"
    );
    let response = client
        .get(format!("{}{}", c.site, path))
        .header("X-Redmine-API-Key", key)
        .header("Accept", "application/json")
        .query(query)
        .send()
        .await
        .map_err(|e| network_error(&e))?;
    let status = response.status();
    let html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/html"));
    let text = response.text().await.unwrap_or_default();
    parse_response(status, html, &text)
}

// ---------------------------------------------------------------------------
// Odczyt odpowiedzi

/// Wynik stronicowania `limit`/`offset`/`total_count`: wiersze listy, łączna
/// liczba według serwera i czy pobrano wszystko (limit stron).
#[derive(Debug)]
pub(crate) struct Paged {
    pub rows: Vec<Value>,
    pub total: u64,
    pub complete: bool,
}
/// Pobiera kolejne strony (`offset` 0, 100, 200…) przez `fetch`, najwyżej
/// `max_pages`. Koniec po pustej stronie, po dojściu do `total_count` albo —
/// bez `total_count` — po niepełnej stronie.
pub(crate) async fn paged<F, Fut>(
    key: &str,
    missing: &str,
    max_pages: usize,
    mut fetch: F,
) -> Result<Paged, String>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let mut rows = Vec::new();
    let mut offset = 0u64;
    let mut total = 0u64;
    for _ in 0..max_pages {
        let value = fetch(offset).await?;
        let items = value[key].as_array().ok_or_else(|| missing.to_owned())?;
        let count = items.len() as u64;
        rows.extend(items.iter().cloned());
        offset += count;
        total = value["total_count"].as_u64().unwrap_or(offset).max(offset);
        let done = count == 0
            || value["total_count"]
                .as_u64()
                .map_or(count < PAGE, |t| offset >= t);
        if done {
            return Ok(Paged {
                rows,
                total,
                complete: true,
            });
        }
    }
    Ok(Paged {
        rows,
        total,
        complete: false,
    })
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").trim().to_owned()
}
fn name(value: &Value) -> String {
    text(&value["name"])
}
/// Jedno zgłoszenie z odpowiedzi; `closed` to mapa statusów z
/// `/issue_statuses.json` na wypadek, gdy serwer nie oddaje `status.is_closed`.
pub(crate) fn parse_issue(value: &Value, site: &str, closed: &HashMap<u64, bool>) -> Option<Issue> {
    let id = value["id"].as_u64().filter(|id| valid_issue_id(*id))?;
    let status_id = value["status"]["id"].as_u64().unwrap_or(0);
    let is_closed = value["status"]["is_closed"]
        .as_bool()
        .or_else(|| closed.get(&status_id).copied())
        .unwrap_or(false);
    let mut subject: String = text(&value["subject"])
        .chars()
        .take(SUBJECT_LIMIT)
        .collect();
    if subject.is_empty() {
        subject = format!("#{id}");
    }
    Some(Issue {
        id,
        subject,
        project_id: value["project"]["id"].as_u64().unwrap_or(0),
        project: name(&value["project"]),
        tracker: name(&value["tracker"]),
        status: name(&value["status"]),
        status_id,
        closed: is_closed,
        priority: name(&value["priority"]),
        assigned_to_id: value["assigned_to"]["id"].as_u64(),
        assigned_to: name(&value["assigned_to"]),
        version: name(&value["fixed_version"]),
        category: name(&value["category"]),
        due_date: text(&value["due_date"]),
        updated_on: text(&value["updated_on"]),
        url: format!("{site}/issues/{id}"),
    })
}
/// Zgłoszenia z listy (`issues`), bez powtórzeń i bez wierszy z niepoprawnym numerem.
pub(crate) fn parse_issues(items: &[Value], site: &str, closed: &HashMap<u64, bool>) -> Vec<Issue> {
    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(|v| parse_issue(v, site, closed))
        .filter(|i| seen.insert(i.id))
        .collect()
}
pub(crate) fn parse_projects(items: &[Value]) -> Vec<Project> {
    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(|v| {
            let id = v["id"].as_u64().filter(|id| *id > 0)?;
            let name = name(v);
            if name.is_empty() || !seen.insert(id) {
                return None;
            }
            Some(Project {
                id,
                name,
                identifier: text(&v["identifier"]),
                parent_id: v["parent"]["id"].as_u64(),
            })
        })
        .collect()
}
pub(crate) fn parse_statuses(value: &Value) -> Result<Vec<Status>, String> {
    let list = value["issue_statuses"]
        .as_array()
        .ok_or("Redmine nie zwrócił listy statusów.")?;
    let mut seen = HashSet::new();
    Ok(list
        .iter()
        .filter_map(|s| {
            let id = s["id"].as_u64()?;
            let name = name(s);
            if name.is_empty() || !seen.insert(id) {
                return None;
            }
            Some(Status {
                id,
                name,
                is_closed: s["is_closed"].as_bool().unwrap_or(false),
            })
        })
        .collect())
}
/// `/users/current.json` → (id, login, „Imię Nazwisko”).
pub(crate) fn parse_user(value: &Value) -> Result<(u64, String, String), String> {
    let user = &value["user"];
    let id = user["id"]
        .as_u64()
        .filter(|id| *id > 0)
        .ok_or("Nie udało się potwierdzić konta Redmine.")?;
    let login = text(&user["login"]);
    if login.is_empty() {
        return Err("Nie udało się potwierdzić konta Redmine.".into());
    }
    let mut display = format!("{} {}", text(&user["firstname"]), text(&user["lastname"]))
        .trim()
        .to_owned();
    if display.is_empty() {
        display = login.clone();
    }
    Ok((id, login, display))
}
/// Parametry `/issues.json`: projekt (Redmine dołącza podprojekty), „Moje”
/// (`assigned_to_id=me`), otwarte (`status_id=open`) albo wszystkie
/// (`status_id=*`, opcjonalnie zmienione od dnia `since`).
pub(crate) fn issues_query(
    project_id: Option<u64>,
    mine: bool,
    open_only: bool,
    since: Option<&str>,
) -> Vec<(&'static str, String)> {
    let mut query = Vec::new();
    if let Some(id) = project_id {
        query.push(("project_id", id.to_string()));
    }
    if mine {
        query.push(("assigned_to_id", "me".to_owned()));
    }
    if open_only {
        query.push(("status_id", "open".to_owned()));
    } else {
        query.push(("status_id", "*".to_owned()));
        if let Some(day) = since {
            query.push(("updated_on", format!(">={day}")));
        }
    }
    query.push(("sort", "updated_on:desc".to_owned()));
    query
}
/// Parametry pobrania po numerach (`issue_id=1,2,3&status_id=*`), paczka ≤ 100.
pub(crate) fn ids_query(ids: &[u64]) -> Vec<(&'static str, String)> {
    let list = ids
        .iter()
        .filter(|id| valid_issue_id(**id))
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    vec![("issue_id", list), ("status_id", "*".to_owned())]
}
fn truncated_warning(shown: usize, total: u64) -> String {
    format!(
        "Pokazano pierwsze {shown} z {total} zgłoszeń. Zawęź zakres (np. „Moje” albo tylko otwarte)."
    )
}
/// Wszystkie strony `/issues.json` dla `query` (do 5 stron); ostrzeżenie, gdy
/// wynik został obcięty.
pub(crate) async fn fetch_issues(
    client: &Client,
    c: &Config,
    key: &str,
    query: &[(&str, String)],
    closed: &HashMap<u64, bool>,
) -> Result<(Vec<Issue>, Option<String>), String> {
    let page = paged(
        "issues",
        "Redmine nie zwrócił listy zgłoszeń.",
        MAX_ISSUE_PAGES,
        |offset| {
            let mut q = query.to_vec();
            q.push(("limit", PAGE.to_string()));
            q.push(("offset", offset.to_string()));
            async move { get(client, c, key, "/issues.json", &q).await }
        },
    )
    .await?;
    let issues = parse_issues(&page.rows, &c.site, closed);
    let warning = (!page.complete).then(|| truncated_warning(page.rows.len(), page.total));
    Ok((issues, warning))
}
/// Zgłoszenia po numerach, paczkami po 100 (R3: status zadań przekazanych innym).
pub(crate) async fn fetch_by_ids(
    client: &Client,
    c: &Config,
    key: &str,
    ids: &[u64],
    closed: &HashMap<u64, bool>,
) -> Result<Vec<Issue>, String> {
    let mut out = Vec::new();
    for chunk in ids.chunks(PAGE as usize) {
        let mut q = ids_query(chunk);
        q.push(("limit", PAGE.to_string()));
        let value = get(client, c, key, "/issues.json", &q).await?;
        let items = value["issues"]
            .as_array()
            .ok_or("Redmine nie zwrócił listy zgłoszeń.")?;
        out.extend(parse_issues(items, &c.site, closed));
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tabele: redmine_issues, redmine_statuses, redmine_tasks

/// Zapisuje pobrane zgłoszenia (`redmine_issues`, bez `assigned` — to pole
/// obserwacji z R3). Zmiana priorytetu w Redmine przenosi pasmo na powiązane
/// zadanie, jak `jira_fields::save_fields`.
pub(crate) fn save_issues(
    tx: &rusqlite::Transaction,
    site: &str,
    issues: &[Issue],
) -> rusqlite::Result<()> {
    for issue in issues {
        let previous: Option<String> = tx
            .query_row(
                "SELECT priority FROM redmine_issues WHERE site=?1 AND issue_id=?2",
                params![site, issue.id as i64],
                |r| r.get(0),
            )
            .optional()?;
        tx.execute(
            "INSERT INTO redmine_issues(site,issue_id,subject,status,status_id,closed,project,tracker,priority,due_date)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
             ON CONFLICT(site,issue_id) DO UPDATE SET subject=excluded.subject, status=excluded.status,
             status_id=excluded.status_id, closed=excluded.closed, project=excluded.project,
             tracker=excluded.tracker, priority=excluded.priority, due_date=excluded.due_date",
            params![
                site,
                issue.id as i64,
                issue.subject,
                issue.status,
                issue.status_id as i64,
                issue.closed,
                issue.project,
                issue.tracker,
                issue.priority,
                issue.due_date
            ],
        )?;
        if previous.as_deref() != Some(issue.priority.as_str()) {
            if let Some(band) = jira_fields::priority_band(&issue.priority) {
                tx.execute(
                    "INSERT INTO task_details(task_id,json) SELECT task_id, json_object('priority',?3) FROM redmine_tasks WHERE site=?1 AND issue_id=?2
                     ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.priority',?3)",
                    params![site, issue.id as i64, band],
                )?;
            }
        }
    }
    Ok(())
}
/// Zastępuje statusy adresu w `redmine_statuses` (kolejność jak z serwera,
/// po `rowid`) i zapamiętuje czas pobrania.
pub(crate) fn save_statuses(
    tx: &rusqlite::Transaction,
    site: &str,
    statuses: &[Status],
    now: i64,
) -> rusqlite::Result<()> {
    tx.execute("DELETE FROM redmine_statuses WHERE site=?1", [site])?;
    for s in statuses {
        tx.execute(
            "INSERT INTO redmine_statuses(site,id,name,is_closed) VALUES(?1,?2,?3,?4)",
            params![site, s.id as i64, s.name, s.is_closed],
        )?;
    }
    tx.execute(
        "INSERT INTO settings(key,value) VALUES('redmine_statuses_at',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [format!("{site}|{now}")],
    )?;
    Ok(())
}
pub(crate) fn read_statuses(db: &Connection, site: &str) -> rusqlite::Result<Vec<Status>> {
    db.prepare("SELECT id, name, is_closed FROM redmine_statuses WHERE site=?1 ORDER BY rowid")?
        .query_map([site], |r| {
            Ok(Status {
                id: r.get::<_, i64>(0)? as u64,
                name: r.get(1)?,
                is_closed: r.get(2)?,
            })
        })?
        .collect()
}
/// Czy statusy adresu pobrano mniej niż dobę temu.
fn statuses_fresh(db: &Connection, site: &str, now: i64) -> rusqlite::Result<bool> {
    let raw: Option<String> = db
        .query_row(
            "SELECT value FROM settings WHERE key='redmine_statuses_at'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw
        .and_then(|v| {
            let (saved_site, at) = v.rsplit_once('|')?;
            let at: i64 = at.parse().ok()?;
            Some(saved_site == site && now >= at && now - at < STATUS_TTL)
        })
        .unwrap_or(false))
}
pub(crate) fn closed_map(statuses: &[Status]) -> HashMap<u64, bool> {
    statuses.iter().map(|s| (s.id, s.is_closed)).collect()
}
/// Statusy w kolejności z serwera (grupy widoku) i przez `closed_map` mapa
/// `status_id → is_closed` do parsowania zgłoszeń (Redmine przed 5.x nie
/// oddaje `status.is_closed` w zgłoszeniu): z `ensure_statuses`, a gdy serwer
/// nie odda statusów — z pamięci podręcznej, z ostrzeżeniem.
pub(crate) async fn known_statuses(
    app: &tauri::AppHandle,
    client: &Client,
    c: &Config,
    key: &str,
) -> Result<(Vec<Status>, Option<String>), String> {
    match ensure_statuses(app, client, c, key, false).await {
        Ok(list) => Ok((list, None)),
        Err(e) => {
            let state = app.state::<Database>();
            let db = state.0.lock().map_err(|e| e.to_string())?;
            let cached = read_statuses(&db, &c.site).map_err(|e| e.to_string())?;
            Ok((cached, Some(format!("Statusy: {e}"))))
        }
    }
}
/// Zgłoszenia pogrupowane według statusu: kolejność statusów z
/// `redmine_statuses` (jak z `/issue_statuses.json`), status spoza listy na
/// końcu (w kolejności pojawienia się), puste grupy pominięte. W grupie pasmo
/// priorytetu malejąco, potem termin (bez terminu na końcu), potem kolejność
/// z serwera (ostatnio zmienione najpierw).
pub(crate) fn group_by_status(issues: &[Issue], statuses: &[Status]) -> Vec<StatusGroup> {
    let mut groups: Vec<(StatusGroup, Option<u64>, Vec<usize>)> = statuses
        .iter()
        .map(|s| {
            (
                StatusGroup {
                    status: s.name.clone(),
                    closed: s.is_closed,
                    ids: Vec::new(),
                },
                Some(s.id),
                Vec::new(),
            )
        })
        .collect();
    for (index, issue) in issues.iter().enumerate() {
        let known = groups
            .iter()
            .position(|(_, id, _)| issue.status_id != 0 && *id == Some(issue.status_id));
        // Bez numeru statusu (albo numer spoza listy) — po nazwie.
        let slot = known.or_else(|| {
            groups.iter().position(|(g, id, _)| {
                g.status == issue.status && (id.is_none() || issue.status_id == 0)
            })
        });
        match slot {
            Some(i) => groups[i].2.push(index),
            None => groups.push((
                StatusGroup {
                    status: issue.status.clone(),
                    closed: issue.closed,
                    ids: Vec::new(),
                },
                None,
                vec![index],
            )),
        }
    }
    groups
        .into_iter()
        .filter(|(_, _, members)| !members.is_empty())
        .map(|(mut group, _, mut members)| {
            members.sort_by_key(|&i| {
                let issue = &issues[i];
                let band = jira_fields::priority_band(&issue.priority).unwrap_or(0);
                (-band, issue.due_date.is_empty(), issue.due_date.clone(), i)
            });
            group.ids = members.into_iter().map(|i| issues[i].id).collect();
            group
        })
        .collect()
}
/// Śledzone projekty z podprojektami (dowolnego poziomu) według listy `known`
/// (`/projects.json` z `parent.id`): Redmine dołącza podprojekty do zgłoszeń
/// projektu, więc ich zgłoszenia nie są „spoza śledzonych projektów”.
pub(crate) fn tracked_project_ids(tracked: &[Project], known: &[Project]) -> Vec<u64> {
    let mut ids: Vec<u64> = Vec::new();
    for p in tracked {
        if !ids.contains(&p.id) {
            ids.push(p.id);
        }
    }
    loop {
        let before = ids.len();
        for p in known {
            if !ids.contains(&p.id) && p.parent_id.is_some_and(|parent| ids.contains(&parent)) {
                ids.push(p.id);
            }
        }
        if ids.len() == before {
            return ids;
        }
    }
}
/// `project_id` dla listy zgłoszeń: puste (moje ze wszystkich projektów) albo
/// dodatnie; `0` nie jest identyfikatorem projektu w Redmine i dałoby mylące 404.
pub(crate) fn check_project(project_id: Option<u64>) -> Result<(), String> {
    match project_id {
        Some(0) => Err(BAD_PROJECT.into()),
        _ => Ok(()),
    }
}
/// Jedno zgłoszenie z `/issues/{id}.json`, parsowane z tą samą mapą statusów
/// co listy (inaczej zamknięte zgłoszenie trafiłoby do `redmine_issues` jako otwarte).
pub(crate) async fn fetch_issue(
    client: &Client,
    c: &Config,
    key: &str,
    id: u64,
    closed: &HashMap<u64, bool>,
) -> Result<Issue, String> {
    let value = get(client, c, key, &format!("/issues/{id}.json"), &[]).await?;
    parse_issue(&value["issue"], &c.site, closed).ok_or(BAD_RESPONSE.into())
}
/// Statusy z pamięci podręcznej, a przy pierwszym użyciu, po dobie albo na
/// żądanie (`force`) z `/issue_statuses.json`.
async fn ensure_statuses(
    app: &tauri::AppHandle,
    client: &Client,
    c: &Config,
    key: &str,
    force: bool,
) -> Result<Vec<Status>, String> {
    let now = db::now();
    if !force {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let cached = read_statuses(&db, &c.site).map_err(|e| e.to_string())?;
        if !cached.is_empty() && statuses_fresh(&db, &c.site, now).map_err(|e| e.to_string())? {
            return Ok(cached);
        }
    }
    let value = get(client, c, key, "/issue_statuses.json", &[]).await?;
    let list = parse_statuses(&value)?;
    let state = app.state::<Database>();
    let mut db = state.0.lock().map_err(|e| e.to_string())?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    save_statuses(&tx, &c.site, &list, now).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(list)
}
/// Czysta część `redmine_select_projects`: walidacja, bez powtórzeń, zapis.
pub(crate) fn select_projects(db: &Connection, projects: Vec<Project>) -> Result<Config, String> {
    if projects.len() > 50
        || projects.iter().any(|p| {
            p.id == 0 || p.name.trim().is_empty() || p.name.len() > 1000 || p.identifier.len() > 255
        })
    {
        return Err("Wybierz maksymalnie 50 projektów.".into());
    }
    let mut c = read_config(db)?;
    let mut unique = HashSet::new();
    c.projects = projects
        .into_iter()
        .filter(|p| unique.insert(p.id))
        .map(|p| Project {
            name: p.name.trim().to_owned(),
            identifier: p.identifier.trim().to_owned(),
            ..p
        })
        .collect();
    write_config(db, &c)?;
    Ok(c)
}
/// „Rozpocznij” dla zgłoszenia: istniejące zadanie z `redmine_tasks` wraca z
/// kosza i archiwum do „do zrobienia”, nowe dostaje tytuł równy tematowi
/// (numer pokazuje chip) i pasmo priorytetu z `redmine_issues`; poprzedni
/// pomiar zatrzymany, nowy wystartowany. Jedna transakcja.
pub(crate) fn start_issue(
    connection: &mut Connection,
    site: &str,
    id: u64,
    subject: &str,
    now: i64,
) -> Result<db::Snapshot, String> {
    if !valid_issue_id(id) {
        return Err("Niepoprawny numer zgłoszenia Redmine.".into());
    }
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT task_id FROM redmine_tasks WHERE site=?1 AND issue_id=?2",
            params![site, id as i64],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let task_id = if let Some(task_id) = existing {
        task_id
    } else {
        let mut title: String = subject.trim().chars().take(240).collect();
        if title.is_empty() {
            title = format!("#{id}");
        }
        tx.execute(
            "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
            params![title, now],
        )
        .map_err(|e| e.to_string())?;
        let task_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(?1,?2,?3)",
            params![task_id, site, id as i64],
        )
        .map_err(|e| e.to_string())?;
        let priority: Option<String> = tx
            .query_row(
                "SELECT priority FROM redmine_issues WHERE site=?1 AND issue_id=?2",
                params![site, id as i64],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(band) = priority.as_deref().and_then(jira_fields::priority_band) {
            tx.execute(
                "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('priority',?2)) ON CONFLICT(task_id) DO UPDATE SET json=json_set(json,'$.priority',?2)",
                params![task_id, band],
            )
            .map_err(|e| e.to_string())?;
        }
        task_id
    };
    tx.execute("UPDATE tasks SET done=0 WHERE id=?1", [task_id])
        .map_err(|e| e.to_string())?;
    crate::planner::clear_waiting(&tx, task_id).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE task_lifecycle SET archived=0,deleted_at=NULL WHERE task_id=?1",
        [task_id],
    )
    .map_err(|e| e.to_string())?;
    let active: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE task_id=?1 AND ended_at IS NULL)",
            [task_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !active {
        db::stop(&tx, now).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sessions(task_id,started_at) VALUES(?1,?2)",
            params![task_id, now],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(connection).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Komendy

#[tauri::command]
pub fn redmine_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    let connected = connected(&app, &c)?;
    Ok(Settings {
        config: c,
        connected,
    })
}
/// Połączenie: adres i klucz API sprawdzone przez `/users/current.json`; klucz
/// trafia do magazynu poświadczeń, login i nazwa do konfiguracji. Zmiana adresu
/// lub konta usuwa stare poświadczenie i czyści stan.
#[tauri::command]
pub async fn redmine_connect(
    app: tauri::AppHandle,
    site: String,
    api_key: String,
    accept_invalid_cert: bool,
) -> Result<Settings, String> {
    let old = config(&app)?;
    let key = api_key.trim();
    let mut c = Config {
        site: normalize_site(&site)?,
        accept_invalid_cert,
        ..Config::default()
    };
    if !valid_api_key(key) {
        return Err(KEY_ERROR.into());
    }
    let user = get(&client(&c)?, &c, key, "/users/current.json", &[]).await?;
    let (user_id, login, display_name) = parse_user(&user)?;
    c.user_id = user_id;
    c.login = login;
    c.display_name = display_name;
    if c.site == old.site {
        c.projects = old.projects.clone();
    }
    entry(&app, &c)?.set_password(key).map_err(|_| {
        "Nie udało się zapisać klucza API w magazynie poświadczeń Windows.".to_owned()
    })?;
    save(&app, &c)?;
    if !old.login.is_empty() && (c.site != old.site || c.login != old.login) {
        let _ = entry(&app, &old)?.delete_credential();
    }
    clear_state(&app)?;
    Ok(Settings {
        config: c,
        connected: true,
    })
}
/// „Testuj połączenie”: `/users/current.json` z zapisanym kluczem.
#[tauri::command]
pub async fn redmine_test(app: tauri::AppHandle) -> Result<String, String> {
    let c = config(&app)?;
    let key = api_key(&app, &c)?;
    let user = get(&client(&c)?, &c, &key, "/users/current.json", &[]).await?;
    let (user_id, login, display_name) = parse_user(&user)?;
    if login == c.login && (user_id != c.user_id || display_name != c.display_name) {
        let current = config(&app)?;
        if current.site == c.site && current.login == c.login {
            save(
                &app,
                &Config {
                    user_id,
                    display_name: display_name.clone(),
                    ..current
                },
            )?;
        }
    }
    Ok(format!(
        "Połączono jako {display_name} ({login}), Redmine odpowiada."
    ))
}
/// Rozłączenie: usuwa klucz z magazynu i czyści stan; konfiguracja (adres,
/// projekty) i powiązania zadań zostają.
#[tauri::command]
pub fn redmine_disconnect(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    if !c.login.is_empty() {
        match entry(&app, &c)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {
                return Err(
                    "Nie udało się usunąć klucza API z magazynu poświadczeń Windows.".into(),
                )
            }
        }
    }
    clear_state(&app)?;
    Ok(Settings {
        config: c,
        connected: false,
    })
}
/// Lista zgłoszeń: projektu (`project_id`) albo moich ze wszystkich projektów
/// (`project_id` puste). Statusy przy pierwszym użyciu i raz na dobę.
#[tauri::command]
pub async fn redmine_issues(
    app: tauri::AppHandle,
    project_id: Option<u64>,
    mine: bool,
    open_only: bool,
) -> Result<Issues, String> {
    check_project(project_id)?;
    let c = config(&app)?;
    let key = api_key(&app, &c)?;
    let client = client(&c)?;
    let (statuses, status_warning) = known_statuses(&app, &client, &c, &key).await?;
    let closed = closed_map(&statuses);
    let mut warnings: Vec<String> = status_warning.into_iter().collect();
    let (issues, warning) = fetch_issues(
        &client,
        &c,
        &key,
        &issues_query(project_id, mine || project_id.is_none(), open_only, None),
        &closed,
    )
    .await?;
    warnings.extend(warning);
    // Sekcja „spoza śledzonych projektów” potrzebuje podprojektów śledzonych
    // projektów: lista projektów z pamięci, a gdy jej nie ma — pobrana raz
    // (błąd nie przerywa: wtedy liczą się same śledzone projekty).
    let mut known = app
        .state::<RedmineState>()
        .projects
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    if project_id.is_none() && !c.projects.is_empty() && known.is_empty() {
        if let Ok(list) = fetch_projects(&client, &c, &key).await {
            *app.state::<RedmineState>()
                .projects
                .lock()
                .map_err(|e| e.to_string())? = list.clone();
            known = list;
        }
    }
    let groups = group_by_status(&issues, &statuses);
    let tracked = tracked_project_ids(&c.projects, &known);
    let current = config(&app)?;
    if current.site != c.site || current.login != c.login {
        return Err("Ustawienia zmieniły się podczas pobierania. Odśwież wyniki.".into());
    }
    {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        save_issues(&tx, &c.site, &issues)
            .and_then(|_| tx.commit())
            .map_err(|e| e.to_string())?;
    }
    let _ = app.emit("data-changed", ());
    // Dopisanie do pamięci (nie zastąpienie): widok pobiera osobno projekt i
    // zgłoszenia spoza śledzonych projektów, a „Rozpocznij” ma działać z obu list.
    app.state::<RedmineState>()
        .issues
        .lock()
        .map_err(|e| e.to_string())?
        .extend(issues.iter().map(|i| (i.id, i.clone())));
    Ok(Issues {
        issues,
        warnings,
        groups,
        tracked,
    })
}
/// Wszystkie projekty (do 10 stron po 100), w kolejności z serwera.
async fn fetch_projects(client: &Client, c: &Config, key: &str) -> Result<Vec<Project>, String> {
    let page = paged(
        "projects",
        "Redmine nie zwrócił listy projektów.",
        MAX_PROJECT_PAGES,
        |offset| {
            let q = vec![("limit", PAGE.to_string()), ("offset", offset.to_string())];
            async move { get(client, c, key, "/projects.json", &q).await }
        },
    )
    .await?;
    Ok(parse_projects(&page.rows))
}
/// Projekty do wyboru śledzonych (do 10 stron po 100), w kolejności z serwera.
#[tauri::command]
pub async fn redmine_projects(app: tauri::AppHandle) -> Result<Vec<Project>, String> {
    let c = config(&app)?;
    let key = api_key(&app, &c)?;
    let client = client(&c)?;
    let projects = fetch_projects(&client, &c, &key).await?;
    *app.state::<RedmineState>()
        .projects
        .lock()
        .map_err(|e| e.to_string())? = projects.clone();
    Ok(projects)
}
#[tauri::command]
pub fn redmine_select_projects(
    app: tauri::AppHandle,
    projects: Vec<Project>,
) -> Result<(), String> {
    {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        select_projects(&db, projects)?;
    }
    app.state::<RedmineState>()
        .issues
        .lock()
        .map_err(|e| e.to_string())?
        .clear();
    Ok(())
}
/// Statusy z pamięci podręcznej albo z serwera (`refresh`, pierwsze użycie, po dobie).
#[tauri::command]
pub async fn redmine_statuses(app: tauri::AppHandle, refresh: bool) -> Result<Vec<Status>, String> {
    let c = config(&app)?;
    let key = api_key(&app, &c)?;
    ensure_statuses(&app, &client(&c)?, &c, &key, refresh).await
}
#[tauri::command]
pub fn redmine_start(app: tauri::AppHandle, id: u64) -> Result<db::Snapshot, String> {
    let c = config(&app)?;
    let issue = app
        .state::<RedmineState>()
        .issues
        .lock()
        .map_err(|e| e.to_string())?
        .get(&id)
        .cloned()
        .ok_or(REFRESH_FIRST)?;
    let state = app.state::<Database>();
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = start_issue(
        &mut connection,
        &c.site,
        issue.id,
        &issue.subject,
        db::now(),
    )?;
    crate::reminder::reset(&app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
/// „Rozpocznij” spoza widoku (szybkie dodawanie, rozszerzenie przeglądarki —
/// R2, R3): jak `redmine_start`, a gdy zgłoszenia nie ma w pobranej liście,
/// temat pobiera wprost z `/issues/{id}.json`.
pub async fn start_by_id(app: &tauri::AppHandle, id: u64) -> Result<db::Snapshot, String> {
    if !valid_issue_id(id) {
        return Err("Niepoprawny numer zgłoszenia Redmine.".into());
    }
    if cached_subject(app, id).is_some() {
        return redmine_start(app.clone(), id);
    }
    let c = config(app)?;
    if c.login.is_empty() {
        return Err("Połącz konto Redmine, żeby rozpocząć zgłoszenie.".into());
    }
    let key = api_key(app, &c)?;
    let client = client(&c)?;
    // Mapa statusów jak dla list: bez niej zamknięte zgłoszenie (Redmine bez
    // `status.is_closed`) trafiłoby do `redmine_issues` jako otwarte.
    let (statuses, _) = known_statuses(app, &client, &c, &key).await?;
    let issue = fetch_issue(&client, &c, &key, id, &closed_map(&statuses)).await?;
    let state = app.state::<Database>();
    let result = {
        let mut connection = state.0.lock().map_err(|e| e.to_string())?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        save_issues(&tx, &c.site, std::slice::from_ref(&issue))
            .and_then(|_| tx.commit())
            .map_err(|e| e.to_string())?;
        start_issue(
            &mut connection,
            &c.site,
            issue.id,
            &issue.subject,
            db::now(),
        )?
    };
    app.state::<RedmineState>()
        .issues
        .lock()
        .map_err(|e| e.to_string())?
        .insert(issue.id, issue);
    crate::reminder::reset(app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
/// Otwiera w przeglądarce tylko adresy pod hostem i ścieżką skonfigurowanego
/// Redmine (zgłoszenie, „Moje konto”) albo `/my/account` wpisanego adresu.
#[tauri::command]
pub fn redmine_open(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let parsed = Url::parse(&url).map_err(|_| "Niepoprawny link.".to_owned())?;
    let c = config(&app)?;
    let allowed = (!c.site.is_empty() && allowed_link(&parsed, &c.site)) || account_link(&parsed);
    if !allowed {
        return Err("Niepoprawny link Redmine.".into());
    }
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|_| "Nie można otworzyć przeglądarki.".into())
}

// ---------------------------------------------------------------------------
// Zapisy (R4): status, przypisanie, komentarz — wyłącznie po kliknięciu

/// Komunikat, gdy główny przełącznik zapisów do Redmine jest wyłączony.
pub const WRITE_DISABLED: &str =
    "Zmiany w Redmine są wyłączone. Włączysz je w Ustawienia → Redmine („Pozwól aplikacji zmieniać zgłoszenia w Redmine”).";
/// Początek komunikatu, gdy Redmine przyjął `PUT`, ale status się nie zmienił
/// (status spoza przepływu bywa ignorowany bez błędu 422).
pub const STATUS_UNCHANGED: &str = "Redmine nie zmienił statusu";
/// Najdłuższy komentarz (`notes`) wysyłany z aplikacji.
pub const NOTES_LIMIT: usize = 4000;
/// Limit treści jednego zapisu do Redmine.
const WRITE_LIMIT: usize = 32 * 1024;
const BAD_STATUS: &str = "Niepoprawny status Redmine.";
const BAD_ISSUE: &str = "Niepoprawny numer zgłoszenia Redmine.";
const NO_USER: &str =
    "Nie znam Twojego konta w Redmine. Kliknij „Testuj połączenie” albo połącz konto ponownie w Ustawieniach → Redmine.";

/// Główny przełącznik zapisów (`redmine_write`): backend odrzuca zapis
/// niezależnie od interfejsu. Niezależny od `jira_write`.
pub fn ensure_write(s: &crate::settings::AppSettings) -> Result<(), String> {
    if s.redmine_write {
        Ok(())
    } else {
        Err(WRITE_DISABLED.into())
    }
}
/// Ścieżka zapisu: tylko `/issues/<numer>.json`.
fn valid_write_path(path: &str) -> bool {
    path.strip_prefix("/issues/")
        .and_then(|rest| rest.strip_suffix(".json"))
        .is_some_and(|n| {
            !n.is_empty()
                && n.len() <= 10
                && n.bytes().all(|b| b.is_ascii_digit())
                && n.parse::<u64>().is_ok_and(valid_issue_id)
        })
}
fn issue_path(id: u64) -> Result<String, String> {
    if valid_issue_id(id) {
        Ok(format!("/issues/{id}.json"))
    } else {
        Err(BAD_ISSUE.into())
    }
}
/// Ciało `PUT /issues/{id}.json` zmieniające status.
pub(crate) fn status_body(status_id: u64) -> Result<Value, String> {
    if status_id == 0 || status_id > 9_999_999_999 {
        return Err(BAD_STATUS.into());
    }
    Ok(serde_json::json!({"issue": {"status_id": status_id}}))
}
/// Ciało „Przypisz do mnie”: `assigned_to_id` = identyfikator mojego konta.
pub(crate) fn assign_body(user_id: u64) -> Result<Value, String> {
    if user_id == 0 {
        return Err(NO_USER.into());
    }
    Ok(serde_json::json!({"issue": {"assigned_to_id": user_id}}))
}
/// Ciało komentarza: zwykły tekst w `notes` (końce linii ujednolicone, bez
/// znaków sterujących poza tabulatorem i końcem linii), 1–4000 znaków.
pub(crate) fn notes_body(text: &str) -> Result<Value, String> {
    let text: String = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Wpisz treść komentarza.".into());
    }
    if trimmed.chars().count() > NOTES_LIMIT {
        return Err(format!(
            "Komentarz może mieć najwyżej {NOTES_LIMIT} znaków."
        ));
    }
    Ok(serde_json::json!({"issue": {"notes": trimmed}}))
}
/// Odpowiedź na zapis: błąd HTTP po polsku (422 z `errors`), pusta odpowiedź
/// (204 albo 200 bez treści) to `Value::Null`, strona HTML to osobny komunikat.
pub(crate) fn write_response(status: StatusCode, html: bool, text: &str) -> Result<Value, String> {
    if !status.is_success() {
        return Err(status_error(status, text));
    }
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    parse_response(status, html, text)
}
/// Wysyła zapis (`PUT /issues/{id}.json`) z kluczem API w nagłówku; ścieżka
/// tylko do zgłoszenia, treść z limitem długości.
pub(crate) async fn send_write(
    client: &Client,
    c: &Config,
    key: &str,
    method: reqwest::Method,
    path: &str,
    body: &Value,
) -> Result<Value, String> {
    if !valid_write_path(path) {
        return Err("Niepoprawny adres zapisu do Redmine.".into());
    }
    let raw = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    if raw.len() > WRITE_LIMIT {
        return Err("Zmiana jest zbyt duża dla Redmine.".into());
    }
    // W testach dozwolony jest tylko lokalny serwer, nigdy prawdziwy Redmine.
    #[cfg(test)]
    assert!(
        c.site.starts_with("http://127.0.0.1:"),
        "testy nie mogą łączyć się z serwerem Redmine"
    );
    let response = client
        .request(method, format!("{}{}", c.site, path))
        .header("X-Redmine-API-Key", key)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(raw)
        .send()
        .await
        .map_err(|e| network_error(&e))?;
    let status = response.status();
    let html = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().contains("text/html"));
    let text = response.text().await.unwrap_or_default();
    write_response(status, html, &text)
}
/// Połączone konto: konfiguracja, klucz API i klient.
fn session(app: &tauri::AppHandle) -> Result<(Config, String, Client), String> {
    let c = config(app)?;
    if c.login.is_empty() || c.site.is_empty() {
        return Err(NOT_CONNECTED.into());
    }
    let key = api_key(app, &c)?;
    let client = client(&c)?;
    Ok((c, key, client))
}
/// Jedyna ścieżka zapisu do Redmine (status, przypisanie, komentarz): główny
/// przełącznik, połączone konto, walidacja i polskie komunikaty błędów.
pub async fn write(
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
    body: &Value,
) -> Result<Value, String> {
    ensure_write(&crate::settings::current(app))?;
    let (c, key, client) = session(app)?;
    send_write(&client, &c, &key, method, path, body).await
}

/// `allowed_statuses` z `/issues/{id}.json?include=allowed_statuses` (Redmine
/// od 5.0); `None`, gdy odpowiedź nie ma tego pola. `is_closed` z mapy
/// statusów, gdy serwer go nie podaje.
pub(crate) fn parse_allowed(value: &Value, closed: &HashMap<u64, bool>) -> Option<Vec<Status>> {
    let list = value["issue"]["allowed_statuses"].as_array()?;
    let mut seen = HashSet::new();
    Some(
        list.iter()
            .filter_map(|s| {
                let id = s["id"].as_u64().filter(|id| *id > 0)?;
                let name = name(s);
                if name.is_empty() || !seen.insert(id) {
                    return None;
                }
                Some(Status {
                    id,
                    name,
                    is_closed: s["is_closed"]
                        .as_bool()
                        .or_else(|| closed.get(&id).copied())
                        .unwrap_or(false),
                })
            })
            .collect(),
    )
}
/// Dozwolone statusy, a bez pola `allowed_statuses` (Redmine przed 5.0)
/// wszystkie statusy z `redmine_statuses` (serwer odrzuci niedozwolony 422).
pub(crate) fn allowed_or_all(allowed: Option<Vec<Status>>, all: &[Status]) -> Vec<Status> {
    allowed.unwrap_or_else(|| all.to_vec())
}
/// „Przesuń w Redmine…” / „Przesuń do…”: dozwolone statusy bez bieżącego.
pub fn transitions_for(allowed: &[Status], current: u64) -> Vec<Status> {
    allowed
        .iter()
        .filter(|s| s.id != current)
        .cloned()
        .collect()
}
/// Cel pytania przy starcie pracy: dozwolone statusy niezamknięte (bez
/// bieżącego) i wybrany status o nazwie z ustawień (wielkość liter bez
/// znaczenia); bez dopasowania `None` — baner pokazuje listę do wyboru.
pub fn choose_status(
    allowed: &[Status],
    current: u64,
    preferred: &str,
) -> (Vec<Status>, Option<u64>) {
    let candidates: Vec<Status> = transitions_for(allowed, current)
        .into_iter()
        .filter(|s| !s.is_closed)
        .collect();
    let wanted = preferred.trim().to_lowercase();
    let chosen = candidates
        .iter()
        .find(|s| !wanted.is_empty() && s.name.trim().to_lowercase() == wanted)
        .map(|s| s.id);
    (candidates, chosen)
}
/// Czy pytać o przesunięcie: zapisy i pytanie włączone, zadanie bez „Nie
/// pytaj”, zgłoszenie niezamknięte w statusie z `redmine_new_statuses`.
pub fn should_ask(
    s: &crate::settings::AppSettings,
    no_ask: bool,
    status: &str,
    closed: bool,
) -> bool {
    let status = status.trim().to_lowercase();
    s.redmine_write
        && s.redmine_ask_move
        && !no_ask
        && !closed
        && !status.is_empty()
        && s.redmine_new_statuses
            .iter()
            .any(|n| n.trim().to_lowercase() == status)
}
/// Zgłoszenie z dozwolonymi statusami z jednego zapytania.
pub(crate) async fn fetch_issue_allowed(
    client: &Client,
    c: &Config,
    key: &str,
    id: u64,
    closed: &HashMap<u64, bool>,
) -> Result<(Issue, Option<Vec<Status>>), String> {
    let path = issue_path(id)?;
    let value = get(
        client,
        c,
        key,
        &path,
        &[("include", "allowed_statuses".to_owned())],
    )
    .await?;
    let issue = parse_issue(&value["issue"], &c.site, closed).ok_or(BAD_RESPONSE)?;
    Ok((issue, parse_allowed(&value, closed)))
}
/// Zapisuje zgłoszenie w `redmine_issues` (kafelek) i w pamięci tematów.
fn remember(app: &tauri::AppHandle, site: &str, issue: &Issue) -> Result<(), String> {
    {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        save_issues(&tx, site, std::slice::from_ref(issue))
            .and_then(|_| tx.commit())
            .map_err(|e| e.to_string())?;
    }
    app.state::<RedmineState>()
        .issues
        .lock()
        .map_err(|e| e.to_string())?
        .insert(issue.id, issue.clone());
    let _ = app.emit("data-changed", ());
    Ok(())
}
/// Bieżące zgłoszenie (odświeżone w `redmine_issues`) i jego dozwolone statusy
/// (bez pola `allowed_statuses` — wszystkie znane statusy).
pub(crate) async fn issue_statuses(
    app: &tauri::AppHandle,
    id: u64,
) -> Result<(Issue, Vec<Status>), String> {
    issue_path(id)?;
    let (c, key, client) = session(app)?;
    let (statuses, _) = known_statuses(app, &client, &c, &key).await?;
    let (issue, allowed) =
        fetch_issue_allowed(&client, &c, &key, id, &closed_map(&statuses)).await?;
    remember(app, &c.site, &issue)?;
    Ok((issue, allowed_or_all(allowed, &statuses)))
}
/// Dozwolone statusy zgłoszenia bez bieżącego (lista „Przesuń…”).
pub(crate) async fn transitions(app: &tauri::AppHandle, id: u64) -> Result<Vec<Status>, String> {
    ensure_write(&crate::settings::current(app))?;
    let (issue, allowed) = issue_statuses(app, id).await?;
    Ok(transitions_for(&allowed, issue.status_id))
}
/// Odczyt zgłoszenia po zapisie (status, osoba) i zapis do `redmine_issues`;
/// bez odczytu — zgłoszenie z pamięci widoku z naniesioną zmianą (`patch`)
/// albo `None`. Kopia z pamięci trafia tylko do widoku (i pamięci widoku), nie
/// do `redmine_issues`: mogłaby nadpisać nowsze dane z obserwacji (temat,
/// priorytet, termin) i ponownie nanieść pasmo priorytetu na zadanie. Pola
/// zmienione zapisem w tabeli uzupełnia wywołujący (status — `set_status`).
async fn refresh_after_write(
    app: &tauri::AppHandle,
    id: u64,
    patch: impl FnOnce(&mut Issue),
) -> Option<Issue> {
    let (c, key, client) = session(app).ok()?;
    let statuses = known_statuses(app, &client, &c, &key)
        .await
        .map(|(list, _)| list)
        .unwrap_or_default();
    match fetch_issue(&client, &c, &key, id, &closed_map(&statuses)).await {
        Ok(issue) => {
            remember(app, &c.site, &issue).ok()?;
            Some(issue)
        }
        Err(_) => {
            let state = app.state::<RedmineState>();
            let mut cache = state.issues.lock().ok()?;
            let issue = cache.get_mut(&id)?;
            patch(issue);
            Some(issue.clone())
        }
    }
}
/// Status po zapisie różny od docelowego: Redmine przyjął zapis bez zmiany.
pub(crate) fn status_unchanged(issue: &Issue, status_id: u64) -> Option<String> {
    (issue.status_id != 0 && issue.status_id != status_id).then(|| {
        format!(
            "{STATUS_UNCHANGED} #{} (nadal „{}”). Zmień status w przeglądarce — przejście może wymagać dodatkowych pól.",
            issue.id, issue.status
        )
    })
}
/// Status po zapisie w `redmine_issues` — tylko pola statusu; temat, priorytet,
/// termin i pasmo zadania bez zmian.
pub(crate) fn save_status(
    db: &Connection,
    site: &str,
    id: u64,
    status: &Status,
) -> rusqlite::Result<()> {
    db.execute(
        "UPDATE redmine_issues SET status=?3, status_id=?4, closed=?5 WHERE site=?1 AND issue_id=?2",
        params![site, id as i64, status.name, status.id as i64, status.is_closed],
    )?;
    Ok(())
}
/// Zmiana statusu (`PUT` z `status_id`), potem odczyt statusu i zapis do
/// `redmine_issues` i kafelka. Bez odczytu po zapisie status docelowy z listy
/// statusów trafia do `redmine_issues` (`None` — brak zgłoszenia w pamięci).
pub(crate) async fn set_status(
    app: &tauri::AppHandle,
    id: u64,
    status_id: u64,
) -> Result<Option<Issue>, String> {
    let path = issue_path(id)?;
    let body = status_body(status_id)?;
    write(app, reqwest::Method::PUT, &path, &body).await?;
    let target = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let site = read_config(&db)?.site;
        let target = read_statuses(&db, &site)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|s| s.id == status_id);
        if let Some(s) = &target {
            save_status(&db, &site, id, s).map_err(|e| e.to_string())?;
        }
        target
    };
    let issue = refresh_after_write(app, id, |issue| {
        if let Some(s) = &target {
            issue.status = s.name.clone();
            issue.status_id = s.id;
            issue.closed = s.is_closed;
        }
    })
    .await;
    let _ = app.emit("data-changed", ());
    match issue.as_ref().and_then(|i| status_unchanged(i, status_id)) {
        Some(message) => Err(message),
        None => Ok(issue),
    }
}
/// Tytuł do nazwy gałęzi: temat z `redmine_issues`, a bez niego tytuł zadania
/// powiązanego ze zgłoszeniem.
pub(crate) fn branch_title(db: &Connection, site: &str, id: u64) -> rusqlite::Result<String> {
    let subject: Option<String> = db
        .query_row(
            "SELECT subject FROM redmine_issues WHERE site=?1 AND issue_id=?2 AND subject<>''",
            params![site, id as i64],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(subject) = subject {
        return Ok(subject);
    }
    Ok(db
        .query_row(
            "SELECT t.title FROM redmine_tasks r JOIN tasks t ON t.id=r.task_id WHERE r.site=?1 AND r.issue_id=?2",
            params![site, id as i64],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

/// „Przesuń do…” w widoku Redmine: dozwolone statusy zgłoszenia bez bieżącego.
#[tauri::command]
pub async fn redmine_issue_transitions(
    app: tauri::AppHandle,
    issue_id: u64,
) -> Result<Vec<Status>, String> {
    transitions(&app, issue_id).await
}
/// Zmiana statusu z widoku Redmine; wynik to zgłoszenie po odczycie (karta
/// przechodzi do grupy nowego statusu) albo `None` bez odczytu.
#[tauri::command]
pub async fn redmine_issue_transition(
    app: tauri::AppHandle,
    issue_id: u64,
    status_id: u64,
) -> Result<Option<Issue>, String> {
    set_status(&app, issue_id, status_id).await
}
/// „Przypisz do mnie”: `PUT` z `assigned_to_id` = identyfikator mojego konta.
#[tauri::command]
pub async fn redmine_assign_me(
    app: tauri::AppHandle,
    issue_id: u64,
) -> Result<Option<Issue>, String> {
    let path = issue_path(issue_id)?;
    ensure_write(&crate::settings::current(&app))?;
    let c = config(&app)?;
    let body = assign_body(c.user_id)?;
    write(&app, reqwest::Method::PUT, &path, &body).await?;
    let (user_id, display_name) = (c.user_id, c.display_name.clone());
    Ok(refresh_after_write(&app, issue_id, move |issue| {
        issue.assigned_to_id = Some(user_id);
        issue.assigned_to = display_name;
    })
    .await)
}
/// „Dodaj komentarz”: `PUT` z `notes` (zwykły tekst, 1–4000 znaków).
#[tauri::command]
pub async fn redmine_comment(
    app: tauri::AppHandle,
    issue_id: u64,
    text: String,
) -> Result<(), String> {
    let path = issue_path(issue_id)?;
    ensure_write(&crate::settings::current(&app))?;
    let body = notes_body(&text)?;
    write(&app, reqwest::Method::PUT, &path, &body)
        .await
        .map(|_| ())
}
/// „Kopiuj nazwę gałęzi”: `feature/1234-krotki-tytul` (ta sama funkcja co dla
/// Jiry, numer zamiast klucza, prefiks z `redmine_branch_prefix`).
#[tauri::command]
pub fn redmine_branch_name(app: tauri::AppHandle, issue_id: u64) -> Result<String, String> {
    issue_path(issue_id)?;
    let title = {
        let state = app.state::<Database>();
        let db = state.0.lock().map_err(|e| e.to_string())?;
        let site = read_config(&db)?.site;
        branch_title(&db, &site, issue_id).map_err(|e| e.to_string())?
    };
    let title = if title.is_empty() {
        cached_subject(&app, issue_id).unwrap_or_default()
    } else {
        title
    };
    Ok(crate::jira_board::branch_name(
        &crate::settings::current(&app).redmine_branch_prefix,
        &issue_id.to_string(),
        &title,
    ))
}

// ---------------------------------------------------------------------------
// Obserwacja przypisań (R3): co 5 minut, jak `jira::start`

/// Co ile sprawdzać nowe przypisania w Redmine.
const WATCH_EVERY: i64 = 5 * 60_000;
/// Co ile wątek sprawdza, czy pora na zapytanie (szybka reakcja na włączenie opcji).
const WATCH_TICK: Duration = Duration::from_secs(20);
/// Zamknięte zgłoszenia zmienione w tylu ostatnich dniach odświeżają status.
const WATCH_CLOSED_DAYS: i64 = 14;

/// Zapytania obserwacji: moje otwarte (do importu) i moje zmienione od
/// `today - 14 dni` w dowolnym statusie (status zamkniętych niedawno).
pub(crate) fn watch_queries(today: chrono::NaiveDate) -> [Vec<(&'static str, String)>; 2] {
    let since = (today - chrono::Duration::days(WATCH_CLOSED_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    [
        issues_query(None, true, true, None),
        issues_query(None, true, false, Some(&since)),
    ]
}
/// Moje zgłoszenia z obu zapytań bez powtórzeń (pierwsze wystąpienie wygrywa).
fn merge_assigned(open: Vec<Issue>, recent: Vec<Issue>) -> Vec<Issue> {
    let mut seen: HashSet<u64> = open.iter().map(|i| i.id).collect();
    let mut out = open;
    out.extend(recent.into_iter().filter(|i| seen.insert(i.id)));
    out
}
/// Numery zgłoszeń otwartych (nieukończonych, nie w koszu) zadań powiązanych z
/// Redmine pod adresem `site`, poza `skip` (już pobrane jako moje).
fn linked_open_ids(db: &Connection, site: &str, skip: &HashSet<u64>) -> rusqlite::Result<Vec<u64>> {
    let ids: Vec<i64> = db
        .prepare(
            "SELECT r.issue_id FROM redmine_tasks r JOIN tasks t ON t.id=r.task_id
             LEFT JOIN task_lifecycle l ON l.task_id=t.id
             WHERE r.site=?1 AND t.done=0 AND l.deleted_at IS NULL ORDER BY r.issue_id",
        )?
        .query_map([site], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(ids
        .into_iter()
        .map(|id| id as u64)
        .filter(|id| valid_issue_id(*id) && !skip.contains(id))
        .collect())
}

/// Zapisuje zgłoszenia (status, priorytet, tracker, termin; pasmo priorytetu
/// powiązanych zadań przy zmianie) i dodaje zadania dla otwartych zgłoszeń,
/// które od ostatniego sprawdzenia zostały przypisane do mnie (`mine`).
/// `others` to zgłoszenia zadań przekazane komuś innemu: tylko odświeżenie.
/// `baseline` (pierwsze sprawdzenie dla konta) tylko zapamiętuje obecne
/// przypisania, bez importu całego backlogu. Nowe zadanie nie ma dnia w planie
/// ani pomiaru, więc trafia do „Do zaplanowania”; zgłoszenie już powiązane z
/// zadaniem (także w archiwum lub koszu) nie jest dublowane; zdjęte z listy i
/// przypisane ponownie (bez zadania) wraca jako nowe. Jak `jira::sync_assigned`.
pub(crate) fn sync_assigned(
    connection: &mut Connection,
    site: &str,
    mine: &[Issue],
    others: &[Issue],
    baseline: bool,
    now: i64,
) -> Result<Vec<Issue>, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let mut imported = Vec::new();
    for issue in mine.iter().filter(|i| !i.closed) {
        let known: bool = tx
            .query_row(
                "SELECT COALESCE((SELECT assigned FROM redmine_issues WHERE site=?1 AND issue_id=?2),0)=1
                 OR EXISTS(SELECT 1 FROM redmine_tasks WHERE site=?1 AND issue_id=?2)",
                params![site, issue.id as i64],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if baseline || known {
            continue;
        }
        let title: String = issue.subject.trim().chars().take(240).collect();
        tx.execute(
            "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
            params![title, now],
        )
        .map_err(|e| e.to_string())?;
        let task_id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(?1,?2,?3)",
            params![task_id, site, issue.id as i64],
        )
        .map_err(|e| e.to_string())?;
        // Pasmo z priorytetu zgłoszenia także wtedy, gdy priorytet w
        // `redmine_issues` się nie zmienił (zgłoszenie widziane wcześniej).
        if let Some(band) = jira_fields::priority_band(&issue.priority) {
            tx.execute(
                "INSERT INTO task_details(task_id,json) VALUES(?1,json_object('priority',?2))",
                params![task_id, band],
            )
            .map_err(|e| e.to_string())?;
        }
        imported.push(issue.clone());
    }
    let all: Vec<Issue> = mine.iter().chain(others.iter()).cloned().collect();
    save_issues(&tx, site, &all).map_err(|e| e.to_string())?;
    tx.execute("UPDATE redmine_issues SET assigned=0 WHERE site=?1", [site])
        .map_err(|e| e.to_string())?;
    for issue in mine.iter().filter(|i| !i.closed) {
        tx.execute(
            "UPDATE redmine_issues SET assigned=1 WHERE site=?1 AND issue_id=?2",
            params![site, issue.id as i64],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(imported)
}

/// Treść powiadomienia o zaimportowanych zgłoszeniach (najwyżej trzy wymienione).
pub(crate) fn imported_message(rows: &[Issue]) -> (String, String) {
    let title = if rows.len() == 1 {
        "Nowe zadanie z Redmine".to_owned()
    } else {
        format!("Nowe z Redmine: {}", crate::notify::tasks_word(rows.len()))
    };
    let mut lines: Vec<String> = rows
        .iter()
        .take(3)
        .map(|r| crate::notify::short(&format!("#{} · {}", r.id, r.subject)))
        .collect();
    if rows.len() > 3 {
        lines.push(format!("i jeszcze {}", rows.len() - 3));
    }
    lines.push("Dodano do „Do zaplanowania”.".into());
    (title, lines.join("\n"))
}

/// Lokalne zadania zaimportowanych zgłoszeń, w kolejności `rows`.
pub(crate) fn imported_task_ids(
    db: &Connection,
    site: &str,
    rows: &[Issue],
) -> rusqlite::Result<Vec<i64>> {
    rows.iter()
        .map(|r| {
            db.query_row(
                "SELECT task_id FROM redmine_tasks WHERE site=?1 AND issue_id=?2",
                params![site, r.id as i64],
                |row| row.get(0),
            )
        })
        .collect()
}

/// Powiadomienie z tymi samymi przyciskami co dla Jiry: kliknięcie otwiera
/// Zadania przy nowych (nierozpoczętych) zadaniach, „Rozpocznij” tylko przy
/// jednym zgłoszeniu, „Zaplanuj na dziś” dla wszystkich.
pub(crate) fn imported_notification(rows: &[Issue], ids: &[i64]) -> crate::notify::Message {
    use crate::notify::{Action, Message, View, PLAN_TODAY, REDMINE_START};
    let (title, body) = imported_message(rows);
    let mut actions = Vec::new();
    if let [id] = ids {
        actions.push(Action::new(REDMINE_START, "Rozpocznij", id));
    }
    if !ids.is_empty() {
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        actions.push(Action::new(PLAN_TODAY, "Zaplanuj na dziś", list));
    }
    Message {
        key: format!(
            "redmine:{}",
            rows.iter()
                .map(|r| r.id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        title,
        body,
        actions,
        open: View {
            section: Some("Nierozpoczęte"),
            task: if ids.len() == 1 { Some(ids[0]) } else { None },
            ..View::tasks()
        },
    }
}

/// „Rozpocznij” z powiadomienia: start zadania powiązanego ze zgłoszeniem
/// Redmine (z kosza i archiwum wraca), jak „Rozpocznij” w widoku Redmine.
pub fn start_task(
    connection: &mut Connection,
    task_id: i64,
    now: i64,
) -> Result<db::Snapshot, String> {
    let (site, id, title): (String, i64, String) = connection
        .query_row(
            "SELECT r.site, r.issue_id, t.title FROM redmine_tasks r JOIN tasks t ON t.id=r.task_id
             WHERE r.task_id=?1",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Nie znaleziono zadania powiązanego ze zgłoszeniem Redmine.")?;
    start_issue(connection, &site, id as u64, &title, now)
}

/// Jedno sprawdzenie przypisań; `None`, gdy opcja wyłączona lub brak połączenia.
/// Wynik: zaimportowane zgłoszenia i ich lokalne zadania.
#[allow(clippy::type_complexity)]
fn watch_once(app: &tauri::AppHandle) -> Option<Result<(Vec<Issue>, Vec<i64>), String>> {
    if !crate::settings::current(app).redmine_auto_import {
        return None;
    }
    let c = config(app).ok()?;
    if c.login.is_empty() || c.site.is_empty() {
        return None;
    }
    let key = api_key(app, &c).ok()?;
    let fetched = tauri::async_runtime::block_on(async {
        let client = client(&c)?;
        let (statuses, _) = known_statuses(app, &client, &c, &key).await?;
        let closed = closed_map(&statuses);
        let [open_query, recent_query] = watch_queries(chrono::Local::now().date_naive());
        let (open, _) = fetch_issues(&client, &c, &key, &open_query, &closed).await?;
        let (recent, _) = fetch_issues(&client, &c, &key, &recent_query, &closed).await?;
        let mine = merge_assigned(open, recent);
        // Zgłoszenia zadań przekazane komuś innemu: status i priorytet bez
        // importu; błąd tego pobrania nie przerywa importu przypisanych.
        let skip: HashSet<u64> = mine.iter().map(|i| i.id).collect();
        let ids = {
            let state = app.state::<Database>();
            let db = state.0.lock().map_err(|e| e.to_string())?;
            linked_open_ids(&db, &c.site, &skip).map_err(|e| e.to_string())?
        };
        let others = if ids.is_empty() {
            Vec::new()
        } else {
            fetch_by_ids(&client, &c, &key, &ids, &closed)
                .await
                .unwrap_or_default()
        };
        Ok::<_, String>((mine, others))
    });
    Some(fetched.and_then(|(mine, others)| {
        let current = config(app)?;
        if current.site != c.site || current.login != c.login {
            return Ok((Vec::new(), Vec::new()));
        }
        let watch = format!("{}|{}", c.site, c.login);
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let baseline = db
            .query_row(
                "SELECT value FROM settings WHERE key='redmine_watch'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .as_deref()
            != Some(watch.as_str());
        let imported = sync_assigned(&mut db, &c.site, &mine, &others, baseline, db::now())?;
        db.execute(
            "INSERT INTO settings(key,value) VALUES('redmine_watch',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [watch],
        )
        .map_err(|e| e.to_string())?;
        let ids = imported_task_ids(&db, &c.site, &imported).map_err(|e| e.to_string())?;
        drop(db);
        // Tematy do „Rozpocznij” i szybkiego dodawania jak po pobraniu listy.
        if let Ok(mut cache) = app.state::<RedmineState>().issues.lock() {
            cache.extend(mine.iter().chain(others.iter()).map(|i| (i.id, i.clone())));
        }
        Ok((imported, ids))
    }))
}

/// Wątek sprawdzający co 5 min nowe przypisania w Redmine, niezależny od wątku
/// Jiry. Kończy się, gdy okno main zniknie, więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let mut last: Option<i64> = None;
        loop {
            std::thread::sleep(WATCH_TICK);
            if app.get_webview_window("main").is_none() {
                break;
            }
            let now = db::now();
            if last.is_some_and(|at| now - at < WATCH_EVERY) {
                continue;
            }
            match watch_once(&app) {
                None => last = None,
                Some(result) => {
                    last = Some(now);
                    if let Ok((imported, ids)) = result {
                        let _ = app.emit("data-changed", ());
                        if !imported.is_empty() {
                            crate::notify::show(&app, &imported_notification(&imported, &ids));
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    const ISSUES: &str = include_str!("../tests/fixtures/redmine_issues.json");
    const PROJECTS: &str = include_str!("../tests/fixtures/redmine_projects.json");
    const STATUSES: &str = include_str!("../tests/fixtures/redmine_statuses.json");
    const USER: &str = include_str!("../tests/fixtures/redmine_user.json");
    const SITE: &str = "https://redmine.example.pl";

    fn fixture_issues() -> Vec<Issue> {
        let value: Value = serde_json::from_str(ISSUES).unwrap();
        let statuses = parse_statuses(&serde_json::from_str(STATUSES).unwrap()).unwrap();
        parse_issues(
            value["issues"].as_array().unwrap(),
            SITE,
            &closed_map(&statuses),
        )
    }
    fn memory() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        db
    }

    #[test]
    fn parses_issues_fixture_with_due_date_closed_unassigned_and_version() {
        let issues = fixture_issues();
        assert_eq!(issues.len(), 4);
        let first = &issues[0];
        assert_eq!(first.id, 1234);
        assert_eq!(
            first.subject,
            "Eksport faktur – błąd przy polskich znakach (ąęśćżźńół)"
        );
        assert_eq!(
            (first.project_id, first.project.as_str()),
            (7, "Magazyn WMS")
        );
        assert_eq!(first.tracker, "Błąd");
        assert_eq!(
            (first.status.as_str(), first.status_id, first.closed),
            ("New", 1, false)
        );
        assert_eq!(first.priority, "High");
        assert_eq!(first.assigned_to_id, Some(12));
        assert_eq!(first.assigned_to, "Mateusz Krajnik");
        assert_eq!(first.version, "2.4");
        assert_eq!(first.category, "Integracje");
        assert_eq!(first.due_date, "2026-10-30");
        assert_eq!(first.updated_on, "2026-10-19T14:22:10Z");
        assert_eq!(first.url, "https://redmine.example.pl/issues/1234");
        // Bez terminu (`due_date: null`) i bez wersji.
        assert_eq!(issues[1].due_date, "");
        assert_eq!(issues[1].version, "");
        // Zamknięte z `is_closed` serwera.
        assert!(issues[2].closed);
        assert_eq!(issues[2].status, "Closed");
        // Bez `assigned_to`; `is_closed` spoza odpowiedzi — z listy statusów.
        assert_eq!(issues[3].assigned_to_id, None);
        assert_eq!(issues[3].assigned_to, "");
        assert!(!issues[3].closed);
        assert_eq!(issues[3].priority, "Immediate");
        // Bez mapy statusów nieznany `is_closed` to „otwarte”; zamknięty status z mapy.
        let value: Value = serde_json::from_str(ISSUES).unwrap();
        let items = value["issues"].as_array().unwrap();
        assert!(
            !parse_issue(&items[3], SITE, &HashMap::new())
                .unwrap()
                .closed
        );
        let closed: HashMap<u64, bool> = [(1, true)].into_iter().collect();
        assert!(parse_issue(&items[3], SITE, &closed).unwrap().closed);
        // Niepoprawny numer i powtórka są pomijane; pusty temat to „#id”.
        let items = vec![
            serde_json::json!({"id": 0, "subject": "zero"}),
            serde_json::json!({"id": "x"}),
            serde_json::json!({"id": 5}),
            serde_json::json!({"id": 5, "subject": "powtórka"}),
        ];
        let parsed = parse_issues(&items, SITE, &HashMap::new());
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].subject, "#5");
    }
    #[test]
    fn parses_projects_statuses_and_user_fixtures() {
        let value: Value = serde_json::from_str(PROJECTS).unwrap();
        let projects = parse_projects(value["projects"].as_array().unwrap());
        assert_eq!(
            projects,
            vec![
                Project {
                    id: 7,
                    name: "Magazyn WMS".into(),
                    identifier: "wms".into(),
                    parent_id: None,
                },
                Project {
                    id: 8,
                    name: "WMS – integracje".into(),
                    identifier: "wms-integracje".into(),
                    parent_id: Some(7),
                },
                Project {
                    id: 9,
                    name: "Portal klienta".into(),
                    identifier: "portal".into(),
                    parent_id: None,
                },
            ]
        );
        let statuses = parse_statuses(&serde_json::from_str(STATUSES).unwrap()).unwrap();
        assert_eq!(statuses.len(), 5);
        assert_eq!(
            statuses[2],
            Status {
                id: 3,
                name: "Feedback".into(),
                is_closed: false
            }
        );
        assert_eq!(
            statuses[4],
            Status {
                id: 5,
                name: "Closed".into(),
                is_closed: true
            }
        );
        assert!(parse_statuses(&serde_json::json!({"issues": []})).is_err());
        let (id, login, display) = parse_user(&serde_json::from_str(USER).unwrap()).unwrap();
        assert_eq!(
            (id, login.as_str(), display.as_str()),
            (12, "mkrajnik", "Mateusz Krajnik")
        );
        // Bez imienia i nazwiska nazwa to login; bez loginu błąd.
        let (_, _, display) =
            parse_user(&serde_json::json!({"user": {"id": 3, "login": "jan"}})).unwrap();
        assert_eq!(display, "jan");
        assert!(parse_user(&serde_json::json!({"user": {"id": 3}})).is_err());
        assert!(parse_user(&serde_json::json!({"error": "x"})).is_err());
        // Nowe nazwy priorytetów Redmine dają pasmo „wysoki”.
        assert_eq!(jira_fields::priority_band("Immediate"), Some(1));
        assert_eq!(jira_fields::priority_band("natychmiastowy"), Some(1));
        assert_eq!(jira_fields::priority_band("Urgent"), Some(1));
        assert_eq!(jira_fields::priority_band("Normal"), Some(0));
        assert_eq!(jira_fields::priority_band("Low"), Some(-1));
    }
    #[test]
    fn select_projects_saves_overwrites_and_clears() {
        let db = memory();
        write_config(
            &db,
            &Config {
                site: SITE.into(),
                login: "mkrajnik".into(),
                user_id: 12,
                display_name: "Mateusz Krajnik".into(),
                ..Config::default()
            },
        )
        .unwrap();
        let project = |id: u64, name: &str| Project {
            id,
            name: name.into(),
            identifier: format!("p{id}"),
            parent_id: None,
        };
        let c =
            select_projects(&db, vec![project(7, " Magazyn WMS "), project(7, "Dubel")]).unwrap();
        assert_eq!(c.projects.len(), 1);
        assert_eq!(c.projects[0].name, "Magazyn WMS");
        // Reszta konfiguracji nietknięta, wybór zapisany w `settings`.
        let saved = read_config(&db).unwrap();
        assert_eq!(
            (saved.site.as_str(), saved.login.as_str(), saved.user_id),
            (SITE, "mkrajnik", 12)
        );
        assert_eq!(saved.projects, c.projects);
        // Nadpisanie innym zestawem.
        let c = select_projects(&db, vec![project(9, "Portal"), project(8, "Integracje")]).unwrap();
        assert_eq!(
            c.projects.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![9, 8]
        );
        assert_eq!(read_config(&db).unwrap().projects.len(), 2);
        // Pusta lista czyści wybór.
        let c = select_projects(&db, vec![]).unwrap();
        assert!(c.projects.is_empty());
        assert!(read_config(&db).unwrap().projects.is_empty());
        // Walidacja: id 0, pusta nazwa, za dużo.
        assert!(select_projects(&db, vec![project(0, "Zero")]).is_err());
        assert!(select_projects(&db, vec![project(1, "  ")]).is_err());
        let many: Vec<_> = (1..=51).map(|i| project(i, "P")).collect();
        assert!(select_projects(&db, many).is_err());
        // Brak konfiguracji w bazie to konfiguracja domyślna (bez adresu).
        let fresh = memory();
        let c = read_config(&fresh).unwrap();
        assert!(c.site.is_empty() && c.login.is_empty() && c.projects.is_empty());
        assert!(!c.accept_invalid_cert && c.user_id == 0);
        // Stara konfiguracja bez nowych pól czyta się z wartościami domyślnymi.
        fresh
            .execute(
                "INSERT INTO settings(key,value) VALUES('redmine','{\"site\":\"https://r.example.pl\"}')",
                [],
            )
            .unwrap();
        assert_eq!(read_config(&fresh).unwrap().site, "https://r.example.pl");
    }
    #[test]
    fn paging_follows_total_count_and_stops_at_page_limit() {
        fn page(offset: u64, total: u64) -> Value {
            let n = (total.saturating_sub(offset)).min(PAGE);
            let issues: Vec<Value> = (0..n)
                .map(|i| serde_json::json!({"id": offset + i + 1, "subject": "x"}))
                .collect();
            serde_json::json!({"issues": issues, "total_count": total, "offset": offset, "limit": PAGE})
        }
        // 250 zgłoszeń: 3 strony (0, 100, 200), wszystko pobrane.
        let mut offsets = Vec::new();
        let out =
            tauri::async_runtime::block_on(paged("issues", "brak", MAX_ISSUE_PAGES, |offset| {
                offsets.push(offset);
                async move { Ok(page(offset, 250)) }
            }))
            .unwrap();
        assert_eq!(offsets, vec![0, 100, 200]);
        assert_eq!(out.rows.len(), 250);
        assert!(out.complete);
        assert_eq!(out.total, 250);
        // 1000 zgłoszeń: limit 5 stron, 500 wierszy i ostrzeżenie.
        let mut calls = 0;
        let out =
            tauri::async_runtime::block_on(paged("issues", "brak", MAX_ISSUE_PAGES, |offset| {
                calls += 1;
                async move { Ok(page(offset, 1000)) }
            }))
            .unwrap();
        assert_eq!(calls, 5);
        assert_eq!(out.rows.len(), 500);
        assert!(!out.complete);
        let warning = truncated_warning(out.rows.len(), out.total);
        assert!(
            warning.starts_with("Pokazano pierwsze 500 z 1000 zgłoszeń."),
            "{warning}"
        );
        // Dokładnie 100: jedna strona, bo offset == total_count.
        let mut calls = 0;
        let out =
            tauri::async_runtime::block_on(paged("issues", "brak", MAX_ISSUE_PAGES, |offset| {
                calls += 1;
                async move { Ok(page(offset, 100)) }
            }))
            .unwrap();
        assert_eq!((calls, out.rows.len()), (1, 100));
        // Pusta lista i brak listy.
        let out =
            tauri::async_runtime::block_on(paged("issues", "brak", MAX_ISSUE_PAGES, |_| async {
                Ok(serde_json::json!({"issues": [], "total_count": 0}))
            }))
            .unwrap();
        assert!(out.rows.is_empty() && out.complete);
        let err = tauri::async_runtime::block_on(paged(
            "issues",
            "brak listy",
            MAX_ISSUE_PAGES,
            |_| async { Ok(serde_json::json!({"projects": []})) },
        ))
        .unwrap_err();
        assert_eq!(err, "brak listy");
        // Błąd strony przerywa pobieranie.
        let err =
            tauri::async_runtime::block_on(paged("issues", "brak", MAX_ISSUE_PAGES, |_| async {
                Err::<Value, String>("sieć".into())
            }))
            .unwrap_err();
        assert_eq!(err, "sieć");
    }
    #[test]
    fn builds_issue_queries() {
        assert_eq!(
            issues_query(Some(7), true, true, None),
            vec![
                ("project_id", "7".to_string()),
                ("assigned_to_id", "me".to_string()),
                ("status_id", "open".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        assert_eq!(
            issues_query(None, true, false, Some("2026-10-06")),
            vec![
                ("assigned_to_id", "me".to_string()),
                ("status_id", "*".to_string()),
                ("updated_on", ">=2026-10-06".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        assert_eq!(
            issues_query(Some(9), false, false, None),
            vec![
                ("project_id", "9".to_string()),
                ("status_id", "*".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        assert_eq!(
            ids_query(&[1, 2, 0, 3]),
            vec![
                ("issue_id", "1,2,3".to_string()),
                ("status_id", "*".to_string())
            ]
        );
    }
    #[test]
    fn validates_site_issue_ids_and_links() {
        assert_eq!(
            normalize_site("https://redmine.example.pl/").unwrap(),
            "https://redmine.example.pl"
        );
        assert_eq!(
            normalize_site(" http://redmine.local "),
            Ok("http://redmine.local".into())
        );
        assert_eq!(
            normalize_site("https://firma.pl/redmine/").unwrap(),
            "https://firma.pl/redmine"
        );
        assert_eq!(
            normalize_site("https://firma.pl/redmine").unwrap(),
            "https://firma.pl/redmine"
        );
        assert_eq!(
            normalize_site("http://10.0.0.5:3000").unwrap(),
            "http://10.0.0.5:3000"
        );
        // Ustawienia → Redmine (R5): adres z wielopoziomowym prefiksem ścieżki
        // i portem, z końcowymi ukośnikami; link „Gdzie znaleźć klucz API”
        // i zgłoszenia pod tym prefiksem.
        let prefixed = normalize_site(" https://intranet.example.pl:8443/apps/redmine// ").unwrap();
        assert_eq!(prefixed, "https://intranet.example.pl:8443/apps/redmine");
        for link in ["/my/account", "/issues/1234"] {
            let url = Url::parse(&format!("{prefixed}{link}")).unwrap();
            assert!(allowed_link(&url, &prefixed), "{link}");
            assert!(!allowed_link(
                &Url::parse(&format!("https://intranet.example.pl:8443{link}")).unwrap(),
                &prefixed
            ));
        }
        assert!(account_link(
            &Url::parse("https://intranet.example.pl:8443/apps/redmine/my/account").unwrap()
        ));
        assert_eq!(
            normalize_site("HTTPS://Redmine.Example.PL/Redmine").unwrap(),
            "https://redmine.example.pl/Redmine"
        );
        for bad in [
            "",
            "redmine.example.pl",
            "ftp://redmine.example.pl",
            "https://",
            "https://redmine.example.pl/?key=1",
            "https://redmine.example.pl?x=1",
            "https://redmine.example.pl/#top",
            "https://user:pass@redmine.example.pl",
            "https://user@redmine.example.pl",
            "mailto:a@b",
        ] {
            assert!(normalize_site(bad).is_err(), "{bad}");
        }
        assert!(valid_issue_id(1));
        assert!(valid_issue_id(9_999_999_999));
        assert!(!valid_issue_id(0));
        assert!(!valid_issue_id(10_000_000_000));
        assert!(valid_api_key("abcdef0123456789"));
        assert!(valid_api_key(&"a".repeat(128)));
        assert!(!valid_api_key(""));
        assert!(!valid_api_key(&"a".repeat(129)));
        assert!(!valid_api_key("abc def"));
        assert!(!valid_api_key("abc\tdef"));
        assert!(!valid_api_key("klucz-ą"));
        let link = |u: &str, site: &str| allowed_link(&Url::parse(u).unwrap(), site);
        assert!(link("https://redmine.example.pl/issues/1234", SITE));
        assert!(link("https://redmine.example.pl/my/account", SITE));
        assert!(link(
            "https://firma.pl/redmine/issues/1",
            "https://firma.pl/redmine"
        ));
        assert!(link(
            "http://10.0.0.5:3000/issues/7",
            "http://10.0.0.5:3000"
        ));
        assert!(!link(
            "https://firma.pl/issues/1",
            "https://firma.pl/redmine"
        ));
        assert!(!link(
            "https://firma.pl/redmineX/issues/1",
            "https://firma.pl/redmine"
        ));
        assert!(!link("https://redmine.example.pl/issues/1234?x=1", SITE));
        assert!(!link("https://redmine.example.pl/issues/1234#note-3", SITE));
        assert!(!link("https://redmine.example.pl/issues/abc", SITE));
        assert!(!link("https://redmine.example.pl/issues/0", SITE));
        assert!(!link("https://redmine.example.pl/issues/", SITE));
        assert!(!link("https://redmine.example.pl/projects/wms", SITE));
        assert!(!link("http://redmine.example.pl/issues/1234", SITE));
        assert!(!link("https://redmine.example.pl:8443/issues/1234", SITE));
        assert!(!link("https://evil.example.pl/issues/1234", SITE));
        assert!(!link("https://user@redmine.example.pl/issues/1234", SITE));
        assert!(!link("https://redmine.example.pl/issues/1234", ""));
        // „Gdzie znaleźć klucz API” przed połączeniem: tylko `/my/account`.
        let account = |u: &str| account_link(&Url::parse(u).unwrap());
        assert!(account("https://firma.pl/redmine/my/account"));
        assert!(account("http://redmine.local/my/account"));
        assert!(!account("https://firma.pl/redmine/my/account?x=1"));
        assert!(!account("https://firma.pl/redmine/issues/1"));
        assert!(!account("ftp://firma.pl/my/account"));
    }
    #[test]
    fn same_site_and_issue_pages_respect_host_port_and_path_prefix() {
        // Ten sam Redmine: schemat i końcowy `/` bez znaczenia, host bez wielkości liter.
        assert!(same_site(
            "https://redmine.example.pl",
            "http://Redmine.example.pl/"
        ));
        assert!(same_site(
            "https://firma.pl/redmine",
            "https://firma.pl/redmine/"
        ));
        assert!(!same_site("https://firma.pl/redmine", "https://firma.pl"));
        assert!(!same_site(
            "https://firma.pl/redmine",
            "https://firma.pl/redmine2"
        ));
        assert!(!same_site(
            "https://redmine.example.pl",
            "https://redmine.example.pl:8443"
        ));
        assert!(!same_site(
            "https://redmine.example.pl",
            "https://inny.example.pl"
        ));
        assert!(!same_site("", ""));
        assert!(!same_site("zły adres", "zły adres"));
        // Strona zgłoszenia: tylko `<prefiks>/issues/<numer>[/...]`.
        let page = |u: &str, site: &str| issue_page(&Url::parse(u).unwrap(), site);
        assert_eq!(
            page(
                "https://firma.pl/redmine/issues/42",
                "https://firma.pl/redmine"
            ),
            Some(42)
        );
        assert_eq!(
            page(
                "http://firma.pl/redmine/issues/42/edit",
                "https://firma.pl/redmine"
            ),
            Some(42)
        );
        assert_eq!(
            page(
                "https://firma.pl/gitlab/g/p/-/issues/42",
                "https://firma.pl/redmine"
            ),
            None
        );
        assert_eq!(
            page("https://firma.pl/issues/42", "https://firma.pl/redmine"),
            None
        );
        assert_eq!(
            page(
                "https://firma.pl/redmine/issues/0",
                "https://firma.pl/redmine"
            ),
            None
        );
        assert_eq!(
            page(
                "https://firma.pl/redmine/issues/x1",
                "https://firma.pl/redmine"
            ),
            None
        );
        assert_eq!(page("https://firma.pl/redmine/issues/42", ""), None);
        assert_eq!(
            page(
                "ftp://firma.pl/redmine/issues/42",
                "https://firma.pl/redmine"
            ),
            None
        );
    }
    #[test]
    fn start_issue_creates_restores_and_switches_measurement() {
        let mut db = memory();
        db::change(&mut db, "add", None, Some("Lokalne".into()), 0).unwrap();
        db::change(&mut db, "start", Some(1), None, 100).unwrap();
        // Nowe zadanie: tytuł = temat, poprzedni pomiar zatrzymany, nowy wystartowany.
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 200).unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.tasks[0].title, "Eksport faktur");
        assert_eq!(s.tasks[0].jira_key, None);
        let active: Vec<_> = s.sessions.iter().filter(|x| x.ended_at.is_none()).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].task_id, s.tasks[0].id);
        assert_eq!(
            s.sessions.iter().find(|x| x.task_id == 1).unwrap().ended_at,
            Some(200)
        );
        let linked: (String, i64) = db
            .query_row(
                "SELECT site, issue_id FROM redmine_tasks WHERE task_id=?1",
                [s.tasks[0].id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(linked, (SITE.to_string(), 1234));
        let id = s.tasks[0].id;
        // Ponowny start tego samego zgłoszenia nie dubluje zadania ani pomiaru.
        let s = start_issue(&mut db, SITE, 1234, "Inny temat", 300).unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.tasks[0].title, "Eksport faktur");
        assert_eq!(s.sessions.len(), 2);
        // Ukończone wraca do zrobienia.
        db::change(&mut db, "toggle", Some(id), None, 400).unwrap();
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 500).unwrap();
        assert!(!s.tasks[0].done);
        assert_eq!(s.sessions.len(), 3);
        // Z kosza.
        db::change(&mut db, "delete", Some(id), None, 600).unwrap();
        assert_eq!(db::snapshot(&db).unwrap().trash.len(), 1);
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 700).unwrap();
        assert!(s.trash.is_empty());
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.sessions.len(), 4);
        // Z archiwum.
        db::change(&mut db, "archive", Some(id), None, 800).unwrap();
        assert!(
            db::snapshot(&db)
                .unwrap()
                .tasks
                .iter()
                .find(|t| t.id == id)
                .unwrap()
                .archived
        );
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 900).unwrap();
        let task = s.tasks.iter().find(|t| t.id == id).unwrap();
        assert!(!task.archived);
        let active: Vec<_> = s.sessions.iter().filter(|x| x.ended_at.is_none()).collect();
        assert_eq!((active.len(), active[0].task_id), (1, id));
        // Ten sam numer pod innym adresem to osobne zadanie; zły numer i pusty temat.
        let s = start_issue(&mut db, "https://inny.example.pl", 1234, "  ", 1000).unwrap();
        assert_eq!(s.tasks.len(), 3);
        assert_eq!(s.tasks[0].title, "#1234");
        assert!(start_issue(&mut db, SITE, 0, "x", 1100).is_err());
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM redmine_tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        // Usunięcie zadania na stałe kasuje powiązanie (ON DELETE CASCADE).
        db.execute("DELETE FROM tasks WHERE id=?1", [id]).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM redmine_tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn saves_issues_statuses_and_priority_band() {
        let mut db = memory();
        let issues = fixture_issues();
        let statuses = parse_statuses(&serde_json::from_str(STATUSES).unwrap()).unwrap();
        let tx = db.transaction().unwrap();
        save_issues(&tx, SITE, &issues).unwrap();
        save_statuses(&tx, SITE, &statuses, 1_000).unwrap();
        tx.commit().unwrap();
        let row: (String, String, i64, bool, String, String, String, String, i64) = db
            .query_row(
                "SELECT subject,status,status_id,closed,project,tracker,priority,due_date,assigned FROM redmine_issues WHERE site=?1 AND issue_id=1234",
                [SITE],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "Eksport faktur – błąd przy polskich znakach (ąęśćżźńół)".into(),
                "New".into(),
                1,
                false,
                "Magazyn WMS".into(),
                "Błąd".into(),
                "High".into(),
                "2026-10-30".into(),
                0
            )
        );
        let closed: bool = db
            .query_row(
                "SELECT closed FROM redmine_issues WHERE site=?1 AND issue_id=1301",
                [SITE],
                |r| r.get(0),
            )
            .unwrap();
        assert!(closed);
        assert_eq!(read_statuses(&db, SITE).unwrap(), statuses);
        assert!(read_statuses(&db, "https://inny.example.pl")
            .unwrap()
            .is_empty());
        assert!(statuses_fresh(&db, SITE, 1_000 + STATUS_TTL - 1).unwrap());
        assert!(!statuses_fresh(&db, SITE, 1_000 + STATUS_TTL).unwrap());
        assert!(!statuses_fresh(&db, SITE, 500).unwrap());
        assert!(!statuses_fresh(&db, "https://inny.example.pl", 2_000).unwrap());
        // Ponowny zapis zastępuje listę statusów (bez dubli), zachowując kolejność.
        let fewer = vec![
            Status {
                id: 2,
                name: "W toku".into(),
                is_closed: false,
            },
            Status {
                id: 1,
                name: "Nowy".into(),
                is_closed: false,
            },
        ];
        let tx = db.transaction().unwrap();
        save_statuses(&tx, SITE, &fewer, 2_000).unwrap();
        tx.commit().unwrap();
        assert_eq!(read_statuses(&db, SITE).unwrap(), fewer);
        // „Rozpocznij” z pobranym priorytetem: nowe zadanie z pasmem „wysoki”.
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 100).unwrap();
        assert_eq!(s.tasks[0].details.priority, 1);
        // Zmiana priorytetu w Redmine przenosi pasmo na istniejące zadanie.
        let mut low = issues[0].clone();
        low.priority = "Low".into();
        low.status = "In Progress".into();
        let tx = db.transaction().unwrap();
        save_issues(&tx, SITE, &[low]).unwrap();
        tx.commit().unwrap();
        let s = db::snapshot(&db).unwrap();
        assert_eq!(s.tasks[0].details.priority, -1);
        let status: String = db
            .query_row(
                "SELECT status FROM redmine_issues WHERE site=?1 AND issue_id=1234",
                [SITE],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "In Progress");
        // Ręczna zmiana pasma zostaje, dopóki priorytet w Redmine się nie zmieni.
        db.execute(
            "UPDATE task_details SET json=json_set(json,'$.priority',0) WHERE task_id=?1",
            [s.tasks[0].id],
        )
        .unwrap();
        let mut same = issues[0].clone();
        same.priority = "Low".into();
        let tx = db.transaction().unwrap();
        save_issues(&tx, SITE, &[same]).unwrap();
        tx.commit().unwrap();
        assert_eq!(db::snapshot(&db).unwrap().tasks[0].details.priority, 0);
        // Wiersze Jiry nietknięte.
        let jira: i64 = db
            .query_row("SELECT COUNT(*) FROM jira_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(jira, 0);
    }
    #[test]
    fn status_after_write_keeps_newer_fields_and_manual_band() {
        let mut db = memory();
        let issues = fixture_issues();
        let tx = db.transaction().unwrap();
        save_issues(&tx, SITE, &issues[..1]).unwrap();
        tx.commit().unwrap();
        let s = start_issue(&mut db, SITE, 1234, "Eksport faktur", 100).unwrap();
        assert_eq!(s.tasks[0].details.priority, 1);
        // Ręczna zmiana pasma po obserwacji (High w redmine_issues).
        db.execute(
            "UPDATE task_details SET json=json_set(json,'$.priority',0) WHERE task_id=?1",
            [s.tasks[0].id],
        )
        .unwrap();
        // Zapis statusu bez odczytu po zapisie zmienia tylko pola statusu.
        let done = Status {
            id: 5,
            name: "Closed".into(),
            is_closed: true,
        };
        save_status(&db, SITE, 1234, &done).unwrap();
        let row: (String, i64, bool, String, String, String) = db
            .query_row(
                "SELECT status,status_id,closed,subject,priority,due_date FROM redmine_issues WHERE site=?1 AND issue_id=1234",
                [SITE],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "Closed".into(),
                5,
                true,
                "Eksport faktur – błąd przy polskich znakach (ąęśćżźńół)".into(),
                "High".into(),
                "2026-10-30".into()
            )
        );
        assert_eq!(db::snapshot(&db).unwrap().tasks[0].details.priority, 0);
        // Inny adres — bez zmian.
        save_status(&db, "https://inny.example.pl", 1234, &done).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM redmine_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn status_errors_are_polish_without_secrets() {
        let e = |s: u16, body: &str| {
            parse_response(StatusCode::from_u16(s).unwrap(), false, body).unwrap_err()
        };
        assert_eq!(
            e(401, "{}"),
            "Redmine odrzucił klucz API. Sprawdź klucz w „Moje konto” w Redmine."
        );
        assert_eq!(e(403, ""), "Brak uprawnień w Redmine do tej operacji.");
        assert_eq!(
            e(404, "<html><body>Not found</body></html>"),
            "Nie znaleziono zgłoszenia albo adres Redmine jest błędny (REST API może być wyłączone)."
        );
        assert_eq!(
            e(
                422,
                r#"{"errors":["Status is invalid","Assignee cannot be blank"]}"#
            ),
            "Redmine odrzucił dane: Status is invalid; Assignee cannot be blank"
        );
        assert_eq!(e(422, ""), "Redmine odrzucił dane.");
        assert_eq!(e(422, "<html>x</html>"), "Redmine odrzucił dane.");
        let long = format!(r#"{{"errors":["{}"]}}"#, "x".repeat(500));
        assert_eq!(
            e(422, &long).chars().count(),
            "Redmine odrzucił dane: ".chars().count() + 200
        );
        assert_eq!(
            e(429, ""),
            "Redmine ograniczył liczbę zapytań. Spróbuj ponownie za chwilę."
        );
        assert_eq!(
            e(500, ""),
            "Redmine zwrócił błąd HTTP 500. Spróbuj ponownie."
        );
        assert_eq!(
            e(503, ""),
            "Redmine zwrócił błąd HTTP 503. Spróbuj ponownie."
        );
        assert!(e(302, "").starts_with("Redmine przekierował żądanie."));
        assert_eq!(e(418, ""), "Redmine zwrócił błąd HTTP 418.");
        // 200 ze stroną logowania zamiast JSON (API wyłączone).
        let html = "<!DOCTYPE html>\n<html><head><title>Redmine</title></head></html>";
        assert_eq!(
            parse_response(StatusCode::OK, false, html).unwrap_err(),
            HTML_ERROR
        );
        assert_eq!(
            parse_response(StatusCode::OK, true, "nie json").unwrap_err(),
            HTML_ERROR
        );
        assert_eq!(
            parse_response(StatusCode::OK, false, "nie json").unwrap_err(),
            BAD_RESPONSE
        );
        assert_eq!(
            parse_response(StatusCode::OK, false, "null").unwrap_err(),
            BAD_RESPONSE
        );
        assert_eq!(
            parse_response(StatusCode::OK, false, r#"{"user":{"id":1}}"#).unwrap()["user"]["id"],
            1
        );
        // Błąd certyfikatu rozpoznawany w łańcuchu przyczyn.
        #[derive(Debug)]
        struct Wrapped(std::io::Error);
        impl std::fmt::Display for Wrapped {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "error sending request")
            }
        }
        impl std::error::Error for Wrapped {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let cert = Wrapped(std::io::Error::other(
            "invalid peer certificate: UnknownIssuer",
        ));
        assert!(certificate_in_chain(&cert));
        let refused = Wrapped(std::io::Error::other("connection refused"));
        assert!(!certificate_in_chain(&refused));
        assert!(CERT_ERROR.contains("Zaawansowane"));
    }
    /// Serwer HTTP na jedno połączenie (127.0.0.1): zwraca otrzymane żądanie.
    fn serve_once(
        status: &'static str,
        content_type: &'static str,
        body: String,
    ) -> (String, std::thread::JoinHandle<String>) {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            write!(socket,"HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), server)
    }
    #[test]
    fn get_sends_api_key_header_and_maps_html_page() {
        let (site, server) = serve_once("200 OK", "application/json; charset=utf-8", USER.into());
        let c = Config {
            site,
            ..Config::default()
        };
        let value = tauri::async_runtime::block_on(get(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            "/users/current.json",
            &[
                ("limit", "100".into()),
                ("updated_on", ">=2026-10-06".into()),
            ],
        ))
        .unwrap();
        assert_eq!(value["user"]["login"], "mkrajnik");
        let request = server.join().unwrap();
        assert!(
            request.starts_with("GET /users/current.json?limit=100&updated_on=%3E%3D2026-10-06 "),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("x-redmine-api-key: test-only-key\r\n"));
        assert!(lower.contains("accept: application/json\r\n"));
        assert!(!lower.contains("authorization:"));
        // Strona logowania zamiast JSON przy wyłączonym API.
        let (site, server) = serve_once(
            "200 OK",
            "text/html; charset=utf-8",
            "<!DOCTYPE html><html><body>Zaloguj się</body></html>".into(),
        );
        let c = Config {
            site,
            ..Config::default()
        };
        let e = tauri::async_runtime::block_on(get(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            "/users/current.json",
            &[],
        ))
        .unwrap_err();
        assert_eq!(e, HTML_ERROR);
        server.join().unwrap();
        // 401 bez klucza w komunikacie.
        let (site, server) = serve_once("401 Unauthorized", "application/json", "{}".into());
        let c = Config {
            site,
            ..Config::default()
        };
        let e = tauri::async_runtime::block_on(get(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            "/issues.json",
            &[],
        ))
        .unwrap_err();
        assert!(e.starts_with("Redmine odrzucił klucz API."));
        assert!(!e.contains("test-only-key"));
        server.join().unwrap();
        // Zamknięty port: błąd sieci bez adresu i bez klucza.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let c = Config {
            site: format!("http://{address}"),
            ..Config::default()
        };
        let e = tauri::async_runtime::block_on(get(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            "/issues.json",
            &[],
        ))
        .unwrap_err();
        assert_eq!(e, NETWORK_ERROR);
    }
    #[test]
    fn fetch_issues_reads_pages_from_local_server() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for page in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                let request = String::from_utf8(request).unwrap();
                paths.push(request.lines().next().unwrap().to_string());
                let issues: Vec<Value> = (0..if page == 0 { 100 } else { 20 })
                    .map(|i| serde_json::json!({"id": page * 100 + i + 1, "subject": format!("Z {}", i), "status": {"id": 1, "name": "New"}}))
                    .collect();
                let body = serde_json::json!({"issues": issues, "total_count": 120, "offset": page * 100, "limit": 100}).to_string();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
            paths
        });
        let c = Config {
            site: format!("http://{address}"),
            ..Config::default()
        };
        let closed: HashMap<u64, bool> = [(1, false)].into_iter().collect();
        let (issues, warning) = tauri::async_runtime::block_on(fetch_issues(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            &issues_query(Some(7), true, true, None),
            &closed,
        ))
        .unwrap();
        assert_eq!(issues.len(), 120);
        assert_eq!(warning, None);
        assert_eq!(issues[0].url, format!("http://{address}/issues/1"));
        let paths = server.join().unwrap();
        assert_eq!(
            paths[0],
            "GET /issues.json?project_id=7&assigned_to_id=me&status_id=open&sort=updated_on%3Adesc&limit=100&offset=0 HTTP/1.1"
        );
        assert!(
            paths[1].ends_with("&limit=100&offset=100 HTTP/1.1"),
            "{}",
            paths[1]
        );
    }
    /// `project_id` dla listy: puste i dodatnie przechodzą, `0` nie trafia do
    /// serwera (Redmine odpowiedziałby mylącym 404).
    #[test]
    fn rejects_zero_project_id() {
        assert_eq!(check_project(None), Ok(()));
        assert_eq!(check_project(Some(7)), Ok(()));
        assert_eq!(check_project(Some(0)), Err(BAD_PROJECT.to_string()));
    }
    /// Pojedyncze zgłoszenie (szybkie dodawanie, rozszerzenie) z Redmine bez
    /// `status.is_closed`: zamknięcie z mapy statusów, jak przy listach.
    #[test]
    fn fetch_issue_uses_status_map_and_reports_missing_issue() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let mut paths = Vec::new();
            for _ in 0..3 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                let request = String::from_utf8(request).unwrap();
                let path = request.lines().next().unwrap().to_string();
                let (status, body) = if path.starts_with("GET /issues/1236.json") {
                    (
                        "200 OK",
                        serde_json::json!({"issue": {"id": 1236, "subject": "Zamknięte", "status": {"id": 5, "name": "Closed"}}}).to_string(),
                    )
                } else {
                    (
                        "404 Not Found",
                        serde_json::json!({"errors": ["Not found"]}).to_string(),
                    )
                };
                paths.push(path);
                write!(socket,"HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",status,body.len(),body).unwrap();
            }
            paths
        });
        let c = Config {
            site: format!("http://{address}"),
            ..Config::default()
        };
        let client = client(&c).unwrap();
        let closed: HashMap<u64, bool> = [(1, false), (5, true)].into_iter().collect();
        let issue = tauri::async_runtime::block_on(fetch_issue(
            &client,
            &c,
            "test-only-key",
            1236,
            &closed,
        ))
        .unwrap();
        assert_eq!((issue.id, issue.subject.as_str()), (1236, "Zamknięte"));
        assert_eq!((issue.status.as_str(), issue.status_id), ("Closed", 5));
        assert!(issue.closed, "zamknięcie z mapy statusów");
        assert_eq!(issue.url, format!("http://{address}/issues/1236"));
        // Bez mapy (pusta pamięć podręczna) zgłoszenie uchodzi za otwarte.
        let open = tauri::async_runtime::block_on(fetch_issue(
            &client,
            &c,
            "test-only-key",
            1236,
            &HashMap::new(),
        ))
        .unwrap();
        assert!(!open.closed);
        // Nieistniejący numer: komunikat 404 bez klucza API.
        let err = tauri::async_runtime::block_on(fetch_issue(
            &client,
            &c,
            "test-only-key",
            999_999,
            &closed,
        ))
        .unwrap_err();
        assert_eq!(
            err,
            "Nie znaleziono zgłoszenia albo adres Redmine jest błędny (REST API może być wyłączone)."
        );
        assert!(!err.contains("test-only-key"));
        let paths = server.join().unwrap();
        assert_eq!(paths[0], "GET /issues/1236.json HTTP/1.1");
        assert_eq!(paths[2], "GET /issues/999999.json HTTP/1.1");
    }

    // --- R3: widok (grupy statusów, projekty) i obserwacja przypisań --------

    fn issue(id: u64, status: &str, closed: bool, priority: &str) -> Issue {
        Issue {
            id,
            subject: format!("Zgłoszenie {id}"),
            project_id: 7,
            project: "Magazyn WMS".into(),
            tracker: "Błąd".into(),
            status: status.into(),
            status_id: match status {
                "New" => 1,
                "In Progress" => 2,
                "Feedback" => 3,
                "Resolved" => 4,
                "Closed" => 5,
                _ => 0,
            },
            closed,
            priority: priority.into(),
            assigned_to_id: Some(12),
            assigned_to: "Mateusz Krajnik".into(),
            version: String::new(),
            category: String::new(),
            due_date: String::new(),
            updated_on: String::new(),
            url: format!("{SITE}/issues/{id}"),
        }
    }
    fn fixture_statuses() -> Vec<Status> {
        parse_statuses(&serde_json::from_str(STATUSES).unwrap()).unwrap()
    }

    #[test]
    fn watch_and_view_queries() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
        let [open, recent] = watch_queries(today);
        // Moje otwarte ze wszystkich projektów (bez `project_id`).
        assert_eq!(
            open,
            vec![
                ("assigned_to_id", "me".to_string()),
                ("status_id", "open".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        // Moje w dowolnym statusie zmienione w ostatnich 14 dniach.
        assert_eq!(
            recent,
            vec![
                ("assigned_to_id", "me".to_string()),
                ("status_id", "*".to_string()),
                ("updated_on", ">=2026-10-06".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        // Widok: projekt i „Moje” (domyślnie) albo „Wszystkie otwarte”.
        assert_eq!(
            issues_query(Some(7), false, true, None),
            vec![
                ("project_id", "7".to_string()),
                ("status_id", "open".to_string()),
                ("sort", "updated_on:desc".to_string()),
            ]
        );
        // Paczki po 100 numerów dla zgłoszeń przekazanych innym.
        let ids: Vec<u64> = (1..=250).collect();
        let chunks: Vec<_> = ids.chunks(PAGE as usize).map(ids_query).collect();
        assert_eq!(chunks.len(), 3);
        assert_eq!(
            chunks[2][0].1,
            (201..=250)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        // Powtórki z dwóch zapytań obserwacji są pomijane.
        let merged = merge_assigned(
            vec![issue(1, "New", false, ""), issue(2, "New", false, "")],
            vec![issue(2, "New", false, ""), issue(3, "Closed", true, "")],
        );
        assert_eq!(
            merged.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn groups_issues_by_status_in_redmine_order() {
        let statuses = fixture_statuses();
        let mut due = issue(4, "New", false, "Normal");
        due.due_date = "2026-10-30".into();
        let mut earlier = issue(5, "New", false, "Normal");
        earlier.due_date = "2026-10-21".into();
        let mut custom = issue(9, "Oczekuje na klienta", false, "");
        custom.status_id = 42;
        let mut nameless = issue(10, "Feedback", false, "");
        nameless.status_id = 0;
        let issues = vec![
            issue(1, "Closed", true, "Low"),
            issue(2, "In Progress", false, "Normal"),
            custom,
            issue(3, "New", false, "Low"),
            due,
            issue(6, "New", false, "Immediate"),
            earlier,
            issue(7, "In Progress", false, "High"),
            nameless,
        ];
        let groups = group_by_status(&issues, &statuses);
        assert_eq!(
            groups
                .iter()
                .map(|g| (g.status.as_str(), g.closed, g.ids.clone()))
                .collect::<Vec<_>>(),
            vec![
                // Kolejność z `/issue_statuses.json`; w grupie priorytet, potem termin.
                ("New", false, vec![6, 5, 4, 3]),
                ("In Progress", false, vec![7, 2]),
                // Bez numeru statusu — po nazwie.
                ("Feedback", false, vec![10]),
                ("Closed", true, vec![1]),
                // Status spoza listy na końcu; puste grupy pominięte.
                ("Oczekuje na klienta", false, vec![9]),
            ]
        );
        // Bez listy statusów: kolejność pojawienia się.
        let groups = group_by_status(&issues[..3], &[]);
        assert_eq!(
            groups.iter().map(|g| g.status.as_str()).collect::<Vec<_>>(),
            vec!["Closed", "In Progress", "Oczekuje na klienta"]
        );
        assert!(group_by_status(&[], &statuses).is_empty());
    }

    #[test]
    fn tracked_projects_include_known_subprojects() {
        let value: Value = serde_json::from_str(PROJECTS).unwrap();
        let mut known = parse_projects(value["projects"].as_array().unwrap());
        known.push(Project {
            id: 10,
            name: "Integracje – EDI".into(),
            identifier: "edi".into(),
            parent_id: Some(8),
        });
        let wms = known[0].clone();
        // Projekt 7 z podprojektem 8 i jego podprojektem 10; portal (9) poza.
        let mut ids = tracked_project_ids(&[wms.clone()], &known);
        ids.sort();
        assert_eq!(ids, vec![7, 8, 10]);
        // Bez listy projektów tylko same śledzone.
        assert_eq!(tracked_project_ids(&[wms], &[]), vec![7]);
        assert!(tracked_project_ids(&[], &known).is_empty());
    }

    #[test]
    fn sync_assigned_imports_new_assignments_like_jira() {
        let mut connection = memory();
        // Pierwsze sprawdzenie: backlog zapamiętany, nic nie zaimportowane.
        let first = vec![
            issue(1, "New", false, "Normal"),
            issue(9, "Closed", true, "Normal"),
        ];
        assert!(sync_assigned(&mut connection, SITE, &first, &[], true, 100)
            .unwrap()
            .is_empty());
        assert!(db::snapshot(&connection).unwrap().tasks.is_empty());
        // Nowe przypisanie #2 trafia do zadań bez dnia w planie i bez pomiaru.
        let second = vec![
            issue(1, "In Progress", false, "Normal"),
            issue(2, "New", false, "High"),
        ];
        let imported = sync_assigned(&mut connection, SITE, &second, &[], false, 200).unwrap();
        assert_eq!(imported, vec![issue(2, "New", false, "High")]);
        let s = db::snapshot(&connection).unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].title, "Zgłoszenie 2");
        assert_eq!(s.tasks[0].redmine_id, Some(2));
        assert_eq!(s.tasks[0].redmine_status.as_deref(), Some("New"));
        assert_eq!(s.tasks[0].redmine_tracker.as_deref(), Some("Błąd"));
        assert_eq!(s.tasks[0].details.planned_day, "");
        assert_eq!(s.tasks[0].details.priority, 1, "pasmo z priorytetu");
        assert!(s.tasks[0].jira_key.is_none());
        assert!(s.sessions.is_empty());
        // Kolejne sprawdzenie nie dubluje, ale odświeża status i pasmo.
        let third = vec![
            issue(1, "In Progress", false, "Normal"),
            issue(2, "Feedback", false, "Low"),
        ];
        assert!(
            sync_assigned(&mut connection, SITE, &third, &[], false, 300)
                .unwrap()
                .is_empty()
        );
        let s = db::snapshot(&connection).unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].redmine_status.as_deref(), Some("Feedback"));
        assert_eq!(s.tasks[0].details.priority, -1);
        // Zadanie w koszu nie wraca, dopóki zgłoszenie jest przypisane.
        db::change(&mut connection, "delete", Some(s.tasks[0].id), None, 400).unwrap();
        assert!(
            sync_assigned(&mut connection, SITE, &third, &[], false, 500)
                .unwrap()
                .is_empty()
        );
        // Zdjęte z listy, przypisane ponownie: zadanie w koszu nadal blokuje
        // duplikat, a #1 (bez zadania) wraca jako nowe.
        let fourth = vec![issue(2, "Feedback", false, "Low")];
        assert!(
            sync_assigned(&mut connection, SITE, &fourth, &[], false, 600)
                .unwrap()
                .is_empty()
        );
        let again = vec![
            issue(1, "New", false, "Normal"),
            issue(2, "Feedback", false, "Low"),
        ];
        let imported = sync_assigned(&mut connection, SITE, &again, &[], false, 700).unwrap();
        assert_eq!(imported.iter().map(|i| i.id).collect::<Vec<_>>(), vec![1]);
        // Zgłoszenie z zadaniem z „Rozpocznij” (zarchiwizowanym) nie jest dublowane.
        start_issue(&mut connection, SITE, 3, "Ręcznie", 800).unwrap();
        let task3 = db::snapshot(&connection)
            .unwrap()
            .tasks
            .into_iter()
            .find(|t| t.redmine_id == Some(3))
            .unwrap()
            .id;
        db::change(&mut connection, "archive", Some(task3), None, 850).unwrap();
        let with3 = vec![issue(1, "New", false, "Normal"), issue(3, "New", false, "")];
        assert!(
            sync_assigned(&mut connection, SITE, &with3, &[], false, 900)
                .unwrap()
                .is_empty()
        );
        // Zamknięte zgłoszenie nie jest importowane.
        let closed = vec![issue(5, "Closed", true, "")];
        assert!(
            sync_assigned(&mut connection, SITE, &closed, &[], false, 1000)
                .unwrap()
                .is_empty()
        );
        // Ten sam numer pod innym adresem Redmine to inne zgłoszenie.
        let other_site = sync_assigned(
            &mut connection,
            "https://inny.example.pl",
            &[issue(1, "New", false, "")],
            &[],
            false,
            1100,
        )
        .unwrap();
        assert_eq!(other_site.len(), 1);
    }

    #[test]
    fn sync_assigned_refreshes_issues_handed_to_someone_else() {
        let mut connection = memory();
        sync_assigned(&mut connection, SITE, &[], &[], true, 100).unwrap();
        sync_assigned(
            &mut connection,
            SITE,
            &[issue(4, "New", false, "Normal")],
            &[],
            false,
            200,
        )
        .unwrap();
        // Przekazane komuś innemu: status, tracker, termin i pasmo odświeżone, bez importu.
        let mut handed = issue(4, "Resolved", false, "Urgent");
        handed.assigned_to_id = Some(3);
        handed.tracker = "Funkcja".into();
        handed.due_date = "2026-11-02".into();
        let imported = sync_assigned(&mut connection, SITE, &[], &[handed], false, 300).unwrap();
        assert!(imported.is_empty());
        let s = db::snapshot(&connection).unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].redmine_status.as_deref(), Some("Resolved"));
        assert_eq!(s.tasks[0].redmine_tracker.as_deref(), Some("Funkcja"));
        assert_eq!(s.tasks[0].redmine_due.as_deref(), Some("2026-11-02"));
        assert_eq!(s.tasks[0].details.priority, 1);
        let assigned: i64 = connection
            .query_row(
                "SELECT assigned FROM redmine_issues WHERE site=?1 AND issue_id=4",
                [SITE],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(assigned, 0);
        // Numery do pobrania: otwarte zadania poza już pobranymi; ukończone i w koszu pominięte.
        assert_eq!(
            linked_open_ids(&connection, SITE, &HashSet::new()).unwrap(),
            vec![4]
        );
        assert!(
            linked_open_ids(&connection, SITE, &[4].into_iter().collect())
                .unwrap()
                .is_empty()
        );
        let id = s.tasks[0].id;
        db::change(&mut connection, "toggle", Some(id), None, 400).unwrap();
        assert!(linked_open_ids(&connection, SITE, &HashSet::new())
            .unwrap()
            .is_empty());
        // Tabele Jiry nietknięte.
        let jira: i64 = connection
            .query_row("SELECT COUNT(*) FROM jira_issues", [], |r| r.get(0))
            .unwrap();
        assert_eq!(jira, 0);
    }

    #[test]
    fn imported_notification_for_one_and_many_issues() {
        use crate::notify::{command, perform, Action, View, PLAN_TODAY, REDMINE_START};
        let (title, body) = imported_message(&[issue(1234, "New", false, "")]);
        assert_eq!(title, "Nowe zadanie z Redmine");
        assert_eq!(
            body,
            "#1234 · Zgłoszenie 1234\nDodano do „Do zaplanowania”."
        );
        let rows: Vec<_> = (1..=5).map(|i| issue(i, "New", false, "")).collect();
        let (title, body) = imported_message(&rows);
        assert_eq!(title, "Nowe z Redmine: 5 zadań");
        assert!(body
            .starts_with("#1 · Zgłoszenie 1\n#2 · Zgłoszenie 2\n#3 · Zgłoszenie 3\ni jeszcze 2"));

        let mut connection = memory();
        db::change(&mut connection, "add", None, Some("Lokalne".into()), 0).unwrap();
        db::change(&mut connection, "start", Some(1), None, 50).unwrap();
        sync_assigned(&mut connection, SITE, &[], &[], true, 100).unwrap();
        let one = sync_assigned(
            &mut connection,
            SITE,
            &[issue(1234, "New", false, "")],
            &[],
            false,
            200,
        )
        .unwrap();
        let ids = imported_task_ids(&connection, SITE, &one).unwrap();
        let m = imported_notification(&one, &ids);
        assert_eq!(m.key, "redmine:1234");
        assert_eq!(m.title, "Nowe zadanie z Redmine");
        assert_eq!(
            m.actions,
            [
                Action::new(REDMINE_START, "Rozpocznij", ids[0]),
                Action::new(PLAN_TODAY, "Zaplanuj na dziś", ids[0]),
            ]
        );
        assert_eq!(m.actions[0].argument(), format!("redmine-start:{}", ids[0]));
        assert_eq!(
            m.open,
            View {
                section: Some("Nierozpoczęte"),
                task: Some(ids[0]),
                ..View::tasks()
            }
        );
        let many = sync_assigned(
            &mut connection,
            SITE,
            &[
                issue(1234, "New", false, ""),
                issue(1235, "New", false, ""),
                issue(1236, "New", false, ""),
            ],
            &[],
            false,
            300,
        )
        .unwrap();
        let many_ids = imported_task_ids(&connection, SITE, &many).unwrap();
        assert_eq!(many_ids.len(), 2);
        let m = imported_notification(&many, &many_ids);
        assert_eq!(m.key, "redmine:1235,1236");
        assert_eq!(m.actions.len(), 1);
        assert_eq!(
            m.actions[0].argument(),
            format!("plan-today:{},{}", many_ids[0], many_ids[1])
        );
        assert_eq!(m.open.task, None);
        // „Rozpocznij” z powiadomienia: zadanie z kosza wraca, jeden aktywny pomiar.
        db::change(&mut connection, "delete", Some(ids[0]), None, 400).unwrap();
        let out = perform(
            &mut connection,
            command(Some(&format!("redmine-start:{}", ids[0])), &View::default()),
            "2026-10-20",
            500,
        )
        .unwrap();
        assert!(out.changed && out.navigate.is_none());
        let s = db::snapshot(&connection).unwrap();
        assert!(s.trash.is_empty());
        let active: Vec<_> = s.sessions.iter().filter(|x| x.ended_at.is_none()).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].task_id, ids[0]);
        // Zadanie bez powiązania z Redmine: błąd, bez zmian.
        assert!(perform(
            &mut connection,
            command(Some("redmine-start:1"), &View::default()),
            "2026-10-20",
            600,
        )
        .is_err());
    }

    // --- R4: zapisy ----------------------------------------------------------

    const ALLOWED: &str = include_str!("../tests/fixtures/redmine_issue_allowed.json");

    fn st(id: u64, name: &str, closed: bool) -> Status {
        Status {
            id,
            name: name.into(),
            is_closed: closed,
        }
    }

    #[test]
    fn chooses_target_status_by_name_or_lists_open_ones() {
        let allowed = [
            st(1, "New", false),
            st(2, "In Progress", false),
            st(3, "Feedback", false),
            st(4, "Resolved", true),
            st(5, "Closed", true),
        ];
        // Jedno dopasowanie po nazwie (bez względu na wielkość liter i spacje).
        let (list, chosen) = choose_status(&allowed, 1, " in progress ");
        assert_eq!(chosen, Some(2));
        // Bieżący i zamknięte odfiltrowane.
        assert_eq!(list.iter().map(|s| s.id).collect::<Vec<_>>(), [2, 3]);
        // Brak dopasowania: lista do wyboru bez wybranego statusu.
        let (list, chosen) = choose_status(&allowed, 1, "W toku");
        assert_eq!(chosen, None);
        assert_eq!(list.len(), 2);
        // Nazwa zamkniętego statusu nie jest celem pytania.
        assert_eq!(choose_status(&allowed, 1, "Closed").1, None);
        // Pusty status w ustawieniach niczego nie wybiera.
        assert_eq!(choose_status(&allowed, 1, " ").1, None);
        // Same zamknięte: nie ma o co pytać.
        assert!(choose_status(&allowed[3..], 1, "Closed").0.is_empty());
        // „Przesuń do…”: wszystkie dozwolone poza bieżącym, także zamknięte.
        assert_eq!(
            transitions_for(&allowed, 3)
                .iter()
                .map(|s| s.id)
                .collect::<Vec<_>>(),
            [1, 2, 4, 5]
        );
    }

    #[test]
    fn asks_only_for_initial_open_statuses_with_switches_on() {
        let s = crate::settings::AppSettings::default();
        assert!(should_ask(&s, false, "New", false));
        assert!(should_ask(&s, false, " nowy ", false));
        // Status spoza `redmine_new_statuses`.
        assert!(!should_ask(&s, false, "In Progress", false));
        assert!(!should_ask(&s, false, "", false));
        // Zamknięte zgłoszenie (np. status „New” oznaczony jako zamknięty).
        assert!(!should_ask(&s, false, "New", true));
        // „Nie pytaj dla tego zadania”.
        assert!(!should_ask(&s, true, "New", false));
        // Własna lista statusów początkowych.
        let custom = crate::settings::AppSettings {
            redmine_new_statuses: vec!["Otwarte".into()],
            ..Default::default()
        };
        assert!(should_ask(&custom, false, "otwarte", false));
        assert!(!should_ask(&custom, false, "New", false));
        // Wyłącznik pytania i główny przełącznik zapisów Redmine.
        let off = crate::settings::AppSettings {
            redmine_ask_move: false,
            ..Default::default()
        };
        assert!(!should_ask(&off, false, "New", false));
        let read_only = crate::settings::AppSettings {
            redmine_write: false,
            ..Default::default()
        };
        assert!(!should_ask(&read_only, false, "New", false));
        // Wyłączniki Jiry nie dotyczą Redmine.
        let jira_off = crate::settings::AppSettings {
            jira_write: false,
            jira_ask_move: false,
            ..Default::default()
        };
        assert!(should_ask(&jira_off, false, "New", false));
    }

    #[test]
    fn parses_allowed_statuses_and_falls_back_to_all_statuses() {
        let statuses = parse_statuses(&serde_json::from_str(STATUSES).unwrap()).unwrap();
        let closed = closed_map(&statuses);
        let value: Value = serde_json::from_str(ALLOWED).unwrap();
        let issue = parse_issue(&value["issue"], SITE, &closed).unwrap();
        assert_eq!((issue.id, issue.status_id), (1234, 1));
        let allowed = parse_allowed(&value, &closed).unwrap();
        assert_eq!(
            allowed,
            [
                st(1, "New", false),
                st(2, "In Progress", false),
                st(3, "Feedback", false),
                st(5, "Closed", true),
            ]
        );
        assert_eq!(allowed_or_all(Some(allowed.clone()), &statuses), allowed);
        // Redmine przed 5.0: brak pola → wszystkie statusy z `redmine_statuses`.
        let mut old = value.clone();
        old["issue"]
            .as_object_mut()
            .unwrap()
            .remove("allowed_statuses");
        assert_eq!(parse_allowed(&old, &closed), None);
        assert_eq!(
            allowed_or_all(parse_allowed(&old, &closed), &statuses),
            statuses
        );
        // Pusta lista to brak dozwolonych zmian (np. brak uprawnień), nie fallback.
        let mut none = value.clone();
        none["issue"]["allowed_statuses"] = serde_json::json!([]);
        assert!(allowed_or_all(parse_allowed(&none, &closed), &statuses).is_empty());
        // `is_closed` z mapy statusów, gdy serwer go nie podaje.
        let mut bare = value;
        bare["issue"]["allowed_statuses"] =
            serde_json::json!([{"id": 4, "name": "Resolved"}, {"id": 0, "name": "x"}, {"id": 6}]);
        assert_eq!(
            parse_allowed(&bare, &closed).unwrap(),
            [st(4, "Resolved", true)]
        );
    }

    #[test]
    fn builds_put_bodies_for_status_assignment_and_notes() {
        assert_eq!(
            status_body(2).unwrap(),
            serde_json::json!({"issue": {"status_id": 2}})
        );
        assert_eq!(status_body(0).unwrap_err(), BAD_STATUS);
        assert_eq!(
            assign_body(12).unwrap(),
            serde_json::json!({"issue": {"assigned_to_id": 12}})
        );
        assert_eq!(assign_body(0).unwrap_err(), NO_USER);
        assert_eq!(
            notes_body("  Zażółć\r\ngęślą jaźń\u{7}\n\n").unwrap(),
            serde_json::json!({"issue": {"notes": "Zażółć\ngęślą jaźń"}})
        );
        assert_eq!(notes_body(" \n ").unwrap_err(), "Wpisz treść komentarza.");
        assert!(notes_body(&"ą".repeat(NOTES_LIMIT)).is_ok());
        assert_eq!(
            notes_body(&"ą".repeat(NOTES_LIMIT + 1)).unwrap_err(),
            "Komentarz może mieć najwyżej 4000 znaków."
        );
        // Ścieżka zapisu tylko do zgłoszenia.
        assert_eq!(issue_path(1234).unwrap(), "/issues/1234.json");
        assert_eq!(issue_path(0).unwrap_err(), BAD_ISSUE);
        assert!(valid_write_path("/issues/1234.json"));
        for bad in [
            "/issues/0.json",
            "/issues/abc.json",
            "/issues/1234",
            "/issues/1234.json?x=1",
            "/users/1.json",
            "/issues/../users/1.json",
            "/issues/12345678901.json",
        ] {
            assert!(!valid_write_path(bad), "{bad}");
        }
    }

    #[test]
    fn write_responses_map_success_422_and_html() {
        assert_eq!(
            write_response(StatusCode::NO_CONTENT, false, ""),
            Ok(Value::Null)
        );
        assert_eq!(write_response(StatusCode::OK, false, "  "), Ok(Value::Null));
        assert_eq!(
            write_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                false,
                r#"{"errors":["Status is invalid"]}"#
            )
            .unwrap_err(),
            "Redmine odrzucił dane: Status is invalid"
        );
        assert_eq!(
            write_response(StatusCode::OK, true, "<html>Logowanie</html>").unwrap_err(),
            HTML_ERROR
        );
        assert_eq!(
            write_response(StatusCode::FORBIDDEN, false, "").unwrap_err(),
            "Brak uprawnień w Redmine do tej operacji."
        );
    }

    /// Serwer HTTP na jedno połączenie z treścią żądania (`Content-Length`).
    fn serve_write(
        status: &'static str,
        body: &'static str,
    ) -> (String, std::thread::JoinHandle<String>) {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            let head_end = loop {
                if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i + 4;
                }
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            };
            let head = String::from_utf8_lossy(&request[..head_end]).to_ascii_lowercase();
            let length: usize = head
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .map(|v| v.trim().parse().unwrap())
                .unwrap_or(0);
            while request.len() < head_end + length {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), server)
    }

    #[test]
    fn send_write_puts_json_with_api_key_and_maps_422() {
        let (site, server) = serve_write("204 No Content", "");
        let c = Config {
            site,
            ..Config::default()
        };
        let body = notes_body("Komentarz z „cudzysłowem”\ni nową linią").unwrap();
        let out = tauri::async_runtime::block_on(send_write(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            reqwest::Method::PUT,
            "/issues/1234.json",
            &body,
        ))
        .unwrap();
        assert_eq!(out, Value::Null);
        let request = server.join().unwrap();
        assert!(
            request.starts_with("PUT /issues/1234.json HTTP/1.1\r\n"),
            "{request}"
        );
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("x-redmine-api-key: test-only-key\r\n"));
        assert!(lower.contains("content-type: application/json\r\n"));
        assert!(!lower.contains("authorization:"));
        let sent: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        assert_eq!(sent, body);
        // 422 z `errors`: komunikat po polsku z treścią Redmine, bez klucza.
        let (site, server) = serve_write(
            "422 Unprocessable Entity",
            r#"{"errors":["Status is invalid","Done ratio cannot be blank"]}"#,
        );
        let c = Config {
            site,
            ..Config::default()
        };
        let err = tauri::async_runtime::block_on(send_write(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            reqwest::Method::PUT,
            "/issues/1234.json",
            &status_body(2).unwrap(),
        ))
        .unwrap_err();
        assert_eq!(
            err,
            "Redmine odrzucił dane: Status is invalid; Done ratio cannot be blank"
        );
        assert!(!err.contains("test-only-key"));
        let request = server.join().unwrap();
        assert!(
            request.ends_with(r#"{"issue":{"status_id":2}}"#),
            "{request}"
        );
        // Niepoprawna ścieżka nie trafia do serwera.
        let c = Config {
            site: "http://127.0.0.1:9".into(),
            ..Config::default()
        };
        let err = tauri::async_runtime::block_on(send_write(
            &client(&c).unwrap(),
            &c,
            "test-only-key",
            reqwest::Method::PUT,
            "/users/1.json",
            &status_body(2).unwrap(),
        ))
        .unwrap_err();
        assert_eq!(err, "Niepoprawny adres zapisu do Redmine.");
    }

    #[test]
    fn writes_are_blocked_when_redmine_write_is_off() {
        let off = crate::settings::AppSettings {
            redmine_write: false,
            ..Default::default()
        };
        assert_eq!(ensure_write(&off).unwrap_err(), WRITE_DISABLED);
        // Przełącznik Jiry jest osobny w obie strony.
        assert!(crate::jira::ensure_write(&off).is_ok());
        let jira_off = crate::settings::AppSettings {
            jira_write: false,
            ..Default::default()
        };
        assert!(ensure_write(&jira_off).is_ok());
        assert!(ensure_write(&crate::settings::AppSettings::default()).is_ok());
    }

    #[test]
    fn status_unchanged_after_put_is_reported() {
        let mut issue = fixture_issues()
            .into_iter()
            .find(|i| i.id == 1234)
            .unwrap_or_else(|| fixture_issues().remove(0));
        issue.status_id = 1;
        issue.status = "New".into();
        let message = status_unchanged(&issue, 2).unwrap();
        assert!(message.starts_with(STATUS_UNCHANGED), "{message}");
        assert!(message.contains("„New”"));
        assert_eq!(status_unchanged(&issue, 1), None);
        issue.status_id = 0;
        assert_eq!(status_unchanged(&issue, 2), None);
    }

    #[test]
    fn branch_name_uses_issue_number_and_subject() {
        assert_eq!(
            crate::jira_board::branch_name("feature/", "1234", "Krótki tytuł"),
            "feature/1234-krotki-tytul"
        );
        assert_eq!(crate::jira_board::branch_name("", "1234", "!!!"), "1234");
        let mut connection = memory();
        // Temat z `redmine_issues`.
        let tx = connection.transaction().unwrap();
        let mut issue = fixture_issues().remove(0);
        issue.id = 1234;
        issue.subject = "Import CSV zamówień".into();
        save_issues(&tx, SITE, &[issue]).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            branch_title(&connection, SITE, 1234).unwrap(),
            "Import CSV zamówień"
        );
        assert_eq!(
            crate::jira_board::branch_name(
                "bugfix/",
                "1234",
                &branch_title(&connection, SITE, 1234).unwrap()
            ),
            "bugfix/1234-import-csv-zamowien"
        );
        // Bez zgłoszenia w `redmine_issues`: tytuł powiązanego zadania.
        db::change(
            &mut connection,
            "add",
            None,
            Some("Lokalny tytuł".into()),
            0,
        )
        .unwrap();
        connection
            .execute(
                "INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(1,?1,77)",
                [SITE],
            )
            .unwrap();
        assert_eq!(
            branch_title(&connection, SITE, 77).unwrap(),
            "Lokalny tytuł"
        );
        assert_eq!(branch_title(&connection, SITE, 78).unwrap(), "");
        // Inny Redmine nie podaje tytułu.
        assert_eq!(
            branch_title(&connection, "https://inny.example.pl", 1234).unwrap(),
            ""
        );
    }
}
