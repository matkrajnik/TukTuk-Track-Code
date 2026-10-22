//! Testy A6: zebranie danych Rano i tygodnia (baza w pamięci), schematy i
//! mapowanie odpowiedzi modelu, brak wywołania dla pustych danych i szczelność
//! filtra osobowego. Żaden test nie uruchamia claude ani codex: `ai::run`
//! działa na podstawionym `Runner`.
use super::*;
use crate::ai::{Cli, Config, Detected, Output, RunError, Runner, Spec, State as AiState};
use crate::ai_privacy::Dictionary;
use chrono::TimeZone;
use chrono_tz::Europe::Warsaw;
use std::sync::Mutex;
use std::time::Duration as StdDuration;

const DAY: &str = "2026-10-15";
const WEEK: &str = "2026-10-12";

fn at(s: &str) -> i64 {
    Warsaw
        .from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis()
}

fn now() -> i64 {
    at("2026-10-15 08:30")
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn settings() -> AppSettings {
    AppSettings::default()
}

/// Dzień pełny danych (czwartek 15.10): plan z kolejnością i priorytetem, zadanie
/// ukończone, wzorzec cyklu i archiwalne (poza planem), terminy bliski i daleki,
/// plan na piątek, spotkanie z nazwiskiem, całodniowe „poza biurem” w poniedziałek
/// 19.10 oraz pomiary i notatki z tygodnia.
fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO tasks(id,title,done,created_at) VALUES
           (1,'Eksport CSV',0,1),(2,'Code review',0,2),(3,'Rozmowa z Janem Kowalskim',0,3),
           (4,'Gotowe',1,4),(5,'Cykl tygodniowy',0,5),(6,'Archiwum',0,6),
           (7,'Termin za tydzień',0,7),(8,'Termin za miesiąc',0,8),(9,'Plan na piątek',0,9);
         INSERT INTO task_lifecycle(task_id,archived) VALUES(6,1);
         INSERT INTO task_details(task_id,json) VALUES
           (1,'{{\"project\":\"WMS\",\"plannedDay\":\"{DAY}\",\"estimateMinutes\":240,\"dueDay\":\"2026-10-16\",\"order\":2,\"notes\":{{\"2026-10-13\":\"Analiza importu\",\"2026-10-20\":\"Poza tygodniem\"}}}}'),
           (2,'{{\"project\":\"API\",\"plannedDay\":\"{DAY}\",\"estimateMinutes\":180,\"order\":1}}'),
           (3,'{{\"plannedDay\":\"{DAY}\",\"estimateMinutes\":120,\"priority\":1,\"order\":3}}'),
           (4,'{{\"plannedDay\":\"{DAY}\",\"estimateMinutes\":30}}'),
           (5,'{{\"plannedDay\":\"{DAY}\",\"repeatDays\":[1]}}'),
           (6,'{{\"plannedDay\":\"{DAY}\",\"estimateMinutes\":60}}'),
           (7,'{{\"dueDay\":\"2026-10-20\",\"estimateMinutes\":60}}'),
           (8,'{{\"dueDay\":\"2026-11-30\"}}'),
           (9,'{{\"plannedDay\":\"2026-10-16\",\"estimateMinutes\":90}}');
         INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'https://x.atlassian.net','WMS-12');
         INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,created_at,updated_at) VALUES
           ('manual:1',{m1},{m2},'Spotkanie z Janem Kowalskim','busy',0,0,0),
           ('manual:2',{o1},{o2},'Urlop','oof',1,0,0);
         INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES
           ('claude','C:\\Projekty\\api',1,{a1},{a2}),('codex','C:\\Projekty\\narzedzia',NULL,{a3},{a4});",
        m1 = at("2026-10-15 10:00"),
        m2 = at("2026-10-15 11:00"),
        o1 = at("2026-10-19 00:00"),
        o2 = at("2026-10-20 00:00"),
        a1 = at("2026-10-13 08:00"),
        a2 = at("2026-10-13 09:00"),
        a3 = at("2026-10-14 08:00"),
        a4 = at("2026-10-14 08:30"),
    ))
    .unwrap();
    for (task, from, to) in [
        (1, "2026-10-13 08:00", "2026-10-13 10:00"),
        (2, "2026-10-13 10:00", "2026-10-13 11:00"),
        (1, "2026-10-14 08:00", "2026-10-14 09:00"),
        (1, "2026-10-15 08:00", "2026-10-15 08:20"),
        // Poza tygodniem.
        (2, "2026-10-20 08:00", "2026-10-20 09:00"),
    ] {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            params![task, at(from), at(to)],
        )
        .unwrap();
    }
    db
}

fn morning(settings: &AppSettings) -> MorningInput {
    collect_morning(&database(), &Warsaw, settings, date(DAY), now()).unwrap()
}

fn week(settings: &AppSettings) -> WeekInput {
    collect_week(&database(), &Warsaw, settings, date(WEEK), now()).unwrap()
}

// ---------------------------------------------------------------------------
// Rano: dane

#[test]
fn plan_follows_the_day_plan_order_without_recurring_archived_and_other_days() {
    let input = morning(&settings());
    let ids: Vec<i64> = input.plan.iter().map(|t| t.id).collect();
    // Priorytet, potem ręczna kolejność; ukończone na końcu; bez cyklu (5),
    // archiwalnego (6) i zadań z innych dni.
    assert_eq!(ids, vec![3, 2, 1, 4]);
    assert_eq!(input.plan[2].key, "WMS-12");
    assert_eq!(input.plan[2].project, "WMS");
    assert_eq!(input.plan[2].estimate_minutes, 240);
    assert_eq!(input.plan[2].due_day, "2026-10-16");
    assert!(input.plan[3].done);
}

#[test]
fn deadlines_are_open_tasks_outside_the_plan_within_two_weeks() {
    let input = morning(&settings());
    let ids: Vec<i64> = input.deadlines.iter().map(|t| t.id).collect();
    // Zadanie 7 (20.10); zadanie 8 (30.11) jest poza oknem, zadanie 1 jest w planie.
    assert_eq!(ids, vec![7]);
    assert_eq!(input.deadlines[0].due_day, "2026-10-20");
}

#[test]
fn capacity_is_goal_minus_meetings_and_tracked_time_is_counted() {
    let input = morning(&settings());
    assert_eq!(input.goal_ms, 480 * 60_000);
    assert_eq!(input.meetings_ms, 60 * 60_000);
    assert_eq!(input.capacity_ms, 420 * 60_000);
    assert!(!input.off);
    assert_eq!(input.tracked_ms, 20 * 60_000);
}

#[test]
fn meeting_titles_leave_only_when_the_switch_is_on() {
    assert_eq!(morning(&settings()).meetings[0].title, None);
    let mut on = settings();
    on.ai_send_meeting_titles = true;
    assert_eq!(
        morning(&on).meetings[0].title.as_deref(),
        Some("Spotkanie z Janem Kowalskim")
    );
}

#[test]
fn next_days_are_work_days_without_weekend_and_days_off_with_planned_minutes() {
    let input = morning(&settings());
    let days: Vec<&str> = input.upcoming.iter().map(|d| d.day.as_str()).collect();
    // 17–18.10 to weekend, 19.10 to całodniowe „poza biurem”.
    assert_eq!(
        days,
        vec![
            "2026-10-16",
            "2026-10-20",
            "2026-10-21",
            "2026-10-22",
            "2026-10-23"
        ]
    );
    assert_eq!(input.upcoming[0].planned_minutes, 90);
    assert_eq!(input.upcoming[0].capacity_ms, 480 * 60_000);
    assert_eq!(input.upcoming[1].planned_minutes, 0);
}

#[test]
fn a_day_off_and_a_weekend_have_no_capacity() {
    let db = database();
    let off = collect_morning(&db, &Warsaw, &settings(), date("2026-10-19"), now()).unwrap();
    assert!(off.off);
    assert_eq!(off.capacity_ms, 0);
    let sunday = collect_morning(&db, &Warsaw, &settings(), date("2026-10-18"), now()).unwrap();
    assert_eq!(sunday.goal_ms, 0);
    assert_eq!(sunday.capacity_ms, 0);
}

#[test]
fn model_input_has_minutes_hours_plan_deadlines_and_next_days_without_titles_of_meetings() {
    let input = morning(&settings());
    let json = morning_json(&input, &Warsaw);
    assert_eq!(json["capacityMinutes"], 420);
    assert_eq!(json["meetingsMinutes"], 60);
    assert_eq!(json["meetings"], json!([{"from": "10:00", "to": "11:00"}]));
    assert_eq!(json["plan"][2]["taskId"], 1);
    assert_eq!(json["plan"][2]["estimateMinutes"], 240);
    assert_eq!(json["plan"][3]["done"], true);
    assert_eq!(json["deadlines"][0]["taskId"], 7);
    assert_eq!(json["deadlines"][0]["dueDay"], "2026-10-20");
    assert_eq!(json["nextDays"].as_array().unwrap().len(), 5);
    let text = json.to_string();
    assert!(!text.contains("Spotkanie z Janem"), "{text}");
}

// ---------------------------------------------------------------------------
// Rano: odpowiedź modelu

fn mv(task: i64, to: &str, reason: &str) -> Value {
    json!({"taskId": task, "toDay": to, "reason": reason})
}

#[test]
fn a_move_keeps_only_open_plan_tasks_and_listed_days_not_after_the_due_day() {
    let input = morning(&settings());
    let answer = json!({
        "comment": "  Plan jest za duży.  ",
        "moves": [
            mv(2, "2026-10-16", "Code review może poczekać."),
            // Zadanie spoza planu, ukończone, nieznane, pusta wartość.
            mv(7, "2026-10-16", "spoza planu"),
            mv(4, "2026-10-16", "ukończone"),
            mv(99, "2026-10-16", "nieznane"),
            json!({"taskId": null, "toDay": "2026-10-16", "reason": "x"}),
            // Dzień spoza listy (weekend, dzień wolny, przeszłość, śmieci).
            mv(3, "2026-10-17", "weekend"),
            mv(3, "2026-10-19", "poza biurem"),
            mv(3, "2026-10-14", "przeszłość"),
            mv(3, "jutro", "śmieci"),
            // Po terminie zadania 1 (16.10).
            mv(1, "2026-10-20", "po terminie"),
            // To samo zadanie drugi raz.
            mv(2, "2026-10-20", "drugi raz"),
            mv(1, "2026-10-16", "w terminie"),
        ],
    });
    let (comment, moves) = parse_morning(&answer, &input);
    assert_eq!(comment, "Plan jest za duży.");
    let got: Vec<(i64, &str, &str)> = moves
        .iter()
        .map(|m| (m.task_id, m.to_day.as_str(), m.reason.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (2, "2026-10-16", "Code review może poczekać."),
            (1, "2026-10-16", "w terminie"),
        ]
    );
    assert_eq!(moves[0].task_title, "Code review");
    assert_eq!(moves[0].from_day, DAY);
    // Numer karty to pozycja w odpowiedzi (stabilny klucz).
    assert_eq!(moves[1].id, 11);
}

#[test]
fn text_is_clipped_and_garbage_does_not_break_the_result() {
    let input = morning(&settings());
    let long = "ą".repeat(COMMENT_MAX + 50);
    let (comment, moves) = parse_morning(
        &json!({"comment": long, "moves": [mv(2, "2026-10-16", &"b".repeat(500))]}),
        &input,
    );
    assert_eq!(comment.chars().count(), COMMENT_MAX);
    assert_eq!(moves[0].reason.chars().count(), REASON_MAX);
    assert_eq!(
        parse_morning(&json!("tekst"), &input),
        (String::new(), vec![])
    );
    assert_eq!(
        parse_morning(&json!({"comment": 5, "moves": "x"}), &input),
        (String::new(), vec![])
    );
}

#[test]
fn schemas_require_the_fields_and_forbid_extras() {
    let m = morning_schema();
    assert_eq!(m["required"], json!(["comment", "moves"]));
    assert_eq!(m["additionalProperties"], false);
    assert_eq!(m["properties"]["moves"]["maxItems"], MOVES_MAX);
    assert_eq!(
        m["properties"]["moves"]["items"]["required"],
        json!(["taskId", "toDay", "reason"])
    );
    let w = week_schema();
    assert_eq!(w["required"], json!(["notes"]));
    assert_eq!(w["additionalProperties"], false);
}

// ---------------------------------------------------------------------------
// Wywołanie: `ai::run` na podstawionym `Runner`

fn claude_reply(structured: Value) -> String {
    json!({
        "type": "result", "subtype": "success", "is_error": false, "result": "",
        "structured_output": structured
    })
    .to_string()
}

struct Fake {
    text: String,
    specs: Mutex<Vec<Spec>>,
}
impl Fake {
    fn answering(structured: Value) -> Self {
        Self {
            text: claude_reply(structured),
            specs: Mutex::new(Vec::new()),
        }
    }
    fn count(&self) -> usize {
        self.specs.lock().unwrap().len()
    }
    fn stdin(&self) -> String {
        self.specs.lock().unwrap().last().unwrap().stdin.clone()
    }
}
impl Runner for Fake {
    fn run(&self, spec: &Spec, _timeout: StdDuration) -> Result<Output, RunError> {
        self.specs.lock().unwrap().push(spec.clone());
        Ok(Output {
            success: true,
            stdout: self.text.clone(),
            stderr: String::new(),
        })
    }
}

fn cfg() -> Config {
    Config {
        engine: "claude".into(),
        detected: Detected {
            claude: Some(Cli {
                path: r"C:\bin\claude.exe".into(),
                version: "2.1.304".into(),
            }),
            codex: None,
        },
        model: String::new(),
        dir: std::env::temp_dir().join(format!("tt-plan-assist-ai-{}", std::process::id())),
    }
}

#[test]
fn comment_calls_the_model_once_without_names_and_returns_the_real_name_back() {
    let mut on = settings();
    on.ai_send_meeting_titles = true;
    let input = morning(&on);
    let runner = Fake::answering(json!({
        "comment": "Rozmowa z Osoba 1 zajmie pół dnia.",
        "moves": [mv(3, "2026-10-16", "Rozmowa z Osoba 1 może być jutro.")],
    }));
    let mut dict = Dictionary::new();
    dict.add_name("Jan Kowalski");
    let state = AiState::default();
    let config = cfg();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    let result = comment(&ask, &input, &Warsaw).unwrap();
    assert_eq!(runner.count(), 1);
    // Szczelność (A0): ani tytuł spotkania, ani tytuł zadania z nazwiskiem w odmianie.
    let sent = runner.stdin();
    assert!(
        !sent.contains("Kowalski") && !sent.contains("Kowalskim"),
        "{sent}"
    );
    assert!(sent.contains("Osoba 1"));
    assert_eq!(result.day, DAY);
    assert_eq!(result.message, "");
    assert_eq!(result.comment, "Rozmowa z Jan Kowalski zajmie pół dnia.");
    assert_eq!(result.moves.len(), 1);
    assert_eq!(
        result.moves[0].reason,
        "Rozmowa z Jan Kowalski może być jutro."
    );
    assert_eq!(result.moves[0].task_title, "Rozmowa z Janem Kowalskim");
    let calls = state.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].kind, MORNING_KIND);
}

#[test]
fn a_day_without_plan_and_deadlines_does_not_call_the_model() {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    let input = collect_morning(&db, &Warsaw, &settings(), date(DAY), now()).unwrap();
    assert!(input.is_empty());
    let runner = Fake::answering(json!({"comment": "x", "moves": []}));
    let state = AiState::default();
    let config = cfg();
    let dict = Dictionary::new();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    let result = comment(&ask, &input, &Warsaw).unwrap();
    assert_eq!(runner.count(), 0);
    assert_eq!(result.message, EMPTY_MORNING);
    assert!(result.comment.is_empty() && result.moves.is_empty());
}

#[test]
fn nothing_to_say_and_errors_reach_the_interface() {
    let input = morning(&settings());
    let empty = |_: &str, _: &str, _: &Value| Ok(json!({"comment": "", "moves": []}));
    assert_eq!(
        comment(&empty, &input, &Warsaw).unwrap().message,
        NOTHING_MORNING
    );
    // Sam komentarz bez przesunięć jest poprawnym wynikiem.
    let only =
        |_: &str, _: &str, _: &Value| Ok(json!({"comment": "Plan się mieści.", "moves": []}));
    let r = comment(&only, &input, &Warsaw).unwrap();
    assert_eq!(r.message, "");
    assert_eq!(r.comment, "Plan się mieści.");
    let failing =
        |_: &str, _: &str, _: &Value| -> Result<Value, String> { Err("Limit subskrypcji".into()) };
    assert_eq!(
        comment(&failing, &input, &Warsaw).unwrap_err(),
        "Limit subskrypcji"
    );
}

#[test]
fn morning_comment_serializes_for_the_interface_in_camel_case() {
    let input = morning(&settings());
    let (comment, moves) = parse_morning(
        &json!({"comment": "Plan.", "moves": [mv(2, "2026-10-16", "Powód.")]}),
        &input,
    );
    let value = serde_json::to_value(&MorningComment {
        day: DAY.into(),
        comment,
        moves,
        message: String::new(),
    })
    .unwrap();
    assert_eq!(
        value["moves"][0],
        json!({
            "id": 0, "taskId": 2, "taskTitle": "Code review",
            "fromDay": DAY, "toDay": "2026-10-16", "reason": "Powód."
        })
    );
}

#[test]
fn calling_the_assistant_does_not_change_the_database() {
    let db = database();
    let dump = |db: &Connection| -> Vec<String> {
        let mut stmt = db
            .prepare("SELECT task_id || json FROM task_details ORDER BY task_id")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        let mut out: Vec<String> = rows.map(|r| r.unwrap()).collect();
        let notes: i64 = db
            .query_row("SELECT COUNT(*) FROM week_notes", [], |r| r.get(0))
            .unwrap();
        out.push(notes.to_string());
        out
    };
    let before = dump(&db);
    let input = collect_morning(&db, &Warsaw, &settings(), date(DAY), now()).unwrap();
    let ask = |_: &str, _: &str, _: &Value| {
        Ok(json!({"comment": "Plan.", "moves": [mv(2, "2026-10-16", "x")], "notes": "Wnioski."}))
    };
    comment(&ask, &input, &Warsaw).unwrap();
    let week_input = collect_week(&db, &Warsaw, &settings(), date(WEEK), now()).unwrap();
    draft(&ask, &week_input).unwrap();
    assert_eq!(dump(&db), before);
}

// ---------------------------------------------------------------------------
// Przegląd tygodnia

#[test]
fn week_collects_days_task_time_notes_plan_and_agents() {
    let input = week(&settings());
    assert_eq!(input.week, WEEK);
    assert_eq!(input.days.len(), 7);
    assert_eq!(input.days[0].day, "2026-10-12");
    assert_eq!(input.days[6].day, "2026-10-18");
    assert_eq!(input.days[1].tracked_ms, 3 * 3_600_000);
    assert_eq!(input.days[2].tracked_ms, 3_600_000);
    assert_eq!(input.days[3].tracked_ms, 20 * 60_000);
    assert_eq!(input.days[3].meetings_ms, 60 * 60_000);
    assert_eq!(input.days[3].capacity_ms, 420 * 60_000);
    assert_eq!(input.days[5].goal_ms, 0);
    // Czas zadań malejąco, bez pomiaru z innego tygodnia; notatka tylko z tygodnia.
    let ids: Vec<i64> = input.tasks.iter().map(|t| t.id).collect();
    assert_eq!(ids, vec![1, 2]);
    assert_eq!(
        input.tasks[0].tracked_ms,
        2 * 3_600_000 + 3_600_000 + 20 * 60_000
    );
    assert_eq!(
        input.tasks[0].notes,
        vec![("2026-10-13".to_string(), "Analiza importu".to_string())]
    );
    // Plan 15.10: zadania 1, 2, 3 otwarte i 4 ukończone (cykl i archiwalne pominięte),
    // plan 16.10: zadanie 9.
    assert_eq!(input.planned, 5);
    assert_eq!(input.planned_done, 1);
    assert_eq!(input.planned_open, 4);
    assert_eq!(input.agent_ms, 90 * 60_000);
    assert_eq!(input.agent_unmatched_ms, 30 * 60_000);
}

#[test]
fn week_model_input_has_minutes_and_no_titles_of_meetings_or_paths() {
    let json = week_json(&week(&settings()));
    assert_eq!(json["week"], WEEK);
    assert_eq!(json["days"][3]["trackedMinutes"], 20);
    assert_eq!(json["days"][3]["meetingsMinutes"], 60);
    assert_eq!(json["days"][5]["goalMinutes"], 0);
    assert_eq!(json["plan"], json!({"planned": 5, "done": 1, "open": 4}));
    assert_eq!(json["tasks"][0]["taskId"], 1);
    assert_eq!(json["tasks"][0]["trackedMinutes"], 200);
    assert_eq!(json["tasks"][0]["estimateMinutes"], 240);
    assert_eq!(json["tasks"][0]["notes"][0]["note"], "Analiza importu");
    assert_eq!(
        json["agentTime"],
        json!({"totalMinutes": 90, "unassignedMinutes": 30})
    );
    let text = json.to_string();
    assert!(
        !text.contains("narzedzia") && !text.contains("Spotkanie z Janem"),
        "{text}"
    );
}

#[test]
fn an_empty_week_does_not_call_the_model() {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    let input = collect_week(&db, &Warsaw, &settings(), date(WEEK), now()).unwrap();
    assert!(input.is_empty());
    let runner = Fake::answering(json!({"notes": "x"}));
    let state = AiState::default();
    let config = cfg();
    let dict = Dictionary::new();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    let result = draft(&ask, &input).unwrap();
    assert_eq!(runner.count(), 0);
    assert_eq!(result.message, EMPTY_WEEK);
    assert_eq!(result.text, "");
}

#[test]
fn draft_calls_the_model_once_without_names_and_restores_them_in_the_text() {
    let input = week(&settings());
    let runner = Fake::answering(json!({
        "notes": "  - Rozmowa z Osoba 1 zabrała czas.\n- Mniej przełączeń.  "
    }));
    let mut dict = Dictionary::new();
    dict.add_name("Jan Kowalski");
    let state = AiState::default();
    let config = cfg();
    let ask = |kind: &str, prompt: &str, schema: &Value| {
        ai::run(&state, &runner, &config, kind, prompt, schema, &dict, now())
    };
    // Tytuł i notatka z nazwiskiem w danych tygodnia też są filtrowane.
    let mut with_name = input.clone();
    with_name.tasks[0].title = "Spotkanie z Janem Kowalskim".into();
    with_name.tasks[0].notes[0].1 = "Rozmowa z Kowalskim o imporcie".into();
    let result = draft(&ask, &with_name).unwrap();
    assert_eq!(runner.count(), 1);
    let sent = runner.stdin();
    assert!(!sent.contains("Kowalsk"), "{sent}");
    assert!(sent.contains("Osoba 1"));
    assert_eq!(result.week, WEEK);
    assert_eq!(result.message, "");
    assert_eq!(
        result.text,
        "- Rozmowa z Jan Kowalski zabrała czas.\n- Mniej przełączeń."
    );
    assert_eq!(state.calls()[0].kind, WEEK_KIND);
}

#[test]
fn draft_is_clipped_and_an_empty_answer_or_error_is_reported() {
    let input = week(&settings());
    let long = "x".repeat(DRAFT_MAX + 10);
    let ask_long = |_: &str, _: &str, _: &Value| Ok(json!({"notes": long.clone()}));
    assert_eq!(
        draft(&ask_long, &input).unwrap().text.chars().count(),
        DRAFT_MAX
    );
    let ask_empty = |_: &str, _: &str, _: &Value| Ok(json!({"notes": "  "}));
    let r = draft(&ask_empty, &input).unwrap();
    assert_eq!(r.text, "");
    assert_eq!(r.message, NOTHING_WEEK);
    let ask_error =
        |_: &str, _: &str, _: &Value| -> Result<Value, String> { Err("Przekroczono czas".into()) };
    assert_eq!(draft(&ask_error, &input).unwrap_err(), "Przekroczono czas");
}

#[test]
fn week_draft_serializes_for_the_interface_in_camel_case() {
    let value = serde_json::to_value(&WeekDraft {
        week: WEEK.into(),
        text: "Wnioski.".into(),
        message: String::new(),
    })
    .unwrap();
    assert_eq!(
        value,
        json!({"week": WEEK, "text": "Wnioski.", "message": ""})
    );
}

// ---------------------------------------------------------------------------
// Przełącznik

#[test]
fn the_switch_and_the_engine_gate_both_functions() {
    let mut s = settings();
    // Domyślnie przełącznik włączony, ale bez wybranego silnika nic nie działa.
    assert!(s.ai_plan_assist);
    assert_eq!(gate(&s).unwrap_err(), ai::OFF);
    s.ai_engine = "claude".into();
    assert!(gate(&s).is_ok());
    s.ai_engine = "off".into();
    assert_eq!(gate(&s).unwrap_err(), ai::OFF);
    s.ai_engine = "codex".into();
    s.ai_plan_assist = false;
    assert_eq!(gate(&s).unwrap_err(), OFF_MESSAGE);
}

#[test]
fn prompts_describe_the_task_and_carry_the_data() {
    let input = morning(&settings());
    let prompt = morning_prompt(&input, &Warsaw);
    assert!(prompt.contains("\"comment\"") && prompt.contains("nextDays"));
    assert!(prompt.contains("\"taskId\":1"));
    let w = week_prompt(&week(&settings()));
    assert!(w.contains("„Wnioski”") && w.contains("\"week\":\"2026-10-12\""));
}
