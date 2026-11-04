//! Wykrywanie pracujących agentów AI (Claude Code i Codex) na tym komputerze.
//!
//! Stan agentów pochodzi z hooków (`hooks.rs`, PLAN_BACKLOG.md B8): agent uruchamia
//! `tuktuk-track-hook.exe` przy każdym zdarzeniu, a maszyna stanów sesji (`hooks::Sessions`)
//! wskazuje, które sesje pracują. Pliki sesji agentów nie są już skanowane.
//!
//! Wątek odświeża listę co 5 s (i od razu po zmianie fazy sesji), wysyła
//! `agents-changed` przy zmianie i zapamiętuje ostatnią chwilę pracy agenta,
//! z której korzysta `idle`.
//!
//! Rozpoznanie zadania (PLAN_1.3.md, etap 4): gałąź git folderu roboczego
//! (`git_branch`, bez uruchamiania procesu `git`, cache 30 s na katalog), klucz Jiry
//! z gałęzi albo z nazwy sesji Claude Code i dopasowanie do zadania (`resolve`):
//! przypisanie folderu (`agent_bindings`) → klucz powiązany z lokalnym zadaniem →
//! klucz znany z `jira_issues` (zadanie powstanie dopiero przy akceptacji).
//! Dopasowanie tylko opisuje pracę agenta; timer nigdy nie przełącza się sam.
use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;
use tauri::{Emitter, Manager};

/// Co ile wątek sprawdza agentów.
const CHECK: Duration = Duration::from_secs(5);
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
    /// Identyfikator sesji z hooka (klucz pamięci asystenta przypisań, A4).
    #[serde(skip)]
    pub session_id: String,
    /// Pierwszy prompt sesji (do 500 znaków), tylko w pamięci i tylko przy
    /// `ai_assign_suggest`; nie trafia do interfejsu, tylko do asystenta (A4).
    #[serde(skip)]
    pub first_prompt: Option<String>,
    /// Zadanie zaproponowane przez asystenta (A4, źródło `assistant`): tylko
    /// podpowiedź dla banera. Nigdy nie zastępuje `task`, więc czas agentów i
    /// dopasowanie nie korzystają z niej, dopóki użytkownik nie przypisze folderu.
    #[serde(skip)]
    pub assist: Option<Match>,
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
    /// `folder` (przypisanie folderu), `task` (klucz lokalnego zadania), `jira`,
    /// `redmine` (`#1234` znane z `redmine_issues`, bez zadania).
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
/// Klucz w dopasowaniu służy tylko do wyświetlenia: z Jiry albo `#numer`
/// z Redmine (dopasowanie po kluczu w `resolve` zostaje dla Jiry).
pub(crate) fn bound_task(db: &Connection, cwd: &str) -> rusqlite::Result<Option<Match>> {
    let cwd = norm_folder(cwd);
    if cwd.is_empty() {
        return Ok(None);
    }
    let rows = db
        .prepare(
            "SELECT b.folder,t.id,t.title,COALESCE(j.issue_key,'#' || r.issue_id),t.done FROM agent_bindings b JOIN tasks t ON t.id=b.task_id
             LEFT JOIN task_lifecycle l ON l.task_id=t.id LEFT JOIN jira_tasks j ON j.task_id=t.id
             LEFT JOIN redmine_tasks r ON r.task_id=t.id
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
    // `#1234` (tylko z rozszerzenia przeglądarki, R3) to zgłoszenie Redmine;
    // klucze z gałęzi i tytułów okien są zawsze kluczami Jiry.
    if let Some(number) = key.strip_prefix('#') {
        return resolve_redmine(db, key, number);
    }
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

/// Dopasowanie `#1234`: lokalne zadanie z `redmine_tasks` (poza koszem), a bez
/// niego zgłoszenie znane z `redmine_issues` (start przez `redmine::start_by_id`).
/// Tylko zgłoszenia skonfigurowanego Redmine: rozszerzenie rozpoznaje `#1234`
/// wyłącznie pod jego adresem, a numer z innego (np. starego) Redmine to inne
/// zgłoszenie.
fn resolve_redmine(db: &Connection, key: &str, number: &str) -> rusqlite::Result<Option<Match>> {
    let Some(id) = number.parse::<u64>().ok().filter(|id| {
        number.bytes().all(|b| b.is_ascii_digit()) && crate::redmine::valid_issue_id(*id)
    }) else {
        return Ok(None);
    };
    let site = crate::redmine::current_site(db);
    if site.is_empty() {
        return Ok(None);
    }
    let local = db
        .prepare(
            "SELECT t.id,t.title,t.done,r.site FROM redmine_tasks r JOIN tasks t ON t.id=r.task_id
             LEFT JOIN task_lifecycle l ON l.task_id=t.id
             WHERE r.issue_id=?1 AND l.deleted_at IS NULL ORDER BY t.done, t.id DESC",
        )?
        .query_map([id as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .find(|(.., s)| crate::redmine::same_site(s, &site));
    if let Some((task_id, title, done, _)) = local {
        return Ok(Some(Match {
            task_id: Some(task_id),
            title,
            key: Some(key.into()),
            source: "task".into(),
            done,
        }));
    }
    let known = db
        .prepare("SELECT site FROM redmine_issues WHERE issue_id=?1")?
        .query_map([id as i64], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|s| crate::redmine::same_site(s, &site));
    Ok(known.then(|| Match {
        task_id: None,
        title: String::new(),
        key: Some(key.into()),
        source: "redmine".into(),
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

/// Stan agentów i sygnał budzenia pętli.
#[derive(Default)]
pub struct State(Mutex<Watch>, Wake);
#[derive(Default)]
struct Watch {
    agents: Vec<Agent>,
    /// Ostatnia chwila, w której jakiś agent pracował.
    last_busy: Option<i64>,
    /// Sesje agentów z hooków (maszyna stanów).
    sessions: crate::hooks::Sessions,
}

/// Budzenie pętli po zmianie fazy sesji, żeby lista nie czekała na kolejne 5 s.
#[derive(Default)]
struct Wake {
    flag: Mutex<bool>,
    cv: Condvar,
}
impl Wake {
    fn notify(&self) {
        if let Ok(mut flag) = self.flag.lock() {
            *flag = true;
            self.cv.notify_all();
        }
    }
    /// Czeka najwyżej `timeout` albo do `notify`.
    fn wait(&self, timeout: Duration) {
        let Ok(mut flag) = self.flag.lock() else {
            std::thread::sleep(timeout);
            return;
        };
        if !*flag {
            match self.cv.wait_timeout(flag, timeout) {
                Ok((guard, _)) => flag = guard,
                Err(_) => return,
            }
        }
        *flag = false;
    }
}

impl State {
    /// Zdarzenie hooka do maszyny stanów; budzi pętlę, gdy zmienia listę pracujących.
    pub fn apply_hook(&self, ev: &crate::hooks::HookEvent, now: i64) {
        // Pętla budzi się też przy zmianie fazy widocznej tylko na tablicy sesji
        // (czekanie, koniec), nie tylko na liście pracujących.
        let changed = self
            .0
            .lock()
            .map(|mut w| {
                let before = w.sessions.board_state(&ev.agent, &ev.session_id);
                let working = w.sessions.apply(ev, now);
                working || before != w.sessions.board_state(&ev.agent, &ev.session_id)
            })
            .unwrap_or(false);
        if changed {
            self.1.notify();
        }
    }
    /// „Przejrzane” dla sesji na tablicy (`agent_board`).
    pub fn mark_seen(&self, agent: &str, id: &str) -> bool {
        self.0
            .lock()
            .map(|mut w| w.sessions.mark_seen(agent, id))
            .unwrap_or(false)
    }
    pub fn mark_all_seen(&self) -> bool {
        self.0
            .lock()
            .map(|mut w| w.sessions.mark_all_seen())
            .unwrap_or(false)
    }
    /// Zapamiętywanie pierwszego promptu sesji (A4) zgodne z `ai_assign_suggest`;
    /// po wyłączeniu zapamiętane prompty znikają z pamięci.
    pub fn set_keep_prompts(&self, keep: bool) {
        if let Ok(mut w) = self.0.lock() {
            if w.sessions.keep_prompts != keep {
                w.sessions.set_keep_prompts(keep);
            }
        }
    }
    pub fn clear_sessions(&self) {
        if let Ok(mut w) = self.0.lock() {
            w.sessions.clear();
        }
        self.1.notify();
    }
    pub fn recent_hooks(&self) -> Vec<crate::hooks::Recent> {
        self.0
            .lock()
            .map(|w| w.sessions.recent())
            .unwrap_or_default()
    }
}

/// Usuwa agentów uruchomionych w katalogu asystenta AI (`ai::dir`): hook
/// odrzuca je znacznikiem `TUKTUK_TRACK_AI`, a to druga zapora (A1).
pub(crate) fn without_ai_dir(agents: &mut Vec<Agent>, ai_dir: &Path) {
    agents.retain(|a| !crate::ai::is_ai_dir(&a.cwd, ai_dir));
}

/// Folder roboczy: ostatni człon ścieżki.
pub(crate) fn folder(cwd: &str) -> String {
    cwd.trim_end_matches(['\\', '/'])
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or_default()
        .to_string()
}

impl Watch {
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

/// Rejestruje stan agentów; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
    app.manage(crate::orca::State::default());
}

/// Pętla co 5 s. Kończy się, gdy okno main zniknie, więc wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Cache gałęzi należy tylko do tego wątku.
        let mut branches = Branches::default();
        // Wywołania asystenta AI (A1) działają w tym katalogu: nie są agentami.
        let ai_dir = crate::ai::dir(&app);
        loop {
            if app.get_webview_window("main").is_none() {
                break;
            }
            let now = db::now();
            let found = app.state::<State>().0.lock().ok().map(|mut w| {
                w.sessions.expire(now);
                (w.sessions.working(), w.sessions.board())
            });
            if let Some((mut found, board)) = found {
                // Tablica sesji: czekające, pracujące i zakończone nieprzejrzane.
                let mut board = crate::agent_board::entries(board);
                if let Some(ai_dir) = &ai_dir {
                    board.retain(|e| !crate::ai::is_ai_dir(&e.agent.cwd, ai_dir));
                }
                if !board.is_empty() {
                    let mut list: Vec<Agent> = board.iter().map(|e| e.agent.clone()).collect();
                    branches.recognize(&mut list, now, git_branch);
                    match_plain(&app, &mut list);
                    for (entry, agent) in board.iter_mut().zip(list) {
                        entry.agent = agent;
                    }
                }
                crate::agent_board::publish(&app, board);
                if let Some(ai_dir) = &ai_dir {
                    without_ai_dir(&mut found, ai_dir);
                }
                // `.git` (także z dysku sieciowego) czytany poza blokadą stanu, żeby
                // `agents_status` w wątku głównym na niego nie czekał.
                branches.recognize(&mut found, now, git_branch);
                // Mapowanie folderów worktree Orki odświeża się w tle tylko wtedy,
                // gdy ktoś nie ma jeszcze klucza (B9); Orki nie uruchamia.
                if found.iter().any(|a| a.key.is_none()) {
                    crate::orca::refresh_if_due(&app, now);
                }
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
                // Hooki agentów: wpisy w plikach Claude Code i Codexa (B8).
                crate::hooks::retry(&app, now);
            }
            app.state::<State>().1.wait(CHECK);
        }
    });
}

/// Dopasowuje zadania agentów (`match_plain`), a sesjom bez dopasowania dokłada
/// podpowiedź asystenta przypisań (A4).
fn match_tasks(app: &tauri::AppHandle, agents: &mut [Agent]) {
    match_plain(app, agents);
    // Sesje bez dopasowania heurystyką: podpowiedź asystenta przypisań (A4).
    crate::assign::apply(app, agents);
}

/// Dopasowuje zadania agentów (`resolve`) bez asystenta; tytuł zgłoszenia
/// znanego tylko z Jiry pochodzi z listy wczytanej w tej sesji
/// (`jira::cached_summary`).
fn match_plain(app: &tauri::AppHandle, agents: &mut [Agent]) {
    {
        let state = app.state::<crate::Database>();
        let Ok(db) = state.0.lock() else {
            return;
        };
        for agent in agents.iter_mut() {
            // Dodatkowe źródło klucza: zgłoszenie podpięte do worktree w Orce (B9).
            if agent.key.is_none() {
                agent.key = crate::orca::lookup(app, &agent.cwd);
            }
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
pub(crate) fn rematch(app: &tauri::AppHandle) {
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
                // Podpowiedź asystenta nie jest widoczna w liście agentów (nie
                // wymaga `agents-changed`), ale propozycje liczą się od niej.
                current.assist = next.assist.clone();
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
            std::env::temp_dir().join(format!("tuktuk-track-git-{name}-{}", std::process::id()));
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
    fn orca_worktree_folder_gives_the_key_when_branch_and_session_do_not() {
        // B9: folder worktree z `orca worktree list` jest dodatkowym źródłem klucza.
        let db = open();
        let map = crate::orca::folder_keys(&[crate::orca::Worktree {
            path: r"C:\Orca\wms\praca".into(),
            references: vec!["https://x.atlassian.net/browse/WMS-123".into()],
            ..Default::default()
        }]);
        let cwd = r"C:\Orca\wms\praca\src";
        let key = crate::orca::key_for_folder(&map, cwd);
        let m = resolve(&db, cwd, key.as_deref()).unwrap().unwrap();
        assert_eq!((m.task_id, m.source.as_str()), (Some(1), "task"));
        // Przypisanie folderu wygrywa z kluczem z Orki.
        bind(&db, r"C:\Orca\wms\praca", 2).unwrap();
        let m = resolve(&db, cwd, key.as_deref()).unwrap().unwrap();
        assert_eq!((m.task_id, m.source.as_str()), (Some(2), "folder"));
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

    #[test]
    fn folder_binding_shows_redmine_number_and_keeps_key_matching_for_jira() {
        let db = open();
        db.execute_batch(
            r#"INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(2,'https://redmine.example.pl',1234);
               INSERT INTO settings(key,value) VALUES('redmine','{"site":"https://redmine.example.pl"}');"#,
        )
        .unwrap();
        bind(&db, "C:\\Projekty\\redmine", 2).unwrap();
        let m = resolve(&db, "C:\\Projekty\\redmine\\src", None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (m.task_id, m.key.as_deref(), m.source.as_str()),
            (Some(2), Some("#1234"), "folder")
        );
        // Numer Redmine nie jest kluczem do dopasowania po gałęzi: z nazwy
        // gałęzi powstają tylko klucze Jiry.
        assert_eq!(jira_key("feature/1234-import"), None);
        assert_eq!(jira_key("#1234"), None);
        // `#1234` przychodzi tylko z rozszerzenia przeglądarki (R3) i wtedy
        // wskazuje zadanie powiązane z tym zgłoszeniem Redmine.
        let m = resolve(&db, "C:\\inne", Some("#1234")).unwrap().unwrap();
        assert_eq!(
            (m.task_id, m.key.as_deref(), m.source.as_str()),
            (Some(2), Some("#1234"), "task")
        );
        assert_eq!(resolve(&db, "C:\\inne", Some("#4321")).unwrap(), None);
        let set_site = |site: &str| {
            db.execute(
                "UPDATE settings SET value=?1 WHERE key='redmine'",
                [serde_json::json!({ "site": site }).to_string()],
            )
            .unwrap()
        };
        // Inny Redmine (np. nowy adres po zmianie w Ustawieniach): ten sam numer
        // to inne zgłoszenie, bez dopasowania.
        set_site("https://firma.example.pl/redmine");
        assert_eq!(resolve(&db, "C:\\inne", Some("#1234")).unwrap(), None);
        // Ten sam Redmine przez http: to samo zgłoszenie.
        set_site("http://redmine.example.pl");
        let m = resolve(&db, "C:\\inne", Some("#1234")).unwrap().unwrap();
        assert_eq!(m.task_id, Some(2));
        // Bez konfiguracji Redmine `#1234` nie wskazuje niczego.
        db.execute("DELETE FROM settings WHERE key='redmine'", [])
            .unwrap();
        assert_eq!(resolve(&db, "C:\\inne", Some("#1234")).unwrap(), None);
    }

    /// A1: agent uruchomiony w katalogu asystenta AI (wywołania `ai::ask`) nie
    /// trafia na listę agentów, nawet gdy hook go przepuścił.
    #[test]
    fn agents_in_ai_directory_are_ignored() {
        let ai = Path::new(r"C:\Users\ala\AppData\Roaming\pl.tuktuktrack.desktop\ai");
        let agent = |cwd: &str| Agent {
            kind: "claude".into(),
            cwd: cwd.into(),
            ..Agent::default()
        };
        let notes = r"C:\Users\ala\AppData\Roaming\pl.tuktuktrack.desktop\ai-notes";
        let mut list = vec![
            agent(r"C:\Users\ala\AppData\Roaming\pl.tuktuktrack.desktop\ai"),
            agent("c:/users/ala/appdata/roaming/pl.tuktuktrack.desktop/AI/"),
            agent(r"C:\repo"),
            agent(notes),
        ];
        without_ai_dir(&mut list, ai);
        let left: Vec<&str> = list.iter().map(|a| a.cwd.as_str()).collect();
        assert_eq!(left, [r"C:\repo", notes]);
    }
}
