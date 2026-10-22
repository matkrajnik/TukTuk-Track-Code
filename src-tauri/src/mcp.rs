//! Serwer MCP w głównym procesie (PLAN_BACKLOG.md, A2).
//!
//! `tuktuk-track-mcp.exe` (agent ↔ stdio) przekazuje wywołania narzędzi nazwanym
//! potokiem `\\.\pipe\tuktuk-track-mcp-<użytkownik>`; tu je wykonujemy:
//! - **narzędzia odczytu** (`tasks_list`, `task_get`, `day_summary`, `week_summary`,
//!   `current_measurement`, `ts_tasks`) czytają bazę i odpowiadają od razu,
//! - **narzędzia `propose_*`** niczego nie zmieniają: dopisują wpis do kolejki
//!   `ai_proposals` (tylko w pamięci), a aplikacja pokazuje go jako baner
//!   „Zastosuj” / „Odrzuć”; „Zastosuj” woła istniejące komendy.
//!
//! **Wynik każdego narzędzia (także błąd) przechodzi przez `ai_privacy::redact`**
//! przed odesłaniem: to, co MCP zwraca sesji agenta, trafia do modelu. Tytuły
//! spotkań są w wyniku tylko przy `ai_send_meeting_titles`; tytuły okien nie są
//! zwracane żadnym narzędziem. Nie ma narzędzi do wysyłki TS, zapisów w Jirze i
//! Redmine, usuwania ani zmiany ustawień. Serwer odpowiada tylko przy włączonym
//! `ai_mcp` i wybranym silniku (`ai_engine`).
//!
//! Rejestracja w Claude Code i Codexie: `mcp_register.rs`.

use crate::ai::{self, Engine};
use crate::ai_privacy::{self, Dictionary, Pseudonyms};
use crate::db::{self, Task};
use crate::mcp_register::{self, Entry};
use crate::mcp_wire;
use crate::settings::AppSettings;
use crate::timesheet::Project;
use crate::{agent_time, calendar, capacity, planner, stats, timesheet, week_notes};
use chrono::{Datelike, NaiveDate, TimeZone};
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Zdarzenie okien po zmianie kolejki propozycji.
pub const EVENT: &str = "ai-proposals-changed";
/// Najwięcej propozycji czekających na decyzję; najstarsze odpadają.
const QUEUE_MAX: usize = 50;
/// Najwięcej zadań w jednej odpowiedzi `tasks_list`.
const LIST_MAX: usize = 50;
/// Najwięcej pomiarów w `day_summary`.
const SESSIONS_MAX: usize = 100;
/// Tyle ostatnich notatek dnia zwraca `task_get`.
const NOTES_MAX: usize = 14;

pub const GATE_OFF: &str = "The MCP server is turned off in TukTuk Track settings.";
pub const GATE_NO_ENGINE: &str =
    "The assistant is not enabled in TukTuk Track. Choose an engine in Settings, AI assistant.";
pub const QUEUED: &str = "Propozycja czeka na akceptację użytkownika w TukTuk Track.";

/// Czy serwer ma odpowiadać: włączony `ai_mcp` i wybrany silnik.
pub fn gate(settings: &AppSettings) -> Result<(), &'static str> {
    if !settings.ai_mcp {
        Err(GATE_OFF)
    } else if Engine::parse(&settings.ai_engine).is_none() {
        Err(GATE_NO_ENGINE)
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Kolejka propozycji

/// Propozycja zmiany czekająca na „Zastosuj” albo „Odrzuć”.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: u64,
    /// `task_create`, `note_append`, `link_folder`, `switch`, `plan`.
    pub kind: String,
    /// Chwila powstania (ms).
    pub at: i64,
    pub task_id: Option<i64>,
    /// Tytuł zadania z chwili propozycji (do pokazania w banerze).
    pub task_title: String,
    pub title: String,
    pub project: String,
    pub key: String,
    pub day: String,
    pub estimate: u32,
    pub text: String,
    pub folder: String,
}

#[derive(Default)]
struct Queue {
    next: u64,
    items: Vec<Proposal>,
}

impl Queue {
    /// Dodaje propozycję; identyczna czekająca już propozycja nie dubluje się.
    fn push(&mut self, mut proposal: Proposal) {
        let same = |p: &Proposal| {
            Proposal {
                id: 0,
                at: 0,
                ..p.clone()
            } == Proposal {
                id: 0,
                at: 0,
                ..proposal.clone()
            }
        };
        if self.items.iter().any(same) {
            return;
        }
        self.next += 1;
        proposal.id = self.next;
        self.items.push(proposal);
        if self.items.len() > QUEUE_MAX {
            self.items.remove(0);
        }
    }
}

/// Stan modułu (jeden na aplikację).
pub struct State {
    queue: Mutex<Queue>,
    /// Pseudonimy w obrębie działania aplikacji: ta sama osoba ma ten sam
    /// pseudonim we wszystkich odpowiedziach; mapa nigdy nie trafia do modelu.
    /// Wspólna z czatem w aplikacji (`ai_chat`), więc „Osoba N” z wiadomości
    /// użytkownika i z wyniku narzędzia to ta sama przestrzeń oznaczeń.
    pseudonyms: Arc<Mutex<Pseudonyms>>,
    /// Lista zadań TS zapamiętana przy ostatnim udanym pobraniu (nigdy na dysku).
    ts_cache: Mutex<Option<Vec<Project>>>,
    status: Mutex<Status>,
    /// Jedna rejestracja naraz (start, zmiana ustawień i kopia piszą te same pliki).
    io: Mutex<()>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            queue: Mutex::new(Queue::default()),
            pseudonyms: Arc::new(Mutex::new(Pseudonyms::new())),
            ts_cache: Mutex::new(None),
            status: Mutex::new(Status::default()),
            io: Mutex::new(()),
        }
    }
}

impl State {
    pub fn proposals(&self) -> Vec<Proposal> {
        self.queue
            .lock()
            .map(|q| q.items.clone())
            .unwrap_or_default()
    }
    /// Usuwa propozycję (po „Zastosuj” i „Odrzuć”); zwraca pozostałe.
    pub fn resolve(&self, id: u64) -> Vec<Proposal> {
        match self.queue.lock() {
            Ok(mut q) => {
                q.items.retain(|p| p.id != id);
                q.items.clone()
            }
            Err(_) => Vec::new(),
        }
    }
    /// Mapa pseudonimów do współdzielenia z czatem (`ai_chat::setup`).
    pub fn pseudonyms(&self) -> Arc<Mutex<Pseudonyms>> {
        Arc::clone(&self.pseudonyms)
    }
    pub fn remember_ts(&self, projects: &[Project]) {
        if let Ok(mut cache) = self.ts_cache.lock() {
            *cache = Some(projects.to_vec());
        }
    }
    fn ts(&self) -> Option<Vec<Project>> {
        self.ts_cache.lock().ok().and_then(|c| c.clone())
    }
}

// ---------------------------------------------------------------------------
// Wykonanie narzędzi

/// Wszystko, czego narzędzia potrzebują poza argumentami.
pub struct Env<'a, Tz: TimeZone> {
    pub db: &'a Connection,
    pub tz: &'a Tz,
    pub settings: &'a AppSettings,
    pub now: i64,
    /// Foldery robocze pracujących sesji agentów, od najnowszej (`propose_link_folder`).
    pub folders: &'a [String],
}

fn text_arg(args: &Value, key: &str) -> String {
    args[key]
        .as_str()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn clip(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(max)
        .collect()
}

fn put(map: &mut Map<String, Value>, key: &str, value: Value) {
    let empty = match &value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Number(n) => n.as_i64() == Some(0),
        Value::Bool(b) => !b,
        _ => false,
    };
    if !empty {
        map.insert(key.to_string(), value);
    }
}

/// Klucz zgłoszenia: Jira (`ABC-12`) albo Redmine (`#1234`).
fn key_of(task: &Task) -> String {
    task.jira_key
        .clone()
        .or_else(|| task.redmine_id.map(|id| format!("#{id}")))
        .unwrap_or_default()
}

fn minutes(ms: i64) -> i64 {
    (ms.max(0) + 30_000) / 60_000
}

fn local_hm<Tz: TimeZone>(tz: &Tz, ms: i64) -> String {
    tz.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local().format("%H:%M").to_string())
        .unwrap_or_default()
}

fn today<Tz: TimeZone>(env: &Env<Tz>) -> NaiveDate {
    env.tz
        .timestamp_millis_opt(env.now)
        .single()
        .map(|t| t.date_naive())
        .unwrap_or_default()
}

fn day_arg<Tz: TimeZone>(env: &Env<Tz>, args: &Value, key: &str) -> Result<NaiveDate, String> {
    let text = text_arg(args, key);
    if text.is_empty() {
        return Ok(today(env));
    }
    if !crate::settings::valid_day(&text) {
        return Err(format!("Invalid {key}: use YYYY-MM-DD."));
    }
    NaiveDate::parse_from_str(&text, "%Y-%m-%d").map_err(|e| e.to_string())
}

/// Zadanie z argumentu: numer, klucz Jiry, `#numer` Redmine albo dokładny tytuł.
fn resolve<'a>(tasks: &'a [Task], arg: &Value, ps: &Pseudonyms) -> Result<&'a Task, String> {
    let raw = match arg {
        Value::Number(n) => n.as_i64().map(|n| n.to_string()).unwrap_or_default(),
        Value::String(s) => ps.restore(s.trim()),
        _ => String::new(),
    };
    if raw.is_empty() {
        return Err("Provide a task: id, Jira key, Redmine #number or exact title.".into());
    }
    if let Ok(id) = raw.parse::<i64>() {
        if let Some(task) = tasks.iter().find(|t| t.id == id) {
            return Ok(task);
        }
    }
    if let Some(number) = raw.strip_prefix('#').and_then(|n| n.parse::<u64>().ok()) {
        if let Some(task) = tasks.iter().find(|t| t.redmine_id == Some(number)) {
            return Ok(task);
        }
    }
    if let Some(task) = tasks.iter().find(|t| {
        t.jira_key
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case(&raw))
    }) {
        return Ok(task);
    }
    let lower = raw.to_lowercase();
    let mut by_title = tasks
        .iter()
        .filter(|t| t.title.trim().to_lowercase() == lower);
    match (by_title.next(), by_title.next()) {
        (Some(task), None) => Ok(task),
        (Some(_), Some(_)) => Err(format!(
            "More than one task has the title \"{raw}\". Use the task id."
        )),
        _ => Err(format!("Task not found: \"{raw}\".")),
    }
}

fn task_json(task: &Task) -> Map<String, Value> {
    let d = &task.details;
    let mut m = Map::new();
    m.insert("id".into(), json!(task.id));
    m.insert("title".into(), json!(task.title));
    put(&mut m, "key", json!(key_of(task)));
    put(&mut m, "project", json!(d.project));
    put(&mut m, "done", json!(task.done));
    put(&mut m, "archived", json!(task.archived));
    put(&mut m, "plannedDay", json!(d.planned_day));
    put(&mut m, "dueDay", json!(d.due_day));
    put(&mut m, "priority", json!(d.priority));
    put(&mut m, "waiting", json!(d.waiting));
    put(&mut m, "estimateMinutes", json!(d.estimate_minutes));
    put(
        &mut m,
        "status",
        json!(task
            .jira_status
            .clone()
            .or_else(|| task.redmine_status.clone())
            .unwrap_or_default()),
    );
    m
}

/// Pomiary nakładające się na `[from, to)`, przycięte do zakresu (trwający kończy
/// się w `now`): zadanie, początek, koniec, czy trwa. Bez zadań z kosza.
fn sessions_between(
    db: &Connection,
    from: i64,
    to: i64,
    now: i64,
) -> Result<Vec<(i64, i64, i64, bool)>, String> {
    let mut stmt = db
        .prepare(
            "SELECT s.task_id, s.started_at, s.ended_at FROM sessions s
             WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
               AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)
             ORDER BY s.started_at, s.id",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![from, to, now], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (task, start, end) = row.map_err(|e| e.to_string())?;
        let active = end.is_none();
        let end = end.unwrap_or(now);
        out.push((task, start.max(from), end.min(to), active));
    }
    Ok(out)
}

fn title_of(tasks: &[Task], id: i64) -> String {
    tasks
        .iter()
        .find(|t| t.id == id)
        .map(|t| t.title.clone())
        .unwrap_or_default()
}

fn per_task(tasks: &[Task], sessions: &[(i64, i64, i64, bool)]) -> Vec<Value> {
    let mut totals: BTreeMap<i64, i64> = BTreeMap::new();
    for (task, start, end, _) in sessions {
        *totals.entry(*task).or_default() += (end - start).max(0);
    }
    let mut list: Vec<(i64, i64)> = totals.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list.into_iter()
        .map(|(id, ms)| json!({"taskId": id, "title": title_of(tasks, id), "minutes": minutes(ms)}))
        .collect()
}

fn tasks_list<Tz: TimeZone>(
    env: &Env<Tz>,
    tasks: &[Task],
    args: &Value,
    ps: &Pseudonyms,
) -> Result<Value, String> {
    let filter = match text_arg(args, "filter").to_lowercase().as_str() {
        "" | "open" | "otwarte" => "open",
        "today" | "dzisiaj" => "today",
        "blocked" | "zablokowane" => "blocked",
        "project" | "projekt" => "project",
        "search" | "szukaj" => "search",
        other => return Err(format!("Unknown filter \"{other}\".")),
    };
    // Jak inne argumenty od modelu: „Osoba 1” z wcześniejszej odpowiedzi to ta sama osoba.
    let query = ps.restore(&text_arg(args, "query")).to_lowercase();
    if matches!(filter, "project" | "search") && query.is_empty() {
        return Err(format!("The {filter} filter needs a query."));
    }
    let date = today(env);
    let day = date.format("%Y-%m-%d").to_string();
    let worked: Vec<i64> = if filter == "today" {
        let (from, to) = stats::day_bounds(env.tz, date);
        sessions_between(env.db, from, to, env.now)?
            .into_iter()
            .map(|s| s.0)
            .collect()
    } else {
        Vec::new()
    };
    let matches: Vec<&Task> = tasks
        .iter()
        .filter(|t| {
            let d = &t.details;
            match filter {
                "today" => {
                    !t.archived
                        && (worked.contains(&t.id)
                            || (!t.done && (d.planned_day == day || d.due_day == day)))
                }
                "open" => !t.done && !t.archived,
                "blocked" => !t.done && !t.archived && !d.waiting.trim().is_empty(),
                "project" => d.project.trim().to_lowercase() == query,
                _ => {
                    let notes: String = d.notes.values().cloned().collect::<Vec<_>>().join(" ");
                    [
                        t.title.as_str(),
                        &key_of(t),
                        d.project.as_str(),
                        notes.as_str(),
                    ]
                    .iter()
                    .any(|text| text.to_lowercase().contains(&query))
                }
            }
        })
        .collect();
    Ok(json!({
        "filter": filter,
        "total": matches.len(),
        "truncated": matches.len() > LIST_MAX,
        "tasks": matches.iter().take(LIST_MAX).map(|t| Value::Object(task_json(t))).collect::<Vec<_>>(),
    }))
}

fn task_get<Tz: TimeZone>(
    env: &Env<Tz>,
    tasks: &[Task],
    args: &Value,
    ps: &Pseudonyms,
) -> Result<Value, String> {
    let arg = if !args["id"].is_null() {
        &args["id"]
    } else {
        &args["key"]
    };
    let task = resolve(tasks, arg, ps)?;
    let mut m = task_json(task);
    let d = &task.details;
    put(
        &mut m,
        "url",
        json!(task
            .jira_url
            .clone()
            .or_else(|| task.redmine_url.clone())
            .unwrap_or_default()),
    );
    put(&mut m, "waitingSince", json!(d.waiting_since));
    put(&mut m, "tsTaskId", json!(d.ts_task_id));
    put(&mut m, "tsTaskName", json!(d.ts_task_name));
    let notes: Map<String, Value> = d
        .notes
        .iter()
        .rev()
        .take(NOTES_MAX)
        .map(|(day, text)| (day.clone(), json!(text)))
        .collect();
    put(
        &mut m,
        "notes",
        if notes.is_empty() {
            Value::Null
        } else {
            Value::Object(notes)
        },
    );
    if !d.checklist.is_empty() {
        m.insert(
            "checklist".into(),
            json!(d
                .checklist
                .iter()
                .map(|c| json!({"text": c.text, "done": c.done}))
                .collect::<Vec<_>>()),
        );
    }
    if !task.jira_fields.is_empty() {
        m.insert("jiraFields".into(), json!(task.jira_fields));
    }
    let date = today(env);
    let (from, to) = stats::day_bounds(env.tz, date);
    let all = sessions_between(env.db, 0, i64::MAX, env.now)?;
    let total: i64 = all
        .iter()
        .filter(|s| s.0 == task.id)
        .map(|s| s.2 - s.1)
        .sum();
    let today_ms: i64 = all
        .iter()
        .filter(|s| s.0 == task.id)
        .map(|s| (s.2.min(to) - s.1.max(from)).max(0))
        .sum();
    m.insert("trackedMinutesTotal".into(), json!(minutes(total)));
    m.insert("trackedMinutesToday".into(), json!(minutes(today_ms)));
    Ok(Value::Object(m))
}

fn day_summary<Tz: TimeZone>(env: &Env<Tz>, tasks: &[Task], args: &Value) -> Result<Value, String> {
    let date = day_arg(env, args, "day")?;
    let day = date.format("%Y-%m-%d").to_string();
    let (from, to) = stats::day_bounds(env.tz, date);
    let sessions = sessions_between(env.db, from, to, env.now)?;
    let total = stats::day_total_ms_in(env.db, env.tz, date, env.now).map_err(|e| e.to_string())?;
    let goal = capacity::goal_ms_in(env.db, env.tz, env.settings, date, env.now)
        .map_err(|e| e.to_string())?;
    let mut result = Map::new();
    result.insert("day".into(), json!(day));
    result.insert("trackedMinutes".into(), json!(minutes(total)));
    result.insert("goalMinutes".into(), json!(minutes(goal)));
    result.insert("perTask".into(), json!(per_task(tasks, &sessions)));
    result.insert(
        "measurements".into(),
        json!(sessions
            .iter()
            .take(SESSIONS_MAX)
            .map(|(task, start, end, active)| {
                let mut m = Map::new();
                m.insert("taskId".into(), json!(task));
                m.insert("start".into(), json!(local_hm(env.tz, *start)));
                m.insert("end".into(), json!(local_hm(env.tz, *end)));
                put(&mut m, "running", json!(active));
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    let meetings = calendar::meetings_between(env.db, from, to)?;
    result.insert(
        "meetings".into(),
        json!(meetings
            .iter()
            .filter(|e| !e.cancelled)
            .map(|e| {
                let mut m = Map::new();
                m.insert("start".into(), json!(local_hm(env.tz, e.start)));
                m.insert("end".into(), json!(local_hm(env.tz, e.end)));
                m.insert(
                    "minutes".into(),
                    json!(minutes((e.end.min(to) - e.start.max(from)).max(0))),
                );
                put(&mut m, "allDay", json!(e.all_day));
                m.insert("showAs".into(), json!(e.show_as));
                // Tytuł spotkania wychodzi do modelu tylko przy włączonym przełączniku.
                if env.settings.ai_send_meeting_titles {
                    put(&mut m, "title", json!(e.subject));
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    let agents = agent_time::query(env.db, from, to, env.now).map_err(|e| e.to_string())?;
    result.insert(
        "agentTime".into(),
        json!({
            "perTask": agents.totals.iter().map(|t| json!({
                "taskId": t.task_id,
                "title": title_of(tasks, t.task_id),
                "agentMinutes": minutes(t.agent_ms),
                "parallelMinutes": minutes(t.parallel_ms),
            })).collect::<Vec<_>>(),
            "unmatchedMinutes": minutes(agents.unmatched_ms),
        }),
    );
    let notes: Vec<Value> = tasks
        .iter()
        .filter_map(|t| {
            let note = t.details.notes.get(&day)?;
            (!note.trim().is_empty())
                .then(|| json!({"taskId": t.id, "title": t.title, "note": note}))
        })
        .collect();
    result.insert("notes".into(), json!(notes));
    if let Some(mark) = crate::days::get(env.db, &day).map_err(|e| e.to_string())? {
        let mut m = Map::new();
        if let Some(at) = mark.started_at {
            m.insert("startedAt".into(), json!(local_hm(env.tz, at)));
        }
        if let Some(at) = mark.ended_at {
            m.insert("endedAt".into(), json!(local_hm(env.tz, at)));
        }
        result.insert("dayMark".into(), Value::Object(m));
    }
    Ok(Value::Object(result))
}

fn week_summary<Tz: TimeZone>(
    env: &Env<Tz>,
    tasks: &[Task],
    args: &Value,
) -> Result<Value, String> {
    let date = day_arg(env, args, "week")?;
    let monday = date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()));
    let sunday = monday + chrono::Duration::days(6);
    let (from, _) = stats::day_bounds(env.tz, monday);
    let (_, to) = stats::day_bounds(env.tz, sunday);
    let mut days = Vec::new();
    for offset in 0..7 {
        let d = monday + chrono::Duration::days(offset);
        let total =
            stats::day_total_ms_in(env.db, env.tz, d, env.now).map_err(|e| e.to_string())?;
        let goal = capacity::goal_ms_in(env.db, env.tz, env.settings, d, env.now)
            .map_err(|e| e.to_string())?;
        days.push(json!({
            "day": d.format("%Y-%m-%d").to_string(),
            "trackedMinutes": minutes(total),
            "goalMinutes": minutes(goal),
        }));
    }
    let sessions = sessions_between(env.db, from, to, env.now)?;
    let week = monday.format("%Y-%m-%d").to_string();
    let note = week_notes::load(env.db, &week)?;
    let mut result = Map::new();
    result.insert("week".into(), json!(week));
    result.insert(
        "trackedMinutes".into(),
        json!(days
            .iter()
            .map(|d| d["trackedMinutes"].as_i64().unwrap_or(0))
            .sum::<i64>()),
    );
    result.insert("days".into(), json!(days));
    result.insert("perTask".into(), json!(per_task(tasks, &sessions)));
    put(&mut result, "note", json!(note));
    Ok(Value::Object(result))
}

fn current_measurement<Tz: TimeZone>(env: &Env<Tz>, tasks: &[Task]) -> Result<Value, String> {
    let active: Option<(i64, i64)> = {
        use rusqlite::OptionalExtension;
        env.db
            .query_row(
                "SELECT task_id, started_at FROM sessions WHERE ended_at IS NULL ORDER BY id DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?
    };
    let Some((id, started)) = active else {
        return Ok(json!({"running": false}));
    };
    let mut m = tasks
        .iter()
        .find(|t| t.id == id)
        .map(task_json)
        .unwrap_or_default();
    m.insert("running".into(), json!(true));
    m.insert("since".into(), json!(local_hm(env.tz, started)));
    m.insert("elapsedMinutes".into(), json!(minutes(env.now - started)));
    Ok(Value::Object(m))
}

fn ts_tasks(env_db: &Connection, cache: Option<Vec<Project>>) -> Result<Value, String> {
    if let Some(projects) = cache {
        return Ok(json!({
            "cached": true,
            "projects": projects.iter().map(|p| json!({
                "name": p.name,
                "tasks": p.tasks.iter().map(|t| json!({"id": t.id, "name": t.name})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }));
    }
    let mapped = timesheet::load_project_map(env_db).map_err(|e| e.to_string())?;
    Ok(json!({
        "cached": false,
        "note": "No Timesheet task list is in memory yet; it is loaded when the Timesheet send dialog is opened in TukTuk Track. Mapped projects are listed instead.",
        "mappedProjects": mapped.iter().map(|m| json!({
            "project": m.project,
            "tsTaskId": m.ts_task_id,
            "tsTaskName": m.ts_task_name,
        })).collect::<Vec<_>>(),
    }))
}

/// Klucz w `propose_task_create`: Jira `ABC-12` (wielkie litery) albo Redmine
/// `#12`, pusty bez klucza. Ta sama reguła co `newTaskOf` w `aiproposals.ts`,
/// więc baner nie obiecuje powiązania, którego „Zastosuj” nie utworzy.
fn proposal_key(key: &str) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() {
        return Ok(String::new());
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    if let Some(number) = key.strip_prefix('#') {
        if digits(number) {
            return Ok(key.to_string());
        }
    } else if let Some((project, number)) = key.split_once('-') {
        let mut chars = project.chars();
        let first_letter = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
        if first_letter
            && project.len() >= 2
            && project.bytes().all(|c| c.is_ascii_alphanumeric())
            && digits(number)
        {
            return Ok(key.to_ascii_uppercase());
        }
    }
    Err("Invalid key: use a Jira key such as ABC-12 or a Redmine number such as #12.".into())
}

fn blank() -> Proposal {
    Proposal::default()
}

fn propose<Tz: TimeZone>(
    env: &Env<Tz>,
    state: &State,
    tasks: &[Task],
    tool: &str,
    args: &Value,
    ps: &Pseudonyms,
) -> Result<Value, String> {
    let restored = |key: &str, max: usize| clip(&ps.restore(&text_arg(args, key)), max);
    let mut proposal = Proposal {
        at: env.now,
        ..blank()
    };
    let open_task = |tasks: &'_ [Task]| -> Result<(i64, String), String> {
        let task = resolve(tasks, &args["task"], ps)?;
        if task.done || task.archived {
            return Err(format!("Task {} is done or archived.", task.id));
        }
        Ok((task.id, task.title.clone()))
    };
    match tool {
        "propose_task_create" => {
            proposal.kind = "task_create".into();
            proposal.title = restored("title", 200);
            if proposal.title.trim().is_empty() {
                return Err("The title is required.".into());
            }
            proposal.project = restored("project", 100);
            proposal.key = proposal_key(&restored("key", 40))?;
            if !text_arg(args, "day").is_empty() {
                proposal.day = day_arg(env, args, "day")?.format("%Y-%m-%d").to_string();
            }
            if let Some(estimate) = args["estimate"].as_i64() {
                if !(1..=1440).contains(&estimate) {
                    return Err("The estimate is in minutes, 1 to 1440.".into());
                }
                proposal.estimate = estimate as u32;
            }
        }
        "propose_note_append" => {
            proposal.kind = "note_append".into();
            let task = resolve(tasks, &args["task"], ps)?;
            proposal.task_id = Some(task.id);
            proposal.task_title = task.title.clone();
            proposal.text = restored("text", 1000);
            if proposal.text.trim().is_empty() {
                return Err("The text is required.".into());
            }
            proposal.day = day_arg(env, args, "day")?.format("%Y-%m-%d").to_string();
        }
        "propose_link_folder" => {
            proposal.kind = "link_folder".into();
            let (id, title) = open_task(tasks)?;
            proposal.task_id = Some(id);
            proposal.task_title = title;
            proposal.folder = env
                .folders
                .first()
                .cloned()
                .ok_or("No agent session with a known working folder is active.")?;
        }
        "propose_switch" => {
            proposal.kind = "switch".into();
            let (id, title) = open_task(tasks)?;
            proposal.task_id = Some(id);
            proposal.task_title = title;
        }
        "propose_plan" => {
            proposal.kind = "plan".into();
            let (id, title) = open_task(tasks)?;
            proposal.task_id = Some(id);
            proposal.task_title = title;
            let day = text_arg(args, "day");
            if !planner::date_valid(&day) {
                return Err("Invalid day: use YYYY-MM-DD.".into());
            }
            proposal.day = day;
        }
        _ => return Err(format!("Unknown tool \"{tool}\".")),
    }
    state
        .queue
        .lock()
        .map_err(|_| "The proposal queue is unavailable.".to_string())?
        .push(proposal);
    Ok(Value::String(QUEUED.into()))
}

fn run<Tz: TimeZone>(
    env: &Env<Tz>,
    state: &State,
    ps: &Pseudonyms,
    tool: &str,
    args: &Value,
) -> Result<Value, String> {
    if !mcp_wire::is_tool(tool) {
        return Err(format!("Unknown tool \"{tool}\"."));
    }
    if tool == "ts_tasks" {
        return ts_tasks(env.db, state.ts());
    }
    let tasks = db::snapshot(env.db).map_err(|e| e.to_string())?.tasks;
    match tool {
        "tasks_list" => tasks_list(env, &tasks, args, ps),
        "task_get" => task_get(env, &tasks, args, ps),
        "day_summary" => day_summary(env, &tasks, args),
        "week_summary" => week_summary(env, &tasks, args),
        "current_measurement" => current_measurement(env, &tasks),
        _ => propose(env, state, &tasks, tool, args, ps),
    }
}

/// Wykonuje narzędzie i **przepuszcza wynik (także błąd) przez `ai_privacy::redact`**.
/// Pseudonimy są wspólne dla wszystkich wywołań w czasie działania aplikacji (i z
/// czatem w aplikacji), a
/// argumenty od modelu przechodzą przez `restore`, więc „Osoba 1” z wcześniejszej
/// odpowiedzi znaczy tę samą osobę także w propozycji.
pub fn call<Tz: TimeZone>(
    env: &Env<Tz>,
    state: &State,
    dict: &Dictionary,
    tool: &str,
    args: &Value,
) -> Result<String, String> {
    let mut ps = state
        .pseudonyms
        .lock()
        .map_err(|_| "The assistant filter is unavailable.".to_string())?;
    match run(env, state, &ps, tool, args) {
        Ok(value) => {
            let text = match value {
                Value::String(s) => s,
                other => other.to_string(),
            };
            Ok(ai_privacy::redact_with(&text, dict, &mut ps))
        }
        Err(error) => Err(ai_privacy::redact_with(&error, dict, &mut ps)),
    }
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

fn reply(result: Result<String, String>) -> Value {
    match result {
        Ok(text) => json!({"ok": true, "text": text}),
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// Folder dla `propose_link_folder`: katalog roboczy sesji, która wywołała
/// narzędzie (`cwd` z ramki programu `tuktuk-track-mcp`). Bez niego (albo gdy to
/// katalog asystenta) foldery pracujących agentów od najnowszego.
fn session_folders(
    cwd: &str,
    ai_dir: Option<&std::path::Path>,
    mut agents: Vec<crate::agents::Agent>,
) -> Vec<String> {
    let usable = |folder: &str| {
        !folder.trim().is_empty() && !ai_dir.is_some_and(|d| ai::is_ai_dir(folder, d))
    };
    let cwd = cwd.trim();
    if usable(cwd) && std::path::Path::new(cwd).is_absolute() {
        return vec![cwd.to_string()];
    }
    agents.retain(|a| usable(&a.cwd));
    agents.sort_by_key(|a| std::cmp::Reverse(a.since));
    agents.into_iter().map(|a| a.cwd).collect()
}

/// Jedna wiadomość z potoku: `{"tool": …, "arguments": {…}}`.
fn on_request(app: &tauri::AppHandle, body: &[u8]) -> Value {
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        return reply(Err("Invalid request.".into()));
    };
    let tool = request["tool"].as_str().unwrap_or("");
    if !mcp_wire::is_tool(tool) {
        return reply(Err(format!("Unknown tool \"{tool}\".")));
    }
    let settings = crate::settings::current(app);
    if let Err(message) = gate(&settings) {
        return reply(Err(message.into()));
    }
    let Some(state) = app.try_state::<State>() else {
        return reply(Err("TukTuk Track is starting.".into()));
    };
    let args = if request["arguments"].is_object() {
        request["arguments"].clone()
    } else {
        json!({})
    };
    // Słownik czyta bazę sam, więc przed zablokowaniem bazy na czas narzędzia.
    let dict = ai::dictionary(app, &settings);
    let folders = session_folders(
        request["cwd"].as_str().unwrap_or(""),
        ai::dir(app).as_deref(),
        crate::agents::current(app),
    );
    let before = state.proposals().len();
    let result = {
        let database = app.state::<crate::Database>();
        let Ok(db) = database.0.lock() else {
            return reply(Err("The database is unavailable.".into()));
        };
        let env = Env {
            db: &db,
            tz: &chrono::Local,
            settings: &settings,
            now: db::now(),
            folders: &folders,
        };
        call(&env, &state, &dict, tool, &args)
    };
    if state.proposals().len() != before || tool.starts_with("propose_") {
        let _ = app.emit(EVENT, ());
    }
    reply(result)
}

/// Serwer nazwanego potoku: każde połączenie programu `tuktuk-track-mcp` to jedna
/// ramka i odpowiedź (jak host rozszerzenia przeglądarki).
pub fn start(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) =
            crate::browser::pipe::serve(&mcp_wire::pipe_name(), move |body| on_request(&app, body))
        {
            eprintln!("Serwer MCP: potok niedostępny ({e}).");
        }
    });
}

/// Rejestruje stan modułu; wołać przed utworzeniem okien.
pub fn setup(app: &tauri::AppHandle) {
    app.manage(State::default());
}

/// Zapamiętuje listę zadań TS (po udanym pobraniu w aplikacji) dla `ts_tasks`.
pub fn remember_ts_tasks(app: &tauri::AppHandle, projects: &[Project]) {
    if let Some(state) = app.try_state::<State>() {
        state.remember_ts(projects);
    }
}

// ---------------------------------------------------------------------------
// Rejestracja w CLI

/// Stan rejestracji do Ustawień.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Program serwera obok aplikacji.
    pub exe: String,
    pub exe_found: bool,
    /// Serwer ma być zarejestrowany (`ai_mcp` i wybrany silnik).
    pub enabled: bool,
    /// Wynik ostatniej synchronizacji, po jednym wpisie na CLI.
    pub entries: Vec<Entry>,
}

/// `tuktuk-track-mcp.exe` obok aplikacji; `None`, gdy go nie ma.
pub fn exe_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(mcp_register::EXE)))
        .filter(|p| p.is_file())
}

/// Czy stary serwer MCP (przed zmianą nazwy aplikacji) był już sprawdzony w tym procesie.
static LEGACY_MCP_CHECKED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Stary serwer sprzed zmiany nazwy (N1). Trwały znacznik w `settings`
/// (`legacy_mcp_checked_at`) powstaje, gdy każde wykryte CLI odpowiedziało, a stary
/// wpis usunięto albo go nie było; potem kolejne starty nie uruchamiają już
/// dodatkowych procesów CLI. Po błędzie CLI próba wraca przy następnym starcie
/// (w tym procesie najwyżej raz, i dopiero gdy wykrywanie znalazło jakieś CLI).
fn remove_legacy_once(app: &tauri::AppHandle, dir: &std::path::Path) {
    use std::sync::atomic::Ordering;
    if LEGACY_MCP_CHECKED.load(Ordering::SeqCst) {
        return;
    }
    let database = app.try_state::<crate::Database>();
    let checked = database
        .as_ref()
        .and_then(|db| db.0.lock().ok().map(|db| crate::legacy::mcp_checked(&db)))
        .unwrap_or(false);
    if checked {
        LEGACY_MCP_CHECKED.store(true, Ordering::SeqCst);
        return;
    }
    let detected = ai::detected_cached(app);
    if !detected.any() {
        return;
    }
    LEGACY_MCP_CHECKED.store(true, Ordering::SeqCst);
    let cleanup = mcp_register::remove_legacy(&ai::Process, &detected, dir);
    if cleanup.complete {
        if let Some(Ok(db)) = database.as_ref().map(|db| db.0.lock()) {
            if let Err(e) = crate::legacy::mark_mcp_checked(&db, crate::db::now()) {
                eprintln!("Znacznik starego serwera MCP: {e}");
            }
        }
    }
}

fn run_sync(app: &tauri::AppHandle) -> Status {
    let Some(state) = app.try_state::<State>() else {
        return Status::default();
    };
    let _io = state.io.lock();
    let settings = crate::settings::current(app);
    let exe = exe_path();
    let enabled = gate(&settings).is_ok();
    let mut entries = Vec::new();
    // Puste `ai_engine` (jeszcze nie pytano) nie uruchamia żadnego procesu.
    if !settings.ai_engine.is_empty() {
        if let Some(dir) = ai::dir(app).filter(|d| std::fs::create_dir_all(d).is_ok()) {
            remove_legacy_once(app, &dir);
            entries = mcp_register::sync_all(
                &ai::Process,
                &ai::detected_cached(app),
                &settings.ai_engine,
                settings.ai_mcp,
                exe.as_deref(),
                &dir,
            );
        }
    }
    let status = Status {
        exe: exe
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        exe_found: exe.is_some(),
        enabled,
        entries,
    };
    if let Ok(mut stored) = state.status.lock() {
        *stored = status.clone();
    }
    let _ = app.emit("mcp-status-changed", ());
    status
}

/// Zgodność rejestracji z ustawieniami (start, zmiana `ai_mcp` albo silnika,
/// przywrócenie kopii). Procesy CLI w osobnym wątku, nigdy w wątku okna.
pub fn sync(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        run_sync(&app);
    });
}

/// Czy przy starcie aplikacji sprawdzać rejestrację. Tylko przy wybranym
/// silniku: przy `""` (jeszcze nie pytano) i `"off"` („Nie używaj”) start nie
/// uruchamia żadnego procesu CLI. Rejestrację usuwa synchronizacja w chwili
/// przełączenia na „Nie używaj” (`set_settings`) i po przywróceniu kopii, a
/// pozostały wpis i tak nic nie zrobi: `gate` odrzuca każde narzędzie.
fn startup_sync_needed(settings: &AppSettings) -> bool {
    Engine::parse(&settings.ai_engine).is_some()
}

/// Synchronizacja przy starcie aplikacji (zob. `startup_sync_needed`).
pub fn sync_at_startup(app: &tauri::AppHandle) {
    if startup_sync_needed(&crate::settings::current(app)) {
        sync(app);
    }
}

// ---------------------------------------------------------------------------
// Komendy okien

/// Propozycje czekające na decyzję (baner w oknie głównym).
#[tauri::command]
pub fn mcp_proposals(app: tauri::AppHandle) -> Vec<Proposal> {
    app.try_state::<State>()
        .map(|s| s.proposals())
        .unwrap_or_default()
}

/// Zdejmuje propozycję z kolejki: po „Odrzuć” albo po udanym „Zastosuj”
/// (zmianę wykonują wcześniej istniejące komendy).
#[tauri::command]
pub fn mcp_proposal_resolve(app: tauri::AppHandle, id: u64) -> Vec<Proposal> {
    let left = app
        .try_state::<State>()
        .map(|s| s.resolve(id))
        .unwrap_or_default();
    let _ = app.emit(EVENT, ());
    left
}

/// Stan rejestracji z ostatniej synchronizacji (bez uruchamiania CLI).
#[tauri::command]
pub fn mcp_status(app: tauri::AppHandle) -> Status {
    let settings = crate::settings::current(&app);
    let exe = exe_path();
    let mut status = app
        .try_state::<State>()
        .and_then(|s| s.status.lock().ok().map(|s| s.clone()))
        .unwrap_or_default();
    status.exe = exe
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    status.exe_found = exe.is_some();
    status.enabled = gate(&settings).is_ok();
    status
}

/// „Zarejestruj ponownie”: synchronizacja po kliknięciu.
#[tauri::command]
pub async fn mcp_sync(app: tauri::AppHandle) -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(move || run_sync(&app))
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
