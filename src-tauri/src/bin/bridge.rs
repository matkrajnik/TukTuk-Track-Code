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
use std::time::Duration;

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
    match forward(pipe, body) {
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

/// Wysyła ramkę do aplikacji i czeka na odpowiedź. Zajęty potok (inna wiadomość
/// w toku) próbujemy otworzyć jeszcze kilka razy.
fn forward(pipe: &str, body: &[u8]) -> Result<Vec<u8>, FrameError> {
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
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(FrameError::Io(e)),
        }
    };
    write_frame(&mut file, body)?;
    read_frame(&mut file)?.ok_or(FrameError::Truncated)
}
