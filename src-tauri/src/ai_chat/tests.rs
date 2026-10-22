//! Testy A3: parser `stream-json` (tekst, wywołania narzędzi, koniec, błąd) na
//! zapisanych strumieniach obu silników, linie poleceń, bramka ustawień,
//! przebieg rozmowy na podstawionym `Spawner` (filtr nazwisk, przywracanie,
//! zatrzymanie, ubity proces, historia po wznowieniu) oraz zarządzanie
//! prawdziwym procesem na procesie zastępczym (plik testowy uruchomiony jako
//! prosty serwer linii). Żaden test nie uruchamia claude ani codex.
use super::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

const CLAUDE: &str = include_str!("../../tests/fixtures/ai_chat_claude.jsonl");
const CLAUDE_LOGIN: &str = include_str!("../../tests/fixtures/ai_chat_claude_login.jsonl");
const CLAUDE_LIMIT: &str = include_str!("../../tests/fixtures/ai_chat_claude_limit.jsonl");
const CODEX: &str = include_str!("../../tests/fixtures/ai_chat_codex.jsonl");
const CODEX_LOGIN: &str = include_str!("../../tests/fixtures/ai_codex_login.jsonl");

fn parse_all(engine: Engine, stream: &str) -> Vec<Parsed> {
    stream
        .lines()
        .flat_map(|line| parse_line(engine, line))
        .collect()
}

// ---------------------------------------------------------------------------
// Parser

#[test]
fn claude_stream_gives_text_tool_call_result_and_end() {
    let events = parse_all(Engine::Claude, CLAUDE);
    assert_eq!(
        events,
        vec![
            Parsed::Text("Sprawdzam dzień.".into()),
            Parsed::Tool {
                name: "day_summary".into(),
                input: json!({"day": "2026-10-20"}),
            },
            Parsed::ToolDone { error: false },
            Parsed::Text("Osoba 1 pracowała nad importem przez 2 h.".into()),
            Parsed::Done("Osoba 1 pracowała nad importem przez 2 h.".into()),
        ]
    );
}

#[test]
fn claude_errors_are_classified() {
    assert_eq!(
        parse_all(Engine::Claude, CLAUDE_LOGIN),
        vec![Parsed::Error(Failure::NotLoggedIn)]
    );
    assert_eq!(
        parse_all(Engine::Claude, CLAUDE_LIMIT),
        vec![Parsed::Error(Failure::Limit)]
    );
}

#[test]
fn claude_ignores_noise_and_failed_tool_result_is_marked() {
    assert!(parse_claude_line("").is_empty());
    assert!(parse_claude_line("to nie jest JSON").is_empty());
    assert!(parse_claude_line(r#"{"type":"rate_limit_event"}"#).is_empty());
    assert!(parse_claude_line(r#"{"type":"stream_event","event":{}}"#).is_empty());
    assert!(parse_claude_line(
        r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"  "}]}}"#
    )
    .is_empty());
    assert_eq!(
        parse_claude_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","is_error":true,"content":"x"}]}}"#
        ),
        vec![Parsed::ToolDone { error: true }]
    );
}

#[test]
fn codex_stream_gives_tool_call_text_and_end() {
    let events = parse_all(Engine::Codex, CODEX);
    assert_eq!(
        events,
        vec![
            Parsed::Tool {
                name: "day_summary".into(),
                input: json!({"day": "2026-10-20"}),
            },
            Parsed::ToolDone { error: false },
            Parsed::Text("Osoba 1 pracowała nad importem przez 2 h.".into()),
            Parsed::Done(String::new()),
        ]
    );
}

#[test]
fn codex_errors_are_classified_and_failed_tool_is_marked() {
    let events = parse_all(Engine::Codex, CODEX_LOGIN);
    assert!(!events.is_empty());
    assert!(events
        .iter()
        .all(|e| *e == Parsed::Error(Failure::NotLoggedIn)));
    assert_eq!(
        parse_codex_line(
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"x","status":"failed"}}"#
        ),
        vec![Parsed::ToolDone { error: true }]
    );
}

#[test]
fn tool_names_lose_the_server_prefix() {
    assert_eq!(short_tool_name("mcp__todo-track__tasks_list"), "tasks_list");
    assert_eq!(short_tool_name("tasks_list"), "tasks_list");
}

// ---------------------------------------------------------------------------
// Linie poleceń i bramka

#[test]
fn claude_chat_command_line_matches_plan() {
    let args = claude_chat_args(Path::new("C:\\ai\\chat-mcp.json"), "opus");
    let at = |flag: &str| args.iter().position(|a| a == flag).unwrap();
    assert_eq!(args[0], "-p");
    assert_eq!(args[at("--input-format") + 1], "stream-json");
    assert_eq!(args[at("--output-format") + 1], "stream-json");
    assert!(args.contains(&"--verbose".to_string()));
    // Wbudowane narzędzia wyłączone (pusty argument), tylko narzędzia serwera.
    assert_eq!(args[at("--tools") + 1], "");
    assert!(args.contains(&"--strict-mcp-config".to_string()));
    assert_eq!(args[at("--mcp-config") + 1], "C:\\ai\\chat-mcp.json");
    assert_eq!(args[at("--allowedTools") + 1], "mcp__todo-track__*");
    assert_eq!(args[at("--permission-prompts") + 1], "none");
    assert_eq!(args[at("--setting-sources") + 1], "");
    assert!(args.contains(&"--no-session-persistence".to_string()));
    assert_eq!(args[at("--append-system-prompt") + 1], ROLE);
    assert_eq!(args[at("--model") + 1], "opus");
    // Bez modelu z ustawień: domyślny model CLI.
    assert!(!claude_chat_args(Path::new("x"), "").contains(&"--model".to_string()));
}

#[test]
fn codex_chat_command_line_passes_the_mcp_server() {
    let args = codex_chat_args(
        Path::new("C:\\ai"),
        Path::new("C:\\Apps\\todo-track-mcp.exe"),
        "gpt-x",
    );
    assert_eq!(&args[..3], ["exec", "--json", "--ephemeral"]);
    assert!(args.contains(&"--skip-git-repo-check".to_string()));
    let at = |flag: &str| args.iter().position(|a| a == flag).unwrap();
    assert_eq!(args[at("-s") + 1], "read-only");
    assert_eq!(args[at("-C") + 1], "C:\\ai");
    assert_eq!(args[at("-m") + 1], "gpt-x");
    // Cała tabela serwerów MCP zastąpiona jednym wpisem: serwery użytkownika z
    // ~/.codex/config.toml (np. Jira z zapisem) nie są w czacie dostępne.
    assert!(args.contains(
        &"mcp_servers={\"todo-track\"={command=\"C:\\\\Apps\\\\todo-track-mcp.exe\",args=[]}}"
            .to_string()
    ));
    assert!(
        !args.iter().any(|a| a.starts_with("mcp_servers.")),
        "częściowe nadpisanie zostawiłoby inne serwery: {args:?}"
    );
    // Bez wbudowanej powłoki i wyszukiwania w sieci; każde `-c` ma wartość.
    for feature in ["shell_tool", "unified_exec", "web_search_request"] {
        let value = format!("features.{feature}=false");
        assert_eq!(args[at(&value) - 1], "-c", "{value}");
    }
    assert_eq!(args[at("notify=[]") - 1], "-c");
    assert!(!codex_chat_args(Path::new("x"), Path::new("y"), "").contains(&"-m".to_string()));
}

#[test]
fn codex_mcp_override_is_a_valid_toml_inline_table() {
    // Ścieżka z cudzysłowem i odwrotnymi ukośnikami nie psuje wartości.
    let args = codex_chat_args(
        Path::new("C:\\ai"),
        Path::new("C:\\Apps \"x\"\\todo-track-mcp.exe"),
        "",
    );
    let value = args
        .iter()
        .find_map(|a| a.strip_prefix("mcp_servers="))
        .unwrap();
    // Ta sama wartość jako JSON po zamianie składni TOML (`=` → `:`, klucze w cudzysłowie).
    let json_like = value
        .replace("={command=", ":{\"command\":")
        .replace(",args=[]}", ",\"args\":[]}");
    let parsed: Value = serde_json::from_str(&json_like).unwrap();
    assert_eq!(
        parsed["todo-track"]["command"],
        "C:\\Apps \"x\"\\todo-track-mcp.exe"
    );
    assert_eq!(parsed.as_object().unwrap().len(), 1);
}

#[test]
fn mcp_config_points_to_the_server_exe() {
    let config = mcp_config(Path::new("C:\\Apps\\todo-track-mcp.exe"));
    assert_eq!(
        config["mcpServers"]["todo-track"]["command"],
        "C:\\Apps\\todo-track-mcp.exe"
    );
    assert_eq!(config["mcpServers"]["todo-track"]["args"], json!([]));
}

fn cli() -> Cli {
    Cli {
        path: "C:\\bin\\claude.exe".into(),
        version: "2.1.304".into(),
    }
}

#[test]
fn gate_requires_engine_chat_mcp_cli_and_server_exe() {
    let mut settings = AppSettings::default();
    let detected = ai::Detected {
        claude: Some(cli()),
        codex: None,
    };
    let exe = || Some(PathBuf::from("C:\\Apps\\todo-track-mcp.exe"));
    // Bez wyboru silnika i przy „Nie używaj” czat nie działa.
    assert_eq!(gate(&settings, &detected, exe()).unwrap_err(), ai::OFF);
    settings.ai_engine = "off".into();
    assert_eq!(gate(&settings, &detected, exe()).unwrap_err(), ai::OFF);
    settings.ai_engine = "claude".into();
    assert!(gate(&settings, &detected, exe()).is_ok());
    settings.ai_chat = false;
    assert_eq!(gate(&settings, &detected, exe()).unwrap_err(), CHAT_OFF);
    settings.ai_chat = true;
    settings.ai_mcp = false;
    assert_eq!(gate(&settings, &detected, exe()).unwrap_err(), MCP_REQUIRED);
    settings.ai_mcp = true;
    assert_eq!(gate(&settings, &detected, None).unwrap_err(), MCP_MISSING);
    settings.ai_engine = "codex".into();
    assert!(gate(&settings, &detected, exe())
        .unwrap_err()
        .contains("Nie znaleziono Codex"));
}

// ---------------------------------------------------------------------------
// Rozmowa na podstawionym procesie

struct Handle {
    tx: Sender<Stream>,
    written: Arc<Mutex<Vec<String>>>,
    killed: Arc<AtomicBool>,
    input_closed: Arc<AtomicBool>,
}

impl Handle {
    fn feed(&self, stream: &str) {
        for line in stream.lines() {
            let _ = self.tx.send(Stream::Stdout(line.to_string()));
        }
    }
    fn written(&self) -> Vec<String> {
        self.written.lock().unwrap().clone()
    }
}

struct FakeChild {
    written: Arc<Mutex<Vec<String>>>,
    killed: Arc<AtomicBool>,
    input_closed: Arc<AtomicBool>,
    tx: Sender<Stream>,
}

impl Child for FakeChild {
    fn write(&mut self, data: &str) -> Result<(), String> {
        self.written.lock().unwrap().push(data.to_string());
        Ok(())
    }
    fn finish_input(&mut self) {
        self.input_closed.store(true, Ordering::SeqCst);
    }
    fn kill(&mut self) {
        self.killed.store(true, Ordering::SeqCst);
        let _ = self.tx.send(Stream::Closed);
    }
}

#[derive(Default)]
struct FakeSpawner {
    specs: Mutex<Vec<Spec>>,
    handles: Mutex<Vec<Arc<Handle>>>,
}

impl FakeSpawner {
    fn handle(&self, index: usize) -> Arc<Handle> {
        self.handles.lock().unwrap()[index].clone()
    }
    fn count(&self) -> usize {
        self.specs.lock().unwrap().len()
    }
}

struct SharedSpawner(Arc<FakeSpawner>);

impl Spawner for SharedSpawner {
    fn spawn(&self, spec: &Spec) -> Result<(Box<dyn Child>, Receiver<Stream>), String> {
        let (tx, rx) = channel();
        let handle = Arc::new(Handle {
            tx: tx.clone(),
            written: Arc::default(),
            killed: Arc::default(),
            input_closed: Arc::default(),
        });
        self.0.specs.lock().unwrap().push(spec.clone());
        self.0.handles.lock().unwrap().push(handle.clone());
        Ok((
            Box::new(FakeChild {
                written: handle.written.clone(),
                killed: handle.killed.clone(),
                input_closed: handle.input_closed.clone(),
                tx,
            }),
            rx,
        ))
    }
}

#[derive(Default)]
struct Events(Mutex<Vec<(u64, Event)>>);

impl Sink for Events {
    fn emit(&self, turn: u64, event: Event) {
        self.0.lock().unwrap().push((turn, event));
    }
}

impl Events {
    fn all(&self) -> Vec<(u64, Event)> {
        self.0.lock().unwrap().clone()
    }
    fn kinds(&self) -> Vec<Event> {
        self.all().into_iter().map(|(_, e)| e).collect()
    }
}

/// Czeka (najwyżej 10 s) na warunek spełniany przez wątek odczytu.
fn wait_for(what: &str, condition: impl Fn() -> bool) {
    let started = Instant::now();
    while !condition() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "nie doczekano się: {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "todo-track-chat-test-{}-{name}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn config(engine: Engine, name: &str) -> Config {
    Config {
        engine,
        cli: Cli {
            path: match engine {
                Engine::Claude => "C:\\bin\\claude.exe".into(),
                Engine::Codex => "C:\\bin\\codex.cmd".into(),
            },
            version: "1.0".into(),
        },
        model: "m".into(),
        dir: temp_dir(name),
        mcp_exe: PathBuf::from("C:\\Apps\\todo-track-mcp.exe"),
    }
}

fn dict() -> Dictionary {
    let mut dict = Dictionary::new();
    dict.add_hidden_names(&["Jan Kowalski".to_string()]);
    dict
}

fn setup() -> (Manager, Arc<FakeSpawner>, Arc<Events>) {
    let spawner = Arc::new(FakeSpawner::default());
    (
        Manager::new(Arc::new(SharedSpawner(spawner.clone())), Arc::default()),
        spawner,
        Arc::new(Events::default()),
    )
}

#[test]
fn claude_message_is_redacted_started_once_and_streamed_back_restored() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "claude-flow");
    let sent = manager
        .send(&cfg, &dict(), "Ile pracował Jan Kowalski?", events.clone())
        .unwrap();
    // Do modelu idzie tekst po filtrze, nigdy nazwisko.
    assert_eq!(sent.sent, "Ile pracował Osoba 1?");
    assert_eq!(sent.turn, 1);
    assert_eq!(spawner.count(), 1);
    let spec = spawner.specs.lock().unwrap()[0].clone();
    assert_eq!(spec.program, PathBuf::from("C:\\bin\\claude.exe"));
    assert_eq!(spec.cwd.as_deref(), Some(cfg.dir.as_path()));
    assert!(spec
        .env
        .contains(&("TODO_TRACK_AI".to_string(), "1".to_string())));
    // Plik konfiguracji MCP leży w katalogu AI i wskazuje serwer.
    let file = std::fs::read_to_string(cfg.dir.join(MCP_FILE)).unwrap();
    assert!(file.contains("todo-track-mcp.exe"));
    let handle = spawner.handle(0);
    let written = handle.written();
    assert_eq!(written.len(), 1);
    assert!(written[0].ends_with('\n'));
    assert!(!written[0].contains("Kowalski"));
    let line: Value = serde_json::from_str(written[0].trim()).unwrap();
    assert_eq!(line["type"], "user");
    assert_eq!(
        line["message"]["content"][0]["text"],
        "Ile pracował Osoba 1?"
    );
    assert!(manager.busy());
    // Druga wiadomość w trakcie odpowiedzi jest odrzucona.
    assert_eq!(
        manager
            .send(&cfg, &dict(), "jeszcze jedno", events.clone())
            .unwrap_err(),
        BUSY
    );
    handle.feed(CLAUDE);
    wait_for("koniec odpowiedzi", || {
        events.kinds().contains(&Event::Done)
    });
    // Odpowiedź wraca z prawdziwym nazwiskiem; pseudonim nie dociera do UI.
    assert_eq!(
        events.kinds(),
        vec![
            Event::Text {
                text: "Sprawdzam dzień.".into()
            },
            Event::Tool {
                name: "day_summary".into(),
                input: json!({"day": "2026-10-20"}),
            },
            Event::ToolDone { error: false },
            Event::Text {
                text: "Jan Kowalski pracowała nad importem przez 2 h.".into()
            },
            Event::Done,
        ]
    );
    assert!(events.all().iter().all(|(turn, _)| *turn == 1));
    assert!(!manager.busy());
    // Kolejna wiadomość trafia do tego samego procesu, bez nowego startu.
    manager
        .send(&cfg, &dict(), "A wczoraj?", events.clone())
        .unwrap();
    assert_eq!(spawner.count(), 1);
    assert_eq!(handle.written().len(), 2);
    assert!(!handle.killed.load(Ordering::SeqCst));
}

#[test]
fn codex_starts_a_process_per_message_with_history_in_the_prompt() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Codex, "codex-flow");
    manager
        .send(&cfg, &dict(), "Co robił Jan Kowalski?", events.clone())
        .unwrap();
    let first = spawner.handle(0);
    assert!(first.input_closed.load(Ordering::SeqCst));
    let prompt = first.written().concat();
    assert!(prompt.starts_with(ROLE));
    assert!(prompt.contains("Nowe pytanie użytkownika:\nCo robił Osoba 1?"));
    assert!(!prompt.contains("Kowalski"));
    assert!(!prompt.contains("Dotychczasowa rozmowa"));
    let spec = spawner.specs.lock().unwrap()[0].clone();
    assert!(spec.args.contains(&"exec".to_string()));
    first.feed(CODEX);
    wait_for("koniec odpowiedzi", || {
        events.kinds().contains(&Event::Done)
    });
    assert!(events.kinds().contains(&Event::Text {
        text: "Jan Kowalski pracowała nad importem przez 2 h.".into()
    }));
    first.tx.send(Stream::Closed).unwrap();
    // Druga wiadomość: nowy proces i rozmowa z pierwszej tury w prompcie.
    manager
        .send(&cfg, &dict(), "A wczoraj?", events.clone())
        .unwrap();
    assert_eq!(spawner.count(), 2);
    let prompt = spawner.handle(1).written().concat();
    assert!(prompt.contains("Dotychczasowa rozmowa:\nUżytkownik: Co robił Osoba 1?"));
    assert!(prompt.contains("Asystent: Osoba 1 pracowała nad importem przez 2 h."));
    assert!(prompt.contains("Nowe pytanie użytkownika:\nA wczoraj?"));
    assert!(!prompt.contains("Kowalski"));
}

#[test]
fn stop_kills_the_process_keeps_partial_text_and_next_message_restarts_with_history() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "stop");
    manager
        .send(&cfg, &dict(), "Podsumuj dzień", events.clone())
        .unwrap();
    let handle = spawner.handle(0);
    handle
        .tx
        .send(Stream::Stdout(
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Zaczynam"}]}}"#
                .into(),
        ))
        .unwrap();
    wait_for("częściowy tekst", || !events.kinds().is_empty());
    manager.stop(None, false, events.as_ref());
    assert!(handle.killed.load(Ordering::SeqCst));
    assert!(!manager.busy());
    assert_eq!(events.kinds().last(), Some(&Event::Stopped));
    // Po zatrzymaniu spóźnione linie starego procesu nie dochodzą do UI.
    let before = events.all().len();
    handle.feed(CLAUDE);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(events.all().len(), before);
    // Nowa wiadomość: nowy proces, a pierwsza wiadomość niesie historię.
    manager
        .send(&cfg, &dict(), "Spróbuj jeszcze raz", events.clone())
        .unwrap();
    assert_eq!(spawner.count(), 2);
    let line: Value = serde_json::from_str(spawner.handle(1).written()[0].trim()).unwrap();
    let text = line["message"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Użytkownik: Podsumuj dzień"));
    assert!(text.contains("Asystent: Zaczynam"));
    assert!(text.contains("Nowe pytanie użytkownika:\nSpróbuj jeszcze raz"));
}

#[test]
fn killed_process_reports_an_error_and_next_message_starts_a_new_one() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "killed");
    manager
        .send(&cfg, &dict(), "Pytanie", events.clone())
        .unwrap();
    // Proces ubity z zewnątrz: strumień kończy się bez wyniku.
    spawner.handle(0).tx.send(Stream::Closed).unwrap();
    wait_for("błąd", || !events.kinds().is_empty());
    assert_eq!(
        events.kinds(),
        vec![Event::Error {
            message: UNEXPECTED_EXIT.into()
        }]
    );
    assert!(!manager.busy());
    manager
        .send(&cfg, &dict(), "Jeszcze raz", events.clone())
        .unwrap();
    assert_eq!(spawner.count(), 2);
}

#[test]
fn stderr_of_a_dead_process_explains_login_problems() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "login");
    manager
        .send(&cfg, &dict(), "Pytanie", events.clone())
        .unwrap();
    let handle = spawner.handle(0);
    handle
        .tx
        .send(Stream::Stderr(
            "Error: not logged in. Please run /login".into(),
        ))
        .unwrap();
    handle.tx.send(Stream::Closed).unwrap();
    wait_for("błąd", || !events.kinds().is_empty());
    assert_eq!(
        events.kinds(),
        vec![Event::Error {
            message: ai::failure_message(Engine::Claude, &Failure::NotLoggedIn)
        }]
    );
}

#[test]
fn error_result_ends_the_turn_and_keeps_the_process() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "limit");
    manager
        .send(&cfg, &dict(), "Pytanie", events.clone())
        .unwrap();
    let handle = spawner.handle(0);
    handle.feed(CLAUDE_LIMIT);
    wait_for("błąd", || !events.kinds().is_empty());
    assert_eq!(
        events.kinds(),
        vec![Event::Error {
            message: ai::failure_message(Engine::Claude, &Failure::Limit)
        }]
    );
    assert!(!manager.busy());
    assert!(!handle.killed.load(Ordering::SeqCst));
}

#[test]
fn reset_kills_the_process_and_forgets_history_but_keeps_the_shared_pseudonyms() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "reset");
    manager
        .send(&cfg, &dict(), "Jan Kowalski", events.clone())
        .unwrap();
    manager.reset();
    assert!(spawner.handle(0).killed.load(Ordering::SeqCst));
    assert!(!manager.busy());
    // Nowa rozmowa: bez historii w prompcie, a ta sama osoba ma ten sam pseudonim
    // (mapa jest wspólna z serwerem MCP przez całe działanie aplikacji).
    let mut other = Dictionary::new();
    other.add_hidden_names(&["Anna Nowak".to_string(), "Jan Kowalski".to_string()]);
    let sent = manager
        .send(&cfg, &other, "Anna Nowak i Jan Kowalski", events.clone())
        .unwrap();
    assert_eq!(sent.sent, "Osoba 2 i Osoba 1");
    let line: Value = serde_json::from_str(spawner.handle(1).written()[0].trim()).unwrap();
    assert_eq!(line["message"]["content"][0]["text"], "Osoba 2 i Osoba 1");
}

#[test]
fn chat_and_mcp_results_share_one_pseudonym_map() {
    // Mapa serwera MCP; czat dostaje ją w `setup` tak jak tutaj.
    let mcp = crate::mcp::State::default();
    let spawner = Arc::new(FakeSpawner::default());
    let manager = Manager::new(Arc::new(SharedSpawner(spawner.clone())), mcp.pseudonyms());
    let events = Arc::new(Events::default());
    let mut people = Dictionary::new();
    people.add_hidden_names(&["Jan Kowalski".to_string(), "Anna Nowak".to_string()]);
    let cfg = config(Engine::Claude, "shared-map");
    // Użytkownik pisze o Kowalskim: Osoba 1.
    let sent = manager
        .send(&cfg, &people, "Co mam z Janem Kowalskim?", events.clone())
        .unwrap();
    assert!(sent.sent.contains("Osoba 1"), "{}", sent.sent);
    // Wynik narzędzia MCP (task_get) z przypisaną Anną Nową: filtr serwera na tej
    // samej mapie daje kolejny numer, a nie drugie „Osoba 1”.
    let result = ai_privacy::redact_with(
        r#"{"assignee":"Anna Nowak"}"#,
        &people,
        &mut mcp.pseudonyms().lock().unwrap(),
    );
    assert!(result.contains("Osoba 2"), "{result}");
    // Model odpowiada o obu osobach i przekazuje pseudonim do narzędzia.
    let handle = spawner.handle(0);
    handle.feed(concat!(
        r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__todo-track__propose_note_append","input":{"task":1,"text":"pinguj Osoba 2"}}]}}"#,
        "\n",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Osoba 2 ma zadanie, o które pyta Osoba 1."}]}}"#,
        "\n",
        r#"{"type":"result","subtype":"success","is_error":false,"result":"x"}"#,
    ));
    wait_for("koniec odpowiedzi", || {
        events.kinds().contains(&Event::Done)
    });
    let kinds = events.kinds();
    assert!(kinds.contains(&Event::Text {
        text: "Anna Nowak ma zadanie, o które pyta Jan Kowalski.".into()
    }));
    assert!(kinds.contains(&Event::Tool {
        name: "propose_note_append".into(),
        input: json!({"task": 1, "text": "pinguj Anna Nowak"}),
    }));
    // Serwer MCP przywraca argumenty `propose_*` tą samą mapą: pseudonim z
    // wiadomości użytkownika wskazuje Kowalskiego, nie osobę z wyniku narzędzia.
    let ps = mcp.pseudonyms();
    let ps = ps.lock().unwrap();
    assert_eq!(ps.restore("Osoba 1"), "Jan Kowalski");
    assert_eq!(ps.restore("Osoba 2"), "Anna Nowak");
}

/// Spawner, który czeka na sygnał przed startem procesu (start trwa).
struct SlowSpawner {
    inner: SharedSpawner,
    started: Mutex<Option<Sender<()>>>,
    release: Mutex<Receiver<()>>,
}

impl Spawner for SlowSpawner {
    fn spawn(&self, spec: &Spec) -> Result<(Box<dyn Child>, Receiver<Stream>), String> {
        if let Some(tx) = self.started.lock().unwrap().take() {
            let _ = tx.send(());
        }
        let _ = self.release.lock().unwrap().recv();
        self.inner.spawn(spec)
    }
}

#[test]
fn stop_during_a_slow_start_does_not_wait_and_the_late_process_is_killed() {
    let fake = Arc::new(FakeSpawner::default());
    let (started_tx, started_rx) = channel();
    let (release_tx, release_rx) = channel();
    let manager = Arc::new(Manager::new(
        Arc::new(SlowSpawner {
            inner: SharedSpawner(fake.clone()),
            started: Mutex::new(Some(started_tx)),
            release: Mutex::new(release_rx),
        }),
        Arc::default(),
    ));
    let events = Arc::new(Events::default());
    let cfg = config(Engine::Claude, "slow-start");
    let sending = {
        let manager = manager.clone();
        let events = events.clone();
        std::thread::spawn(move || manager.send(&cfg, &dict(), "Pytanie", events))
    };
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // Start procesu trwa: „Zatrzymaj” i „Nowa rozmowa” nie czekają na blokadę.
    let begun = Instant::now();
    manager.stop(None, false, events.as_ref());
    manager.reset();
    assert!(begun.elapsed() < Duration::from_secs(2));
    assert!(!manager.busy());
    assert_eq!(events.kinds(), vec![Event::Stopped]);
    release_tx.send(()).unwrap();
    let sent = sending.join().unwrap().unwrap();
    assert_eq!(sent.turn, 1);
    // Spóźniony proces nie zostaje żywy, a odpowiedź nie trafia do UI.
    assert!(fake.handle(0).killed.load(Ordering::SeqCst));
    fake.handle(0).feed(CLAUDE);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(events.kinds(), vec![Event::Stopped]);
    assert!(!manager.busy());
}

#[test]
fn stop_of_a_given_turn_leaves_other_turns_and_can_drop_it_from_history() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "stop-turn");
    manager
        .send(&cfg, &dict(), "Pierwsze", events.clone())
        .unwrap();
    spawner.handle(0).feed(CLAUDE);
    wait_for("koniec odpowiedzi", || {
        events.kinds().contains(&Event::Done)
    });
    let second = manager
        .send(&cfg, &dict(), "Drugie", events.clone())
        .unwrap();
    // Spóźnione „Zatrzymaj” pierwszej tury nie rusza trwającej drugiej.
    manager.stop(Some(1), false, events.as_ref());
    assert!(manager.busy());
    assert!(!spawner.handle(0).killed.load(Ordering::SeqCst));
    // Zatrzymanie drugiej tury z usunięciem jej z historii (sprzed „Nowej rozmowy”).
    manager.stop(Some(second.turn), true, events.as_ref());
    assert!(!manager.busy());
    assert!(spawner.handle(0).killed.load(Ordering::SeqCst));
    manager
        .send(&cfg, &dict(), "Trzecie", events.clone())
        .unwrap();
    let line: Value = serde_json::from_str(spawner.handle(1).written()[0].trim()).unwrap();
    let text = line["message"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("Użytkownik: Pierwsze"), "{text}");
    assert!(!text.contains("Drugie"), "{text}");
}

#[test]
fn empty_message_is_rejected_without_starting_a_process() {
    let (manager, spawner, events) = setup();
    let cfg = config(Engine::Claude, "empty");
    assert_eq!(
        manager.send(&cfg, &dict(), "   ", events).unwrap_err(),
        EMPTY
    );
    assert_eq!(spawner.count(), 0);
}

// ---------------------------------------------------------------------------
// Prawdziwy proces zastępczy

/// Proces zastępczy: ta sama binarka testowa uruchomiona z `AI_CHAT_STAND_IN=1`
/// czyta linie ze stdin, na każdą odpowiada linią `result` i kończy się na
/// końcu wejścia albo na linii zawierającej „die” (ubity proces). Bez zmiennej
/// środowiska test nic nie robi.
#[test]
fn stand_in_process() {
    if std::env::var("AI_CHAT_STAND_IN").as_deref() != Ok("1") {
        return;
    }
    use std::io::BufRead;
    let stdin = std::io::stdin();
    for line in stdin.lock().lines().map_while(Result::ok) {
        if line.contains("die") {
            std::process::exit(3);
        }
        println!(r#"{{"type":"result","subtype":"success","is_error":false,"result":"echo"}}"#);
        let _ = std::io::stdout().flush();
    }
}

/// Spawner prawdziwych procesów, który zamiast CLI uruchamia proces zastępczy
/// i liczy starty.
struct StandIn {
    spec: Spec,
    starts: Arc<AtomicUsize>,
}

impl Spawner for StandIn {
    fn spawn(&self, _spec: &Spec) -> Result<(Box<dyn Child>, Receiver<Stream>), String> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        ProcessSpawner.spawn(&self.spec)
    }
}

fn stand_in_manager() -> (Manager, Arc<AtomicUsize>, Config) {
    let exe = std::env::current_exe().unwrap();
    let mut cfg = config(Engine::Claude, "stand-in");
    cfg.cli.path = exe.to_string_lossy().into_owned();
    let spec = Spec {
        program: exe,
        args: ["ai_chat::tests::stand_in_process", "--exact", "--nocapture"]
            .map(String::from)
            .to_vec(),
        stdin: String::new(),
        cwd: None,
        env: vec![("AI_CHAT_STAND_IN".into(), "1".into())],
    };
    let starts = Arc::new(AtomicUsize::new(0));
    let spawner = StandIn {
        spec,
        starts: starts.clone(),
    };
    (Manager::new(Arc::new(spawner), Arc::default()), starts, cfg)
}

fn errors(events: &Events) -> usize {
    events
        .kinds()
        .iter()
        .filter(|e| matches!(e, Event::Error { .. }))
        .count()
}

#[test]
fn real_process_starts_answers_and_is_reused() {
    let (manager, starts, cfg) = stand_in_manager();
    let events = Arc::new(Events::default());
    manager
        .send(&cfg, &dict(), "pierwsze", events.clone())
        .unwrap();
    wait_for("odpowiedź procesu", || {
        events.kinds().contains(&Event::Done)
    });
    assert_eq!(
        events.kinds(),
        vec![
            Event::Text {
                text: "echo".into()
            },
            Event::Done
        ]
    );
    manager
        .send(&cfg, &dict(), "drugie", events.clone())
        .unwrap();
    wait_for("druga odpowiedź", || {
        events.kinds().iter().filter(|e| **e == Event::Done).count() == 2
    });
    // Jeden proces na całą rozmowę.
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    manager.reset();
}

#[test]
fn real_process_killed_mid_turn_reports_an_error_and_stop_ends_the_next_one() {
    let (manager, starts, cfg) = stand_in_manager();
    // Proces kończy się w trakcie odpowiedzi (jak ubity z zewnątrz).
    let events = Arc::new(Events::default());
    manager.send(&cfg, &dict(), "die", events.clone()).unwrap();
    wait_for("błąd po ubitym procesie", || errors(&events) == 1);
    assert!(!manager.busy());
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    // Kolejna wiadomość startuje nowy proces; „Zatrzymaj” kończy go bez odpowiedzi.
    let events = Arc::new(Events::default());
    manager
        .send(&cfg, &dict(), "zwykła wiadomość", events.clone())
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    manager.stop(None, false, events.as_ref());
    assert!(!manager.busy());
    assert_eq!(events.kinds().last(), Some(&Event::Stopped));
    // Po zatrzymaniu proces nie żyje: następna wiadomość startuje trzeci.
    manager
        .send(&cfg, &dict(), "jeszcze", Arc::new(Events::default()))
        .unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 3);
    manager.reset();
}
