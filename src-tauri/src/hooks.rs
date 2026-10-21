//! Wykrywanie agentów AI przez hooki Claude Code i Codexa (PLAN_BACKLOG.md, B8).
//!
//! Przepływ: agent uruchamia `todo-track-hook.exe` przy każdym zdarzeniu → potok
//! `\\.\pipe\todo-track-hooks` → wątek w tym module (`start`) → [`Sessions`] (maszyna
//! stanów sesji) w `agents::Watch` → pętla `agents::start` liczy z niej listę
//! pracujących agentów. Stan nie jest skanowany z plików: po restarcie aplikacji
//! lista jest pusta i zapełnia się zdarzeniami (`PreToolUse` przychodzi co kilka
//! sekund).
//!
//! Maszyna stanów sesji (klucz: agent + `session_id`):
//! - `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `SubagentStart` = praca;
//! - `PermissionRequest` i `Notification` (`permission_prompt`, `agent_needs_input`,
//!   `idle_prompt` w trakcie pracy) = czekanie na użytkownika, nie praca;
//! - `Stop` = koniec tury, ale dopóki trwają subagenci, sesja nadal pracuje;
//!   `Interrupt` = koniec; `SessionEnd` usuwa sesję;
//! - zdarzenia z `agent_id` to subagenci, liczeni do sesji nadrzędnej;
//! - sesja bez zdarzeń przez [`SILENCE`] kończy się sama (ubity proces).
//!
//! Rejestracja hooków (`hooks_config`) jest zgodna z ustawieniem `agent_hooks`:
//! przy starcie i co minutę (po aktualizacji instalatorem ścieżka exe się zmienia),
//! przy zmianie ustawienia i po przywróceniu kopii.
use crate::hooks_config::{self, Dirs, Registration};
use crate::{agents, db, hook_wire, settings};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::Manager;

/// Sesja bez zdarzeń tak długo kończy się sama.
pub const SILENCE: i64 = 10 * 60_000;
/// Zakończona sesja jest zapominana po takim czasie bez zdarzeń.
pub const FORGET: i64 = 12 * 3_600_000;
/// Najwięcej pamiętanych sesji (ochrona pamięci przed zalewem zdarzeń).
const MAX_SESSIONS: usize = 128;
/// Ile ostatnich zdarzeń pokazuje diagnostyka w Ustawieniach.
const RECENT: usize = 30;
/// Co ile ponawiamy sprawdzenie rejestracji w plikach agentów.
const RESYNC: i64 = 60_000;

/// Zdarzenie hooka po odchudzeniu (`hook_wire::envelope`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HookEvent {
    /// `claude` albo `codex`.
    pub agent: String,
    pub session_id: String,
    pub cwd: String,
    pub event: String,
    /// Obecne tylko w zdarzeniach subagentów.
    pub agent_id: Option<String>,
    pub notification_type: Option<String>,
    pub prompt: Option<String>,
}

/// Koperta z potoku → zdarzenie. `None` dla niepoprawnej koperty, nieznanego agenta
/// albo zdarzenia bez `session_id` i nazwy.
pub fn parse(body: &[u8]) -> Option<HookEvent> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let agent = value.get("agent")?.as_str()?;
    if agent != "claude" && agent != "codex" {
        return None;
    }
    let event = value.get("event")?.as_object()?;
    let text = |key: &str| {
        event
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let session_id = text("session_id")?;
    let name = text("hook_event_name")?;
    if session_id.chars().count() > 200 || name.chars().count() > 64 {
        return None;
    }
    Some(HookEvent {
        agent: agent.into(),
        session_id: session_id.into(),
        cwd: text("cwd")
            .map(|c| c.chars().take(1024).collect())
            .unwrap_or_default(),
        event: name.into(),
        agent_id: text("agent_id").map(|s| s.chars().take(200).collect()),
        notification_type: text("notification_type").map(str::to_string),
        prompt: text("prompt").map(|p| hook_wire::clip(p, hook_wire::PROMPT_MAX)),
    })
}

/// Faza sesji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Working,
    /// Agent czeka na użytkownika (zgoda, pytanie); to nie jest praca.
    Waiting,
    Done,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub agent: String,
    pub cwd: String,
    pub phase: Phase,
    /// Początek bieżącej pracy (ms); zaczyna się od nowa po czekaniu.
    pub since: i64,
    /// Ostatnie zdarzenie (ms).
    pub last: i64,
    /// Subagenci, którzy zaczęli pracę i jeszcze się nie zakończyli.
    pub subagents: BTreeSet<String>,
    /// Pierwszy prompt sesji (do 500 znaków), tylko w pamięci; pod asystenta (A4).
    #[allow(dead_code)]
    pub first_prompt: Option<String>,
}

/// Wpis diagnostyki: ostatnie zdarzenia hooków (bez treści promptów).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub at: i64,
    pub agent: String,
    pub event: String,
    /// Początek identyfikatora sesji.
    pub session: String,
    pub cwd: String,
    /// Subagent, notification_type albo faza po zdarzeniu.
    pub detail: String,
}

/// Wszystkie sesje agentów znane z hooków.
#[derive(Default)]
pub struct Sessions {
    map: HashMap<(String, String), Session>,
    recent: VecDeque<Recent>,
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Working => "pracuje",
        Phase::Waiting => "czeka",
        Phase::Done => "koniec",
    }
}

impl Sessions {
    /// Stosuje zdarzenie. Zwraca `true`, gdy zmieniło się to, co widać w liście
    /// pracujących agentów (faza, folder, pojawienie się albo usunięcie sesji);
    /// wtedy pętla agentów budzi się od razu.
    pub fn apply(&mut self, ev: &HookEvent, now: i64) -> bool {
        let key = (ev.agent.clone(), ev.session_id.clone());
        let before = self.visible(&key);
        let mut detail = ev
            .notification_type
            .clone()
            .or_else(|| ev.agent_id.clone())
            .unwrap_or_default();
        if ev.event == "SessionEnd" {
            self.map.remove(&key);
        } else {
            if !self.map.contains_key(&key) {
                self.make_room(now);
            }
            let s = self.map.entry(key.clone()).or_insert_with(|| Session {
                agent: ev.agent.clone(),
                cwd: ev.cwd.clone(),
                phase: Phase::Done,
                since: now,
                last: now,
                subagents: BTreeSet::new(),
                first_prompt: None,
            });
            s.last = now;
            if !ev.cwd.is_empty() {
                s.cwd = ev.cwd.clone();
            }
            Self::transition(s, ev, now);
            if detail.is_empty() {
                detail = phase_name(s.phase).into();
            }
        }
        self.recent.push_back(Recent {
            at: now,
            agent: ev.agent.clone(),
            event: ev.event.clone(),
            session: ev.session_id.chars().take(8).collect(),
            cwd: ev.cwd.clone(),
            detail,
        });
        while self.recent.len() > RECENT {
            self.recent.pop_front();
        }
        before != self.visible(&key)
    }

    fn transition(s: &mut Session, ev: &HookEvent, now: i64) {
        let work = |s: &mut Session| {
            if s.phase != Phase::Working {
                s.since = now;
            }
            s.phase = Phase::Working;
        };
        match ev.event.as_str() {
            "UserPromptSubmit" => {
                work(s);
                if s.first_prompt.is_none() {
                    s.first_prompt = ev.prompt.clone();
                }
            }
            "PreToolUse" | "PostToolUse" => {
                if let Some(id) = &ev.agent_id {
                    s.subagents.insert(id.clone());
                }
                work(s);
            }
            "SubagentStart" => {
                if let Some(id) = &ev.agent_id {
                    s.subagents.insert(id.clone());
                }
                work(s);
            }
            "SubagentStop" => {
                if let Some(id) = &ev.agent_id {
                    s.subagents.remove(id);
                }
            }
            "PermissionRequest" => s.phase = Phase::Waiting,
            "Notification" => match ev.notification_type.as_deref() {
                Some("permission_prompt" | "agent_needs_input" | "elicitation_dialog") => {
                    s.phase = Phase::Waiting
                }
                // Claude przypomina o bezczynności także po zakończonej turze.
                Some("idle_prompt") if s.phase == Phase::Working => s.phase = Phase::Waiting,
                _ => {}
            },
            "Stop" => {
                if s.subagents.is_empty() {
                    s.phase = Phase::Done;
                }
            }
            "Interrupt" => {
                s.subagents.clear();
                s.phase = Phase::Done;
            }
            // SessionStart zakłada sesję (bezczynną); pozostałe zdarzenia tylko
            // odświeżają „ostatnie zdarzenie”.
            _ => {}
        }
    }

    /// Faza i folder sesji widoczne w liście (praca) albo `None`.
    fn visible(&self, key: &(String, String)) -> Option<(String, i64)> {
        self.map
            .get(key)
            .filter(|s| s.phase == Phase::Working)
            .map(|s| (s.cwd.clone(), s.since))
    }

    /// Zwalnia miejsce na nową sesję: najpierw zakończone, potem najstarsze.
    fn make_room(&mut self, now: i64) {
        self.expire(now);
        while self.map.len() >= MAX_SESSIONS {
            let oldest = self
                .map
                .iter()
                .min_by_key(|(_, s)| (s.phase != Phase::Done, s.last))
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => self.map.remove(&k),
                None => break,
            };
        }
    }

    /// Timeout braku zdarzeń: praca i czekanie kończą się po [`SILENCE`], a
    /// zakończone sesje są zapominane po [`FORGET`]. Zwraca `true`, gdy ubyła
    /// pracująca sesja.
    pub fn expire(&mut self, now: i64) -> bool {
        let mut changed = false;
        for s in self.map.values_mut() {
            if s.phase != Phase::Done && (now - s.last > SILENCE || now < s.last) {
                changed |= s.phase == Phase::Working;
                s.phase = Phase::Done;
                s.subagents.clear();
            }
        }
        self.map
            .retain(|_, s| s.phase != Phase::Done || (now >= s.last && now - s.last <= FORGET));
        changed
    }

    /// Pracujące sesje jako agenci (jedna sesja = jeden agent, subagenci w niej).
    pub fn working(&self) -> Vec<agents::Agent> {
        let mut list: Vec<agents::Agent> = self
            .map
            .values()
            .filter(|s| s.phase == Phase::Working)
            .map(|s| {
                let name = agents::folder(&s.cwd);
                agents::Agent {
                    kind: s.agent.clone(),
                    name: if name.is_empty() {
                        "sesja".into()
                    } else {
                        name
                    },
                    cwd: s.cwd.clone(),
                    since: s.since,
                    ..agents::Agent::default()
                }
            })
            .collect();
        list.sort_by(|a, b| (a.since, &a.kind, &a.cwd).cmp(&(b.since, &b.kind, &b.cwd)));
        list
    }

    pub fn session(&self, agent: &str, id: &str) -> Option<&Session> {
        self.map.get(&(agent.to_string(), id.to_string()))
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.recent.clear();
    }

    /// Ostatnie zdarzenia, od najnowszego.
    pub fn recent(&self) -> Vec<Recent> {
        self.recent.iter().rev().cloned().collect()
    }
}

// --- zaufanie hooków Codexa (`[hooks.state]` w config.toml) ---

/// Zaufanie do hooków aplikacji w Codexie.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// Hooków nie ma w `hooks.json` (nic do zatwierdzania).
    None,
    /// Żadne zdarzenie nie ma zatwierdzonego hooka: wymaga `/hooks` w Codexie.
    Untrusted,
    Partial,
    Trusted,
}

impl Trust {
    pub fn as_str(self) -> &'static str {
        match self {
            Trust::None => "none",
            Trust::Untrusted => "untrusted",
            Trust::Partial => "partial",
            Trust::Trusted => "trusted",
        }
    }
}

/// Cudzysłowiony napis z początku tekstu (po białych znakach) i reszta.
fn quoted(text: &str) -> Option<(String, &str)> {
    let rest = text.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                if let Some((_, n)) = chars.next() {
                    out.push(n);
                }
            }
            '"' => return Some((out, &rest[i + 1..])),
            c => out.push(c),
        }
    }
    None
}

/// Wartość `trusted_hash` z linii (`trusted_hash = "sha256:…"`), jeśli niepusta.
fn trusted_hash(line: &str) -> Option<String> {
    let at = line.find("trusted_hash")?;
    let after = line[at + "trusted_hash".len()..].trim_start();
    let value = quoted(after.strip_prefix('=')?)?.0;
    (!value.trim().is_empty()).then_some(value)
}

/// Wpisy sekcji `[hooks.state]` z `config.toml` jako (klucz, czy ma `trusted_hash`).
/// Lekki parser zamiast pełnego TOML-a: obsługuje tabele `[hooks.state."klucz"]`
/// z polem `trusted_hash` oraz wpisy `"klucz" = { trusted_hash = "…" }` pod
/// `[hooks.state]`.
pub fn parse_hook_state(text: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = Vec::new();
    let mut add = |key: String, trusted: bool| {
        if let Some(e) = entries.iter_mut().find(|e| e.0 == key) {
            e.1 |= trusted;
        } else {
            entries.push((key, trusted));
        }
    };
    // `Some(None)`: w `[hooks.state]` (wpisy w liniach), `Some(Some(k))`: w tabeli klucza.
    let mut current: Option<Option<String>> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            let header = line
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim();
            current = match header.strip_prefix("hooks.state") {
                Some("") => Some(None),
                Some(rest) if rest.starts_with('.') => {
                    let key = rest.trim_start_matches('.').trim();
                    let key = quoted(key).map(|q| q.0).unwrap_or_else(|| key.to_string());
                    add(key.clone(), false);
                    Some(Some(key))
                }
                _ => None,
            };
            continue;
        }
        match &current {
            Some(Some(key)) => {
                if trusted_hash(line).is_some() {
                    add(key.clone(), true);
                }
            }
            Some(None) => {
                if let Some((key, _)) = quoted(line) {
                    add(key, trusted_hash(line).is_some());
                } else if let Some((key, rest)) = line.split_once(".trusted_hash") {
                    add(
                        key.trim().trim_matches('"').to_string(),
                        trusted_hash(&format!("trusted_hash{rest}")).is_some(),
                    );
                }
            }
            None => {}
        }
    }
    entries
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Zaufanie do hooków aplikacji: zdarzenie liczy się jako zatwierdzone, gdy klucz
/// wpisu `hooks.state` wskazuje `hooks.json` i to zdarzenie (w dowolnym zapisie,
/// np. `PreToolUse` albo `pre_tool_use`) i wpis ma `trusted_hash`.
pub fn codex_trust(registered: Registration, config: &str) -> Trust {
    if registered == Registration::Missing {
        return Trust::None;
    }
    let states = parse_hook_state(config);
    let trusted = hooks_config::CODEX_EVENTS
        .iter()
        .filter(|event| {
            let token = squash(event);
            states.iter().any(|(key, ok)| {
                let key = squash(key);
                *ok && key.contains("hooksjson") && key.contains(&token)
            })
        })
        .count();
    match trusted {
        0 => Trust::Untrusted,
        n if n == hooks_config::CODEX_EVENTS.len() => Trust::Trusted,
        _ => Trust::Partial,
    }
}

// --- stan aplikacji i rejestracja ---

#[derive(Default)]
struct Registered {
    /// Ostatnio udanie zastosowany stan wyłącznika (`None`: jeszcze nie albo błąd).
    applied: Option<bool>,
    last_try: i64,
    error: String,
}

#[derive(Default)]
pub struct State {
    registered: Mutex<Registered>,
}

pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

fn exe_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(hooks_config::HOOK_EXE)))
        .filter(|p| p.is_file())
}

/// Rejestruje albo usuwa hooki w plikach agentów; błędy obu agentów w jednym tekście.
fn apply_dirs(dirs: &Dirs, exe: Option<&Path>, enabled: bool) -> Result<(), String> {
    if enabled && exe.is_none() {
        return Err(format!(
            "Brak programu {} obok aplikacji. Zainstaluj aplikację ponownie.",
            hooks_config::HOOK_EXE
        ));
    }
    let (claude, codex) = hooks_config::sync(dirs, enabled.then_some(exe).flatten());
    let errors: Vec<String> = [("Claude Code", claude), ("Codex", codex)]
        .into_iter()
        .filter(|(_, o)| !o.error.is_empty())
        .map(|(name, o)| format!("{name}: {}", o.error))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" "))
    }
}

/// Stosuje ustawienie `agent_hooks` (start, zapis ustawień, przywrócenie kopii).
pub fn sync(app: &tauri::AppHandle, s: &settings::AppSettings) {
    attempt(app, s.agent_hooks, db::now(), false);
}

/// Ponowne sprawdzenie rejestracji co [`RESYNC`] z pętli agentów (nowa ścieżka exe
/// po aktualizacji, plik nadpisany przez agenta albo błąd przy poprzedniej próbie).
pub fn retry(app: &tauri::AppHandle, now: i64) {
    attempt(app, settings::current(app).agent_hooks, now, true);
}

fn attempt(app: &tauri::AppHandle, enabled: bool, now: i64, periodic: bool) {
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    {
        let Ok(mut reg) = state.registered.lock() else {
            return;
        };
        let due = if periodic {
            (enabled || reg.applied != Some(false))
                && (now < reg.last_try || now - reg.last_try >= RESYNC)
        } else {
            true
        };
        if !due {
            return;
        }
        reg.last_try = now;
    }
    let result = apply_dirs(&Dirs::detect(), exe_path().as_deref(), enabled);
    if !enabled {
        // Wyłączone: sesje z hooków znikają z listy.
        if let Some(agents) = app.try_state::<agents::State>() {
            agents.clear_sessions();
        }
    }
    if let Ok(mut reg) = state.registered.lock() {
        reg.applied = result.is_ok().then_some(enabled);
        reg.error = result.err().unwrap_or_default();
    };
}

/// Zdarzenie z potoku: do maszyny stanów, o ile hooki są włączone.
fn on_event(app: &tauri::AppHandle, body: &[u8]) -> Value {
    if let Some(ev) = parse(body) {
        if settings::current(app).agent_hooks {
            if let Some(agents) = app.try_state::<agents::State>() {
                agents.apply_hook(&ev, db::now());
            }
        }
    }
    json!({ "ok": true })
}

/// Wątek nasłuchu na potoku; wołać po `create_windows`.
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) =
            crate::browser::pipe::serve(hook_wire::PIPE, move |body| on_event(&app, body))
        {
            eprintln!("Hooki agentów: potok niedostępny ({e}).");
        }
    });
}

// --- status do Ustawień → Agenci ---

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStatus {
    /// Folder konfiguracji agenta istnieje (agent jest zainstalowany).
    pub found: bool,
    pub file: String,
    /// `missing`, `stale` albo `current`.
    pub registration: String,
    pub error: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enabled: bool,
    /// Program hooka leży obok aplikacji.
    pub exe_found: bool,
    /// Błąd ostatniej rejestracji.
    pub error: String,
    pub claude: AgentStatus,
    pub codex: AgentStatus,
    /// `none`, `untrusted`, `partial` albo `trusted`.
    pub codex_trust: String,
    pub events: Vec<Recent>,
}

fn agent_status(
    dir: Option<&Path>,
    file: Option<PathBuf>,
    events: &[&str],
    command: &str,
) -> (AgentStatus, Registration) {
    let shown = file
        .as_ref()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (found, reg, error) = hooks_config::inspect(dir, file, events, command);
    (
        AgentStatus {
            found,
            file: shown,
            registration: reg.as_str().into(),
            error,
        },
        reg,
    )
}

/// Stan rejestracji w plikach agentów (`dirs`, `exe` wstrzykiwane).
pub fn status_for(
    dirs: &Dirs,
    exe: Option<&Path>,
    enabled: bool,
    error: String,
    events: Vec<Recent>,
) -> Status {
    let exe_for_cmd = exe.unwrap_or(Path::new(hooks_config::HOOK_EXE));
    let (claude, _) = agent_status(
        dirs.claude.as_deref(),
        dirs.claude_file(),
        &hooks_config::CLAUDE_EVENTS,
        &hooks_config::hook_command(exe_for_cmd, "claude"),
    );
    let (codex, reg) = agent_status(
        dirs.codex.as_deref(),
        dirs.codex_file(),
        &hooks_config::CODEX_EVENTS,
        &hooks_config::hook_command(exe_for_cmd, "codex"),
    );
    let config = dirs
        .codex_config()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    Status {
        enabled,
        exe_found: exe.is_some(),
        error,
        claude,
        codex,
        codex_trust: codex_trust(reg, &config).as_str().into(),
        events,
    }
}

#[tauri::command]
pub fn agent_hooks_status(app: tauri::AppHandle) -> Status {
    let enabled = settings::current(&app).agent_hooks;
    let error = app
        .try_state::<State>()
        .and_then(|s| s.registered.lock().ok().map(|r| r.error.clone()))
        .unwrap_or_default();
    let events = app
        .try_state::<agents::State>()
        .map(|s| s.recent_hooks())
        .unwrap_or_default();
    status_for(
        &Dirs::detect(),
        exe_path().as_deref(),
        enabled,
        error,
        events,
    )
}

#[cfg(test)]
mod tests;
