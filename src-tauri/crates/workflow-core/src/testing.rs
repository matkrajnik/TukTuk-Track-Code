//! Pomocnicy testów: deterministyczny runner, repozytorium w pamięci, ujście
//! nagrywające zdarzenia i przestrzeń robocza ze zmienialnym skrótem.
//! Dostępne w testach rdzenia i, przez funkcję `testing`, w testach hosta.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::engine::{decide, Effect, Input, RunEvent, RunState};
use crate::model::WorkflowGraph;
use crate::ports::{
    AgentRunner, CancelToken, Capabilities, CreatedRun, EventSink, NewRun, RepoError, RunId,
    RunRepository, RunRequest, RunnerEvent, RunnerOutcome, StoredEvent, WorkspaceAccess,
};
use crate::run::{FailureCode, RunStatus};
use crate::support::lock;
pub use crate::support::replay_stored;

/// Najdłuższe oczekiwanie skryptowanego `Hang`, żeby test nigdy nie wisiał.
const HANG_SAFETY: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(2);

pub use crate::samples;

#[derive(Debug, Clone)]
pub enum Action {
    Complete(Value),
    CompleteRaw {
        output: Value,
        exit_code: i32,
    },
    Fail(FailureCode, String),
    /// Czeka na anulowanie, potem zwraca `Cancelled`.
    Hang,
    /// Ignoruje anulowanie: śpi podany czas i dopiero wtedy zwraca `Cancelled`
    /// (runner, który nie reaguje w czasie łaski).
    Stubborn(Duration),
}

#[derive(Debug, Clone)]
pub struct Step {
    pub delay: Duration,
    pub events: Vec<RunnerEvent>,
    pub action: Action,
}

impl Step {
    fn of(action: Action) -> Self {
        Self {
            delay: Duration::ZERO,
            events: Vec::new(),
            action,
        }
    }

    pub fn complete(output: Value) -> Self {
        Self::of(Action::Complete(output))
    }

    pub fn complete_raw(output: Value, exit_code: i32) -> Self {
        Self::of(Action::CompleteRaw { output, exit_code })
    }

    pub fn fail(code: FailureCode, message: &str) -> Self {
        Self::of(Action::Fail(code, message.to_owned()))
    }

    pub fn hang() -> Self {
        Self::of(Action::Hang)
    }

    pub fn stubborn(duration: Duration) -> Self {
        Self::of(Action::Stubborn(duration))
    }

    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_events(mut self, events: Vec<RunnerEvent>) -> Self {
        self.events = events;
        self
    }
}

type Probe = Arc<dyn Fn(&RunRequest) + Send + Sync>;

/// Runner z kolejką odpowiedzi na każdy węzeł; liczy wywołania i równoległość.
#[derive(Default)]
pub struct ScriptedRunner {
    scripts: Mutex<HashMap<String, VecDeque<Step>>>,
    calls: Mutex<Vec<String>>,
    cancelled: Mutex<Vec<String>>,
    current: AtomicUsize,
    max: AtomicUsize,
    probe: Mutex<Option<Probe>>,
}

impl ScriptedRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dopisuje odpowiedzi dla węzła; kolejne wywołania biorą kolejne kroki.
    pub fn script(self, node_id: &str, steps: Vec<Step>) -> Self {
        lock(&self.scripts)
            .entry(node_id.to_owned())
            .or_default()
            .extend(steps);
        self
    }

    /// Skrypt całego szablonu: analiza, implementacja i `reviews` kolejnych review;
    /// po każdym `changes_requested` przychodzi poprawka.
    pub fn template_script(self, reviews: Vec<Value>) -> Self {
        let fixes = vec![Step::complete(samples::implementation()); reviews.len()];
        self.script("analysis", vec![Step::complete(samples::analysis())])
            .script(
                "implementation",
                vec![Step::complete(samples::implementation())],
            )
            .script("fix", fixes)
            .script("review", reviews.into_iter().map(Step::complete).collect())
    }

    pub fn with_probe(self, probe: impl Fn(&RunRequest) + Send + Sync + 'static) -> Self {
        *lock(&self.probe) = Some(Arc::new(probe));
        self
    }

    pub fn calls(&self, node_id: &str) -> usize {
        lock(&self.calls).iter().filter(|n| *n == node_id).count()
    }

    pub fn total_calls(&self) -> usize {
        lock(&self.calls).len()
    }

    /// Węzły w kolejności wywołań.
    pub fn call_order(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }

    /// Ile wywołań runnera trwa w tej chwili.
    pub fn running(&self) -> usize {
        self.current.load(Ordering::SeqCst)
    }

    pub fn max_concurrency(&self) -> usize {
        self.max.load(Ordering::SeqCst)
    }

    /// Węzły, których krok zobaczył sygnał anulowania.
    pub fn saw_cancel(&self, node_id: &str) -> bool {
        lock(&self.cancelled).iter().any(|n| n == node_id)
    }

    fn sleep(&self, delay: Duration, cancel: &CancelToken) -> bool {
        let until = Instant::now() + delay;
        while Instant::now() < until {
            if cancel.load(Ordering::SeqCst) {
                return false;
            }
            thread::sleep(POLL);
        }
        true
    }

    fn execute(
        &self,
        request: &RunRequest,
        step: Step,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
    ) -> RunnerOutcome {
        let cancelled = |node: &str| {
            lock(&self.cancelled).push(node.to_owned());
            RunnerOutcome::Cancelled
        };
        if !self.sleep(step.delay, cancel) {
            return cancelled(&request.node_id);
        }
        step.events.into_iter().for_each(&mut *on_event);
        match step.action {
            Action::Complete(output) => RunnerOutcome::Completed {
                output,
                exit_code: 0,
                session_id: None,
            },
            Action::CompleteRaw { output, exit_code } => RunnerOutcome::Completed {
                output,
                exit_code,
                session_id: None,
            },
            Action::Fail(code, message) => RunnerOutcome::Failed { code, message },
            Action::Hang => {
                let until = Instant::now() + HANG_SAFETY;
                while Instant::now() < until {
                    if cancel.load(Ordering::SeqCst) {
                        return cancelled(&request.node_id);
                    }
                    thread::sleep(POLL);
                }
                RunnerOutcome::Failed {
                    code: FailureCode::Timeout,
                    message: "skryptowany Hang bez anulowania".into(),
                }
            }
            Action::Stubborn(duration) => {
                thread::sleep(duration.min(HANG_SAFETY));
                cancelled(&request.node_id)
            }
        }
    }
}

impl AgentRunner for ScriptedRunner {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn run(
        &self,
        request: &RunRequest,
        cancel: &CancelToken,
        on_event: &mut dyn FnMut(RunnerEvent),
    ) -> RunnerOutcome {
        let now = self.current.fetch_add(1, Ordering::SeqCst) + 1;
        self.max.fetch_max(now, Ordering::SeqCst);
        lock(&self.calls).push(request.node_id.clone());
        let probe = lock(&self.probe).clone();
        if let Some(probe) = probe {
            probe(request);
        }
        let step = lock(&self.scripts)
            .get_mut(&request.node_id)
            .and_then(VecDeque::pop_front);
        let outcome = match step {
            Some(step) => self.execute(request, step, cancel, on_event),
            None => RunnerOutcome::Failed {
                code: FailureCode::ProtocolError,
                message: format!("brak skryptu dla węzła {}", request.node_id),
            },
        };
        self.current.fetch_sub(1, Ordering::SeqCst);
        outcome
    }
}

struct RunData {
    version_id: i64,
    request_id: String,
    graph: WorkflowGraph,
    events: Vec<StoredEvent>,
    status: RunStatus,
    goal: Option<String>,
}

#[derive(Default)]
struct Inner {
    graphs: HashMap<i64, WorkflowGraph>,
    runs: BTreeMap<RunId, RunData>,
}

/// Repozytorium w pamięci z tą samą semantyką sekwencji co SQLite.
#[derive(Default)]
pub struct MemoryRepository {
    inner: Mutex<Inner>,
}

fn fnv_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}

impl MemoryRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rejestruje „opublikowaną wersję” grafu dla `create_run`.
    pub fn register_graph(&self, version_id: i64, graph: WorkflowGraph) {
        lock(&self.inner).graphs.insert(version_id, graph);
    }

    /// Tworzy przebieg z grafu i zwraca jego id.
    pub fn create(&self, graph: &WorkflowGraph, request_id: &str) -> RunId {
        let version_id = 1;
        self.register_graph(version_id, graph.clone());
        let new = NewRun {
            version_id,
            request_id: request_id.to_owned(),
            ..NewRun::default()
        };
        self.create_run(&new).expect("create_run").run_id
    }

    /// Jak `create`, z opisem celu przebiegu.
    pub fn create_with_goal(&self, graph: &WorkflowGraph, request_id: &str, goal: &str) -> RunId {
        self.register_graph(1, graph.clone());
        let new = NewRun {
            version_id: 1,
            request_id: request_id.to_owned(),
            goal: Some(goal.to_owned()),
            ..NewRun::default()
        };
        self.create_run(&new).expect("create_run").run_id
    }

    pub fn state(&self, run_id: RunId) -> RunState {
        let graph = self.graph(run_id).expect("graph");
        let events = self.load_events(run_id, 0).expect("events");
        replay_stored(&graph, &events).expect("replay")
    }
}

impl RunRepository for MemoryRepository {
    fn create_run(&self, new: &NewRun) -> Result<CreatedRun, RepoError> {
        if new.request_id.is_empty() {
            return Err(RepoError::BadInput("Pusty request_id.".into()));
        }
        let mut inner = lock(&self.inner);
        if let Some((id, run)) = inner
            .runs
            .iter()
            .find(|(_, r)| r.request_id == new.request_id)
        {
            return if run.version_id == new.version_id {
                Ok(CreatedRun {
                    run_id: *id,
                    created: false,
                })
            } else {
                Err(RepoError::Conflict)
            };
        }
        let graph = inner
            .graphs
            .get(&new.version_id)
            .cloned()
            .ok_or(RepoError::NotFound)?;
        let run_id = inner.runs.keys().next_back().map_or(1, |id| id + 1);
        let graph_hash = fnv_hash(&serde_json::to_string(&graph).unwrap_or_default());
        let created = RunEvent::RunCreated { run_id, graph_hash };
        inner.runs.insert(
            run_id,
            RunData {
                version_id: new.version_id,
                request_id: new.request_id.clone(),
                graph,
                events: vec![StoredEvent::new(run_id, 1, new.now, &created)],
                status: RunStatus::Queued,
                goal: new.goal.clone(),
            },
        );
        Ok(CreatedRun {
            run_id,
            created: true,
        })
    }

    fn graph(&self, run_id: RunId) -> Result<WorkflowGraph, RepoError> {
        lock(&self.inner)
            .runs
            .get(&run_id)
            .map(|r| r.graph.clone())
            .ok_or(RepoError::NotFound)
    }

    fn append(
        &self,
        run_id: RunId,
        expected_next_sequence: u64,
        events: &[RunEvent],
        now: i64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let mut inner = lock(&self.inner);
        let run = inner.runs.get_mut(&run_id).ok_or(RepoError::NotFound)?;
        if expected_next_sequence != run.events.len() as u64 + 1 {
            return Err(RepoError::Conflict);
        }
        let stored: Vec<StoredEvent> = events
            .iter()
            .enumerate()
            .map(|(i, e)| StoredEvent::new(run_id, expected_next_sequence + i as u64, now, e))
            .collect();
        for event in events {
            if let RunEvent::RunStateChanged { to, .. } = event {
                run.status = *to;
            }
        }
        run.events.extend(stored.iter().cloned());
        Ok(stored)
    }

    fn load_events(
        &self,
        run_id: RunId,
        after_sequence: u64,
    ) -> Result<Vec<StoredEvent>, RepoError> {
        let inner = lock(&self.inner);
        let run = inner.runs.get(&run_id).ok_or(RepoError::NotFound)?;
        Ok(run
            .events
            .iter()
            .filter(|e| e.sequence > after_sequence)
            .cloned()
            .collect())
    }

    fn last_sequence(&self, run_id: RunId) -> Result<u64, RepoError> {
        lock(&self.inner)
            .runs
            .get(&run_id)
            .map(|r| r.events.len() as u64)
            .ok_or(RepoError::NotFound)
    }

    fn run_goal(&self, run_id: RunId) -> Result<Option<String>, RepoError> {
        lock(&self.inner)
            .runs
            .get(&run_id)
            .map(|r| r.goal.clone())
            .ok_or(RepoError::NotFound)
    }

    fn active_runs(&self) -> Result<Vec<RunId>, RepoError> {
        Ok(lock(&self.inner)
            .runs
            .iter()
            .filter(|(_, r)| !r.status.is_terminal())
            .map(|(id, _)| *id)
            .collect())
    }
}

#[derive(Default)]
pub struct RecordingSink {
    events: Mutex<Vec<StoredEvent>>,
    faults: Mutex<Vec<(RunId, String)>>,
}

impl RecordingSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> Vec<StoredEvent> {
        lock(&self.events).clone()
    }

    /// Błędy przebiegów zgłoszone poza historią zdarzeń.
    pub fn faults(&self) -> Vec<(RunId, String)> {
        lock(&self.faults).clone()
    }
}

impl EventSink for RecordingSink {
    fn emit(&self, event: &StoredEvent) {
        lock(&self.events).push(event.clone());
    }

    fn fault(&self, run_id: RunId, message: &str) {
        lock(&self.faults).push((run_id, message.to_owned()));
    }
}

/// Przestrzeń robocza ze zmienialnym skrótem artefaktu.
pub struct FixedWorkspace {
    hash: Mutex<String>,
    fail: AtomicBool,
}

impl FixedWorkspace {
    pub fn new(hash: &str) -> Self {
        Self {
            hash: Mutex::new(hash.to_owned()),
            fail: AtomicBool::new(false),
        }
    }

    pub fn set_hash(&self, hash: &str) {
        *lock(&self.hash) = hash.to_owned();
    }

    pub fn fail_next(&self, fail: bool) {
        self.fail.store(fail, Ordering::SeqCst);
    }
}

impl WorkspaceAccess for FixedWorkspace {
    fn artifact_hash(&self, _run_id: RunId) -> Result<String, String> {
        if self.fail.load(Ordering::SeqCst) {
            return Err("skrót niedostępny".into());
        }
        Ok(lock(&self.hash).clone())
    }
}

/// Stosuje kolejno wejścia przez `decide` i zapisuje wynik w repozytorium, bez
/// sterownika. Służy do przygotowania historii (np. granicy kroków) w testach.
pub fn apply_inputs(
    repo: &dyn RunRepository,
    run_id: RunId,
    inputs: Vec<Input>,
) -> Result<Vec<Effect>, RepoError> {
    let graph = repo.graph(run_id)?;
    let mut effects = Vec::new();
    for input in inputs {
        let events = repo.load_events(run_id, 0)?;
        let state = replay_stored(&graph, &events)?;
        let transition =
            decide(&graph, &state, input).map_err(|e| RepoError::BadInput(e.to_string()))?;
        repo.append(run_id, state.next_sequence(), &transition.events, 0)?;
        effects.extend(transition.effects);
    }
    Ok(effects)
}

/// Komunikat runnera do skryptów testów.
pub fn message(text: &str) -> RunnerEvent {
    RunnerEvent::Message {
        text: text.to_owned(),
    }
}
