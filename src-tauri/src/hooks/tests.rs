use super::*;
use crate::hooks_config::{
    hook_command, install, registration, uninstall, CLAUDE_EVENTS, CODEX_EVENTS,
};
use serde_json::json;

const CLAUDE: &str = include_str!("../../tests/fixtures/hooks_claude.json");
const CODEX: &str = include_str!("../../tests/fixtures/hooks_codex.json");
const T0: i64 = 1_000_000;

/// Zdarzenia z fixture: (offset ms, zdarzenie po drodze hook → koperta → parser).
fn fixture(agent: &str, text: &str) -> Vec<(i64, HookEvent)> {
    serde_json::from_str::<Vec<Value>>(text)
        .unwrap()
        .into_iter()
        .map(|e| {
            let raw = serde_json::to_vec(&e["payload"]).unwrap();
            let body = hook_wire::envelope(agent, &raw).expect("koperta");
            (e["at"].as_i64().unwrap(), parse(&body).expect("zdarzenie"))
        })
        .collect()
}

fn ev(agent: &str, event: &str) -> HookEvent {
    HookEvent {
        agent: agent.into(),
        session_id: "s1".into(),
        cwd: "C:\\Projekty\\app".into(),
        event: event.into(),
        ..HookEvent::default()
    }
}

fn sub(event: &str, id: &str) -> HookEvent {
    HookEvent {
        agent_id: Some(id.into()),
        ..ev("claude", event)
    }
}

fn phase(s: &Sessions) -> Phase {
    s.session("claude", "s1").unwrap().phase
}

#[test]
fn parses_claude_events_from_fixture() {
    let events = fixture("claude", CLAUDE);
    assert_eq!(events.len(), 10);
    let (_, start) = &events[0];
    assert_eq!(start.agent, "claude");
    assert_eq!(start.session_id, "c1111111-aaaa");
    assert_eq!(start.cwd, "C:\\Projekty\\app");
    assert_eq!(start.event, "SessionStart");
    assert_eq!(start.agent_id, None);
    assert_eq!(
        events[1].1.prompt.as_deref(),
        Some("Napraw eksport do Excela w zadaniu WMS-12 i dopisz test")
    );
    // Zdarzenie subagenta niesie jego identyfikator.
    assert_eq!(events[4].1.agent_id.as_deref(), Some("agent-7"));
    assert_eq!(
        events[5].1.notification_type.as_deref(),
        Some("permission_prompt")
    );
    // Narzędzia, odpowiedzi i transkrypt nie wychodzą poza program hooka.
    let raw = serde_json::to_vec(&json!({
        "session_id": "x", "cwd": "C:\\a", "hook_event_name": "PreToolUse",
        "tool_input": { "command": "tajne" }, "transcript_path": "C:\\t.jsonl"
    }))
    .unwrap();
    let body = String::from_utf8(hook_wire::envelope("claude", &raw).unwrap()).unwrap();
    assert!(!body.contains("tajne") && !body.contains("transcript"));
}

#[test]
fn parses_codex_events_from_fixture() {
    let events = fixture("codex", CODEX);
    assert_eq!(events.len(), 6);
    assert!(events.iter().all(|(_, e)| e.agent == "codex"));
    assert_eq!(events[3].1.event, "PermissionRequest");
    assert_eq!(events[5].1.event, "Interrupt");
    assert_eq!(
        events[1].1.prompt.as_deref(),
        Some("Dodaj endpoint /health")
    );
}

#[test]
fn parser_rejects_bad_envelopes() {
    assert!(parse(b"nie json").is_none());
    assert!(
        parse(br#"{"agent":"orca","event":{"session_id":"a","hook_event_name":"Stop"}}"#).is_none()
    );
    assert!(parse(br#"{"agent":"claude","event":{"hook_event_name":"Stop"}}"#).is_none());
    assert!(parse(br#"{"agent":"claude","event":{"session_id":"a"}}"#).is_none());
    assert!(hook_wire::envelope("orca", b"{}").is_none());
    assert!(hook_wire::envelope("claude", b"[1]").is_none());
}

#[test]
fn prompt_is_clipped_to_500_chars_and_only_for_user_prompt() {
    let long = "ż".repeat(900);
    let raw = |event: &str| {
        serde_json::to_vec(&json!({
            "session_id": "s", "hook_event_name": event, "prompt": long
        }))
        .unwrap()
    };
    let body = hook_wire::envelope("claude", &raw("UserPromptSubmit")).unwrap();
    assert_eq!(parse(&body).unwrap().prompt.unwrap().chars().count(), 500);
    let body = hook_wire::envelope("claude", &raw("PreToolUse")).unwrap();
    assert_eq!(parse(&body).unwrap().prompt, None);
}

#[test]
fn hook_drops_events_from_ai_processes() {
    assert!(hook_wire::suppressed(Some("1")));
    assert!(hook_wire::suppressed(Some(" 1 ")));
    assert!(!hook_wire::suppressed(None));
    assert!(!hook_wire::suppressed(Some("0")));
    assert!(!hook_wire::suppressed(Some("")));
}

#[test]
fn claude_session_from_fixture_goes_through_work_wait_and_end() {
    let mut s = Sessions::default();
    let mut phases = Vec::new();
    for (at, e) in fixture("claude", CLAUDE) {
        s.apply(&e, T0 + at);
        phases.push(s.session("claude", "c1111111-aaaa").map(|x| x.phase));
    }
    use Phase::*;
    assert_eq!(
        phases,
        vec![
            Some(Done),    // SessionStart
            Some(Working), // UserPromptSubmit
            Some(Working), // PreToolUse
            Some(Working), // SubagentStart
            Some(Working), // PreToolUse subagenta
            Some(Waiting), // Notification permission_prompt
            Some(Working), // PostToolUse po zgodzie
            Some(Working), // SubagentStop, ale sesja nadal pracuje do Stop
            Some(Done),    // Stop
            None,          // SessionEnd
        ]
    );
}

#[test]
fn codex_session_waits_for_permission_and_ends_on_interrupt() {
    let mut s = Sessions::default();
    let mut phases = Vec::new();
    for (at, e) in fixture("codex", CODEX) {
        s.apply(&e, T0 + at);
        phases.push(s.session("codex", "019aaaaa-0000").map(|x| x.phase));
    }
    use Phase::*;
    assert_eq!(
        phases,
        vec![
            Some(Done),
            Some(Working),
            Some(Working),
            Some(Waiting),
            Some(Working),
            Some(Done)
        ]
    );
}

#[test]
fn only_working_sessions_are_listed_and_waiting_is_not_work() {
    let mut s = Sessions::default();
    assert!(s.apply(&ev("claude", "UserPromptSubmit"), T0));
    let list = s.working();
    assert_eq!(list.len(), 1);
    assert_eq!(
        (list[0].kind.as_str(), list[0].name.as_str(), list[0].since),
        ("claude", "app", T0)
    );
    assert_eq!(list[0].cwd, "C:\\Projekty\\app");
    // Zwykłe zdarzenie w trakcie pracy nie zmienia listy (pętla się nie budzi).
    assert!(!s.apply(&ev("claude", "PreToolUse"), T0 + 1_000));
    // Czekanie na zgodę zdejmuje sesję z listy.
    assert!(s.apply(&ev("claude", "PermissionRequest"), T0 + 2_000));
    assert!(s.working().is_empty());
    // Po zgodzie praca liczy się od nowa.
    assert!(s.apply(&ev("claude", "PostToolUse"), T0 + 60_000));
    assert_eq!(s.working()[0].since, T0 + 60_000);
    assert!(s.apply(&ev("claude", "Stop"), T0 + 61_000));
    assert!(s.working().is_empty());
}

#[test]
fn notification_types_decide_whether_the_agent_waits() {
    let notify = |t: &str| HookEvent {
        notification_type: Some(t.into()),
        ..ev("claude", "Notification")
    };
    for waits in ["permission_prompt", "agent_needs_input"] {
        let mut s = Sessions::default();
        s.apply(&ev("claude", "UserPromptSubmit"), T0);
        s.apply(&notify(waits), T0 + 1);
        assert_eq!(phase(&s), Phase::Waiting, "{waits}");
    }
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&notify("idle_prompt"), T0 + 1);
    assert_eq!(phase(&s), Phase::Waiting);
    // Inne powiadomienia nie zmieniają fazy.
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&notify("auth_success"), T0 + 1);
    assert_eq!(phase(&s), Phase::Working);
    // Przypomnienie po zakończonej turze nie robi z niej czekania ani pracy.
    s.apply(&ev("claude", "Stop"), T0 + 2);
    s.apply(&notify("idle_prompt"), T0 + 3);
    assert_eq!(phase(&s), Phase::Done);
}

#[test]
fn subagents_count_to_the_parent_session() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&sub("SubagentStart", "a1"), T0 + 1);
    s.apply(&sub("SubagentStart", "a2"), T0 + 2);
    // Jedna sesja nadrzędna, bez osobnych wpisów subagentów.
    assert_eq!(s.len(), 1);
    assert_eq!(s.working().len(), 1);
    // Stop głównego agenta, gdy subagenci jeszcze pracują, nie kończy sesji.
    s.apply(&ev("claude", "Stop"), T0 + 3);
    assert_eq!(phase(&s), Phase::Working);
    // Praca subagenta w tle (workflow) podtrzymuje sesję także po Stop.
    s.apply(&sub("PreToolUse", "a1"), T0 + 20 * 60_000);
    s.expire(T0 + 20 * 60_000 + 1_000);
    assert_eq!(phase(&s), Phase::Working);
    s.apply(&sub("SubagentStop", "a1"), T0 + 20 * 60_000 + 2_000);
    assert_eq!(phase(&s), Phase::Working);
    s.apply(&sub("SubagentStop", "a2"), T0 + 20 * 60_000 + 3_000);
    // Dopiero koniec ostatniego subagenta i Stop kończą sesję.
    s.apply(&ev("claude", "Stop"), T0 + 20 * 60_000 + 4_000);
    assert_eq!(phase(&s), Phase::Done);
    // Interrupt kończy od razu, razem z subagentami.
    s.apply(&sub("SubagentStart", "a3"), T0 + 21 * 60_000);
    s.apply(&ev("claude", "Interrupt"), T0 + 21 * 60_000 + 1);
    assert_eq!(phase(&s), Phase::Done);
    assert!(s.session("claude", "s1").unwrap().subagents.is_empty());
}

#[test]
fn silence_ends_working_and_waiting_sessions_then_forgets_them() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    let mut waiting = ev("codex", "PermissionRequest");
    waiting.session_id = "w".into();
    s.apply(&waiting, T0);
    assert!(!s.expire(T0 + SILENCE));
    assert_eq!(phase(&s), Phase::Working);
    // Zdarzenie przedłuża sesję.
    // (PostToolUse: bez niezakończonego narzędzia obowiązuje zwykłe SILENCE.)
    s.apply(&ev("claude", "PostToolUse"), T0 + SILENCE - 1);
    assert!(!s.expire(T0 + 2 * SILENCE - 2));
    assert_eq!(phase(&s), Phase::Working);
    // Brak zdarzeń przez SILENCE: sesja kończy się, lista pustnieje.
    assert!(s.expire(T0 + 2 * SILENCE + 1));
    assert_eq!(phase(&s), Phase::Done);
    assert!(s.working().is_empty());
    assert_eq!(s.session("codex", "w").unwrap().phase, Phase::Done);
    // Zakończone sesje są po długim czasie zapominane.
    s.expire(T0 + 2 * SILENCE + FORGET + 10);
    assert_eq!(s.len(), 0);
}

#[test]
fn session_end_removes_the_session_and_keeps_first_prompt_until_then() {
    // Bez `keep_prompts` (do A4: `ai_assign_suggest`) prompt nie zostaje w pamięci.
    let mut off = Sessions::default();
    let mut first = ev("claude", "UserPromptSubmit");
    first.prompt = Some("pierwszy".into());
    off.apply(&first, T0);
    assert_eq!(off.session("claude", "s1").unwrap().first_prompt, None);
    let mut s = Sessions {
        keep_prompts: true,
        ..Sessions::default()
    };
    let mut first = ev("claude", "UserPromptSubmit");
    first.prompt = Some("pierwszy".into());
    s.apply(&first, T0);
    let mut second = ev("claude", "UserPromptSubmit");
    second.prompt = Some("drugi".into());
    s.apply(&second, T0 + 1);
    assert_eq!(
        s.session("claude", "s1").unwrap().first_prompt.as_deref(),
        Some("pierwszy")
    );
    assert!(s.apply(&ev("claude", "SessionEnd"), T0 + 2));
    assert!(s.session("claude", "s1").is_none());
    assert!(s.working().is_empty());
}

#[test]
fn sessions_of_two_agents_with_one_id_stay_separate() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&ev("codex", "UserPromptSubmit"), T0 + 1);
    s.apply(&ev("codex", "Stop"), T0 + 2);
    assert_eq!(s.working().len(), 1);
    assert_eq!(s.working()[0].kind, "claude");
}

#[test]
fn session_count_is_capped_and_recent_events_are_kept_short() {
    let mut s = Sessions::default();
    for i in 0..(MAX_SESSIONS + 20) {
        let mut e = ev("claude", "UserPromptSubmit");
        e.session_id = format!("id-{i}");
        s.apply(&e, T0 + i as i64);
    }
    assert!(s.len() <= MAX_SESSIONS);
    let recent = s.recent();
    assert_eq!(recent.len(), RECENT);
    assert_eq!(recent[0].session, "id-147");
    assert_eq!(recent[0].event, "UserPromptSubmit");
    assert_eq!(recent[0].detail, "pracuje");
}

// --- scalanie JSON ---

fn orca_settings() -> Value {
    json!({
        "theme": "dark",
        "hooks": {
            "PreToolUse": [
                { "matcher": "Bash", "hooks": [{ "type": "command", "command": "node C:/orca/hook.js pre" }] }
            ],
            "Stop": [
                { "hooks": [{ "type": "command", "command": "node C:/orca/hook.js stop" }] }
            ],
            "Elicitation": [
                { "hooks": [{ "type": "command", "command": "node C:/orca/hook.js ask" }] }
            ]
        },
        "model": "opus"
    })
}

fn commands(root: &Value, event: &str) -> Vec<String> {
    root["hooks"][event]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
        .filter_map(|h| h["command"].as_str().map(str::to_string))
        .collect()
}

#[test]
fn merge_adds_own_hooks_next_to_foreign_ones() {
    let exe = Path::new("C:\\Program Files\\TukTukTrack\\tuktuk-track-hook.exe");
    let command = hook_command(exe, "claude");
    assert_eq!(
        command,
        "\"C:/Program Files/TukTukTrack/tuktuk-track-hook.exe\" claude"
    );
    let mut root = orca_settings();
    assert!(install(&mut root, &CLAUDE_EVENTS, &command).unwrap());
    for event in CLAUDE_EVENTS {
        assert!(commands(&root, event).contains(&command), "{event}");
    }
    // Wpisy Orki i inne pola zostają.
    assert_eq!(
        commands(&root, "PreToolUse"),
        vec!["node C:/orca/hook.js pre".to_string(), command.clone()]
    );
    assert_eq!(
        commands(&root, "Elicitation"),
        vec!["node C:/orca/hook.js ask"]
    );
    assert_eq!(root["theme"], "dark");
    assert_eq!(root["model"], "opus");
    assert_eq!(
        registration(&root, &CLAUDE_EVENTS, &command),
        Registration::Current
    );
    // Drugie wywołanie nic nie zmienia.
    assert!(!install(&mut root, &CLAUDE_EVENTS, &command).unwrap());
}

#[test]
fn merge_updates_the_exe_path_without_duplicates() {
    let old = hook_command(Path::new("C:\\Stare\\tuktuk-track-hook.exe"), "claude");
    let new = hook_command(Path::new("C:\\Nowe\\tuktuk-track-hook.exe"), "claude");
    let mut root = orca_settings();
    install(&mut root, &CLAUDE_EVENTS, &old).unwrap();
    assert_eq!(
        registration(&root, &CLAUDE_EVENTS, &new),
        Registration::Stale
    );
    assert!(install(&mut root, &CLAUDE_EVENTS, &new).unwrap());
    for event in CLAUDE_EVENTS {
        let own: Vec<_> = commands(&root, event)
            .into_iter()
            .filter(|c| hooks_config::is_own(c))
            .collect();
        assert_eq!(own, vec![new.clone()], "{event}");
    }
    assert_eq!(commands(&root, "Stop")[0], "node C:/orca/hook.js stop");
    assert_eq!(
        registration(&root, &CLAUDE_EVENTS, &new),
        Registration::Current
    );
}

#[test]
fn uninstall_removes_only_own_entries() {
    let command = hook_command(Path::new("C:\\App\\tuktuk-track-hook.exe"), "claude");
    let original = orca_settings();
    let mut root = original.clone();
    install(&mut root, &CLAUDE_EVENTS, &command).unwrap();
    assert!(uninstall(&mut root).unwrap());
    // Po usunięciu zostaje dokładnie to, co było przed dopisaniem.
    assert_eq!(root, original);
    assert_eq!(
        registration(&root, &CLAUDE_EVENTS, &command),
        Registration::Missing
    );
    assert!(!uninstall(&mut root).unwrap());
    // Plik złożony tylko z naszych wpisów traci pole `hooks`.
    let mut only = json!({});
    install(&mut only, &CODEX_EVENTS, "x/tuktuk-track-hook.exe codex").unwrap();
    assert!(uninstall(&mut only).unwrap());
    assert_eq!(only, json!({}));
    // Własny wpis w tej samej grupie co obcy: obcy zostaje.
    let mut mixed = json!({"hooks": {"Stop": [{"hooks": [
        {"type": "command", "command": "orca stop"},
        {"type": "command", "command": "C:/a/tuktuk-track-hook.exe claude"}
    ]}]}});
    assert!(uninstall(&mut mixed).unwrap());
    assert_eq!(commands(&mixed, "Stop"), vec!["orca stop"]);
}

#[test]
fn merge_keeps_key_order_and_rejects_odd_shapes() {
    let text = r#"{"zeta":1,"hooks":{"Stop":[{"matcher":"","hooks":[{"type":"command","command":"orca stop","timeout":3}]}]},"alfa":{"z":1,"a":2}}"#;
    let mut doc = hooks_config::Doc::parse(text).unwrap();
    install(
        &mut doc.value,
        &CODEX_EVENTS,
        "a/tuktuk-track-hook.exe codex",
    )
    .unwrap();
    let out = doc.to_text().unwrap();
    let at = |needle: &str| {
        out.find(needle)
            .unwrap_or_else(|| panic!("{needle} w {out}"))
    };
    // Klucze z pliku zostają w swojej kolejności (także zagnieżdżone), nowe na końcu.
    assert!(at("\"zeta\"") < at("\"hooks\"") && at("\"hooks\"") < at("\"alfa\""));
    assert!(at("\"z\"") < at("\"a\""));
    assert!(
        at("\"matcher\"") < at("\"type\"") && at("\"type\"") < at("\"command\": \"orca stop\"")
    );
    assert!(at("\"command\": \"orca stop\"") < at("\"timeout\": 3"));
    assert!(at("\"Stop\"") < at("\"SessionStart\""));
    // Wpisanie i usunięcie własnych wpisów oddaje plik w pierwotnym układzie.
    let mut back = hooks_config::Doc::parse(&out).unwrap();
    assert!(uninstall(&mut back.value).unwrap());
    assert_eq!(
        back.to_text().unwrap(),
        hooks_config::Doc::parse(text).unwrap().to_text().unwrap()
    );
    let mut bad = json!({"hooks": []});
    assert!(install(&mut bad, &CODEX_EVENTS, "x").is_err());
    let mut bad = json!({"hooks": {"Stop": {}}});
    assert!(install(&mut bad, &CODEX_EVENTS, "x").is_err());
    let mut bad = json!([]);
    assert!(install(&mut bad, &CODEX_EVENTS, "x").is_err());
}

/// Katalog tymczasowy testu (czyszczony na początku).
fn temp(name: &str) -> PathBuf {
    let root =
        std::env::temp_dir().join(format!("tuktuk-track-hooks-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn dirs(root: &Path) -> Dirs {
    Dirs {
        claude: Some(root.join(".claude")),
        codex: Some(root.join(".codex")),
    }
}

#[test]
fn files_are_registered_updated_and_cleaned_in_a_temp_home() {
    let root = temp("files");
    let dirs = dirs(&root);
    std::fs::create_dir_all(dirs.claude.as_ref().unwrap()).unwrap();
    std::fs::create_dir_all(dirs.codex.as_ref().unwrap()).unwrap();
    let claude_file = dirs.claude_file().unwrap();
    let codex_file = dirs.codex_file().unwrap();
    std::fs::write(
        &claude_file,
        serde_json::to_string_pretty(&orca_settings()).unwrap(),
    )
    .unwrap();

    let exe = root.join("app").join("tuktuk-track-hook.exe");
    let (claude, codex) = hooks_config::sync(&dirs, Some(&exe));
    assert!(claude.found && claude.changed && claude.error.is_empty());
    assert!(codex.found && codex.changed);
    // Codex: plik powstał od zera.
    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(&codex_file).unwrap()).unwrap();
    assert!(commands(&hooks, "Interrupt")[0].ends_with("tuktuk-track-hook.exe codex"));
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_file).unwrap()).unwrap();
    assert!(commands(&settings, "Notification")[0].ends_with("tuktuk-track-hook.exe claude"));
    assert_eq!(commands(&settings, "Elicitation").len(), 1);

    // Status: zarejestrowane z aktualną ścieżką, potem stare po zmianie exe.
    let status = status_for(&dirs, Some(&exe), true, String::new(), Vec::new());
    assert_eq!(status.claude.registration, "current");
    assert_eq!(status.codex.registration, "current");
    let moved = root.join("nowy").join("tuktuk-track-hook.exe");
    let status = status_for(&dirs, Some(&moved), true, String::new(), Vec::new());
    assert_eq!(status.claude.registration, "stale");
    let (claude, _) = hooks_config::sync(&dirs, Some(&moved));
    assert!(claude.changed);
    assert_eq!(
        status_for(&dirs, Some(&moved), true, String::new(), Vec::new())
            .claude
            .registration,
        "current"
    );
    // Powtórka nic nie zapisuje.
    let (claude, codex) = hooks_config::sync(&dirs, Some(&moved));
    assert!(!claude.changed && !codex.changed);

    // Wyłączenie usuwa własne wpisy, a pliki Orki zostają.
    let (claude, codex) = hooks_config::sync(&dirs, None);
    assert!(claude.changed && codex.changed);
    let settings: Value =
        serde_json::from_str(&std::fs::read_to_string(&claude_file).unwrap()).unwrap();
    assert_eq!(settings, orca_settings());
    let status = status_for(&dirs, Some(&moved), false, String::new(), Vec::new());
    assert_eq!(status.claude.registration, "missing");
    assert_eq!(status.codex_trust, "none");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_agent_folder_is_skipped_and_broken_json_is_left_alone() {
    let root = temp("skip");
    let dirs = dirs(&root);
    let exe = root.join("tuktuk-track-hook.exe");
    // Żaden agent nie jest zainstalowany: nic nie powstaje.
    let (claude, codex) = hooks_config::sync(&dirs, Some(&exe));
    assert!(!claude.found && !codex.found);
    assert!(!root.join(".claude").exists() && !root.join(".codex").exists());
    // Plik z komentarzami (nie JSON): bez zmian, z komunikatem.
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    let text = "{\n  // moje ustawienia\n  \"model\": \"opus\"\n}\n";
    std::fs::write(root.join(".claude").join("settings.json"), text).unwrap();
    let (claude, _) = hooks_config::sync(&dirs, Some(&exe));
    assert!(claude.found && !claude.changed);
    assert!(claude.error.contains("nie jest poprawnym JSON"));
    assert_eq!(
        std::fs::read_to_string(root.join(".claude").join("settings.json")).unwrap(),
        text
    );
    // Wyłączenie z uszkodzonym plikiem też go nie rusza.
    let (claude, _) = hooks_config::sync(&dirs, None);
    assert!(!claude.error.is_empty());
    assert_eq!(
        std::fs::read_to_string(root.join(".claude").join("settings.json")).unwrap(),
        text
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn missing_exe_is_an_error_when_enabling_but_not_when_disabling() {
    let root = temp("noexe");
    let dirs = dirs(&root);
    std::fs::create_dir_all(root.join(".claude")).unwrap();
    assert!(apply_dirs(&dirs, None, true)
        .unwrap_err()
        .contains("tuktuk-track-hook.exe"));
    assert!(apply_dirs(&dirs, None, false).is_ok());
    assert!(!root.join(".claude").join("settings.json").exists());
    let _ = std::fs::remove_dir_all(&root);
}

// --- hooks.state z config.toml ---

const CONFIG_TABLES: &str = r#"
model = "gpt-5"

[hooks.state."C:\\Users\\jan\\.codex\\hooks.json:pre_tool_use:0:0"]
trusted_hash = "sha256:aaa"

[hooks.state."C:\\Users\\jan\\.codex\\hooks.json:stop:0:0"]
enabled = true

[projects."C:\\Projekty"]
trust_level = "trusted"
"#;

#[test]
fn reads_hook_state_tables_and_inline_entries() {
    let entries = parse_hook_state(CONFIG_TABLES);
    assert_eq!(
        entries,
        vec![
            (
                "C:\\Users\\jan\\.codex\\hooks.json:pre_tool_use:0:0".to_string(),
                true
            ),
            (
                "C:\\Users\\jan\\.codex\\hooks.json:stop:0:0".to_string(),
                false
            ),
        ]
    );
    let inline = r#"
[hooks.state]
"C:\\x\\hooks.json:session_start:0:0" = { trusted_hash = "sha256:b" }
"C:\\x\\hooks.json:stop:0:0" = { trusted_hash = "" }
"#;
    assert_eq!(
        parse_hook_state(inline),
        vec![
            ("C:\\x\\hooks.json:session_start:0:0".to_string(), true),
            ("C:\\x\\hooks.json:stop:0:0".to_string(), false),
        ]
    );
    assert!(parse_hook_state("model = \"x\"\n[hooks]\nfoo = 1\n").is_empty());
}

#[test]
fn codex_trust_follows_state_of_each_event() {
    assert_eq!(
        codex_trust(Registration::Missing, CONFIG_TABLES),
        Trust::None
    );
    assert_eq!(codex_trust(Registration::Current, ""), Trust::Untrusted);
    assert_eq!(
        codex_trust(Registration::Current, CONFIG_TABLES),
        Trust::Partial
    );
    let all: String = CODEX_EVENTS
        .iter()
        .map(|e| format!("[hooks.state.\"C:\\\\u\\\\.codex\\\\hooks.json:{e}:0:0\"]\ntrusted_hash = \"sha256:x\"\n"))
        .collect();
    assert_eq!(codex_trust(Registration::Current, &all), Trust::Trusted);
    // Zaufanie wpisu z innego pliku niż hooks.json nie liczy się.
    let other =
        "[hooks.state.\"C:\\\\u\\\\config.toml:pre_tool_use:0:0\"]\ntrusted_hash = \"sha256:x\"\n";
    assert_eq!(codex_trust(Registration::Current, other), Trust::Untrusted);
}

#[test]
fn status_reads_codex_trust_from_injected_config() {
    let root = temp("trust");
    let dirs = dirs(&root);
    std::fs::create_dir_all(root.join(".codex")).unwrap();
    let exe = root.join("tuktuk-track-hook.exe");
    hooks_config::sync(&dirs, Some(&exe));
    let status = status_for(&dirs, Some(&exe), true, String::new(), Vec::new());
    assert!(status.exe_found && status.codex.found && !status.claude.found);
    assert_eq!(status.codex_trust, "untrusted");
    std::fs::write(
        root.join(".codex").join("config.toml"),
        CODEX_EVENTS
            .iter()
            .map(|e| format!("[hooks.state.\"{e}-hooks.json\"]\ntrusted_hash = \"sha256:x\"\n"))
            .collect::<String>(),
    )
    .unwrap();
    let status = status_for(&dirs, Some(&exe), true, String::new(), Vec::new());
    assert_eq!(status.codex_trust, "trusted");
    let _ = std::fs::remove_dir_all(&root);
}

// --- potok ---

#[cfg(windows)]
#[test]
fn event_travels_from_hook_client_to_pipe_server() {
    use std::sync::{Arc, Mutex};
    let name = format!(r"\\.\pipe\tuktuk-track-hooks-test-{}", std::process::id());
    let got: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
    let sink = got.clone();
    std::thread::spawn(move || {
        let _ = crate::browser::pipe::serve(&name_for_server(), move |body| {
            sink.lock().unwrap().push(body.to_vec());
            json!({ "ok": true })
        });
    });
    fn name_for_server() -> String {
        format!(r"\\.\pipe\tuktuk-track-hooks-test-{}", std::process::id())
    }
    let raw = serde_json::to_vec(&json!({
        "session_id": "p1", "cwd": "C:\\x", "hook_event_name": "PreToolUse"
    }))
    .unwrap();
    let body = hook_wire::envelope("codex", &raw).unwrap();
    let mut sent = false;
    for _ in 0..100 {
        if hook_wire::forward(&name, &body).is_ok() {
            sent = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(sent, "potok nie odpowiedział");
    let received = got.lock().unwrap().clone();
    assert_eq!(received.len(), 1);
    let ev = parse(&received[0]).unwrap();
    assert_eq!((ev.agent.as_str(), ev.session_id.as_str()), ("codex", "p1"));
    // Bez serwera hook kończy się błędem, który program połyka (cicho, bez aplikacji).
    assert!(hook_wire::forward(r"\\.\pipe\tuktuk-track-hooks-nie-ma", &body).is_err());
}

#[test]
fn stop_after_a_lost_subagent_stop_ends_the_session() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&sub("SubagentStart", "a1"), T0 + 1_000);
    s.apply(&sub("PreToolUse", "a1"), T0 + 2_000);
    s.apply(&sub("PostToolUse", "a1"), T0 + 3_000);
    // SubagentStop zgubiony (przerwanie Esc, zalany potok). Kolejna tura po czasie:
    s.apply(&ev("claude", "UserPromptSubmit"), T0 + 10 * 60_000);
    s.apply(&ev("claude", "PreToolUse"), T0 + 10 * 60_000 + 1_000);
    s.apply(&ev("claude", "PostToolUse"), T0 + 10 * 60_000 + 2_000);
    // Stop kończy sesję, bo subagent milczy dłużej niż SUBAGENT_SILENCE.
    s.apply(&ev("claude", "Stop"), T0 + 10 * 60_000 + 3_000);
    assert_eq!(phase(&s), Phase::Done);
    assert!(s.session("claude", "s1").unwrap().subagents.is_empty());
    assert!(s.working().is_empty());
}

#[test]
fn session_waiting_only_for_silent_subagents_ends_without_a_second_stop() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&sub("SubagentStart", "a1"), T0 + 1_000);
    // Stop przy żywym subagencie: sesja pracuje dalej.
    s.apply(&ev("claude", "Stop"), T0 + 2_000);
    assert_eq!(phase(&s), Phase::Working);
    assert!(!s.expire(T0 + 2_000 + SUBAGENT_SILENCE - 2_000));
    assert_eq!(phase(&s), Phase::Working);
    // SubagentStop się zgubił: po SUBAGENT_SILENCE (nie dopiero po SILENCE) koniec.
    assert!(s.expire(T0 + 2_000 + SUBAGENT_SILENCE + 1_000));
    assert_eq!(phase(&s), Phase::Done);
    // Subagent w trakcie długiego narzędzia nadal trzyma sesję po Stop.
    let mut t = Sessions::default();
    t.apply(&ev("claude", "UserPromptSubmit"), T0);
    t.apply(&sub("PreToolUse", "a1"), T0 + 1_000);
    t.apply(&ev("claude", "Stop"), T0 + 2_000);
    assert!(!t.expire(T0 + 30 * 60_000));
    assert_eq!(phase(&t), Phase::Working);
    t.apply(&sub("PostToolUse", "a1"), T0 + 31 * 60_000);
    t.apply(&sub("SubagentStop", "a1"), T0 + 31 * 60_000 + 1);
    assert!(t.expire(T0 + 31 * 60_000 + SUBAGENT_SILENCE + 10));
    assert_eq!(phase(&t), Phase::Done);
}

#[test]
fn long_tool_call_keeps_the_session_working_past_silence() {
    let mut s = Sessions::default();
    s.apply(&ev("claude", "UserPromptSubmit"), T0);
    s.apply(&ev("claude", "PreToolUse"), T0 + 1_000);
    // Build trwa 25 minut bez zdarzeń: sesja dalej pracuje, `since` bez zmian.
    assert!(!s.expire(T0 + 25 * 60_000));
    assert_eq!(phase(&s), Phase::Working);
    s.apply(&ev("claude", "PostToolUse"), T0 + 25 * 60_000 + 1);
    assert_eq!(s.session("claude", "s1").unwrap().since, T0);
    assert!(s.session("claude", "s1").unwrap().tools.is_empty());
    // Po PostToolUse znów obowiązuje SILENCE.
    assert!(s.expire(T0 + 25 * 60_000 + SILENCE + 10));
    assert_eq!(phase(&s), Phase::Done);
    // Także narzędzie kończy się po TOOL_SILENCE (zawieszony proces).
    let mut t = Sessions::default();
    t.apply(&ev("claude", "PreToolUse"), T0);
    assert!(!t.expire(T0 + TOOL_SILENCE));
    assert!(t.expire(T0 + TOOL_SILENCE + 1));
    // Stop i Interrupt zamykają niezakończone wywołania.
    let mut u = Sessions::default();
    u.apply(&ev("claude", "PreToolUse"), T0);
    u.apply(&ev("claude", "Stop"), T0 + 1);
    assert!(u.session("claude", "s1").unwrap().tools.is_empty());
    u.apply(&sub("PreToolUse", "a9"), T0 + 2);
    u.apply(&ev("claude", "Interrupt"), T0 + 3);
    assert!(u.session("claude", "s1").unwrap().tools.is_empty());
}

#[test]
fn legacy_hook_entries_are_replaced_and_removed_with_the_own_ones() {
    let legacy = hooks_config::legacy_names::HOOK;
    let old = format!("\"C:/Users/ala/AppData/Local/Stara/{legacy}.exe\" claude");
    let new = hook_command(Path::new("C:\\App\\tuktuk-track-hook.exe"), "claude");
    assert!(hooks_config::is_own(&old));
    // Rejestracja zastępuje stary wpis nowym, wpisy Orki zostają.
    let mut root = orca_settings();
    install(&mut root, &CLAUDE_EVENTS, &old).unwrap();
    assert_eq!(
        registration(&root, &CLAUDE_EVENTS, &new),
        Registration::Stale
    );
    assert!(install(&mut root, &CLAUDE_EVENTS, &new).unwrap());
    for event in CLAUDE_EVENTS {
        let own: Vec<_> = commands(&root, event)
            .into_iter()
            .filter(|c| hooks_config::is_own(c))
            .collect();
        assert_eq!(own, vec![new.clone()], "{event}");
        assert!(!commands(&root, event).iter().any(|c| c.contains(legacy)));
    }
    assert_eq!(commands(&root, "Stop")[0], "node C:/orca/hook.js stop");
    // Wyłączenie hooków usuwa też stare wpisy; wpisy Orki zostają.
    let original = orca_settings();
    let mut only_old = original.clone();
    install(&mut only_old, &CLAUDE_EVENTS, &old).unwrap();
    assert!(uninstall(&mut only_old).unwrap());
    assert_eq!(only_old, original);
}
