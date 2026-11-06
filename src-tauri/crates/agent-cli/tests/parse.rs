use std::path::PathBuf;

use agent_cli::{
    classify, parse_claude_line_raw, parse_claude_line_traced, parse_codex_line_raw, parse_version,
    Failure, Parsed, Traced,
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

// ---------------------------------------------------------------------------
// Parser z rodzicem i identyfikatorami narzędzi (E8.2)

fn traced_fixture(name: &str) -> Vec<Traced> {
    fixture(name)
        .lines()
        .flat_map(parse_claude_line_traced)
        .collect()
}

#[test]
fn traced_parser_reads_parent_tool_use_id_of_subagent_lines() {
    let all = traced_fixture("workflow_claude_dynamic_ok.jsonl");
    let find = |item: Parsed| all.iter().find(|t| t.item == item).expect("zdarzenie");
    assert_eq!(find(Parsed::Session("sess-dyn-1".into())).parent, None);
    assert_eq!(
        find(Parsed::Session("sess-sub-1".into())).parent.as_deref(),
        Some("toolu_A")
    );
    assert_eq!(
        find(Parsed::Done("subagent gotowy".into()))
            .parent
            .as_deref(),
        Some("toolu_A")
    );
    assert_eq!(find(Parsed::Done("koniec".into())).parent, None);
    assert_eq!(
        find(Parsed::Text("Szukam w plikach.".into()))
            .parent
            .as_deref(),
        Some("toolu_B")
    );
}

#[test]
fn traced_parser_keeps_tool_use_ids_for_pairing() {
    let all = traced_fixture("workflow_claude_dynamic_ok.jsonl");
    let started: Vec<(String, Option<String>, Option<String>)> = all
        .iter()
        .filter_map(|t| match &t.item {
            Parsed::Tool { name, .. } => {
                Some((name.clone(), t.tool_use_id.clone(), t.parent.clone()))
            }
            _ => None,
        })
        .collect();
    let s = |n: &str, id: &str, p: Option<&str>| {
        (n.to_string(), Some(id.to_string()), p.map(str::to_string))
    };
    assert_eq!(
        started,
        vec![
            s("Agent", "toolu_A", None),
            s("Task", "toolu_B", None),
            s("Read", "toolu_A1", Some("toolu_A")),
            s("Grep", "toolu_B1", Some("toolu_B")),
        ]
    );
    let finished: Vec<(Option<String>, bool)> = all
        .iter()
        .filter_map(|t| match &t.item {
            Parsed::ToolDone { error } => Some((t.tool_use_id.clone(), *error)),
            _ => None,
        })
        .collect();
    let f = |id: &str, e: bool| (Some(id.to_string()), e);
    assert_eq!(
        finished,
        vec![
            f("toolu_A1", false),
            f("toolu_B1", true),
            f("toolu_A", false),
            f("toolu_B", false),
        ]
    );
}

#[test]
fn traced_parser_ignores_thinking_blocks() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"sekret"},{"type":"text","text":"widoczny"}]},"parent_tool_use_id":"toolu_A"}"#;
    let parsed = parse_claude_line_traced(line);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].item, Parsed::Text("widoczny".into()));
    assert_eq!(parsed[0].parent.as_deref(), Some("toolu_A"));
}

#[test]
fn traced_parser_survives_unknown_subagent_shapes() {
    let all = traced_fixture("workflow_claude_dynamic_unknown.jsonl");
    let numeric = all
        .iter()
        .find(|t| t.item == Parsed::Text("liczbowy rodzic".into()))
        .expect("tekst z liczbowym rodzicem");
    assert_eq!(numeric.parent.as_deref(), Some("42"));
    let tool = all
        .iter()
        .find(|t| matches!(t.item, Parsed::Tool { .. }))
        .expect("tool_use bez id");
    assert_eq!(tool.tool_use_id, None);
    let done = all
        .iter()
        .find(|t| matches!(t.item, Parsed::ToolDone { .. }))
        .expect("tool_result bez id");
    assert_eq!(done.tool_use_id, None);
    let last = all.last().expect("ostatnie zdarzenie");
    assert_eq!(last.item, Parsed::Done("ok".into()));
    assert_eq!(last.parent, None);
}

#[test]
fn raw_parser_output_is_unchanged_for_subagent_lines() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"x"},{"type":"tool_use","id":"toolu_A1","name":"Read","input":{"file_path":"a.txt"}}]},"parent_tool_use_id":"toolu_A"}"#;
    assert_eq!(
        parse_claude_line_raw(line),
        vec![Parsed::Tool {
            name: "Read".into(),
            input: serde_json::json!({"file_path": "a.txt"})
        }]
    );
    let result = r#"{"type":"result","subtype":"success","is_error":false,"result":"subagent gotowy","parent_tool_use_id":"toolu_A"}"#;
    assert_eq!(
        parse_claude_line_raw(result),
        vec![Parsed::Done("subagent gotowy".into())]
    );
    let user = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_B1","is_error":true}]},"parent_tool_use_id":"toolu_B"}"#;
    assert_eq!(
        parse_claude_line_raw(user),
        vec![Parsed::ToolDone { error: true }]
    );
    for name in [
        "workflow_claude_dynamic_ok.jsonl",
        "workflow_claude_dynamic_unknown.jsonl",
        "ai_chat_claude.jsonl",
        "workflow_claude_review_ok.jsonl",
    ] {
        for l in fixture(name).lines() {
            let traced: Vec<Parsed> = parse_claude_line_traced(l)
                .into_iter()
                .map(|t| t.item)
                .collect();
            assert_eq!(traced, parse_claude_line_raw(l), "{name}: {l}");
        }
    }
}
