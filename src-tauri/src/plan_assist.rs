//! Rano i przegląd tygodnia z asystentem (PLAN_BACKLOG.md, A6).
//!
//! Dwie funkcje, obie **na kliknięcie** (nigdy same), za przełącznikiem
//! `ai_plan_assist` i wybranym silnikiem; model główny, dane przez `ai::ask`
//! (filtr osobowy `ai_privacy::redact` działa wewnątrz `ai::run`):
//!
//! - **Komentarz asystenta w Rano** ([`ai_morning_comment`]): ocena planu dnia
//!   względem pojemności (cel minus spotkania) i terminów. Wynik to krótki
//!   komentarz i ewentualne propozycje przesunięć zadania z dzisiejszego planu
//!   na jeden z najbliższych dni roboczych. Komentarz niczego nie zmienia;
//!   przesunięcie jest kartą „Zastosuj” / „Odrzuć”, a zapisu dokonuje dopiero
//!   kliknięcie (istniejąca komenda `plan_task`).
//! - **Szkic wniosków** ([`ai_week_draft`]): tekst do pola „Wnioski” przeglądu
//!   tygodnia (`week_notes`) z czasu kontra cel, planu kontra wykonania, czasu
//!   zadań i agentów oraz notatek dni. Szkic wypełnia pole do edycji; zapis
//!   robi dopiero „Zapisz wnioski”.
//!
//! Przepuszczane są tylko propozycje zgodne z danymi wejściowymi: przesunięcie
//! dotyczy otwartego zadania z dzisiejszego planu, dzień musi być jednym z
//! zaproponowanych dni roboczych (i nie późniejszy niż termin zadania).
use crate::ai::{self, Tier};
use crate::settings::AppSettings;
use crate::{agent_time, calendar, capacity, db, holidays, planner, stats, timesheet, week_notes};
use chrono::{Datelike, Duration, NaiveDate, TimeZone};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use tauri::Manager;

/// Rodzaje wywołań w dzienniku „Ostatnie wywołania”.
pub const MORNING_KIND: &str = "morning";
pub const WEEK_KIND: &str = "week_notes";
/// Ile dni roboczych po dzisiejszym jest oferowanych jako cel przesunięcia.
pub const HORIZON_DAYS: usize = 5;
/// Jak daleko (dni kalendarzowych) szukamy tych dni roboczych.
const HORIZON_SCAN: i64 = 21;
/// Terminy z tego okna (dni od dziś) trafiają do modelu.
pub const DEADLINE_WINDOW: i64 = 14;
pub const PLAN_MAX: usize = 40;
pub const DEADLINES_MAX: usize = 20;
pub const MOVES_MAX: usize = 10;
pub const COMMENT_MAX: usize = 1500;
pub const REASON_MAX: usize = 300;
pub const SUBJECT_MAX: usize = 160;
pub const WEEK_TASKS_MAX: usize = 15;
pub const NOTE_MAX: usize = 300;
pub const DRAFT_MAX: usize = 3000;

pub const OFF_MESSAGE: &str =
    "Asystent w Rano i przeglądzie tygodnia jest wyłączony w Ustawieniach → Asystent AI.";
pub const EMPTY_MORNING: &str = "Brak zadań w planie dnia ani terminów do oceny.";
pub const NOTHING_MORNING: &str = "Asystent nie ma uwag do planu.";
pub const EMPTY_WEEK: &str = "Brak danych z tego tygodnia do szkicu.";
pub const NOTHING_WEEK: &str = "Asystent nie napisał szkicu wniosków.";

fn clip(text: &str, max: usize) -> String {
    text.trim().chars().take(max).collect()
}

fn minutes(ms: i64) -> i64 {
    (ms.max(0) + 30_000) / 60_000
}

fn put(map: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.trim().is_empty() {
        map.insert(key.into(), json!(value.trim()));
    }
}

fn hm<Tz: TimeZone>(tz: &Tz, ms: i64) -> String {
    tz.timestamp_millis_opt(ms)
        .single()
        .map(|t| t.naive_local().format("%H:%M").to_string())
        .unwrap_or_default()
}

fn day_text(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn key_of(task: &db::Task) -> String {
    task.jira_key
        .clone()
        .or_else(|| task.redmine_id.map(|id| format!("#{id}")))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Rano: dane

/// Zadanie planu albo termin dla modelu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlanTask {
    pub id: i64,
    pub title: String,
    pub project: String,
    pub key: String,
    pub estimate_minutes: u32,
    pub due_day: String,
    pub planned_day: String,
    pub priority: i32,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Meeting {
    pub from: i64,
    pub to: i64,
    /// Tytuł tylko przy `ai_send_meeting_titles`.
    pub title: Option<String>,
}

/// Najbliższy dzień roboczy, na który można przesunąć zadanie.
#[derive(Clone, Debug, PartialEq)]
pub struct UpcomingDay {
    pub day: String,
    pub goal_ms: i64,
    pub meetings_ms: i64,
    pub capacity_ms: i64,
    /// Suma szacunków otwartych zadań już zaplanowanych na ten dzień.
    pub planned_minutes: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MorningInput {
    pub day: String,
    pub goal_ms: i64,
    pub meetings_ms: i64,
    pub capacity_ms: i64,
    pub off: bool,
    pub tracked_ms: i64,
    pub meetings: Vec<Meeting>,
    /// Plan dnia: najpierw otwarte, w kolejności listy „Plan dnia”.
    pub plan: Vec<PlanTask>,
    /// Otwarte zadania spoza planu dnia z terminem w najbliższych dniach.
    pub deadlines: Vec<PlanTask>,
    pub upcoming: Vec<UpcomingDay>,
    /// Szacunki (`estimate_minutes`, `planned_minutes`) są po poprawce szacunków.
    pub corrected: bool,
}

/// Poprawka szacunków z widoku Rano („Planuj z poprawką szacunków”): mnożniki
/// liczy interfejs (`estimates.ts`, `corrections`), tu tylko się je stosuje,
/// żeby model widział to samo obciążenie co pasek pojemności.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EstimateCorrection {
    /// Mnożnik dla projektów bez własnego wpisu.
    pub all: f64,
    /// Mnożniki według nazwy projektu z zadania (dokładnie jak w `project`).
    #[serde(default)]
    pub projects: BTreeMap<String, f64>,
}

impl EstimateCorrection {
    fn factor(&self, project: &str) -> f64 {
        let factor = self.projects.get(project).copied().unwrap_or(self.all);
        if factor.is_finite() {
            factor.clamp(0.5, 3.0)
        } else {
            1.0
        }
    }

    /// Szacunek po poprawce w minutach (jak `plannedMs`); 0 bez szacunku.
    pub fn minutes(&self, estimate: u32, project: &str) -> u32 {
        if estimate == 0 {
            return 0;
        }
        (f64::from(estimate) * self.factor(project)).round() as u32
    }
}

impl MorningInput {
    /// Czy nie ma nic, o co warto pytać model.
    pub fn is_empty(&self) -> bool {
        self.plan.is_empty() && self.deadlines.is_empty()
    }
}

fn plan_task(t: &db::Task, correction: Option<&EstimateCorrection>) -> PlanTask {
    PlanTask {
        id: t.id,
        title: clip(&t.title, SUBJECT_MAX),
        project: clip(&t.details.project, SUBJECT_MAX),
        key: key_of(t),
        estimate_minutes: estimate(t, correction),
        due_day: t.details.due_day.clone(),
        planned_day: t.details.planned_day.clone(),
        priority: t.details.priority,
        done: t.done,
    }
}

fn estimate(t: &db::Task, correction: Option<&EstimateCorrection>) -> u32 {
    match correction {
        Some(c) => c.minutes(t.details.estimate_minutes, &t.details.project),
        None => t.details.estimate_minutes,
    }
}

/// Zadanie, które liczy się w planie: nie archiwalne i nie wzorzec cyklu.
fn plannable(t: &db::Task) -> bool {
    !t.archived && !planner::is_recurring(&t.details)
}

/// Odczyt dnia z bazy: plan, spotkania, pojemność, terminy i dni do przesunięcia.
pub fn collect_morning<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    settings: &AppSettings,
    date: NaiveDate,
    now: i64,
) -> Result<MorningInput, String> {
    collect_morning_with(db, tz, settings, date, now, None)
}

/// Jak `collect_morning`, z szacunkami po poprawce, gdy podano `correction`.
pub fn collect_morning_with<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    settings: &AppSettings,
    date: NaiveDate,
    now: i64,
    correction: Option<&EstimateCorrection>,
) -> Result<MorningInput, String> {
    let day = day_text(date);
    let bounds = stats::day_bounds(tz, date);
    let snapshot = db::snapshot(db).map_err(|e| e.to_string())?;
    let events = calendar::meetings_between(db, bounds.0, bounds.1)?;
    let target = i64::from(holidays::day_target_minutes(settings, date)) * 60_000;
    let cap = capacity::day_capacity(target, &events, bounds);
    let mut input = MorningInput {
        day: day.clone(),
        goal_ms: target,
        meetings_ms: cap.meetings_ms,
        capacity_ms: cap.capacity_ms,
        off: cap.off,
        tracked_ms: stats::day_total_ms_in(db, tz, date, now).map_err(|e| e.to_string())?,
        corrected: correction.is_some(),
        ..MorningInput::default()
    };
    for e in &events {
        if e.all_day || !calendar::occupies(e) {
            continue;
        }
        let (a, b) = (e.start.max(bounds.0), e.end.min(bounds.1));
        if b > a {
            input.meetings.push(Meeting {
                from: a,
                to: b,
                title: settings
                    .ai_send_meeting_titles
                    .then(|| clip(&e.subject, SUBJECT_MAX))
                    .filter(|t| !t.is_empty()),
            });
        }
    }
    // Plan dnia w kolejności z listy „Plan dnia” (zakończone na końcu).
    let mut plan: Vec<&db::Task> = snapshot
        .tasks
        .iter()
        .filter(|t| plannable(t) && t.details.planned_day == day)
        .collect();
    plan.sort_by(|a, b| {
        a.done
            .cmp(&b.done)
            .then(b.details.priority.cmp(&a.details.priority))
            .then(a.details.order.cmp(&b.details.order))
            .then(b.id.cmp(&a.id))
    });
    input.plan = plan
        .into_iter()
        .take(PLAN_MAX)
        .map(|t| plan_task(t, correction))
        .collect();
    // Terminy: otwarte zadania spoza dzisiejszego planu, termin do `DEADLINE_WINDOW` dni.
    let last = day_text(date + Duration::days(DEADLINE_WINDOW));
    let mut deadlines: Vec<&db::Task> = snapshot
        .tasks
        .iter()
        .filter(|t| {
            plannable(t)
                && !t.done
                && !t.details.due_day.is_empty()
                && t.details.due_day <= last
                && t.details.planned_day != day
        })
        .collect();
    deadlines.sort_by(|a, b| {
        a.details
            .due_day
            .cmp(&b.details.due_day)
            .then(a.id.cmp(&b.id))
    });
    input.deadlines = deadlines
        .into_iter()
        .take(DEADLINES_MAX)
        .map(|t| plan_task(t, correction))
        .collect();
    // Najbliższe dni robocze (bez dni wolnych, świąt i „poza biurem”).
    for offset in 1..=HORIZON_SCAN {
        if input.upcoming.len() >= HORIZON_DAYS {
            break;
        }
        let next = date + Duration::days(offset);
        let target = i64::from(holidays::day_target_minutes(settings, next)) * 60_000;
        if target <= 0 {
            continue;
        }
        let range = stats::day_bounds(tz, next);
        let events = calendar::meetings_between(db, range.0, range.1)?;
        let cap = capacity::day_capacity(target, &events, range);
        if cap.off {
            continue;
        }
        let key = day_text(next);
        let planned: i64 = snapshot
            .tasks
            .iter()
            .filter(|t| plannable(t) && !t.done && t.details.planned_day == key)
            .map(|t| i64::from(estimate(t, correction)))
            .sum();
        input.upcoming.push(UpcomingDay {
            day: key,
            goal_ms: target,
            meetings_ms: cap.meetings_ms,
            capacity_ms: cap.capacity_ms,
            planned_minutes: planned,
        });
    }
    Ok(input)
}

fn task_json(t: &PlanTask, with_planned: bool) -> Value {
    let mut m = Map::new();
    m.insert("taskId".into(), json!(t.id));
    put(&mut m, "title", &t.title);
    put(&mut m, "project", &t.project);
    put(&mut m, "key", &t.key);
    if t.estimate_minutes > 0 {
        m.insert("estimateMinutes".into(), json!(t.estimate_minutes));
    }
    put(&mut m, "dueDay", &t.due_day);
    if with_planned {
        put(&mut m, "plannedDay", &t.planned_day);
    }
    if t.priority != 0 {
        m.insert("priority".into(), json!(t.priority));
    }
    if t.done {
        m.insert("done".into(), json!(true));
    }
    Value::Object(m)
}

/// Dane dnia jako JSON dla modelu: godziny „HH:MM” w strefie użytkownika, bez
/// tytułów spotkań (gdy wyłączone). Filtr osobowy dokłada `ai::run`.
pub fn morning_json<Tz: TimeZone>(input: &MorningInput, tz: &Tz) -> Value {
    let mut out = Map::new();
    out.insert("day".into(), json!(input.day));
    out.insert("goalMinutes".into(), json!(minutes(input.goal_ms)));
    out.insert("meetingsMinutes".into(), json!(minutes(input.meetings_ms)));
    out.insert("capacityMinutes".into(), json!(minutes(input.capacity_ms)));
    if input.off {
        out.insert("off".into(), json!(true));
    }
    out.insert("trackedMinutes".into(), json!(minutes(input.tracked_ms)));
    if input.corrected {
        out.insert("estimatesCorrected".into(), json!(true));
    }
    out.insert(
        "meetings".into(),
        json!(input
            .meetings
            .iter()
            .map(|e| {
                let mut m = Map::new();
                m.insert("from".into(), json!(hm(tz, e.from)));
                m.insert("to".into(), json!(hm(tz, e.to)));
                if let Some(title) = &e.title {
                    put(&mut m, "title", title);
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "plan".into(),
        json!(input
            .plan
            .iter()
            .map(|t| task_json(t, false))
            .collect::<Vec<_>>()),
    );
    out.insert(
        "deadlines".into(),
        json!(input
            .deadlines
            .iter()
            .map(|t| task_json(t, true))
            .collect::<Vec<_>>()),
    );
    out.insert(
        "nextDays".into(),
        json!(input
            .upcoming
            .iter()
            .map(|d| json!({
                "day": d.day,
                "goalMinutes": minutes(d.goal_ms),
                "meetingsMinutes": minutes(d.meetings_ms),
                "capacityMinutes": minutes(d.capacity_ms),
                "plannedMinutes": d.planned_minutes,
            }))
            .collect::<Vec<_>>()),
    );
    Value::Object(out)
}

/// Schemat odpowiedzi: `{ "comment": …, "moves": [ { taskId, toDay, reason } ] }`.
pub fn morning_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "comment": { "type": "string" },
            "moves": {
                "type": "array",
                "maxItems": MOVES_MAX,
                "items": {
                    "type": "object",
                    "properties": {
                        "taskId": { "type": "integer" },
                        "toDay": { "type": "string" },
                        "reason": { "type": "string" }
                    },
                    "required": ["taskId", "toDay", "reason"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["comment", "moves"],
        "additionalProperties": false
    })
}

/// Treść zapytania „Komentarz asystenta” przed filtrem osobowym; tę samą treść
/// pokazuje „Pokaż, co zostanie wysłane”.
pub fn morning_prompt<Tz: TimeZone>(input: &MorningInput, tz: &Tz) -> String {
    format!(
        "Aplikacja do śledzenia czasu pracy prosi o krótką ocenę planu na dziś. Porównaj sumę \
         szacunków zadań z planu (\"plan\", pole \"estimateMinutes\"; brak oznacza nieznany czas, \
         zadania z \"done\" są już zrobione) z pojemnością dnia (\"capacityMinutes\": cel minus \
         spotkania) oraz uwzględnij terminy (\"dueDay\", także zadań z listy \"deadlines\"). \
         Gdy w danych jest \"estimatesCorrected\": true, \"estimateMinutes\" i \"plannedMinutes\" \
         są już szacunkami z poprawką według dotychczasowych rozjazdów (tak liczy je aplikacja) \
         i nie należy ich dalej korygować. \
         Napisz po polsku w \"comment\" od dwóch do czterech krótkich zdań: czy plan mieści się \
         w dniu, co jest zagrożone terminem i co warto zrobić najpierw. Gdy plan jest za duży, \
         zaproponuj w \"moves\" przesunięcia otwartych zadań z planu na jeden z dni z listy \
         \"nextDays\" (\"toDay\" musi być jednym z tych dni i nie może być późniejszy niż termin \
         zadania); wybieraj dni z wolną pojemnością (\"capacityMinutes\" minus \"plannedMinutes\"). \
         Do każdego przesunięcia dodaj w \"reason\" jedno krótkie zdanie z konkretnym powodem. \
         Używaj wyłącznie id zadań z listy \"plan\". Nie zgaduj: gdy plan się mieści, nie \
         proponuj przesunięć (pusta lista jest poprawna). Odpowiedz wyłącznie obiektem JSON \
         {{\"comment\": …, \"moves\": [{{\"taskId\": …, \"toDay\": …, \"reason\": …}}]}}.\n\nDane:\n{}",
        morning_json(input, tz)
    )
}

// ---------------------------------------------------------------------------
// Rano: propozycje

/// Propozycja przesunięcia zadania z dzisiejszego planu na inny dzień.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorningMove {
    /// Numer w odpowiedzi (stabilny klucz karty).
    pub id: usize,
    pub task_id: i64,
    pub task_title: String,
    pub from_day: String,
    pub to_day: String,
    pub reason: String,
}

/// Wynik „Komentarz asystenta”.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MorningComment {
    pub day: String,
    pub comment: String,
    pub moves: Vec<MorningMove>,
    /// Powód braku komentarza (pusty, gdy jest).
    pub message: String,
}

/// Odpowiedź modelu jako komentarz i przesunięcia zgodne z danymi: zadanie
/// otwarte z planu dnia, dzień z `nextDays` nie późniejszy niż termin, jedno
/// przesunięcie na zadanie.
pub fn parse_morning(value: &Value, input: &MorningInput) -> (String, Vec<MorningMove>) {
    let comment = value
        .get("comment")
        .and_then(Value::as_str)
        .map(|c| clip(c, COMMENT_MAX))
        .unwrap_or_default();
    let mut moves: Vec<MorningMove> = Vec::new();
    let items = value
        .get("moves")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for (index, raw) in items.iter().enumerate() {
        if moves.len() >= MOVES_MAX {
            break;
        }
        let Some(task_id) = raw.get("taskId").and_then(Value::as_i64) else {
            continue;
        };
        let Some(task) = input.plan.iter().find(|t| t.id == task_id && !t.done) else {
            continue;
        };
        let to_day = raw
            .get("toDay")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !input.upcoming.iter().any(|d| d.day == to_day) {
            continue;
        }
        if !task.due_day.is_empty() && to_day > task.due_day.as_str() {
            continue;
        }
        if moves.iter().any(|m| m.task_id == task_id) {
            continue;
        }
        moves.push(MorningMove {
            id: index,
            task_id,
            task_title: task.title.clone(),
            from_day: input.day.clone(),
            to_day: to_day.to_string(),
            reason: clip(
                raw.get("reason").and_then(Value::as_str).unwrap_or(""),
                REASON_MAX,
            ),
        });
    }
    (comment, moves)
}

/// Komentarz na dowolnym `ask` (testowalny rdzeń komendy). Dzień bez planu i
/// terminów nie woła modelu; błąd wywołania wraca do interfejsu.
pub fn comment<Tz: TimeZone>(
    ask: &dyn Fn(&str, &str, &Value) -> Result<Value, String>,
    input: &MorningInput,
    tz: &Tz,
) -> Result<MorningComment, String> {
    if input.is_empty() {
        return Ok(MorningComment {
            day: input.day.clone(),
            comment: String::new(),
            moves: Vec::new(),
            message: EMPTY_MORNING.into(),
        });
    }
    let answer = ask(MORNING_KIND, &morning_prompt(input, tz), &morning_schema())?;
    let (comment, moves) = parse_morning(&answer, input);
    let message = if comment.is_empty() && moves.is_empty() {
        NOTHING_MORNING.to_string()
    } else {
        String::new()
    };
    Ok(MorningComment {
        day: input.day.clone(),
        comment,
        moves,
        message,
    })
}

// ---------------------------------------------------------------------------
// Przegląd tygodnia: dane

#[derive(Clone, Debug, PartialEq)]
pub struct WeekDay {
    pub day: String,
    pub tracked_ms: i64,
    pub goal_ms: i64,
    pub meetings_ms: i64,
    pub capacity_ms: i64,
    pub off: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeekTask {
    pub id: i64,
    pub title: String,
    pub project: String,
    pub key: String,
    pub tracked_ms: i64,
    pub estimate_minutes: u32,
    pub done: bool,
    /// Notatki dni z tego tygodnia: (dzień, tekst).
    pub notes: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeekInput {
    /// Poniedziałek `RRRR-MM-DD`.
    pub week: String,
    pub days: Vec<WeekDay>,
    pub tasks: Vec<WeekTask>,
    pub planned: usize,
    pub planned_done: usize,
    pub planned_open: usize,
    pub agent_ms: i64,
    pub agent_unmatched_ms: i64,
}

impl WeekInput {
    pub fn is_empty(&self) -> bool {
        self.days.iter().all(|d| d.tracked_ms == 0)
            && self.tasks.is_empty()
            && self.planned == 0
            && self.agent_ms == 0
    }
}

/// Odczyt tygodnia (poniedziałek `monday`) z bazy.
pub fn collect_week<Tz: TimeZone>(
    db: &Connection,
    tz: &Tz,
    settings: &AppSettings,
    monday: NaiveDate,
    now: i64,
) -> Result<WeekInput, String> {
    let mut input = WeekInput {
        week: day_text(monday),
        ..WeekInput::default()
    };
    let snapshot = db::snapshot(db).map_err(|e| e.to_string())?;
    for offset in 0..7 {
        let date = monday + Duration::days(offset);
        let range = stats::day_bounds(tz, date);
        let events = calendar::meetings_between(db, range.0, range.1)?;
        let target = i64::from(holidays::day_target_minutes(settings, date)) * 60_000;
        let cap = capacity::day_capacity(target, &events, range);
        input.days.push(WeekDay {
            day: day_text(date),
            tracked_ms: stats::day_total_ms_in(db, tz, date, now).map_err(|e| e.to_string())?,
            goal_ms: target,
            meetings_ms: cap.meetings_ms,
            capacity_ms: cap.capacity_ms,
            off: cap.off,
        });
    }
    let sunday = monday + Duration::days(6);
    let from = stats::day_bounds(tz, monday).0;
    let to = stats::day_bounds(tz, sunday).1;
    // Czas zadań w tygodniu (bez zadań z kosza), trwający liczony do `now`.
    let mut per_task: BTreeMap<i64, i64> = BTreeMap::new();
    let mut stmt = db
        .prepare(
            "SELECT s.task_id, s.started_at, s.ended_at FROM sessions s
             WHERE s.started_at < ?2 AND COALESCE(s.ended_at, ?3) > ?1
               AND s.task_id NOT IN (SELECT task_id FROM task_lifecycle WHERE deleted_at IS NOT NULL)",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![from, to, now], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (task, start, end) = row.map_err(|e| e.to_string())?;
        let ms = (end.unwrap_or(now).min(to) - start.max(from)).max(0);
        *per_task.entry(task).or_default() += ms;
    }
    let keys: Vec<String> = (0..7)
        .map(|o| day_text(monday + Duration::days(o)))
        .collect();
    let mut tasks: Vec<WeekTask> = Vec::new();
    for t in &snapshot.tasks {
        let tracked = per_task.get(&t.id).copied().unwrap_or(0);
        let notes: Vec<(String, String)> = keys
            .iter()
            .filter_map(|k| {
                let text = clip(t.details.notes.get(k)?, NOTE_MAX);
                (!text.is_empty()).then(|| (k.clone(), text))
            })
            .collect();
        if tracked == 0 && notes.is_empty() {
            continue;
        }
        tasks.push(WeekTask {
            id: t.id,
            title: clip(&t.title, SUBJECT_MAX),
            project: clip(&t.details.project, SUBJECT_MAX),
            key: key_of(t),
            tracked_ms: tracked,
            estimate_minutes: t.details.estimate_minutes,
            done: t.done,
            notes,
        });
    }
    tasks.sort_by(|a, b| b.tracked_ms.cmp(&a.tracked_ms).then(a.id.cmp(&b.id)));
    tasks.truncate(WEEK_TASKS_MAX);
    input.tasks = tasks;
    // Plan kontra wykonanie: zadania z planem w tym tygodniu.
    let (first, last) = (day_text(monday), day_text(sunday));
    for t in snapshot
        .tasks
        .iter()
        .filter(|t| !planner::is_recurring(&t.details) && (t.done || !t.archived))
    {
        let planned = t.details.planned_day.as_str();
        if planned >= first.as_str() && planned <= last.as_str() {
            input.planned += 1;
            if t.done {
                input.planned_done += 1;
            } else {
                input.planned_open += 1;
            }
        }
    }
    let agents = agent_time::query(db, from, to, now).map_err(|e| e.to_string())?;
    input.agent_ms = agents.totals.iter().map(|t| t.agent_ms).sum::<i64>() + agents.unmatched_ms;
    input.agent_unmatched_ms = agents.unmatched_ms;
    Ok(input)
}

/// Dane tygodnia jako JSON dla modelu (bez autorów i bez tytułów spotkań).
pub fn week_json(input: &WeekInput) -> Value {
    let mut out = Map::new();
    out.insert("week".into(), json!(input.week));
    out.insert(
        "days".into(),
        json!(input
            .days
            .iter()
            .map(|d| {
                let mut m = Map::new();
                m.insert("day".into(), json!(d.day));
                m.insert("trackedMinutes".into(), json!(minutes(d.tracked_ms)));
                m.insert("goalMinutes".into(), json!(minutes(d.goal_ms)));
                m.insert("meetingsMinutes".into(), json!(minutes(d.meetings_ms)));
                if d.off {
                    m.insert("off".into(), json!(true));
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    out.insert(
        "plan".into(),
        json!({
            "planned": input.planned,
            "done": input.planned_done,
            "open": input.planned_open,
        }),
    );
    out.insert(
        "tasks".into(),
        json!(input
            .tasks
            .iter()
            .map(|t| {
                let mut m = Map::new();
                m.insert("taskId".into(), json!(t.id));
                put(&mut m, "title", &t.title);
                put(&mut m, "project", &t.project);
                put(&mut m, "key", &t.key);
                m.insert("trackedMinutes".into(), json!(minutes(t.tracked_ms)));
                if t.estimate_minutes > 0 {
                    m.insert("estimateMinutes".into(), json!(t.estimate_minutes));
                }
                if t.done {
                    m.insert("done".into(), json!(true));
                }
                if !t.notes.is_empty() {
                    m.insert(
                        "notes".into(),
                        json!(t
                            .notes
                            .iter()
                            .map(|(day, text)| json!({"day": day, "note": text}))
                            .collect::<Vec<_>>()),
                    );
                }
                Value::Object(m)
            })
            .collect::<Vec<_>>()),
    );
    if input.agent_ms > 0 {
        out.insert(
            "agentTime".into(),
            json!({
                "totalMinutes": minutes(input.agent_ms),
                "unassignedMinutes": minutes(input.agent_unmatched_ms),
            }),
        );
    }
    Value::Object(out)
}

/// Schemat odpowiedzi: `{ "notes": "…" }`.
pub fn week_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "notes": { "type": "string" } },
        "required": ["notes"],
        "additionalProperties": false
    })
}

/// Treść zapytania „Szkic wniosków” przed filtrem osobowym.
pub fn week_prompt(input: &WeekInput) -> String {
    format!(
        "Aplikacja do śledzenia czasu pracy prosi o szkic pola „Wnioski” w przeglądzie tygodnia. \
         Na podstawie danych napisz po polsku krótki szkic (od trzech do sześciu krótkich \
         punktów, każdy w osobnym wierszu, bez nagłówków i bez formatowania): jak wyszedł czas \
         względem celu i spotkań, jak plan wypadł względem wykonania, które zadania pochłonęły \
         najwięcej czasu albo przekroczyły szacunek (\"estimateMinutes\" to szacunek całego \
         zadania) i co warto zmienić w przyszłym tygodniu. Opieraj się wyłącznie na danych, \
         nie wymyślaj faktów ani liczb. Odpowiedz wyłącznie obiektem JSON {{\"notes\": \"…\"}}.\n\nDane:\n{}",
        week_json(input)
    )
}

/// Szkic wniosków.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeekDraft {
    pub week: String,
    pub text: String,
    /// Powód braku szkicu (pusty, gdy jest).
    pub message: String,
}

pub fn parse_week(value: &Value) -> String {
    value
        .get("notes")
        .and_then(Value::as_str)
        .map(|t| clip(t, DRAFT_MAX))
        .unwrap_or_default()
}

/// Szkic na dowolnym `ask`. Tydzień bez danych nie woła modelu.
pub fn draft(
    ask: &dyn Fn(&str, &str, &Value) -> Result<Value, String>,
    input: &WeekInput,
) -> Result<WeekDraft, String> {
    if input.is_empty() {
        return Ok(WeekDraft {
            week: input.week.clone(),
            text: String::new(),
            message: EMPTY_WEEK.into(),
        });
    }
    let answer = ask(WEEK_KIND, &week_prompt(input), &week_schema())?;
    let text = parse_week(&answer);
    let message = if text.is_empty() {
        NOTHING_WEEK.to_string()
    } else {
        String::new()
    };
    Ok(WeekDraft {
        week: input.week.clone(),
        text,
        message,
    })
}

// ---------------------------------------------------------------------------
// Integracja z aplikacją

/// Sprawdza przełącznik i silnik.
fn gate(settings: &AppSettings) -> Result<(), String> {
    if !settings.ai_plan_assist {
        return Err(OFF_MESSAGE.to_string());
    }
    if ai::Engine::parse(&settings.ai_engine).is_none() {
        return Err(ai::OFF.to_string());
    }
    Ok(())
}

fn morning_request(
    app: &tauri::AppHandle,
    day: &str,
    correction: Option<&EstimateCorrection>,
) -> Result<(MorningInput, AppSettings), String> {
    timesheet::check_date(day)?;
    let settings = crate::settings::current(app);
    gate(&settings)?;
    let date = NaiveDate::parse_from_str(day, "%Y-%m-%d").map_err(|e| e.to_string())?;
    let database = app.state::<crate::Database>();
    let conn = database.0.lock().map_err(|e| e.to_string())?;
    // Poprawka tylko przy włączonym „Planuj z poprawką szacunków”, jak w widoku.
    let correction = correction.filter(|_| settings.plan_with_correction);
    let input = collect_morning_with(
        &conn,
        &chrono::Local,
        &settings,
        date,
        db::now(),
        correction,
    )?;
    Ok((input, settings))
}

fn week_request(app: &tauri::AppHandle, week: &str) -> Result<(WeekInput, AppSettings), String> {
    if !week_notes::valid_week(week) {
        return Err("Tydzień musi być poniedziałkiem w formacie RRRR-MM-DD.".into());
    }
    let settings = crate::settings::current(app);
    gate(&settings)?;
    let date = NaiveDate::parse_from_str(week, "%Y-%m-%d").map_err(|e| e.to_string())?;
    debug_assert_eq!(date.weekday(), chrono::Weekday::Mon);
    let database = app.state::<crate::Database>();
    let conn = database.0.lock().map_err(|e| e.to_string())?;
    let input = collect_week(&conn, &chrono::Local, &settings, date, db::now())?;
    Ok((input, settings))
}

/// „Komentarz asystenta” w Rano: jedno wywołanie modelu (poziom „główny”) na
/// kliknięcie. Niczego nie zmienia w planie.
#[tauri::command]
pub async fn ai_morning_comment(
    app: tauri::AppHandle,
    day: String,
    correction: Option<EstimateCorrection>,
) -> Result<MorningComment, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, _) = morning_request(&app, &day, correction.as_ref())?;
        comment(
            &|kind, prompt, schema| ai::ask(&app, kind, Tier::Main, prompt, schema),
            &input,
            &chrono::Local,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// „Pokaż, co zostanie wysłane” dla komentarza w Rano: treść po filtrze osobowym.
#[tauri::command]
pub async fn ai_morning_preview(
    app: tauri::AppHandle,
    day: String,
    correction: Option<EstimateCorrection>,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, settings) = morning_request(&app, &day, correction.as_ref())?;
        let dict = ai::dictionary(&app, &settings);
        Ok(crate::ai_privacy::redact(&morning_prompt(&input, &chrono::Local), &dict).0)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// „Szkic wniosków” w przeglądzie tygodnia: jedno wywołanie modelu (poziom
/// „główny”) na kliknięcie. Niczego nie zapisuje.
#[tauri::command]
pub async fn ai_week_draft(app: tauri::AppHandle, week: String) -> Result<WeekDraft, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, _) = week_request(&app, &week)?;
        draft(
            &|kind, prompt, schema| ai::ask(&app, kind, Tier::Main, prompt, schema),
            &input,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// „Pokaż, co zostanie wysłane” dla szkicu wniosków.
#[tauri::command]
pub async fn ai_week_preview(app: tauri::AppHandle, week: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (input, settings) = week_request(&app, &week)?;
        let dict = ai::dictionary(&app, &settings);
        Ok(crate::ai_privacy::redact(&week_prompt(&input), &dict).0)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests;
