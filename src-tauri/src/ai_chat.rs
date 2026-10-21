//! Czat w aplikacji (PLAN_BACKLOG.md, A3).
//!
//! Panel czatu rozmawia z modelem przez to samo CLI z subskrypcji co reszta
//! asystenta (`ai.rs`), a dane aplikacji czyta narzędziami serwera MCP (A2):
//! - **Claude**: jeden długo żyjący proces na rozmowę (`claude -p --input-format
//!   stream-json --output-format stream-json …`); wiadomości idą na stdin jako
//!   linie JSON, zdarzenia ze stdout są parsowane i strumieniowane do UI,
//! - **Codex**: `codex exec --json` na każdą wiadomość, z historią rozmowy w
//!   prompcie i serwerem MCP podanym przez `-c mcp_servers.todo-track.command=…`.
//!
//! **Prywatność**: każda wiadomość użytkownika przechodzi przez
//! `ai_privacy::redact_with` (ta sama mapa pseudonimów w całej rozmowie), a
//! tekst i argumenty narzędzi z odpowiedzi przez `restore`, zanim trafią do UI.
//! Do modelu nie trafia nic poza przefiltrowaną wiadomością i historią rozmowy
//! (też przefiltrowaną). **Model tylko proponuje**: narzędzia MCP tworzą
//! propozycje, które UI pokazuje jako karty „Zastosuj” / „Odrzuć”.
//!
//! Zarządzanie procesem idzie przez cechy [`Spawner`] i [`Child`], a zdarzenia
//! przez [`Sink`]; testy podstawiają zapisane strumienie i proces zastępczy,
//! więc nigdy nie uruchamiają prawdziwego `claude` ani `codex`. Historia
//! rozmowy żyje tylko w pamięci: zamknięcie aplikacji kończy rozmowy.
use crate::ai::{self, Cli, Engine, Failure, Spec};
use crate::ai_privacy::{self, Dictionary, Pseudonyms};
use crate::mcp_register;
use crate::settings::AppSettings;
use serde::Serialize;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager as _};

/// Zdarzenie okien ze strumieniem odpowiedzi.
pub const EVENT: &str = "ai-chat-event";
/// Plik konfiguracji MCP czatu Claude w katalogu AI.
pub const MCP_FILE: &str = "chat-mcp.json";
/// Prefiks nazw narzędzi serwera w Claude Code (`mcp__<serwer>__<narzędzie>`).
const TOOL_PREFIX: &str = "mcp__todo-track__";
/// Ile ostatnich linii stderr pamiętamy do rozpoznania przyczyny błędu.
const STDERR_TAIL: usize = 20;
/// Najwięcej wiadomości historii dołączanej do promptu (Codex i wznowienie Claude).
const HISTORY_MAX: usize = 40;

pub const BUSY: &str = "Asystent jeszcze odpowiada. Poczekaj albo kliknij „Zatrzymaj”.";
pub const EMPTY: &str = "Wpisz pytanie do asystenta.";
pub const CHAT_OFF: &str = "Czat jest wyłączony. Włącz „Czat” w Ustawieniach → Asystent AI.";
pub const MCP_REQUIRED: &str = "Czat czyta dane przez serwer MCP. Włącz „Serwer MCP dla agentów” w Ustawieniach → Asystent AI.";
pub const MCP_MISSING: &str =
    "Brak programu todo-track-mcp.exe obok aplikacji. Zainstaluj aplikację ponownie.";
pub const UNEXPECTED_EXIT: &str =
    "Asystent zakończył pracę przed końcem odpowiedzi. Zadaj pytanie ponownie.";

/// Rola asystenta dodawana do promptu systemowego Claude i do pierwszej
/// wiadomości Codexa.
pub const ROLE: &str = "Jesteś asystentem aplikacji Todo Track (czas pracy, zadania, Jira). \
Odpowiadaj po polsku, krótko i konkretnie. Dane czytasz wyłącznie narzędziami serwera \
todo-track; nie zgaduj ich. Zapisów nie wykonujesz: narzędzia propose_* tylko tworzą \
propozycje, które użytkownik zatwierdza w aplikacji. Nie masz dostępu do wysyłki czasu \
do Ideo Timesheet ani do zapisów w Jirze. Osoby występują jako „Osoba N”; używaj tych \
oznaczeń bez zmian i nie próbuj ustalać, kto za nimi stoi.";

// ---------------------------------------------------------------------------
// Zdarzenia

/// Zdarzenie jednej tury rozmowy dla interfejsu.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Event {
    /// Tekst odpowiedzi (kolejny akapit).
    Text { text: String },
    /// Wywołanie narzędzia serwera MCP (nazwa bez prefiksu, argumenty).
    Tool { name: String, input: Value },
    /// Narzędzie zwróciło wynik (treść nie trafia do UI).
    ToolDone { error: bool },
    /// Koniec odpowiedzi.
    Done,
    /// Błąd po polsku.
    Error { message: String },
    /// Użytkownik kliknął „Zatrzymaj”.
    Stopped,
}

/// Zdarzenie razem z numerem tury (`ai-chat-event`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Payload {
    pub turn: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// Odbiorca zdarzeń (w aplikacji `emit`, w testach lista).
pub trait Sink: Send + Sync {
    fn emit(&self, turn: u64, event: Event);
}

/// Zdarzenie wyczytane z jednej linii strumienia CLI, przed przywróceniem nazw.
#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Text(String),
    Tool {
        name: String,
        input: Value,
    },
    ToolDone {
        error: bool,
    },
    /// Koniec tury; tekst końcowy (gdy odpowiedź nie miała wcześniejszych akapitów).
    Done(String),
    Error(Failure),
}

/// Nazwa narzędzia bez prefiksu serwera (`mcp__todo-track__day_summary` → `day_summary`).
pub fn short_tool_name(name: &str) -> String {
    name.strip_prefix(TOOL_PREFIX).unwrap_or(name).to_string()
}

/// Linia `stream-json` Claude Code.
pub fn parse_claude_line(line: &str) -> Vec<Parsed> {
    let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let content = event["message"]["content"].as_array();
    match event["type"].as_str().unwrap_or_default() {
        "assistant" => content
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => block["text"]
                    .as_str()
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| Parsed::Text(t.to_string())),
                Some("tool_use") => Some(Parsed::Tool {
                    name: short_tool_name(block["name"].as_str().unwrap_or_default()),
                    input: block["input"].clone(),
                }),
                _ => None,
            })
            .collect(),
        "user" => content
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "tool_result")
            .map(|block| Parsed::ToolDone {
                error: block["is_error"] == true,
            })
            .collect(),
        "result" => {
            let text = event["result"].as_str().unwrap_or_default();
            let subtype = event["subtype"].as_str().unwrap_or_default();
            if event["is_error"] == true || subtype.starts_with("error") {
                vec![Parsed::Error(ai::classify(&format!("{text} {subtype}")))]
            } else {
                vec![Parsed::Done(text.to_string())]
            }
        }
        _ => Vec::new(),
    }
}

/// Linia JSONL `codex exec --json`: wiadomość agenta, wywołanie narzędzia MCP,
/// koniec tury albo błąd.
pub fn parse_codex_line(line: &str) -> Vec<Parsed> {
    let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let kind = event["type"].as_str().unwrap_or_default();
    let item = &event["item"];
    if item["type"] == "mcp_tool_call" {
        return match kind {
            "item.started" => vec![Parsed::Tool {
                name: short_tool_name(item["tool"].as_str().unwrap_or_default()),
                input: if item["arguments"].is_null() {
                    json!({})
                } else {
                    item["arguments"].clone()
                },
            }],
            "item.completed" => vec![Parsed::ToolDone {
                error: item["status"] == "failed" || !item["error"].is_null(),
            }],
            _ => Vec::new(),
        };
    }
    if kind == "turn.completed" {
        return vec![Parsed::Done(String::new())];
    }
    match ai::codex_event_text(&event) {
        Some((true, text)) => vec![Parsed::Error(ai::classify(&text))],
        Some((false, text)) if !text.trim().is_empty() => vec![Parsed::Text(text)],
        _ => Vec::new(),
    }
}

fn parse_line(engine: Engine, line: &str) -> Vec<Parsed> {
    match engine {
        Engine::Claude => parse_claude_line(line),
        Engine::Codex => parse_codex_line(line),
    }
}

// ---------------------------------------------------------------------------
// Linie poleceń

/// Konfiguracja MCP czatu Claude: serwer `todo-track` wskazujący program obok aplikacji.
pub fn mcp_config(exe: &Path) -> Value {
    json!({
        "mcpServers": {
            mcp_register::SERVER: {
                "command": exe.to_string_lossy(),
                "args": []
            }
        }
    })
}

/// `claude -p --input-format stream-json --output-format stream-json --verbose
/// --tools "" --strict-mcp-config --mcp-config <plik> --allowedTools "mcp__todo-track__*"
/// --permission-prompts none --setting-sources "" --no-session-persistence
/// --append-system-prompt <rola> [--model <główny>]`.
pub fn claude_chat_args(mcp_file: &Path, model: &str) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--strict-mcp-config",
        "--mcp-config",
        &mcp_file.to_string_lossy(),
        "--allowedTools",
        "mcp__todo-track__*",
        "--permission-prompts",
        "none",
        "--setting-sources",
        "",
        "--no-session-persistence",
        "--append-system-prompt",
        ROLE,
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

/// Wartość TOML (tekst w cudzysłowie z ucieczkami) dla `-c klucz=wartość`.
fn toml_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// `codex exec --json --ephemeral --skip-git-repo-check -s read-only -C <katalog AI>
/// -c mcp_servers.todo-track.command=<exe> -c mcp_servers.todo-track.args=[]
/// -c notify=[] [-m <model>]`; prompt idzie na stdin.
pub fn codex_chat_args(dir: &Path, exe: &Path, model: &str) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".into(),
        "--json".into(),
        "--ephemeral".into(),
        "--skip-git-repo-check".into(),
        "-s".into(),
        "read-only".into(),
        "-C".into(),
        dir.to_string_lossy().into_owned(),
        "-c".into(),
        format!(
            "mcp_servers.{}.command={}",
            mcp_register::SERVER,
            toml_string(&exe.to_string_lossy())
        ),
        "-c".into(),
        format!("mcp_servers.{}.args=[]", mcp_register::SERVER),
        "-c".into(),
        "notify=[]".into(),
    ];
    if !model.is_empty() {
        args.push("-m".into());
        args.push(model.into());
    }
    args
}

/// Wiadomość użytkownika dla Claude w `--input-format stream-json`.
pub fn claude_user_line(text: &str) -> String {
    json!({
        "type": "user",
        "message": {"role": "user", "content": [{"type": "text", "text": text}]}
    })
    .to_string()
}

// ---------------------------------------------------------------------------
// Proces

/// Strumień z procesu CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum Stream {
    Stdout(String),
    Stderr(String),
    /// Proces zakończył się (albo zamknął stdout).
    Closed,
}

/// Uruchomiony proces CLI.
pub trait Child: Send {
    /// Zapisuje tekst na stdin.
    fn write(&mut self, data: &str) -> Result<(), String>;
    /// Zamyka stdin (Codex czyta prompt do końca wejścia).
    fn finish_input(&mut self);
    /// Kończy proces razem z potomkami.
    fn kill(&mut self);
}

/// Uruchamia procesy CLI; testy podstawiają zapisane strumienie.
pub trait Spawner: Send + Sync {
    fn spawn(&self, spec: &Spec) -> Result<(Box<dyn Child>, Receiver<Stream>), String>;
}

/// Prawdziwe procesy, bez okna konsoli.
pub struct ProcessSpawner;

struct ProcessChild {
    pid: u32,
    stdin: Option<std::process::ChildStdin>,
}

impl Child for ProcessChild {
    fn write(&mut self, data: &str) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or("Wejście procesu jest zamknięte.")?;
        stdin
            .write_all(data.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| e.to_string())
    }
    fn finish_input(&mut self) {
        self.stdin = None;
    }
    fn kill(&mut self) {
        self.stdin = None;
        ai::kill_tree(self.pid);
    }
}

impl Spawner for ProcessSpawner {
    fn spawn(&self, spec: &Spec) -> Result<(Box<dyn Child>, Receiver<Stream>), String> {
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
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let pid = child.id();
        let stdin = child.stdin.take();
        let (tx, rx) = channel();
        let stderr = child.stderr.take();
        let err_tx = tx.clone();
        let stderr_thread = std::thread::spawn(move || {
            if let Some(pipe) = stderr {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = err_tx.send(Stream::Stderr(line));
                }
            }
        });
        let stdout = child.stdout.take();
        std::thread::spawn(move || {
            if let Some(pipe) = stdout {
                for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                    let _ = tx.send(Stream::Stdout(line));
                }
            }
            let _ = stderr_thread.join();
            let _ = child.wait();
            let _ = tx.send(Stream::Closed);
        });
        Ok((Box::new(ProcessChild { pid, stdin }), rx))
    }
}

// ---------------------------------------------------------------------------
// Rozmowa

/// Wszystko, czego potrzebuje wysłanie wiadomości poza słownikiem.
#[derive(Clone, Debug)]
pub struct Config {
    pub engine: Engine,
    pub cli: Cli,
    /// Model „główny” z ustawień; pusty = domyślny model CLI.
    pub model: String,
    /// Katalog AI (roboczy katalog procesu, plik konfiguracji MCP).
    pub dir: PathBuf,
    /// Program serwera MCP obok aplikacji.
    pub mcp_exe: PathBuf,
}

/// Czy czat może działać: wybrany i wykryty silnik, włączony `ai_chat` i serwer MCP.
pub fn gate(
    settings: &AppSettings,
    detected: &ai::Detected,
    mcp_exe: Option<PathBuf>,
) -> Result<(Engine, Cli, PathBuf), String> {
    let engine = Engine::parse(&settings.ai_engine).ok_or_else(|| ai::OFF.to_string())?;
    if !settings.ai_chat {
        return Err(CHAT_OFF.to_string());
    }
    if !settings.ai_mcp {
        return Err(MCP_REQUIRED.to_string());
    }
    let cli = detected
        .get(engine)
        .cloned()
        .ok_or_else(|| ai::missing_message(engine, detected))?;
    let exe = mcp_exe.ok_or_else(|| MCP_MISSING.to_string())?;
    Ok((engine, cli, exe))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Role {
    User,
    Assistant,
}

struct Live {
    gen: u64,
    child: Box<dyn Child>,
}

struct Active {
    turn: u64,
    gen: u64,
    /// Odpowiedź asystenta w tej turze (z pseudonimami), do historii.
    text: String,
}

#[derive(Default)]
struct Inner {
    next_gen: u64,
    turn: u64,
    live: Option<Live>,
    active: Option<Active>,
    /// Historia rozmowy po filtrze (z pseudonimami); do promptu Codexa i wznowienia Claude.
    history: Vec<(Role, String)>,
    pseudonyms: Pseudonyms,
}

/// Wynik wysłania wiadomości.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sent {
    pub turn: u64,
    /// Tekst wysłany do modelu, po filtrze danych osobowych.
    pub sent: String,
}

/// Jedna rozmowa: proces, historia i pseudonimy.
pub struct Manager {
    spawner: Arc<dyn Spawner>,
    inner: Arc<Mutex<Inner>>,
}

/// Historia jako tekst na początek promptu (Codex i wznowienie Claude po
/// zatrzymaniu); ostatnie [`HISTORY_MAX`] wiadomości.
fn transcript(history: &[(Role, String)]) -> String {
    let start = history.len().saturating_sub(HISTORY_MAX);
    history[start..]
        .iter()
        .map(|(role, text)| match role {
            Role::User => format!("Użytkownik: {text}"),
            Role::Assistant => format!("Asystent: {text}"),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Prompt Codexa: rola, dotychczasowa rozmowa i nowe pytanie.
fn codex_prompt(history: &[(Role, String)], message: &str) -> String {
    let mut prompt = format!("{ROLE}\n\n");
    if !history.is_empty() {
        prompt.push_str(&format!(
            "Dotychczasowa rozmowa:\n{}\n\n",
            transcript(history)
        ));
    }
    prompt.push_str(&format!("Nowe pytanie użytkownika:\n{message}"));
    prompt
}

/// Pierwsza wiadomość nowego procesu Claude: po zatrzymaniu rozmowy zawiera
/// dotychczasową historię, bo `--no-session-persistence` nie zachowuje kontekstu.
fn claude_first_message(history: &[(Role, String)], message: &str) -> String {
    if history.is_empty() {
        message.to_string()
    } else {
        format!(
            "Dotychczasowa rozmowa:\n{}\n\nNowe pytanie użytkownika:\n{message}",
            transcript(history)
        )
    }
}

fn lock(inner: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}

impl Manager {
    pub fn new(spawner: Arc<dyn Spawner>) -> Self {
        Self {
            spawner,
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    /// Czy trwa odpowiedź (testy).
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        lock(&self.inner).active.is_some()
    }

    /// Wysyła wiadomość: filtr danych osobowych, start procesu (Claude: tylko gdy
    /// nie żyje), zapis na stdin i strumieniowanie odpowiedzi do `sink`.
    pub fn send(
        &self,
        cfg: &Config,
        dict: &Dictionary,
        text: &str,
        sink: Arc<dyn Sink>,
    ) -> Result<Sent, String> {
        if text.trim().is_empty() {
            return Err(EMPTY.to_string());
        }
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        if inner.active.is_some() {
            return Err(BUSY.to_string());
        }
        // Wejście zawsze po filtrze; ta sama mapa pseudonimów w całej rozmowie.
        let sent = ai_privacy::redact_with(text.trim(), dict, &mut inner.pseudonyms);
        std::fs::create_dir_all(&cfg.dir)
            .map_err(|e| format!("Nie można utworzyć katalogu asystenta: {e}"))?;
        // Proces Claude, który zakończył się sam, nie jest już żywy (wątek odczytu
        // zdejmuje go przy `Closed`); wiadomość startuje wtedy nowy.
        let reuse = cfg.engine == Engine::Claude && inner.live.is_some();
        let (payload, stdin) = match cfg.engine {
            Engine::Claude => {
                let first = if reuse {
                    sent.clone()
                } else {
                    claude_first_message(&inner.history, &sent)
                };
                (claude_user_line(&first) + "\n", false)
            }
            Engine::Codex => (codex_prompt(&inner.history, &sent), true),
        };
        let mut spawned: Option<(Box<dyn Child>, Receiver<Stream>)> = None;
        if !reuse {
            let spec = self.spec(cfg)?;
            spawned = Some(
                self.spawner
                    .spawn(&spec)
                    .map_err(|e| format!("Nie udało się uruchomić {}: {e}", cfg.engine.name()))?,
            );
        }
        inner.next_gen += 1;
        let gen = if reuse {
            inner.live.as_ref().map_or(inner.next_gen, |l| l.gen)
        } else {
            inner.next_gen
        };
        let write = |child: &mut Box<dyn Child>| -> Result<(), String> {
            child.write(&payload)?;
            if stdin {
                child.finish_input();
            }
            Ok(())
        };
        if let Some((mut child, rx)) = spawned {
            if let Err(e) = write(&mut child) {
                child.kill();
                return Err(format!(
                    "Nie udało się wysłać wiadomości do {}: {e}",
                    cfg.engine.name()
                ));
            }
            // Poprzedni, martwy proces zostaje zastąpiony.
            if let Some(mut old) = inner.live.take() {
                old.child.kill();
            }
            inner.live = Some(Live { gen, child });
            let engine = cfg.engine;
            let state = Arc::clone(&self.inner);
            let sink = Arc::clone(&sink);
            std::thread::spawn(move || read_stream(state, sink, engine, gen, rx));
        } else if let Some(live) = inner.live.as_mut() {
            if let Err(e) = write(&mut live.child) {
                if let Some(mut dead) = inner.live.take() {
                    dead.child.kill();
                }
                return Err(format!(
                    "Nie udało się wysłać wiadomości do {}: {e}",
                    cfg.engine.name()
                ));
            }
        }
        inner.turn += 1;
        let turn = inner.turn;
        inner.history.push((Role::User, sent.clone()));
        inner.active = Some(Active {
            turn,
            gen,
            text: String::new(),
        });
        Ok(Sent { turn, sent })
    }

    /// Polecenie startu procesu; zapisuje plik konfiguracji MCP Claude.
    fn spec(&self, cfg: &Config) -> Result<Spec, String> {
        let args = match cfg.engine {
            Engine::Claude => {
                let file = cfg.dir.join(MCP_FILE);
                std::fs::write(&file, mcp_config(&cfg.mcp_exe).to_string())
                    .map_err(|e| format!("Nie można zapisać konfiguracji MCP: {e}"))?;
                claude_chat_args(&file, &cfg.model)
            }
            Engine::Codex => codex_chat_args(&cfg.dir, &cfg.mcp_exe, &cfg.model),
        };
        Ok(Spec {
            program: PathBuf::from(&cfg.cli.path),
            args,
            stdin: String::new(),
            cwd: Some(cfg.dir.clone()),
            env: ai::call_env(),
        })
    }

    /// „Zatrzymaj”: kończy proces i bieżącą odpowiedź; częściowy tekst zostaje
    /// w historii. Kolejna wiadomość startuje nowy proces (Claude dostaje historię).
    pub fn stop(&self, sink: &dyn Sink) {
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        if let Some(mut live) = inner.live.take() {
            live.child.kill();
        }
        if let Some(active) = inner.active.take() {
            if !active.text.is_empty() {
                inner.history.push((Role::Assistant, active.text));
            }
            sink.emit(active.turn, Event::Stopped);
        }
    }

    /// „Nowa rozmowa” i koniec aplikacji: kończy proces, kasuje historię i pseudonimy.
    pub fn reset(&self) {
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        if let Some(mut live) = inner.live.take() {
            live.child.kill();
        }
        inner.active = None;
        inner.history.clear();
        inner.pseudonyms = Pseudonyms::new();
    }
}

/// Wątek odczytu jednego procesu: parsuje linie, przywraca nazwy i wysyła zdarzenia.
fn read_stream(
    state: Arc<Mutex<Inner>>,
    sink: Arc<dyn Sink>,
    engine: Engine,
    gen: u64,
    rx: Receiver<Stream>,
) {
    let mut stderr: Vec<String> = Vec::new();
    for item in rx {
        match item {
            Stream::Stderr(line) => {
                stderr.push(line);
                if stderr.len() > STDERR_TAIL {
                    stderr.remove(0);
                }
            }
            Stream::Stdout(line) => {
                for parsed in parse_line(engine, &line) {
                    handle(&state, sink.as_ref(), engine, gen, parsed);
                }
            }
            Stream::Closed => {
                let mut guard = lock(&state);
                let inner = &mut *guard;
                if inner.live.as_ref().is_some_and(|l| l.gen == gen) {
                    inner.live = None;
                }
                let Some(active) = inner.active.take_if(|a| a.gen == gen) else {
                    return;
                };
                // Proces skończył bez wyniku: Codex po tekście bez `turn.completed`
                // to koniec odpowiedzi, reszta to ubity albo przerwany proces.
                if engine == Engine::Codex && !active.text.is_empty() {
                    let turn = active.turn;
                    inner.history.push((Role::Assistant, active.text));
                    sink.emit(turn, Event::Done);
                    return;
                }
                let failure = ai::classify(&stderr.join("\n"));
                let message = match failure {
                    Failure::Other | Failure::Unreadable => UNEXPECTED_EXIT.to_string(),
                    other => ai::failure_message(engine, &other),
                };
                if !active.text.is_empty() {
                    inner.history.push((Role::Assistant, active.text.clone()));
                }
                sink.emit(active.turn, Event::Error { message });
                return;
            }
        }
    }
}

fn handle(state: &Mutex<Inner>, sink: &dyn Sink, engine: Engine, gen: u64, parsed: Parsed) {
    let mut guard = lock(state);
    let inner = &mut *guard;
    let Some(active) = inner.active.as_mut().filter(|a| a.gen == gen) else {
        return;
    };
    let turn = active.turn;
    match parsed {
        Parsed::Text(text) => {
            if !active.text.is_empty() {
                active.text.push_str("\n\n");
            }
            active.text.push_str(&text);
            let text = inner.pseudonyms.restore(&text);
            sink.emit(turn, Event::Text { text });
        }
        Parsed::Tool { name, input } => {
            let input = inner.pseudonyms.restore_value(&input);
            sink.emit(turn, Event::Tool { name, input });
        }
        Parsed::ToolDone { error } => sink.emit(turn, Event::ToolDone { error }),
        Parsed::Done(final_text) => {
            // Odpowiedź bez wcześniejszych akapitów: tekst wyniku jest jedyną treścią.
            if active.text.is_empty() && !final_text.trim().is_empty() {
                active.text = final_text.clone();
                let text = inner.pseudonyms.restore(&final_text);
                sink.emit(turn, Event::Text { text });
            }
            finish(inner, engine, true);
            sink.emit(turn, Event::Done);
        }
        Parsed::Error(failure) => {
            let message = ai::failure_message(engine, &failure);
            finish(inner, engine, false);
            sink.emit(turn, Event::Error { message });
        }
    }
}

/// Zamyka turę: historia dostaje odpowiedź, a proces Codexa (jedna tura na
/// proces) przestaje być „żywy”.
fn finish(inner: &mut Inner, engine: Engine, keep_text: bool) {
    if let Some(active) = inner.active.take() {
        if keep_text && !active.text.is_empty() {
            inner.history.push((Role::Assistant, active.text));
        }
    }
    if engine == Engine::Codex {
        // Proces kończy się sam; wątek odczytu zdejmie go przy `Closed`.
        inner.live = None;
    }
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

/// Stan modułu (jedna rozmowa na aplikację).
pub struct State {
    manager: Manager,
}

struct AppSink(tauri::AppHandle);

impl Sink for AppSink {
    fn emit(&self, turn: u64, event: Event) {
        let _ = self.0.emit(EVENT, Payload { turn, event });
    }
}

/// Rejestruje stan modułu; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State {
        manager: Manager::new(Arc::new(ProcessSpawner)),
    });
}

/// Koniec aplikacji kończy rozmowę i proces CLI.
pub fn shutdown(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<State>() {
        state.manager.reset();
    }
}

/// Wysyła wiadomość czatu. Odpowiedź przychodzi zdarzeniami `ai-chat-event`;
/// wynik zawiera tekst po filtrze danych osobowych („Pokaż, co zostało wysłane”).
#[tauri::command]
pub async fn ai_chat_send(app: tauri::AppHandle, text: String) -> Result<Sent, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let settings = crate::settings::current(&app);
        let detected = ai::detected_cached(&app);
        let (engine, cli, mcp_exe) = gate(&settings, &detected, crate::mcp::exe_path())?;
        let dir = ai::dir(&app).ok_or("Nie można ustalić katalogu danych aplikacji.")?;
        let cfg = Config {
            engine,
            cli,
            model: ai::model_for(&settings, ai::Tier::Main).to_string(),
            dir,
            mcp_exe,
        };
        let dict = ai::dictionary(&app, &settings);
        let state = app
            .try_state::<State>()
            .ok_or("Asystent AI nie jest gotowy.")?;
        state
            .manager
            .send(&cfg, &dict, &text, Arc::new(AppSink(app.clone())))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// „Zatrzymaj”: kończy odpowiedź i proces.
#[tauri::command]
pub fn ai_chat_stop(app: tauri::AppHandle) {
    if let Some(state) = app.try_state::<State>() {
        state.manager.stop(&AppSink(app.clone()));
    }
}

/// „Nowa rozmowa”: kończy proces i kasuje historię.
#[tauri::command]
pub fn ai_chat_reset(app: tauri::AppHandle) {
    shutdown(&app);
}

#[cfg(test)]
mod tests;
