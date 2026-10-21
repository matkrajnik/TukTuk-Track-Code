//! Budowa poleceń rejestracji, parsowanie `mcp get` / `mcp list` z zapisanych
//! plików i przebieg rejestracji na podstawionym `Runner` (A2). Żaden test nie
//! uruchamia claude ani codex i nie dotyka plików użytkownika.
use super::*;
use std::sync::Mutex;

const CLAUDE_GET: &str = include_str!("../tests/fixtures/mcp_claude_get.txt");
const CLAUDE_MISSING: &str = include_str!("../tests/fixtures/mcp_claude_get_missing.txt");
const CODEX_LIST: &str = include_str!("../tests/fixtures/mcp_codex_list.txt");
const CODEX_OTHER: &str = include_str!("../tests/fixtures/mcp_codex_list_other.txt");
const EXE_PATH: &str = r"C:\Users\ala\AppData\Local\Todo Track\todo-track-mcp.exe";
const DIR: &str = r"C:\dane\ai";

fn exe() -> PathBuf {
    PathBuf::from(EXE_PATH)
}
fn cli(path: &str) -> Cli {
    Cli {
        path: path.into(),
        version: "1.0".into(),
    }
}
fn out(success: bool, stdout: &str, stderr: &str) -> Output {
    Output {
        success,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}
fn dir() -> &'static Path {
    Path::new(DIR)
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
    fn args(&self) -> Vec<String> {
        self.specs
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.args.join(" "))
            .collect()
    }
    fn count(&self) -> usize {
        self.specs.lock().unwrap().len()
    }
}
impl Runner for Fake {
    fn run(&self, spec: &Spec, _timeout: Duration) -> Result<Output, RunError> {
        self.specs.lock().unwrap().push(spec.clone());
        (self.reply)(spec)
    }
}

fn is(spec: &Spec, verb: &str) -> bool {
    spec.args.get(1).map(String::as_str) == Some(verb)
}

/// Claude: `get` odpowiada tekstem (jako sukces albo błąd), `add` i `remove` się udają.
fn claude_with(get: &'static str, get_ok: bool) -> Fake {
    Fake::new(move |spec| {
        Ok(if is(spec, "get") {
            if get_ok {
                out(true, get, "")
            } else {
                out(false, "", get)
            }
        } else {
            out(true, "", "")
        })
    })
}

/// Codex: `list` odpowiada tekstem, `add` i `remove` się udają.
fn codex_with(list: &'static str) -> Fake {
    Fake::new(move |spec| {
        Ok(if is(spec, "list") {
            out(true, list, "")
        } else {
            out(true, "", "")
        })
    })
}

#[test]
fn registration_commands_match_the_plan() {
    let exe = exe();
    assert_eq!(
        add_args(Engine::Claude, &exe),
        vec![
            "mcp",
            "add",
            "--scope",
            "user",
            "todo-track",
            "--",
            EXE_PATH
        ]
    );
    assert_eq!(
        add_args(Engine::Codex, &exe),
        vec!["mcp", "add", "todo-track", "--", EXE_PATH]
    );
    assert_eq!(
        remove_args(Engine::Claude),
        vec!["mcp", "remove", "--scope", "user", "todo-track"]
    );
    assert_eq!(
        remove_args(Engine::Codex),
        vec!["mcp", "remove", "todo-track"]
    );
    assert_eq!(query_args(Engine::Claude), vec!["mcp", "get", "todo-track"]);
    assert_eq!(query_args(Engine::Codex), vec!["mcp", "list"]);
}

#[test]
fn parses_claude_mcp_get() {
    assert_eq!(parse_claude_get(CLAUDE_GET).as_deref(), Some(EXE_PATH));
    assert_eq!(parse_claude_get(CLAUDE_MISSING), None);
    assert_eq!(parse_claude_get("Command:   \n"), None);
    assert!(claude_missing(CLAUDE_MISSING));
    assert!(!claude_missing(CLAUDE_GET));
}

#[test]
fn parses_codex_mcp_list() {
    // Ścieżka ze spacjami jest ucinana na `.exe`; inne serwery są pomijane.
    assert_eq!(parse_codex_list(CODEX_LIST).as_deref(), Some(EXE_PATH));
    // `todo-track-x` to inna nazwa, nie nasz serwer.
    assert_eq!(parse_codex_list(CODEX_OTHER), None);
    assert_eq!(parse_codex_list(""), None);
    assert_eq!(
        parse_codex_list("todo-track  \"C:\\Program Files\\T\\todo-track-mcp.exe\"  -  -")
            .as_deref(),
        Some(r"C:\Program Files\T\todo-track-mcp.exe")
    );
    assert_eq!(
        parse_codex_list("todo-track  node  server.js").as_deref(),
        Some("node")
    );
}

#[test]
fn path_comparison_ignores_case_slashes_quotes_and_prefix() {
    assert!(same_path(EXE_PATH, &EXE_PATH.to_uppercase()));
    assert!(same_path(
        "C:/Users/ala/AppData/Local/Todo Track/todo-track-mcp.exe",
        EXE_PATH
    ));
    assert!(same_path(&format!("\"{EXE_PATH}\""), EXE_PATH));
    assert!(same_path(&format!(r"\\?\{EXE_PATH}"), EXE_PATH));
    assert!(!same_path(r"C:\Stara\todo-track-mcp.exe", EXE_PATH));
    assert!(!same_path("", ""));
}

#[test]
fn plan_step_covers_every_case() {
    let exe = exe();
    let same = Found::Registered(EXE_PATH.into());
    let old = Found::Registered(r"C:\Stara\todo-track-mcp.exe".into());
    assert_eq!(plan_step(true, &Found::Missing, Some(&exe)), Step::Add);
    assert_eq!(plan_step(true, &same, Some(&exe)), Step::Nothing);
    assert_eq!(plan_step(true, &old, Some(&exe)), Step::Replace);
    assert_eq!(plan_step(false, &Found::Missing, Some(&exe)), Step::Nothing);
    assert_eq!(plan_step(false, &same, Some(&exe)), Step::Remove);
    assert_eq!(plan_step(false, &old, None), Step::Remove);
}

#[test]
fn claude_adds_when_missing() {
    let runner = claude_with(CLAUDE_MISSING, false);
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "registered");
    assert_eq!(
        runner.args(),
        vec![
            "mcp get todo-track".to_string(),
            format!("mcp add --scope user todo-track -- {EXE_PATH}"),
        ]
    );
    // Katalog roboczy to katalog AI, nie katalog użytkownika.
    assert_eq!(runner.specs.lock().unwrap()[0].cwd.as_deref(), Some(dir()));
}

#[test]
fn claude_does_nothing_when_up_to_date() {
    let runner = claude_with(CLAUDE_GET, true);
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "registered");
    assert_eq!(runner.args(), vec!["mcp get todo-track".to_string()]);
}

#[test]
fn claude_replaces_a_stale_path_after_update() {
    let moved = PathBuf::from(r"D:\Nowa\todo-track-mcp.exe");
    let runner = claude_with(CLAUDE_GET, true);
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&moved),
        true,
        dir(),
    );
    assert_eq!(entry.state, "registered");
    assert_eq!(
        runner.args(),
        vec![
            "mcp get todo-track".to_string(),
            "mcp remove --scope user todo-track".to_string(),
            r"mcp add --scope user todo-track -- D:\Nowa\todo-track-mcp.exe".to_string(),
        ]
    );
}

#[test]
fn disabling_removes_only_when_registered() {
    let registered = claude_with(CLAUDE_GET, true);
    let entry = sync_one(
        &registered,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&exe()),
        false,
        dir(),
    );
    assert_eq!(entry.state, "missing");
    assert_eq!(
        registered.args(),
        vec![
            "mcp get todo-track".to_string(),
            "mcp remove --scope user todo-track".to_string()
        ]
    );
    let absent = claude_with(CLAUDE_MISSING, false);
    let entry = sync_one(
        &absent,
        Engine::Claude,
        &cli("claude.exe"),
        None,
        false,
        dir(),
    );
    assert_eq!(entry.state, "missing");
    assert_eq!(absent.args(), vec!["mcp get todo-track".to_string()]);
}

#[test]
fn codex_flow_uses_mcp_list() {
    let runner = codex_with(CODEX_OTHER);
    let entry = sync_one(
        &runner,
        Engine::Codex,
        &cli("codex.cmd"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "registered");
    assert_eq!(
        runner.args(),
        vec![
            "mcp list".to_string(),
            format!("mcp add todo-track -- {EXE_PATH}")
        ]
    );
    let runner = codex_with(CODEX_LIST);
    let entry = sync_one(
        &runner,
        Engine::Codex,
        &cli("codex.cmd"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "registered");
    assert_eq!(runner.args(), vec!["mcp list".to_string()]);
    let runner = codex_with(CODEX_LIST);
    let entry = sync_one(
        &runner,
        Engine::Codex,
        &cli("codex.cmd"),
        None,
        false,
        dir(),
    );
    assert_eq!(entry.state, "missing");
    assert_eq!(
        runner.args(),
        vec!["mcp list".to_string(), "mcp remove todo-track".to_string()]
    );
}

#[test]
fn errors_are_reported_per_cli_in_one_line() {
    // Brak exe przy włączonym serwerze: bez żadnego procesu.
    let runner = claude_with(CLAUDE_MISSING, false);
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        None,
        true,
        dir(),
    );
    assert_eq!(entry.state, "error");
    assert!(entry.detail.contains("todo-track-mcp.exe"));
    assert_eq!(runner.count(), 0);
    // `get` kończy się nieznanym błędem.
    let runner = claude_with("Error: boom\nsecond line", false);
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "error");
    assert_eq!(entry.detail, "Error: boom");
    // Nieudane `add`.
    let runner = Fake::new(|spec| {
        Ok(if is(spec, "get") {
            out(false, "", "No MCP server found with name: todo-track")
        } else {
            out(false, "", "permission denied")
        })
    });
    let entry = sync_one(
        &runner,
        Engine::Claude,
        &cli("claude.exe"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "error");
    assert_eq!(entry.detail, "permission denied");
    // Przekroczony czas i błąd uruchomienia.
    let runner = Fake::new(|_| Err(RunError::Timeout));
    let entry = sync_one(
        &runner,
        Engine::Codex,
        &cli("codex.cmd"),
        Some(&exe()),
        true,
        dir(),
    );
    assert_eq!(entry.state, "error");
    assert!(entry.detail.contains("30 sekund"));
    let runner = Fake::new(|_| Err(RunError::Spawn("nie ma pliku".into())));
    let entry = sync_one(
        &runner,
        Engine::Codex,
        &cli("codex.cmd"),
        Some(&exe()),
        true,
        dir(),
    );
    assert!(entry.detail.contains("nie ma pliku"));
}

#[test]
fn sync_all_respects_engine_choice_and_detection() {
    let detected = Detected {
        claude: Some(cli("claude.exe")),
        codex: None,
    };
    // Jeszcze nie pytano o silnik: żaden proces.
    let runner = claude_with(CLAUDE_MISSING, false);
    assert!(sync_all(&runner, &detected, "", true, Some(&exe()), dir()).is_empty());
    assert_eq!(runner.count(), 0);
    // Wybrany silnik i włączone MCP: rejestracja tylko w wykrytym CLI.
    let runner = claude_with(CLAUDE_MISSING, false);
    let entries = sync_all(&runner, &detected, "codex", true, Some(&exe()), dir());
    assert_eq!(
        entries
            .iter()
            .map(|e| (e.engine.as_str(), e.state.as_str()))
            .collect::<Vec<_>>(),
        vec![("claude", "registered"), ("codex", "unavailable")]
    );
    // „Nie używaj” albo wyłączone MCP: tylko sprzątanie istniejącej rejestracji.
    for (engine, mcp) in [("off", true), ("claude", false)] {
        let runner = claude_with(CLAUDE_GET, true);
        let entries = sync_all(&runner, &detected, engine, mcp, Some(&exe()), dir());
        assert_eq!(entries[0].state, "missing");
        assert!(runner.args().iter().any(|a| a.starts_with("mcp remove")));
        assert!(!runner.args().iter().any(|a| a.starts_with("mcp add")));
    }
}
