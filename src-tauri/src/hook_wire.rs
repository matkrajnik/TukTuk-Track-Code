//! Zdarzenie hooka między programem `tuktuk-track-hook` a aplikacją (B8).
//!
//! Wspólny plik aplikacji (`mod hook_wire`) i programu hooka (`src/bin/hook.rs`,
//! przez `#[path]`). Agent (Claude Code albo Codex) uruchamia hooka przy każdym
//! zdarzeniu i podaje JSON na stdin; hook zostawia z niego tylko pola potrzebne
//! do stanu sesji (bez argumentów narzędzi, treści plików ani transkryptu; nazwa
//! narzędzia i komunikat powiadomienia opisują czekanie w tablicy sesji), pakuje je w
//! kopertę `{"agent": …, "event": {…}}` i wysyła ramką native messaging do
//! potoku `\\.\pipe\tuktuk-track-hooks`. Gdy aplikacja nie działa, kończy się cicho.
#![allow(dead_code)]

use crate::native_msg::{read_frame, write_frame, FrameError};
use serde_json::{json, Map, Value};
use std::time::Duration;

/// Nazwa potoku, na którym aplikacja nasłuchuje zdarzeń hooków.
pub const PIPE: &str = r"\\.\pipe\tuktuk-track-hooks";
/// Pierwszy prompt trzymany w pamięci sesji ma najwyżej tyle znaków.
pub const PROMPT_MAX: usize = 500;
/// Zmienna środowiskowa ustawiana przez aplikację na procesach AI uruchamianych
/// przez nią samą (A1): ich zdarzeń nie wysyłamy, żeby nie liczyły się jako praca
/// użytkownika.
pub const AI_ENV: &str = "TUKTUK_TRACK_AI";

/// Czy zdarzenie ma zostać odrzucone: proces uruchomiony z `TUKTUK_TRACK_AI=1`.
pub fn suppressed(env_value: Option<&str>) -> bool {
    env_value.map(str::trim) == Some("1")
}

/// Zmienna ustawiana na krokach modułu Workflows (`<runId>:<executionId>`).
/// Ich zdarzeń hook nie wysyła: czas i stan kroków pochodzą wyłącznie ze zdarzeń
/// runtime, więc tablica agentów i czas pracy nie liczą ich drugi raz.
pub const WORKFLOW_ENV: &str = "TUKTUK_TRACK_WORKFLOW";

/// Czy zdarzenie pochodzi z kroku workflow: niepusta wartość po przycięciu.
pub fn workflow_suppressed(env_value: Option<&str>) -> bool {
    env_value.is_some_and(|v| !v.trim().is_empty())
}

/// Komunikat powiadomienia (`Notification`) ma najwyżej tyle znaków.
pub const MESSAGE_MAX: usize = 200;
/// Nazwa narzędzia (`PreToolUse`, `PermissionRequest`) ma najwyżej tyle znaków.
pub const TOOL_MAX: usize = 64;

/// Pola kopiowane z zdarzenia agenta (tylko napisy).
const FIELDS: [&str; 7] = [
    "session_id",
    "cwd",
    "hook_event_name",
    "agent_id",
    "notification_type",
    "reason",
    "turn_id",
];

/// Skraca tekst do `max` znaków (po przycięciu białych znaków).
pub fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Źródła sesji (gdzie działa agent), wykrywane ze środowiska procesu hooka.
pub const HOST_ORCA: &str = "orca";
pub const HOST_CLAUDE_DESKTOP: &str = "claude-desktop";
pub const HOST_CODEX_DESKTOP: &str = "codex-desktop";
pub const HOST_VSCODE: &str = "vscode";
pub const HOST_TERMINAL: &str = "terminal";
pub const HOSTS: [&str; 5] = [
    HOST_ORCA,
    HOST_CLAUDE_DESKTOP,
    HOST_CODEX_DESKTOP,
    HOST_VSCODE,
    HOST_TERMINAL,
];

/// Uchwyt terminala Orki (`ORCA_TERMINAL_HANDLE`, np. `term_abc123`).
pub fn valid_orca_handle(text: &str) -> bool {
    // Pierwszy znak nie może być `-`: uchwyt idzie jako wartość `--terminal`.
    (1..=128).contains(&text.len())
        && text
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_')
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b':' | b'.'))
}

/// Identyfikator sesji Claude Desktop (`CLAUDE_CODE_HOST_SESSION_ID`); ten sam
/// wzorzec przyjmuje `claude://code/continue?session=…`.
pub fn valid_claude_session(text: &str) -> bool {
    text.strip_prefix("local_").is_some_and(|id| {
        (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    })
}

/// Skąd jest sesja i jej identyfikator w tej aplikacji. Kolejność: terminal Orki
/// (agent CLI w Orce), Claude Desktop, Codex Desktop, VS Code, zwykły terminal.
/// Zmienne: `ORCA_TERMINAL_HANDLE`, `CLAUDE_CODE_ENTRYPOINT` z
/// `CLAUDE_CODE_HOST_SESSION_ID`, `CODEX_INTERNAL_ORIGINATOR_OVERRIDE`, `TERM_PROGRAM`.
pub fn host(agent: &str, env: &dyn Fn(&str) -> Option<String>) -> (&'static str, Option<String>) {
    let var = |key: &str| {
        env(key)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    if let Some(handle) = var("ORCA_TERMINAL_HANDLE").filter(|h| valid_orca_handle(h)) {
        return (HOST_ORCA, Some(handle));
    }
    if agent == "claude" {
        match var("CLAUDE_CODE_ENTRYPOINT").as_deref() {
            Some("claude-desktop") => {
                let id = var("CLAUDE_CODE_HOST_SESSION_ID").filter(|id| valid_claude_session(id));
                return (HOST_CLAUDE_DESKTOP, id);
            }
            Some("claude-vscode") => return (HOST_VSCODE, None),
            _ => {}
        }
    }
    if agent == "codex" {
        let origin = var("CODEX_INTERNAL_ORIGINATOR_OVERRIDE")
            .unwrap_or_default()
            .to_lowercase();
        if origin.contains("desktop") {
            return (HOST_CODEX_DESKTOP, None);
        }
        if origin.contains("vscode") {
            return (HOST_VSCODE, None);
        }
    }
    if var("TERM_PROGRAM").is_some_and(|t| t.eq_ignore_ascii_case("vscode")) {
        return (HOST_VSCODE, None);
    }
    (HOST_TERMINAL, None)
}

/// Koperta bez środowiska (bez źródła sesji); program hooka używa [`envelope_from`].
pub fn envelope(agent: &str, raw: &[u8]) -> Option<Vec<u8>> {
    envelope_with(agent, raw, None)
}

/// Koperta ze źródłem sesji (`host`, `host_ref`) ze środowiska procesu hooka.
pub fn envelope_from(
    agent: &str,
    raw: &[u8],
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<Vec<u8>> {
    envelope_with(agent, raw, Some(env))
}

/// Koperta do wysłania: tylko znani agenci i obiekt JSON; `None`, gdy wejście nie
/// ma sensu (wtedy hook nic nie wysyła).
fn envelope_with(
    agent: &str,
    raw: &[u8],
    env: Option<&dyn Fn(&str) -> Option<String>>,
) -> Option<Vec<u8>> {
    if agent != "claude" && agent != "codex" {
        return None;
    }
    let value: Value = serde_json::from_slice(raw).ok()?;
    let obj = value.as_object()?;
    let mut event = Map::new();
    for field in FIELDS {
        if let Some(text) = obj.get(field).and_then(Value::as_str) {
            event.insert(field.into(), Value::String(clip(text, 1024)));
        }
    }
    let name = obj.get("hook_event_name").and_then(Value::as_str);
    let mut copy = |field: &str, max: usize| {
        if let Some(text) = obj.get(field).and_then(Value::as_str) {
            event.insert(field.into(), Value::String(clip(text, max)));
        }
    };
    // Prompt tylko z UserPromptSubmit i tylko jego początek (pod asystenta, A4).
    if name == Some("UserPromptSubmit") {
        copy("prompt", PROMPT_MAX);
    }
    // Powód czekania na użytkownika (tablica sesji): sama nazwa narzędzia, bez
    // argumentów, i komunikat powiadomienia agenta.
    if matches!(name, Some("PreToolUse" | "PermissionRequest")) {
        copy("tool_name", TOOL_MAX);
    }
    if name == Some("Notification") {
        copy("message", MESSAGE_MAX);
    }
    // Źródło sesji: nazwa aplikacji i jej identyfikator (uchwyt terminala Orki,
    // `local_…` Claude Desktop); żadnych innych zmiennych środowiskowych.
    if let Some(env) = env {
        let (host, host_ref) = host(agent, env);
        event.insert("host".into(), Value::String(host.into()));
        if let Some(id) = host_ref {
            event.insert("host_ref".into(), Value::String(id));
        }
    }
    serde_json::to_vec(&json!({ "agent": agent, "event": event })).ok()
}

/// Wysyła ramkę do aplikacji i czeka na potwierdzenie. Zajęty potok (inne zdarzenie
/// w toku) próbujemy otworzyć jeszcze kilka razy; brak potoku to błąd (aplikacja
/// nie działa).
pub fn forward(pipe: &str, body: &[u8]) -> Result<(), FrameError> {
    const PIPE_BUSY: i32 = 231;
    let mut attempt = 0;
    let mut file = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe)
        {
            Ok(f) => break f,
            Err(e) if e.raw_os_error() == Some(PIPE_BUSY) && attempt < 20 => {
                attempt += 1;
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(e) => return Err(FrameError::Io(e)),
        }
    };
    write_frame(&mut file, body)?;
    read_frame(&mut file)?;
    Ok(())
}

#[cfg(test)]
mod workflow_env_tests {
    use super::*;

    #[test]
    fn workflow_env_suppresses_events() {
        assert!(workflow_suppressed(Some("12:exec-3")));
        assert!(!workflow_suppressed(None));
    }

    #[test]
    fn empty_workflow_env_does_not_suppress() {
        assert!(!workflow_suppressed(Some("")));
        assert!(!workflow_suppressed(Some("   ")));
    }
}
