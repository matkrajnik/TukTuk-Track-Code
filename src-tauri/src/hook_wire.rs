//! Zdarzenie hooka między programem `tuktuk-track-hook` a aplikacją (B8).
//!
//! Wspólny plik aplikacji (`mod hook_wire`) i programu hooka (`src/bin/hook.rs`,
//! przez `#[path]`). Agent (Claude Code albo Codex) uruchamia hooka przy każdym
//! zdarzeniu i podaje JSON na stdin; hook zostawia z niego tylko pola potrzebne
//! do stanu sesji (bez narzędzi, treści plików ani transkryptu), pakuje je w
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

/// Koperta do wysłania: tylko znani agenci i obiekt JSON; `None`, gdy wejście nie
/// ma sensu (wtedy hook nic nie wysyła).
pub fn envelope(agent: &str, raw: &[u8]) -> Option<Vec<u8>> {
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
    // Prompt tylko z UserPromptSubmit i tylko jego początek (pod asystenta, A4).
    if obj.get("hook_event_name").and_then(Value::as_str) == Some("UserPromptSubmit") {
        if let Some(prompt) = obj.get("prompt").and_then(Value::as_str) {
            event.insert("prompt".into(), Value::String(clip(prompt, PROMPT_MAX)));
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
