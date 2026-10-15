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
use crate::db;
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

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    /// `claude` albo `codex`.
    pub kind: String,
    /// Nazwa sesji, a bez niej nazwa folderu roboczego.
    pub name: String,
    pub cwd: String,
    /// Od kiedy pracuje (ms).
    pub since: i64,
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
    let name = s
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| folder(&s.cwd));
    Some(Agent {
        kind: "claude".into(),
        name,
        cwd: s.cwd,
        since,
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
    std::thread::spawn(move || loop {
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
        let changed = app.state::<State>().0.lock().ok().map(|mut w| {
            found.extend(w.codex_agents(fresh, now));
            w.update(found, now).then(|| w.agents.clone())
        });
        if let Some(Some(agents)) = changed {
            let _ = app.emit("agents-changed", agents);
        }
        std::thread::sleep(CHECK);
    });
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
        };
        assert!(w.update(vec![a.clone()], 2));
        assert!(!w.update(vec![a], 3));
        assert_eq!(w.last_busy, Some(3));
        assert!(w.update(Vec::new(), 4));
        assert_eq!(w.last_busy, Some(3));
    }
}
