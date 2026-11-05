use std::path::PathBuf;

use agent_cli::{
    classify, parse_claude_line_raw, parse_codex_line_raw, parse_version, Failure, Parsed,
};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn raw_parser_keeps_the_full_tool_name() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__x__y","input":{"a":1}}]}}"#;
    let parsed = parse_claude_line_raw(line);
    assert_eq!(
        parsed,
        vec![Parsed::Tool {
            name: "mcp__x__y".into(),
            input: serde_json::json!({"a": 1})
        }]
    );
    let codex = r#"{"type":"item.started","item":{"type":"mcp_tool_call","tool":"srv__tool"}}"#;
    assert!(matches!(
        parse_codex_line_raw(codex).as_slice(),
        [Parsed::Tool { name, .. }] if name == "srv__tool"
    ));
}

#[test]
fn claude_init_line_gives_the_session() {
    let line = r#"{"type":"system","subtype":"init","session_id":"abc-123"}"#;
    assert_eq!(
        parse_claude_line_raw(line),
        vec![Parsed::Session("abc-123".into())]
    );
}

#[test]
fn codex_thread_started_gives_the_session() {
    let line = r#"{"type":"thread.started","thread_id":"t-9"}"#;
    assert_eq!(
        parse_codex_line_raw(line),
        vec![Parsed::Session("t-9".into())]
    );
}

#[test]
fn unknown_and_broken_lines_give_nothing() {
    assert!(parse_claude_line_raw(r#"{"type":"zdarzenie_z_przyszlosci"}"#).is_empty());
    assert!(parse_codex_line_raw(r#"{"type":"nowe"}"#).is_empty());
    assert!(parse_claude_line_raw("{uszkodzona").is_empty());
    assert!(parse_codex_line_raw("").is_empty());
}

#[test]
fn classify_recognises_login_and_limit() {
    assert_eq!(classify("Please /login to continue"), Failure::NotLoggedIn);
    assert_eq!(classify("You hit your limit"), Failure::Limit);
    assert_eq!(classify("coś innego"), Failure::Other);
}

#[test]
fn recorded_chat_streams_parse_through_the_raw_parser() {
    let claude = fixture("ai_chat_claude.jsonl");
    let parsed: Vec<Parsed> = claude.lines().flat_map(parse_claude_line_raw).collect();
    assert!(parsed.iter().any(|p| matches!(p, Parsed::Done(_))));
    let codex = fixture("ai_chat_codex.jsonl");
    let parsed: Vec<Parsed> = codex.lines().flat_map(parse_codex_line_raw).collect();
    assert!(parsed.iter().any(|p| matches!(p, Parsed::Done(_))));
}

#[test]
fn version_is_taken_from_the_first_number() {
    assert_eq!(parse_version("2.1.304 (Claude Code)"), "2.1.304");
    assert_eq!(parse_version("codex-cli 0.162.1\n"), "0.162.1");
}
