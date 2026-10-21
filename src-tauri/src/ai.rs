//! Silnik AI i wybór agenta (PLAN_BACKLOG.md, A1).
//!
//! Aplikacja nie ma klucza API: pyta model przez narzędzia CLI z subskrypcji
//! użytkownika, `claude` (Claude Code) albo `codex` (Codex CLI). Moduł:
//! - wykrywa oba CLI ([`detect`]; miejsca z planu, wersja z `--version`,
//!   ścieżki i `PATH` wstrzykiwane przez [`Locations`], więc testy nie dotykają
//!   prawdziwego systemu),
//! - decyduje o dialogu pierwszego uruchomienia ([`should_ask`]),
//! - buduje linie poleceń obu silników ([`claude_args`], [`codex_args`]),
//! - wykonuje wywołanie jednorazowe [`run`] / [`ask`]: wejście przez
//!   `ai_privacy::redact`, odpowiedź przez `restore`, walidacja ze schematem,
//!   pamięć podręczna (rodzaj + skrót wejścia), jeden proces na rodzaj,
//!   limit 60 s, błędy po polsku,
//! - trzyma dziennik ostatnich wywołań (rodzaj, czas, długość, wynik; bez treści).
//!
//! Uruchamianie procesu idzie przez cechę [`Runner`]; testy podstawiają zapisane
//! odpowiedzi i nigdy nie uruchamiają `claude` ani `codex`.
//!
//! Izolacja: proces dostaje katalog roboczy `<dane aplikacji>\ai\` (pusty, bez
//! CLAUDE.md i `.git`) oraz `TODO_TRACK_AI=1`. `todo-track-hook.exe` dziedziczy
//! środowisko i odrzuca zdarzenia z tym znacznikiem (`hook_wire::suppressed`),
//! a `agents.rs` dodatkowo ignoruje agenta z `cwd` równym katalogowi AI.
//! Model tylko proponuje: żadna funkcja tego modułu nie zmienia timera, TS,
//! Jiry ani Redmine.
#![allow(dead_code)]
use crate::ai_privacy::{self, Dictionary};
use crate::settings::AppSettings;
use crate::{db, hook_wire};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::Manager;

/// Nazwa katalogu roboczego wywołań AI w danych aplikacji.
pub const DIR_NAME: &str = "ai";
/// Limit czasu jednego wywołania.
pub const TIMEOUT: Duration = Duration::from_secs(60);
/// Limit czasu `--version` przy wykrywaniu.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);
/// Pusta konfiguracja MCP (`--mcp-config` razem z `--strict-mcp-config`).
const EMPTY_MCP: &str = r#"{"mcpServers":{}}"#;
/// Ile wpisów trzyma pamięć podręczna i dziennik wywołań.
const CACHE_MAX: usize = 100;
const CALLS_MAX: usize = 30;

pub const OFF: &str = "Asystent AI jest wyłączony. Wybierz silnik w Ustawieniach → Asystent AI.";
pub const NOT_FOUND_ANY: &str = "Nie znaleziono Claude Code ani Codexa. Zainstaluj jedno z nich albo podaj ścieżkę w Ustawieniach → Asystent AI.";
pub const TIMED_OUT: &str = "Asystent nie odpowiedział w ciągu 60 sekund.";
pub const BAD_SCHEMA: &str = "Odpowiedź asystenta jest niezgodna ze schematem.";
pub const BUSY: &str = "To wywołanie asystenta już trwa.";

// ---------------------------------------------------------------------------
// Silniki i wykrywanie

/// Silnik asystenta.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Claude,
    Codex,
}

impl Engine {
    /// Wartość ustawienia `ai_engine`; `""` i `off` to brak silnika.
    pub fn parse(value: &str) -> Option<Engine> {
        match value {
            "claude" => Some(Engine::Claude),
            "codex" => Some(Engine::Codex),
            _ => None,
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Engine::Claude => "claude",
            Engine::Codex => "codex",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Engine::Claude => "Claude Code",
            Engine::Codex => "Codex",
        }
    }
    fn login_hint(self) -> &'static str {
        match self {
            Engine::Claude => "uruchom `claude` w terminalu",
            Engine::Codex => "uruchom `codex login` w terminalu",
        }
    }
}

/// Wykryte CLI.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cli {
    pub path: String,
    pub version: String,
}

/// Wynik wykrywania: `None` = nie znaleziono działającego CLI.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Detected {
    pub claude: Option<Cli>,
    pub codex: Option<Cli>,
}

impl Detected {
    pub fn get(&self, engine: Engine) -> Option<&Cli> {
        match engine {
            Engine::Claude => self.claude.as_ref(),
            Engine::Codex => self.codex.as_ref(),
        }
    }
    pub fn any(&self) -> bool {
        self.claude.is_some() || self.codex.is_some()
    }
}

/// Miejsca, w których szukamy CLI (wstrzykiwane w testach).
#[derive(Clone, Debug, Default)]
pub struct Locations {
    /// `%USERPROFILE%`.
    pub user_profile: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
    /// `%APPDATA%` (npm global leży w `%APPDATA%\npm`).
    pub app_data: Option<PathBuf>,
    /// Katalogi z `PATH`.
    pub path: Vec<PathBuf>,
}

impl Locations {
    /// Z prawdziwego środowiska procesu.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        Self {
            user_profile: var("USERPROFILE"),
            local_app_data: var("LOCALAPPDATA"),
            app_data: var("APPDATA"),
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
        }
    }
}

/// Pliki wykonywalne w katalogach z `PATH` (`name.exe`, potem `name.cmd`).
fn on_path(loc: &Locations, name: &str) -> Vec<PathBuf> {
    loc.path
        .iter()
        .filter(|dir| !dir.as_os_str().is_empty())
        .flat_map(|dir| {
            [
                dir.join(format!("{name}.exe")),
                dir.join(format!("{name}.cmd")),
            ]
        })
        .collect()
}

fn configured(path: &str) -> Option<PathBuf> {
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Kolejność z planu: `%USERPROFILE%\.local\bin\claude.exe`, `PATH`, `ai_claude_path`.
pub fn claude_candidates(loc: &Locations, configured_path: &str) -> Vec<PathBuf> {
    let mut list = Vec::new();
    if let Some(home) = &loc.user_profile {
        list.push(home.join(".local").join("bin").join("claude.exe"));
    }
    list.extend(on_path(loc, "claude"));
    list.extend(configured(configured_path));
    dedup(list)
}

/// Kolejność z planu: `PATH`, znane miejsca (npm global, `%LOCALAPPDATA%\Programs\OpenAI\…`),
/// `ai_codex_path`.
pub fn codex_candidates(loc: &Locations, configured_path: &str) -> Vec<PathBuf> {
    let mut list = on_path(loc, "codex");
    if let Some(roaming) = &loc.app_data {
        let npm = roaming.join("npm");
        list.push(npm.join("codex.cmd"));
        list.push(npm.join("codex.exe"));
    }
    if let Some(local) = &loc.local_app_data {
        let openai = local.join("Programs").join("OpenAI");
        list.push(openai.join("codex").join("codex.exe"));
        list.push(openai.join("Codex").join("codex.exe"));
        list.push(openai.join("codex.exe"));
        list.push(openai.join("bin").join("codex.exe"));
    }
    list.extend(configured(configured_path));
    dedup(list)
}

fn dedup(list: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|p| seen.insert(p.to_string_lossy().to_lowercase()))
        .collect()
}

/// Wersja z wyjścia `--version`: pierwszy token wyglądający jak numer
/// (`2.1.304 (Claude Code)`, `codex-cli 0.162.1`), a bez niego pierwsza linia.
pub fn parse_version(output: &str) -> String {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty());
    let Some(line) = line else {
        return String::new();
    };
    line.split_whitespace()
        .map(|t| t.trim_start_matches('v'))
        .find(|t| {
            t.chars().next().is_some_and(|c| c.is_ascii_digit())
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                && t.contains('.')
        })
        .map(str::to_string)
        .unwrap_or_else(|| line.chars().take(60).collect())
}

/// Pierwsze istniejące CLI z listy, które odpowiada na `--version`. Także
/// wykrywanie działa w katalogu AI i ze znacznikiem dla hooka.
fn first_working(
    candidates: &[PathBuf],
    exists: &dyn Fn(&Path) -> bool,
    runner: &dyn Runner,
    dir: &Path,
) -> Option<Cli> {
    candidates.iter().filter(|p| exists(p)).find_map(|path| {
        let spec = Spec {
            program: path.clone(),
            args: vec!["--version".into()],
            stdin: String::new(),
            cwd: Some(dir.to_path_buf()),
            env: call_env(),
        };
        let output = runner.run(&spec, VERSION_TIMEOUT).ok()?;
        let version = parse_version(&output.stdout);
        (output.success && !version.is_empty()).then(|| Cli {
            path: path.to_string_lossy().into_owned(),
            version,
        })
    })
}

/// Wykrywa oba CLI. `exists` i `runner` są wstrzykiwane (testy); `dir` to
/// katalog AI (roboczy katalog `--version`).
pub fn detect(
    loc: &Locations,
    settings: &AppSettings,
    exists: &dyn Fn(&Path) -> bool,
    runner: &dyn Runner,
    dir: &Path,
) -> Detected {
    Detected {
        claude: first_working(
            &claude_candidates(loc, &settings.ai_claude_path),
            exists,
            runner,
            dir,
        ),
        codex: first_working(
            &codex_candidates(loc, &settings.ai_codex_path),
            exists,
            runner,
            dir,
        ),
    }
}

/// Dialog pierwszego uruchomienia: jeszcze nie pytano (`""`) i jest co wybrać.
/// Brak CLI = brak dialogu (Ustawienia pokazują, jak je zainstalować).
pub fn should_ask(engine: &str, detected: &Detected) -> bool {
    engine.is_empty() && detected.any()
}

/// Silnik, którym aplikacja faktycznie woła model: wybrany i wykryty.
pub fn active_engine(engine: &str, detected: &Detected) -> Option<Engine> {
    Engine::parse(engine).filter(|e| detected.get(*e).is_some())
}

// ---------------------------------------------------------------------------
// Uruchamianie procesu

/// Polecenie do uruchomienia.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Treść na stdin (prompt).
    pub stdin: String,
    /// Katalog roboczy (`None` = bieżący katalog aplikacji); aplikacja zawsze
    /// podaje katalog AI.
    pub cwd: Option<PathBuf>,
    /// Dodatkowe zmienne środowiska procesu.
    pub env: Vec<(String, String)>,
}

/// Wynik procesu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Błąd uruchomienia.
#[derive(Clone, Debug, PartialEq)]
pub enum RunError {
    Spawn(String),
    Timeout,
}

/// Uruchamia CLI; testy podstawiają zapisane odpowiedzi.
pub trait Runner: Send + Sync {
    fn run(&self, spec: &Spec, timeout: Duration) -> Result<Output, RunError>;
}

/// Prawdziwy proces, bez okna konsoli.
pub struct Process;

/// `taskkill /T /F /PID <pid>`: kończy proces razem z potomkami. Nakładka `.cmd`
/// (np. `%APPDATA%\npm\codex.cmd`) to `cmd.exe`, a model działa w procesie
/// potomnym, którego samo `kill` nie zatrzyma.
pub fn kill_tree_spec(pid: u32, system_root: Option<&Path>) -> (PathBuf, Vec<String>) {
    let program = system_root
        .map(|root| root.join("System32").join("taskkill.exe"))
        .unwrap_or_else(|| PathBuf::from("taskkill.exe"));
    let args = ["/T", "/F", "/PID", &pid.to_string()]
        .iter()
        .map(|s| s.to_string())
        .collect();
    (program, args)
}

/// Zabija drzewo procesów po przekroczeniu czasu (tylko [`Process`]).
fn kill_tree(pid: u32) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::{Command, Stdio};
        let root = std::env::var_os("SystemRoot").map(PathBuf::from);
        let (program, args) = kill_tree_spec(pid, root.as_deref());
        let _ = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x0800_0000)
            .status();
    }
    #[cfg(not(windows))]
    let _ = pid;
}

impl Runner for Process {
    fn run(&self, spec: &Spec, timeout: Duration) -> Result<Output, RunError> {
        use std::process::{Command, Stdio};
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = &spec.cwd {
            command.current_dir(cwd);
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: bez migającego okna konsoli.
            command.creation_flags(0x0800_0000);
        }
        let mut child = command
            .spawn()
            .map_err(|e| RunError::Spawn(e.to_string()))?;
        // Prompt na stdin w osobnym wątku (duży tekst nie zablokuje odczytu).
        if let Some(mut stdin) = child.stdin.take() {
            let text = spec.stdin.clone().into_bytes();
            std::thread::spawn(move || {
                let _ = stdin.write_all(&text);
            });
        }
        let (tx, rx) = std::sync::mpsc::channel::<(bool, String)>();
        let read = |pipe: Option<Box<dyn Read + Send>>, is_out: bool| {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut bytes = Vec::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_end(&mut bytes);
                }
                let _ = tx.send((is_out, String::from_utf8_lossy(&bytes).into_owned()));
            });
        };
        read(child.stdout.take().map(|p| Box::new(p) as _), true);
        read(child.stderr.take().map(|p| Box::new(p) as _), false);
        drop(tx);
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() > timeout => {
                    kill_tree(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(RunError::Timeout);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(40)),
                Err(e) => return Err(RunError::Spawn(e.to_string())),
            }
        };
        let mut output = Output {
            success: status.success(),
            ..Output::default()
        };
        let deadline = started + timeout;
        for _ in 0..2 {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok((true, text)) => output.stdout = text,
                Ok((false, text)) => output.stderr = text,
                Err(_) => return Err(RunError::Timeout),
            }
        }
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Linie poleceń

/// Poziom modelu: „szybki” (dopasowania, pytania o dane) albo „główny”
/// (Zamknięcie dnia, dłuższe rozmowy).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Fast,
    Main,
}

/// Model z ustawień dla poziomu; pusty = domyślny model CLI.
pub fn model_for(settings: &AppSettings, tier: Tier) -> &str {
    match tier {
        Tier::Fast => settings.ai_model_fast.trim(),
        Tier::Main => settings.ai_model_main.trim(),
    }
}

/// `claude -p --output-format json --json-schema <schemat> --tools "" --strict-mcp-config
/// --mcp-config <pusty> --setting-sources "" --permission-prompts none
/// --no-session-persistence [--model <model>]`; prompt idzie na stdin.
pub fn claude_args(schema: &Value, model: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--json-schema",
        &schema.to_string(),
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        EMPTY_MCP,
        "--setting-sources",
        "",
        "--permission-prompts",
        "none",
        "--no-session-persistence",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if !model.is_empty() {
        args.push("--model".into());
        args.push(model.into());
    }
    args
}

/// `codex exec --json --output-schema <plik> --ephemeral --skip-git-repo-check
/// -s read-only -C <katalog AI> -c mcp_servers={} -c notify=[] [-m <model>] -o <plik>`;
/// prompt idzie na stdin.
pub fn codex_args(schema_file: &Path, out_file: &Path, dir: &Path, model: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".into(),
        "--json".into(),
        "--output-schema".into(),
        schema_file.to_string_lossy().into_owned(),
        "--ephemeral".into(),
        "--skip-git-repo-check".into(),
        "-s".into(),
        "read-only".into(),
        "-C".into(),
        dir.to_string_lossy().into_owned(),
        // Bez serwerów MCP i powiadomień z ~/.codex/config.toml: sandbox
        // ogranicza tylko polecenia powłoki, nie narzędzia MCP (np. zapis w Jirze).
        "-c".into(),
        "mcp_servers={}".into(),
        "-c".into(),
        "notify=[]".into(),
    ];
    if !model.is_empty() {
        args.push("-m".into());
        args.push(model.into());
    }
    args.push("-o".into());
    args.push(out_file.to_string_lossy().into_owned());
    args
}

/// Zmienne środowiska każdego wywołania AI: znacznik dla hooka.
pub(crate) fn call_env() -> Vec<(String, String)> {
    vec![(hook_wire::AI_ENV.to_string(), "1".to_string())]
}

/// Nazwa pliku z rodzaju wywołania (litery, cyfry, `-`).
fn file_stem(kind: &str) -> String {
    let stem: String = kind
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(40)
        .collect();
    if stem.is_empty() {
        "call".into()
    } else {
        stem
    }
}

/// Polecenie jednego wywołania; `dir` to katalog AI.
pub fn build_spec(
    engine: Engine,
    cli: &Cli,
    dir: &Path,
    kind: &str,
    model: &str,
    schema: &Value,
    prompt: &str,
) -> Spec {
    let stem = file_stem(kind);
    let args = match engine {
        Engine::Claude => claude_args(schema, model),
        Engine::Codex => codex_args(
            &dir.join(format!("schema-{stem}.json")),
            &dir.join(format!("last-{stem}.txt")),
            dir,
            model,
        ),
    };
    Spec {
        program: PathBuf::from(&cli.path),
        args,
        stdin: prompt.to_string(),
        cwd: Some(dir.to_path_buf()),
        env: call_env(),
    }
}

// ---------------------------------------------------------------------------
// Odpowiedzi i błędy

/// Powód nieudanego wywołania po stronie CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    NotLoggedIn,
    Limit,
    /// Inny błąd CLI (treść nie trafia do interfejsu).
    Other,
    /// Odpowiedź nie jest JSON-em.
    Unreadable,
}

/// Rodzaj błędu w dzienniku wywołań (bez treści).
fn failure_code(f: &Failure) -> &'static str {
    match f {
        Failure::NotLoggedIn => "login",
        Failure::Limit => "limit",
        Failure::Other => "error",
        Failure::Unreadable => "schema",
    }
}

/// Błąd CLI po polsku. Treść błędu CLI nie jest przekazywana dalej (mogłaby
/// zawierać fragmenty promptu).
pub fn failure_message(engine: Engine, failure: &Failure) -> String {
    match failure {
        Failure::NotLoggedIn => format!(
            "{} nie jest zalogowany. Zaloguj się: {}.",
            engine.name(),
            engine.login_hint()
        ),
        Failure::Limit => format!(
            "Limit subskrypcji {} jest wyczerpany. Spróbuj później.",
            engine.name()
        ),
        Failure::Other => format!("{} zakończył się błędem.", engine.name()),
        Failure::Unreadable => BAD_SCHEMA.to_string(),
    }
}

/// Rozpoznaje błąd po tekście zwróconym przez CLI (limit przed logowaniem:
/// komunikat o limicie bywa długi i zawiera słowa o koncie).
pub fn classify(text: &str) -> Failure {
    let lower = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has(&[
        "usage limit",
        "limit reached",
        "rate limit",
        "rate_limit",
        "quota",
        "hit your limit",
        "too many requests",
        "429",
        "overloaded_limit",
    ]) {
        Failure::Limit
    } else if has(&[
        "not logged in",
        "not authenticated",
        "/login",
        "invalid api key",
        "authentication",
        "unauthorized",
        "401",
        "log in",
        "login",
        "sign in",
    ]) {
        Failure::NotLoggedIn
    } else {
        Failure::Other
    }
}

/// JSON z tekstu odpowiedzi modelu: czysty JSON albo JSON w bloku ``` .
fn extract_json(text: &str) -> Option<Value> {
    let text = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }
    let inner = text.strip_prefix("```")?;
    let inner = inner.split_once('\n').map(|(_, rest)| rest)?;
    let inner = inner.rsplit_once("```").map(|(body, _)| body)?;
    serde_json::from_str(inner.trim()).ok()
}

/// Odpowiedź `claude -p --output-format json`: obiekt (albo lista zdarzeń)
/// z wynikiem `type: "result"`. Przy `--json-schema` dane są w
/// `structured_output`, a w razie braku w tekście `result`.
pub fn parse_claude(stdout: &str) -> Result<Value, Failure> {
    let value: Value = serde_json::from_str(stdout.trim()).map_err(|_| classify(stdout))?;
    let result = match &value {
        Value::Array(items) => items
            .iter()
            .rev()
            .find(|i| i["type"] == "result")
            .cloned()
            .ok_or(Failure::Unreadable)?,
        other => other.clone(),
    };
    let text = result["result"].as_str().unwrap_or_default();
    let subtype = result["subtype"].as_str().unwrap_or_default();
    if result["is_error"] == true || subtype.starts_with("error") {
        return Err(classify(&format!("{text} {subtype}")));
    }
    if !result["structured_output"].is_null() {
        return Ok(result["structured_output"].clone());
    }
    extract_json(text).ok_or(Failure::Unreadable)
}

/// Tekst z jednego zdarzenia `codex exec --json`: wiadomość agenta albo błąd.
fn codex_event_text(event: &Value) -> Option<(bool, String)> {
    let kind = event["type"].as_str().unwrap_or_default();
    let item = &event["item"];
    let msg = &event["msg"];
    if kind == "error" || kind == "turn.failed" || kind == "stream_error" {
        let text = event["message"]
            .as_str()
            .or_else(|| event["error"]["message"].as_str())
            .or_else(|| event["error"].as_str())
            .unwrap_or("error");
        return Some((true, text.to_string()));
    }
    let message_item = if item["type"] == "agent_message" {
        Some(item)
    } else if msg["type"] == "agent_message" {
        Some(msg)
    } else {
        None
    }?;
    let text = message_item["text"]
        .as_str()
        .or_else(|| message_item["message"].as_str())?;
    Some((false, text.to_string()))
}

/// Wynik `codex exec --json`: JSONL zdarzeń. Ostatnia wiadomość agenta (albo
/// zawartość pliku z `-o`, gdy jest) to JSON zgodny ze schematem; zdarzenie
/// `error` / `turn.failed` bez wiadomości to błąd.
pub fn parse_codex(stdout: &str, last_message: Option<&str>) -> Result<Value, Failure> {
    let mut message: Option<String> = None;
    let mut error: Option<String> = None;
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match codex_event_text(&event) {
            Some((false, text)) => message = Some(text),
            Some((true, text)) => error = Some(text),
            None => {}
        }
    }
    let file = last_message.map(str::trim).filter(|t| !t.is_empty());
    match file.map(str::to_string).or(message) {
        Some(text) => extract_json(&text).ok_or(Failure::Unreadable),
        None => Err(classify(error.as_deref().unwrap_or(stdout))),
    }
}

// ---------------------------------------------------------------------------
// Walidacja ze schematem

fn json_type(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) if n.is_i64() || n.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_matches(expected: &str, value: &Value) -> bool {
    let actual = json_type(value);
    actual == expected || (expected == "number" && actual == "integer")
}

/// Sprawdza wartość ze schematem (podzbiór JSON Schema używany przez aplikację:
/// `type`, `enum`, `const`, `properties`, `required`, `additionalProperties`,
/// `items`, `minItems`, `maxItems`, `minLength`, `maxLength`, `minimum`,
/// `maximum`, `anyOf`). Zwraca ścieżkę i powód pierwszego błędu.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    check(schema, value, "$")
}

fn check(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };
    if let Some(types) = schema.get("type") {
        let ok = match types {
            Value::String(t) => type_matches(t, value),
            Value::Array(list) => list
                .iter()
                .filter_map(Value::as_str)
                .any(|t| type_matches(t, value)),
            _ => true,
        };
        if !ok {
            return Err(format!("{path}: zły typ ({})", json_type(value)));
        }
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        if !options.contains(value) {
            return Err(format!("{path}: wartość spoza listy"));
        }
    }
    if let Some(expected) = schema.get("const") {
        if expected != value {
            return Err(format!("{path}: inna wartość stała"));
        }
    }
    if let Some(options) = schema.get("anyOf").and_then(Value::as_array) {
        if !options.iter().any(|s| check(s, value, path).is_ok()) {
            return Err(format!("{path}: żaden wariant nie pasuje"));
        }
    }
    match value {
        Value::Object(map) => {
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for name in required.iter().filter_map(Value::as_str) {
                    if !map.contains_key(name) {
                        return Err(format!("{path}.{name}: brak pola"));
                    }
                }
            }
            let props = schema.get("properties").and_then(Value::as_object);
            for (name, field) in map {
                match props.and_then(|p| p.get(name)) {
                    Some(sub) => check(sub, field, &format!("{path}.{name}"))?,
                    None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                        return Err(format!("{path}.{name}: nieznane pole"));
                    }
                    None => {}
                }
            }
        }
        Value::Array(items) => {
            let count = items.len() as u64;
            if schema
                .get("minItems")
                .and_then(Value::as_u64)
                .is_some_and(|m| count < m)
                || schema
                    .get("maxItems")
                    .and_then(Value::as_u64)
                    .is_some_and(|m| count > m)
            {
                return Err(format!("{path}: zła liczba elementów"));
            }
            if let Some(sub) = schema.get("items") {
                for (i, item) in items.iter().enumerate() {
                    check(sub, item, &format!("{path}[{i}]"))?;
                }
            }
        }
        Value::String(text) => {
            let len = text.chars().count() as u64;
            if schema
                .get("minLength")
                .and_then(Value::as_u64)
                .is_some_and(|m| len < m)
                || schema
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .is_some_and(|m| len > m)
            {
                return Err(format!("{path}: zła długość tekstu"));
            }
        }
        Value::Number(n) => {
            let x = n.as_f64().unwrap_or(0.0);
            if schema
                .get("minimum")
                .and_then(Value::as_f64)
                .is_some_and(|m| x < m)
                || schema
                    .get("maximum")
                    .and_then(Value::as_f64)
                    .is_some_and(|m| x > m)
            {
                return Err(format!("{path}: liczba poza zakresem"));
            }
        }
        _ => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Stan: wykrycie, dziennik, pamięć podręczna, blokada rodzaju

/// Wpis „Ostatnich wywołań”: bez treści promptu i odpowiedzi.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    pub kind: String,
    /// Początek wywołania (ms).
    pub at: i64,
    /// Długość wysłanego tekstu (znaki, po filtrze).
    pub length: usize,
    pub ok: bool,
    /// Rodzaj błędu (`login`, `limit`, `timeout`, `schema`, `busy`, `spawn`,
    /// `error`, `off`), pusty przy sukcesie.
    pub error: String,
    /// Wynik z pamięci podręcznej (bez procesu).
    pub cached: bool,
}

/// Stan modułu (jeden na aplikację).
#[derive(Default)]
pub struct State {
    detected: Mutex<Option<Detected>>,
    calls: Mutex<VecDeque<Call>>,
    cache: Mutex<HashMap<String, Value>>,
    running: Mutex<HashSet<String>>,
}

impl State {
    pub fn detected(&self) -> Option<Detected> {
        self.detected.lock().ok().and_then(|d| d.clone())
    }
    pub fn set_detected(&self, detected: Detected) {
        if let Ok(mut d) = self.detected.lock() {
            *d = Some(detected);
        }
    }
    /// Dziennik od najnowszego wpisu.
    pub fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .map(|c| c.iter().rev().cloned().collect())
            .unwrap_or_default()
    }
    fn record(&self, call: Call) {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push_back(call);
            while calls.len() > CALLS_MAX {
                calls.pop_front();
            }
        }
    }
    fn cached(&self, key: &str) -> Option<Value> {
        self.cache.lock().ok().and_then(|c| c.get(key).cloned())
    }
    fn store(&self, key: String, value: Value) {
        if let Ok(mut cache) = self.cache.lock() {
            if cache.len() >= CACHE_MAX {
                cache.clear();
            }
            cache.insert(key, value);
        }
    }
    /// Zajmuje rodzaj wywołania; `None`, gdy proces tego rodzaju już trwa.
    fn acquire(&self, kind: &str) -> Option<Running<'_>> {
        let mut running = self.running.lock().ok()?;
        running.insert(kind.to_string()).then(|| Running {
            state: self,
            kind: kind.to_string(),
        })
    }
}

/// Zwalnia rodzaj wywołania po zakończeniu (także przy panice).
struct Running<'a> {
    state: &'a State,
    kind: String,
}
impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut running) = self.state.running.lock() {
            running.remove(&self.kind);
        }
    }
}

/// Skrót wejścia do pamięci podręcznej: rodzaj, silnik, model, schemat i tekst po filtrze.
fn cache_key(kind: &str, engine: Engine, model: &str, schema: &Value, prompt: &str) -> String {
    let mut hash = Sha256::new();
    for part in [kind, engine.id(), model, &schema.to_string(), prompt] {
        hash.update(part.as_bytes());
        hash.update([0]);
    }
    let digest = hash.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("{kind}:{hex}")
}

/// Wszystko, czego potrzebuje jedno wywołanie poza promptem.
pub struct Config {
    /// Ustawienie `ai_engine` (`""`, `claude`, `codex`, `off`).
    pub engine: String,
    pub detected: Detected,
    pub model: String,
    /// Katalog AI (roboczy katalog procesu).
    pub dir: PathBuf,
}

/// Wywołanie jednorazowe na dowolnym `Runner` (testowalny rdzeń [`ask`]).
/// `prompt` przechodzi przez `ai_privacy::redact`, odpowiedź przez `restore`.
#[allow(clippy::too_many_arguments)]
pub fn run(
    state: &State,
    runner: &dyn Runner,
    cfg: &Config,
    kind: &str,
    prompt: &str,
    schema: &Value,
    dict: &Dictionary,
    now: i64,
) -> Result<Value, String> {
    let mut call = Call {
        kind: kind.to_string(),
        at: now,
        length: 0,
        ok: false,
        error: String::new(),
        cached: false,
    };
    let result = run_inner(state, runner, cfg, kind, prompt, schema, dict, &mut call);
    call.ok = result.is_ok();
    state.record(call);
    result
}

#[allow(clippy::too_many_arguments)]
fn run_inner(
    state: &State,
    runner: &dyn Runner,
    cfg: &Config,
    kind: &str,
    prompt: &str,
    schema: &Value,
    dict: &Dictionary,
    call: &mut Call,
) -> Result<Value, String> {
    // Wpis dziennika dostaje rodzaj błędu, użytkownik komunikat po polsku.
    let fail = |call: &mut Call, code: &'static str, message: String| -> String {
        call.error = code.to_string();
        message
    };
    let Some(engine) = Engine::parse(&cfg.engine) else {
        return Err(fail(call, "off", OFF.to_string()));
    };
    let Some(cli) = cfg.detected.get(engine) else {
        let message = if cfg.detected.any() {
            format!(
                "Nie znaleziono {}. Wybierz inny silnik albo podaj ścieżkę w Ustawieniach → Asystent AI.",
                engine.name()
            )
        } else {
            NOT_FOUND_ANY.to_string()
        };
        return Err(fail(call, "missing", message));
    };
    // Wejście zawsze po filtrze danych osobowych.
    let (text, pseudonyms) = ai_privacy::redact(prompt, dict);
    call.length = text.chars().count();
    let key = cache_key(kind, engine, &cfg.model, schema, &text);
    // Pamięć trzyma odpowiedź z pseudonimami; ten sam tekst po filtrze to ta
    // sama numeracja „Osoba N”, więc przywracają ją pseudonimy tego wywołania
    // (prompty różniące się tylko osobą dają ten sam klucz).
    if let Some(hit) = state.cached(&key) {
        call.cached = true;
        return Ok(pseudonyms.restore_value(&hit));
    }
    let Some(_guard) = state.acquire(kind) else {
        return Err(fail(call, "busy", BUSY.to_string()));
    };
    let stem = file_stem(kind);
    let schema_file = cfg.dir.join(format!("schema-{stem}.json"));
    let out_file = cfg.dir.join(format!("last-{stem}.txt"));
    std::fs::create_dir_all(&cfg.dir).map_err(|e| {
        fail(
            call,
            "spawn",
            format!("Nie można utworzyć katalogu asystenta: {e}"),
        )
    })?;
    if engine == Engine::Codex {
        std::fs::write(&schema_file, schema.to_string())
            .map_err(|e| fail(call, "spawn", format!("Nie można zapisać schematu: {e}")))?;
        let _ = std::fs::remove_file(&out_file);
    }
    let spec = build_spec(engine, cli, &cfg.dir, kind, &cfg.model, schema, &text);
    let output = runner.run(&spec, TIMEOUT).map_err(|e| match e {
        RunError::Timeout => fail(call, "timeout", TIMED_OUT.to_string()),
        RunError::Spawn(e) => fail(
            call,
            "spawn",
            format!("Nie udało się uruchomić {}: {e}", engine.name()),
        ),
    })?;
    let parsed = match engine {
        Engine::Claude => parse_claude(&output.stdout),
        Engine::Codex => {
            let last = std::fs::read_to_string(&out_file).ok();
            let _ = std::fs::remove_file(&out_file);
            parse_codex(&output.stdout, last.as_deref())
        }
    };
    let value = match parsed {
        Ok(value) => value,
        // Brak czytelnej odpowiedzi przy błędzie procesu: powód z komunikatów CLI.
        Err(Failure::Unreadable) | Err(Failure::Other) if !output.success => {
            let failure = classify(&format!("{} {}", output.stderr, output.stdout));
            return Err(fail(
                call,
                failure_code(&failure),
                failure_message(engine, &failure),
            ));
        }
        Err(failure) => {
            return Err(fail(
                call,
                failure_code(&failure),
                failure_message(engine, &failure),
            ));
        }
    };
    if validate(schema, &value).is_err() {
        return Err(fail(call, "schema", BAD_SCHEMA.to_string()));
    }
    // Odpowiedź wraca do prawdziwych nazw dopiero tu, już po walidacji.
    let restored = pseudonyms.restore_value(&value);
    state.store(key, value);
    Ok(restored)
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

/// Katalog roboczy wywołań AI w danych aplikacji.
pub fn dir(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(DIR_NAME))
}

/// Czy `cwd` to katalog AI (porównanie jak przypisania folderów).
pub fn is_ai_dir(cwd: &str, ai_dir: &Path) -> bool {
    let cwd = crate::agents::norm_folder(cwd);
    !cwd.is_empty() && cwd == crate::agents::norm_folder(&ai_dir.to_string_lossy())
}

/// Słownik filtra: ustawienia, osoby z bazy oraz własne imię i nazwisko z konta
/// Jiry i Redmine (NEXT_SESSION.md, uwaga A0).
pub fn dictionary(app: &tauri::AppHandle, settings: &AppSettings) -> Dictionary {
    let mut extra: Vec<String> = Vec::new();
    if let Ok(config) = crate::jira::config(app) {
        extra.push(config.display_name);
    }
    if let Ok(config) = crate::redmine::config(app) {
        extra.push(config.display_name);
    }
    let fallback = |settings: &AppSettings, extra: &[String]| {
        let mut dict = Dictionary::new();
        dict.add_hidden_names(&settings.ai_hidden_names);
        dict.add_hidden_names(extra);
        dict
    };
    let state = app.state::<crate::Database>();
    let Ok(conn) = state.0.lock() else {
        return fallback(settings, &extra);
    };
    Dictionary::from_db(&conn, settings, &extra).unwrap_or_else(|_| fallback(settings, &extra))
}

/// Wykrywa CLI w prawdziwym systemie i zapamiętuje wynik.
pub fn detect_now(app: &tauri::AppHandle) -> Detected {
    let settings = crate::settings::current(app);
    // Bez katalogu AI `--version` nie rusza (żaden proces poza tym katalogiem).
    let Some(ai_dir) = dir(app).filter(|d| std::fs::create_dir_all(d).is_ok()) else {
        return Detected::default();
    };
    let detected = detect(
        &Locations::from_env(),
        &settings,
        &|p| p.is_file(),
        &Process,
        &ai_dir,
    );
    if let Some(state) = app.try_state::<State>() {
        state.set_detected(detected.clone());
    }
    detected
}

/// Wykryte CLI z pamięci; pierwsze zapytanie wykrywa (lazy, poza wątkiem UI).
pub(crate) fn detected_cached(app: &tauri::AppHandle) -> Detected {
    app.try_state::<State>()
        .and_then(|s| s.detected())
        .unwrap_or_else(|| detect_now(app))
}

/// `ai::ask(kind, prompt, schema)` z planu: wywołuje wybrany silnik z
/// ustawień. Wołać tylko na zdarzenie (nigdy w pętli) i poza wątkiem UI.
pub fn ask(
    app: &tauri::AppHandle,
    kind: &str,
    tier: Tier,
    prompt: &str,
    schema: &Value,
) -> Result<Value, String> {
    let settings = crate::settings::current(app);
    let dir = dir(app).ok_or("Nie można ustalić katalogu danych aplikacji.")?;
    let cfg = Config {
        engine: settings.ai_engine.clone(),
        detected: detected_cached(app),
        model: model_for(&settings, tier).to_string(),
        dir,
    };
    let dict = dictionary(app, &settings);
    let state = app
        .try_state::<State>()
        .ok_or("Asystent AI nie jest gotowy.")?;
    run(
        &state,
        &Process,
        &cfg,
        kind,
        prompt,
        schema,
        &dict,
        db::now(),
    )
}

/// Stan dla dialogu pierwszego uruchomienia i Ustawień.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub detected: Detected,
    pub engine: String,
    /// Pokaż dialog pierwszego uruchomienia.
    pub ask: bool,
    /// Wybrany silnik jest wykryty (funkcje AI mogą działać).
    pub active: bool,
    /// Katalog roboczy wywołań AI.
    pub dir: String,
}

pub fn status_of(settings: &AppSettings, detected: Detected, dir: &Path) -> Status {
    Status {
        ask: should_ask(&settings.ai_engine, &detected),
        active: active_engine(&settings.ai_engine, &detected).is_some(),
        engine: settings.ai_engine.clone(),
        dir: dir.to_string_lossy().into_owned(),
        detected,
    }
}

fn status(app: &tauri::AppHandle, detected: Detected) -> Status {
    let settings = crate::settings::current(app);
    status_of(&settings, detected, &dir(app).unwrap_or_default())
}

/// Rejestruje stan modułu; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Wykryte CLI i wybrany silnik; pierwsze wywołanie wykrywa CLI.
#[tauri::command]
pub async fn ai_status(app: tauri::AppHandle) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let detected = detected_cached(&app);
        status(&app, detected)
    })
    .await
    .map_err(|e| e.to_string())
}

/// „Sprawdź ponownie”: wykrywa CLI od nowa (po zmianie ścieżek albo instalacji).
#[tauri::command]
pub async fn ai_detect(app: tauri::AppHandle) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let detected = detect_now(&app);
        status(&app, detected)
    })
    .await
    .map_err(|e| e.to_string())
}

/// Ostatnie wywołania (bez treści), od najnowszego.
#[tauri::command]
pub fn ai_calls(app: tauri::AppHandle) -> Vec<Call> {
    app.try_state::<State>()
        .map(|s| s.calls())
        .unwrap_or_default()
}

/// „Pokaż, co zostanie wysłane”: tekst po filtrze danych osobowych; nic nie
/// wywołuje i nic nie wysyła.
#[tauri::command]
pub async fn ai_redact_preview(app: tauri::AppHandle, text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::current(&app);
        let dict = dictionary(&app, &settings);
        ai_privacy::redact(&text, &dict).0
    })
    .await
    .map_err(|e| e.to_string())
}

/// Prompt próbnego wywołania z Ustawień (to samo pokazuje „Pokaż, co zostanie wysłane”).
pub const TEST_PROMPT: &str =
    "Test połączenia z aplikacją Todo Track. Odpowiedz wyłącznie obiektem JSON {\"ok\": true}.";

/// Schemat odpowiedzi próbnego wywołania.
pub fn test_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "ok": { "type": "boolean" } },
        "required": ["ok"],
        "additionalProperties": false
    })
}

/// „Przetestuj silnik”: jedno próbne wywołanie po kliknięciu w Ustawieniach.
#[tauri::command]
pub async fn ai_test(app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let value = ask(&app, "test", Tier::Fast, TEST_PROMPT, &test_schema())?;
        if value["ok"] == true {
            Ok("Asystent odpowiedział poprawnie.".to_string())
        } else {
            Err(BAD_SCHEMA.to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
