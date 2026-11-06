//! Powiadomienia przebiegów: czysta funkcja `notice_for` i deduplikacja kluczy.

use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde_json::json;
use workflow_core::ports::StoredEvent;

use super::*;

const ON: NoticeSettings = NoticeSettings {
    waiting: true,
    done: true,
};

fn info() -> RunInfo {
    RunInfo {
        workflow_name: "Analiza".into(),
        task_title: Some("Naprawa kalkulatora".into()),
    }
}

fn event(kind: &str, payload: serde_json::Value) -> StoredEvent {
    StoredEvent {
        run_id: 7,
        sequence: 3,
        execution_id: None,
        kind: kind.into(),
        timestamp: 1,
        payload,
    }
}

fn approval(id: &str) -> StoredEvent {
    event(
        "approvalRequested",
        json!({"type": "approvalRequested", "approvalId": id,
               "executionId": "e", "artifactHash": "h"}),
    )
}

fn state_change(to: &str) -> StoredEvent {
    event(
        "runStateChanged",
        json!({"type": "runStateChanged", "from": "running", "to": to, "code": null}),
    )
}

#[test]
fn notice_for_approval_and_terminal_states_only() {
    let message = notice_for(&approval("a-1"), &info(), ON).expect("zgoda");
    assert_eq!(message.key, "wf-approval:a-1");
    for status in [
        "succeeded",
        "rejected",
        "failed",
        "cancelled",
        "interrupted",
    ] {
        let message = notice_for(&state_change(status), &info(), ON).expect(status);
        assert_eq!(message.key, "wf-end:7");
    }
    for status in ["running", "waitingForApproval", "cancelling", "queued"] {
        assert_eq!(
            notice_for(&state_change(status), &info(), ON),
            None,
            "{status}"
        );
    }
    let message = event("message", json!({"type": "message"}));
    assert_eq!(notice_for(&message, &info(), ON), None);
}

#[test]
fn notice_respects_agent_notify_settings() {
    let off = NoticeSettings {
        waiting: false,
        done: false,
    };
    assert_eq!(notice_for(&approval("a"), &info(), off), None);
    assert_eq!(notice_for(&state_change("succeeded"), &info(), off), None);
    let only_waiting = NoticeSettings {
        waiting: true,
        done: false,
    };
    assert!(notice_for(&approval("a"), &info(), only_waiting).is_some());
    assert_eq!(
        notice_for(&state_change("succeeded"), &info(), only_waiting),
        None
    );
}

#[test]
fn notice_targets_the_run_view() {
    let message = notice_for(&approval("a"), &info(), ON).unwrap();
    assert_eq!(message.open.view, Some("workflow-runs"));
    assert_eq!(message.open.run, Some(7));
    assert!(message.body.contains("Naprawa kalkulatora"));
}

#[test]
fn notice_keys_are_deduplicated_via_notifications_sent() {
    let dir = std::env::temp_dir().join(format!("tuktuk-wf-notice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("db.sqlite");
    let _ = std::fs::remove_file(&file);
    {
        let conn = Connection::open(&file).unwrap();
        crate::db::init(&conn).unwrap();
        let db: SharedDb = Arc::new(Mutex::new(conn));
        assert!(claim_key(&db, "wf-approval:a-1", 1));
        assert!(!claim_key(&db, "wf-approval:a-1", 2));
        assert!(claim_key(&db, "wf-end:7", 3));
    }
    // Po ponownym otwarciu bazy (restart aplikacji) klucz nadal blokuje powtórkę.
    let conn = Connection::open(&file).unwrap();
    let db: SharedDb = Arc::new(Mutex::new(conn));
    assert!(!claim_key(&db, "wf-approval:a-1", 4));
    assert!(!claim_key(&db, "wf-end:7", 4));
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Błąd bazy przy zapisie klucza jest zgłaszany (nie udaje sukcesu) i nie daje toastu.
#[test]
fn claim_key_db_error_is_logged_not_treated_as_sent() {
    // Baza bez tabel: INSERT zawodzi.
    let broken: SharedDb = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
    assert!(try_claim_key(&broken, "wf-end:1", 1).is_err());
    assert!(!claim_key(&broken, "wf-end:1", 1));
    // Zatruta blokada.
    let poisoned: SharedDb = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
    let clone = poisoned.clone();
    let _ = std::thread::spawn(move || {
        let _guard = clone.lock().unwrap();
        panic!("zatrucie blokady w teście");
    })
    .join();
    assert!(try_claim_key(&poisoned, "wf-end:2", 1)
        .unwrap_err()
        .contains("zatruta"));
    assert!(!claim_key(&poisoned, "wf-end:2", 1));
}

#[test]
fn toast_claims_the_key_first_and_skips_when_the_window_is_active() {
    use std::cell::Cell;
    let claimed = Cell::new(false);
    let claim = || {
        claimed.set(true);
        true
    };
    assert!(!should_show_toast(claim, || true));
    assert!(
        claimed.get(),
        "klucz musi być zapisany także przy aktywnym oknie"
    );
    assert!(should_show_toast(|| true, || false));
    let asked = Cell::new(false);
    assert!(!should_show_toast(
        || false,
        || {
            asked.set(true);
            false
        }
    ));
    assert!(
        !asked.get(),
        "powtórka nie sprawdza okna ani nie wysyła toastu"
    );
}

/// Tylko zgoda i zmiana stanu przebiegu mogą dać powiadomienie; komunikaty
/// agentów i zdarzenia narzędzi nie czytają ustawień ani bazy.
#[test]
fn only_approval_and_state_events_reach_the_notice_path() {
    assert!(may_notify(&approval("a-1")));
    assert!(may_notify(&state_change("running")));
    for kind in [
        "message",
        "toolStarted",
        "toolFinished",
        "nodeStateChanged",
        "error",
        "approvalResolved",
    ] {
        assert!(!may_notify(&event(kind, json!({"type": kind}))), "{kind}");
    }
}

/// Błąd bazy przy odczycie danych przebiegu jest zwracany (i logowany w
/// `run_info`), a brak przebiegu to `Ok(None)`.
#[test]
fn run_info_reports_db_errors_and_missing_runs_separately() {
    let broken: SharedDb = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
    assert!(try_run_info(&broken, 1).is_err());
    assert_eq!(run_info(&broken, 1), None);
    let conn = Connection::open_in_memory().unwrap();
    crate::db::init(&conn).unwrap();
    let db: SharedDb = Arc::new(Mutex::new(conn));
    assert_eq!(try_run_info(&db, 99), Ok(None));
}

/// Sink, który w `emit` blokuje bazę (jak `TauriSink`: dane przebiegu i klucz
/// powiadomienia), nie zakleszcza sterownika: repozytorium nie trzyma blokady
/// w trakcie przekazywania zdarzeń do magistrali.
#[test]
fn sink_that_locks_db_does_not_deadlock_driver() {
    use std::time::Duration;
    use workflow_core::driver::{DriverConfig, Runtime};
    use workflow_core::model::AgentConfig;
    use workflow_core::ports::{EventSink, NewRun, RunRepository};
    use workflow_core::template::analysis_implementation_review;
    use workflow_core::testing::{samples, FixedWorkspace, ScriptedRunner};

    use crate::workflows::runs::SqliteRunRepository;
    use crate::workflows::store;

    struct LockingSink {
        db: SharedDb,
        seen: Mutex<usize>,
    }
    impl EventSink for LockingSink {
        fn emit(&self, event: &StoredEvent) {
            let _ = run_info(&self.db, event.run_id);
            let _ = claim_key(&self.db, &format!("t:{}", event.sequence), 1);
            *self.seen.lock().unwrap() += 1;
        }
        fn fault(&self, _run_id: i64, message: &str) {
            panic!("błąd sterownika: {message}");
        }
    }

    let conn = Connection::open_in_memory().unwrap();
    crate::db::init(&conn).unwrap();
    let id = store::create(&conn, "P", "", &analysis_implementation_review(), 1).unwrap();
    store::publish(&conn, id, 1, 2).unwrap();
    let version_id: i64 = conn
        .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
        .unwrap();
    let db: SharedDb = Arc::new(Mutex::new(conn));
    let repo = SqliteRunRepository::new(db.clone());
    let run = repo
        .create_run(&NewRun {
            version_id,
            request_id: "lock".into(),
            now: 1,
            ..NewRun::default()
        })
        .unwrap()
        .run_id;
    let sink = Arc::new(LockingSink {
        db: db.clone(),
        seen: Mutex::new(0),
    });
    let handle = Runtime::start(
        Arc::new(repo),
        Arc::new(ScriptedRunner::new().template_script(vec![samples::review_approved()])),
        sink.clone(),
        Arc::new(FixedWorkspace::new("h1")),
        DriverConfig {
            step_timeout: |_: &AgentConfig| Duration::from_secs(20),
            cancel_grace: Duration::from_secs(2),
            poll_interval: Duration::from_millis(5),
            now: || 1_000,
            on_error: |m| panic!("błąd sterownika: {m}"),
        },
    );
    handle.start_run(run);
    assert!(
        handle.wait_idle(Duration::from_secs(10)),
        "sterownik zakleszczony przez sink blokujący bazę"
    );
    let status: String = db
        .lock()
        .unwrap()
        .query_row(
            "SELECT status FROM workflow_runs WHERE id = ?1",
            [run],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status, "waitingForApproval");
    assert!(*sink.seen.lock().unwrap() > 5);
}

/// Kryterium 12 E6: aktualizację blokuje przebieg z procesem (`queued`, `running`,
/// `cancelling`), ale nie sam przebieg czekający na zgodę.
#[test]
fn update_blocker_rejects_queued_running_cancelling_but_not_waiting() {
    let t = crate::workflows::test_db::TestDb::new();
    let run = t.create("u");
    for (status, blocked) in [
        ("queued", true),
        ("running", true),
        ("cancelling", true),
        ("waitingForApproval", false),
        ("interrupted", false),
        ("succeeded", false),
    ] {
        let conn = t.db.lock().unwrap();
        conn.execute(
            "UPDATE workflow_runs SET status = ?1 WHERE id = ?2",
            rusqlite::params![status, run],
        )
        .unwrap();
        let blocker = update_blocker(&conn).unwrap();
        assert_eq!(blocker.is_some(), blocked, "{status}");
        if let Some(message) = blocker {
            assert!(message.starts_with("Aktualizacja wstrzymana."), "{message}");
        }
    }
}

/// Runda 2 E6: zatrzymany sterownik nie przyjmuje nowych przebiegów. Bez tego
/// `workflow_start` zapisałby przebieg, a komenda startu przepadłaby po cichu.
#[test]
fn stopped_runtime_rejects_new_work_with_message() {
    use std::time::Duration;
    use workflow_core::driver::{DriverConfig, Runtime};
    use workflow_core::model::AgentConfig;
    use workflow_core::testing::{FixedWorkspace, RecordingSink, ScriptedRunner};

    assert!(live_handle(None).is_err());
    let t = crate::workflows::test_db::TestDb::new();
    let handle = Runtime::start(
        Arc::new(t.repo.clone()),
        Arc::new(ScriptedRunner::new()),
        Arc::new(RecordingSink::new()),
        Arc::new(FixedWorkspace::new("h1")),
        DriverConfig {
            step_timeout: |_: &AgentConfig| Duration::from_secs(20),
            cancel_grace: Duration::from_millis(300),
            poll_interval: Duration::from_millis(5),
            now: || 1_000,
            on_error: |m| panic!("błąd sterownika: {m}"),
        },
    );
    assert!(live_handle(Some(&handle)).is_ok());
    handle.shutdown_graceful(Duration::from_secs(5));
    assert_eq!(
        live_handle(Some(&handle)).err().as_deref(),
        Some(ERR_RUNTIME_STOPPED)
    );
}
