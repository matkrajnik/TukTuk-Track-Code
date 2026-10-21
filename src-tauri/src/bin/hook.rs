//! `todo-track-hook.exe`: hook Claude Code i Codexa (PLAN_BACKLOG.md, B8).
//!
//! Agent uruchamia ten program przy każdym zdarzeniu (`todo-track-hook claude`
//! albo `todo-track-hook codex`) i podaje JSON na stdin. Program przekazuje
//! odchudzone zdarzenie do działającej aplikacji nazwanym potokiem
//! `\\.\pipe\todo-track-hooks` i kończy się. Gdy aplikacja nie działa, kończy się
//! cicho, bez błędu i bez żadnego wyjścia (nie blokuje agenta). Procesy z
//! `TODO_TRACK_AI=1` (agent uruchomiony przez aplikację, A1) są pomijane.
//!
//! `--uninstall` (deinstalator) usuwa wpisy hooków z plików Claude Code i Codexa.
//!
//! Nic nie trafia na dysk ani do logów (poza `--uninstall`).
#[path = "../hook_wire.rs"]
mod hook_wire;
#[path = "../hooks_config.rs"]
mod hooks_config;
#[path = "../native_msg.rs"]
mod native_msg;

use std::io::Read;

fn main() {
    let mut args = std::env::args().skip(1);
    let first = args.next().unwrap_or_default();
    if first == "--uninstall" {
        hooks_config::sync(&hooks_config::Dirs::detect(), None);
        return;
    }
    if hook_wire::suppressed(std::env::var(hook_wire::AI_ENV).ok().as_deref()) {
        return;
    }
    let mut raw = Vec::new();
    let _ = std::io::stdin()
        .lock()
        .take(native_msg::MAX as u64)
        .read_to_end(&mut raw);
    if let Some(body) = hook_wire::envelope(&first, &raw) {
        let _ = hook_wire::forward(hook_wire::PIPE, &body);
    }
}
