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

// Próbki `workflow_claude_dynamic_*.jsonl` to zanonimizowane, skrócone linie z
// prawdziwego Claude Code 2.1.319 (E8_ROZPOZNANIE.md, testy A, D, E).

#[test]
fn traced_parser_reads_parent_tool_use_id_of_subagent_lines() {
    let all = traced_fixture("workflow_claude_dynamic_ok.jsonl");
    // Rodzic linii subagenta to `id` bloku tool_use narzędzia Agent.
    let read = all
        .iter()
        .find(|t| matches!(&t.item, Parsed::Tool { name, .. } if name == "Read"))
        .expect("Read subagenta");
    assert_eq!(read.parent.as_deref(), Some("toolu_agent1"));
    assert_eq!(read.tool_use_id.as_deref(), Some("toolu_read1"));
    let text = all
        .iter()
        .find(|t| matches!(&t.item, Parsed::Text(x) if x.contains("jeden")))
        .expect("tekst subagenta");
    assert_eq!(text.parent.as_deref(), Some("toolu_agent1"));
    // Linie głównego agenta nie mają rodzica.
    let agent = all
        .iter()
        .find(|t| matches!(&t.item, Parsed::Tool { name, .. } if name == "Agent"))
        .expect("Agent");
    assert_eq!(agent.parent, None);
    let done: Vec<&Traced> = all
        .iter()
        .filter(|t| matches!(t.item, Parsed::Done(_)))
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(done[0].parent, None);
}

#[test]
fn subagent_lines_have_no_init_or_result_in_real_samples() {
    for name in [
        "workflow_claude_dynamic_ok.jsonl",
        "workflow_claude_dynamic_unknown.jsonl",
        "workflow_claude_dynamic_parallel.jsonl",
    ] {
        let all = fixture(name);
        for line in all.lines() {
            let v: serde_json::Value = serde_json::from_str(line).expect("poprawny JSON");
            let is_sub = !v["agent_id"].is_null() || !v["parent_tool_use_id"].is_null();
            if is_sub {
                assert!(
                    v["type"] == "assistant" || v["type"] == "user",
                    "{name}: linia subagenta typu {}",
                    v["type"]
                );
            }
        }
        let traced = traced_fixture(name);
        assert!(
            !traced.iter().any(|t| {
                t.parent.is_some() && matches!(t.item, Parsed::Session(_) | Parsed::Done(_))
            }),
            "{name}: subagent nie emituje init ani result"
        );
        let sessions: Vec<_> = traced
            .iter()
            .filter(|t| matches!(t.item, Parsed::Session(_)))
            .collect();
        assert_eq!(sessions.len(), 1, "{name}");
        assert_eq!(sessions[0].item, Parsed::Session("sess-e8-root".into()));
    }
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
            s("Agent", "toolu_agent1", None),
            s("Read", "toolu_read1", Some("toolu_agent1")),
            s("StructuredOutput", "toolu_structuredoutput1", None),
        ]
    );
    let finished: Vec<(Option<String>, Option<String>, bool)> = all
        .iter()
        .filter_map(|t| match &t.item {
            Parsed::ToolDone { error } => Some((t.tool_use_id.clone(), t.parent.clone(), *error)),
            _ => None,
        })
        .collect();
    let f = |id: &str, p: Option<&str>| (Some(id.to_string()), p.map(str::to_string), false);
    assert_eq!(
        finished,
        vec![
            f("toolu_read1", Some("toolu_agent1")),
            f("toolu_agent1", None),
            f("toolu_structuredoutput1", None),
        ]
    );
}

#[test]
fn traced_parser_pairs_three_interleaved_subagents() {
    let all = traced_fixture("workflow_claude_dynamic_parallel.jsonl");
    let agents: Vec<_> = all
        .iter()
        .filter(|t| matches!(&t.item, Parsed::Tool { name, .. } if name == "Agent"))
        .filter_map(|t| t.tool_use_id.clone())
        .collect();
    assert_eq!(agents, ["toolu_agent1", "toolu_agent2", "toolu_agent3"]);
    for parent in &agents {
        assert!(
            all.iter().any(|t| {
                t.parent.as_deref() == Some(parent) && matches!(t.item, Parsed::Tool { .. })
            }),
            "brak narzędzia subagenta {parent}"
        );
    }
}

#[test]
fn traced_parser_ignores_new_system_lines() {
    // Prawdziwe linie z próbki `_unknown` plus `permission_denied` (test C, literał).
    let mut lines: Vec<String> = fixture("workflow_claude_dynamic_unknown.jsonl")
        .lines()
        .filter(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("JSON");
            (v["type"] == "system" && v["subtype"] != "init") || v["type"] == "rate_limit_event"
        })
        .map(str::to_owned)
        .collect();
    let subtypes: Vec<String> = lines
        .iter()
        .filter_map(|l| {
            serde_json::from_str::<serde_json::Value>(l).ok()?["subtype"]
                .as_str()
                .map(str::to_owned)
        })
        .collect();
    for wanted in [
        "task_started",
        "task_progress",
        "task_notification",
        "task_updated",
        "task_summary",
        "post_turn_summary",
        "thinking_tokens",
    ] {
        assert!(subtypes.iter().any(|s| s == wanted), "brak {wanted}");
    }
    lines.push(r#"{"type":"system","subtype":"permission_denied","tool_name":"Workflow"}"#.into());
    lines.push(r#"{"type":"system","subtype":"background_tasks_changed","tasks":[]}"#.into());
    for l in &lines {
        assert!(parse_claude_line_traced(l).is_empty(), "{l}");
        assert!(parse_claude_line_raw(l).is_empty(), "{l}");
    }
}

#[test]
fn agent_tool_result_with_text_blocks_is_a_tool_done() {
    let all = traced_fixture("workflow_claude_dynamic_ok.jsonl");
    let done = all
        .iter()
        .find(|t| {
            t.tool_use_id.as_deref() == Some("toolu_agent1")
                && matches!(t.item, Parsed::ToolDone { .. })
        })
        .expect("tool_result Agenta");
    assert_eq!(done.item, Parsed::ToolDone { error: false });
    assert_eq!(done.parent, None);
}

#[test]
fn traced_parser_ignores_thinking_blocks() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"sekret"},{"type":"text","text":"widoczny"}]},"parent_tool_use_id":"toolu_A"}"#;
    let parsed = parse_claude_line_traced(line);
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].item, Parsed::Text("widoczny".into()));
    assert_eq!(parsed[0].parent.as_deref(), Some("toolu_A"));
    // Prawdziwy kształt z `--forward-subagent-text`: pusty thinking z samym podpisem.
    let empty = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"","signature":""}]},"parent_tool_use_id":"toolu_A"}"#;
    assert!(parse_claude_line_traced(empty).is_empty());
}

// Przypadki skrajne: SYNTETYCZNE literały (nie pochodzą z prawdziwego CLI).
#[test]
fn traced_parser_survives_unknown_subagent_shapes() {
    let lines = [
        r#"{"type":"assistant","message":{"content":"tekst jako string"},"parent_tool_use_id":"toolu_X"}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"liczbowy rodzic"}]},"parent_tool_use_id":42}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta"},"parent_tool_use_id":"toolu_X"}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"server_tool_use","id":"srv_1","name":"web_search","input":{}}]},"parent_tool_use_id":"toolu_X"}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"b.txt"}}]},"parent_tool_use_id":"toolu_X"}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"bez id"}]},"parent_tool_use_id":"toolu_X"}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"uszkodzona"#,
        r#"{"type":"result","subtype":"success","is_error":false,"result":"ok","parent_tool_use_id":null}"#,
    ];
    let all: Vec<Traced> = lines
        .iter()
        .flat_map(|l| parse_claude_line_traced(l))
        .collect();
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
        "workflow_claude_dynamic_parallel.jsonl",
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
