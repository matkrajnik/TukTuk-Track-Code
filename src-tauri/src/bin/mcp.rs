//! `tuktuk-track-mcp.exe`: serwer MCP dla agentów (PLAN_BACKLOG.md, A2).
//!
//! Claude Code albo Codex uruchamia ten program jako serwer MCP na stdio
//! (jedna wiadomość JSON-RPC w wierszu). Program nie ma własnej logiki:
//! `initialize`, `ping` i `tools/list` odpowiada sam, a każde `tools/call`
//! przekazuje do działającej aplikacji nazwanym potokiem
//! `\.\pipe\tuktuk-track-mcp-<użytkownik>` i odsyła agentowi odpowiedź. Gdy
//! aplikacja nie działa, narzędzie zwraca błąd „TukTuk Track nie działa”.
//! Program kończy się, gdy agent zamknie stdin.
//!
//! Nic nie trafia na dysk ani do logów.
#[path = "../mcp_wire.rs"]
mod mcp_wire;
#[path = "../native_msg.rs"]
mod native_msg;

use serde_json::Value;
use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let pipe = mcp_wire::pipe_name();
    // CLI uruchamia serwer w folderze sesji; aplikacja używa go w `propose_link_folder`.
    let cwd = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let mut forward = |tool: &str, arguments: &Value| {
        mcp_wire::call_over_pipe(&pipe, tool, arguments, cwd.as_deref())
    };
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Some(reply) = mcp_wire::respond(&line, env!("CARGO_PKG_VERSION"), &mut forward) else {
            continue;
        };
        let text = serde_json::to_string(&reply).unwrap_or_default();
        if writeln!(output, "{text}").is_err() || output.flush().is_err() {
            break;
        }
    }
}
