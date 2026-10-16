//! Wykrywanie pracujących agentów AI (Claude Code i Codex) na tym komputerze.
//!
//! - Claude Code zapisuje stan każdej sesji w `~/.claude/sessions/<pid>.json`
//!   (`status`: `busy`/`idle`). Sesja pracuje, gdy ma `busy` i jej proces żyje.
//!   Gdy `busy` trwa dłużej niż `STALL`, wymagamy też świeżego zapisu transkryptu
//!   (`~/.claude/projects/*/<sessionId>.jsonl`), żeby zawieszona sesja nie
//!   blokowała wykrywania bezczynności w nieskończoność.
//! - Codex dopisuje zdarzenia do `~/.codex/sessions/RRRR/MM/DD/rollout-*.jsonl`.
//!   Sesja pracuje, gdy jej plik zmienił się w ciągu `CODEX_FRESH`.
//!
//! Wątek co 5 s odświeża listę, wysyła `agents-changed` przy zmianie i zapamiętuje
//! ostatnią chwilę pracy agenta, z której korzysta `idle`.
//!
//! Rozpoznanie zadania (PLAN_1.3.md, etap 4): gałąź git folderu roboczego
//! (`git_branch`, bez uruchamiania procesu `git`, cache 30 s na katalog), klucz Jiry
//! z gałęzi albo z nazwy sesji Claude Code i dopasowanie do zadania (`resolve`):
//! przypisanie folderu (`agent_bindings`) → klucz powiązany z lokalnym zadaniem →
//! klucz znany z `jira_issues` (zadanie powstanie dopiero przy akceptacji).
//! Dopasowanie tylko opisuje pracę agenta; timer nigdy nie przełącza się sam.
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};
use tauri::{Emitter, Manager};

/// Co ile wątek sprawdza agentów.
const CHECK: Duration = Duration::from_secs(5);
/// Zapis rolloutu Codex młodszy niż ten próg oznacza pracę.
const CODEX_FRESH: i64 = 90_000;
/// Po takim czasie `busy` sesja Claude musi mieć świeży transkrypt.
const STALL: i64 = 15 * 60_000;
/// Jak długo pamiętamy odczytaną gałąź katalogu.
const BRANCH_TTL: i64 = 30_000;

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// `claude` albo `codex`.
    pub kind: String,
    /// Nazwa sesji, a bez niej nazwa folderu roboczego.
    pub name: String,
    pub cwd: String,
    /// Od kiedy pracuje (ms).
    pub since: i64,
    /// Nazwa sesji Claude Code nadana przez użytkownika (źródło klucza bez gałęzi).
    #[serde(skip)]
    pub session: Option<String>,
    /// Gałąź git folderu roboczego, a przy odłączonym HEAD skrót commita.
    pub branch: Option<String>,
    /// `true`, gdy HEAD jest odłączony (`branch` to wtedy skrót commita).
    pub detached: bool,
    /// Klucz Jiry z gałęzi, a bez niego z nazwy sesji.
    pub key: Option<String>,
    /// Dopasowane zadanie.
    pub task: Option<Match>,
}

/// Zadanie, nad którym pracuje agent.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    /// Lokalne zadanie; `None`, gdy klucz zna tylko lista zgłoszeń z Jiry.
    pub task_id: Option<i64>,
    /// Tytuł zadania albo zgłoszenia (pusty, gdy tytuł zgłoszenia nie jest znany).
    pub title: String,
    pub key: Option<String>,
    /// `folder` (przypisanie folderu), `task` (klucz lokalnego zadania), `jira`.
    pub source: String,
    /// Lokalne zadanie jest ukończone: bez propozycji przełączenia i przypisania
    /// przerwy (start wymagałby przywrócenia go do zrobienia).
    pub done: bool,
}

/// HEAD repozytorium.
#[derive(Clone, Debug, PartialEq)]
pub enum Head {
    Branch(String),
    /// Odłączony HEAD: skrót commita (7 znaków).
    Detached(String),
}

/// Katalog git dla `cwd`: idzie w górę do `.git`. Plik `.git` z `gitdir:` (worktree,
/// submoduł) wskazuje katalog względny wobec folderu, w którym leży.
fn git_dir(cwd: &Path) -> Option<PathBuf> {
    if !cwd.is_absolute() {
        return None;
    }
    for dir in cwd.ancestors() {
        let dot = dir.join(".git");
        let Ok(meta) = std::fs::metadata(&dot) else {
            continue;
        };
        if meta.is_dir() {
            return Some(dot);
        }
        let text = std::fs::read_to_string(&dot).ok()?;
        let target = text
            .lines()
            .find_map(|l| l.trim_start_matches('\u{feff}').strip_prefix("gitdir:"))?
            .trim();
        if target.is_empty() {
            return None;
        }
        let path = Path::new(target);
        return Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            dir.join(path)
        });
    }
    None
}

/// Gałąź (albo odłączony HEAD) repozytorium, w którym leży `cwd`; czyta plik `HEAD`
/// bez uruchamiania procesu `git`. `None` poza repozytorium.
pub fn git_branch(cwd: &Path) -> Option<Head> {
    let head = std::fs::read_to_string(git_dir(cwd)?.join("HEAD")).ok()?;
    let head = head.trim();
    if let Some(name) = head.strip_prefix("ref:") {
        let name = name.trim();
        let name = name.strip_prefix("refs/heads/").unwrap_or(name);
        return (!name.is_empty()).then(|| Head::Branch(name.to_string()));
    }
    (head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| Head::Detached(head[..7].to_string()))
}

/// Pierwszy klucz Jiry (`[A-Z][A-Z0-9]+-\d+`) w tekście, np. `feature/WMS-123-eksport`.
/// Klucz nie może być doklejony do poprzedzającej litery lub cyfry.
pub fn jira_key(text: &str) -> Option<String> {
    let b = text.as_bytes();
    for start in 0..b.len() {
        if !b[start].is_ascii_uppercase() || (start > 0 && b[start - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let mut dash = start + 1;
        while dash < b.len() && (b[dash].is_ascii_uppercase() || b[dash].is_ascii_digit()) {
            dash += 1;
        }
        if dash - start < 2 || b.get(dash) != Some(&b'-') {
            continue;
        }
        let mut end = dash + 1;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        if end > dash + 1 {
            return Some(text[start..end].to_string());
        }
    }
    None
}

/// Klucz agenta: z gałęzi, a gdy go tam nie ma, z nazwy sesji Claude Code.
fn agent_key(agent: &Agent) -> Option<String> {
    agent
        .branch
        .as_deref()
        .filter(|_| !agent.detached)
        .and_then(jira_key)
        .or_else(|| agent.session.as_deref().and_then(jira_key))
}

/// Folder do porównań: ukośniki `/`, bez końcowego separatora, małe litery
/// (ścieżki Windows nie rozróżniają wielkości liter).
pub fn norm_folder(folder: &str) -> String {
    folder
        .trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Przypisanie folderu obejmuje też jego podfoldery; wygrywa najdłuższe.
pub(crate) fn bound_task(db: &Connection, cwd: &str) -> rusqlite::Result<Option<Match>> {
    let cwd = norm_folder(cwd);
    if cwd.is_empty() {
        return Ok(None);
    }
    let rows = db
        .prepare(
            "SELECT b.folder,t.id,t.title,j.issue_key,t.done FROM agent_bindings b JOIN tasks t ON t.id=b.task_id
             LEFT JOIN task_lifecycle l ON l.task_id=t.id LEFT JOIN jira_tasks j ON j.task_id=t.id
             WHERE l.deleted_at IS NULL",
        )?
        .query_map([], |r| {
            Ok((
                norm_folder(&r.get::<_, String>(0)?),
                Match {
                    task_id: Some(r.get(1)?),
                    title: r.get(2)?,
                    key: r.get(3)?,
                    source: "folder".into(),
                    done: r.get(4)?,
                },
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .filter(|(f, _)| !f.is_empty() && (cwd == *f || cwd.starts_with(&format!("{f}/"))))
        .max_by_key(|(f, _)| f.len())
        .map(|(_, m)| m))
}

/// Dopasowanie zadania w kolejności z planu: przypisanie folderu → klucz powiązany
/// z lokalnym zadaniem (poza koszem) → klucz znany z `jira_issues`.
pub fn resolve(db: &Connection, cwd: &str, key: Option<&str>) -> rusqlite::Result<Option<Match>> {
    if let Some(found) = bound_task(db, cwd)? {
        return Ok(Some(found));
    }
    let Some(key) = key else {
        return Ok(None);
    };
    let local = db
        .query_row(
            "SELECT t.id,t.title,t.done FROM jira_tasks j JOIN tasks t ON t.id=j.task_id
             LEFT JOIN task_lifecycle l ON l.task_id=t.id
             WHERE j.issue_key=?1 AND l.deleted_at IS NULL ORDER BY t.done, t.id DESC LIMIT 1",
            [key],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((id, title, done)) = local {
        return Ok(Some(Match {
            task_id: Some(id),
            title,
            key: Some(key.into()),
            source: "task".into(),
            done,
        }));
    }
    let known: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM jira_issues WHERE issue_key=?1)",
        [key],
        |r| r.get(0),
    )?;
    Ok(known.then(|| Match {
        task_id: None,
        title: String::new(),
        key: Some(key.into()),
        source: "jira".into(),
        done: false,
    }))
}

/// Gałęzie katalogów z czasem odczytu.
#[derive(Default)]
struct Branches(HashMap<String, (i64, Option<Head>)>);
impl Branches {
    /// Gałąź z cache albo z `read`, gdy wpis jest starszy niż `BRANCH_TTL`.
    fn get(&mut self, cwd: &str, now: i64, read: impl Fn(&Path) -> Option<Head>) -> Option<Head> {
        if let Some((at, head)) = self.0.get(cwd) {
            if now - at < BRANCH_TTL && now >= *at {
                return head.clone();
            }
        }
        self.0
            .retain(|_, (at, _)| now - *at < BRANCH_TTL && now >= *at);
        let head = read(Path::new(cwd));
        self.0.insert(cwd.to_string(), (now, head.clone()));
        head
    }
    /// Dopisuje gałąź (z cache 30 s, inaczej z `read`) i klucz Jiry.
    fn recognize(&mut self, agents: &mut [Agent], now: i64, read: impl Fn(&Path) -> Option<Head>) {
        for agent in agents {
            (agent.branch, agent.detached) = match self.get(&agent.cwd, now, &read) {
                Some(Head::Branch(name)) => (Some(name), false),
                Some(Head::Detached(hash)) => (Some(hash), true),
                None => (None, false),
            };
            agent.key = agent_key(agent);
        }
    }
}

/// Folder przypisany do zadania (`agent_bindings`).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Binding {
    pub folder: String,
    pub task_id: i64,
    pub title: String,
}

pub fn bindings(db: &Connection) -> rusqlite::Result<Vec<Binding>> {
    db.prepare(
        "SELECT b.folder,b.task_id,t.title FROM agent_bindings b JOIN tasks t ON t.id=b.task_id ORDER BY b.folder COLLATE NOCASE",
    )?
    .query_map([], |r| {
        Ok(Binding {
            folder: r.get(0)?,
            task_id: r.get(1)?,
            title: r.get(2)?,
        })
    })?
    .collect()
}

/// Przypisuje folder do zadania (zastępuje wcześniejsze przypisanie tego folderu).
pub fn bind(db: &Connection, folder: &str, task_id: i64) -> Result<Vec<Binding>, String> {
    let folder = canon_folder(folder);
    if folder.is_empty() || folder.chars().count() > 1024 {
        return Err("Niepoprawny folder.".into());
    }
    let live: bool = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM tasks t LEFT JOIN task_lifecycle l ON l.task_id=t.id WHERE t.id=?1 AND l.deleted_at IS NULL)",
            [task_id],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !live {
        return Err("Nie znaleziono zadania.".into());
    }
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    drop_folder(&tx, &folder).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO agent_bindings(folder,task_id) VALUES(?1,?2) ON CONFLICT(folder) DO UPDATE SET folder=excluded.folder, task_id=excluded.task_id",
        params![folder, task_id],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    // Okresy agentów z ostatnich 7 dni bez zadania dostają zadanie z przypisania.
    crate::agent_time::backfill(db, db::now()).map_err(|e| e.to_string())?;
    bindings(db).map_err(|e| e.to_string())
}

pub fn unbind(db: &Connection, folder: &str) -> Result<Vec<Binding>, String> {
    drop_folder(db, folder).map_err(|e| e.to_string())?;
    bindings(db).map_err(|e| e.to_string())
}

/// Postać zapisywana w `agent_bindings`: separatory systemu (w Windows `\`), bez
/// końcowego separatora; wielkość liter zostaje do wyświetlania.
fn canon_folder(folder: &str) -> String {
    let sep = std::path::MAIN_SEPARATOR;
    folder
        .trim()
        .replace(['\\', '/'], &sep.to_string())
        .trim_end_matches(sep)
        .to_string()
}

/// Usuwa każde przypisanie tego samego folderu (także zapisane wcześniej innymi
/// separatorami albo inną wielkością liter).
fn drop_folder(db: &Connection, folder: &str) -> rusqlite::Result<()> {
    let norm = norm_folder(folder);
    let same: Vec<String> = db
        .prepare("SELECT folder FROM agent_bindings")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|f| norm_folder(f) == norm)
        .collect();
    for f in same {
        db.execute("DELETE FROM agent_bindings WHERE folder=?1", [f])?;
    }
    Ok(())
}

#[derive(Default)]
pub struct State(Mutex<Watch>);
#[derive(Default)]
struct Watch {
    agents: Vec<Agent>,
    /// Ostatnia chwila, w której jakiś agent pracował.
    last_busy: Option<i64>,
    /// Codex: od kiedy plik jest świeży (start pracy) i odczytany folder roboczy.
    codex_since: HashMap<PathBuf, i64>,
    codex_cwd: HashMap<PathBuf, String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeSession {
    pid: u32,
    session_id: Option<String>,
    status: Option<String>,
    #[serde(default)]
    cwd: String,
    name: Option<String>,
    status_updated_at: Option<i64>,
    updated_at: Option<i64>,
}

fn folder(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string()
}

fn modified_ms(path: &Path) -> Option<i64> {
    let time = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(time.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

/// Sesja Claude z pliku stanu, gdy pracuje. `alive` sprawdza proces, `transcript`
/// zwraca czas ostatniego zapisu transkryptu sesji.
fn claude_agent(
    raw: &str,
    now: i64,
    alive: impl Fn(u32) -> bool,
    transcript: impl Fn(&str) -> Option<i64>,
) -> Option<Agent> {
    let s: ClaudeSession = serde_json::from_str(raw).ok()?;
    if s.status.as_deref() != Some("busy") || !alive(s.pid) {
        return None;
    }
    let since = s.status_updated_at.or(s.updated_at).unwrap_or(now);
    if now - since > STALL {
        let fresh = s
            .session_id
            .as_deref()
            .and_then(&transcript)
            .is_some_and(|at| now - at <= STALL);
        if !fresh {
            return None;
        }
    }
    let session = s.name.filter(|n| !n.trim().is_empty());
    let name = session.clone().unwrap_or_else(|| folder(&s.cwd));
    Some(Agent {
        kind: "claude".into(),
        name,
        cwd: s.cwd,
        since,
        session,
        ..Agent::default()
    })
}

fn claude_agents(home: &Path, now: i64) -> Vec<Agent> {
    let Ok(entries) = std::fs::read_dir(home.join("sessions")) else {
        return Vec::new();
    };
    let projects = home.join("projects");
    let transcript = |id: &str| -> Option<i64> {
        std::fs::read_dir(&projects)
            .ok()?
            .flatten()
            .filter_map(|dir| modified_ms(&dir.path().join(format!("{id}.jsonl"))))
            .max()
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .filter_map(|raw| claude_agent(&raw, now, process_alive, transcript))
        .collect()
}

/// Folder roboczy z pierwszej linii rolloutu (`session_meta`).
fn codex_cwd(path: &Path) -> String {
    use std::io::BufRead;
    let mut line = String::new();
    let read = std::fs::File::open(path)
        .map(std::io::BufReader::new)
        .and_then(|mut r| r.read_line(&mut line));
    if read.is_err() {
        return String::new();
    }
    serde_json::from_str::<serde_json::Value>(&line)
        .ok()
        .and_then(|v| v["payload"]["cwd"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Świeże pliki rollout (ścieżka, czas zapisu) w całym drzewie `sessions`.
fn codex_fresh(home: &Path, now: i64) -> Vec<(PathBuf, i64)> {
    let mut out = Vec::new();
    let mut dirs = vec![home.join("sessions")];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(path);
            } else if path.extension().is_some_and(|x| x == "jsonl") {
                if let Some(at) = modified_ms(&path).filter(|at| now - at <= CODEX_FRESH) {
                    out.push((path, at));
                }
            }
        }
    }
    out
}

impl Watch {
    fn codex_agents(&mut self, fresh: Vec<(PathBuf, i64)>, now: i64) -> Vec<Agent> {
        self.codex_since
            .retain(|p, _| fresh.iter().any(|(f, _)| f == p));
        fresh
            .into_iter()
            .map(|(path, _)| {
                let since = *self.codex_since.entry(path.clone()).or_insert(now);
                let cwd = self
                    .codex_cwd
                    .entry(path.clone())
                    .or_insert_with(|| codex_cwd(&path))
                    .clone();
                Agent {
                    kind: "codex".into(),
                    name: folder(&cwd),
                    cwd,
                    since,
                    ..Agent::default()
                }
            })
            .collect()
    }
    /// Zapisuje nową listę; `true`, gdy się zmieniła.
    fn update(&mut self, mut agents: Vec<Agent>, now: i64) -> bool {
        agents.sort_by(|a, b| (a.since, &a.kind, &a.name).cmp(&(b.since, &b.kind, &b.name)));
        if !agents.is_empty() {
            self.last_busy = Some(now);
        }
        let changed = agents != self.agents;
        self.agents = agents;
        changed
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
fn claude_home() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".claude")))
}
fn codex_home() -> Option<PathBuf> {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| home().map(|h| h.join(".codex")))
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    type Handle = *mut std::ffi::c_void;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
        fn GetExitCodeProcess(process: Handle, code: *mut u32) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
    }
    const QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let handle = OpenProcess(QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}
#[cfg(not(windows))]
fn process_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// Rejestruje stan agentów; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Pętla co 5 s. Kończy się, gdy okno main zniknie, więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Cache gałęzi należy tylko do tego wątku.
        let mut branches = Branches::default();
        loop {
            if app.get_webview_window("main").is_none() {
                break;
            }
            let now = db::now();
            let mut found = claude_home()
                .map(|h| claude_agents(&h, now))
                .unwrap_or_default();
            let fresh = codex_home()
                .map(|h| codex_fresh(&h, now))
                .unwrap_or_default();
            let codex = app
                .state::<State>()
                .0
                .lock()
                .ok()
                .map(|mut w| w.codex_agents(fresh, now));
            if let Some(codex) = codex {
                found.extend(codex);
                // `.git` (także z dysku sieciowego) czytany poza blokadą stanu, żeby
                // `agents_status` w wątku głównym na niego nie czekał.
                branches.recognize(&mut found, now, git_branch);
                match_tasks(&app, &mut found);
                // Czas agentów jako osobny tor (`agent_sessions`).
                crate::agent_time::observe(&app, &found, now);
                let changed = app
                    .state::<State>()
                    .0
                    .lock()
                    .ok()
                    .and_then(|mut w| w.update(found, now).then(|| w.agents.clone()));
                if let Some(agents) = changed {
                    let _ = app.emit("agents-changed", agents);
                }
                // Aktywne okno (W3, gdy włączone w ustawieniach): sygnał trafia do
                // tej samej reguły propozycji co agenci.
                crate::foreground::observe(&app, now, |p| {
                    branches.get(&p.to_string_lossy(), now, git_branch)
                });
                // Propozycje przełączenia liczone co przebieg: zależą też od czasu
                // pracy agenta (2 min) i od bieżącego pomiaru.
                crate::suggest::refresh(&app, now);
                // Rozszerzenie przeglądarki: ponowienie nieudanej rejestracji hosta.
                crate::browser::retry(&app, now);
            }
            std::thread::sleep(CHECK);
        }
    });
}

/// Dopasowuje zadania agentów (`resolve`); tytuł zgłoszenia znanego tylko z Jiry
/// pochodzi z listy wczytanej w tej sesji (`jira::cached_summary`).
fn match_tasks(app: &tauri::AppHandle, agents: &mut [Agent]) {
    {
        let state = app.state::<crate::Database>();
        let Ok(db) = state.0.lock() else {
            return;
        };
        for agent in agents.iter_mut() {
            agent.task = resolve(&db, &agent.cwd, agent.key.as_deref())
                .ok()
                .flatten();
        }
    }
    for found in agents.iter_mut().filter_map(|a| a.task.as_mut()) {
        if found.title.is_empty() {
            if let Some(key) = found.key.clone() {
                found.title = crate::jira::cached_summary(app, &key).unwrap_or_default();
            }
        }
    }
}

/// Ponowne dopasowanie po zmianie przypisań folderów, bez czekania na pętlę.
fn rematch(app: &tauri::AppHandle) {
    let Some(mut agents) = app.state::<State>().0.lock().ok().map(|w| w.agents.clone()) else {
        return;
    };
    match_tasks(app, &mut agents);
    let changed = app.state::<State>().0.lock().ok().and_then(|mut w| {
        let same = |a: &Agent, b: &Agent| a.kind == b.kind && a.cwd == b.cwd && a.since == b.since;
        let mut changed = false;
        for current in w.agents.iter_mut() {
            if let Some(next) = agents.iter().find(|a| same(a, current)) {
                if current.task != next.task {
                    current.task = next.task.clone();
                    changed = true;
                }
            }
        }
        changed.then(|| w.agents.clone())
    });
    if let Some(agents) = changed {
        let _ = app.emit("agents-changed", agents);
    }
    crate::suggest::refresh(app, db::now());
}

/// Agenci pracujący teraz (ostatni odczyt pętli).
pub fn current(app: &tauri::AppHandle) -> Vec<Agent> {
    app.state::<State>()
        .0
        .lock()
        .map(|w| w.agents.clone())
        .unwrap_or_default()
}

/// Ile ms minęło od ostatniej pracy agenta (`u32::MAX`, gdy nigdy).
pub fn idle_ms(app: &tauri::AppHandle, now: i64) -> u32 {
    app.state::<State>()
        .0
        .lock()
        .ok()
        .and_then(|w| w.last_busy)
        .map(|at| (now - at).clamp(0, i64::from(u32::MAX)) as u32)
        .unwrap_or(u32::MAX)
}

#[tauri::command]
pub fn agents_status(app: tauri::AppHandle) -> Vec<Agent> {
    app.state::<State>()
        .0
        .lock()
        .map(|w| w.agents.clone())
        .unwrap_or_default()
}

#[tauri::command]
pub fn agent_bindings(state: tauri::State<crate::Database>) -> Result<Vec<Binding>, String> {
    let db = state.0.lock().map_err(|e| e.to_string())?;
    bindings(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn agent_bind(
    app: tauri::AppHandle,
    state: tauri::State<crate::Database>,
    folder: String,
    task_id: i64,
) -> Result<Vec<Binding>, String> {
    let list = bind(
        &*state.0.lock().map_err(|e| e.to_string())?,
        &folder,
        task_id,
    )?;
    rematch(&app);
    Ok(list)
}

#[tauri::command]
pub fn agent_unbind(
    app: tauri::AppHandle,
    state: tauri::State<crate::Database>,
    folder: String,
) -> Result<Vec<Binding>, String> {
    let list = unbind(&*state.0.lock().map_err(|e| e.to_string())?, &folder)?;
    rematch(&app);
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 10_000_000;

    fn session(status: &str, since: i64) -> String {
        format!(
            r#"{{"pid":7,"sessionId":"abc","cwd":"C:\\Projekty\\app","name":"","status":"{status}","updatedAt":{since},"statusUpdatedAt":{since}}}"#
        )
    }

    #[test]
    fn claude_busy_session_with_live_process_is_working() {
        let agent = claude_agent(&session("busy", NOW - 60_000), NOW, |_| true, |_| None).unwrap();
        assert_eq!(agent.kind, "claude");
        assert_eq!(agent.name, "app");
        assert_eq!(agent.since, NOW - 60_000);
        assert!(claude_agent(&session("idle", NOW), NOW, |_| true, |_| None).is_none());
        assert!(claude_agent(&session("busy", NOW), NOW, |_| false, |_| None).is_none());
        assert!(claude_agent("nie json", NOW, |_| true, |_| None).is_none());
    }

    #[test]
    fn long_busy_claude_session_needs_fresh_transcript() {
        let old = session("busy", NOW - STALL - 1);
        assert!(claude_agent(&old, NOW, |_| true, |_| None).is_none());
        assert!(claude_agent(&old, NOW, |_| true, |_| Some(NOW - STALL - 1)).is_none());
        assert!(claude_agent(
            &old,
            NOW,
            |_| true,
            |id| (id == "abc").then_some(NOW - 1_000)
        )
        .is_some());
    }

    #[test]
    fn scans_claude_and_codex_folders() {
        let root = std::env::temp_dir().join(format!("todo-track-agents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let claude = root.join("claude");
        std::fs::create_dir_all(claude.join("sessions")).unwrap();
        let now = db::now();
        let me = std::process::id();
        std::fs::write(
            claude.join("sessions").join("1.json"),
            format!(r#"{{"pid":{me},"cwd":"/x/proj","name":"Moja sesja","status":"busy","statusUpdatedAt":{now}}}"#),
        )
        .unwrap();
        std::fs::write(
            claude.join("sessions").join("2.json"),
            format!(r#"{{"pid":{me},"cwd":"/x/proj","status":"idle","statusUpdatedAt":{now}}}"#),
        )
        .unwrap();
        let found = claude_agents(&claude, now);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Moja sesja");

        let codex = root.join("codex");
        let day = codex.join("sessions").join("2026").join("10").join("15");
        std::fs::create_dir_all(&day).unwrap();
        let file = day.join("rollout-1.jsonl");
        std::fs::write(
            &file,
            "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"C:\\\\Projekty\\\\api\"}}\n{}\n",
        )
        .unwrap();
        let fresh = codex_fresh(&codex, now);
        assert_eq!(fresh.len(), 1);
        assert!(codex_fresh(&codex, now + CODEX_FRESH + 60_000).is_empty());

        let mut watch = Watch::default();
        let agents = watch.codex_agents(fresh.clone(), now);
        assert_eq!(agents[0].name, "api");
        assert_eq!(agents[0].since, now);
        // Kolejne odczyty zachowują początek pracy.
        assert_eq!(watch.codex_agents(fresh, now + 5_000)[0].since, now);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn update_reports_changes_and_remembers_last_work() {
        let mut w = Watch::default();
        assert!(!w.update(Vec::new(), 1));
        assert_eq!(w.last_busy, None);
        let a = Agent {
            kind: "codex".into(),
            name: "api".into(),
            cwd: "api".into(),
            since: 1,
            ..Agent::default()
        };
        assert!(w.update(vec![a.clone()], 2));
        assert!(!w.update(vec![a], 3));
        assert_eq!(w.last_busy, Some(3));
        assert!(w.update(Vec::new(), 4));
        assert_eq!(w.last_busy, Some(3));
    }

    /// Katalog tymczasowy testu (usuwany na początku, żeby powtórka była czysta).
    fn temp(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("todo-track-git-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn git_branch_reads_head_of_plain_repo_from_subfolder() {
        let root = temp("plain");
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::create_dir_all(repo.join("src").join("deep")).unwrap();
        std::fs::write(
            repo.join(".git").join("HEAD"),
            "ref: refs/heads/feature/WMS-123-eksport\n",
        )
        .unwrap();
        let expected = Some(Head::Branch("feature/WMS-123-eksport".into()));
        assert_eq!(git_branch(&repo), expected);
        assert_eq!(git_branch(&repo.join("src").join("deep")), expected);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn git_branch_follows_gitdir_file_of_worktree_and_submodule() {
        let root = temp("worktree");
        let main = root.join("repo");
        let admin = main.join(".git").join("worktrees").join("wt");
        std::fs::create_dir_all(&admin).unwrap();
        std::fs::write(main.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(admin.join("HEAD"), "ref: refs/heads/bugfix/ABC-7\n").unwrap();
        let wt = root.join("wt");
        std::fs::create_dir_all(wt.join("src")).unwrap();
        // Git na Windows zapisuje ścieżkę bezwzględną z ukośnikami `/`.
        let gitdir = admin.to_string_lossy().replace('\\', "/");
        std::fs::write(wt.join(".git"), format!("gitdir: {gitdir}\n")).unwrap();
        assert_eq!(
            git_branch(&wt.join("src")),
            Some(Head::Branch("bugfix/ABC-7".into()))
        );

        // Submoduł: względny `gitdir:` liczony od folderu z plikiem `.git`.
        let modules = main.join(".git").join("modules").join("lib");
        std::fs::create_dir_all(&modules).unwrap();
        std::fs::write(modules.join("HEAD"), "ref: refs/heads/lib-dev\n").unwrap();
        let sub = main.join("lib");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/lib\n").unwrap();
        assert_eq!(git_branch(&sub), Some(Head::Branch("lib-dev".into())));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn git_branch_reports_detached_head_and_missing_repo() {
        let root = temp("detached");
        let repo = root.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        std::fs::write(
            repo.join(".git").join("HEAD"),
            "0123456789abcdef0123456789abcdef01234567\n",
        )
        .unwrap();
        assert_eq!(git_branch(&repo), Some(Head::Detached("0123456".into())));

        let plain = root.join("bez-repo");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(git_branch(&plain), None);
        // Ścieżka względna albo pusta nie jest szukana względem procesu aplikacji.
        assert_eq!(git_branch(Path::new("")), None);
        assert_eq!(git_branch(Path::new("repo")), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn branch_cache_keeps_result_for_thirty_seconds() {
        let reads = std::cell::Cell::new(0);
        let read = |_: &Path| {
            reads.set(reads.get() + 1);
            Some(Head::Branch(format!("b{}", reads.get())))
        };
        let b = |name: &str| Some(Head::Branch(name.into()));
        let mut cache = Branches::default();
        assert_eq!(cache.get("C:\\a", 0, read), b("b1"));
        assert_eq!(cache.get("C:\\a", BRANCH_TTL - 1, read), b("b1"));
        assert_eq!(cache.get("C:\\b", 1, read), b("b2"));
        assert_eq!(cache.get("C:\\a", BRANCH_TTL, read), b("b3"));
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn extracts_jira_key_from_branch_or_session_name() {
        let key = |text: &str| jira_key(text);
        assert_eq!(key("feature/WMS-123-eksport").as_deref(), Some("WMS-123"));
        assert_eq!(key("WMS-7").as_deref(), Some("WMS-7"));
        assert_eq!(key("Poprawka AB2-45 w raporcie").as_deref(), Some("AB2-45"));
        assert_eq!(key("fix/ABC-1x").as_deref(), Some("ABC-1"));
        assert_eq!(key("Zażółć WMS-9").as_deref(), Some("WMS-9"));
        for none in ["feature/wms-123", "W-1", "xWMS-12", "WMS-", "main", ""] {
            assert_eq!(key(none), None, "{none}");
        }

        let mut agent = Agent {
            branch: Some("feature/WMS-123-eksport".into()),
            session: Some("Przegląd DEV-5".into()),
            ..Agent::default()
        };
        assert_eq!(agent_key(&agent).as_deref(), Some("WMS-123"));
        agent.branch = Some("main".into());
        assert_eq!(agent_key(&agent).as_deref(), Some("DEV-5"));
        // Skrót commita przy odłączonym HEAD nie jest gałęzią.
        agent.branch = Some("ABCD-12".into());
        agent.detached = true;
        assert_eq!(agent_key(&agent).as_deref(), Some("DEV-5"));
        agent.session = None;
        assert_eq!(agent_key(&agent), None);
    }

    #[test]
    fn recognize_fills_branch_and_key() {
        let mut w = Branches::default();
        let mut agents = vec![
            Agent {
                cwd: "C:\\repo".into(),
                session: Some("Sesja PRJ-2".into()),
                ..Agent::default()
            },
            Agent {
                cwd: "C:\\inne".into(),
                session: Some("Sesja PRJ-2".into()),
                ..Agent::default()
            },
        ];
        w.recognize(&mut agents, 0, |p| {
            (p == Path::new("C:\\repo")).then(|| Head::Branch("feature/WMS-1-a".into()))
        });
        assert_eq!(agents[0].branch.as_deref(), Some("feature/WMS-1-a"));
        assert_eq!(agents[0].key.as_deref(), Some("WMS-1"));
        assert_eq!(agents[1].branch, None);
        assert_eq!(agents[1].key.as_deref(), Some("PRJ-2"));
    }

    fn open() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db::init(&db).unwrap();
        for title in ["Eksport", "Przypisane", "W koszu"] {
            db.execute("INSERT INTO tasks(title,created_at) VALUES(?1,0)", [title])
                .unwrap();
        }
        db.execute_batch(
            "INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-123');
             INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(3,'https://x.atlassian.net','WMS-7');
             INSERT INTO task_lifecycle(task_id,archived,deleted_at) VALUES(3,0,5);
             INSERT INTO jira_issues(site,issue_key,status) VALUES('https://x.atlassian.net','WMS-200','Do zrobienia');
             INSERT INTO jira_issues(site,issue_key,status) VALUES('https://x.atlassian.net','WMS-7','W toku');",
        )
        .unwrap();
        db
    }

    #[test]
    fn resolve_prefers_folder_binding_then_local_task_then_jira_issue() {
        let db = open();
        let found = |cwd: &str, key: Option<&str>| resolve(&db, cwd, key).unwrap();

        // Klucz powiązany z lokalnym zadaniem.
        let m = found("C:\\Projekty\\wms", Some("WMS-123")).unwrap();
        assert_eq!(
            (m.task_id, m.title.as_str(), m.source.as_str()),
            (Some(1), "Eksport", "task")
        );
        // Klucz znany tylko z listy zgłoszeń: zadanie powstanie przy akceptacji.
        let m = found("C:\\Projekty\\wms", Some("WMS-200")).unwrap();
        assert_eq!(
            (m.task_id, m.key.as_deref(), m.source.as_str()),
            (None, Some("WMS-200"), "jira")
        );
        // Zadanie w koszu nie jest dopasowane; zostaje zgłoszenie z Jiry.
        assert_eq!(found("C:\\x", Some("WMS-7")).unwrap().source, "jira");
        assert_eq!(found("C:\\x", Some("NIE-1")), None);
        assert_eq!(found("C:\\x", None), None);

        // Przypisanie folderu wygrywa z kluczem i obejmuje podfoldery.
        bind(&db, "c:/projekty/WMS/", 2).unwrap();
        let m = found("C:\\Projekty\\wms\\src", Some("WMS-123")).unwrap();
        assert_eq!(
            (m.task_id, m.title.as_str(), m.source.as_str()),
            (Some(2), "Przypisane", "folder")
        );
        assert_eq!(found("C:\\Projekty\\wms-api", None), None);
        // Bliższe przypisanie wygrywa z dalszym.
        bind(&db, "C:\\Projekty\\wms\\src", 1).unwrap();
        assert_eq!(
            found("C:\\Projekty\\wms\\src\\a", None).unwrap().task_id,
            Some(1)
        );
        assert_eq!(
            found("C:\\Projekty\\wms\\doc", None).unwrap().task_id,
            Some(2)
        );
        // Przypisanie do zadania w koszu jest pomijane.
        db.execute(
            "UPDATE task_lifecycle SET deleted_at=NULL WHERE task_id=3",
            [],
        )
        .unwrap();
        bind(&db, "C:\\kosz", 3).unwrap();
        db.execute("UPDATE task_lifecycle SET deleted_at=9 WHERE task_id=3", [])
            .unwrap();
        assert_eq!(found("C:\\kosz", Some("WMS-123")).unwrap().source, "task");
    }

    #[test]
    fn bindings_replace_folder_and_reject_missing_or_trashed_task() {
        let db = open();
        let missing = Err("Nie znaleziono zadania.".to_string());
        assert_eq!(bind(&db, "C:\\a", 99), missing);
        assert_eq!(bind(&db, "C:\\a", 3), missing);
        assert_eq!(bind(&db, "  ", 1), Err("Niepoprawny folder.".into()));
        bind(&db, "C:\\a\\", 1).unwrap();
        let list = bind(&db, "c:\\A", 2).unwrap();
        assert_eq!(
            list,
            vec![Binding {
                folder: "c:\\A".into(),
                task_id: 2,
                title: "Przypisane".into()
            }]
        );
        assert!(unbind(&db, "C:\\a").unwrap().is_empty());
        // Trwałe usunięcie zadania usuwa przypisanie (ON DELETE CASCADE).
        bind(&db, "C:\\b", 2).unwrap();
        db.execute("DELETE FROM tasks WHERE id=2", []).unwrap();
        assert!(bindings(&db).unwrap().is_empty());
    }

    #[test]
    fn bindings_ignore_separator_style() {
        let db = open();
        let sep = std::path::MAIN_SEPARATOR;
        // Zapis w jednej postaci: separatory systemu, bez końcowego.
        let list = bind(&db, "C:/Projekty/wms/", 1).unwrap();
        assert_eq!(list[0].folder, format!("C:{sep}Projekty{sep}wms"));
        // Ten sam folder innymi separatorami zastępuje przypisanie, nie dubluje go.
        let list = bind(&db, "c:\\projekty\\WMS", 2).unwrap();
        assert_eq!(list.iter().map(|b| b.task_id).collect::<Vec<_>>(), vec![2]);
        assert!(unbind(&db, "C:\\Projekty\\wms").unwrap().is_empty());
        bind(&db, "C:/Projekty/wms", 1).unwrap();
        assert!(unbind(&db, "c:\\projekty\\wms\\").unwrap().is_empty());
        // Duplikaty zapisane przed ujednoliceniem znikają razem.
        db.execute_batch(
            "INSERT INTO agent_bindings(folder,task_id) VALUES('C:\\x',1);
             INSERT INTO agent_bindings(folder,task_id) VALUES('C:/x',2);",
        )
        .unwrap();
        assert_eq!(bindings(&db).unwrap().len(), 2);
        assert!(unbind(&db, "c:/X/").unwrap().is_empty());
    }
}
