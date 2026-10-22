//! Asystent przypisań (PLAN_BACKLOG.md, A4).
//!
//! Dwie funkcje, obie za przełącznikiem `ai_assign_suggest` i wybranym silnikiem AI:
//!
//! 1. **Nieprzypisana sesja agenta.** Sesja pracuje co najmniej
//!    [`suggest::MIN_WORK`], `agents::resolve` nie dało dopasowania, folder nie jest
//!    wyciszony: `ai::ask("assign", …)` dostaje nazwę folderu (bez ścieżki), gałąź,
//!    pierwszy prompt sesji (B8, do 500 znaków, tylko w pamięci) i listę otwartych
//!    zadań (id, tytuł, projekt, klucz). Odpowiedź `{ taskId | null, confidence,
//!    reason }` z `confidence ≥ 0,6` staje się podpowiedzią (`Agent::assist`, źródło
//!    `assistant`), którą `suggest.rs` pokazuje w banerze: „Przypisz folder”
//!    (`agent_bind`), „Przełącz timer” (`agent_switch`), „Nie” (`agent_dismiss`).
//!    Jedno wywołanie na `session_id` ([`Memory`]): także nieudane i odrzucone nie
//!    wracają, dopóki sesja żyje. Gdy heurystyka dopasowała zadanie, modelu nie
//!    wołamy w ogóle.
//! 2. **Zadanie TS dla zadania.** `ai::ask("ts_task", …)` z tytułem, projektem,
//!    kluczem, notatką dnia i listą zadań TS ([`ai_suggest_ts_task`]); wynik tylko
//!    wypełnia sugestię w interfejsie, zapis dopiero po wyborze użytkownika.
//!
//! Dane wejściowe przechodzą przez `ai_privacy::redact` wewnątrz `ai::run` (nazwiska
//! ze słownika znikają, zob. test szczelności); wywołanie dzieje się tylko na
//! zdarzenie (nowa sesja, kliknięcie), nigdy w pętli. Model tylko proponuje: timer,
//! TS i przypisania zmienia wyłącznie kliknięcie.
use crate::agents::{self, Agent, Match};
use crate::ai::{self, Tier};
use crate::settings::AppSettings;
use crate::{db, suggest, timesheet};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use tauri::Manager;

/// Rodzaj wywołania w dzienniku „Ostatnie wywołania”: sesja agenta → zadanie.
pub const KIND_ASSIGN: &str = "assign";
/// Rodzaj wywołania: zadanie → zadanie TS.
pub const KIND_TS_TASK: &str = "ts_task";
/// `Match::source` podpowiedzi asystenta (w interfejsie „Asystent”).
pub const SOURCE: &str = "assistant";
/// Najmniejsza pewność, od której podpowiedź jest pokazywana.
pub const MIN_CONFIDENCE: f64 = 0.6;
/// Ile otwartych zadań trafia do modelu (najnowsze).
pub const OPEN_MAX: usize = 80;
/// Ile zadań TS trafia do modelu.
pub const TS_MAX: usize = 300;
/// Najdłuższy fragment promptu sesji i notatki dnia (znaki).
pub const TEXT_MAX: usize = 500;
/// Ile sesji pamięta [`Memory`], zanim zapomni najstarsze.
const REMEMBERED: usize = 200;

const BAD_ANSWER: &str = "Asystent nie znalazł pewnego dopasowania.";
const OFF_MESSAGE: &str = "Podpowiedzi przypisań są wyłączone w Ustawieniach → Asystent AI.";

// ---------------------------------------------------------------------------
// Dane wejściowe i schematy

/// Otwarte zadanie na liście dla modelu.
#[derive(Clone, Debug, PartialEq)]
pub struct OpenTask {
    pub id: i64,
    pub title: String,
    pub project: String,
    /// Klucz Jiry albo `#numer` Redmine, pusty bez zgłoszenia.
    pub key: String,
}

fn put(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        map.insert(key.into(), json!(value.trim()));
    }
}

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

/// Klucz zgłoszenia zadania: Jira (`ABC-12`) albo Redmine (`#1234`).
fn key_of(task: &db::Task) -> String {
    task.jira_key
        .clone()
        .or_else(|| task.redmine_id.map(|id| format!("#{id}")))
        .unwrap_or_default()
}

/// Otwarte (nieukończone, niezarchiwizowane) zadania, najnowsze pierwsze, najwyżej
/// [`OPEN_MAX`]; kosz nie wchodzi do migawki.
pub fn open_tasks(tasks: &[db::Task]) -> Vec<OpenTask> {
    tasks
        .iter()
        .filter(|t| !t.done && !t.archived)
        .take(OPEN_MAX)
        .map(|t| OpenTask {
            id: t.id,
            title: t.title.clone(),
            project: t.details.project.clone(),
            key: key_of(t),
        })
        .collect()
}

/// Schemat odpowiedzi „sesja agenta → zadanie”.
pub fn assign_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "taskId": { "type": ["integer", "null"] },
            "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
            "reason": { "type": "string" }
        },
        "required": ["taskId", "confidence", "reason"],
        "additionalProperties": false
    })
}

/// Schemat odpowiedzi „zadanie → zadanie TS”.
pub fn ts_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "tsTaskId": { "type": ["string", "null"] },
            "confidence": { "type": "number", "minimum": 0, "maximum": 1 },
            "reason": { "type": "string" }
        },
        "required": ["tsTaskId", "confidence", "reason"],
        "additionalProperties": false
    })
}

/// Treść zapytania „sesja agenta → zadanie”. Folder to sama nazwa katalogu
/// (bez ścieżki, która mogłaby zawierać nazwę użytkownika systemu); gałąź tylko
/// przy zwykłym HEAD. Filtr osobowy dokłada `ai::run`.
pub fn assign_prompt(
    folder: &str,
    branch: Option<&str>,
    first_prompt: Option<&str>,
    tasks: &[OpenTask],
) -> String {
    let mut session = Map::new();
    put(&mut session, "folder", folder);
    put(&mut session, "branch", branch.unwrap_or(""));
    put(
        &mut session,
        "firstPrompt",
        &clip(first_prompt.unwrap_or(""), TEXT_MAX),
    );
    let list: Vec<Value> = tasks
        .iter()
        .map(|t| {
            let mut m = Map::new();
            m.insert("id".into(), json!(t.id));
            put(&mut m, "title", &t.title);
            put(&mut m, "project", &t.project);
            put(&mut m, "key", &t.key);
            Value::Object(m)
        })
        .collect();
    format!(
        "Aplikacja do śledzenia czasu pracy pyta, do którego z otwartych zadań użytkownika \
         należy sesja agenta AI (Claude Code albo Codex). Dopasuj po nazwie folderu, gałęzi \
         i pierwszym poleceniu sesji. Odpowiedz wyłącznie obiektem JSON \
         {{\"taskId\": <id zadania z listy albo null>, \"confidence\": <liczba od 0 do 1>, \
         \"reason\": \"<jedno krótkie zdanie po polsku>\"}}. Gdy żadne zadanie nie pasuje \
         wyraźnie, podaj taskId null i niską pewność.\n\nDane:\n{}",
        json!({ "session": Value::Object(session), "openTasks": list })
    )
}

/// Zadanie TS z listy pobranej z TS (płaska postać z nazwą projektu TS).
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TsOption {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project: String,
}

/// Dane zadania lokalnego dla pytania o zadanie TS.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TsQuery {
    pub title: String,
    pub project: String,
    pub key: String,
    /// Notatka zadania z wybranego dnia.
    pub note: String,
}

/// Treść zapytania „zadanie → zadanie TS”.
pub fn ts_prompt(query: &TsQuery, options: &[TsOption]) -> String {
    let mut task = Map::new();
    put(&mut task, "title", &query.title);
    put(&mut task, "project", &query.project);
    put(&mut task, "key", &query.key);
    put(&mut task, "dayNote", &clip(&query.note, TEXT_MAX));
    let list: Vec<Value> = options
        .iter()
        .take(TS_MAX)
        .map(|o| {
            let mut m = Map::new();
            m.insert("id".into(), json!(o.id));
            put(&mut m, "name", &o.name);
            put(&mut m, "tsProject", &o.project);
            Value::Object(m)
        })
        .collect();
    format!(
        "Aplikacja do śledzenia czasu pracy pyta, które zadanie z systemu Timesheet (TS) \
         pasuje do zadania użytkownika. Odpowiedz wyłącznie obiektem JSON \
         {{\"tsTaskId\": <id zadania TS z listy albo null>, \"confidence\": <liczba od 0 do 1>, \
         \"reason\": \"<jedno krótkie zdanie po polsku>\"}}. Gdy żadne zadanie nie pasuje \
         wyraźnie, podaj tsTaskId null i niską pewność.\n\nDane:\n{}",
        json!({ "task": Value::Object(task), "tsTasks": list })
    )
}

// ---------------------------------------------------------------------------
// Odpowiedzi modelu

/// Podpowiedź przypisania sesji (pewność i uzasadnienie służą testom i dzienniku).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub struct Proposal {
    pub task_id: i64,
    pub confidence: f64,
    pub reason: String,
}

fn confident(value: &Value) -> Option<f64> {
    let confidence = value.get("confidence")?.as_f64()?;
    (MIN_CONFIDENCE..=1.0)
        .contains(&confidence)
        .then_some(confidence)
}

/// Odpowiedź „sesja → zadanie” na podpowiedź: `taskId` musi być z listy, a
/// pewność co najmniej [`MIN_CONFIDENCE`]; `null` i resztę odrzuca.
pub fn parse_assign(value: &Value, open: &[OpenTask]) -> Option<Proposal> {
    let task_id = value.get("taskId")?.as_i64()?;
    let confidence = confident(value)?;
    open.iter().any(|t| t.id == task_id).then(|| Proposal {
        task_id,
        confidence,
        reason: clip(
            value.get("reason").and_then(Value::as_str).unwrap_or(""),
            300,
        ),
    })
}

/// Wynik pytania o zadanie TS dla interfejsu.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TsPick {
    /// Zadanie TS z listy; `None`, gdy model nie jest pewny albo odpowiedział spoza listy.
    pub ts_task_id: Option<String>,
    pub confidence: f64,
    /// Uzasadnienie modelu, a przy braku propozycji powód.
    pub reason: String,
}

/// Odpowiedź „zadanie → zadanie TS”: id musi być z listy, pewność co najmniej
/// [`MIN_CONFIDENCE`]; inaczej brak propozycji z powodem.
pub fn parse_ts(value: &Value, options: &[TsOption]) -> TsPick {
    let reason = clip(
        value.get("reason").and_then(Value::as_str).unwrap_or(""),
        300,
    );
    let confidence = value
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let none = |why: &str| TsPick {
        ts_task_id: None,
        confidence,
        reason: if reason.is_empty() {
            why.to_string()
        } else {
            format!("{why} {reason}")
        },
    };
    let Some(id) = value.get("tsTaskId").and_then(Value::as_str) else {
        return none(BAD_ANSWER);
    };
    if confident(value).is_none() {
        return none(BAD_ANSWER);
    }
    if !options.iter().any(|o| o.id == id) {
        return none("Asystent wskazał zadanie spoza listy TS.");
    }
    TsPick {
        ts_task_id: Some(id.to_string()),
        confidence,
        reason,
    }
}

// ---------------------------------------------------------------------------
// Reguły: kiedy pytać i pamięć po `session_id`

/// Czy ustawienia pozwalają zapamiętywać pierwszy prompt sesji (B8): przełącznik
/// `ai_assign_suggest` i wybrany silnik.
pub fn keep_prompts(s: &AppSettings) -> bool {
    s.ai_assign_suggest && ai::Engine::parse(&s.ai_engine).is_some()
}

/// Czy sesja agenta kwalifikuje się do pytania modelu: heurystyka niczego nie
/// dopasowała, sesja pracuje co najmniej [`suggest::MIN_WORK`], folder nie jest
/// wyciszony, propozycje agentów i podpowiedzi przypisań są włączone.
pub fn wants(agent: &Agent, s: &AppSettings, now: i64) -> bool {
    s.ai_assign_suggest
        && s.agent_suggest
        && agent.task.is_none()
        && matches!(agent.kind.as_str(), "claude" | "codex")
        && !agent.session_id.is_empty()
        && now - agent.since >= suggest::MIN_WORK
        && !suggest::muted(&s.agent_muted_folders, &agent.cwd)
}

type SessionKey = (String, String);

/// Stan pytania o jedną sesję.
#[derive(Clone, Debug, PartialEq)]
enum Entry {
    /// Wywołanie trwa.
    Pending,
    /// Gotowe; `None` = brak pewnej podpowiedzi albo błąd (bez ponawiania).
    Done(Option<Proposal>),
}

/// Co zrobić z sesją w tej chwili.
#[derive(Clone, Debug, PartialEq)]
pub enum Slot {
    /// Pierwsze spotkanie z sesją: zapytać model (wpis oznaczony jako trwający).
    Ask,
    /// Wywołanie trwa.
    Waiting,
    /// Odpowiedź zapamiętana: podpowiedź albo jej brak.
    Answer(Option<Proposal>),
}

/// Pamięć wywołań po `session_id` (rodzaj agenta + identyfikator sesji): jedno
/// wywołanie na sesję na czas działania aplikacji.
#[derive(Debug, Default)]
pub struct Memory {
    map: HashMap<SessionKey, Entry>,
    order: VecDeque<SessionKey>,
}

impl Memory {
    pub fn key(agent: &Agent) -> SessionKey {
        (agent.kind.clone(), agent.session_id.clone())
    }

    /// Stan sesji; nieznana sesja przechodzi w `Pending` i zwraca [`Slot::Ask`].
    pub fn begin(&mut self, key: &SessionKey) -> Slot {
        match self.map.get(key) {
            Some(Entry::Pending) => Slot::Waiting,
            Some(Entry::Done(answer)) => Slot::Answer(answer.clone()),
            None => {
                while self.order.len() >= REMEMBERED {
                    if let Some(old) = self.order.pop_front() {
                        self.map.remove(&old);
                    }
                }
                self.map.insert(key.clone(), Entry::Pending);
                self.order.push_back(key.clone());
                Slot::Ask
            }
        }
    }

    /// Zapisuje wynik wywołania (także brak podpowiedzi i błąd).
    pub fn finish(&mut self, key: &SessionKey, answer: Option<Proposal>) {
        if let Some(entry) = self.map.get_mut(key) {
            *entry = Entry::Done(answer);
        }
    }
}

/// Wywołanie modelu o jedną sesję: `ask(rodzaj, prompt, schemat)`. Brak otwartych
/// zadań to brak pytania; błąd wywołania to brak podpowiedzi (dziennik zna powód).
pub fn decide(
    ask: &dyn Fn(&str, &str, &Value) -> Result<Value, String>,
    folder: &str,
    branch: Option<&str>,
    first_prompt: Option<&str>,
    tasks: &[OpenTask],
) -> Option<Proposal> {
    if tasks.is_empty() {
        return None;
    }
    let prompt = assign_prompt(folder, branch, first_prompt, tasks);
    let answer = ask(KIND_ASSIGN, &prompt, &assign_schema()).ok()?;
    parse_assign(&answer, tasks)
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

/// Pamięć sesji i kolejka wywołań (jedno naraz, żeby „busy” nie gubiło sesji).
#[derive(Default)]
pub struct State {
    memory: Mutex<Memory>,
    worker: Mutex<()>,
}

/// Rejestruje stan modułu; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Podpowiedź jako dopasowanie dla banera: tytuł, klucz i stan ukończenia
/// z bazy; `None`, gdy zadanie zniknęło (kosz, usunięte).
pub fn assist_match(db: &Connection, task_id: i64) -> rusqlite::Result<Option<Match>> {
    db.query_row(
        "SELECT t.title,COALESCE(j.issue_key,'#' || r.issue_id),t.done FROM tasks t
         LEFT JOIN task_lifecycle l ON l.task_id=t.id LEFT JOIN jira_tasks j ON j.task_id=t.id
         LEFT JOIN redmine_tasks r ON r.task_id=t.id
         WHERE t.id=?1 AND l.deleted_at IS NULL",
        [task_id],
        |r| {
            Ok(Match {
                task_id: Some(task_id),
                title: r.get(0)?,
                key: r.get(1)?,
                source: SOURCE.into(),
                done: r.get(2)?,
            })
        },
    )
    .optional()
}

/// Dane sesji potrzebne do wywołania (zebrane przed uruchomieniem wątku).
struct Job {
    key: SessionKey,
    folder: String,
    branch: Option<String>,
    prompt: Option<String>,
}

/// Uzupełnia `assist` agentów bez dopasowania: z pamięci, a pierwszą sesję oddaje
/// do wywołania w tle (po odpowiedzi `agents::rematch` przelicza propozycje).
/// Wołać po `resolve`; nigdy nie dotyka `task`.
pub fn apply(app: &tauri::AppHandle, agents: &mut [Agent]) {
    for agent in agents.iter_mut() {
        agent.assist = None;
    }
    let settings = crate::settings::current(app);
    if !settings.ai_assign_suggest {
        return;
    }
    // Wykrycie CLI robi `ai_status`; tu bez uruchamiania procesów w pętli agentów.
    let ready = app
        .try_state::<ai::State>()
        .and_then(|s| s.detected())
        .is_some_and(|d| ai::active_engine(&settings.ai_engine, &d).is_some());
    if !ready {
        return;
    }
    let Some(state) = app.try_state::<State>() else {
        return;
    };
    let now = db::now();
    let mut have: Vec<(usize, Proposal)> = Vec::new();
    let mut jobs: Vec<Job> = Vec::new();
    if let Ok(mut memory) = state.memory.lock() {
        for (i, agent) in agents.iter().enumerate() {
            if !wants(agent, &settings, now) {
                continue;
            }
            let key = Memory::key(agent);
            match memory.begin(&key) {
                Slot::Answer(Some(p)) => have.push((i, p)),
                Slot::Answer(None) | Slot::Waiting => {}
                Slot::Ask => jobs.push(Job {
                    key,
                    folder: agents::folder(&agent.cwd),
                    branch: agent
                        .branch
                        .clone()
                        .filter(|_| !agent.detached)
                        .filter(|b| !b.is_empty()),
                    prompt: agent.first_prompt.clone(),
                }),
            }
        }
    }
    if !have.is_empty() {
        let database = app.state::<crate::Database>();
        let conn = database.0.lock();
        if let Ok(conn) = conn {
            for (i, p) in have {
                agents[i].assist = assist_match(&conn, p.task_id).ok().flatten();
            }
        }
    }
    for job in jobs {
        spawn(app, job);
    }
}

/// Wywołanie modelu w osobnym wątku: lista otwartych zadań z bazy, jedno
/// wywołanie naraz, wynik do pamięci i ponowne dopasowanie agentów.
fn spawn(app: &tauri::AppHandle, job: Job) {
    let app = app.clone();
    std::thread::spawn(move || {
        let tasks = {
            let database = app.state::<crate::Database>();
            database
                .0
                .lock()
                .ok()
                .and_then(|conn| db::snapshot(&conn).ok())
                .map(|snapshot| open_tasks(&snapshot.tasks))
                .unwrap_or_default()
        };
        let Some(state) = app.try_state::<State>() else {
            return;
        };
        let answer = {
            // Wywołania jednego rodzaju nie nakładają się (`ai::acquire`), więc
            // sesje czekają w kolejce zamiast dostać błąd zajętości.
            let _turn = state.worker.lock().ok();
            decide(
                &|kind, prompt, schema| ai::ask(&app, kind, Tier::Fast, prompt, schema),
                &job.folder,
                job.branch.as_deref(),
                job.prompt.as_deref(),
                &tasks,
            )
        };
        if let Ok(mut memory) = state.memory.lock() {
            memory.finish(&job.key, answer);
        }
        agents::rematch(&app);
    });
}

// ---------------------------------------------------------------------------
// Zadanie TS dla zadania

/// Pytanie o zadanie TS dla zadania lokalnego; `options` to lista zadań TS
/// z formularza dnia, którą interfejs już ma. Zwraca sugestię (zapis dopiero po
/// wyborze użytkownika); wywołanie tylko po kliknięciu „Podpowiedz”.
#[tauri::command]
pub async fn ai_suggest_ts_task(
    app: tauri::AppHandle,
    task_id: i64,
    day: String,
    options: Vec<TsOption>,
) -> Result<TsPick, String> {
    tauri::async_runtime::spawn_blocking(move || {
        timesheet::check_date(&day)?;
        if !crate::settings::current(&app).ai_assign_suggest {
            return Err(OFF_MESSAGE.to_string());
        }
        if options.is_empty() {
            return Err("Brak listy zadań TS: połącz konto TS i odśwież listę.".to_string());
        }
        let query = {
            let database = app.state::<crate::Database>();
            let conn = database.0.lock().map_err(|e| e.to_string())?;
            let snapshot = db::snapshot(&conn).map_err(|e| e.to_string())?;
            ts_query(&snapshot.tasks, task_id, &day).ok_or("Nie znaleziono zadania.")?
        };
        ts_pick(
            &|kind, prompt, schema| ai::ask(&app, kind, Tier::Fast, prompt, schema),
            &query,
            &options,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Dane zadania do pytania o zadanie TS.
pub fn ts_query(tasks: &[db::Task], task_id: i64, day: &str) -> Option<TsQuery> {
    let task = tasks.iter().find(|t| t.id == task_id)?;
    Some(TsQuery {
        title: task.title.clone(),
        project: task.details.project.clone(),
        key: key_of(task),
        note: task.details.notes.get(day).cloned().unwrap_or_default(),
    })
}

/// Pytanie o zadanie TS na dowolnym `ask` (testowalny rdzeń komendy).
pub fn ts_pick(
    ask: &dyn Fn(&str, &str, &Value) -> Result<Value, String>,
    query: &TsQuery,
    options: &[TsOption],
) -> Result<TsPick, String> {
    let options = &options[..options.len().min(TS_MAX)];
    let answer = ask(KIND_TS_TASK, &ts_prompt(query, options), &ts_schema())?;
    Ok(parse_ts(&answer, options))
}

#[cfg(test)]
mod tests;
