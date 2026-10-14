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
fn config(app: &tauri::AppHandle) -> Result<Config, String> {
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
fn valid_key(key: &str) -> bool {
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
fn token(app: &tauri::AppHandle, c: &Config) -> Result<String, String> {
    entry(app, c)?
        .get_password()
        .map_err(|_| "Połącz konto Jiry: podaj e-mail i token API.".into())
}
fn client() -> Result<Client, String> {
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
async fn get(
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
    let mut cursor = String::new();
    let mut seen = HashSet::new();
    for _ in 0..200 {
        let mut query = vec![
            ("jql", jql.into()),
            ("maxResults", "100".into()),
            ("fields", "summary,status".into()),
        ];
        if !cursor.is_empty() {
            query.push(("nextPageToken", cursor.clone()));
        }
        let value = get(
            client,
            c,
            secret,
            &format!("/rest/software/1.0/board/{}/issue", board.id),
            &query,
        )
        .await?;
        merge_issues(&mut result, &value, c, board)?;
        if value["isLast"].as_bool() == Some(true) {
            return Ok(result);
        }
        match value["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
            Some(next) if seen.insert(next.to_owned()) => cursor = next.to_owned(),
            None if value["isLast"].as_bool() != Some(false) => return Ok(result),
            _ => {
                return Err(
                    "Nie można pobrać kolejnej strony wyników Jiry. Odśwież tablicę.".into(),
                )
            }
        }
    }
    Err("Tablica zwróciła zbyt wiele wyników. Ogranicz wybór do otwartych zadań.".into())
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
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || !parsed
            .path()
            .strip_prefix("/browse/")
            .is_some_and(valid_key)
    {
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
