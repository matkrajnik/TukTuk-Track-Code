//! Tablica sesji agentów: które sesje Claude Code i Codexa czekają na użytkownika,
//! które pracują, a które skończyły się i nikt ich jeszcze nie przejrzał.
//!
//! Źródłem jest maszyna stanów hooków (`hooks::Sessions::board`). Pętla agentów
//! (`agents::start`) co 5 s i po zmianie fazy sesji rozpoznaje gałąź i zadanie
//! każdej sesji (jak dla pracujących agentów, bez asystenta przypisań) i oddaje
//! listę tutaj ([`publish`]). Ten moduł trzyma ostatnią listę, emituje
//! [`EVENT`] przy zmianie, wysyła powiadomienia o czekaniu i o końcu (osobne
//! wyłączniki w ustawieniach) i podaje liczniki dla traya.
//!
//! Wszystko jest w pamięci: po restarcie aplikacji tablica jest pusta, tak jak
//! lista pracujących agentów.
use crate::agents::{self, Agent};
use crate::hooks::{BoardSession, Phase};
use crate::notify::{self, Message, View};
use crate::settings::AppSettings;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Zdarzenie z nową tablicą sesji dla okna.
pub const EVENT: &str = "agent-board-changed";
/// Ile sesji wymienia zbiorcze powiadomienie.
const NAMES: usize = 3;

/// Sesja na tablicy: agent (folder, gałąź, klucz, zadanie) i jego stan.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    #[serde(flatten)]
    pub agent: Agent,
    /// Identyfikator sesji (do „Przejrzane”).
    pub id: String,
    /// `working`, `waiting` albo `done`.
    pub phase: &'static str,
    /// Wejście w bieżącą fazę (ms).
    pub phase_at: i64,
    pub seen: bool,
    /// Na co czeka sesja albo `brak zdarzeń` przy końcu z ciszy.
    pub reason: Option<String>,
    /// Pracujący subagenci.
    pub subagents: usize,
    /// Koniec z braku zdarzeń (ubity proces), nie po `Stop`.
    pub timed_out: bool,
    /// Folder sesji to worktree Orki (ostatni odczyt `worktree list`): „Przejdź” do Orki.
    pub orca: bool,
    /// Źródło sesji (`orca`, `claude-desktop`, `codex-desktop`, `vscode`, `terminal`;
    /// puste, gdy hook go nie podał).
    pub host: String,
    /// Identyfikator w źródle; tylko dla „Przejdź”, nie trafia do okna ani MCP.
    #[serde(skip)]
    pub host_ref: Option<String>,
}

impl Entry {
    fn waiting(&self) -> bool {
        self.phase == Phase::Waiting.as_str()
    }
    fn done(&self) -> bool {
        self.phase == Phase::Done.as_str()
    }
}

/// Sesje z maszyny stanów jako wpisy tablicy (przed rozpoznaniem gałęzi i zadania).
pub fn entries(board: Vec<BoardSession>) -> Vec<Entry> {
    board
        .into_iter()
        .map(|b| {
            let name = agents::folder(&b.cwd);
            let timed_out = b.timed_out();
            Entry {
                agent: Agent {
                    kind: b.agent,
                    name: if name.is_empty() {
                        "sesja".into()
                    } else {
                        name
                    },
                    cwd: b.cwd,
                    since: b.phase_at,
                    session_id: b.session_id.clone(),
                    ..Agent::default()
                },
                id: b.session_id,
                phase: b.phase.as_str(),
                phase_at: b.phase_at,
                seen: b.seen,
                reason: b.reason,
                subagents: b.subagents,
                timed_out,
                orca: false,
                host: b.host,
                host_ref: b.host_ref,
            }
        })
        .collect()
}

/// Liczniki do traya i nawigacji: nieprzejrzane czekające i zakończone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub waiting: usize,
    pub done: usize,
}

/// Koniec z braku zdarzeń (ubity proces) jest na tablicy, ale nie w licznikach:
/// nikt na niego nie czeka i nie ma o nim powiadomienia.
pub fn counts(list: &[Entry]) -> Counts {
    list.iter()
        .filter(|e| !e.seen && !e.timed_out)
        .fold(Counts::default(), |c, e| Counts {
            waiting: c.waiting + usize::from(e.waiting()),
            done: c.done + usize::from(e.done()),
        })
}

/// Wiersz traya: „Agenci: czeka 2 · skończone 1”; `None`, gdy nic nie czeka.
pub fn tray_line(c: Counts) -> Option<String> {
    let parts: Vec<String> = [("czeka", c.waiting), ("skończone", c.done)]
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(label, n)| format!("{label} {n}"))
        .collect();
    (!parts.is_empty()).then(|| format!("Agenci: {}", parts.join(" · ")))
}

/// Klucz powiadomienia: sesja i chwila wejścia w fazę (jedno na czekanie lub koniec).
fn notice_key(e: &Entry) -> String {
    format!("{}|{}|{}|{}", e.agent.kind, e.id, e.phase, e.phase_at)
}

/// Pamięć tablicy: ostatnia lista i sesje, o których już powiadomiliśmy.
#[derive(Default)]
pub struct Memory {
    entries: Vec<Entry>,
    notified: HashSet<String>,
    /// Przejrzane końce zdjęte z listy od ostatniego `update`.
    dismissed: Vec<String>,
}

impl Memory {
    /// Zapisuje listę; zwraca `true`, gdy się zmieniła, i nowe wpisy do
    /// powiadomienia (nieprzejrzane czekanie albo koniec po `Stop`, każdy raz).
    pub fn update(&mut self, mut list: Vec<Entry>) -> (bool, Vec<Entry>) {
        // Lista z pętli powstaje przed rozpoznaniem zadań; „Przejrzane” kliknięte
        // w tym czasie (ta sama faza) nie może się cofnąć.
        let seen: HashSet<String> = self
            .entries
            .iter()
            .filter(|e| e.seen)
            .map(notice_key)
            .chain(self.dismissed.iter().cloned())
            .collect();
        for e in list.iter_mut() {
            if e.phase != Phase::Working.as_str() && seen.contains(&notice_key(e)) {
                e.seen = true;
            }
        }
        list.retain(|e| !(e.done() && e.seen));
        let fresh: Vec<Entry> = list
            .iter()
            .filter(|e| !e.seen && (e.waiting() || (e.done() && !e.timed_out)))
            .filter(|e| !self.notified.contains(&notice_key(e)))
            .cloned()
            .collect();
        // Pamiętamy tylko klucze sesji, które nadal są na tablicy.
        let current: HashSet<String> = list.iter().map(notice_key).collect();
        self.notified.retain(|k| current.contains(k));
        self.notified.extend(fresh.iter().map(notice_key));
        self.dismissed.clear();
        let changed = list != self.entries;
        self.entries = list;
        (changed, fresh)
    }

    /// „Przejrzane” w zapamiętanej liście (bez czekania na pętlę agentów);
    /// `id: None` = wszystkie czekające i zakończone.
    pub fn mark_seen(&mut self, kind: &str, id: Option<&str>) {
        for e in self.entries.iter_mut() {
            if !e.waiting() && !e.done() {
                continue;
            }
            if id.is_none_or(|id| e.id == id && e.agent.kind == kind) {
                e.seen = true;
            }
        }
        // Przejrzane końce znikają z listy, ale ich klucz musi przetrwać do
        // następnego `update`, żeby starsza lista z pętli ich nie przywróciła.
        let dismissed: Vec<String> = self
            .entries
            .iter()
            .filter(|e| e.done() && e.seen)
            .map(notice_key)
            .collect();
        self.dismissed.extend(dismissed);
        self.entries.retain(|e| !(e.done() && e.seen));
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
}

/// Powiadomienie o nowych wpisach według ustawień; `None`, gdy nie ma o czym.
pub fn message(fresh: &[Entry], s: &AppSettings) -> Option<Message> {
    let shown: Vec<&Entry> = fresh
        .iter()
        .filter(|e| (e.waiting() && s.agent_notify_waiting) || (e.done() && s.agent_notify_done))
        .collect();
    let label = |e: &Entry| match &e.agent.task {
        Some(task) if !task.title.is_empty() => {
            format!("{} · {}", e.agent.name, notify::short(&task.title))
        }
        _ => e.agent.name.clone(),
    };
    let message = |title: String, body: String| Message {
        key: String::new(),
        title,
        body,
        actions: Vec::new(),
        open: View {
            view: Some("agents"),
            ..View::default()
        },
    };
    match shown.as_slice() {
        [] => None,
        [one] if one.waiting() => Some(message(
            format!("{} czeka na Ciebie", kind_label(&one.agent.kind)),
            match &one.reason {
                Some(reason) => format!("{} · {reason}", label(one)),
                None => label(one),
            },
        )),
        [one] => Some(message(
            format!("{} skończył pracę", kind_label(&one.agent.kind)),
            label(one),
        )),
        many => {
            let waiting = many.iter().filter(|e| e.waiting()).count();
            let title = match (waiting, many.len() - waiting) {
                (w, 0) => format!("Sesje agentów czekają na Ciebie: {w}"),
                (0, d) => format!("Sesje agentów skończyły pracę: {d}"),
                (w, d) => format!("Sesje agentów: czeka {w}, skończone {d}"),
            };
            let mut names: Vec<String> = many.iter().take(NAMES).map(|e| label(e)).collect();
            if many.len() > NAMES {
                names.push(format!("i {} więcej", many.len() - NAMES));
            }
            Some(message(title, names.join(", ")))
        }
    }
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "codex" => "Codex",
        _ => "Claude",
    }
}

#[derive(Default)]
pub struct State(Mutex<Memory>);

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Nowa tablica z pętli agentów: zapis, zdarzenie przy zmianie, powiadomienie.
/// Gdy główne okno ma fokus, użytkownik widzi licznik w nawigacji i toast jest
/// pomijany (sesja i tak zostaje oznaczona jako zgłoszona).
pub fn publish(app: &tauri::AppHandle, list: Vec<Entry>) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let Ok(mut memory) = state.0.lock() else {
        return;
    };
    let (changed, fresh) = memory.update(list);
    let snapshot = changed.then(|| memory.entries.clone());
    drop(memory);
    if let Some(list) = snapshot {
        let _ = app.emit(EVENT, list);
    }
    if fresh.is_empty() || main_focused(app) {
        return;
    }
    let settings = crate::settings::current(app);
    if let Some(msg) = message(&fresh, &settings) {
        notify::show(app, &msg);
    }
}

fn main_focused(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("main")
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Liczniki z ostatniej tablicy (tray).
pub fn current_counts(app: &tauri::AppHandle) -> Counts {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|m| counts(m.entries())))
        .unwrap_or_default()
}

fn snapshot(app: &tauri::AppHandle) -> Vec<Entry> {
    app.try_state::<State>()
        .and_then(|s| s.0.lock().ok().map(|m| m.entries.clone()))
        .unwrap_or_default()
}

/// „Przejrzane” w maszynie stanów i w zapamiętanej liście; `id: None` = wszystkie.
fn mark(app: &tauri::AppHandle, kind: &str, id: Option<&str>) -> Vec<Entry> {
    let changed = app
        .try_state::<agents::State>()
        .is_some_and(|agents| match id {
            Some(id) => agents.mark_seen(kind, id),
            None => agents.mark_all_seen(),
        });
    if changed {
        if let Some(state) = app.try_state::<State>() {
            if let Ok(mut memory) = state.0.lock() {
                memory.mark_seen(kind, id);
            }
        }
    }
    let list = snapshot(app);
    if changed {
        let _ = app.emit(EVENT, list.clone());
    }
    list
}

#[tauri::command]
pub fn agent_board(app: tauri::AppHandle) -> Vec<Entry> {
    snapshot(&app)
}

#[tauri::command]
pub fn agent_board_seen(app: tauri::AppHandle, kind: String, id: String) -> Vec<Entry> {
    mark(&app, &kind, Some(&id))
}

#[tauri::command]
pub fn agent_board_seen_all(app: tauri::AppHandle) -> Vec<Entry> {
    mark(&app, "", None)
}

// ---------------------------------------------------------------------------
// Narzędzie MCP `agent_sessions`

/// Tablica dla agenta (MCP): sama nazwa folderu (bez ścieżki), faza, od kiedy,
/// powód czekania, zadanie i gałąź. Wynik przechodzi potem przez
/// `ai_privacy::redact` jak każde narzędzie MCP.
pub fn mcp_json(
    list: &[Entry],
    now: i64,
    hm: impl Fn(i64) -> String,
    waited_today_ms: i64,
) -> serde_json::Value {
    use serde_json::{json, Map, Value};
    let sessions: Vec<Value> = list
        .iter()
        .map(|e| {
            let mut m = Map::new();
            m.insert("agent".into(), json!(e.agent.kind));
            m.insert("folder".into(), json!(e.agent.name));
            m.insert("phase".into(), json!(e.phase));
            if !e.host.is_empty() {
                // Sama nazwa źródła; identyfikator (`host_ref`) nie wychodzi.
                m.insert("host".into(), json!(e.host));
            }
            m.insert("since".into(), json!(hm(e.phase_at)));
            m.insert("minutes".into(), json!((now - e.phase_at).max(0) / 60_000));
            if let Some(reason) = &e.reason {
                m.insert("reason".into(), json!(reason));
            }
            if e.phase != Phase::Working.as_str() {
                m.insert("seen".into(), json!(e.seen));
            }
            if e.subagents > 0 {
                m.insert("subagents".into(), json!(e.subagents));
            }
            if let Some(branch) = e.agent.branch.as_ref().filter(|_| !e.agent.detached) {
                m.insert("branch".into(), json!(branch));
            }
            if let Some(task) = &e.agent.task {
                m.insert(
                    "task".into(),
                    json!({"taskId": task.task_id, "title": task.title, "key": task.key}),
                );
            }
            Value::Object(m)
        })
        .collect();
    let c = counts(list);
    json!({
        "sessions": sessions,
        "waitingUnseen": c.waiting,
        "doneUnseen": c.done,
        "waitedForUserTodayMinutes": waited_today_ms / 60_000,
    })
}

/// Ostatnia tablica (do narzędzia MCP).
pub fn current(app: &tauri::AppHandle) -> Vec<Entry> {
    snapshot(app)
}

// ---------------------------------------------------------------------------
// „Przejdź”: sesja w jej aplikacji (Orca, Claude/Codex Desktop), folder w VS Code
// albo w Eksploratorze

/// Dokąd prowadzi „Przejdź”.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    /// Aplikacja, w której działa sesja ([`app_link`]): terminal Orki, sesja
    /// Claude Desktop albo wątek Codex Desktop.
    App,
    /// `Code.exe <folder>`: VS Code otwiera folder albo przełącza na jego okno.
    Code,
    /// Folder w Eksploratorze.
    Folder,
}

/// Znaki, których w ścieżce folderu nie przyjmujemy (cudzysłów, zmienne,
/// znaki sterujące): folder pochodzi z hooka agenta.
const UNSAFE: [char; 7] = ['"', '%', '!', '^', '\n', '\r', '\0'];

/// Folder sesji do otwarcia: bezwzględna ścieżka lokalnego, istniejącego
/// katalogu z literą dysku. Ścieżki sieciowe (`\\serwer\…`) odpadają przed
/// sprawdzeniem katalogu, żeby nie łączyć się z cudzym serwerem.
pub fn check_folder(cwd: &str, is_dir: impl Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return Err("Sesja nie podała folderu roboczego.".into());
    }
    let drive = cwd.as_bytes();
    let local = drive.len() >= 3
        && drive[0].is_ascii_alphabetic()
        && drive[1] == b':'
        && matches!(drive[2], b'\\' | b'/');
    if cwd.contains(UNSAFE) || !local {
        return Err("Ścieżki folderu nie da się bezpiecznie otworzyć.".into());
    }
    let path = PathBuf::from(cwd);
    if !is_dir(&path) {
        return Err(format!("Folder {cwd} nie istnieje."));
    }
    Ok(path)
}

/// Miejsca `Code.exe` (bez pliku wsadowego `code.cmd`): instalacja użytkownika,
/// instalacja systemowa, potem katalog nad `bin` z PATH (tam leży `code.cmd`).
pub fn code_candidates(env: impl Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let exe = |root: PathBuf| root.join("Microsoft VS Code").join("Code.exe");
    let mut out = Vec::new();
    if let Some(local) = env("LOCALAPPDATA") {
        out.push(exe(PathBuf::from(local).join("Programs")));
    }
    if let Some(program_files) = env("ProgramFiles") {
        out.push(exe(PathBuf::from(program_files)));
    }
    if let Some(path) = env("PATH") {
        out.extend(
            std::env::split_paths(&path)
                .filter(|dir| {
                    dir.file_name()
                        .is_some_and(|n| n.eq_ignore_ascii_case("bin"))
                })
                .filter_map(|dir| dir.parent().map(|p| p.join("Code.exe"))),
        );
    }
    out
}

fn open_in_code(folder: &Path) -> Result<(), String> {
    let code = code_candidates(|key| std::env::var(key).ok())
        .into_iter()
        .find(|p| p.is_file())
        .ok_or("Nie znaleziono VS Code (Code.exe).")?;
    let mut command = std::process::Command::new(code);
    command
        .arg(folder)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Nie udało się uruchomić VS Code: {e}"))
}

/// Jak dotrzeć do sesji w aplikacji, w której działa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppLink {
    /// `claude://…` albo `codex://…` (protokoły zarejestrowane przez te aplikacje).
    Url(String),
    /// `orca terminal switch --terminal <uchwyt>`.
    OrcaTerminal(String),
    /// `orca open`: samo okno Orki (sesja w worktree Orki bez uchwytu terminala).
    OrcaWindow,
}

/// UUID w zapisie z myślnikami (identyfikator wątku Codexa).
fn is_uuid(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, len)| p.len() == len && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// Adres sesji w jej aplikacji. Claude Desktop: `claude://code/continue?session=local_…`
/// (bez identyfikatora widok „Sessions Waiting for You”, `claude://code/needs-input`);
/// Codex Desktop: `codex://threads/<id wątku>` (= `session_id` z hooka); Orca:
/// terminal sesji, a bez uchwytu samo okno.
pub fn app_link(entry: &Entry) -> Result<AppLink, String> {
    use crate::hook_wire as w;
    let host = entry.host.as_str();
    let id = entry.host_ref.as_deref();
    match host {
        w::HOST_CLAUDE_DESKTOP => Ok(AppLink::Url(
            match id.filter(|i| w::valid_claude_session(i)) {
                Some(id) => format!("claude://code/continue?session={id}"),
                None => "claude://code/needs-input".into(),
            },
        )),
        w::HOST_CODEX_DESKTOP if is_uuid(&entry.id) => {
            Ok(AppLink::Url(format!("codex://threads/{}", entry.id)))
        }
        w::HOST_CODEX_DESKTOP => Err("Codex nie podał identyfikatora wątku.".into()),
        w::HOST_ORCA => Ok(match id.filter(|h| w::valid_orca_handle(h)) {
            Some(handle) => AppLink::OrcaTerminal(handle.into()),
            None => AppLink::OrcaWindow,
        }),
        _ if entry.orca => Ok(AppLink::OrcaWindow),
        _ => Err("Ta sesja nie działa w Orce, Claude Desktop ani Codex Desktop.".into()),
    }
}

fn open_app(app: &tauri::AppHandle, entry: &Entry) -> Result<(), String> {
    match app_link(entry)? {
        AppLink::Url(url) => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_url(&url, None::<&str>)
                .map_err(|e| format!("Nie udało się otworzyć aplikacji: {e}"))
        }
        AppLink::OrcaTerminal(handle) => crate::orca::focus_terminal(app, &handle),
        AppLink::OrcaWindow => crate::orca::bring_to_front(app),
    }
}

fn open_target(app: &tauri::AppHandle, entry: &Entry, target: Target) -> Result<(), String> {
    if target == Target::App {
        return open_app(app, entry);
    }
    let folder = check_folder(&entry.agent.cwd, Path::is_dir)?;
    match target {
        Target::Folder => {
            use tauri_plugin_opener::OpenerExt;
            app.opener()
                .open_path(folder.to_string_lossy(), None::<&str>)
                .map_err(|e| format!("Nie udało się otworzyć folderu: {e}"))
        }
        Target::Code => open_in_code(&folder),
        Target::App => open_app(app, entry),
    }
}

/// „Przejdź”: otwiera folder sesji (folder z tablicy, nie od okna) i oznacza
/// sesję jako przejrzaną. Orka może startować do 90 s, więc poza wątkiem okna.
#[tauri::command]
pub async fn agent_board_open(
    app: tauri::AppHandle,
    kind: String,
    id: String,
    target: Target,
) -> Result<Vec<Entry>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let entry = snapshot(&app)
            .into_iter()
            .find(|e| e.agent.kind == kind && e.id == id)
            .ok_or("Tej sesji nie ma już na tablicy.")?;
        open_target(&app, &entry, target)?;
        Ok(mark(&app, &kind, Some(&id)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
