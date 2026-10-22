//! Testy A2: protokół MCP na zapisanych wymianach (initialize, list, call, błąd,
//! aplikacja nie działa), każde narzędzie odczytu na bazie w pamięci, szczelność
//! wyjścia filtra osobowego (jak w A0), propozycje trafiające do kolejki bez zmian
//! w bazie oraz bramka `ai_mcp`. Nic nie uruchamia claude ani codex i nie rusza
//! plików użytkownika.
use super::*;
use crate::mcp_wire::{self, Reply};
use chrono::TimeZone;
use chrono_tz::Europe::Warsaw;
use chrono_tz::Tz;
use serde_json::json;

const EXCHANGE: &str = include_str!("../../tests/fixtures/mcp_exchange.json");
const EXCHANGE_ERRORS: &str = include_str!("../../tests/fixtures/mcp_exchange_errors.json");
const EXCHANGE_DOWN: &str = include_str!("../../tests/fixtures/mcp_exchange_down.json");

const SITE: &str = "https://x.atlassian.net";

fn at(s: &str) -> i64 {
    Warsaw
        .from_local_datetime(&chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap())
        .earliest()
        .unwrap()
        .timestamp_millis()
}

fn now() -> i64 {
    at("2026-10-15 12:00")
}

/// Baza z dniem pełnym danych: zadania z Jiry i Redmine, pomiary (w tym trwający),
/// ręczne spotkanie z nazwiskiem, czas agentów i pole osobowe Jiry.
fn db() -> Connection {
    let mut db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    db.execute_batch(&format!(
        "INSERT INTO tasks(id,title,done,created_at) VALUES
           (1,'Analiza importu',0,1),(2,'Code review',0,2),(3,'Raport miesięczny',1,3),
           (4,'Import CSV',0,4),(5,'Usunięte zadanie',0,5);
         INSERT INTO task_lifecycle(task_id,deleted_at) VALUES(5,10);
         INSERT INTO task_details(task_id,json) VALUES
           (1,'{{\"project\":\"WMS\",\"plannedDay\":\"2026-10-15\",\"estimateMinutes\":120,\"priority\":1,\"notes\":{{\"2026-10-15\":\"Rozmowa z Janem Kowalskim o imporcie\",\"2026-10-14\":\"Wcześniej\"}},\"checklist\":[{{\"text\":\"Parser\",\"done\":true}}]}}'),
           (2,'{{\"project\":\"API\",\"dueDay\":\"2026-10-16\",\"waiting\":\"Czekam na odpowiedź Anny Nowak\"}}'),
           (4,'{{\"project\":\"WMS\"}}');
         INSERT INTO jira_tasks(task_id,site,issue_key) VALUES(1,'{SITE}','WMS-12');
         INSERT INTO jira_issues(site,issue_key,status,assigned) VALUES('{SITE}','WMS-12','In Progress',1);
         INSERT INTO redmine_tasks(task_id,site,issue_id) VALUES(4,'https://redmine.example.pl',20);
         INSERT INTO redmine_issues(site,issue_id,subject,status) VALUES('https://redmine.example.pl',20,'Import CSV','Nowe');
         INSERT INTO jira_fields(site,id,name,custom,kind) VALUES('{SITE}','customfield_10050','Odpowiedzialny',1,'user');
         INSERT INTO jira_issue_fields(site,issue_key,field_id,value) VALUES('{SITE}','WMS-12','customfield_10050','Anna Nowak');
         INSERT INTO agent_sessions(agent,folder,task_id,start,\"end\") VALUES
           ('claude','C:\\Projekty\\wms',1,{a1},{a2}),('codex','C:\\Projekty\\inne',NULL,{a3},{a4});
         INSERT INTO manual_meetings(id,start,\"end\",subject,show_as,all_day,created_at,updated_at)
           VALUES('manual:1',{m1},{m2},'Spotkanie z Janem Kowalskim','busy',0,0,0);",
        a1 = at("2026-10-15 08:30"),
        a2 = at("2026-10-15 09:00"),
        a3 = at("2026-10-15 09:00"),
        a4 = at("2026-10-15 09:20"),
        m1 = at("2026-10-15 10:00"),
        m2 = at("2026-10-15 10:30"),
    ))
    .unwrap();
    for (task, from, to) in [
        (1, "2026-10-13 09:00", "2026-10-13 10:00"),
        (2, "2026-10-14 09:00", "2026-10-14 11:00"),
        (1, "2026-10-15 08:00", "2026-10-15 09:30"),
        (2, "2026-10-15 09:30", "2026-10-15 10:00"),
        // Zadanie z kosza: nie liczy się do niczego.
        (5, "2026-10-15 07:00", "2026-10-15 07:30"),
    ] {
        db.execute(
            "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(?1,?2,?3)",
            rusqlite::params![task, at(from), at(to)],
        )
        .unwrap();
    }
    // Trwający pomiar od 11:00 (do „teraz” 12:00).
    db.execute(
        "INSERT INTO sessions(task_id,started_at,ended_at) VALUES(1,?1,NULL)",
        [at("2026-10-15 11:00")],
    )
    .unwrap();
    crate::settings::save(
        &mut db,
        &json!({"jiraCardFields": ["customfield_10050"], "targetDayMinutes": 480}),
    )
    .unwrap();
    db
}

fn settings(db: &Connection) -> AppSettings {
    let mut s = crate::settings::load(db);
    s.ai_engine = "claude".into();
    s.ai_mcp = true;
    s
}

fn dict(db: &Connection, settings: &AppSettings) -> Dictionary {
    Dictionary::from_db(db, settings, &["Jan Kowalski".to_string()]).unwrap()
}

struct Fixture {
    db: Connection,
    settings: AppSettings,
    folders: Vec<String>,
    dict: Dictionary,
    state: State,
}

impl Fixture {
    fn new() -> Self {
        let db = db();
        let settings = settings(&db);
        let dict = dict(&db, &settings);
        Fixture {
            db,
            settings,
            folders: vec![r"C:\Projekty\wms".into(), r"C:\Starsze".into()],
            dict,
            state: State::default(),
        }
    }
    fn call(&self, tool: &str, args: Value) -> Result<String, String> {
        let env = Env {
            db: &self.db,
            tz: &Warsaw,
            settings: &self.settings,
            now: now(),
            folders: &self.folders,
        };
        call::<Tz>(&env, &self.state, &self.dict, tool, &args)
    }
    fn json(&self, tool: &str, args: Value) -> Value {
        let text = self
            .call(tool, args)
            .unwrap_or_else(|e| panic!("{tool}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|_| panic!("{tool}: nie JSON: {text}"))
    }
}

// ---------------------------------------------------------------------------
// Protokół na zapisanych wymianach

/// Odtwarza plik z wymianą: każdy krok to wiersz od agenta, spodziewana odpowiedź
/// i (przy `tools/call`) zapisana odpowiedź aplikacji.
fn replay(text: &str) {
    let file: Value = serde_json::from_str(text).unwrap();
    let version = file["version"].as_str().unwrap();
    for (n, step) in file["steps"].as_array().unwrap().iter().enumerate() {
        let line = match &step["in"] {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut forwarded: Vec<(String, Value)> = Vec::new();
        let mut forward = |tool: &str, args: &Value| -> Result<Reply, String> {
            forwarded.push((tool.to_string(), args.clone()));
            match &step["forward"]["reply"] {
                Value::String(s) if s == "down" => Err("potok niedostępny".into()),
                reply => Ok(Reply::parse(reply)),
            }
        };
        let out = mcp_wire::respond(&line, version, &mut forward);
        if let Some(names) = step["toolNames"].as_array() {
            let out = out.unwrap_or_else(|| panic!("krok {n}: brak odpowiedzi"));
            let got: Vec<&str> = out["result"]["tools"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["name"].as_str().unwrap())
                .collect();
            let want: Vec<&str> = names.iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(got, want, "krok {n}: lista narzędzi");
            assert_eq!(out["id"], step["in"]["id"]);
            assert_eq!(out["jsonrpc"], "2.0");
        } else {
            assert_eq!(out.unwrap_or(Value::Null), step["out"], "krok {n}: {line}");
        }
        match step.get("forward") {
            Some(f) => assert_eq!(
                forwarded,
                vec![(
                    f["tool"].as_str().unwrap().to_string(),
                    f["arguments"].clone()
                )],
                "krok {n}: przekazanie do aplikacji"
            ),
            None => assert!(
                forwarded.is_empty(),
                "krok {n}: nic nie miało iść do aplikacji"
            ),
        }
    }
}

#[test]
fn recorded_exchange_initialize_list_call_and_error() {
    replay(EXCHANGE);
}

#[test]
fn recorded_exchange_protocol_errors() {
    replay(EXCHANGE_ERRORS);
}

#[test]
fn recorded_exchange_when_the_app_is_not_running() {
    replay(EXCHANGE_DOWN);
}

#[test]
fn batch_requests_get_a_batch_answer() {
    let mut forward = |_: &str, _: &Value| -> Result<Reply, String> { Err("x".into()) };
    let line = r#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"ping"}]"#;
    let out = mcp_wire::respond(line, "1", &mut forward).unwrap();
    assert_eq!(out.as_array().unwrap().len(), 2);
    assert_eq!(out[1]["id"], 2);
}

#[test]
fn tool_definitions_are_english_short_and_have_no_forbidden_tools() {
    let tools = mcp_wire::tools();
    let tools = tools.as_array().unwrap();
    assert_eq!(tools.len(), 11);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let description = tool["description"].as_str().unwrap();
        assert!(description.chars().count() <= 200, "{name}: opis za długi");
        assert!(
            !description
                .chars()
                .any(|c| "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ".contains(c)),
            "{name}: opis po polsku"
        );
        assert_eq!(tool["inputSchema"]["type"], "object");
        if name.starts_with("propose_") {
            assert!(
                description.contains("proposal"),
                "{name}: opis musi mówić o propozycji"
            );
            assert!(description.contains("approve"), "{name}");
        }
        // Bez instrukcji „zawsze wywołuj”, które zaśmiecałyby kontekst sesji.
        let lower = description.to_lowercase();
        assert!(
            !lower.contains("always") && !lower.contains("must call"),
            "{name}"
        );
        // Brak wysyłki TS, zapisów w Jirze i Redmine, usuwania i zmiany ustawień.
        for banned in ["ts_send", "jira", "redmine", "delete", "remove", "setting"] {
            assert!(!name.contains(banned), "{name}");
        }
    }
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for name in names {
        assert!(mcp_wire::is_tool(name));
    }
    assert!(!mcp_wire::is_tool("ts_send"));
}

#[test]
fn pipe_name_is_per_user_and_sanitized() {
    assert_eq!(
        mcp_wire::pipe_name_for("Jan Kowalski"),
        r"\\.\pipe\tuktuk-track-mcp-Jan_Kowalski"
    );
    assert_eq!(
        mcp_wire::pipe_name_for(""),
        r"\\.\pipe\tuktuk-track-mcp-user"
    );
}

#[test]
fn reply_frames_are_parsed() {
    assert_eq!(
        Reply::parse(&json!({"ok": true, "text": "x"})),
        Reply {
            ok: true,
            text: "x".into()
        }
    );
    assert_eq!(
        Reply::parse(&json!({"ok": false, "error": "e"})),
        Reply {
            ok: false,
            text: "e".into()
        }
    );
    let broken = Reply::parse(&json!({"foo": 1}));
    assert!(!broken.ok && !broken.text.is_empty());
}

/// Cała droga programu `tuktuk-track-mcp` do aplikacji przez prawdziwy nazwany potok:
/// ramka z narzędziem i odpowiedź; bez serwera program zgłasza „nie działa”.
#[cfg(windows)]
#[test]
fn named_pipe_round_trip_and_app_not_running() {
    let name = mcp_wire::pipe_name_for(&format!("mcp-test-{}-{}", std::process::id(), db::now()));
    let missing = mcp_wire::call_over_pipe(&name, "tasks_list", &json!({}), None);
    assert!(missing.is_err());
    let mut forward = |tool: &str, args: &Value| mcp_wire::call_over_pipe(&name, tool, args, None);
    let line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"tasks_list","arguments":{}}}"#;
    let out = mcp_wire::respond(line, "1", &mut forward).unwrap();
    assert_eq!(out["result"]["isError"], true);
    assert_eq!(out["result"]["content"][0]["text"], mcp_wire::NOT_RUNNING);

    let server = name.clone();
    std::thread::spawn(move || {
        let _ = crate::browser::pipe::serve(&server, |body| {
            let request: Value = serde_json::from_slice(body).unwrap();
            json!({"ok": true, "text": format!("{}:{}:{}", request["tool"], request["arguments"]["filter"], request["cwd"])})
        });
    });
    let mut reply = Err("nie połączono".to_string());
    for _ in 0..50 {
        reply = mcp_wire::call_over_pipe(
            &name,
            "tasks_list",
            &json!({"filter": "today"}),
            Some(r"C:\Projekty\wms"),
        );
        if reply.is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    assert_eq!(
        reply.unwrap(),
        Reply {
            ok: true,
            text: r#""tasks_list":"today":"C:\\Projekty\\wms""#.into()
        }
    );
}

// ---------------------------------------------------------------------------
// Narzędzia odczytu na bazie w pamięci

fn ids(value: &Value) -> Vec<i64> {
    value["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_i64().unwrap())
        .collect()
}

#[test]
fn tasks_list_filters() {
    let f = Fixture::new();
    // Domyślnie: otwarte, bez ukończonych i bez kosza.
    let open = f.json("tasks_list", json!({}));
    let mut open_ids = ids(&open);
    open_ids.sort();
    assert_eq!(open_ids, vec![1, 2, 4]);
    assert_eq!(open["total"], 3);
    assert_eq!(open["truncated"], false);
    // Klucz Jiry i numer Redmine są w wyniku.
    let task1 = open["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == 1)
        .unwrap();
    assert_eq!(task1["key"], "WMS-12");
    assert_eq!(task1["status"], "In Progress");
    assert_eq!(task1["estimateMinutes"], 120);
    assert!(open["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["key"] == "#20"));
    // Dzisiaj: zaplanowane na dziś albo mierzone dziś (zadania 1 i 2), bez ukończonych.
    let mut today = ids(&f.json("tasks_list", json!({"filter": "today"})));
    today.sort();
    assert_eq!(today, vec![1, 2]);
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "dzisiaj"}))).len(),
        2
    );
    // Zablokowane.
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "blocked"}))),
        vec![2]
    );
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "zablokowane"}))),
        vec![2]
    );
    // Projekt (bez rozróżniania wielkości liter) i szukanie po tytule, kluczu i notatce.
    let mut wms = ids(&f.json("tasks_list", json!({"filter": "project", "query": "wms"})));
    wms.sort();
    assert_eq!(wms, vec![1, 4]);
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "projekt", "query": "API"}))),
        vec![2]
    );
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "search", "query": "wms-12"}))),
        vec![1]
    );
    assert_eq!(
        ids(&f.json("tasks_list", json!({"filter": "szukaj", "query": "csv"}))),
        vec![4]
    );
    // Notatki dnia też są przeszukiwane (słowo spoza słownika imion).
    assert_eq!(
        ids(&f.json(
            "tasks_list",
            json!({"filter": "search", "query": "imporcie"})
        )),
        vec![1]
    );
    // Błędy.
    assert!(f
        .call("tasks_list", json!({"filter": "wszystko"}))
        .unwrap_err()
        .contains("Unknown filter"));
    assert!(f
        .call("tasks_list", json!({"filter": "search"}))
        .unwrap_err()
        .contains("needs a query"));
    assert!(f
        .call("tasks_list", json!({"filter": "project", "query": " "}))
        .is_err());
}

#[test]
fn tasks_list_truncates_long_results() {
    let f = Fixture::new();
    for i in 0..60 {
        f.db.execute(
            "INSERT INTO tasks(title,created_at) VALUES(?1,0)",
            [format!("Zadanie {i}")],
        )
        .unwrap();
    }
    let list = f.json("tasks_list", json!({}));
    assert_eq!(list["tasks"].as_array().unwrap().len(), 50);
    assert_eq!(list["total"], 63);
    assert_eq!(list["truncated"], true);
}

#[test]
fn task_get_by_id_key_redmine_number_and_title() {
    let f = Fixture::new();
    let by_id = f.json("task_get", json!({"id": 1}));
    assert_eq!(by_id["title"], "Analiza importu");
    assert_eq!(by_id["key"], "WMS-12");
    assert_eq!(by_id["plannedDay"], "2026-10-15");
    assert_eq!(by_id["checklist"][0]["text"], "Parser");
    assert_eq!(by_id["notes"]["2026-10-14"], "Wcześniej");
    assert_eq!(by_id["url"], format!("{SITE}/browse/WMS-12"));
    // 90 min rano + 60 min trwającego pomiaru dziś, 60 min trzy dni temu.
    assert_eq!(by_id["trackedMinutesToday"], 150);
    assert_eq!(by_id["trackedMinutesTotal"], 210);
    assert_eq!(f.json("task_get", json!({"key": "wms-12"}))["id"], 1);
    assert_eq!(f.json("task_get", json!({"key": "#20"}))["id"], 4);
    assert_eq!(f.json("task_get", json!({"key": "Code review"}))["id"], 2);
    assert_eq!(f.json("task_get", json!({"key": "3"}))["id"], 3);
    // Zadanie z kosza i nieistniejące.
    assert!(f
        .call("task_get", json!({"id": 5}))
        .unwrap_err()
        .contains("not found"));
    assert!(f
        .call("task_get", json!({"id": 999}))
        .unwrap_err()
        .contains("not found"));
    assert!(f
        .call("task_get", json!({}))
        .unwrap_err()
        .contains("Provide a task"));
    // Dwa zadania o tym samym tytule: potrzebny numer.
    f.db.execute(
        "INSERT INTO tasks(id,title,created_at) VALUES(9,'Import CSV',0)",
        [],
    )
    .unwrap();
    assert!(f
        .call("task_get", json!({"key": "Import CSV"}))
        .unwrap_err()
        .contains("More than one"));
    assert_eq!(f.json("task_get", json!({"id": 9}))["id"], 9);
}

#[test]
fn day_summary_reports_time_goal_meetings_agents_and_notes() {
    let f = Fixture::new();
    let day = f.json("day_summary", json!({"day": "2026-10-15"}));
    assert_eq!(day["day"], "2026-10-15");
    // 90 + 30 + 60 (trwający do „teraz”); zadanie z kosza nie liczy się.
    assert_eq!(day["trackedMinutes"], 180);
    // Cel 8 h minus pół godziny spotkania bez pomiaru.
    assert_eq!(day["goalMinutes"], 450);
    let per_task = day["perTask"].as_array().unwrap();
    assert_eq!(
        per_task[0],
        json!({"taskId": 1, "title": "Analiza importu", "minutes": 150})
    );
    assert_eq!(per_task[1]["taskId"], 2);
    assert_eq!(per_task.len(), 2);
    let measurements = day["measurements"].as_array().unwrap();
    assert_eq!(measurements.len(), 3);
    assert_eq!(measurements[0]["start"], "08:00");
    assert_eq!(measurements[2]["running"], true);
    assert_eq!(measurements[2]["end"], "12:00");
    // Spotkanie bez tytułu, dopóki przełącznik jest wyłączony.
    let meetings = day["meetings"].as_array().unwrap();
    assert_eq!(meetings.len(), 1);
    assert_eq!(meetings[0]["start"], "10:00");
    assert_eq!(meetings[0]["minutes"], 30);
    assert!(meetings[0].get("title").is_none());
    assert!(!day.to_string().contains("Spotkanie"));
    // Czas agentów: 30 min nad zadaniem 1, 20 min bez zadania.
    assert_eq!(day["agentTime"]["perTask"][0]["taskId"], 1);
    assert_eq!(day["agentTime"]["perTask"][0]["agentMinutes"], 30);
    assert_eq!(day["agentTime"]["unmatchedMinutes"], 20);
    // Notatki dnia.
    assert_eq!(day["notes"].as_array().unwrap().len(), 1);
    assert_eq!(day["notes"][0]["taskId"], 1);
    // Domyślnie dziś; zły dzień to błąd.
    assert_eq!(f.json("day_summary", json!({}))["day"], "2026-10-15");
    assert!(f
        .call("day_summary", json!({"day": "jutro"}))
        .unwrap_err()
        .contains("Invalid day"));
    // Dzień bez pracy.
    let empty = f.json("day_summary", json!({"day": "2026-10-12"}));
    assert_eq!(empty["trackedMinutes"], 0);
    assert!(empty["perTask"].as_array().unwrap().is_empty());
}

#[test]
fn meeting_titles_appear_only_when_the_switch_is_on() {
    let mut f = Fixture::new();
    f.settings.ai_send_meeting_titles = true;
    let day = f.json("day_summary", json!({"day": "2026-10-15"}));
    let title = day["meetings"][0]["title"].as_str().unwrap();
    // Tytuł jest, ale nazwisko z niego ukrył filtr.
    assert!(title.starts_with("Spotkanie z"));
    assert!(!title.contains("Kowalski"), "{title}");
}

#[test]
fn week_summary_sums_days_and_tasks() {
    let f = Fixture::new();
    // Środa 14 października należy do tygodnia od poniedziałku 12.
    let week = f.json("week_summary", json!({"week": "2026-10-14"}));
    assert_eq!(week["week"], "2026-10-12");
    let days = week["days"].as_array().unwrap();
    assert_eq!(days.len(), 7);
    assert_eq!(days[0]["day"], "2026-10-12");
    assert_eq!(days[0]["trackedMinutes"], 0);
    assert_eq!(days[1]["trackedMinutes"], 60);
    assert_eq!(days[2]["trackedMinutes"], 120);
    assert_eq!(days[2]["day"], "2026-10-14");
    assert_eq!(days[3]["trackedMinutes"], 180);
    assert_eq!(days[5]["goalMinutes"], 0, "sobota nie ma celu");
    assert_eq!(week["trackedMinutes"], 60 + 120 + 180);
    let per_task = week["perTask"].as_array().unwrap();
    assert_eq!(per_task[0]["taskId"], 1);
    assert_eq!(per_task[0]["minutes"], 60 + 150);
    assert_eq!(per_task[1]["taskId"], 2);
    assert_eq!(per_task[1]["minutes"], 120 + 30);
    // Bez argumentu: bieżący tydzień; wniosek tygodnia, gdy zapisany.
    assert_eq!(f.json("week_summary", json!({}))["week"], "2026-10-12");
    week_notes::save(&f.db, "2026-10-12", "Wnioski tygodnia").unwrap();
    assert_eq!(
        f.json("week_summary", json!({}))["note"],
        "Wnioski tygodnia"
    );
    assert!(f
        .call("week_summary", json!({"week": "x"}))
        .unwrap_err()
        .contains("Invalid week"));
}

#[test]
fn current_measurement_running_and_idle() {
    let f = Fixture::new();
    let running = f.json("current_measurement", json!({}));
    assert_eq!(running["running"], true);
    assert_eq!(running["id"], 1);
    assert_eq!(running["since"], "11:00");
    assert_eq!(running["elapsedMinutes"], 60);
    f.db.execute(
        "UPDATE sessions SET ended_at=?1 WHERE ended_at IS NULL",
        [now()],
    )
    .unwrap();
    assert_eq!(
        f.json("current_measurement", json!({})),
        json!({"running": false})
    );
}

#[test]
fn ts_tasks_uses_the_memory_cache_and_never_the_network() {
    let f = Fixture::new();
    let before = f.json("ts_tasks", json!({}));
    assert_eq!(before["cached"], false);
    assert!(before["note"].as_str().unwrap().contains("memory"));
    f.db.execute(
        "INSERT INTO ts_project_map(project,ts_task_id,ts_task_name) VALUES('WMS','77','WMS rozwój')",
        [],
    )
    .unwrap();
    let mapped = f.json("ts_tasks", json!({}));
    assert_eq!(mapped["mappedProjects"][0]["tsTaskName"], "WMS rozwój");
    f.state.remember_ts(&[Project {
        name: "Projekt A".into(),
        tasks: vec![timesheet::TsTask {
            id: "12".into(),
            name: "Rozwój".into(),
        }],
    }]);
    let cached = f.json("ts_tasks", json!({}));
    assert_eq!(cached["cached"], true);
    assert_eq!(
        cached["projects"][0]["tasks"][0],
        json!({"id": "12", "name": "Rozwój"})
    );
}

#[test]
fn unknown_tools_are_rejected() {
    let f = Fixture::new();
    for tool in ["ts_send", "jira_comment", "settings_set", "task_delete"] {
        assert!(f
            .call(tool, json!({}))
            .unwrap_err()
            .contains("Unknown tool"));
    }
}

// ---------------------------------------------------------------------------
// Filtr osobowy na wyjściu (jak „szczelność” w A0)

fn has_word(text: &str, word: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .any(|w| w.eq_ignore_ascii_case(word) || w.to_lowercase() == word.to_lowercase())
}

#[test]
fn output_of_every_read_tool_is_tight() {
    let mut f = Fixture::new();
    // Najgorszy przypadek: tytuły spotkań włączone.
    f.settings.ai_send_meeting_titles = true;
    let calls = [
        ("tasks_list", json!({})),
        ("tasks_list", json!({"filter": "blocked"})),
        ("tasks_list", json!({"filter": "search", "query": "import"})),
        ("task_get", json!({"id": 1})),
        ("task_get", json!({"id": 2})),
        ("day_summary", json!({"day": "2026-10-15"})),
        ("week_summary", json!({"week": "2026-10-15"})),
        ("current_measurement", json!({})),
        ("ts_tasks", json!({})),
        // Błędy też przechodzą przez filtr: tytuł z nazwiskiem w komunikacie.
        ("task_get", json!({"key": "Spotkanie z Janem Kowalskim"})),
    ];
    let mut all = String::new();
    for (tool, args) in calls {
        let out = f.call(tool, args.clone()).unwrap_or_else(|e| e);
        all.push_str(&out);
        all.push('\n');
    }
    for name in [
        "Kowalski",
        "Kowalskim",
        "Kowalskiego",
        "Nowak",
        "Anna",
        "Anny",
        "Jan",
        "Janem",
    ] {
        assert!(
            !has_word(&all, name),
            "nazwa {name} wyszła do modelu:\n{all}"
        );
    }
    // Filtr zadziałał (pseudonimy są), a zwykłe słowa zostały.
    assert!(all.contains("Osoba"), "{all}");
    assert!(all.contains("Analiza importu"));
    // Pole osobowe Jiry podmienione w całości.
    let task = f.json("task_get", json!({"id": 1}));
    let field = &task["jiraFields"][0];
    assert_eq!(field["id"], "customfield_10050");
    assert!(field["value"].as_str().unwrap().starts_with("Osoba "));
}

#[test]
fn same_person_has_the_same_pseudonym_across_calls() {
    let f = Fixture::new();
    let blocked = f.json("tasks_list", json!({"filter": "blocked"}));
    let waiting = blocked["tasks"][0]["waiting"].as_str().unwrap().to_string();
    let again = f.json("task_get", json!({"id": 2}));
    assert_eq!(again["waiting"].as_str().unwrap(), waiting);
    assert!(waiting.contains("Osoba"));
}

// ---------------------------------------------------------------------------
// Propozycje: kolejka zamiast zmian

/// Zrzut wszystkich tabel bazy (wartości jako tekst) do porównania „przed i po”.
fn dump(db: &Connection) -> String {
    let tables: Vec<String> = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    let mut out = String::new();
    for table in tables {
        let mut stmt = db.prepare(&format!("SELECT * FROM \"{table}\"")).unwrap();
        let columns = stmt.column_count();
        let rows: Vec<String> = stmt
            .query_map([], |r| {
                Ok((0..columns)
                    .map(|i| format!("{:?}", r.get_ref(i).unwrap()))
                    .collect::<Vec<_>>()
                    .join("|"))
            })
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        out.push_str(&format!("{table}:{}\n", rows.join(";")));
    }
    out
}

#[test]
fn propose_tools_queue_proposals_and_change_nothing() {
    let f = Fixture::new();
    let before = dump(&f.db);
    let cases = [
        (
            "propose_task_create",
            json!({"title": "Nowe zadanie", "project": "WMS", "key": "WMS-99", "day": "2026-10-16", "estimate": 90}),
        ),
        (
            "propose_note_append",
            json!({"task": "WMS-12", "text": "Parser gotowy.", "day": "2026-10-15"}),
        ),
        ("propose_link_folder", json!({"task": 2})),
        ("propose_switch", json!({"task": "Code review"})),
        ("propose_plan", json!({"task": "#20", "day": "2026-10-17"})),
    ];
    for (tool, args) in cases {
        assert_eq!(f.call(tool, args).unwrap(), QUEUED, "{tool}");
    }
    assert_eq!(dump(&f.db), before, "propozycje nie mogą zmieniać bazy");
    let queue = f.state.proposals();
    assert_eq!(
        queue.iter().map(|p| p.kind.as_str()).collect::<Vec<_>>(),
        vec![
            "task_create",
            "note_append",
            "link_folder",
            "switch",
            "plan"
        ]
    );
    assert_eq!(
        queue.iter().map(|p| p.id).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    let create = &queue[0];
    assert_eq!(
        (
            create.title.as_str(),
            create.project.as_str(),
            create.key.as_str(),
            create.day.as_str(),
            create.estimate
        ),
        ("Nowe zadanie", "WMS", "WMS-99", "2026-10-16", 90)
    );
    let note = &queue[1];
    assert_eq!(
        (note.task_id, note.task_title.as_str(), note.day.as_str()),
        (Some(1), "Analiza importu", "2026-10-15")
    );
    assert_eq!(note.text, "Parser gotowy.");
    // Folder z pracującej sesji agenta (najnowszej).
    assert_eq!(queue[2].folder, r"C:\Projekty\wms");
    assert_eq!(queue[2].task_id, Some(2));
    assert_eq!(queue[3].task_id, Some(2));
    assert_eq!(
        (queue[4].task_id, queue[4].day.as_str()),
        (Some(4), "2026-10-17")
    );
    // Brak zmian także w danych widocznych w migawce.
    assert_eq!(db::snapshot(&f.db).unwrap().tasks.len(), 4);
}

#[test]
fn propose_note_defaults_to_today_and_duplicates_are_ignored() {
    let f = Fixture::new();
    let args = json!({"task": 1, "text": "Zrobione."});
    f.call("propose_note_append", args.clone()).unwrap();
    f.call("propose_note_append", args).unwrap();
    let queue = f.state.proposals();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].day, "2026-10-15");
    f.call(
        "propose_note_append",
        json!({"task": 1, "text": "Inny tekst."}),
    )
    .unwrap();
    assert_eq!(f.state.proposals().len(), 2);
}

#[test]
fn propose_validation_errors_leave_the_queue_empty() {
    let mut f = Fixture::new();
    let bad = [
        ("propose_task_create", json!({})),
        ("propose_task_create", json!({"title": "  "})),
        ("propose_task_create", json!({"title": "x", "day": "jutro"})),
        (
            "propose_task_create",
            json!({"title": "x", "estimate": 5000}),
        ),
        ("propose_note_append", json!({"task": 999, "text": "x"})),
        ("propose_note_append", json!({"task": 1, "text": " "})),
        (
            "propose_note_append",
            json!({"task": 1, "text": "x", "day": "2026-13-40"}),
        ),
        ("propose_switch", json!({"task": 3})),
        ("propose_switch", json!({"task": "nie ma takiego"})),
        ("propose_plan", json!({"task": 1})),
        ("propose_plan", json!({"task": 1, "day": "poniedziałek"})),
        ("propose_plan", json!({"task": 3, "day": "2026-10-16"})),
        ("propose_link_folder", json!({})),
    ];
    for (tool, args) in bad {
        assert!(f.call(tool, args.clone()).is_err(), "{tool} {args}");
    }
    // Brak pracującej sesji agenta z folderem.
    f.folders.clear();
    assert!(f
        .call("propose_link_folder", json!({"task": 1}))
        .unwrap_err()
        .contains("No agent session"));
    assert!(f.state.proposals().is_empty());
}

#[test]
fn proposals_restore_pseudonyms_so_the_user_sees_real_names() {
    let f = Fixture::new();
    // Model widział „Osoba N” w wyniku narzędzia i używa jej w propozycji.
    let blocked = f.json("tasks_list", json!({"filter": "blocked"}));
    let waiting = blocked["tasks"][0]["waiting"].as_str().unwrap().to_string();
    let pseudonym = waiting
        .split(' ')
        .collect::<Vec<_>>()
        .windows(2)
        .find(|w| w[0] == "Osoba")
        .map(|w| {
            format!(
                "{} {}",
                w[0],
                w[1].trim_matches(|c: char| !c.is_alphanumeric())
            )
        })
        .unwrap();
    f.call(
        "propose_note_append",
        json!({"task": 1, "text": format!("Ustalone z {pseudonym}.")}),
    )
    .unwrap();
    let proposal = &f.state.proposals()[0];
    assert!(!proposal.text.contains("Osoba"), "{}", proposal.text);
    assert!(proposal.text.contains("Nowak"), "{}", proposal.text);
}

#[test]
fn pseudonyms_from_the_chat_and_from_tool_results_do_not_collide() {
    let f = Fixture::new();
    // Czat filtruje wiadomość użytkownika na mapie serwera (wspólnej, A3).
    let sent = ai_privacy::redact_with(
        "Ustal z Jan Kowalski",
        &f.dict,
        &mut f.state.pseudonyms().lock().unwrap(),
    );
    assert_eq!(sent, "Ustal z Osoba 1");
    // Wynik narzędzia z inną osobą dostaje kolejny numer.
    let blocked = f.call("tasks_list", json!({"filter": "blocked"})).unwrap();
    assert!(blocked.contains("Osoba 2"), "{blocked}");
    assert!(!blocked.contains("Osoba 1"), "{blocked}");
    // Model przekazuje do propozycji pseudonim z wiadomości użytkownika.
    f.call(
        "propose_note_append",
        json!({"task": 1, "text": "Ustalone z Osoba 1, czeka Osoba 2."}),
    )
    .unwrap();
    let text = &f.state.proposals()[0].text;
    assert!(text.contains("Ustalone z Jan Kowalski"), "{text}");
    assert!(text.contains("Nowak"), "{text}");
}

#[test]
fn queue_is_capped_and_resolve_removes_one() {
    let f = Fixture::new();
    for i in 0..55 {
        f.call(
            "propose_task_create",
            json!({"title": format!("Zadanie {i}")}),
        )
        .unwrap();
    }
    let queue = f.state.proposals();
    assert_eq!(queue.len(), QUEUE_MAX);
    assert_eq!(queue[0].title, "Zadanie 5");
    let id = queue[1].id;
    let left = f.state.resolve(id);
    assert_eq!(left.len(), QUEUE_MAX - 1);
    assert!(left.iter().all(|p| p.id != id));
    // Nieistniejący numer nic nie zmienia.
    assert_eq!(f.state.resolve(9999).len(), QUEUE_MAX - 1);
}

#[test]
fn proposal_serializes_for_the_window() {
    let f = Fixture::new();
    f.call("propose_plan", json!({"task": 1, "day": "2026-10-16"}))
        .unwrap();
    let value = serde_json::to_value(f.state.proposals()).unwrap();
    assert_eq!(value[0]["kind"], "plan");
    assert_eq!(value[0]["taskId"], 1);
    assert_eq!(value[0]["taskTitle"], "Analiza importu");
    assert_eq!(value[0]["day"], "2026-10-16");
    assert!(value[0].get("estimate").is_some());
}

// ---------------------------------------------------------------------------
// Bramka i start

#[test]
fn gate_needs_ai_mcp_and_a_chosen_engine() {
    let mut s = AppSettings::default();
    assert_eq!(
        gate(&s),
        Err(GATE_NO_ENGINE),
        "domyślnie nic nie działa (silnik niewybrany)"
    );
    s.ai_engine = "off".into();
    assert_eq!(gate(&s), Err(GATE_NO_ENGINE));
    s.ai_engine = "codex".into();
    assert_eq!(gate(&s), Ok(()));
    s.ai_engine = "claude".into();
    s.ai_mcp = false;
    assert_eq!(gate(&s), Err(GATE_OFF));
}

/// `mcp::setup` (manage) przed oknami, `mcp::start` i `mcp::sync` po oknach.
#[test]
fn mcp_is_wired_into_startup_in_the_right_order() {
    let source = include_str!("../main.rs");
    let setup = &source[source.find(".setup(|app| {").unwrap()..];
    let windows = setup.find("create_windows(app)?;").unwrap();
    assert!(setup.find("mcp::setup(").unwrap() < windows);
    assert!(setup.find("mcp::start(").unwrap() > windows);
    assert!(setup.find("mcp::sync_at_startup(").unwrap() > windows);
    for command in [
        "mcp::mcp_proposals",
        "mcp::mcp_proposal_resolve",
        "mcp::mcp_status",
        "mcp::mcp_sync",
    ] {
        assert!(
            source.contains(command),
            "{command} nie jest zarejestrowana"
        );
    }
}

#[test]
fn the_mcp_program_is_declared_for_the_installer() {
    let manifest = include_str!("../../Cargo.toml");
    assert!(manifest.contains("name = \"tuktuk-track-mcp\""));
    assert!(manifest.contains("path = \"src/bin/mcp.rs\""));
}

// ---------------------------------------------------------------------------
// Uwagi z przeglądu A2

/// Pierwszy pseudonim „Osoba N” w tekście.
fn first_pseudonym(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    words
        .windows(2)
        .find(|w| w[0].ends_with("Osoba"))
        .map(|w| {
            format!(
                "Osoba {}",
                w[1].trim_matches(|c: char| !c.is_alphanumeric())
            )
        })
        .unwrap_or_else(|| panic!("brak pseudonimu w {text}"))
}

#[test]
fn search_by_a_pseudonym_from_an_earlier_answer_finds_the_task() {
    let f = Fixture::new();
    f.db.execute(
        "UPDATE task_details SET json=?1 WHERE task_id=2",
        [r#"{"project":"API","notes":{"2026-10-15":"Ustalenia: Jan Kowalski zatwierdza"}}"#],
    )
    .unwrap();
    // Model zobaczył notatkę z pseudonimem i szuka po nim.
    let task = f.call("task_get", json!({"id": 2})).unwrap();
    assert!(!task.contains("Kowalski"), "{task}");
    let pseudonym = first_pseudonym(&task);
    let found = f.json(
        "tasks_list",
        json!({"filter": "search", "query": pseudonym}),
    );
    assert_eq!(ids(&found), vec![2], "{found}");
    // Filtr projektu też przechodzi przez przywrócenie (bez pseudonimu bez zmian).
    let project = f.json("tasks_list", json!({"filter": "project", "query": "api"}));
    assert_eq!(ids(&project), vec![2]);
}

#[test]
fn task_create_key_must_be_jira_or_redmine() {
    let f = Fixture::new();
    for (key, stored) in [("wms-99", "WMS-99"), ("#20", "#20"), ("  ", "")] {
        f.call(
            "propose_task_create",
            json!({"title": format!("Zadanie {stored}"), "key": key}),
        )
        .unwrap();
        assert_eq!(f.state.proposals().last().unwrap().key, stored, "{key}");
    }
    let before = f.state.proposals().len();
    for key in [
        "WMS",
        "#",
        "#12a",
        "W-1",
        "1WMS-2",
        "WMS-",
        "WMS-1x",
        "zadanie z Jirą",
    ] {
        let error = f
            .call("propose_task_create", json!({"title": "x", "key": key}))
            .unwrap_err();
        assert!(error.contains("Invalid key"), "{key}: {error}");
    }
    assert_eq!(f.state.proposals().len(), before);
}

fn agent(cwd: &str, since: i64) -> crate::agents::Agent {
    crate::agents::Agent {
        kind: "claude".into(),
        name: "sesja".into(),
        cwd: cwd.into(),
        since,
        ..Default::default()
    }
}

#[test]
fn link_folder_uses_the_calling_session_folder_with_two_agents_working() {
    let ai_dir = std::path::Path::new(r"C:\dane\tuktuk-track\ai");
    let agents = || {
        vec![
            agent(r"C:\Projekty\wms", 100),
            agent(r"C:\Projekty\api", 200),
            agent(r"C:\dane\tuktuk-track\ai", 300),
        ]
    };
    // Starsza sesja wywołuje narzędzie: jej folder, nie najnowszej.
    assert_eq!(
        session_folders(r"C:\Projekty\wms", Some(ai_dir), agents()),
        vec![r"C:\Projekty\wms".to_string()]
    );
    // Bez `cwd`, z katalogiem asystenta albo ścieżką względną: sesje od najnowszej.
    for cwd in ["", r"C:\dane\tuktuk-track\ai", "wzgledna"] {
        assert_eq!(
            session_folders(cwd, Some(ai_dir), agents()),
            vec![
                r"C:\Projekty\api".to_string(),
                r"C:\Projekty\wms".to_string()
            ],
            "{cwd}"
        );
    }
}

#[test]
fn startup_runs_no_cli_when_ai_is_off_or_not_chosen() {
    let mut s = AppSettings::default();
    for (engine, needed) in [
        ("", false),
        ("off", false),
        ("claude", true),
        ("codex", true),
    ] {
        s.ai_engine = engine.into();
        assert_eq!(startup_sync_needed(&s), needed, "{engine}");
    }
    // Wyłączone MCP przy wybranym silniku: start sprawdza, żeby usunąć stary wpis.
    s.ai_mcp = false;
    assert!(startup_sync_needed(&s));
}
