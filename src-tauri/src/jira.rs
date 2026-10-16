use crate::{db, Database};
use keyring::Entry;
use reqwest::{Client, StatusCode, Url};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Mutex,
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

#[derive(Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: u64,
    pub name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub site: String,
    pub email: String,
    pub display_name: String,
    pub boards: Vec<Board>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            site: "https://asistonwms.atlassian.net".into(),
            email: String::new(),
            display_name: String::new(),
            boards: vec![],
        }
    }
}
#[derive(Serialize)]
pub struct Settings {
    config: Config,
    connected: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    key: String,
    summary: String,
    status: String,
    boards: Vec<String>,
    url: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardPage {
    values: Vec<Board>,
    next_start: Option<u64>,
}
#[derive(Serialize)]
pub struct Issues {
    issues: Vec<Issue>,
    warnings: Vec<String>,
}
pub struct JiraState(Mutex<HashMap<String, Issue>>);

pub fn setup(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    app.manage(JiraState(Mutex::new(HashMap::new())));
    Ok(())
}
/// Tytuł zgłoszenia z ostatnio pobranej listy (`jira_issues`), np. dla szybkiego
/// dodawania z samym kluczem.
pub fn cached_summary(app: &tauri::AppHandle, key: &str) -> Option<String> {
    app.state::<JiraState>()
        .0
        .lock()
        .ok()?
        .get(key)
        .map(|issue| issue.summary.clone())
}
/// Dopisuje zgłoszenia z tablicy (J2) do listy sesji, żeby „Start” z tablicy
/// działał jak z listy (`jira_start`). Istniejące wpisy dostają nazwę tablicy.
pub fn remember(app: &tauri::AppHandle, rows: &[(String, String, String)], board: &str) {
    let Ok(c) = config(app) else {
        return;
    };
    let state = app.state::<JiraState>();
    let Ok(mut cache) = state.0.lock() else {
        return;
    };
    for (key, summary, status) in rows {
        let row = cache.entry(key.clone()).or_insert_with(|| Issue {
            key: key.clone(),
            summary: summary.clone(),
            status: status.clone(),
            boards: vec![],
            url: format!("{}/browse/{}", c.site, key),
        });
        row.summary = summary.clone();
        row.status = status.clone();
        if !row.boards.iter().any(|b| b == board) {
            row.boards.push(board.to_string());
        }
    }
}
pub fn config(app: &tauri::AppHandle) -> Result<Config, String> {
    let state = app.state::<Database>();
    let db = state.0.lock().map_err(|e| e.to_string())?;
    let raw: Option<String> = db
        .query_row("SELECT value FROM settings WHERE key='jira'", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    raw.map(|s| serde_json::from_str(&s).map_err(|_| "Nie można odczytać ustawień Jiry.".into()))
        .unwrap_or_else(|| Ok(Config::default()))
}
fn save(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    let raw = serde_json::to_string(config).map_err(|e| e.to_string())?;
    app.state::<Database>().0.lock().map_err(|e| e.to_string())?
        .execute("INSERT INTO settings(key,value) VALUES('jira',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [raw]).map_err(|e| e.to_string())?;
    Ok(())
}
fn normalize_site(site: &str) -> Result<String, String> {
    let url = Url::parse(site.trim())
        .map_err(|_| "Podaj poprawny adres Jiry Cloud (https://nazwa.atlassian.net).".to_owned())?;
    if url.scheme() != "https"
        || !url
            .host_str()
            .is_some_and(|h| h.ends_with(".atlassian.net"))
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("Podaj główny adres Jiry Cloud, np. https://asistonwms.atlassian.net.".into());
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}
pub fn valid_key(key: &str) -> bool {
    key.len() <= 100
        && key.split_once('-').is_some_and(|(project, number)| {
            !project.is_empty()
                && project
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                && !number.is_empty()
                && number.bytes().all(|c| c.is_ascii_digit())
        })
}
fn entry(app: &tauri::AppHandle, c: &Config) -> Result<Entry, String> {
    Entry::new(
        &format!("{}.jira", app.config().identifier),
        &format!("{}|{}", c.site, c.email),
    )
    .map_err(|_| "Nie można otworzyć magazynu poświadczeń Windows.".into())
}
pub(crate) fn token(app: &tauri::AppHandle, c: &Config) -> Result<String, String> {
    entry(app, c)?
        .get_password()
        .map_err(|_| "Połącz konto Jiry: podaj e-mail i token API.".into())
}
pub(crate) fn client() -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Nie udało się utworzyć połączenia HTTPS.".into())
}
fn status_error(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => {
            "Jira odrzuciła logowanie. Sprawdź e-mail i token API (bez zakresów).".into()
        }
        StatusCode::FORBIDDEN => "Brak uprawnień do tej tablicy lub zgłoszeń w Jirze.".into(),
        StatusCode::NOT_FOUND => "Nie znaleziono tablicy lub zgłoszenia w Jirze.".into(),
        StatusCode::TOO_MANY_REQUESTS => {
            "Jira ograniczyła liczbę zapytań. Spróbuj ponownie za chwilę.".into()
        }
        s if s.is_redirection() => {
            "Jira przekierowała żądanie. Sprawdź adres i sposób logowania.".into()
        }
        _ => format!(
            "Jira zwróciła błąd HTTP {}. Spróbuj ponownie.",
            status.as_u16()
        ),
    }
}
pub(crate) async fn get(
    client: &Client,
    c: &Config,
    secret: &str,
    path: &str,
    query: &[(&str, String)],
) -> Result<Value, String> {
    let response = client
        .get(format!("{}{}", c.site, path))
        .basic_auth(&c.email, Some(secret))
        .header("Accept", "application/json")
        .query(query)
        .send()
        .await
        .map_err(|_| {
            "Nie można połączyć się z Jirą. Sprawdź Internet i adres serwera.".to_owned()
        })?;
    if !response.status().is_success() {
        return Err(status_error(response.status()));
    }
    response
        .json()
        .await
        .map_err(|_| "Jira zwróciła niepoprawną odpowiedź.".into())
}
#[tauri::command]
pub fn jira_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    let connected = if c.email.is_empty() {
        false
    } else {
        match entry(&app, &c)?.get_password() {
            Ok(_) => true,
            Err(keyring::Error::NoEntry) => false,
            Err(_) => {
                return Err("Nie można odczytać tokenu z magazynu poświadczeń Windows.".into())
            }
        }
    };
    Ok(Settings {
        config: c,
        connected,
    })
}
#[tauri::command]
pub async fn jira_connect(
    app: tauri::AppHandle,
    site: String,
    email: String,
    api_token: String,
) -> Result<Settings, String> {
    let old = config(&app)?;
    let mut c = Config {
        site: normalize_site(&site)?,
        email: email.trim().to_owned(),
        ..Config::default()
    };
    if c.email.is_empty()
        || c.email.len() > 254
        || api_token.trim().is_empty()
        || api_token.len() > 4096
    {
        return Err("Podaj e-mail konta Atlassian oraz token API bez zakresów.".into());
    }
    let user = get(&client()?, &c, api_token.trim(), "/rest/api/3/myself", &[]).await?;
    if user.get("accountId").and_then(Value::as_str).is_none() {
        return Err("Nie udało się potwierdzić konta Jiry.".into());
    }
    c.display_name = user["displayName"].as_str().unwrap_or(&c.email).to_owned();
    if c.site == old.site && c.email == old.email {
        c.boards = old.boards.clone();
    }
    entry(&app, &c)?
        .set_password(api_token.trim())
        .map_err(|_| "Nie udało się zapisać tokenu w magazynie poświadczeń Windows.".to_owned())?;
    save(&app, &c)?;
    if !old.email.is_empty() && (c.site != old.site || c.email != old.email) {
        let _ = entry(&app, &old)?.delete_credential();
    }
    app.state::<JiraState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .clear();
    Ok(Settings {
        config: c,
        connected: true,
    })
}
#[tauri::command]
pub fn jira_disconnect(app: tauri::AppHandle) -> Result<Settings, String> {
    let c = config(&app)?;
    if !c.email.is_empty() {
        match entry(&app, &c)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(_) => {
                return Err("Nie udało się usunąć tokenu z magazynu poświadczeń Windows.".into())
            }
        }
    }
    let c = Config {
        site: c.site,
        ..Config::default()
    };
    save(&app, &c)?;
    app.state::<JiraState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .clear();
    Ok(Settings {
        config: c,
        connected: false,
    })
}
#[tauri::command]
pub async fn jira_boards(
    app: tauri::AppHandle,
    name: String,
    start_at: u64,
) -> Result<BoardPage, String> {
    let c = config(&app)?;
    let value = get(
        &client()?,
        &c,
        &token(&app, &c)?,
        "/rest/agile/1.0/board",
        &[
            ("name", name),
            ("startAt", start_at.to_string()),
            ("maxResults", "50".into()),
        ],
    )
    .await?;
    let values: Vec<Board> = serde_json::from_value(value["values"].clone())
        .map_err(|_| "Jira nie zwróciła listy tablic.".to_owned())?;
    let next = start_at + values.len() as u64;
    let last = value["isLast"].as_bool().unwrap_or(false)
        || values.is_empty()
        || value["total"].as_u64().is_some_and(|n| next >= n);
    Ok(BoardPage {
        values,
        next_start: if last { None } else { Some(next) },
    })
}
#[tauri::command]
pub fn jira_select_boards(app: tauri::AppHandle, boards: Vec<Board>) -> Result<(), String> {
    if boards.len() > 20
        || boards
            .iter()
            .any(|b| b.id == 0 || b.name.is_empty() || b.name.len() > 1000)
    {
        return Err("Wybierz maksymalnie 20 tablic.".into());
    }
    let mut c = config(&app)?;
    let mut unique = HashSet::new();
    c.boards = boards.into_iter().filter(|b| unique.insert(b.id)).collect();
    save(&app, &c)?;
    app.state::<JiraState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .clear();
    Ok(())
}
fn merge_issues(
    result: &mut BTreeMap<String, Issue>,
    value: &Value,
    c: &Config,
    board: &Board,
) -> Result<(), String> {
    let issues = value["issues"]
        .as_array()
        .ok_or("Jira nie zwróciła listy zgłoszeń.")?;
    for issue in issues {
        let key = issue["key"]
            .as_str()
            .filter(|k| valid_key(k))
            .ok_or("Niepoprawny klucz zgłoszenia w odpowiedzi Jiry.")?;
        let row = result.entry(key.to_owned()).or_insert_with(|| Issue {
            key: key.to_owned(),
            summary: issue["fields"]["summary"]
                .as_str()
                .unwrap_or(key)
                .to_owned(),
            status: issue["fields"]["status"]["name"]
                .as_str()
                .unwrap_or("")
                .to_owned(),
            boards: vec![],
            url: format!("{}/browse/{}", c.site, key),
        });
        if !row.boards.contains(&board.name) {
            row.boards.push(board.name.clone());
        }
    }
    Ok(())
}
/// Pobiera wszystkie strony wyniku JQL (stronicowanie `nextPageToken`) i podaje
/// każdą odpowiedź do `each`; `Ok(false)` z `each` kończy pobieranie (limit).
pub(crate) async fn pages(
    client: &Client,
    c: &Config,
    secret: &str,
    path: &str,
    jql: &str,
    fields: &str,
    mut each: impl FnMut(&Value) -> Result<bool, String>,
) -> Result<(), String> {
    let mut cursor = String::new();
    let mut seen = HashSet::new();
    for _ in 0..200 {
        let mut query = vec![
            ("jql", jql.into()),
            ("maxResults", "100".into()),
            ("fields", fields.into()),
        ];
        if !cursor.is_empty() {
            query.push(("nextPageToken", cursor.clone()));
        }
        let value = get(client, c, secret, path, &query).await?;
        if !each(&value)? {
            return Ok(());
        }
        if value["isLast"].as_bool() == Some(true) {
            return Ok(());
        }
        match value["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
            Some(next) if seen.insert(next.to_owned()) => cursor = next.to_owned(),
            None if value["isLast"].as_bool() != Some(false) => return Ok(()),
            _ => {
                return Err(
                    "Nie można pobrać kolejnej strony wyników Jiry. Odśwież tablicę.".into(),
                )
            }
        }
    }
    Err("Tablica zwróciła zbyt wiele wyników. Ogranicz wybór do otwartych zadań.".into())
}
async fn board_issues(
    client: &Client,
    c: &Config,
    secret: &str,
    board: &Board,
    open_only: bool,
) -> Result<BTreeMap<String, Issue>, String> {
    let jql = if open_only {
        "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
    } else {
        "assignee = currentUser() ORDER BY updated DESC"
    };
    let mut result = BTreeMap::new();
    let path = format!("/rest/software/1.0/board/{}/issue", board.id);
    pages(client, c, secret, &path, jql, "summary,status", |value| {
        merge_issues(&mut result, value, c, board).map(|_| true)
    })
    .await?;
    Ok(result)
}
#[tauri::command]
pub async fn jira_issues(app: tauri::AppHandle, open_only: bool) -> Result<Issues, String> {
    let c = config(&app)?;
    if c.boards.is_empty() {
        return Err("Najpierw wybierz tablice.".into());
    }
    let secret = token(&app, &c)?;
    let client = client()?;
    let mut result: BTreeMap<String, Issue> = BTreeMap::new();
    let mut warnings = vec![];
    for board in &c.boards {
        match board_issues(&client, &c, &secret, board, open_only).await {
            Ok(rows) => {
                for (key, row) in rows {
                    if let Some(existing) = result.get_mut(&key) {
                        existing.boards.extend(row.boards);
                    } else {
                        result.insert(key, row);
                    }
                }
            }
            Err(e) => warnings.push(format!("{}: {}", board.name, e)),
        }
    }
    let current = config(&app)?;
    if current.site != c.site
        || current.email != c.email
        || current.boards.iter().map(|b| b.id).collect::<Vec<_>>()
            != c.boards.iter().map(|b| b.id).collect::<Vec<_>>()
    {
        return Err("Ustawienia zmieniły się podczas pobierania. Odśwież wyniki.".into());
    }
    let issues: Vec<Issue> = result.into_values().collect();
    {
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        save_statuses(
            &tx,
            &c.site,
            issues.iter().map(|i| (i.key.as_str(), i.status.as_str())),
        )
        .and_then(|_| tx.commit())
        .map_err(|e| e.to_string())?;
    }
    let _ = app.emit("data-changed", ());
    *app.state::<JiraState>()
        .0
        .lock()
        .map_err(|e| e.to_string())? = issues.iter().map(|i| (i.key.clone(), i.clone())).collect();
    Ok(Issues { issues, warnings })
}
#[tauri::command]
pub fn jira_start(app: tauri::AppHandle, key: String) -> Result<db::Snapshot, String> {
    let c = config(&app)?;
    let issue = app
        .state::<JiraState>()
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .get(&key)
        .cloned()
        .ok_or("Odśwież listę zgłoszeń przed rozpoczęciem.")?;
    let state = app.state::<Database>();
    let mut connection = state.0.lock().map_err(|e| e.to_string())?;
    let result = start_issue(
        &mut connection,
        &c.site,
        &issue.key,
        &issue.summary,
        db::now(),
    )?;
    crate::reminder::reset(&app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
/// Tytuł jednego zgłoszenia wprost z Jiry (`/rest/api/3/issue/{klucz}`).
async fn issue_summary(
    client: &Client,
    c: &Config,
    secret: &str,
    key: &str,
) -> Result<String, String> {
    if !valid_key(key) {
        return Err("Niepoprawny klucz zgłoszenia Jiry.".into());
    }
    let value = get(
        client,
        c,
        secret,
        &format!("/rest/api/3/issue/{key}"),
        &[("fields", "summary".to_string())],
    )
    .await?;
    value["fields"]["summary"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Jira zwróciła niepoprawną odpowiedź.".into())
}
/// „Przełącz” z propozycji agenta dla zgłoszenia bez lokalnego zadania: jak
/// `jira_start`, a gdy zgłoszenia nie ma w pobranej liście (np. tuż po starcie
/// aplikacji, zanim ktoś otworzył widok Jira), tytuł pobiera wprost z Jiry.
pub async fn start_by_key(app: &tauri::AppHandle, key: &str) -> Result<db::Snapshot, String> {
    if cached_summary(app, key).is_some() {
        return jira_start(app.clone(), key.to_string());
    }
    let c = config(app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry, żeby rozpocząć zgłoszenie.".into());
    }
    let secret = token(app, &c)?;
    let summary = issue_summary(&client()?, &c, &secret, key).await?;
    let state = app.state::<Database>();
    let result = {
        let mut connection = state.0.lock().map_err(|e| e.to_string())?;
        start_issue(&mut connection, &c.site, key, &summary, db::now())?
    };
    crate::reminder::reset(app);
    let _ = app.emit("data-changed", ());
    Ok(result)
}
/// „Rozpocznij” z powiadomienia o nowym zgłoszeniu: ta sama ścieżka co widok Jira
/// (`start_issue`), więc zadanie z kosza lub archiwum wraca, a poprzedni pomiar staje.
pub fn start_task(
    connection: &mut rusqlite::Connection,
    task_id: i64,
    now: i64,
) -> Result<db::Snapshot, String> {
    let (site, key, title): (String, String, String) = connection
        .query_row(
            "SELECT j.site, j.issue_key, t.title FROM jira_tasks j JOIN tasks t ON t.id=j.task_id
             WHERE j.task_id=?1",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or("Nie znaleziono zadania powiązanego ze zgłoszeniem Jiry.")?;
    start_issue(connection, &site, &key, &title, now)
}
fn start_issue(
    connection: &mut rusqlite::Connection,
    site: &str,
    key: &str,
    summary: &str,
    now: i64,
) -> Result<db::Snapshot, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let existing: Option<i64> = tx
        .query_row(
            "SELECT task_id FROM jira_tasks WHERE site=?1 AND issue_key=?2",
            params![site, key],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let id = if let Some(id) = existing {
        id
    } else {
        let title: String = summary.chars().take(240).collect();
        tx.execute(
            "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
            params![title, now],
        )
        .map_err(|e| e.to_string())?;
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(?1,?2,?3)",
            params![id, site, key],
        )
        .map_err(|e| e.to_string())?;
        id
    };
    tx.execute("UPDATE tasks SET done=0 WHERE id=?1", [id])
        .map_err(|e| e.to_string())?;
    crate::planner::clear_waiting(&tx, id).map_err(|e| e.to_string())?;
    tx.execute(
        "UPDATE task_lifecycle SET archived=0,deleted_at=NULL WHERE task_id=?1",
        [id],
    )
    .map_err(|e| e.to_string())?;
    let active: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE task_id=?1 AND ended_at IS NULL)",
            [id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !active {
        db::stop(&tx, now).map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO sessions(task_id,started_at) VALUES(?1,?2)",
            params![id, now],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    db::snapshot(connection).map_err(|e| e.to_string())
}
/// Co ile sprawdzać nowe przypisania w Jirze.
const WATCH_EVERY: i64 = 5 * 60_000;
/// Co ile wątek sprawdza, czy pora na zapytanie (szybka reakcja na włączenie opcji).
const WATCH_TICK: Duration = Duration::from_secs(20);
/// Moje zgłoszenia: otwarte (do importu) i niedawno zamknięte (aktualizacja statusu).
const WATCH_JQL: &str =
    "assignee = currentUser() AND (statusCategory != Done OR updated >= -14d) ORDER BY created DESC";

/// Zgłoszenie z odpowiedzi wyszukiwania: klucz, tytuł, status i czy jest otwarte.
#[derive(Clone, Debug, PartialEq)]
struct Assigned {
    key: String,
    summary: String,
    status: String,
    open: bool,
}

fn assigned_rows(value: &Value, out: &mut Vec<Assigned>) -> Result<(), String> {
    let issues = value["issues"]
        .as_array()
        .ok_or("Jira nie zwróciła listy zgłoszeń.")?;
    for issue in issues {
        let Some(key) = issue["key"].as_str().filter(|k| valid_key(k)) else {
            continue;
        };
        let status = &issue["fields"]["status"];
        out.push(Assigned {
            key: key.to_owned(),
            summary: issue["fields"]["summary"]
                .as_str()
                .unwrap_or(key)
                .to_owned(),
            status: status["name"].as_str().unwrap_or("").to_owned(),
            open: status["statusCategory"]["key"].as_str() != Some("done"),
        });
    }
    Ok(())
}

async fn fetch_assigned(
    client: &Client,
    c: &Config,
    secret: &str,
) -> Result<Vec<Assigned>, String> {
    let mut rows = Vec::new();
    pages(
        client,
        c,
        secret,
        "/rest/api/3/search/jql",
        WATCH_JQL,
        "summary,status",
        |value| assigned_rows(value, &mut rows).map(|_| true),
    )
    .await?;
    Ok(rows)
}

pub(crate) fn save_statuses<'a>(
    tx: &rusqlite::Transaction,
    site: &str,
    rows: impl Iterator<Item = (&'a str, &'a str)>,
) -> rusqlite::Result<()> {
    for (key, status) in rows {
        tx.execute(
            "INSERT INTO jira_issues(site,issue_key,status) VALUES(?1,?2,?3) ON CONFLICT(site,issue_key) DO UPDATE SET status=excluded.status",
            params![site, key, status],
        )?;
    }
    Ok(())
}

/// Zapisuje statusy i dodaje zadania dla otwartych zgłoszeń, które od ostatniego
/// sprawdzenia zostały przypisane do mnie. `baseline` (pierwsze sprawdzenie dla
/// konta) tylko zapamiętuje obecne przypisania, bez importu całego backlogu.
/// Nowe zadanie nie ma dnia w planie, więc trafia do „Do zaplanowania”; zgłoszenie
/// już powiązane z lokalnym zadaniem (także w archiwum lub koszu) nie jest dublowane.
fn sync_assigned(
    connection: &mut rusqlite::Connection,
    site: &str,
    rows: &[Assigned],
    baseline: bool,
    now: i64,
) -> Result<Vec<Assigned>, String> {
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    save_statuses(
        &tx,
        site,
        rows.iter().map(|r| (r.key.as_str(), r.status.as_str())),
    )
    .map_err(|e| e.to_string())?;
    let mut imported = Vec::new();
    for row in rows.iter().filter(|r| r.open) {
        let known: bool = tx
            .query_row(
                "SELECT assigned=1 OR EXISTS(SELECT 1 FROM jira_tasks WHERE site=?1 AND issue_key=?2) FROM jira_issues WHERE site=?1 AND issue_key=?2",
                params![site, row.key],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if baseline || known {
            continue;
        }
        let title: String = row.summary.chars().take(240).collect();
        tx.execute(
            "INSERT INTO tasks(title,created_at) VALUES(?1,?2)",
            params![title, now],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(?1,?2,?3)",
            params![tx.last_insert_rowid(), site, row.key],
        )
        .map_err(|e| e.to_string())?;
        imported.push(row.clone());
    }
    tx.execute("UPDATE jira_issues SET assigned=0 WHERE site=?1", [site])
        .map_err(|e| e.to_string())?;
    for row in rows.iter().filter(|r| r.open) {
        tx.execute(
            "UPDATE jira_issues SET assigned=1 WHERE site=?1 AND issue_key=?2",
            params![site, row.key],
        )
        .map_err(|e| e.to_string())?;
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(imported)
}

/// Powiadomienie o zaimportowanych zgłoszeniach (najwyżej trzy wymienione).
fn imported_message(rows: &[Assigned]) -> (String, String) {
    let title = if rows.len() == 1 {
        "Nowe zadanie z Jiry".to_owned()
    } else {
        format!("Nowe z Jiry: {}", crate::notify::tasks_word(rows.len()))
    };
    let mut lines: Vec<String> = rows
        .iter()
        .take(3)
        .map(|r| crate::notify::short(&format!("{} · {}", r.key, r.summary)))
        .collect();
    if rows.len() > 3 {
        lines.push(format!("i jeszcze {}", rows.len() - 3));
    }
    lines.push("Dodano do „Do zaplanowania”.".into());
    (title, lines.join("\n"))
}

/// Lokalne zadania zaimportowanych zgłoszeń, w kolejności `rows`.
fn imported_task_ids(
    db: &rusqlite::Connection,
    site: &str,
    rows: &[Assigned],
) -> rusqlite::Result<Vec<i64>> {
    rows.iter()
        .map(|r| {
            db.query_row(
                "SELECT task_id FROM jira_tasks WHERE site=?1 AND issue_key=?2",
                params![site, r.key],
                |row| row.get(0),
            )
        })
        .collect()
}

/// Powiadomienie z akcjami: kliknięcie otwiera Zadania przy nowych (nierozpoczętych)
/// zadaniach, „Rozpocznij” tylko przy jednym zgłoszeniu, „Zaplanuj na dziś” dla wszystkich.
fn imported_notification(rows: &[Assigned], ids: &[i64]) -> crate::notify::Message {
    use crate::notify::{Action, Message, View, JIRA_START, PLAN_TODAY};
    let (title, body) = imported_message(rows);
    let mut actions = Vec::new();
    if let [id] = ids {
        actions.push(Action::new(JIRA_START, "Rozpocznij", id));
    }
    if !ids.is_empty() {
        let list = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        actions.push(Action::new(PLAN_TODAY, "Zaplanuj na dziś", list));
    }
    Message {
        key: format!(
            "jira:{}",
            rows.iter()
                .map(|r| r.key.as_str())
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

/// Jedno sprawdzenie przypisań; `None`, gdy opcja wyłączona lub brak połączenia.
/// Wynik: zaimportowane zgłoszenia i ich lokalne zadania.
#[allow(clippy::type_complexity)]
fn watch_once(app: &tauri::AppHandle) -> Option<Result<(Vec<Assigned>, Vec<i64>), String>> {
    if !crate::settings::current(app).jira_auto_import {
        return None;
    }
    let c = config(app).ok()?;
    if c.email.is_empty() {
        return None;
    }
    let secret = token(app, &c).ok()?;
    let fetched =
        tauri::async_runtime::block_on(async { fetch_assigned(&client()?, &c, &secret).await });
    Some(fetched.and_then(|rows| {
        let current = config(app)?;
        if current.site != c.site || current.email != c.email {
            return Ok((Vec::new(), Vec::new()));
        }
        let watch = format!("{}|{}", c.site, c.email);
        let state = app.state::<Database>();
        let mut db = state.0.lock().map_err(|e| e.to_string())?;
        let baseline = db
            .query_row("SELECT value FROM settings WHERE key='jira_watch'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
            .as_deref()
            != Some(watch.as_str());
        let imported = sync_assigned(&mut db, &c.site, &rows, baseline, db::now())?;
        db.execute(
            "INSERT INTO settings(key,value) VALUES('jira_watch',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [watch],
        )
        .map_err(|e| e.to_string())?;
        let ids = imported_task_ids(&db, &c.site, &imported).map_err(|e| e.to_string())?;
        Ok((imported, ids))
    }))
}

/// Wątek sprawdzający co 5 min nowe przypisania w Jirze. Kończy się, gdy okno
/// main zniknie, więc wołać po `create_windows`.
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

/// Komunikat, gdy główny przełącznik zapisów do Jiry jest wyłączony.
pub const WRITE_DISABLED: &str =
    "Zmiany w Jirze są wyłączone. Włączysz je w Ustawienia → Jira („Pozwól aplikacji zmieniać zgłoszenia w Jirze”).";
/// Limit treści jednego zapisu do Jiry.
const WRITE_LIMIT: usize = 32 * 1024;

/// Główny przełącznik zapisów (J1, J2): backend odrzuca zapis niezależnie od
/// interfejsu.
pub fn ensure_write(s: &crate::settings::AppSettings) -> Result<(), String> {
    if s.jira_write {
        Ok(())
    } else {
        Err(WRITE_DISABLED.into())
    }
}

/// Szczegóły z odpowiedzi błędu Jiry (`errorMessages` i `errors`), najwyżej 300 znaków.
fn error_details(body: &Value) -> String {
    let mut parts: Vec<String> = body["errorMessages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| m.as_str().map(str::to_string))
        .collect();
    if let Some(errors) = body["errors"].as_object() {
        parts.extend(
            errors
                .values()
                .filter_map(|m| m.as_str().map(str::to_string)),
        );
    }
    parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(300)
        .collect()
}

/// Polski komunikat dla nieudanego zapisu do Jiry (400/401/403/404 i inne).
pub fn write_error(status: StatusCode, body: &Value) -> String {
    let details = error_details(body);
    let suffix = if details.is_empty() {
        String::new()
    } else {
        format!(" Jira: {details}")
    };
    match status {
        StatusCode::BAD_REQUEST => format!(
            "Jira odrzuciła zmianę. Przejście może wymagać wypełnienia pól na ekranie w Jirze — zrób je w przeglądarce.{suffix}"
        ),
        StatusCode::UNAUTHORIZED => status_error(status),
        StatusCode::FORBIDDEN => {
            format!("Brak uprawnień do zmiany tego zgłoszenia w Jirze.{suffix}")
        }
        StatusCode::NOT_FOUND => {
            format!("Nie znaleziono zgłoszenia albo przejścia w Jirze.{suffix}")
        }
        StatusCode::CONFLICT => {
            format!("Zgłoszenie zmieniło się w Jirze w międzyczasie. Odśwież i spróbuj ponownie.{suffix}")
        }
        _ => status_error(status),
    }
}

/// Wysyła zapis (np. `POST …/transitions`). Ścieżka musi zaczynać się od `/rest/`,
/// treść ma limit długości. Pusta odpowiedź (204) to `Value::Null`.
pub(crate) async fn send_write(
    client: &Client,
    c: &Config,
    secret: &str,
    method: reqwest::Method,
    path: &str,
    body: &Value,
) -> Result<Value, String> {
    if !path.starts_with("/rest/")
        || path.len() > 500
        || path.contains("..")
        || path.contains(['?', '#', ' '])
    {
        return Err("Niepoprawny adres zapisu do Jiry.".into());
    }
    let raw = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    if raw.len() > WRITE_LIMIT {
        return Err("Zmiana jest zbyt duża dla Jiry.".into());
    }
    let response = client
        .request(method, format!("{}{}", c.site, path))
        .basic_auth(&c.email, Some(secret))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(raw)
        .send()
        .await
        .map_err(|_| {
            "Nie można połączyć się z Jirą. Sprawdź Internet i adres serwera.".to_owned()
        })?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if status.is_success() {
        Ok(value)
    } else {
        Err(write_error(status, &value))
    }
}

/// Jedyna ścieżka zapisu do Jiry (J1 i J2: przejścia, przypisanie, komentarze,
/// tworzenie zgłoszeń): główny przełącznik, połączone
/// konto, walidacja i polskie komunikaty błędów.
pub async fn write(
    app: &tauri::AppHandle,
    method: reqwest::Method,
    path: &str,
    body: &Value,
) -> Result<Value, String> {
    ensure_write(&crate::settings::current(app))?;
    let c = config(app)?;
    if c.email.is_empty() {
        return Err("Połącz konto Jiry, żeby zmieniać zgłoszenia.".into());
    }
    let secret = token(app, &c)?;
    send_write(&client()?, &c, &secret, method, path, body).await
}

/// Przejście zgłoszenia z `GET /rest/api/3/issue/{klucz}/transitions`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transition {
    pub id: String,
    pub name: String,
    /// Status docelowy i jego kategoria (`new`, `indeterminate`, `done`).
    pub to_status: String,
    pub to_category: String,
}

/// Identyfikator przejścia: same cyfry, najwyżej 20 znaków.
pub fn valid_transition_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())
}

/// Odczyt odpowiedzi `transitions`; przejścia z niepoprawnym identyfikatorem są pomijane.
pub fn parse_transitions(value: &Value) -> Result<Vec<Transition>, String> {
    let list = value["transitions"]
        .as_array()
        .ok_or("Jira nie zwróciła listy przejść.")?;
    Ok(list
        .iter()
        .filter_map(|t| {
            let id = match &t["id"] {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => return None,
            };
            if !valid_transition_id(&id) {
                return None;
            }
            let to = &t["to"];
            let to_status = to["name"].as_str().unwrap_or("").trim().to_string();
            let name = t["name"].as_str().unwrap_or("").trim().to_string();
            Some(Transition {
                id,
                name: if name.is_empty() {
                    to_status.clone()
                } else {
                    name
                },
                to_status,
                to_category: to["statusCategory"]["key"]
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect())
}

/// Dostępne przejścia zgłoszenia.
pub async fn transitions(
    client: &Client,
    c: &Config,
    secret: &str,
    key: &str,
) -> Result<Vec<Transition>, String> {
    if !valid_key(key) {
        return Err("Niepoprawny klucz zgłoszenia Jiry.".into());
    }
    let value = get(
        client,
        c,
        secret,
        &format!("/rest/api/3/issue/{key}/transitions"),
        &[],
    )
    .await?;
    parse_transitions(&value)
}

/// Bieżący status zgłoszenia: nazwa i kategoria (`statusCategory.key`).
pub async fn issue_status(
    client: &Client,
    c: &Config,
    secret: &str,
    key: &str,
) -> Result<(String, String), String> {
    if !valid_key(key) {
        return Err("Niepoprawny klucz zgłoszenia Jiry.".into());
    }
    let value = get(
        client,
        c,
        secret,
        &format!("/rest/api/3/issue/{key}"),
        &[("fields", "status".to_string())],
    )
    .await?;
    let status = &value["fields"]["status"];
    let name = status["name"]
        .as_str()
        .ok_or("Jira zwróciła niepoprawną odpowiedź.")?;
    Ok((
        name.to_string(),
        status["statusCategory"]["key"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    ))
}

/// Zapisuje status jednego zgłoszenia w `jira_issues` (kafelek, propozycje).
pub fn store_status(app: &tauri::AppHandle, site: &str, key: &str, status: &str) {
    let state = app.state::<Database>();
    let Ok(mut db) = state.0.lock() else {
        return;
    };
    let saved = db.transaction().and_then(|tx| {
        save_statuses(&tx, site, std::iter::once((key, status)))?;
        tx.commit()
    });
    drop(saved);
}

/// Wykonuje przejście zgłoszenia (`POST …/transitions`) przez wspólną ścieżkę zapisu.
pub async fn transition(
    app: &tauri::AppHandle,
    key: &str,
    transition_id: &str,
) -> Result<(), String> {
    if !valid_key(key) {
        return Err("Niepoprawny klucz zgłoszenia Jiry.".into());
    }
    if !valid_transition_id(transition_id) {
        return Err("Niepoprawne przejście w Jirze.".into());
    }
    write(
        app,
        reqwest::Method::POST,
        &format!("/rest/api/3/issue/{key}/transitions"),
        &serde_json::json!({"transition": {"id": transition_id}}),
    )
    .await
    .map(|_| ())
}

/// Adres tworzenia zgłoszenia w Jirze (J2: typ z polami spoza formularza aplikacji).
pub const CREATE_PATH: &str = "/secure/CreateIssue!default.jspa";

/// Link, który aplikacja otwiera w przeglądarce: zgłoszenie (`/browse/KLUCZ`) albo
/// tworzenie zgłoszenia, bez zapytania, fragmentu, portu i danych logowania.
fn allowed_link(parsed: &Url) -> bool {
    parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.query().is_none()
        && parsed.fragment().is_none()
        && (parsed.path() == CREATE_PATH
            || parsed
                .path()
                .strip_prefix("/browse/")
                .is_some_and(valid_key))
}

#[tauri::command]
pub fn jira_open(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if url == "https://id.atlassian.com/manage-profile/security/api-tokens" {
        return app
            .opener()
            .open_url(url, None::<String>)
            .map_err(|_| "Nie można otworzyć przeglądarki.".into());
    }
    let parsed = Url::parse(&url).map_err(|_| "Niepoprawny link.".to_owned())?;
    let site = format!("https://{}", parsed.host_str().unwrap_or(""));
    normalize_site(&site)?;
    if !allowed_link(&parsed) {
        return Err("Niepoprawny link zgłoszenia Jiry.".into());
    }
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|_| "Nie można otworzyć przeglądarki.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_board_search_follows_cursors_and_filters_current_user() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for page in 0..2 {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = socket.read(&mut buffer).unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request.starts_with("GET /rest/software/1.0/board/9/issue?"));
                let path = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let url = Url::parse(&format!("http://localhost{path}")).unwrap();
                let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
                assert_eq!(
                    query["jql"],
                    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC"
                );
                assert_eq!(query["fields"], "summary,status");
                if page == 1 {
                    assert_eq!(query["nextPageToken"], "second page");
                }
                let body = serde_json::json!({"isLast":page == 1,"nextPageToken":if page == 0 {Some("second page")}else{None},"issues":[{"key":format!("WMS-{}",page+1),"fields":{"summary":"Test","status":{"name":"Open"}}}]}).to_string();
                write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        let c = Config {
            site: format!("http://{address}"),
            email: "test@example.invalid".into(),
            ..Config::default()
        };
        let rows = tauri::async_runtime::block_on(board_issues(
            &client().unwrap(),
            &c,
            "test-only",
            &Board {
                id: 9,
                name: "Test".into(),
            },
            true,
        ))
        .unwrap();
        assert_eq!(rows.len(), 2);
        server.join().unwrap();
    }
    #[test]
    fn issue_summary_reads_single_issue_for_agent_switch() {
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
            let mut buffer = [0; 1024];
            while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(request.starts_with("GET /rest/api/3/issue/WMS-9?fields=summary "));
            let body = serde_json::json!({"key":"WMS-9","fields":{"summary":"Eksport faktur"}})
                .to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
        });
        let c = Config {
            site: format!("http://{address}"),
            email: "test@example.invalid".into(),
            ..Config::default()
        };
        let summary = tauri::async_runtime::block_on(issue_summary(
            &client().unwrap(),
            &c,
            "test-only",
            "WMS-9",
        ))
        .unwrap();
        assert_eq!(summary, "Eksport faktur");
        // Niepoprawny klucz nie wychodzi do sieci.
        assert!(tauri::async_runtime::block_on(issue_summary(
            &client().unwrap(),
            &c,
            "test-only",
            "wms-9/../x"
        ))
        .is_err());
        // Zadanie powstaje z tytułu pobranego wprost z Jiry (ścieżka „Rozpocznij”).
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&conn).unwrap();
        let snap = start_issue(&mut conn, &c.site, "WMS-9", &summary, 1_000).unwrap();
        assert_eq!(snap.tasks[0].title, "Eksport faktur");
        assert!(snap.sessions.iter().any(|s| s.ended_at.is_none()));
        server.join().unwrap();
    }
    fn row(key: &str, status: &str, open: bool) -> Assigned {
        Assigned {
            key: key.into(),
            summary: format!("Zgłoszenie {key}"),
            status: status.into(),
            open,
        }
    }
    #[test]
    fn reads_status_and_category_from_search() {
        let value = serde_json::json!({"issues":[
            {"key":"WMS-1","fields":{"summary":"A","status":{"name":"W toku","statusCategory":{"key":"indeterminate"}}}},
            {"key":"WMS-2","fields":{"summary":"B","status":{"name":"Gotowe","statusCategory":{"key":"done"}}}},
            {"key":"../x","fields":{}}
        ]});
        let mut rows = Vec::new();
        assigned_rows(&value, &mut rows).unwrap();
        assert_eq!(
            rows,
            vec![
                Assigned {
                    key: "WMS-1".into(),
                    summary: "A".into(),
                    status: "W toku".into(),
                    open: true
                },
                Assigned {
                    key: "WMS-2".into(),
                    summary: "B".into(),
                    status: "Gotowe".into(),
                    open: false
                },
            ]
        );
    }
    #[test]
    fn sync_imports_only_new_assignments_and_keeps_statuses() {
        let site = "https://a.atlassian.net";
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&connection).unwrap();
        // Pierwsze sprawdzenie: backlog zapamiętany, nic nie zaimportowane.
        let first = vec![
            row("WMS-1", "Do zrobienia", true),
            row("WMS-9", "Gotowe", false),
        ];
        assert!(sync_assigned(&mut connection, site, &first, true, 100)
            .unwrap()
            .is_empty());
        assert!(db::snapshot(&connection).unwrap().tasks.is_empty());
        // Nowe przypisanie WMS-2 trafia do zadań bez dnia w planie.
        let second = vec![
            row("WMS-1", "W toku", true),
            row("WMS-2", "Do zrobienia", true),
        ];
        let imported = sync_assigned(&mut connection, site, &second, false, 200).unwrap();
        assert_eq!(imported, vec![row("WMS-2", "Do zrobienia", true)]);
        let s = db::snapshot(&connection).unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].title, "Zgłoszenie WMS-2");
        assert_eq!(s.tasks[0].jira_key.as_deref(), Some("WMS-2"));
        assert_eq!(s.tasks[0].jira_status.as_deref(), Some("Do zrobienia"));
        assert_eq!(s.tasks[0].details.planned_day, "");
        assert!(s.sessions.is_empty());
        // Kolejne sprawdzenie nie dubluje, ale aktualizuje status.
        let third = vec![
            row("WMS-1", "W toku", true),
            row("WMS-2", "Code review", true),
        ];
        assert!(sync_assigned(&mut connection, site, &third, false, 300)
            .unwrap()
            .is_empty());
        let s = db::snapshot(&connection).unwrap();
        assert_eq!(s.tasks.len(), 1);
        assert_eq!(s.tasks[0].jira_status.as_deref(), Some("Code review"));
        // Usunięte lokalnie zadanie nie wraca, dopóki zgłoszenie jest przypisane.
        db::change(&mut connection, "delete", Some(s.tasks[0].id), None, 400).unwrap();
        assert!(sync_assigned(&mut connection, site, &third, false, 500)
            .unwrap()
            .is_empty());
        // Zgłoszenie z istniejącym lokalnym zadaniem (Rozpocznij) nie jest dublowane.
        start_issue(&mut connection, site, "WMS-3", "Ręcznie", 600).unwrap();
        let fourth = vec![row("WMS-3", "W toku", true)];
        assert!(sync_assigned(&mut connection, site, &fourth, false, 700)
            .unwrap()
            .is_empty());
        // WMS-1 zdjęte z listy i przypisane ponownie: import.
        let again = vec![
            row("WMS-1", "Do zrobienia", true),
            row("WMS-3", "W toku", true),
        ];
        let imported = sync_assigned(&mut connection, site, &again, false, 800).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!(imported[0].key, "WMS-1");
        // Zamknięte zgłoszenie nie jest importowane.
        let closed = vec![row("WMS-5", "Gotowe", false)];
        assert!(sync_assigned(&mut connection, site, &closed, false, 900)
            .unwrap()
            .is_empty());
    }
    #[test]
    fn imported_message_lists_first_three() {
        let (title, body) = imported_message(&[row("WMS-1", "", true)]);
        assert_eq!(title, "Nowe zadanie z Jiry");
        assert!(body.starts_with("WMS-1 · Zgłoszenie WMS-1"));
        let rows: Vec<_> = (1..=5)
            .map(|i| row(&format!("WMS-{i}"), "", true))
            .collect();
        let (title, body) = imported_message(&rows);
        assert_eq!(title, "Nowe z Jiry: 5 zadań");
        assert!(body.contains("i jeszcze 2"));
    }
    #[test]
    fn imported_notification_offers_start_only_for_single_issue() {
        use crate::notify::{Action, View, JIRA_START, PLAN_TODAY};
        let site = "https://a.atlassian.net";
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&connection).unwrap();
        sync_assigned(&mut connection, site, &[], true, 100).unwrap();
        let one =
            sync_assigned(&mut connection, site, &[row("WMS-1", "", true)], false, 200).unwrap();
        let ids = imported_task_ids(&connection, site, &one).unwrap();
        let m = imported_notification(&one, &ids);
        assert_eq!(m.title, "Nowe zadanie z Jiry");
        assert_eq!(
            m.actions,
            [
                Action::new(JIRA_START, "Rozpocznij", ids[0]),
                Action::new(PLAN_TODAY, "Zaplanuj na dziś", ids[0]),
            ]
        );
        assert_eq!(m.actions[0].argument(), format!("jira-start:{}", ids[0]));
        assert_eq!(
            m.open,
            View {
                section: Some("Nierozpoczęte"),
                task: Some(ids[0]),
                ..View::tasks()
            }
        );
        let two = sync_assigned(
            &mut connection,
            site,
            &[
                row("WMS-1", "", true),
                row("WMS-2", "", true),
                row("WMS-3", "", true),
            ],
            false,
            300,
        )
        .unwrap();
        let ids = imported_task_ids(&connection, site, &two).unwrap();
        assert_eq!(ids.len(), 2);
        let m = imported_notification(&two, &ids);
        assert_eq!(m.actions.len(), 1);
        assert_eq!(
            m.actions[0].argument(),
            format!("plan-today:{},{}", ids[0], ids[1])
        );
        assert_eq!(m.open.task, None);
        // „Zaplanuj na dziś” ustawia dzień planu obu zadań, na końcu dnia.
        let out = crate::notify::perform(
            &mut connection,
            crate::notify::command(Some(&m.actions[0].argument()), &m.open),
            "2026-10-15",
            400,
        )
        .unwrap();
        assert!(out.changed && out.navigate.is_none());
        let s = db::snapshot(&connection).unwrap();
        for id in &ids {
            let task = s.tasks.iter().find(|t| t.id == *id).unwrap();
            assert_eq!(task.details.planned_day, "2026-10-15");
        }
        assert!(s.sessions.is_empty(), "planowanie nie uruchamia pomiaru");
    }
    #[test]
    fn notification_start_restores_trashed_issue_like_jira_view() {
        use crate::notify::{command, perform, View};
        let site = "https://a.atlassian.net";
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&connection).unwrap();
        db::change(&mut connection, "add", None, Some("Lokalne".into()), 0).unwrap();
        db::change(&mut connection, "start", Some(1), None, 50).unwrap();
        sync_assigned(&mut connection, site, &[], true, 100).unwrap();
        let rows =
            sync_assigned(&mut connection, site, &[row("WMS-4", "", true)], false, 200).unwrap();
        let id = imported_task_ids(&connection, site, &rows).unwrap()[0];
        db::change(&mut connection, "delete", Some(id), None, 300).unwrap();
        assert_eq!(db::snapshot(&connection).unwrap().trash.len(), 1);
        let m = imported_notification(&rows, &[id]);
        let out = perform(
            &mut connection,
            command(Some(&m.actions[0].argument()), &View::default()),
            "2026-10-15",
            400,
        )
        .unwrap();
        assert!(out.changed);
        assert_eq!(out.navigate, None);
        let s = db::snapshot(&connection).unwrap();
        assert!(s.trash.is_empty());
        assert_eq!(s.tasks.len(), 2);
        // Jeden aktywny pomiar: poprzedni zamknięty, nowy na zadaniu z Jiry.
        let active: Vec<_> = s.sessions.iter().filter(|x| x.ended_at.is_none()).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].task_id, id);
        assert_eq!(
            s.sessions.iter().find(|x| x.task_id == 1).unwrap().ended_at,
            Some(400)
        );
        // Zadanie bez powiązania z Jirą: błąd, bez zmian.
        assert!(perform(
            &mut connection,
            command(Some("jira-start:1"), &View::default()),
            "2026-10-15",
            500,
        )
        .is_err());
    }
    #[test]
    fn parses_transitions_with_target_category() {
        let value = serde_json::json!({"expand":"transitions","transitions":[
            {"id":"21","name":"Rozpocznij","to":{"name":"In Progress","statusCategory":{"key":"indeterminate"}}},
            {"id":31,"name":"","to":{"name":"Gotowe","statusCategory":{"key":"done"}}},
            {"id":"x/../1","name":"Zły","to":{"name":"A"}},
            {"name":"Bez id"}
        ]});
        assert_eq!(
            parse_transitions(&value).unwrap(),
            vec![
                Transition {
                    id: "21".into(),
                    name: "Rozpocznij".into(),
                    to_status: "In Progress".into(),
                    to_category: "indeterminate".into(),
                },
                Transition {
                    id: "31".into(),
                    name: "Gotowe".into(),
                    to_status: "Gotowe".into(),
                    to_category: "done".into(),
                },
            ]
        );
        assert!(parse_transitions(&serde_json::json!({"issues":[]})).is_err());
        assert!(valid_transition_id("123"));
        assert!(!valid_transition_id(""));
        assert!(!valid_transition_id("1?x"));
    }
    #[test]
    fn write_errors_are_polish_with_jira_details() {
        let body = serde_json::json!({"errorMessages":["Brak pola"],"errors":{"resolution":"Resolution is required."}});
        let e = write_error(StatusCode::BAD_REQUEST, &body);
        assert!(e.starts_with("Jira odrzuciła zmianę."), "{e}");
        assert!(e.contains("Brak pola") && e.contains("Resolution is required."));
        let e = write_error(StatusCode::FORBIDDEN, &Value::Null);
        assert_eq!(e, "Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
        assert!(write_error(StatusCode::NOT_FOUND, &Value::Null).starts_with("Nie znaleziono"));
        assert!(write_error(StatusCode::UNAUTHORIZED, &Value::Null).contains("token API"));
        // Szczegóły są przycinane.
        let long = serde_json::json!({"errorMessages":["x".repeat(1000)]});
        assert!(write_error(StatusCode::BAD_REQUEST, &long).chars().count() < 450);
    }
    /// Serwer HTTP na jedno połączenie: zwraca żądanie (nagłówki i treść) i odpowiada
    /// podanym statusem i treścią.
    fn serve_once(status: &'static str, body: String) -> (String, std::thread::JoinHandle<String>) {
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
            loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buffer[..n]);
                let text = String::from_utf8_lossy(&request).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let length = text
                        .lines()
                        .find_map(|l| {
                            l.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(socket,"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            String::from_utf8(request).unwrap()
        });
        (format!("http://{address}"), server)
    }
    fn test_config(site: String) -> Config {
        Config {
            site,
            email: "test@example.invalid".into(),
            ..Config::default()
        }
    }
    #[test]
    fn transition_post_sends_id_and_maps_errors() {
        let (site, server) = serve_once("204 No Content", String::new());
        let c = test_config(site);
        let ok = tauri::async_runtime::block_on(send_write(
            &client().unwrap(),
            &c,
            "test-only",
            reqwest::Method::POST,
            "/rest/api/3/issue/WMS-1/transitions",
            &serde_json::json!({"transition":{"id":"21"}}),
        ))
        .unwrap();
        assert_eq!(ok, Value::Null);
        let request = server.join().unwrap();
        assert!(request.starts_with("POST /rest/api/3/issue/WMS-1/transitions "));
        assert!(request.ends_with(r#"{"transition":{"id":"21"}}"#));
        // 400: przejście z ekranem wymagających pól.
        let (site, server) = serve_once(
            "400 Bad Request",
            serde_json::json!({"errorMessages":[],"errors":{"resolution":"Pole wymagane"}})
                .to_string(),
        );
        let e = tauri::async_runtime::block_on(send_write(
            &client().unwrap(),
            &test_config(site),
            "test-only",
            reqwest::Method::POST,
            "/rest/api/3/issue/WMS-1/transitions",
            &serde_json::json!({"transition":{"id":"21"}}),
        ))
        .unwrap_err();
        assert!(e.starts_with("Jira odrzuciła zmianę.") && e.contains("Pole wymagane"));
        server.join().unwrap();
        // 403: brak uprawnień.
        let (site, server) = serve_once("403 Forbidden", "{}".into());
        let e = tauri::async_runtime::block_on(send_write(
            &client().unwrap(),
            &test_config(site),
            "test-only",
            reqwest::Method::POST,
            "/rest/api/3/issue/WMS-1/transitions",
            &serde_json::json!({"transition":{"id":"21"}}),
        ))
        .unwrap_err();
        assert_eq!(e, "Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
        server.join().unwrap();
        // Ścieżka spoza REST nie wychodzi do sieci.
        assert!(tauri::async_runtime::block_on(send_write(
            &client().unwrap(),
            &c,
            "test-only",
            reqwest::Method::POST,
            "/rest/../x",
            &Value::Null,
        ))
        .is_err());
    }
    #[test]
    fn reads_issue_status_and_transitions() {
        let (site, server) = serve_once(
            "200 OK",
            serde_json::json!({"key":"WMS-1","fields":{"status":{"name":"Do zrobienia","statusCategory":{"key":"new"}}}}).to_string(),
        );
        let c = test_config(site);
        let status =
            tauri::async_runtime::block_on(issue_status(&client().unwrap(), &c, "t", "WMS-1"))
                .unwrap();
        assert_eq!(status, ("Do zrobienia".to_string(), "new".to_string()));
        assert!(server
            .join()
            .unwrap()
            .starts_with("GET /rest/api/3/issue/WMS-1?fields=status "));
        let (site, server) = serve_once(
            "200 OK",
            serde_json::json!({"transitions":[{"id":"21","name":"Start","to":{"name":"In Progress","statusCategory":{"key":"indeterminate"}}}]}).to_string(),
        );
        let list = tauri::async_runtime::block_on(transitions(
            &client().unwrap(),
            &test_config(site),
            "t",
            "WMS-1",
        ))
        .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].to_status, "In Progress");
        assert!(server
            .join()
            .unwrap()
            .starts_with("GET /rest/api/3/issue/WMS-1/transitions "));
    }
    #[test]
    fn validates_cloud_address_and_issue_keys() {
        assert_eq!(
            normalize_site("https://asistonwms.atlassian.net/").unwrap(),
            "https://asistonwms.atlassian.net"
        );
        for bad in [
            "http://asistonwms.atlassian.net",
            "https://atlassian.net.evil.com",
            "https://a.atlassian.net/path",
            "https://user@a.atlassian.net",
            "https://a.atlassian.net?x=1",
        ] {
            assert!(normalize_site(bad).is_err());
        }
        // Linki do przeglądarki: zgłoszenie i tworzenie zgłoszenia (J2), nic więcej.
        let link = |u: &str| allowed_link(&Url::parse(u).unwrap());
        assert!(link("https://a.atlassian.net/browse/WMS-1"));
        assert!(link(&format!("https://a.atlassian.net{CREATE_PATH}")));
        assert!(!link(
            "https://a.atlassian.net/secure/CreateIssue!default.jspa?pid=1"
        ));
        assert!(!link("https://a.atlassian.net/secure/Other.jspa"));
        assert!(!link("http://a.atlassian.net/browse/WMS-1"));
        assert!(valid_key("WMS-123"));
        assert!(!valid_key("../123"));
        assert!(!valid_key("WMS-1?x"));
    }
    #[test]
    fn merges_issues_across_boards() {
        let c = Config::default();
        let mut rows = BTreeMap::new();
        let value = serde_json::json!({"issues":[{"key":"WMS-1","fields":{"summary":"Test","status":{"name":"W toku"}}}]});
        merge_issues(
            &mut rows,
            &value,
            &c,
            &Board {
                id: 1,
                name: "A".into(),
            },
        )
        .unwrap();
        merge_issues(
            &mut rows,
            &value,
            &c,
            &Board {
                id: 2,
                name: "B".into(),
            },
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows["WMS-1"].boards, vec!["A", "B"]);
    }
    #[test]
    fn import_start_is_atomic_idempotent_and_preserves_history() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        db::init(&connection).unwrap();
        db::change(&mut connection, "add", None, Some("Lokalne".into()), 0).unwrap();
        db::change(&mut connection, "start", Some(1), None, 100).unwrap();
        let s = start_issue(
            &mut connection,
            "https://a.atlassian.net",
            "WMS-1",
            "Z Jiry",
            200,
        )
        .unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.sessions[1].ended_at, Some(200));
        assert_eq!(s.tasks[0].jira_key.as_deref(), Some("WMS-1"));
        let s = start_issue(
            &mut connection,
            "https://a.atlassian.net",
            "WMS-1",
            "Z Jiry",
            300,
        )
        .unwrap();
        assert_eq!(s.tasks.len(), 2);
        assert_eq!(s.sessions.len(), 2);
        db::change(&mut connection, "toggle", Some(2), None, 400).unwrap();
        let s = start_issue(
            &mut connection,
            "https://a.atlassian.net",
            "WMS-1",
            "Z Jiry",
            500,
        )
        .unwrap();
        assert!(!s.tasks[0].done);
        assert_eq!(s.sessions.len(), 3);
        db::change(&mut connection, "delete", Some(2), None, 600).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM jira_tasks", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let restored = start_issue(
            &mut connection,
            "https://a.atlassian.net",
            "WMS-1",
            "Z Jiry",
            700,
        )
        .unwrap();
        assert!(restored.trash.is_empty());
        assert_eq!(restored.tasks.len(), 2);
        assert_eq!(restored.sessions.len(), 4);
    }
}
