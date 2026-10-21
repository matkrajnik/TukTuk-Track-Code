//! Testy A1: wykrywanie CLI, linie poleceń obu silników, parsowanie odpowiedzi z
//! zapisanych plików, walidacja ze schematem, mapowanie błędów, logika dialogu
//! pierwszego uruchomienia i przebieg `run` na podstawionym `Runner`.
//! Żaden test nie uruchamia claude ani codex.
use super::*;
use std::sync::Mutex;

const CLAUDE_OK: &str = include_str!("../../tests/fixtures/ai_claude_ok.json");
const CLAUDE_TEXT: &str = include_str!("../../tests/fixtures/ai_claude_text.json");
const CLAUDE_LOGIN: &str = include_str!("../../tests/fixtures/ai_claude_login.json");
const CLAUDE_LIMIT: &str = include_str!("../../tests/fixtures/ai_claude_limit.json");
const CLAUDE_EVENTS: &str = include_str!("../../tests/fixtures/ai_claude_events.json");
const CODEX_OK: &str = include_str!("../../tests/fixtures/ai_codex_ok.jsonl");
const CODEX_LAST: &str = include_str!("../../tests/fixtures/ai_codex_last.txt");
const CODEX_LOGIN: &str = include_str!("../../tests/fixtures/ai_codex_login.jsonl");
const CODEX_LIMIT: &str = include_str!("../../tests/fixtures/ai_codex_limit.jsonl");

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "task": {"type": "string"},
            "confidence": {"type": "number", "minimum": 0, "maximum": 1},
            "note": {"type": "string"}
        },
        "required": ["task", "confidence"],
        "additionalProperties": false
    })
}

fn out(success: bool, stdout: &str) -> Output {
    Output {
        success,
        stdout: stdout.into(),
        stderr: String::new(),
    }
}

/// Zapisane odpowiedzi; zapamiętuje polecenia.
struct Fake {
    reply: Box<dyn Fn(&Spec) -> Result<Output, RunError> + Send + Sync>,
    specs: Mutex<Vec<Spec>>,
}
impl Fake {
    fn new(reply: impl Fn(&Spec) -> Result<Output, RunError> + Send + Sync + 'static) -> Self {
        Self {
            reply: Box::new(reply),
            specs: Mutex::new(Vec::new()),
        }
    }
    fn fixed(result: Result<Output, RunError>) -> Self {
        Self::new(move |_| result.clone())
    }
    fn count(&self) -> usize {
        self.specs.lock().unwrap().len()
    }
    fn last(&self) -> Spec {
        self.specs.lock().unwrap().last().cloned().unwrap()
    }
}
impl Runner for Fake {
    fn run(&self, spec: &Spec, _timeout: Duration) -> Result<Output, RunError> {
        self.specs.lock().unwrap().push(spec.clone());
        (self.reply)(spec)
    }
}

fn loc() -> Locations {
    Locations {
        user_profile: Some(PathBuf::from(r"C:\Users\ala")),
        local_app_data: Some(PathBuf::from(r"C:\Users\ala\AppData\Local")),
        app_data: Some(PathBuf::from(r"C:\Users\ala\AppData\Roaming")),
        path: vec![PathBuf::from(r"C:\Tools"), PathBuf::from(r"C:\Bin")],
    }
}
fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}
const AI_DIR: &str = r"C:\dane\ai";

// ---------------------------------------------------------------------------
// Wykrywanie

#[test]
fn claude_candidates_follow_plan_order() {
    let list = claude_candidates(&loc(), r"D:\moje\claude.exe");
    assert_eq!(
        list,
        vec![
            p(r"C:\Users\ala\.local\bin\claude.exe"),
            p(r"C:\Tools\claude.exe"),
            p(r"C:\Tools\claude.cmd"),
            p(r"C:\Bin\claude.exe"),
            p(r"C:\Bin\claude.cmd"),
            p(r"D:\moje\claude.exe"),
        ]
    );
    // Bez ścieżki z ustawień i bez PATH zostaje tylko katalog domowy.
    let only = claude_candidates(
        &Locations {
            user_profile: loc().user_profile,
            ..Locations::default()
        },
        "  ",
    );
    assert_eq!(only, vec![p(r"C:\Users\ala\.local\bin\claude.exe")]);
}

#[test]
fn codex_candidates_follow_plan_order() {
    let list = codex_candidates(&loc(), r"D:\moje\codex.cmd");
    assert_eq!(list[0], p(r"C:\Tools\codex.exe"));
    assert_eq!(list[3], p(r"C:\Bin\codex.cmd"));
    let npm = list
        .iter()
        .position(|c| *c == p(r"C:\Users\ala\AppData\Roaming\npm\codex.cmd"))
        .unwrap();
    let openai = list
        .iter()
        .position(|c| *c == p(r"C:\Users\ala\AppData\Local\Programs\OpenAI\codex\codex.exe"))
        .unwrap();
    // PATH, potem npm global, potem Programs\OpenAI, na końcu ścieżka z ustawień.
    assert!(3 < npm && npm < openai);
    assert_eq!(list.last().unwrap(), &p(r"D:\moje\codex.cmd"));
}

#[test]
fn candidates_are_unique_ignoring_case() {
    let l = Locations {
        user_profile: Some(p(r"C:\Users\ala")),
        path: vec![p(r"C:\USERS\ALA\.local\bin")],
        ..Locations::default()
    };
    let list = claude_candidates(&l, r"c:\users\ala\.local\bin\CLAUDE.EXE");
    assert_eq!(
        list.iter()
            .filter(|c| c.to_string_lossy().to_lowercase().ends_with("claude.exe"))
            .count(),
        1
    );
}

#[test]
fn version_comes_from_first_numeric_token() {
    assert_eq!(parse_version("2.1.304 (Claude Code)\n"), "2.1.304");
    assert_eq!(parse_version("codex-cli 0.162.1\n"), "0.162.1");
    assert_eq!(parse_version("\n\nv1.2.3-beta\n"), "1.2.3-beta");
    assert_eq!(parse_version("nieznana"), "nieznana");
    assert_eq!(parse_version("   "), "");
}

#[test]
fn detect_uses_first_working_cli_and_reads_version() {
    let existing = [
        r"C:\Users\ala\.local\bin\claude.exe",
        r"C:\Tools\codex.exe",
        r"C:\Users\ala\AppData\Roaming\npm\codex.cmd",
    ];
    let exists = |path: &Path| existing.iter().any(|e| path == Path::new(e));
    let runner = Fake::new(|spec| {
        assert_eq!(spec.args, vec!["--version".to_string()]);
        // Także `--version` działa w katalogu AI i ze znacznikiem dla hooka.
        assert_eq!(spec.cwd, Some(p(AI_DIR)));
        assert_eq!(
            spec.env,
            vec![("TODO_TRACK_AI".to_string(), "1".to_string())]
        );
        let program = spec.program.to_string_lossy().to_lowercase();
        if program.ends_with(r"tools\codex.exe") {
            // Pierwszy kandydat istnieje, ale nie działa: bierzemy następny.
            Ok(out(false, ""))
        } else if program.contains("claude") {
            Ok(out(true, "2.1.304 (Claude Code)\n"))
        } else {
            Ok(out(true, "codex-cli 0.162.1\n"))
        }
    });
    let found = detect(
        &loc(),
        &AppSettings::default(),
        &exists,
        &runner,
        &p(AI_DIR),
    );
    let claude = found.claude.unwrap();
    assert_eq!(claude.path, r"C:\Users\ala\.local\bin\claude.exe");
    assert_eq!(claude.version, "2.1.304");
    let codex = found.codex.unwrap();
    assert_eq!(codex.path, r"C:\Users\ala\AppData\Roaming\npm\codex.cmd");
    assert_eq!(codex.version, "0.162.1");
    // Nieistniejące pliki nie są uruchamiane: dokładnie 3 wywołania `--version`.
    assert_eq!(runner.count(), 3);
}

#[test]
fn detect_uses_configured_path_last_and_reports_none() {
    let exists = |path: &Path| path == Path::new(r"D:\moje\claude.exe");
    let runner = Fake::fixed(Ok(out(true, "9.9.9\n")));
    let settings = AppSettings {
        ai_claude_path: r"D:\moje\claude.exe".into(),
        ..AppSettings::default()
    };
    let found = detect(&loc(), &settings, &exists, &runner, &p(AI_DIR));
    assert_eq!(found.claude.unwrap().path, r"D:\moje\claude.exe");
    assert!(found.codex.is_none());
    // Brak plików: nic nie jest uruchamiane i nic nie wykryto.
    let none = Fake::fixed(Err(RunError::Timeout));
    let found = detect(&loc(), &settings, &|_| false, &none, &p(AI_DIR));
    assert_eq!(found, Detected::default());
    assert_eq!(none.count(), 0);
    // Błąd uruchomienia = kolejny kandydat, a na końcu brak.
    let broken = Fake::fixed(Err(RunError::Spawn("zablokowany".into())));
    let found = detect(&loc(), &settings, &|_| true, &broken, &p(AI_DIR));
    assert!(!found.any());
}

// ---------------------------------------------------------------------------
// Dialog pierwszego uruchomienia

fn cli(path: &str) -> Cli {
    Cli {
        path: path.into(),
        version: "1.0.0".into(),
    }
}

#[test]
fn first_run_dialog_logic() {
    let none = Detected::default();
    let claude = Detected {
        claude: Some(cli("c")),
        codex: None,
    };
    let both = Detected {
        claude: Some(cli("c")),
        codex: Some(cli("x")),
    };
    // `""` + wykryte CLI → pytaj.
    assert!(should_ask("", &claude));
    assert!(should_ask("", &both));
    // Brak CLI → nie pytaj (Ustawienia pokażą instalację).
    assert!(!should_ask("", &none));
    // Wybór zapisany (także „Nie używaj”) → nie pytaj.
    for engine in ["claude", "codex", "off"] {
        assert!(!should_ask(engine, &both), "{engine}");
    }
    // Silnik jest aktywny tylko wtedy, gdy wybrany i wykryty.
    assert_eq!(active_engine("claude", &claude), Some(Engine::Claude));
    assert_eq!(active_engine("codex", &claude), None);
    assert_eq!(active_engine("off", &both), None);
    assert_eq!(active_engine("", &both), None);
}

#[test]
fn status_reports_ask_active_and_dir() {
    let detected = Detected {
        claude: Some(cli("c")),
        codex: None,
    };
    let dir = p(r"C:\dane\ai");
    let s = status_of(&AppSettings::default(), detected.clone(), &dir);
    assert!(s.ask && !s.active);
    assert_eq!(s.dir, r"C:\dane\ai");
    let chosen = AppSettings {
        ai_engine: "claude".into(),
        ..AppSettings::default()
    };
    let s = status_of(&chosen, detected, &dir);
    assert!(!s.ask && s.active);
    let json = serde_json::to_value(&s).unwrap();
    assert_eq!(json["detected"]["claude"]["version"], "1.0.0");
    assert!(json["detected"]["codex"].is_null());
}

// ---------------------------------------------------------------------------
// Linie poleceń

#[test]
fn claude_command_line_matches_plan() {
    let s = schema();
    assert_eq!(
        claude_args(&s, ""),
        vec![
            "-p",
            "--output-format",
            "json",
            "--json-schema",
            &s.to_string(),
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
            "--setting-sources",
            "",
            "--permission-prompts",
            "none",
            "--no-session-persistence",
        ]
    );
    let with_model = claude_args(&s, "haiku");
    assert_eq!(&with_model[with_model.len() - 2..], ["--model", "haiku"]);
}

#[test]
fn codex_command_line_matches_plan() {
    let args = codex_args(
        Path::new(r"C:\ai\schema-x.json"),
        Path::new(r"C:\ai\last-x.txt"),
        Path::new(r"C:\ai"),
        "gpt-5",
    );
    assert_eq!(
        args,
        vec![
            "exec",
            "--json",
            "--output-schema",
            r"C:\ai\schema-x.json",
            "--ephemeral",
            "--skip-git-repo-check",
            "-s",
            "read-only",
            "-C",
            r"C:\ai",
            "-c",
            "mcp_servers={}",
            "-c",
            "notify=[]",
            "-m",
            "gpt-5",
            "-o",
            r"C:\ai\last-x.txt",
        ]
    );
    // Bez modelu nie ma `-m`.
    let args = codex_args(Path::new("s"), Path::new("o"), Path::new("d"), "");
    assert!(!args.contains(&"-m".to_string()));
    // Serwery MCP z ~/.codex/config.toml są wyłączone także bez modelu.
    assert!(args.windows(2).any(|w| w == ["-c", "mcp_servers={}"]));
}

#[test]
fn timeout_kills_the_whole_process_tree() {
    let (program, args) = kill_tree_spec(4321, Some(Path::new(r"C:\Windows")));
    assert_eq!(program, p(r"C:\Windows\System32\taskkill.exe"));
    assert_eq!(args, vec!["/T", "/F", "/PID", "4321"]);
    let (program, _) = kill_tree_spec(1, None);
    assert_eq!(program, p("taskkill.exe"));
}

#[test]
fn spec_is_isolated_with_marker_stdin_and_hidden_prompt() {
    let dir = p(r"C:\dane\ai");
    let c = cli(r"C:\bin\claude.exe");
    let spec = build_spec(Engine::Claude, &c, &dir, "assign", "", &schema(), "Pytanie");
    assert_eq!(spec.program, p(r"C:\bin\claude.exe"));
    assert_eq!(spec.cwd, Some(dir.clone()));
    assert_eq!(spec.stdin, "Pytanie");
    assert_eq!(
        spec.env,
        vec![("TODO_TRACK_AI".to_string(), "1".to_string())]
    );
    // Prompt nigdy nie jest argumentem (widać go w liście procesów).
    assert!(spec.args.iter().all(|a| !a.contains("Pytanie")));
    let spec = build_spec(
        Engine::Codex,
        &cli("x"),
        &dir,
        "Assign/1",
        "",
        &schema(),
        "Pytanie",
    );
    assert!(spec
        .args
        .contains(&r"C:\dane\ai\schema-assign-1.json".to_string()));
    assert!(spec
        .args
        .contains(&r"C:\dane\ai\last-assign-1.txt".to_string()));
    assert_eq!(spec.stdin, "Pytanie");
}

#[test]
fn hook_marker_is_the_one_the_hook_program_rejects() {
    assert_eq!(hook_wire::AI_ENV, "TODO_TRACK_AI");
    let spec = build_spec(
        Engine::Claude,
        &cli("c"),
        Path::new("d"),
        "k",
        "",
        &schema(),
        "",
    );
    let (name, value) = &spec.env[0];
    assert_eq!(name, hook_wire::AI_ENV);
    assert!(hook_wire::suppressed(Some(value)));
}

#[test]
fn ai_directory_is_recognised_ignoring_case_and_slashes() {
    let ai = Path::new(r"C:\Users\ala\AppData\Roaming\pl.todotrack.desktop\ai");
    assert!(is_ai_dir(
        r"C:\Users\ala\AppData\Roaming\pl.todotrack.desktop\ai",
        ai
    ));
    assert!(is_ai_dir(
        r"c:/users/ala/appdata/roaming/pl.todotrack.desktop/AI/",
        ai
    ));
    assert!(!is_ai_dir(
        r"C:\Users\ala\AppData\Roaming\pl.todotrack.desktop\ai-notes",
        ai
    ));
    assert!(!is_ai_dir(
        r"C:\Users\ala\AppData\Roaming\pl.todotrack.desktop",
        ai
    ));
    assert!(!is_ai_dir("", ai));
}

// ---------------------------------------------------------------------------
// Parsowanie odpowiedzi

#[test]
fn claude_structured_output_is_parsed() {
    let value = parse_claude(CLAUDE_OK).unwrap();
    assert_eq!(value["task"], "Analiza importu");
    assert_eq!(value["confidence"], 0.8);
    assert!(validate(&schema(), &value).is_ok());
}

#[test]
fn claude_result_text_with_fence_and_event_list_are_parsed() {
    let value = parse_claude(CLAUDE_TEXT).unwrap();
    assert_eq!(value["task"], "Import CSV");
    let value = parse_claude(CLAUDE_EVENTS).unwrap();
    assert_eq!(value["task"], "Analiza importu");
    assert_eq!(parse_claude("nie json"), Err(Failure::Other));
    let no_json = r#"{"type":"result","is_error":false,"result":"Nie wiem."}"#;
    assert_eq!(parse_claude(no_json), Err(Failure::Unreadable));
}

#[test]
fn claude_errors_are_mapped() {
    assert_eq!(parse_claude(CLAUDE_LOGIN), Err(Failure::NotLoggedIn));
    assert_eq!(parse_claude(CLAUDE_LIMIT), Err(Failure::Limit));
    assert_eq!(parse_claude("Not logged in"), Err(Failure::NotLoggedIn));
}

#[test]
fn codex_jsonl_is_parsed_from_events_and_from_last_message_file() {
    let value = parse_codex(CODEX_OK, None).unwrap();
    assert_eq!(value["task"], "Analiza importu");
    assert_eq!(value["confidence"], 0.7);
    // Plik z `-o` ma pierwszeństwo przed zdarzeniami.
    let from_file = parse_codex("", Some(CODEX_LAST)).unwrap();
    assert_eq!(from_file, value);
    let both = parse_codex(CODEX_OK, Some(r#"{"task":"Inne","confidence":1}"#)).unwrap();
    assert_eq!(both["task"], "Inne");
    // Śmieci w strumieniu są pomijane.
    let noisy = format!("WARN coś\n{CODEX_OK}");
    assert_eq!(parse_codex(&noisy, None).unwrap(), value);
}

#[test]
fn codex_errors_are_mapped() {
    assert_eq!(parse_codex(CODEX_LOGIN, None), Err(Failure::NotLoggedIn));
    assert_eq!(parse_codex(CODEX_LIMIT, None), Err(Failure::Limit));
    assert_eq!(
        parse_codex(r#"{"type":"turn.started"}"#, None),
        Err(Failure::Other)
    );
    let text = r#"{"type":"item.completed","item":{"type":"agent_message","text":"zwykły tekst"}}"#;
    assert_eq!(parse_codex(text, None), Err(Failure::Unreadable));
}

#[test]
fn error_messages_are_polish_and_hide_cli_text() {
    let m = failure_message(Engine::Claude, &Failure::NotLoggedIn);
    assert_eq!(
        m,
        "Claude Code nie jest zalogowany. Zaloguj się: uruchom `claude` w terminalu."
    );
    let m = failure_message(Engine::Codex, &Failure::NotLoggedIn);
    assert!(m.contains("codex login"));
    assert!(failure_message(Engine::Codex, &Failure::Limit).contains("Limit subskrypcji Codex"));
    assert_eq!(
        failure_message(Engine::Claude, &Failure::Unreadable),
        BAD_SCHEMA
    );
    assert!(TIMED_OUT.contains("60 sekund"));
    assert_eq!(classify("Please run /login"), Failure::NotLoggedIn);
    assert_eq!(
        classify("5-hour limit reached ∙ resets 3pm"),
        Failure::Limit
    );
    assert_eq!(classify("coś innego"), Failure::Other);
}

// ---------------------------------------------------------------------------
// Walidacja

#[test]
fn schema_validation() {
    let s = schema();
    assert!(validate(&s, &json!({"task": "A", "confidence": 1})).is_ok());
    assert!(validate(&s, &json!({"task": "A"}))
        .unwrap_err()
        .contains("brak pola"));
    assert!(validate(&s, &json!({"task": 5, "confidence": 1})).is_err());
    assert!(validate(&s, &json!({"task": "A", "confidence": 2})).is_err());
    assert!(validate(&s, &json!({"task": "A", "confidence": 1, "x": 1}))
        .unwrap_err()
        .contains("nieznane pole"));
    assert!(validate(&s, &json!([1])).is_err());
    let list = json!({
        "type": "array",
        "minItems": 1,
        "maxItems": 2,
        "items": {"type": "string", "enum": ["a", "b"], "minLength": 1}
    });
    assert!(validate(&list, &json!(["a"])).is_ok());
    assert!(validate(&list, &json!([])).is_err());
    assert!(validate(&list, &json!(["a", "b", "a"])).is_err());
    assert!(validate(&list, &json!(["c"])).is_err());
    let nullable = json!({"anyOf": [{"type": "null"}, {"type": "integer"}]});
    assert!(validate(&nullable, &Value::Null).is_ok());
    assert!(validate(&nullable, &json!(3)).is_ok());
    assert!(validate(&nullable, &json!(3.5)).is_err());
    assert!(validate(&json!({"type": ["string", "null"]}), &Value::Null).is_ok());
}

// ---------------------------------------------------------------------------
// Przebieg wywołania

fn cfg(engine: &str, dir: &Path) -> Config {
    Config {
        engine: engine.into(),
        detected: Detected {
            claude: Some(cli(r"C:\bin\claude.exe")),
            codex: Some(cli(r"C:\bin\codex.cmd")),
        },
        model: "haiku".into(),
        dir: dir.to_path_buf(),
    }
}
fn dict() -> Dictionary {
    let mut d = Dictionary::new();
    d.add_name("Jan Kowalski");
    d
}
fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tt-ai-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
const PROMPT: &str = "Do zadania przypisano Jana Kowalskiego, sprawdź import.";

#[test]
fn claude_run_redacts_input_restores_output_and_caches() {
    let dir = temp("claude");
    let state = State::default();
    let runner = Fake::new(|spec| {
        // Do modelu nie trafia nazwisko, tylko pseudonim.
        assert!(!spec.stdin.contains("Kowalsk"), "{}", spec.stdin);
        if spec.stdin.starts_with("Do zadania") {
            assert!(spec.stdin.contains("Osob"), "{}", spec.stdin);
        }
        Ok(out(true, CLAUDE_OK))
    });
    let c = cfg("claude", &dir);
    let value = run(
        &state,
        &runner,
        &c,
        "assign",
        PROMPT,
        &schema(),
        &dict(),
        1000,
    )
    .unwrap();
    // Odpowiedź wraca z prawdziwą nazwą.
    assert_eq!(value["note"], "Zadanie od Jan Kowalski");
    assert_eq!(runner.count(), 1);
    let spec = runner.last();
    assert_eq!(spec.program, p(r"C:\bin\claude.exe"));
    assert_eq!(spec.cwd, Some(dir.clone()));
    assert!(spec
        .env
        .contains(&("TODO_TRACK_AI".to_string(), "1".to_string())));
    assert!(spec.args.windows(2).any(|w| w == ["--model", "haiku"]));
    // Drugie takie samo wywołanie z pamięci podręcznej, bez procesu.
    let again = run(
        &state,
        &runner,
        &c,
        "assign",
        PROMPT,
        &schema(),
        &dict(),
        2000,
    )
    .unwrap();
    assert_eq!(again, value);
    assert_eq!(runner.count(), 1);
    // Inny rodzaj albo inny tekst to nowe wywołanie.
    run(
        &state,
        &runner,
        &c,
        "other",
        PROMPT,
        &schema(),
        &dict(),
        3000,
    )
    .unwrap();
    run(
        &state,
        &runner,
        &c,
        "assign",
        "Inny tekst",
        &schema(),
        &dict(),
        4000,
    )
    .unwrap();
    assert_eq!(runner.count(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_hit_restores_names_of_the_current_call() {
    let dir = temp("cache-names");
    let state = State::default();
    let runner = Fake::fixed(Ok(out(true, CLAUDE_OK)));
    let c = cfg("claude", &dir);
    let mut d = Dictionary::new();
    d.add_name("Jan Kowalski");
    d.add_name("Anna Nowak");
    let first = run(
        &state,
        &runner,
        &c,
        "assign",
        "Spotkanie z Jan Kowalski",
        &schema(),
        &d,
        1000,
    )
    .unwrap();
    assert_eq!(first["note"], "Zadanie od Jan Kowalski");
    // Ten sam tekst po filtrze („Osoba 1”), więc ten sam klucz pamięci, ale
    // odpowiedź wraca z osobą z bieżącego promptu.
    let second = run(
        &state,
        &runner,
        &c,
        "assign",
        "Spotkanie z Anna Nowak",
        &schema(),
        &d,
        2000,
    )
    .unwrap();
    assert_eq!(runner.count(), 1);
    assert!(state.calls()[0].cached);
    assert_eq!(second["note"], "Zadanie od Anna Nowak");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn call_log_has_no_content() {
    let dir = temp("log");
    let state = State::default();
    let runner = Fake::fixed(Ok(out(true, CLAUDE_OK)));
    let c = cfg("claude", &dir);
    run(
        &state,
        &runner,
        &c,
        "assign",
        PROMPT,
        &schema(),
        &dict(),
        1000,
    )
    .unwrap();
    run(
        &state,
        &runner,
        &c,
        "assign",
        PROMPT,
        &schema(),
        &dict(),
        2000,
    )
    .unwrap();
    let calls = state.calls();
    assert_eq!(calls.len(), 2);
    // Od najnowszego; drugie z pamięci podręcznej.
    assert_eq!(calls[0].at, 2000);
    assert!(calls[0].cached && calls[0].ok);
    assert!(!calls[1].cached);
    assert!(calls[1].length > 10 && calls[1].length < PROMPT.len() + 10);
    let text = serde_json::to_string(&calls).unwrap();
    assert!(!text.contains("import") && !text.contains("Kowalsk") && !text.contains("Osoba"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn errors_are_mapped_logged_and_not_cached() {
    let dir = temp("errors");
    let state = State::default();
    let c = cfg("claude", &dir);
    let go =
        |runner: &Fake, kind: &str| run(&state, runner, &c, kind, PROMPT, &schema(), &dict(), 1);
    let login = Fake::fixed(Ok(out(false, CLAUDE_LOGIN)));
    assert_eq!(
        go(&login, "a").unwrap_err(),
        "Claude Code nie jest zalogowany. Zaloguj się: uruchom `claude` w terminalu."
    );
    let limit = Fake::fixed(Ok(out(true, CLAUDE_LIMIT)));
    assert!(go(&limit, "b")
        .unwrap_err()
        .contains("Limit subskrypcji Claude Code"));
    let timeout = Fake::fixed(Err(RunError::Timeout));
    assert_eq!(go(&timeout, "c").unwrap_err(), TIMED_OUT);
    let spawn = Fake::fixed(Err(RunError::Spawn("odmowa dostępu".into())));
    assert!(go(&spawn, "d")
        .unwrap_err()
        .starts_with("Nie udało się uruchomić Claude Code"));
    // Błąd procesu bez czytelnej odpowiedzi: powód ze stderr.
    let stderr = Fake::fixed(Ok(Output {
        success: false,
        stdout: String::new(),
        stderr: "Error: Not logged in".into(),
    }));
    assert!(go(&stderr, "e")
        .unwrap_err()
        .contains("nie jest zalogowany"));
    let broken = Fake::fixed(Ok(Output {
        success: false,
        stdout: String::new(),
        stderr: "segfault".into(),
    }));
    assert_eq!(
        go(&broken, "f").unwrap_err(),
        "Claude Code zakończył się błędem."
    );
    // Odpowiedź niezgodna ze schematem.
    let wrong = Fake::fixed(Ok(out(
        true,
        r#"{"type":"result","is_error":false,"structured_output":{"task":1}}"#,
    )));
    assert_eq!(go(&wrong, "g").unwrap_err(), BAD_SCHEMA);
    let codes: Vec<String> = state
        .calls()
        .iter()
        .rev()
        .map(|c| c.error.clone())
        .collect();
    assert_eq!(
        codes,
        ["login", "limit", "timeout", "spawn", "login", "error", "schema"]
    );
    assert!(state.calls().iter().all(|c| !c.ok));
    // Błędy nie trafiają do pamięci podręcznej: ponowienie woła proces.
    let ok = Fake::fixed(Ok(out(true, CLAUDE_OK)));
    assert!(go(&ok, "a").is_ok());
    assert_eq!(ok.count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn engine_missing_or_off_never_starts_a_process() {
    let dir = temp("off");
    let state = State::default();
    let runner = Fake::fixed(Ok(out(true, CLAUDE_OK)));
    for engine in ["", "off"] {
        let err = run(
            &state,
            &runner,
            &cfg(engine, &dir),
            "k",
            PROMPT,
            &schema(),
            &dict(),
            1,
        )
        .unwrap_err();
        assert_eq!(err, OFF);
    }
    let mut c = cfg("codex", &dir);
    c.detected.codex = None;
    let err = run(&state, &runner, &c, "k", PROMPT, &schema(), &dict(), 1).unwrap_err();
    assert!(err.contains("Nie znaleziono Codex"));
    c.detected.claude = None;
    let err = run(&state, &runner, &c, "k", PROMPT, &schema(), &dict(), 1).unwrap_err();
    assert_eq!(err, NOT_FOUND_ANY);
    assert_eq!(runner.count(), 0);
    assert!(!dir.exists());
}

#[test]
fn one_process_per_kind_at_a_time() {
    let dir = temp("busy");
    let state = State::default();
    let runner = Fake::fixed(Ok(out(true, CLAUDE_OK)));
    let c = cfg("claude", &dir);
    let guard = state.acquire("assign").unwrap();
    assert!(state.acquire("assign").is_none());
    // Inny rodzaj nie czeka.
    assert!(run(&state, &runner, &c, "other", PROMPT, &schema(), &dict(), 1).is_ok());
    assert_eq!(
        run(&state, &runner, &c, "assign", PROMPT, &schema(), &dict(), 2).unwrap_err(),
        BUSY
    );
    drop(guard);
    assert!(run(&state, &runner, &c, "assign", PROMPT, &schema(), &dict(), 3).is_ok());
    // Rodzaj jest zwalniany także po błędzie.
    let failing = Fake::fixed(Err(RunError::Timeout));
    assert!(run(&state, &failing, &c, "third", PROMPT, &schema(), &dict(), 4).is_err());
    assert!(run(&state, &runner, &c, "third", PROMPT, &schema(), &dict(), 5).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn codex_run_writes_schema_reads_last_message_and_restores() {
    let dir = temp("codex");
    let state = State::default();
    let schema_value = schema();
    let expected_schema = schema_value.to_string();
    let runner = Fake::new(move |spec| {
        assert!(!spec.stdin.contains("Kowalsk"));
        let after = |flag: &str| {
            let at = spec.args.iter().position(|a| a == flag).unwrap();
            PathBuf::from(&spec.args[at + 1])
        };
        // Schemat jest zapisany w pliku przed uruchomieniem.
        assert_eq!(
            std::fs::read_to_string(after("--output-schema")).unwrap(),
            expected_schema
        );
        // Codex zapisuje ostatnią wiadomość w pliku z `-o`.
        std::fs::write(after("-o"), CODEX_LAST).unwrap();
        Ok(out(true, CODEX_OK))
    });
    let c = cfg("codex", &dir);
    let value = run(
        &state,
        &runner,
        &c,
        "assign",
        PROMPT,
        &schema_value,
        &dict(),
        1,
    )
    .unwrap();
    assert_eq!(value["note"], "Zadanie od Jan Kowalski");
    let spec = runner.last();
    assert_eq!(spec.program, p(r"C:\bin\codex.cmd"));
    assert_eq!(&spec.args[..2], ["exec", "--json"]);
    assert!(spec.args.windows(2).any(|w| w == ["-s", "read-only"]));
    assert!(spec.args.windows(2).any(|w| w == ["-m", "haiku"]));
    // Plik z ostatnią odpowiedzią nie zostaje (mogłaby mieć dane po restore).
    assert!(!dir.join("last-assign.txt").exists());
    // Błędy Codexa po polsku.
    let login = Fake::fixed(Ok(out(false, CODEX_LOGIN)));
    let err = run(&state, &login, &c, "x", PROMPT, &schema_value, &dict(), 2).unwrap_err();
    assert!(err.contains("Codex nie jest zalogowany") && err.contains("codex login"));
    let limit = Fake::fixed(Ok(out(false, CODEX_LIMIT)));
    let err = run(&state, &limit, &c, "y", PROMPT, &schema_value, &dict(), 3).unwrap_err();
    assert!(err.contains("Limit subskrypcji Codex"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cache_key_depends_on_kind_engine_model_schema_and_text() {
    let s = schema();
    let a = cache_key("k", Engine::Claude, "", &s, "tekst");
    assert_eq!(a, cache_key("k", Engine::Claude, "", &s, "tekst"));
    assert_ne!(a, cache_key("l", Engine::Claude, "", &s, "tekst"));
    assert_ne!(a, cache_key("k", Engine::Codex, "", &s, "tekst"));
    assert_ne!(a, cache_key("k", Engine::Claude, "m", &s, "tekst"));
    assert_ne!(a, cache_key("k", Engine::Claude, "", &json!({}), "tekst"));
    assert_ne!(a, cache_key("k", Engine::Claude, "", &s, "inny"));
}

#[test]
fn model_tiers_come_from_settings() {
    let s = AppSettings {
        ai_model_fast: "haiku".into(),
        ai_model_main: "opus".into(),
        ..AppSettings::default()
    };
    assert_eq!(model_for(&s, Tier::Fast), "haiku");
    assert_eq!(model_for(&s, Tier::Main), "opus");
    assert_eq!(model_for(&AppSettings::default(), Tier::Main), "");
}

#[test]
fn call_log_keeps_the_latest_thirty() {
    let state = State::default();
    for i in 0..45 {
        state.record(Call {
            kind: "k".into(),
            at: i,
            length: 1,
            ok: true,
            error: String::new(),
            cached: false,
        });
    }
    let calls = state.calls();
    assert_eq!(calls.len(), 30);
    assert_eq!(calls[0].at, 44);
    assert_eq!(calls[29].at, 15);
}
