//! Sterownik przebiegów na wątkach std i mpsc, bez tokio (PLAN_WDROZENIA.md, 5.1 i 6).
//!
//! Jeden wątek roboczy przetwarza przebiegi po kolei (FIFO), więc w jednej chwili
//! działa najwyżej jeden krok agenta. Krok agenta idzie na osobnym wątku, a
//! wątek roboczy czeka na wynik przez `recv_timeout`. Repozytorium jest
//! wołane tylko krótko: nigdy w trakcie pracy runnera.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::engine::{decide, Effect, EngineError, Input, InputRef, RunEvent, RunState, Transition};
use crate::model::{AgentConfig, WorkflowGraph};
use crate::ports::{
    resolve_inputs, AgentRunner, EventSink, RepoError, RunId, RunRepository, RunRequest,
    RunnerEvent, RunnerOutcome, StoredEvent, WorkspaceAccess,
};
use crate::run::{FailureCode, RunStatus};
use crate::support::{lock, replay_stored};

mod decide;
mod messages;

pub use decide::{DecideError, DecideRequest, DecideResult, MAX_APPROVAL_COMMENT_CHARS};
use messages::MessageBuffer;
pub use messages::{MAX_RUNNER_EVENTS_PER_EXECUTION, MAX_RUNNER_PAYLOAD_BYTES, RUNNER_EVENT_BATCH};

/// Co ile sterownik sprawdza komendy i limity czasu podczas czekania na krok.
pub const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// Czas, jaki runner ma na reakcję na anulowanie (przez użytkownika albo po limicie kroku).
pub const CANCEL_GRACE: Duration = Duration::from_secs(5);
/// Ile razy ponawiamy zapis po konflikcie sekwencji (po ponownym wczytaniu stanu).
pub const MAX_APPEND_RETRIES: u32 = 3;

pub fn default_step_timeout(config: &AgentConfig) -> Duration {
    Duration::from_secs(u64::from(config.timeout_secs))
}

pub fn system_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn stderr_report(message: &str) {
    eprintln!("[workflow-core] {message}");
}

/// Ustawienia sterownika; testy skracają limity do milisekund.
#[derive(Clone)]
pub struct DriverConfig {
    pub step_timeout: fn(&AgentConfig) -> Duration,
    pub cancel_grace: Duration,
    pub poll_interval: Duration,
    pub now: fn() -> i64,
    /// Błędy, których nie da się zwrócić wołającemu (wątek w tle).
    pub on_error: fn(&str),
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            step_timeout: default_step_timeout,
            cancel_grace: CANCEL_GRACE,
            poll_interval: POLL_INTERVAL,
            now: system_now,
            on_error: stderr_report,
        }
    }
}

#[derive(Debug)]
enum DriverError {
    Repo(RepoError),
    Engine(EngineError),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverError::Repo(e) => write!(f, "{e}"),
            DriverError::Engine(e) => write!(f, "{e}"),
        }
    }
}

impl From<RepoError> for DriverError {
    fn from(e: RepoError) -> Self {
        DriverError::Repo(e)
    }
}

impl From<EngineError> for DriverError {
    fn from(e: EngineError) -> Self {
        DriverError::Engine(e)
    }
}

enum Command {
    Start(RunId),
    Decide(DecideRequest, Sender<Result<DecideResult, DecideError>>),
    Cancel(RunId),
    Wake,
    Shutdown,
}

/// Licznik komend jeszcze nie rozliczonych; pozwala testom poczekać na bezczynność.
struct Idle {
    pending: Mutex<usize>,
    changed: Condvar,
}

impl Idle {
    fn add(&self) {
        *lock(&self.pending) += 1;
    }

    fn sub(&self, count: usize) {
        let mut pending = lock(&self.pending);
        *pending = pending.saturating_sub(count);
        self.changed.notify_all();
    }
}

pub struct RuntimeHandle {
    tx: Sender<Command>,
    idle: Arc<Idle>,
    join: Option<JoinHandle<()>>,
}

impl RuntimeHandle {
    fn send(&self, command: Command) {
        self.idle.add();
        if self.tx.send(command).is_err() {
            self.idle.sub(1);
        }
    }

    /// Kolejkuje przebieg do wykonania (po `create_run`).
    pub fn start_run(&self, run_id: RunId) {
        self.send(Command::Start(run_id));
    }

    pub fn cancel(&self, run_id: RunId) {
        self.send(Command::Cancel(run_id));
    }

    /// Ponownie przegląda aktywne przebiegi (też robi to przy starcie).
    pub fn wake(&self) {
        self.send(Command::Wake);
    }

    pub fn decide(&self, request: DecideRequest) -> Result<DecideResult, DecideError> {
        let (reply, answer) = mpsc::channel();
        self.send(Command::Decide(request, reply));
        answer
            .recv()
            .map_err(|_| DecideError::Storage("Sterownik przebiegów jest zatrzymany.".into()))?
    }

    /// Czeka, aż wszystkie komendy i wynikające z nich kroki się skończą.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut pending = lock(&self.idle.pending);
        while *pending > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            pending = self
                .idle
                .changed
                .wait_timeout(pending, left)
                .map(|(guard, _)| guard)
                .unwrap_or_else(|e| e.into_inner().0);
        }
        true
    }

    /// Zatrzymuje wątek roboczy; krok w toku zostaje bez wyniku (po restarcie: Interrupted).
    pub fn shutdown(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        let _ = self.tx.send(Command::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for RuntimeHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct Runtime;

impl Runtime {
    pub fn start(
        repo: Arc<dyn RunRepository>,
        runner: Arc<dyn AgentRunner>,
        sink: Arc<dyn EventSink>,
        workspace: Arc<dyn WorkspaceAccess>,
        config: DriverConfig,
    ) -> RuntimeHandle {
        let (tx, rx) = mpsc::channel();
        let idle = Arc::new(Idle {
            pending: Mutex::new(0),
            changed: Condvar::new(),
        });
        let worker = Worker {
            repo,
            runner,
            sink,
            workspace,
            config,
            rx,
            queue: VecDeque::new(),
            current: None,
            consumed: 0,
            stopping: false,
            idle: Arc::clone(&idle),
        };
        let mut handle = RuntimeHandle {
            tx,
            idle,
            join: None,
        };
        handle.send(Command::Wake);
        handle.join = thread::Builder::new()
            .name("workflow-runtime".into())
            .spawn(move || worker.main_loop())
            .ok();
        handle
    }
}

struct InFlight {
    run_id: RunId,
    execution_id: String,
    cancel: Arc<AtomicBool>,
    user_cancelled: bool,
}

struct Worker {
    repo: Arc<dyn RunRepository>,
    runner: Arc<dyn AgentRunner>,
    sink: Arc<dyn EventSink>,
    workspace: Arc<dyn WorkspaceAccess>,
    config: DriverConfig,
    rx: Receiver<Command>,
    queue: VecDeque<RunId>,
    current: Option<InFlight>,
    consumed: usize,
    stopping: bool,
    idle: Arc<Idle>,
}

enum RunnerMessage {
    Event(RunnerEvent),
    Done(RunnerOutcome),
}

/// Jak skończyło się czekanie na krok agenta.
enum StepEnd {
    /// Sterownik się zatrzymuje; krok zostaje bez wyniku (po restarcie: Interrupted).
    Stopping,
    /// Runner zwrócił wynik.
    Finished(RunnerOutcome),
    /// Limit kroku minął albo użytkownik anulował, a runner skończył w czasie łaski.
    TimedOut,
    /// Runner nie zwrócił wyniku w czasie łaski; jego wątek nadal działa.
    Unresponsive(Receiver<RunnerMessage>),
}

struct Committed {
    state: RunState,
    transition: Transition,
}

impl Worker {
    fn report(&self, message: &str) {
        (self.config.on_error)(message);
    }

    fn main_loop(mut self) {
        while !self.stopping {
            if self.queue.is_empty() {
                self.idle.sub(std::mem::take(&mut self.consumed));
                match self.rx.recv() {
                    Ok(command) => self.handle(command),
                    Err(_) => break,
                }
            }
            while let Ok(command) = self.rx.try_recv() {
                self.handle(command);
            }
            if self.stopping {
                break;
            }
            if let Some(run_id) = self.queue.pop_front() {
                self.advance_run(run_id);
            }
        }
        self.idle.sub(std::mem::take(&mut self.consumed));
    }

    fn enqueue(&mut self, run_id: RunId) {
        if !self.queue.contains(&run_id) {
            self.queue.push_back(run_id);
        }
    }

    fn handle(&mut self, command: Command) {
        self.consumed += 1;
        match command {
            Command::Start(run_id) => self.enqueue(run_id),
            Command::Wake => match self.repo.active_runs() {
                Ok(runs) => runs.into_iter().for_each(|id| self.enqueue(id)),
                Err(e) => self.report(&format!("Nie udało się wczytać aktywnych przebiegów: {e}")),
            },
            Command::Cancel(run_id) => self.cancel_run(run_id),
            Command::Decide(request, reply) => {
                let result = self.decide_approval(&request);
                let _ = reply.send(result);
            }
            Command::Shutdown => self.stopping = true,
        }
    }

    fn load(&self, run_id: RunId) -> Result<(WorkflowGraph, RunState), RepoError> {
        let graph = self.repo.graph(run_id)?;
        let events = self.repo.load_events(run_id, 0)?;
        let state = replay_stored(&graph, &events)?;
        Ok((graph, state))
    }

    /// Zapisuje zdarzenia i dopiero po sukcesie przekazuje je do magistrali.
    fn commit(
        &self,
        run_id: RunId,
        expected_next: u64,
        events: &[RunEvent],
    ) -> Result<(), RepoError> {
        let stored: Vec<StoredEvent> =
            self.repo
                .append(run_id, expected_next, events, (self.config.now)())?;
        stored.iter().for_each(|e| self.sink.emit(e));
        Ok(())
    }

    /// Wczytaj → odtwórz → `decide` → zapisz; po konflikcie sekwencji powtarza od wczytania.
    fn step_with(
        &self,
        run_id: RunId,
        pick: impl Fn(&RunState) -> Option<Input>,
    ) -> Result<Option<Committed>, DriverError> {
        for _ in 0..=MAX_APPEND_RETRIES {
            let (graph, state) = self.load(run_id)?;
            let Some(input) = pick(&state) else {
                return Ok(None);
            };
            let transition = decide(&graph, &state, input)?;
            if !transition.events.is_empty() {
                match self.commit(run_id, state.next_sequence(), &transition.events) {
                    Ok(()) => {}
                    Err(RepoError::Conflict) => continue,
                    Err(e) => return Err(e.into()),
                }
            }
            let mut after = state;
            for event in &transition.events {
                after.apply(&graph, event).map_err(EngineError::from)?;
            }
            return Ok(Some(Committed {
                state: after,
                transition,
            }));
        }
        Err(RepoError::Conflict.into())
    }

    fn advance_run(&mut self, run_id: RunId) {
        loop {
            if self.stopping {
                return;
            }
            let pick = |state: &RunState| match (state.status, state.unfinished().is_some()) {
                (RunStatus::Queued, _) => Some(Input::Start),
                (RunStatus::Running, false) => Some(Input::Continue),
                (RunStatus::Running | RunStatus::Cancelling, true) => Some(Input::Recover),
                (
                    RunStatus::Cancelling
                    | RunStatus::WaitingForApproval
                    | RunStatus::Cancelled
                    | RunStatus::Succeeded
                    | RunStatus::Rejected
                    | RunStatus::Failed
                    | RunStatus::Interrupted,
                    _,
                ) => None,
            };
            let committed = match self.step_with(run_id, pick) {
                Ok(Some(committed)) => committed,
                Ok(None) => return,
                Err(e) => {
                    self.fail_after_error(run_id, "następny krok", &e);
                    return;
                }
            };
            for effect in committed.transition.effects.clone() {
                match effect {
                    Effect::LaunchAgent {
                        execution_id,
                        node_id,
                        config,
                        inputs,
                    } => self.run_step(run_id, &committed, execution_id, node_id, &config, &inputs),
                    Effect::RequestApproval {
                        approval_id,
                        execution_id,
                    } => self.prepare_approval(run_id, &approval_id, &execution_id),
                    Effect::CancelExecution { .. } => {}
                }
            }
        }
    }

    fn prepare_approval(&self, run_id: RunId, approval_id: &str, execution_id: &str) {
        let input = match self.workspace.artifact_hash(run_id) {
            Ok(artifact_hash) => Input::ApprovalPrepared {
                approval_id: approval_id.to_owned(),
                artifact_hash,
            },
            Err(message) => Input::ExecutionResult {
                execution_id: execution_id.to_owned(),
                outcome: RunnerOutcome::Failed {
                    code: FailureCode::StorageError,
                    message,
                },
            },
        };
        if let Err(e) = self.step_with(run_id, |_| Some(input.clone())) {
            self.report(&format!("Przebieg {run_id}: zgoda: {e}"));
        }
    }

    fn run_step(
        &mut self,
        run_id: RunId,
        committed: &Committed,
        execution_id: String,
        node_id: String,
        config: &AgentConfig,
        inputs: &[InputRef],
    ) {
        let timeout = (self.config.step_timeout)(config);
        // Krótka transakcja przed startem runnera; nigdy w trakcie jego pracy.
        // Błąd bazy kończy krok `StorageError` bez startu agenta: krok bez celu
        // użytkownika pracowałby na niepełnym promptcie. Brak celu (`Ok(None)`) jest
        // dozwolony.
        let goal = match self.repo.run_goal(run_id) {
            Ok(goal) => goal,
            Err(e) => {
                self.report(&format!("Przebieg {run_id}: cel przebiegu: {e}"));
                let outcome = RunnerOutcome::Failed {
                    code: FailureCode::StorageError,
                    message: "Nie udało się odczytać celu przebiegu z bazy; agent nie został uruchomiony."
                        .into(),
                };
                return self.record_outcome(run_id, &execution_id, outcome);
            }
        };
        let request = RunRequest {
            run_id,
            execution_id: execution_id.clone(),
            node_id,
            role: config.role,
            workspace: self.workspace.workspace_path(run_id),
            provider: config.provider,
            model: config.model.clone(),
            instruction: config.instruction.clone(),
            inputs: resolve_inputs(inputs, |id| {
                committed
                    .state
                    .execution(id)
                    .and_then(|e| e.output.as_ref())
                    .map(|o| o.to_value())
            }),
            access: config.access,
            timeout,
            session_id: None,
            goal,
        };
        let cancel = Arc::new(AtomicBool::new(false));
        self.current = Some(InFlight {
            run_id,
            execution_id: execution_id.clone(),
            cancel: Arc::clone(&cancel),
            user_cancelled: false,
        });
        let end = self.wait_for_runner(run_id, &execution_id, request, &cancel, timeout);
        let user_cancelled = self.current.take().is_some_and(|c| c.user_cancelled);
        let outcome = match end {
            StepEnd::Stopping => return,
            StepEnd::Finished(outcome) => outcome,
            StepEnd::TimedOut if user_cancelled => RunnerOutcome::Cancelled,
            StepEnd::TimedOut => RunnerOutcome::Failed {
                code: FailureCode::Timeout,
                message: FailureCode::Timeout.message().to_owned(),
            },
            StepEnd::Unresponsive(rx) => {
                return self.unresponsive_runner(run_id, &execution_id, &rx)
            }
        };
        self.record_outcome(run_id, &execution_id, outcome);
    }

    /// Zapisuje wynik kroku; trwały błąd zapisu kończy przebieg (`fail_after_error`).
    fn record_outcome(&self, run_id: RunId, execution_id: &str, outcome: RunnerOutcome) {
        let result = self.step_with(run_id, |_| {
            Some(Input::ExecutionResult {
                execution_id: execution_id.to_owned(),
                outcome: outcome.clone(),
            })
        });
        if let Err(e) = result {
            self.fail_after_error(run_id, "zapis wyniku kroku", &e);
        }
    }

    /// Runner nie potwierdził końca: zapisujemy `Interrupted` (nie `Cancelled` ani
    /// `Timeout`) i nie zwalniamy miejsca dla kolejnego kroku, dopóki wątek runnera
    /// żyje (PLAN_WDROZENIA.md, 5.1 i 8.3).
    fn unresponsive_runner(
        &mut self,
        run_id: RunId,
        execution_id: &str,
        rx: &Receiver<RunnerMessage>,
    ) {
        let input = Input::Unresponsive {
            execution_id: execution_id.to_owned(),
        };
        if let Err(e) = self.step_with(run_id, |_| Some(input.clone())) {
            self.fail_after_error(run_id, "krok bez reakcji na anulowanie", &e);
        }
        self.report(&format!(
            "Przebieg {run_id}: runner kroku {execution_id} nie zareagował na anulowanie; \
             kolejny krok poczeka na jego koniec."
        ));
        self.await_runner_exit(rx);
    }

    /// Obsługuje komendy, ale nie uruchamia kolejnego kroku, dopóki runner żyje.
    fn await_runner_exit(&mut self, rx: &Receiver<RunnerMessage>) {
        loop {
            while let Ok(command) = self.rx.try_recv() {
                self.handle(command);
            }
            if self.stopping {
                return;
            }
            match rx.recv_timeout(self.config.poll_interval) {
                // Komunikaty po zapisaniu końca kroku nie trafiają do historii.
                Ok(RunnerMessage::Event(_)) | Err(RecvTimeoutError::Timeout) => {}
                Ok(RunnerMessage::Done(_)) | Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    /// Trwały błąd zapisu albo silnika: kończy przebieg jako `Failed`, żeby nie
    /// wisiał w `Running`. Gdy i to się nie uda, zgłasza błąd na magistralę.
    fn fail_after_error(&self, run_id: RunId, context: &str, error: &DriverError) {
        let code = match error {
            DriverError::Repo(_) => FailureCode::StorageError,
            DriverError::Engine(EngineError::InvalidDefinition(_)) => {
                FailureCode::InvalidDefinition
            }
            DriverError::Engine(_) => FailureCode::ProtocolError,
        };
        let message = format!("{context}: {error}");
        self.report(&format!("Przebieg {run_id}: {message}"));
        let fault = Input::Fault {
            code,
            message: message.clone(),
        };
        let recorded = self.step_with(run_id, |state| {
            matches!(state.status, RunStatus::Running | RunStatus::Cancelling)
                .then(|| fault.clone())
        });
        match recorded {
            Ok(Some(_)) => {}
            Ok(None) => self.sink.fault(run_id, &message),
            Err(e) => self.sink.fault(run_id, &format!("{message} ({e})")),
        }
    }

    /// Czeka na wynik kroku z limitem czasu i czasem łaski po anulowaniu.
    fn wait_for_runner(
        &mut self,
        run_id: RunId,
        execution_id: &str,
        request: RunRequest,
        cancel: &Arc<AtomicBool>,
        timeout: Duration,
    ) -> StepEnd {
        let (tx, rx) = mpsc::channel();
        let runner = Arc::clone(&self.runner);
        let thread_cancel = Arc::clone(cancel);
        let thread_tx = tx.clone();
        let spawned = thread::Builder::new()
            .name("workflow-step".into())
            .spawn(move || {
                let mut on_event = |event| {
                    let _ = thread_tx.send(RunnerMessage::Event(event));
                };
                let outcome = runner.run(&request, &thread_cancel, &mut on_event);
                let _ = thread_tx.send(RunnerMessage::Done(outcome));
            });
        drop(tx);
        if spawned.is_err() {
            return StepEnd::Finished(RunnerOutcome::Failed {
                code: FailureCode::SpawnFailed,
                message: FailureCode::SpawnFailed.message().to_owned(),
            });
        }
        let deadline = Instant::now() + timeout;
        let mut grace_until: Option<Instant> = None;
        let mut buffer = MessageBuffer::new(execution_id);
        loop {
            while let Ok(command) = self.rx.try_recv() {
                self.handle(command);
            }
            if self.stopping {
                cancel.store(true, Ordering::SeqCst);
                self.flush(run_id, &mut buffer);
                return StepEnd::Stopping;
            }
            let now = Instant::now();
            // Anulowanie przez użytkownika ma własny, krótki limit (8.3): czas łaski
            // liczy się od anulowania, a nie dopiero po limicie kroku.
            let user_cancelled = self.current.as_ref().is_some_and(|c| c.user_cancelled);
            if user_cancelled && grace_until.is_none() {
                grace_until = Some(now + self.config.cancel_grace);
            }
            let wait = match grace_until {
                Some(until) => until.saturating_duration_since(now),
                None => deadline.saturating_duration_since(now),
            }
            .min(self.config.poll_interval);
            match rx.recv_timeout(wait) {
                Ok(RunnerMessage::Event(event)) => {
                    buffer.push(event);
                    if buffer.pending.len() >= RUNNER_EVENT_BATCH {
                        self.flush(run_id, &mut buffer);
                    }
                }
                Ok(RunnerMessage::Done(outcome)) => {
                    self.flush(run_id, &mut buffer);
                    return if grace_until.is_some() {
                        StepEnd::TimedOut
                    } else {
                        StepEnd::Finished(outcome)
                    };
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.flush(run_id, &mut buffer);
                    let now = Instant::now();
                    match grace_until {
                        None if now >= deadline => {
                            cancel.store(true, Ordering::SeqCst);
                            grace_until = Some(now + self.config.cancel_grace);
                        }
                        Some(until) if now >= until => {
                            // Wynik mógł przyjść tuż przed końcem łaski.
                            return match rx.try_recv() {
                                Ok(RunnerMessage::Done(_)) => StepEnd::TimedOut,
                                _ => StepEnd::Unresponsive(rx),
                            };
                        }
                        _ => {}
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.flush(run_id, &mut buffer);
                    return StepEnd::Finished(RunnerOutcome::Failed {
                        code: FailureCode::ProtocolError,
                        message: "Wątek runnera zakończył się bez wyniku.".into(),
                    });
                }
            }
        }
    }

    fn flush(&self, run_id: RunId, buffer: &mut MessageBuffer) {
        if buffer.pending.is_empty() {
            return;
        }
        let events = std::mem::take(&mut buffer.pending);
        for _ in 0..=MAX_APPEND_RETRIES {
            let next = match self.repo.last_sequence(run_id) {
                Ok(last) => last + 1,
                Err(e) => return self.report(&format!("Przebieg {run_id}: komunikaty: {e}")),
            };
            match self.commit(run_id, next, &events) {
                Ok(()) => return,
                Err(RepoError::Conflict) => continue,
                Err(e) => return self.report(&format!("Przebieg {run_id}: komunikaty: {e}")),
            }
        }
        self.report(&format!(
            "Przebieg {run_id}: komunikaty nie zostały zapisane."
        ));
    }

    fn cancel_run(&mut self, run_id: RunId) {
        let result = self.step_with(run_id, |_| Some(Input::CancelRequested));
        match result {
            Ok(Some(committed)) => {
                for effect in &committed.transition.effects {
                    if let Effect::CancelExecution { execution_id } = effect {
                        if let Some(current) = self.current.as_mut() {
                            if current.run_id == run_id && current.execution_id == *execution_id {
                                current.user_cancelled = true;
                                current.cancel.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => self.report(&format!("Przebieg {run_id}: anulowanie: {e}")),
        }
    }
}
