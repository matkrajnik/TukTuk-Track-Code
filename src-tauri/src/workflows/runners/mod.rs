//! Adaptery agentów Workflows: Claude Code i Codex (PLAN_WDROZENIA.md, 8.1, E4).
//!
//! `CliRunner` implementuje `AgentRunner` nad wstrzykiwanym `Spawner`; testy
//! podstawiają skryptowane strumienie, aplikacja prawdziwy proces. Runner nie
//! dotyka bazy: wyniki zapisuje sterownik krótkimi transakcjami.
#![cfg_attr(not(test), allow(dead_code))]

pub mod claude;
pub mod codex;
pub mod env;
pub mod guard;
pub mod process;
pub mod prompt;

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use agent_cli::{
    parse_claude_line_raw, parse_codex_line_raw, Failure, Parsed, ProcessEvent, SpawnError, Spec,
};
use serde_json::Value;
use workflow_core::model::{AgentRole, Provider};
use workflow_core::output::json_schema;
use workflow_core::ports::{
    AgentRunner, CancelToken, Capabilities, RunRequest, RunnerEvent, RunnerOutcome, WorkspaceAccess,
};
use workflow_core::FailureCode;

use process::{Proc, Spawner};

use super::processes::{ProcState, ProcessLedger};

/// Jak często runner sprawdza `cancel`.
const POLL: Duration = Duration::from_millis(50);
/// Czas na potwierdzenie końca procesu po `kill`.
const KILL_CONFIRM: Duration = Duration::from_secs(5);
/// Najdłużej czekamy na koniec procesu, którego `kill` nie został potwierdzony
/// (domyślnie; testy skracają przez [`CliRunner::with_unconfirmed_max_wait`]).
pub const UNCONFIRMED_MAX_WAIT: Duration = Duration::from_secs(600);
/// Ile znaków stderr trafia do komunikatu błędu.
const STDERR_MESSAGE_CHARS: usize = 300;
/// Token-podobny ciąg: co najmniej tyle znaków bez spacji.
const SECRET_MIN_LEN: usize = 24;

/// Gdzie leżą programy CLI i dane aplikacji.
#[derive(Debug, Clone)]
pub struct CliPaths {
    pub claude: Option<PathBuf>,
    pub codex: Option<PathBuf>,
    pub app_data: PathBuf,
}

pub struct CliRunner {
    spawner: Arc<dyn Spawner>,
    paths: CliPaths,
    /// Dostawca ścieżek: CLI zainstalowane po starcie aplikacji działa bez restartu.
    paths_provider: Option<Arc<dyn Fn() -> CliPaths + Send + Sync>>,
    workspace: Arc<dyn WorkspaceAccess>,
    unconfirmed_max_wait: Duration,
    /// Rejestr procesów (E6): bez niego proces po awarii hosta nie dałby się sprawdzić.
    ledger: Option<Arc<dyn ProcessLedger>>,
}

impl CliRunner {
    pub fn new(
        spawner: Arc<dyn Spawner>,
        paths: CliPaths,
        workspace: Arc<dyn WorkspaceAccess>,
    ) -> Self {
        Self {
            spawner,
            paths,
            paths_provider: None,
            workspace,
            unconfirmed_max_wait: UNCONFIRMED_MAX_WAIT,
            ledger: None,
        }
    }

    /// Zapisuje każdy uruchomiony proces (PID i czas utworzenia) oraz jego koniec.
    pub fn with_ledger(mut self, ledger: Arc<dyn ProcessLedger>) -> Self {
        self.ledger = Some(ledger);
        self
    }

    /// Ścieżki liczone przy każdym kroku zamiast jednorazowo przy starcie.
    pub fn with_paths_provider(
        mut self,
        provider: Arc<dyn Fn() -> CliPaths + Send + Sync>,
    ) -> Self {
        self.paths_provider = Some(provider);
        self
    }

    fn paths(&self) -> CliPaths {
        match &self.paths_provider {
            Some(provider) => provider(),
            None => self.paths.clone(),
        }
    }

    /// Jak długo czekać na koniec procesu po niepotwierdzonym `kill`.
    pub fn with_unconfirmed_max_wait(mut self, wait: Duration) -> Self {
        self.unconfirmed_max_wait = wait;
        self
    }
}

fn failed(code: FailureCode, message: impl Into<String>) -> RunnerOutcome {
    RunnerOutcome::Failed {
        code,
        message: message.into(),
    }
}

/// Słowo przypominające token: długie, bez ścieżek, tylko znaki typowe dla kluczy.
pub(crate) fn is_secret_word(word: &str) -> bool {
    word.len() >= SECRET_MIN_LEN
        && !word.contains(['\\', '/'])
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.=".contains(c))
}

/// Maskuje ciągi przypominające tokeny i nagłówki autoryzacji; skraca tekst.
pub fn redact(text: &str) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut hide_next = false;
    for word in text.split_whitespace() {
        if hide_next {
            words.push("[ukryte]".into());
            hide_next = false;
        } else if word.eq_ignore_ascii_case("bearer") || word.eq_ignore_ascii_case("token:") {
            words.push(word.to_owned());
            hide_next = true;
        } else if is_secret_word(word) {
            words.push("[ukryte]".into());
        } else {
            words.push(word.to_owned());
        }
    }
    words.join(" ").chars().take(STDERR_MESSAGE_CHARS).collect()
}

fn protocol_error(proc: &dyn Proc, what: &str) -> RunnerOutcome {
    let tail = proc.stderr_tail();
    let detail = redact(&tail.text);
    let message = if detail.is_empty() {
        what.to_owned()
    } else {
        format!("{what} Koniec stderr: {detail}")
    };
    failed(FailureCode::ProtocolError, message)
}

/// Stan odczytu strumienia jednego kroku.
#[derive(Default)]
struct Stream {
    session_id: Option<String>,
    structured: Option<Value>,
    last_text: Option<String>,
    done: bool,
    login_failed: bool,
    other_error: Option<Failure>,
    too_long: bool,
    tools: Vec<String>,
}

impl Stream {
    fn apply(&mut self, provider: Provider, line: &str, on_event: &mut dyn FnMut(RunnerEvent)) {
        if provider == Provider::Claude {
            if let Ok(event) = serde_json::from_str::<Value>(line.trim()) {
                if event["type"] == "result" && !event["structured_output"].is_null() {
                    self.structured = Some(event["structured_output"].clone());
                }
            }
        }
        let parsed = match provider {
            Provider::Claude => parse_claude_line_raw(line),
            Provider::Codex => parse_codex_line_raw(line),
        };
        for item in parsed {
            match item {
                Parsed::Session(id) => self.session_id = Some(id),
                Parsed::Text(text) => {
                    self.last_text = Some(text.clone());
                    on_event(RunnerEvent::Message { text });
                }
                Parsed::Tool { name, .. } => {
                    self.tools.push(name.clone());
                    on_event(RunnerEvent::ToolStarted { name });
                }
                Parsed::ToolDone { error } => {
                    let name = self.tools.pop().unwrap_or_default();
                    on_event(RunnerEvent::ToolFinished { name, ok: !error });
                }
                Parsed::Done(_) => self.done = true,
                Parsed::Error(Failure::NotLoggedIn) => self.login_failed = true,
                Parsed::Error(other) => self.other_error = Some(other),
            }
        }
    }
}

/// Po niepotwierdzonym `kill` ponawia kończenie drzewa, aż jego koniec zostanie
/// potwierdzony (`Cancelled`) albo minie `max_wait` (`Interrupted`).
///
/// `ProcessEvent::Exited` nie wystarcza: w prawdziwym procesie oznacza koniec
/// tylko bezpośredniego potomka, a wnuki mogą dalej działać (PLAN 8.3).
fn wait_for_unconfirmed_end(proc: &dyn Proc, max_wait: Duration) -> RunnerOutcome {
    let deadline = Instant::now() + max_wait;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if proc.kill(remaining.min(KILL_CONFIRM)) == agent_cli::KillResult::Confirmed {
            return RunnerOutcome::Cancelled;
        }
        std::thread::sleep(POLL.min(deadline.saturating_duration_since(Instant::now())));
    }
    failed(
        FailureCode::Interrupted,
        "Nie udało się potwierdzić zakończenia procesu agenta.",
    )
}

struct Prepared {
    spec: Spec,
    out_file: Option<PathBuf>,
}

impl CliRunner {
    fn prepare(&self, request: &RunRequest, worktree: &str) -> Result<Prepared, RunnerOutcome> {
        let hash = if request.role == AgentRole::Review {
            Some(
                self.workspace
                    .artifact_hash(request.run_id)
                    .map_err(|e| failed(FailureCode::StorageError, e))?,
            )
        } else {
            None
        };
        let prompt = prompt::build(request, hash.as_deref())
            .map_err(|e| failed(FailureCode::ProtocolError, e))?;
        let schema = json_schema(request.role);
        let worktree_path = PathBuf::from(worktree);
        let env = env::from_process(request.run_id, &request.execution_id);
        let paths = self.paths();
        match request.provider {
            Provider::Claude => {
                let program = paths.claude.clone().ok_or_else(|| {
                    failed(FailureCode::CliMissing, FailureCode::CliMissing.message())
                })?;
                let session =
                    claude::new_session_id().map_err(|e| failed(FailureCode::SpawnFailed, e))?;
                let args = claude::args(&schema, &session, request.model.as_deref(), request.access)
                    .ok_or_else(|| {
                        failed(
                            FailureCode::PermissionViolation,
                            "Claude nie ma w MVP egzekwowanego ograniczenia zapisu do katalogu roboczego.",
                        )
                    })?;
                Ok(Prepared {
                    spec: Spec {
                        program,
                        args,
                        stdin: prompt,
                        cwd: Some(worktree_path),
                        env,
                    },
                    out_file: None,
                })
            }
            Provider::Codex => {
                let program = paths.codex.clone().ok_or_else(|| {
                    failed(FailureCode::CliMissing, FailureCode::CliMissing.message())
                })?;
                let dir = paths
                    .app_data
                    .join("workflows")
                    .join("runs")
                    .join(request.run_id.to_string())
                    .join(&request.execution_id);
                std::fs::create_dir_all(&dir).map_err(|e| {
                    failed(
                        FailureCode::StorageError,
                        format!("Nie można utworzyć katalogu kroku: {e}"),
                    )
                })?;
                let schema_file = dir.join("schema.json");
                let out_file = dir.join("result.json");
                std::fs::write(&schema_file, schema.to_string()).map_err(|e| {
                    failed(
                        FailureCode::StorageError,
                        format!("Nie można zapisać schematu: {e}"),
                    )
                })?;
                let args = codex::args(
                    &worktree_path,
                    &schema_file,
                    &out_file,
                    request.model.as_deref(),
                    request.access,
                )
                .map_err(|e| failed(FailureCode::StorageError, e))?;
                Ok(Prepared {
                    spec: Spec {
                        program,
                        args,
                        stdin: prompt,
                        cwd: Some(worktree_path),
                        env,
                    },
                    out_file: Some(out_file),
                })
            }
        }
    }
}

impl AgentRunner for CliRunner {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            read_only_enforced: false,
            resumable_sessions: false,
        }
    }

    fn run(
        &self,
        request: &RunRequest,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
    ) -> RunnerOutcome {
        let Some(worktree) = request.workspace.as_deref() else {
            return failed(
                FailureCode::StorageError,
                "Przebieg nie ma katalogu roboczego; krok nie zostanie uruchomiony.",
            );
        };
        let prepared = match self.prepare(request, worktree) {
            Ok(p) => p,
            Err(outcome) => return outcome,
        };
        let proc = match self.spawner.spawn(&prepared.spec) {
            Ok(p) => p,
            Err(SpawnError::NotFound(_)) => {
                return failed(FailureCode::CliMissing, FailureCode::CliMissing.message())
            }
            Err(SpawnError::Failed(e)) => {
                return failed(
                    FailureCode::SpawnFailed,
                    format!("{} {}", FailureCode::SpawnFailed.message(), redact(&e)),
                )
            }
        };
        let recorded = match self.record_spawn(request, proc.as_ref()) {
            Ok(recorded) => recorded,
            Err(outcome) => return outcome,
        };
        let outcome = self.drive(request, cancel, on_event, proc.as_ref(), &prepared);
        if recorded {
            self.record_end(request, &outcome);
        }
        outcome
    }
}

/// Stan rejestru po wyniku kroku: anulowanie z potwierdzonym końcem to `Killed`,
/// niepotwierdzony koniec (`Interrupted`) to `Unknown`, reszta to zwykłe wyjście.
fn ledger_state(outcome: &RunnerOutcome) -> ProcState {
    match outcome {
        RunnerOutcome::Cancelled => ProcState::Killed,
        RunnerOutcome::Failed {
            code: FailureCode::Interrupted,
            ..
        } => ProcState::Unknown,
        _ => ProcState::Exited,
    }
}

impl CliRunner {
    /// Zapis procesu w rejestrze przed czytaniem strumienia. Gdy zapis się nie uda,
    /// proces jest kończony, a krok kończy się błędem zapisu: proces bez wpisu nie
    /// dałby się sprawdzić po awarii hosta.
    fn record_spawn(&self, request: &RunRequest, proc: &dyn Proc) -> Result<bool, RunnerOutcome> {
        let (Some(ledger), Some(identity)) = (&self.ledger, proc.identity()) else {
            return Ok(false);
        };
        match ledger.spawned(request.run_id, &request.execution_id, &identity) {
            Ok(()) => Ok(true),
            Err(e) => {
                let ended = proc.kill(KILL_CONFIRM);
                let message = match ended {
                    agent_cli::KillResult::Confirmed => {
                        format!("Nie zapisano procesu agenta w rejestrze ({e}); proces zakończono.")
                    }
                    agent_cli::KillResult::Unconfirmed => {
                        return Err(failed(
                            FailureCode::Interrupted,
                            format!("Nie zapisano procesu agenta w rejestrze ({e}) i nie potwierdzono jego końca."),
                        ))
                    }
                };
                Err(failed(FailureCode::StorageError, message))
            }
        }
    }

    /// Błąd zapisu końca zostawia wpis `running`; rekonsyliacja przy starcie sprawdzi
    /// go po PID i czasie utworzenia, więc tu wystarczy zgłoszenie w logu.
    fn record_end(&self, request: &RunRequest, outcome: &RunnerOutcome) {
        if let Some(ledger) = &self.ledger {
            if let Err(e) = ledger.ended(&request.execution_id, ledger_state(outcome)) {
                eprintln!(
                    "Workflows: nie zapisano końca procesu kroku {}: {e}",
                    request.execution_id
                );
            }
        }
    }
}

impl CliRunner {
    fn drive(
        &self,
        request: &RunRequest,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
        proc: &dyn Proc,
        prepared: &Prepared,
    ) -> RunnerOutcome {
        let mut stream = Stream::default();
        let exit_code = loop {
            if cancel.load(Ordering::SeqCst) {
                return match proc.kill(KILL_CONFIRM) {
                    agent_cli::KillResult::Confirmed => RunnerOutcome::Cancelled,
                    agent_cli::KillResult::Unconfirmed => {
                        wait_for_unconfirmed_end(proc, self.unconfirmed_max_wait)
                    }
                };
            }
            match proc.next(POLL) {
                None => {}
                Some(ProcessEvent::Line(line)) => {
                    stream.apply(request.provider, &line, on_event);
                }
                Some(ProcessEvent::LineTooLong) => stream.too_long = true,
                Some(ProcessEvent::Exited(code)) => break code,
            }
        };
        self.finish(request, proc, prepared, stream, exit_code)
    }

    fn finish(
        &self,
        request: &RunRequest,
        proc: &dyn Proc,
        prepared: &Prepared,
        stream: Stream,
        exit_code: Option<i32>,
    ) -> RunnerOutcome {
        if stream.login_failed {
            return failed(FailureCode::NotLoggedIn, FailureCode::NotLoggedIn.message());
        }
        let output = match request.provider {
            Provider::Claude => stream.structured.clone(),
            Provider::Codex => codex_result(prepared, &stream),
        };
        match output {
            Some(output) => RunnerOutcome::Completed {
                output,
                exit_code: exit_code.unwrap_or(-1),
                session_id: stream.session_id,
            },
            None if stream.too_long => protocol_error(
                proc,
                "Agent zwrócił linię dłuższą niż limit i nie przekazał wyniku.",
            ),
            None if stream.other_error.is_some() => {
                protocol_error(proc, "Agent zakończył krok błędem.")
            }
            None => protocol_error(proc, "Agent nie zwrócił zdarzenia z wynikiem."),
        }
    }
}

/// Wynik Codexa: plik `-o`, a gdy go brak, ostatnia wiadomość agenta. Tekst, który
/// nie jest JSON-em, przechodzi jako string: walidację robi silnik (`InvalidOutput`).
fn codex_result(prepared: &Prepared, stream: &Stream) -> Option<Value> {
    let from_file = prepared
        .out_file
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .filter(|t| !t.trim().is_empty());
    let text = from_file.or_else(|| stream.last_text.clone().filter(|_| stream.done))?;
    Some(serde_json::from_str(text.trim()).unwrap_or(Value::String(text)))
}

#[cfg(test)]
mod tests;
