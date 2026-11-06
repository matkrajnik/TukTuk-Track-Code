//! Start przebiegu na prawdziwym gicie w katalogu tymczasowym; CLI są wstrzyknięte.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use workflow_core::template::analysis_implementation_review;

use super::*;
use crate::ai::Cli;
use crate::workflows::workspace::{git, worktrees_dir};

static COUNTER: AtomicU32 = AtomicU32::new(0);

pub(super) struct Sandbox {
    pub root: PathBuf,
    _git: crate::workflows::test_git::GitPermit,
}

impl Sandbox {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("tuktuk svc żółć {}-{n}", std::process::id()));
        std::fs::create_dir_all(&root).expect("katalog testu");
        Self {
            root,
            _git: crate::workflows::test_git::GitPermit::acquire(),
        }
    }

    pub fn app_data(&self) -> PathBuf {
        let dir = self.root.join("app data");
        std::fs::create_dir_all(&dir).expect("app data");
        dir
    }

    /// Przykładowe repozytorium z E4_TEST_RECZNY.md: kalkulator z błędem odejmowania.
    pub fn sample_repo(&self) -> PathBuf {
        let dir = self.root.join("kalkulator próbny");
        std::fs::create_dir_all(dir.join("src")).expect("repo");
        let g = |args: &[&str]| {
            let mut full = vec!["-c", "user.name=t", "-c", "user.email=t@example.com"];
            full.extend_from_slice(args);
            git(&dir, &full).expect("git");
        };
        g(&["init", "-q"]);
        std::fs::write(
            dir.join("src/kalkulator.js"),
            "function subtract(a, b) {\n  return a - b;\n}\nmodule.exports = { subtract };\n",
        )
        .expect("plik");
        std::fs::write(dir.join("README.md"), "# Kalkulator\n").expect("plik");
        g(&["add", "-A"]);
        g(&["commit", "-q", "-m", "start"]);
        dir
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn cli(version: &str) -> Option<Cli> {
    Some(Cli {
        path: "x".into(),
        version: version.into(),
    })
}

pub(super) fn detected_ok() -> Detected {
    Detected {
        claude: cli("2.1.318 (Claude Code)"),
        codex: cli("codex-cli 0.163.0"),
    }
}

pub(super) fn config_ok() -> Result<Option<String>, String> {
    Ok(Some("[windows]\nsandbox = \"elevated\"\n".into()))
}

pub(super) struct Env {
    pub db: SharedDb,
    pub version_id: i64,
    pub enqueued: RefCell<Vec<i64>>,
}

impl Env {
    pub fn from_connection(unlocked: bool) -> Self {
        let conn = Connection::open_in_memory().expect("db");
        crate::db::init(&conn).expect("init");
        conn.execute(
            "INSERT INTO tasks(title, created_at) VALUES('Zadanie z bazy', 1)",
            [],
        )
        .expect("zadanie");
        let id = store::create(&conn, "P", "", &analysis_implementation_review(), 1).expect("p");
        store::publish(&conn, id, 1, 2).expect("publish");
        let version_id = conn
            .query_row("SELECT id FROM workflow_versions", [], |r| r.get(0))
            .expect("wersja");
        if unlocked {
            store::set_unlocked(&conn).expect("odblokowanie");
        }
        Self {
            db: Arc::new(Mutex::new(conn)),
            version_id,
            enqueued: RefCell::new(Vec::new()),
        }
    }

    pub fn start_with(
        &self,
        sb: &Sandbox,
        detected: &Detected,
        req: StartRequest,
    ) -> Result<StartResult, String> {
        let enqueue = |id: i64| self.enqueued.borrow_mut().push(id);
        let emit = |_: &StoredEvent| {};
        start(
            &StartDeps {
                db: &self.db,
                app_data: &sb.app_data(),
                detected,
                read_codex_config: config_ok,
                now: 100,
                enqueue: &enqueue,
                emit: &emit,
            },
            req,
        )
    }

    pub fn request(&self, repo: &Path, request_id: &str) -> StartRequest {
        StartRequest {
            version_id: self.version_id,
            request_id: request_id.into(),
            task_id: None,
            repo: repo.to_string_lossy().into_owned(),
            goal: None,
        }
    }

    pub fn count(&self, sql: &str) -> i64 {
        self.db
            .lock()
            .unwrap()
            .query_row(sql, [], |r| r.get(0))
            .unwrap()
    }
}

fn worktree_count(repo: &Path) -> usize {
    git(repo, &["worktree", "list"])
        .map(|b| String::from_utf8_lossy(&b).lines().count())
        .unwrap_or(0)
}

#[test]
fn locked_module_rejects_start() {
    let sb = Sandbox::new();
    let env = Env::from_connection(false);
    let repo = sb.sample_repo();
    let err = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "r"))
        .unwrap_err();
    assert!(err.contains("nie jest odblokowany"));
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 0);
}

#[test]
fn archived_or_unknown_version_is_rejected() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let mut unknown = env.request(&repo, "a");
    unknown.version_id = 999;
    assert!(env
        .start_with(&sb, &detected_ok(), unknown)
        .unwrap_err()
        .contains("Nie znaleziono"));
    env.db
        .lock()
        .unwrap()
        .execute("UPDATE workflows SET archived_at = 5", [])
        .unwrap();
    assert!(env
        .start_with(&sb, &detected_ok(), env.request(&repo, "b"))
        .unwrap_err()
        .contains("zarchiwizowany"));
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 0);
}

#[test]
fn blocked_preflight_creates_no_run() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let none = Detected::default();
    let err = env
        .start_with(&sb, &none, env.request(&repo, "r"))
        .unwrap_err();
    assert!(err.contains("zablokowany"), "{err}");
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 0);
    assert!(env.enqueued.borrow().is_empty());
    assert_eq!(worktree_count(&repo), 1);
}

#[test]
fn dirty_repo_creates_no_run_and_no_worktree() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    std::fs::write(repo.join("README.md"), "zmiana\n").unwrap();
    let err = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "r"))
        .unwrap_err();
    assert!(err.contains("niezacommitowane"));
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 0);
    assert_eq!(worktree_count(&repo), 1);
    assert!(!worktrees_dir(&sb.app_data()).join("1").exists());
}

#[test]
fn same_request_id_twice_creates_one_run_one_worktree_one_enqueue() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let first = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "dbl"))
        .unwrap();
    let second = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "dbl"))
        .unwrap();
    assert!(first.created && !second.created);
    assert_eq!(first.run_id, second.run_id);
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 1);
    assert_eq!(worktree_count(&repo), 2, "repo główne + jeden worktree");
    assert_eq!(*env.enqueued.borrow(), [first.run_id]);
    let workspace: Option<String> = env
        .db
        .lock()
        .unwrap()
        .query_row("SELECT workspace FROM workflow_runs", [], |r| r.get(0))
        .unwrap();
    assert!(workspace.is_some());
}

#[test]
fn workspace_failure_after_create_marks_run_failed_workspace_unavailable() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    // Zajęty katalog docelowy worktree przebiegu 1 wymusza błąd po create_run.
    std::fs::create_dir_all(worktrees_dir(&sb.app_data()).join("1")).unwrap();
    let err = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "r"))
        .unwrap_err();
    assert!(!err.is_empty());
    let (status, code): (String, Option<String>) = env
        .db
        .lock()
        .unwrap()
        .query_row("SELECT status, error_code FROM workflow_runs", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(status, "failed");
    assert_eq!(code.as_deref(), Some("workspaceUnavailable"));
    assert!(env.enqueued.borrow().is_empty());
}

#[test]
fn task_title_comes_from_db_not_client() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let mut req = env.request(&repo, "t");
    req.task_id = Some(1);
    let run = env.start_with(&sb, &detected_ok(), req).unwrap().run_id;
    let title: String = env
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT task_title FROM workflow_runs WHERE id = ?1",
            [run],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "Zadanie z bazy");
    let mut missing = env.request(&repo, "t2");
    missing.task_id = Some(77);
    assert!(env.start_with(&sb, &detected_ok(), missing).is_err());
}

#[test]
fn goal_over_limit_is_rejected() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let mut req = env.request(&repo, "g");
    req.goal = Some("x".repeat(MAX_GOAL_CHARS + 1));
    assert!(env
        .start_with(&sb, &detected_ok(), req)
        .unwrap_err()
        .contains("za długi"));
    let mut relative = env.request(&repo, "g2");
    relative.repo = "wzgledna/sciezka".into();
    assert!(env.start_with(&sb, &detected_ok(), relative).is_err());
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 0);
}

/// Pierwotna przyczyna (błąd worktree) nie ginie, gdy zawiedzie też oznaczenie
/// przebiegu jako nieudanego; komunikat mówi, że przebieg mógł zostać w kolejce.
#[test]
fn failed_fail_queued_run_is_reported_not_swallowed() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    std::fs::create_dir_all(worktrees_dir(&sb.app_data()).join("1")).unwrap();
    // Każdy zapis zdarzenia po `runCreated` zawodzi.
    env.db
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER test_fail_events BEFORE INSERT ON workflow_events
             WHEN NEW.sequence > 1 BEGIN SELECT RAISE(ABORT, 'awaria testowa'); END;",
        )
        .unwrap();
    let err = env
        .start_with(&sb, &detected_ok(), env.request(&repo, "ff"))
        .unwrap_err();
    assert!(err.contains("może zostać w kolejce"), "{err}");
    assert!(err.contains("awaria testowa"), "{err}");
    let first = err.split(" Dodatkowo").next().unwrap();
    assert!(
        !first.is_empty() && !first.contains("awaria testowa"),
        "{err}"
    );
    assert!(env.enqueued.borrow().is_empty());
}

/// Dwa równoległe żądania z tym samym `requestId`: jeden przebieg, jeden worktree,
/// jedno wstawienie do kolejki (UNIQUE w bazie rozstrzyga wyścig).
#[test]
fn concurrent_start_same_request_id_creates_one_run() {
    let sb = Sandbox::new();
    let env = Env::from_connection(true);
    let repo = sb.sample_repo();
    let app_data = sb.app_data();
    let enqueued = Mutex::new(Vec::new());
    let barrier = std::sync::Barrier::new(2);
    let detected = detected_ok();
    let db = &env.db;
    let request = env.request(&repo, "same-concurrent");
    let results: Vec<StartResult> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let enqueue = |id: i64| enqueued.lock().unwrap().push(id);
                    let emit = |_: &StoredEvent| {};
                    let deps = StartDeps {
                        db,
                        app_data: &app_data,
                        detected: &detected,
                        read_codex_config: config_ok,
                        now: 100,
                        enqueue: &enqueue,
                        emit: &emit,
                    };
                    barrier.wait();
                    start(&deps, request.clone()).expect("start")
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    assert_eq!(results[0].run_id, results[1].run_id);
    assert_eq!(results.iter().filter(|r| r.created).count(), 1);
    assert_eq!(env.count("SELECT COUNT(*) FROM workflow_runs"), 1);
    assert_eq!(worktree_count(&repo), 2, "repo główne + jeden worktree");
    assert_eq!(*enqueued.lock().unwrap(), [results[0].run_id]);
}
