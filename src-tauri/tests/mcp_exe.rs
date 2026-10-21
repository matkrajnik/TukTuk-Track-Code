//! `todo-track-mcp.exe` jako proces (A2): serwer MCP na stdio bez działającej
//! aplikacji. Uruchamia wyłącznie własny program z tej paczki (nie claude ani
//! codex); nazwa użytkownika w środowisku jest unikalna, więc potok aplikacji
//! (także prawdziwej, jeśli działa) nie istnieje.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn stdio_server_answers_and_reports_that_the_app_is_not_running() {
    let user = format!("mcp-exe-test-{}", std::process::id());
    let mut child = Command::new(env!("CARGO_BIN_EXE_todo-track-mcp"))
        .env("USERNAME", user)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("nie udało się uruchomić todo-track-mcp");
    let mut stdin = child.stdin.take().unwrap();
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "tasks_list", "arguments": {}}}),
    ];
    for request in &requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let replies: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map(|line| serde_json::from_str(&line.unwrap()).unwrap())
        .collect();
    assert!(child.wait().unwrap().success());
    // Powiadomienie nie dostaje odpowiedzi.
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "todo-track");
    assert_eq!(
        replies[0]["result"]["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(replies[1]["result"]["tools"].as_array().unwrap().len(), 11);
    assert_eq!(replies[2]["id"], 3);
    assert_eq!(replies[2]["result"]["isError"], true);
    assert_eq!(
        replies[2]["result"]["content"][0]["text"],
        "Todo Track nie działa. Uruchom aplikację i spróbuj ponownie."
    );
}
