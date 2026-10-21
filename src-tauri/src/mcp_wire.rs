//! Protokół MCP między agentem, programem `todo-track-mcp` i aplikacją (A2).
//!
//! Wspólny plik aplikacji (`mod mcp_wire`) i programu `todo-track-mcp`
//! (`src/bin/todo-track-mcp.rs`, przez `#[path]`). Agent (Claude Code albo Codex)
//! uruchamia program jako serwer MCP na stdio: jedna wiadomość JSON-RPC w wierszu.
//! Program nie ma własnej logiki: `initialize`, `ping` i `tools/list` odpowiada
//! sam, a każde `tools/call` przekazuje ramką native messaging do działającej
//! aplikacji nazwanym potokiem `\\.\pipe\todo-track-mcp-<użytkownik>` i odsyła
//! agentowi odpowiedź. Bez aplikacji narzędzie zwraca błąd „Todo Track nie działa”.
//!
//! Opisy narzędzi są po angielsku i krótkie; zapisy to propozycje, nie zmiany.
//! Nic nie trafia na dysk ani do logów.
#![allow(dead_code)]

use crate::native_msg::{self, read_frame, write_frame, FrameError};
use serde_json::{json, Value};
use std::time::Duration;

/// Nazwa serwera MCP w rejestracji i w `serverInfo`.
pub const SERVER_NAME: &str = "todo-track";
/// Najnowsza wersja protokołu, którą serwer zna.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Wersje protokołu, które serwer potrafi obsłużyć (to samo `tools/*`).
pub const SUPPORTED: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// Błąd narzędzia, gdy aplikacja nie działa (albo nie odpowiada na potoku).
pub const NOT_RUNNING: &str = "Todo Track nie działa. Uruchom aplikację i spróbuj ponownie.";

/// `\\.\pipe\todo-track-mcp-<użytkownik>`.
pub fn pipe_name_for(user: &str) -> String {
    format!(
        r"\\.\pipe\todo-track-mcp-{}",
        native_msg::sanitize_user(user)
    )
}

/// Potok bieżącego użytkownika (zmienna `USERNAME`).
pub fn pipe_name() -> String {
    pipe_name_for(&std::env::var("USERNAME").unwrap_or_default())
}

/// Narzędzia tylko do odczytu: wykonywane od razu.
pub const READ_TOOLS: [&str; 6] = [
    "tasks_list",
    "task_get",
    "day_summary",
    "week_summary",
    "current_measurement",
    "ts_tasks",
];
/// Narzędzia zapisu: tworzą propozycje czekające na „Zastosuj” w aplikacji.
pub const PROPOSE_TOOLS: [&str; 5] = [
    "propose_task_create",
    "propose_note_append",
    "propose_link_folder",
    "propose_switch",
    "propose_plan",
];

/// Czy `name` to narzędzie tego serwera.
pub fn is_tool(name: &str) -> bool {
    READ_TOOLS.contains(&name) || PROPOSE_TOOLS.contains(&name)
}

const PROPOSAL_NOTE: &str =
    "Creates a proposal the user must approve in Todo Track; changes nothing by itself.";

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
        },
    })
}

/// `tools/list`: definicje narzędzi. Brak narzędzi do wysyłki TS, zapisów w Jirze
/// i Redmine, usuwania oraz zmiany ustawień (świadomie, plan A2).
pub fn tools() -> Value {
    let task = json!({
        "type": ["string", "integer"],
        "description": "Task id, Jira key (ABC-12), Redmine #number or exact title."
    });
    let day = json!({"type": "string", "description": "Day as YYYY-MM-DD."});
    Value::Array(vec![
        tool(
            "tasks_list",
            "List tasks. Filters: today (planned, due or worked on today), open, blocked, project, search.",
            json!({
                "filter": {
                    "type": "string",
                    "enum": ["today", "open", "blocked", "project", "search"],
                    "description": "Default: open."
                },
                "query": {"type": "string", "description": "Project name or search text."}
            }),
            &[],
        ),
        tool(
            "task_get",
            "Get one task with notes, checklist, plan and tracked time.",
            json!({
                "id": {"type": "integer"},
                "key": {"type": "string", "description": "Jira key (ABC-12) or Redmine #number."}
            }),
            &[],
        ),
        tool(
            "day_summary",
            "Summary of a day: tracked time per task, goal, meetings, agent time and notes.",
            json!({"day": day.clone()}),
            &[],
        ),
        tool(
            "week_summary",
            "Summary of a week (Monday to Sunday): tracked time per day and per task.",
            json!({"week": {"type": "string", "description": "Any date in the week, YYYY-MM-DD."}}),
            &[],
        ),
        tool(
            "current_measurement",
            "The task being measured right now, if any.",
            json!({}),
            &[],
        ),
        tool(
            "ts_tasks",
            "Timesheet tasks from the app's cache. Does not contact Timesheet.",
            json!({}),
            &[],
        ),
        tool(
            "propose_task_create",
            &format!("Propose a new task. {PROPOSAL_NOTE}"),
            json!({
                "title": {"type": "string"},
                "project": {"type": "string"},
                "key": {"type": "string", "description": "Jira key or Redmine #number to link."},
                "day": day.clone(),
                "estimate": {"type": "integer", "description": "Estimate in minutes."}
            }),
            &["title"],
        ),
        tool(
            "propose_note_append",
            &format!("Propose adding a sentence to a task's note for a day. {PROPOSAL_NOTE}"),
            json!({
                "task": task.clone(),
                "text": {"type": "string"},
                "day": day.clone()
            }),
            &["task", "text"],
        ),
        tool(
            "propose_link_folder",
            &format!("Propose linking the working folder of the active agent session to a task. {PROPOSAL_NOTE}"),
            json!({"task": task.clone()}),
            &["task"],
        ),
        tool(
            "propose_switch",
            &format!("Propose switching the measurement to a task. {PROPOSAL_NOTE}"),
            json!({"task": task.clone()}),
            &["task"],
        ),
        tool(
            "propose_plan",
            &format!("Propose planning a task for a day. {PROPOSAL_NOTE}"),
            json!({"task": task, "day": day}),
            &["task", "day"],
        ),
    ])
}

/// Odpowiedź aplikacji na wywołanie narzędzia.
#[derive(Clone, Debug, PartialEq)]
pub struct Reply {
    pub ok: bool,
    /// Tekst wyniku albo błędu.
    pub text: String,
}

impl Reply {
    /// Ramka z aplikacji: `{"ok": true, "text": …}` albo `{"ok": false, "error": …}`.
    pub fn parse(value: &Value) -> Reply {
        let ok = value["ok"].as_bool().unwrap_or(false);
        let key = if ok { "text" } else { "error" };
        let text = value[key].as_str().unwrap_or("").to_string();
        Reply {
            ok,
            text: if !ok && text.is_empty() {
                "Todo Track odpowiedział niepoprawnie.".into()
            } else {
                text
            },
        }
    }
}

fn rpc_result(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn tool_result(text: &str, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

/// Jedna wiadomość JSON-RPC. `forward(narzędzie, argumenty)` przekazuje wywołanie
/// do aplikacji; `Err` oznacza, że aplikacja nie odpowiada. Zwraca odpowiedź albo
/// `None` dla powiadomień i odpowiedzi klienta.
pub fn handle(
    message: &Value,
    version: &str,
    forward: &mut dyn FnMut(&str, &Value) -> Result<Reply, String>,
) -> Option<Value> {
    let Some(object) = message.as_object() else {
        return Some(rpc_error(&Value::Null, -32600, "Invalid Request"));
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        // Odpowiedź klienta (np. na zapytanie serwera) albo śmieci: nic nie robimy.
        return None;
    };
    let Some(id) = object.get("id").filter(|id| !id.is_null()) else {
        // Powiadomienie (`notifications/initialized`, `…/cancelled`): bez odpowiedzi.
        return None;
    };
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let chosen = if SUPPORTED.contains(&asked) {
                asked
            } else {
                PROTOCOL_VERSION
            };
            rpc_result(
                id,
                json!({
                    "protocolVersion": chosen,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": SERVER_NAME, "version": version},
                }),
            )
        }
        "ping" => rpc_result(id, json!({})),
        "tools/list" => rpc_result(id, json!({"tools": tools()})),
        "tools/call" => {
            let Some(name) = params["name"].as_str().filter(|n| is_tool(n)) else {
                return Some(rpc_error(id, -32602, "Unknown tool"));
            };
            let arguments = match params.get("arguments") {
                None | Some(Value::Null) => json!({}),
                Some(v) if v.is_object() => v.clone(),
                Some(_) => return Some(rpc_error(id, -32602, "Arguments must be an object")),
            };
            match forward(name, &arguments) {
                Ok(reply) => rpc_result(id, tool_result(&reply.text, !reply.ok)),
                Err(_) => rpc_result(id, tool_result(NOT_RUNNING, true)),
            }
        }
        _ => rpc_error(id, -32601, "Method not found"),
    })
}

/// Jeden wiersz ze stdin: poprawny JSON (obiekt albo tablica wsadowa) albo błąd
/// parsowania. `None`, gdy nic nie trzeba odpowiadać.
pub fn respond(
    line: &str,
    version: &str,
    forward: &mut dyn FnMut(&str, &Value) -> Result<Reply, String>,
) -> Option<Value> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(line) {
        Err(_) => Some(rpc_error(&Value::Null, -32700, "Parse error")),
        Ok(Value::Array(items)) => {
            let replies: Vec<Value> = items
                .iter()
                .filter_map(|m| handle(m, version, forward))
                .collect();
            (!replies.is_empty()).then_some(Value::Array(replies))
        }
        Ok(message) => handle(&message, version, forward),
    }
}

/// Ramka do aplikacji: `{"tool": …, "arguments": {…}}`.
pub fn request(tool: &str, arguments: &Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"tool": tool, "arguments": arguments})).unwrap_or_default()
}

/// Wysyła ramkę do aplikacji i czeka na odpowiedź. Zajęty potok (inna wiadomość
/// w toku) próbujemy otworzyć jeszcze kilka razy.
pub fn forward_to(pipe: &str, body: &[u8]) -> Result<Vec<u8>, FrameError> {
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

/// Wywołanie narzędzia przez potok `pipe`.
pub fn call_over_pipe(pipe: &str, tool: &str, arguments: &Value) -> Result<Reply, String> {
    let reply = forward_to(pipe, &request(tool, arguments)).map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&reply).map_err(|e| e.to_string())?;
    Ok(Reply::parse(&value))
}
