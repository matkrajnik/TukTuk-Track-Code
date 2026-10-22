//! Czat w aplikacji (PLAN_BACKLOG.md, A3).
//!
//! Panel czatu rozmawia z modelem przez to samo CLI z subskrypcji co reszta
//! asystenta (`ai.rs`), a dane aplikacji czyta narzędziami serwera MCP (A2):
//! - **Claude**: jeden długo żyjący proces na rozmowę (`claude -p --input-format
//!   stream-json --output-format stream-json …`); wiadomości idą na stdin jako
//!   linie JSON, zdarzenia ze stdout są parsowane i strumieniowane do UI,
//! - **Codex**: `codex exec --json` na każdą wiadomość, z historią rozmowy w
//!   prompcie i serwerem MCP jako jedynym serwerem (`-c mcp_servers={…}` zastępuje
//!   całą tabelę serwerów z `~/.codex/config.toml`), bez wbudowanej powłoki.
//!
//! **Prywatność**: każda wiadomość użytkownika przechodzi przez
//! `ai_privacy::redact_with`, a tekst i argumenty narzędzi z odpowiedzi przez
//! `restore`, zanim trafią do UI. Mapa pseudonimów jest **wspólna z serwerem MCP**
//! (`mcp::State::pseudonyms`): „Osoba N” z wiadomości użytkownika i z wyniku
//! narzędzia to jedna przestrzeń oznaczeń, więc przywrócenie zawsze trafia we
//! właściwą osobę (także w argumentach `propose_*`).
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
const TOOL_PREFIX: &str = "mcp__tuktuk-track__";
/// Ile ostatnich linii stderr pamiętamy do rozpoznania przyczyny błędu.
const STDERR_TAIL: usize = 20;
/// Najwięcej wiadomości historii dołączanej do promptu (Codex i wznowienie Claude).
const HISTORY_MAX: usize = 40;

pub const BUSY: &str = "Asystent jeszcze odpowiada. Poczekaj albo kliknij „Zatrzymaj”.";
pub const EMPTY: &str = "Wpisz pytanie do asystenta.";
pub const CHAT_OFF: &str = "Czat jest wyłączony. Włącz „Czat” w Ustawieniach → Asystent AI.";
pub const MCP_REQUIRED: &str = "Czat czyta dane przez serwer MCP. Włącz „Serwer MCP dla agentów” w Ustawieniach → Asystent AI.";
pub const MCP_MISSING: &str =
    "Brak programu tuktuk-track-mcp.exe obok aplikacji. Zainstaluj aplikację ponownie.";
pub const UNEXPECTED_EXIT: &str =
    "Asystent zakończył pracę przed końcem odpowiedzi. Zadaj pytanie ponownie.";

/// Rola asystenta dodawana do promptu systemowego Claude i do pierwszej
/// wiadomości Codexa.
pub const ROLE: &str = "Jesteś asystentem aplikacji TukTuk Track (czas pracy, zadania, Jira). \
Odpowiadaj po polsku, krótko i konkretnie. Dane czytasz wyłącznie narzędziami serwera \
tuktuk-track; nie zgaduj ich. Zapisów nie wykonujesz: narzędzia propose_* tylko tworzą \
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

/// Nazwa narzędzia bez prefiksu serwera (`mcp__tuktuk-track__day_summary` → `day_summary`).
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

/// Konfiguracja MCP czatu Claude: serwer `tuktuk-track` wskazujący program obok aplikacji.
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
/// --tools "" --strict-mcp-config --mcp-config <plik> --allowedTools "mcp__tuktuk-track__*"
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
        "mcp__tuktuk-track__*",
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

/// Wbudowane narzędzia Codexa wyłączone w czacie (`-c features.<klucz>=false`):
/// powłoka mogłaby czytać pliki poza katalogiem AI (bazę aplikacji, pamięć
/// podręczną Jiry) z pominięciem filtra danych osobowych, a wyszukiwanie w sieci
/// wysyłałoby treść rozmowy poza model.
pub const CODEX_DISABLED_FEATURES: [&str; 3] = ["shell_tool", "unified_exec", "web_search_request"];

/// `codex exec --json --ephemeral --skip-git-repo-check -s read-only -C <katalog AI>
/// -c mcp_servers={"tuktuk-track"={command=<exe>,args=[]}} -c features.shell_tool=false
/// -c features.unified_exec=false -c features.web_search_request=false -c notify=[]
/// [-m <model>]`; prompt idzie na stdin.
///
/// Cała tabela `mcp_servers` jest zastąpiona jednym wpisem (odpowiednik
/// `--strict-mcp-config` Claude): serwery MCP zarejestrowane przez użytkownika w
/// `~/.codex/config.toml` (np. Jira z narzędziami zapisu) nie są w czacie dostępne.
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
            "mcp_servers={{{}={{command={},args=[]}}}}",
            toml_string(mcp_register::SERVER),
            toml_string(&exe.to_string_lossy())
        ),
    ];
    for feature in CODEX_DISABLED_FEATURES {
        args.push("-c".into());
        args.push(format!("features.{feature}=false"));
    }
    args.push("-c".into());
    args.push("notify=[]".into());
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

/// Wiadomość historii po filtrze (z pseudonimami) z numerem tury, żeby
/// „Zatrzymaj” spóźnionej tury mogło ją usunąć z rozmowy.
#[derive(Clone, Debug)]
struct Line {
    role: Role,
    text: String,
    turn: u64,
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
    /// Rośnie przy „Zatrzymaj” i „Nowej rozmowie”: wysyłka, która zaczęła się
    /// przed nimi, nie instaluje już swojego procesu.
    epoch: u64,
    /// Pokolenie procesu, którego strumień się zamknął (proces Claude wyjęty na
    /// czas zapisu nie wraca wtedy jako żywy).
    closed_gen: Option<u64>,
    live: Option<Live>,
    active: Option<Active>,
    /// Historia rozmowy po filtrze (z pseudonimami); do promptu Codexa i wznowienia Claude.
    history: Vec<Line>,
}

/// Wynik wysłania wiadomości.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Sent {
    pub turn: u64,
    /// Tekst wysłany do modelu, po filtrze danych osobowych.
    pub sent: String,
}

/// Jedna rozmowa: proces i historia. Pseudonimy są wspólne z serwerem MCP.
pub struct Manager {
    spawner: Arc<dyn Spawner>,
    inner: Arc<Mutex<Inner>>,
    pseudonyms: Arc<Mutex<Pseudonyms>>,
}

/// Historia jako tekst na początek promptu (Codex i wznowienie Claude po
/// zatrzymaniu); ostatnie [`HISTORY_MAX`] wiadomości.
fn transcript(history: &[Line]) -> String {
    let start = history.len().saturating_sub(HISTORY_MAX);
    history[start..]
        .iter()
        .map(|line| match line.role {
            Role::User => format!("Użytkownik: {}", line.text),
            Role::Assistant => format!("Asystent: {}", line.text),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Prompt Codexa: rola, dotychczasowa rozmowa i nowe pytanie.
fn codex_prompt(history: &[Line], message: &str) -> String {
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
fn claude_first_message(history: &[Line], message: &str) -> String {
    if history.is_empty() {
        message.to_string()
    } else {
        format!(
            "Dotychczasowa rozmowa:\n{}\n\nNowe pytanie użytkownika:\n{message}",
            transcript(history)
        )
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Proces gotowy do zainstalowania po zapisie wiadomości (faza bez blokady).
type Prepared = (Live, Option<Receiver<Stream>>);

impl Manager {
    /// `pseudonyms`: mapa wspólna z serwerem MCP (`mcp::State::pseudonyms`).
    pub fn new(spawner: Arc<dyn Spawner>, pseudonyms: Arc<Mutex<Pseudonyms>>) -> Self {
        Self {
            spawner,
            inner: Arc::new(Mutex::new(Inner::default())),
            pseudonyms,
        }
    }

    /// Czy trwa odpowiedź (testy).
    #[cfg(test)]
    pub fn busy(&self) -> bool {
        lock(&self.inner).active.is_some()
    }

    /// Wysyła wiadomość: filtr danych osobowych, start procesu (Claude: tylko gdy
    /// nie żyje), zapis na stdin i strumieniowanie odpowiedzi do `sink`.
    ///
    /// Blokada stanu nie jest trzymana w czasie startu procesu i zapisu na stdin
    /// (długi prompt może czekać na potok): tura jest rezerwowana pod blokadą,
    /// proces startuje bez niej, a na końcu tura jest zatwierdzana, o ile w
    /// międzyczasie nie było „Zatrzymaj” ani „Nowej rozmowy”.
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
        // 1. Rezerwacja tury.
        let (turn, gen, epoch, sent, payload, close_input, reused) = {
            let mut guard = lock(&self.inner);
            let inner = &mut *guard;
            if inner.active.is_some() {
                return Err(BUSY.to_string());
            }
            // Wejście zawsze po filtrze; mapa wspólna z wynikami narzędzi MCP.
            let sent = ai_privacy::redact_with(text.trim(), dict, &mut lock(&self.pseudonyms));
            // Proces Claude, który zakończył się sam, nie jest już żywy (wątek
            // odczytu zdejmuje go przy `Closed`); wiadomość startuje wtedy nowy.
            let reused = match cfg.engine {
                Engine::Claude => inner.live.take(),
                Engine::Codex => None,
            };
            let (payload, close_input) = match cfg.engine {
                Engine::Claude => {
                    let first = if reused.is_some() {
                        sent.clone()
                    } else {
                        claude_first_message(&inner.history, &sent)
                    };
                    (claude_user_line(&first) + "\n", false)
                }
                Engine::Codex => (codex_prompt(&inner.history, &sent), true),
            };
            inner.next_gen += 1;
            let gen = reused.as_ref().map_or(inner.next_gen, |l| l.gen);
            inner.turn += 1;
            let turn = inner.turn;
            inner.history.push(Line {
                role: Role::User,
                text: sent.clone(),
                turn,
            });
            inner.active = Some(Active {
                turn,
                gen,
                text: String::new(),
            });
            (turn, gen, inner.epoch, sent, payload, close_input, reused)
        };
        // 2. Start procesu i zapis wiadomości bez blokady.
        let prepared = self.prepare(cfg, gen, reused, &payload, close_input);
        // 3. Zatwierdzenie tury.
        let mut guard = lock(&self.inner);
        let inner = &mut *guard;
        let (mut live, rx) = match prepared {
            Ok(prepared) => prepared,
            Err(e) => {
                if inner.active.as_ref().is_some_and(|a| a.turn == turn) {
                    inner.active = None;
                }
                inner.history.retain(|line| line.turn != turn);
                return Err(e);
            }
        };
        if inner.epoch != epoch {
            // „Zatrzymaj” albo „Nowa rozmowa” w trakcie startu: proces niepotrzebny.
            drop(guard);
            live.child.kill();
            return Ok(Sent { turn, sent });
        }
        if rx.is_none() && inner.closed_gen == Some(gen) {
            // Proces Claude zamknął się w trakcie zapisu; wątek odczytu zgłosił już błąd.
            drop(guard);
            live.child.kill();
            return Ok(Sent { turn, sent });
        }
        let old = inner.live.replace(live);
        if let Some(rx) = rx {
            let engine = cfg.engine;
            let state = Arc::clone(&self.inner);
            let pseudonyms = Arc::clone(&self.pseudonyms);
            let sink = Arc::clone(&sink);
            std::thread::spawn(move || read_stream(state, pseudonyms, sink, engine, gen, rx));
        }
        drop(guard);
        // Poprzedni, martwy proces zostaje zastąpiony.
        if let Some(mut old) = old {
            old.child.kill();
        }
        Ok(Sent { turn, sent })
    }

    /// Faza bez blokady: nowy proces (z plikiem konfiguracji MCP) albo żywy proces
    /// Claude i zapis wiadomości na stdin.
    fn prepare(
        &self,
        cfg: &Config,
        gen: u64,
        reused: Option<Live>,
        payload: &str,
        close_input: bool,
    ) -> Result<Prepared, String> {
        let write = |child: &mut Box<dyn Child>| -> Result<(), String> {
            child.write(payload)?;
            if close_input {
                child.finish_input();
            }
            Ok(())
        };
        let failed = |e: String| {
            format!(
                "Nie udało się wysłać wiadomości do {}: {e}",
                cfg.engine.name()
            )
        };
        if let Some(mut live) = reused {
            if let Err(e) = write(&mut live.child) {
                live.child.kill();
                return Err(failed(e));
            }
            return Ok((live, None));
        }
        std::fs::create_dir_all(&cfg.dir)
            .map_err(|e| format!("Nie można utworzyć katalogu asystenta: {e}"))?;
        let spec = self.spec(cfg)?;
        let (mut child, rx) = self
            .spawner
            .spawn(&spec)
            .map_err(|e| format!("Nie udało się uruchomić {}: {e}", cfg.engine.name()))?;
        if let Err(e) = write(&mut child) {
            child.kill();
            return Err(failed(e));
        }
        Ok((Live { gen, child }, Some(rx)))
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
    ///
    /// `turn`: tylko ta tura (spóźniona wysyłka zatrzymana w interfejsie, zanim
    /// znał jej numer); inna trwająca tura i żywy proces zostają. `discard`: tura
    /// znika z historii (wysyłka sprzed „Nowej rozmowy”). Proces jest kończony po
    /// zwolnieniu blokady.
    pub fn stop(&self, turn: Option<u64>, discard: bool, sink: &dyn Sink) {
        let killed = {
            let mut guard = lock(&self.inner);
            let inner = &mut *guard;
            let matches = |active: &Option<Active>| match turn {
                None => true,
                Some(t) => active.as_ref().is_some_and(|a| a.turn == t),
            };
            let killed = if matches(&inner.active) {
                inner.epoch += 1;
                let live = inner.live.take();
                if let Some(active) = inner.active.take() {
                    if !active.text.is_empty() {
                        inner.history.push(Line {
                            role: Role::Assistant,
                            text: active.text,
                            turn: active.turn,
                        });
                    }
                    sink.emit(active.turn, Event::Stopped);
                }
                live
            } else {
                None
            };
            if let (Some(t), true) = (turn, discard) {
                inner.history.retain(|line| line.turn != t);
            }
            killed
        };
        if let Some(mut live) = killed {
            live.child.kill();
        }
    }

    /// „Nowa rozmowa” i koniec aplikacji: kończy proces i kasuje historię.
    /// Pseudonimy zostają: mapa jest wspólna z serwerem MCP, z którego mogą
    /// korzystać też sesje agentów poza czatem, a ta sama osoba ma mieć ten sam
    /// pseudonim w całym działaniu aplikacji.
    pub fn reset(&self) {
        let killed = {
            let mut guard = lock(&self.inner);
            let inner = &mut *guard;
            inner.epoch += 1;
            inner.active = None;
            inner.history.clear();
            inner.live.take()
        };
        if let Some(mut live) = killed {
            live.child.kill();
        }
    }
}

/// Wątek odczytu jednego procesu: parsuje linie, przywraca nazwy i wysyła zdarzenia.
fn read_stream(
    state: Arc<Mutex<Inner>>,
    pseudonyms: Arc<Mutex<Pseudonyms>>,
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
                    handle(&state, &pseudonyms, sink.as_ref(), engine, gen, parsed);
                }
            }
            Stream::Closed => {
                let mut guard = lock(&state);
                let inner = &mut *guard;
                inner.closed_gen = Some(gen);
                if inner.live.as_ref().is_some_and(|l| l.gen == gen) {
                    inner.live = None;
                }
                let Some(active) = inner.active.take_if(|a| a.gen == gen) else {
                    return;
                };
                let turn = active.turn;
                // Proces skończył bez wyniku: Codex po tekście bez `turn.completed`
                // to koniec odpowiedzi, reszta to ubity albo przerwany proces.
                if engine == Engine::Codex && !active.text.is_empty() {
                    inner.history.push(Line {
                        role: Role::Assistant,
                        text: active.text,
                        turn,
                    });
                    sink.emit(turn, Event::Done);
                    return;
                }
                let failure = ai::classify(&stderr.join("\n"));
                let message = match failure {
                    Failure::Other | Failure::Unreadable => UNEXPECTED_EXIT.to_string(),
                    other => ai::failure_message(engine, &other),
                };
                if !active.text.is_empty() {
                    inner.history.push(Line {
                        role: Role::Assistant,
                        text: active.text,
                        turn,
                    });
                }
                sink.emit(turn, Event::Error { message });
                return;
            }
        }
    }
}

fn handle(
    state: &Mutex<Inner>,
    pseudonyms: &Mutex<Pseudonyms>,
    sink: &dyn Sink,
    engine: Engine,
    gen: u64,
    parsed: Parsed,
) {
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
            let text = lock(pseudonyms).restore(&text);
            sink.emit(turn, Event::Text { text });
        }
        Parsed::Tool { name, input } => {
            let input = lock(pseudonyms).restore_value(&input);
            sink.emit(turn, Event::Tool { name, input });
        }
        Parsed::ToolDone { error } => sink.emit(turn, Event::ToolDone { error }),
        Parsed::Done(final_text) => {
            // Odpowiedź bez wcześniejszych akapitów: tekst wyniku jest jedyną treścią.
            if active.text.is_empty() && !final_text.trim().is_empty() {
                active.text = final_text.clone();
                let text = lock(pseudonyms).restore(&final_text);
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
            inner.history.push(Line {
                role: Role::Assistant,
                text: active.text,
                turn: active.turn,
            });
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

/// Rejestruje stan modułu; wołać po `mcp::setup` (wspólna mapa pseudonimów)
/// i przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    let pseudonyms = app
        .try_state::<crate::mcp::State>()
        .map(|mcp| mcp.pseudonyms())
        .unwrap_or_default();
    app.manage(State {
        manager: Manager::new(Arc::new(ProcessSpawner), pseudonyms),
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

/// „Zatrzymaj”: kończy odpowiedź i proces. `turn`: tylko ta tura (spóźniona
/// wysyłka), `discard`: usuń ją z historii (wysyłka sprzed „Nowej rozmowy”).
/// Poza wątkiem okna: kończenie procesu czeka na `taskkill`.
#[tauri::command]
pub async fn ai_chat_stop(
    app: tauri::AppHandle,
    turn: Option<u64>,
    discard: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(state) = app.try_state::<State>() {
            state
                .manager
                .stop(turn, discard.unwrap_or(false), &AppSink(app.clone()));
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// „Nowa rozmowa”: kończy proces i kasuje historię (poza wątkiem okna).
#[tauri::command]
pub async fn ai_chat_reset(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || shutdown(&app))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
