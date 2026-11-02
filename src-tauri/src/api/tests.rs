use super::*;
use chrono_tz::Europe::Warsaw;
use std::sync::Mutex;

const MIN: i64 = 60_000;

fn open() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db::init(&db).unwrap();
    db
}

/// 2026-11-02 o `h`:`m` czasu warszawskiego (UTC+1).
fn at(h: u32, m: u32) -> i64 {
    Warsaw
        .with_ymd_and_hms(2026, 11, 2, h, m, 0)
        .unwrap()
        .timestamp_millis()
}

fn run(db: &mut Connection, request: Value, now: i64) -> Outcome {
    handle(true, db, &Warsaw, &Links::none(), &request, now)
}

fn open_session(db: &Connection) -> Option<(i64, i64)> {
    db.query_row(
        "SELECT task_id, started_at FROM sessions WHERE ended_at IS NULL",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .optional()
    .unwrap()
}

#[test]
fn disabled_setting_rejects_everything_without_changes() {
    let mut db = open();
    let out = handle(
        false,
        &mut db,
        &Warsaw,
        &Links::none(),
        &json!({"action": "add", "title": "X", "start": true}),
        at(9, 0),
    );
    assert_eq!(out.reply["ok"], false);
    assert_eq!(out.reply["code"], code::DISABLED);
    assert_eq!(out.effect, Effect::default());
    let tasks: i64 = db
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 0);
}

#[test]
fn status_without_measurement() {
    let mut db = open();
    let out = run(&mut db, json!({"action": "status"}), at(9, 0));
    assert_eq!(
        out.reply,
        json!({"ok": true, "data": {
            "running": false, "task": null, "startedAt": null,
            "startedAtMs": null, "elapsedSeconds": 0,
        }})
    );
    assert_eq!(out.effect, Effect::default());
}

#[test]
fn add_without_start_leaves_the_timer_alone() {
    let mut db = open();
    db::change(&mut db, "add", None, Some("Stare".into()), 0).unwrap();
    db::change(&mut db, "start", Some(1), None, at(8, 0)).unwrap();
    let out = run(
        &mut db,
        json!({"action": "add", "title": "  Raport  "}),
        at(9, 0),
    );
    let data = &out.reply["data"];
    assert_eq!(data["taskId"], 2);
    assert_eq!(data["title"], "Raport");
    assert_eq!(data["started"], false);
    assert_eq!(data["status"]["task"]["id"], 1);
    assert_eq!(open_session(&db), Some((1, at(8, 0))));
    assert_eq!(
        out.effect,
        Effect {
            changed: true,
            feedback: None,
            day_resumed: false
        }
    );
}

#[test]
fn add_with_start_stops_previous_measurement() {
    let mut db = open();
    db::change(&mut db, "add", None, Some("Stare".into()), 0).unwrap();
    db::change(&mut db, "start", Some(1), None, at(8, 0)).unwrap();
    let out = run(
        &mut db,
        json!({"action": "add", "title": "Spotkanie", "start": true}),
        at(9, 0),
    );
    let data = &out.reply["data"];
    assert_eq!(data["started"], true);
    assert_eq!(data["dayResumed"], false);
    assert_eq!(data["status"]["running"], true);
    assert_eq!(data["status"]["task"]["title"], "Spotkanie");
    assert_eq!(open_session(&db), Some((2, at(9, 0))));
    let old_end: i64 = db
        .query_row("SELECT ended_at FROM sessions WHERE task_id=1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(old_end, at(9, 0));
    assert_eq!(out.effect.feedback.as_deref(), Some("Start: Spotkanie"));
}

#[test]
fn status_reports_task_start_and_elapsed_time() {
    let mut db = open();
    run(
        &mut db,
        json!({"action": "add", "title": "Kod", "start": true}),
        at(9, 15),
    );
    let out = run(
        &mut db,
        json!({"action": "status"}),
        at(9, 15) + 25 * MIN + 30_000,
    );
    assert_eq!(
        out.reply["data"],
        json!({
            "running": true,
            "task": {"id": 1, "title": "Kod", "jiraKey": null, "redmineId": null},
            "startedAt": "2026-11-02T09:15:00+01:00",
            "startedAtMs": at(9, 15),
            "elapsedSeconds": 25 * 60 + 30,
        })
    );
}

#[test]
fn stop_ends_measurement_and_reports_duration() {
    let mut db = open();
    run(
        &mut db,
        json!({"action": "add", "title": "Kod", "start": true}),
        at(9, 0),
    );
    let out = run(&mut db, json!({"action": "stop"}), at(10, 30));
    assert_eq!(
        out.reply["data"]["stopped"],
        json!({"taskId": 1, "title": "Kod", "durationSeconds": 90 * 60})
    );
    assert_eq!(out.reply["data"]["status"]["running"], false);
    assert_eq!(open_session(&db), None);
    assert_eq!(out.effect.feedback.as_deref(), Some("Stop: Kod · 01:30:00"));
    assert!(out.effect.changed);
}

#[test]
fn stop_without_measurement_changes_nothing() {
    let mut db = open();
    let out = run(&mut db, json!({"action": "stop"}), at(9, 0));
    assert_eq!(out.reply["ok"], true);
    assert_eq!(out.reply["data"]["stopped"], Value::Null);
    assert_eq!(out.effect, Effect::default());
}

#[test]
fn start_after_ended_day_resumes_the_day_silently() {
    let mut db = open();
    db.execute(
        "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES('2026-11-02',?1,?2,0)",
        [at(8, 0), at(16, 0)],
    )
    .unwrap();
    let out = run(
        &mut db,
        json!({"action": "add", "title": "Pilne", "start": true}),
        at(17, 0),
    );
    assert_eq!(out.reply["data"]["dayResumed"], true);
    assert_eq!(open_session(&db), Some((1, at(17, 0))));
    let mark = days::get(&db, "2026-11-02").unwrap().unwrap();
    assert_eq!((mark.ended_at, mark.resumed), (None, 1));
    assert!(out.effect.day_resumed);
}

#[test]
fn add_without_start_keeps_the_ended_day_ended() {
    let mut db = open();
    db.execute(
        "INSERT INTO day_marks(day,started_at,ended_at,resumed) VALUES('2026-11-02',?1,?2,0)",
        [at(8, 0), at(16, 0)],
    )
    .unwrap();
    let out = run(
        &mut db,
        json!({"action": "add", "title": "Na jutro"}),
        at(17, 0),
    );
    assert_eq!(out.reply["data"]["dayResumed"], false);
    let mark = days::get(&db, "2026-11-02").unwrap().unwrap();
    assert_eq!(mark.ended_at, Some(at(16, 0)));
}

#[test]
fn rejects_bad_requests_with_codes() {
    let mut db = open();
    for (request, expected) in [
        (json!({}), code::BAD_REQUEST),
        (json!({"action": "delete"}), code::BAD_REQUEST),
        (json!({"action": 1}), code::BAD_REQUEST),
        (
            json!({"action": "add", "title": "X", "foo": 1}),
            code::BAD_REQUEST,
        ),
        (
            json!({"action": "add", "title": "X", "Start": true}),
            code::BAD_REQUEST,
        ),
        (json!({"action": "add", "title": 5}), code::BAD_REQUEST),
        (json!({"action": "add", "redmineId": -1}), code::BAD_REQUEST),
        (json!({"action": "add", "title": "   "}), code::INVALID),
        (
            json!({"action": "add", "title": "x".repeat(241)}),
            code::INVALID,
        ),
        // Jira niepołączona: klucz nie jest wiązany z nieznanym adresem.
        (
            json!({"action": "add", "title": "X", "jiraKey": "WMS-1"}),
            code::INVALID,
        ),
    ] {
        let out = run(&mut db, request.clone(), at(9, 0));
        assert_eq!(out.reply["ok"], false, "{request}");
        assert_eq!(out.reply["code"], expected, "{request}");
        assert!(out.reply["error"].as_str().is_some_and(|e| !e.is_empty()));
        assert_eq!(out.effect, Effect::default(), "{request}");
    }
    let tasks: i64 = db
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 0);
}

#[test]
fn links_jira_key_and_title_from_cached_issue() {
    let mut db = open();
    let summary = |key: &str| (key == "WMS-7").then(|| "Poprawka eksportu".to_string());
    let links = Links {
        jira_site: Some("https://example.atlassian.net".into()),
        redmine_site: None,
        summary: &summary,
        subject: &|_| None,
    };
    let out = handle(
        true,
        &mut db,
        &Warsaw,
        &links,
        &json!({"action": "add", "jiraKey": "WMS-7", "start": true}),
        at(9, 0),
    );
    assert_eq!(out.reply["data"]["title"], "Poprawka eksportu");
    assert_eq!(out.reply["data"]["status"]["task"]["jiraKey"], "WMS-7");
}

#[cfg(windows)]
#[test]
fn pipe_round_trip_through_the_wire_client() {
    let pipe = api_wire::pipe_name_for(&format!("api-test-{}-{}", std::process::id(), db::now()));
    let server = pipe.clone();
    let db = Mutex::new(open());
    std::thread::spawn(move || {
        let _ = crate::browser::pipe::serve(&server, move |body| {
            let Ok(request) = serde_json::from_slice::<Value>(body) else {
                return api_wire::error(code::BAD_REQUEST, "x");
            };
            let mut db = db.lock().unwrap();
            handle(true, &mut db, &Warsaw, &Links::none(), &request, at(9, 0)).reply
        });
    });
    let mut reply = Value::Null;
    for _ in 0..100 {
        reply = api_wire::call(
            &pipe,
            &json!({"action": "add", "title": "Z potoku", "start": true}),
        );
        if reply["code"] != code::NOT_RUNNING {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(reply["ok"], true, "{reply}");
    let status = api_wire::call(&pipe, &json!({"action": "status"}));
    assert_eq!(status["data"]["task"]["title"], "Z potoku");
}
