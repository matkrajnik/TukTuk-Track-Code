//! `tuktuk-track-bridge.exe`: host native messaging dla rozszerzenia Chrome i Edge
//! (PLAN_1.7.md, W4).
//!
//! Przeglądarka uruchamia ten program, gdy rozszerzenie otwiera połączenie
//! (`chrome.runtime.connectNative`), i rozmawia z nim przez stdin/stdout ramkami
//! native messaging (4 bajty długości little-endian + JSON). Każdą wiadomość
//! przekazujemy bez zmian do działającej aplikacji przez nazwany potok
//! `\\.\pipe\tuktuk-track-bridge-<użytkownik>` i odsyłamy przeglądarce odpowiedź
//! aplikacji. Gdy aplikacja nie działa, odpowiadamy „niepołączone” i nic więcej
//! nie robimy. Program kończy się, gdy przeglądarka zamknie stdin.
//!
//! Nic nie trafia na dysk ani do logów.
#[path = "../native_msg.rs"]
mod native_msg;

use native_msg::{read_frame, write_frame, FrameError};
use serde_json::{json, Value};
use std::io::{self, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut input = stdin.lock();
    let mut output = stdout.lock();
    let pipe = native_msg::pipe_name();
    loop {
        let reply = match read_frame(&mut input) {
            Ok(None) => break,
            Ok(Some(body)) => handle(&pipe, &body),
            Err(FrameError::TooLarge(n)) => json!({
                "ok": false,
                "error": format!("Wiadomość za duża ({n} bajtów, limit 1 MB)."),
            }),
            // Urwany strumień albo błąd odczytu: przeglądarka zamknęła połączenie.
            Err(_) => break,
        };
        let body = serde_json::to_vec(&reply).unwrap_or_default();
        if write_frame(&mut output, &body).is_err() {
            break;
        }
    }
    let _ = output.flush();
}

/// Jedna wiadomość z przeglądarki: sprawdzenie JSON i przekazanie do aplikacji.
fn handle(pipe: &str, body: &[u8]) -> Value {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(_)) => {}
        _ => return json!({ "ok": false, "error": "Niepoprawna wiadomość." }),
    }
    match native_msg::exchange(pipe, body) {
        Ok(reply) => serde_json::from_slice(&reply).unwrap_or_else(
            |_| json!({ "ok": false, "error": "Niepoprawna odpowiedź aplikacji." }),
        ),
        Err(_) => json!({
            "ok": false,
            "connected": false,
            "status": "niepołączone",
        }),
    }
}
