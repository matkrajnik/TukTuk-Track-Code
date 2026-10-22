//! Rejestracja serwera MCP w Claude Code i Codexie (PLAN_BACKLOG.md, A2).
//!
//! Przez polecenia CLI, nie przez edycję plików agentów: `claude mcp add|remove|get`
//! i `codex mcp add|remove|list`. Budowa poleceń, parsowanie `mcp get` / `mcp list`
//! i decyzja, co zrobić, są czystymi funkcjami; uruchamianie idzie przez
//! [`Runner`](crate::ai::Runner), więc testy nie wywołują prawdziwego CLI.

use crate::ai::{Cli, Detected, Engine, Output, RunError, Runner, Spec};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Nazwa serwera w rejestracji CLI.
pub const SERVER: &str = crate::mcp_wire::SERVER_NAME;
/// Program serwera obok aplikacji.
pub const EXE: &str = "tuktuk-track-mcp.exe";
/// Najdłuższy czas jednego polecenia CLI (`claude mcp get` sprawdza połączenie).
pub const TIMEOUT: Duration = Duration::from_secs(30);

/// `mcp add`: rejestracja serwera na stdio wskazującego `exe`.
pub fn add_args(engine: Engine, exe: &Path) -> Vec<String> {
    let exe = exe.to_string_lossy().into_owned();
    let words: Vec<&str> = match engine {
        Engine::Claude => vec!["mcp", "add", "--scope", "user", SERVER, "--"],
        Engine::Codex => vec!["mcp", "add", SERVER, "--"],
    };
    words
        .into_iter()
        .map(str::to_string)
        .chain(std::iter::once(exe))
        .collect()
}

/// `mcp remove`.
pub fn remove_args(engine: Engine) -> Vec<String> {
    remove_args_named(engine, SERVER)
}

fn remove_args_named(engine: Engine, name: &str) -> Vec<String> {
    let words: Vec<&str> = match engine {
        Engine::Claude => vec!["mcp", "remove", "--scope", "user", name],
        Engine::Codex => vec!["mcp", "remove", name],
    };
    words.into_iter().map(str::to_string).collect()
}

/// Polecenie odczytujące rejestrację: `claude mcp get tuktuk-track` i `codex mcp list`.
pub fn query_args(engine: Engine) -> Vec<String> {
    let words: &[&str] = match engine {
        Engine::Claude => &["mcp", "get", SERVER],
        Engine::Codex => &["mcp", "list"],
    };
    words.iter().map(|s| s.to_string()).collect()
}

/// Rejestracja odczytana z CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum Found {
    /// Serwer nie jest zarejestrowany.
    Missing,
    /// Zarejestrowany; polecenie (ścieżka programu) z rejestracji.
    Registered(String),
}

/// Odczytuje polecenie z wyjścia `claude mcp get tuktuk-track`: wiersz `Command:`.
/// `None` = wyjście nie wygląda na wpis serwera.
pub fn parse_claude_get(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("Command:")?;
        let command = rest.trim();
        (!command.is_empty()).then(|| command.to_string())
    })
}

/// Odczytuje polecenie serwera z wyjścia `codex mcp list`: wiersz zaczynający się
/// od nazwy serwera; polecenie to tekst do końca `.exe` (ścieżka może mieć
/// spacje) albo pierwsze słowo. `None` = serwera nie ma na liście.
pub fn parse_codex_list(output: &str) -> Option<String> {
    parse_codex_list_named(output, SERVER)
}

fn parse_codex_list_named(output: &str, name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix(name)?;
        // Nazwa musi być całym pierwszym słowem (nie `tuktuk-track-extra`).
        if !rest.chars().next().is_some_and(char::is_whitespace) {
            return None;
        }
        let rest = rest.trim_start().trim_start_matches('"');
        let lower = rest.to_lowercase();
        let end = lower
            .find(".exe")
            .map(|i| i + ".exe".len())
            .or_else(|| rest.find(char::is_whitespace))
            .unwrap_or(rest.len());
        let command = rest[..end].trim().trim_end_matches('"');
        (!command.is_empty()).then(|| command.to_string())
    })
}

/// Czy wyjście `claude mcp get` mówi, że serwera nie ma.
fn claude_missing(output: &str) -> bool {
    output.to_lowercase().contains("no mcp server found")
}

/// Ścieżka do porównania: bez cudzysłowów i prefiksu `\\?\`, ukośniki `\`, małe litery.
pub fn norm_path(path: &str) -> String {
    let trimmed = path.trim().trim_matches('"');
    let trimmed = trimmed.strip_prefix(r"\\?\").unwrap_or(trimmed);
    trimmed
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

pub fn same_path(a: &str, b: &str) -> bool {
    !a.trim().is_empty() && norm_path(a) == norm_path(b)
}

/// Co zrobić z rejestracją w jednym CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nic (już zgodna albo już usunięta).
    Nothing,
    Add,
    /// Rejestracja wskazuje inny program (po aktualizacji instalatorem): usuń i dodaj.
    Replace,
    Remove,
}

/// Decyzja: `want` = serwer ma być zarejestrowany; `exe` = aktualny program.
pub fn plan_step(want: bool, found: &Found, exe: Option<&Path>) -> Step {
    match (want, found) {
        (false, Found::Missing) => Step::Nothing,
        (false, Found::Registered(_)) => Step::Remove,
        (true, Found::Missing) => Step::Add,
        (true, Found::Registered(command)) => match exe {
            Some(exe) if same_path(command, &exe.to_string_lossy()) => Step::Nothing,
            _ => Step::Replace,
        },
    }
}

/// Stan rejestracji w jednym CLI do pokazania w Ustawieniach.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// `claude` albo `codex`.
    pub engine: String,
    pub name: String,
    /// `registered`, `missing`, `error`, `unavailable` (brak CLI).
    pub state: String,
    pub detail: String,
}

fn entry(engine: Engine, state: &str, detail: impl Into<String>) -> Entry {
    Entry {
        engine: engine.id().into(),
        name: engine.name().into(),
        state: state.into(),
        detail: detail.into(),
    }
}

fn spec(cli: &Cli, args: Vec<String>, dir: &Path) -> Spec {
    Spec {
        program: PathBuf::from(&cli.path),
        args,
        stdin: String::new(),
        cwd: Some(dir.to_path_buf()),
        // Znacznik jak przy każdym uruchomieniu CLI przez aplikację (hook pomija sesję).
        env: crate::ai::call_env(),
    }
}

fn error_text(output: &Output) -> String {
    let text = if output.stderr.trim().is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("polecenie nie powiodło się")
        .chars()
        .take(200)
        .collect()
}

fn run_error(e: RunError) -> String {
    match e {
        RunError::Timeout => "polecenie nie zakończyło się w 30 sekund".into(),
        RunError::Spawn(e) => format!("nie udało się uruchomić: {e}"),
    }
}

/// Odczyt rejestracji przez CLI.
pub fn query(runner: &dyn Runner, engine: Engine, cli: &Cli, dir: &Path) -> Result<Found, String> {
    let output = runner
        .run(&spec(cli, query_args(engine), dir), TIMEOUT)
        .map_err(run_error)?;
    let text = format!("{}\n{}", output.stdout, output.stderr);
    match engine {
        Engine::Claude => match parse_claude_get(&text) {
            Some(command) => Ok(Found::Registered(command)),
            None if claude_missing(&text) => Ok(Found::Missing),
            None if !output.success => Err(error_text(&output)),
            None => Err("nieznany format odpowiedzi `claude mcp get`".into()),
        },
        Engine::Codex => {
            if !output.success {
                return Err(error_text(&output));
            }
            Ok(parse_codex_list(&text).map_or(Found::Missing, Found::Registered))
        }
    }
}

fn run_step(runner: &dyn Runner, cli: &Cli, args: Vec<String>, dir: &Path) -> Result<(), String> {
    let output = runner
        .run(&spec(cli, args, dir), TIMEOUT)
        .map_err(run_error)?;
    if output.success {
        Ok(())
    } else {
        Err(error_text(&output))
    }
}

/// Doprowadza rejestrację w jednym CLI do stanu `want` (odczyt, potem najwyżej
/// usunięcie i dodanie). Zwraca stan po zmianie.
pub fn sync_one(
    runner: &dyn Runner,
    engine: Engine,
    cli: &Cli,
    exe: Option<&Path>,
    want: bool,
    dir: &Path,
) -> Entry {
    if want && exe.is_none() {
        return entry(
            engine,
            "error",
            format!("Brak programu {EXE} obok aplikacji. Zainstaluj aplikację ponownie."),
        );
    }
    let found = match query(runner, engine, cli, dir) {
        Ok(found) => found,
        Err(e) => return entry(engine, "error", e),
    };
    let step = plan_step(want, &found, exe);
    let result = match step {
        Step::Nothing => Ok(()),
        Step::Remove => run_step(runner, cli, remove_args(engine), dir),
        Step::Add => run_step(
            runner,
            cli,
            add_args(engine, exe.unwrap_or(Path::new(EXE))),
            dir,
        ),
        Step::Replace => run_step(runner, cli, remove_args(engine), dir).and_then(|_| {
            run_step(
                runner,
                cli,
                add_args(engine, exe.unwrap_or(Path::new(EXE))),
                dir,
            )
        }),
    };
    match result {
        Err(e) => entry(engine, "error", e),
        Ok(()) if want => entry(engine, "registered", ""),
        Ok(()) => entry(engine, "missing", ""),
    }
}

/// Czy polecenie z rejestracji to program sprzed zmiany nazwy aplikacji (N1).
pub fn is_legacy_command(command: &str) -> bool {
    command
        .to_lowercase()
        .contains(&crate::hooks_config::legacy_names::MCP_EXE.to_lowercase())
}

/// Usuwa z wykrytych CLI stary serwer MCP sprzed zmiany nazwy (N1), ale tylko wtedy,
/// gdy wskazuje stary program (`...\<stary program>.exe`); wpis o tej nazwie
/// z innym poleceniem zostaje. Najlepszym wysiłkiem: błąd jednego CLI nie przerywa
/// drugiego. Zwraca silniki, z których usunięto wpis.
pub fn remove_legacy(runner: &dyn Runner, detected: &Detected, dir: &Path) -> Vec<Engine> {
    let name = crate::hooks_config::legacy_names::MCP_SERVER;
    let mut removed = Vec::new();
    for engine in [Engine::Claude, Engine::Codex] {
        let Some(cli) = detected.get(engine) else {
            continue;
        };
        let query = match engine {
            Engine::Claude => vec!["mcp".to_string(), "get".to_string(), name.to_string()],
            Engine::Codex => query_args(engine),
        };
        let Ok(output) = runner.run(&spec(cli, query, dir), TIMEOUT) else {
            continue;
        };
        let text = format!(
            "{}
{}",
            output.stdout, output.stderr
        );
        let command = match engine {
            Engine::Claude => parse_claude_get(&text),
            Engine::Codex => parse_codex_list_named(&text, name),
        };
        if command.is_some_and(|c| is_legacy_command(&c))
            && run_step(runner, cli, remove_args_named(engine, name), dir).is_ok()
        {
            removed.push(engine);
        }
    }
    removed
}

/// Rejestracja we wszystkich wykrytych CLI. `engine_setting` to `ai_engine`: puste
/// („jeszcze nie pytano”) nie rusza żadnego procesu, bo wybór agenta poprzedza MCP.
/// `want` = `ai_mcp` przy wybranym silniku.
pub fn sync_all(
    runner: &dyn Runner,
    detected: &Detected,
    engine_setting: &str,
    mcp_enabled: bool,
    exe: Option<&Path>,
    dir: &Path,
) -> Vec<Entry> {
    if engine_setting.is_empty() {
        return Vec::new();
    }
    let want = mcp_enabled && Engine::parse(engine_setting).is_some();
    [Engine::Claude, Engine::Codex]
        .into_iter()
        .map(|engine| match detected.get(engine) {
            Some(cli) => sync_one(runner, engine, cli, exe, want, dir),
            None => entry(engine, "unavailable", ""),
        })
        .collect()
}

#[cfg(test)]
#[path = "mcp_register_tests.rs"]
mod tests;
