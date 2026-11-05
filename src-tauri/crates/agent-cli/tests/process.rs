use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use agent_cli::{
    KillResult, Options, ProcessEvent, RunningProcess, SpawnError, Spec, StreamingProcess,
};

const FAKE: &str = env!("CARGO_BIN_EXE_agent-cli-fake");
const WAIT: Duration = Duration::from_secs(20);

fn spec(args: &[&str]) -> Spec {
    Spec {
        program: PathBuf::from(FAKE),
        args: args.iter().map(|a| a.to_string()).collect(),
        stdin: String::new(),
        cwd: None,
        env: Vec::new(),
    }
}

/// Zbiera wszystkie linie i kod wyjścia.
fn drain(process: &RunningProcess) -> (Vec<String>, usize, Option<i32>) {
    let mut lines = Vec::new();
    let mut too_long = 0;
    loop {
        match process.events().recv_timeout(WAIT).expect("zdarzenie") {
            ProcessEvent::Line(l) => lines.push(l),
            ProcessEvent::LineTooLong => too_long += 1,
            ProcessEvent::Exited(code) => return (lines, too_long, code),
        }
    }
}

fn unique_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("agent-cli-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("katalog");
    dir
}

fn wait_for_file(path: &Path) -> String {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(path) {
            if !text.is_empty() {
                return text;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("brak pliku {}", path.display());
}

#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .expect("tasklist");
    String::from_utf8_lossy(&out.stdout).contains(&pid.to_string())
}

#[cfg(windows)]
fn assert_dead_soon(pid: u32) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while process_alive(pid) {
        assert!(Instant::now() < deadline, "proces {pid} nadal żyje");
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn streams_lines_and_reports_exit_code_zero_and_three() {
    let ok = StreamingProcess::spawn(&spec(&["exit", "0"]), Options::default()).expect("spawn");
    assert_eq!(drain(&ok), (vec!["bye".to_string()], 0, Some(0)));
    let bad = StreamingProcess::spawn(&spec(&["exit", "3"]), Options::default()).expect("spawn");
    assert_eq!(drain(&bad).2, Some(3));
}

#[test]
fn over_long_line_is_reported_and_reading_continues() {
    let options = Options {
        max_line_bytes: 1000,
        ..Options::default()
    };
    let p = StreamingProcess::spawn(&spec(&["long-line", "100000"]), options).expect("spawn");
    let (lines, too_long, code) = drain(&p);
    assert_eq!(too_long, 1);
    assert_eq!(lines, vec!["after".to_string()]);
    assert_eq!(code, Some(0));
}

#[test]
fn large_stderr_does_not_block_and_only_the_tail_remains() {
    let options = Options {
        stderr_tail_bytes: 4096,
        ..Options::default()
    };
    let ten_mb = (10 * 1024 * 1024).to_string();
    let p = StreamingProcess::spawn(&spec(&["big-stderr", &ten_mb]), options).expect("spawn");
    let (lines, _, code) = drain(&p);
    assert_eq!(lines, vec!["done".to_string()]);
    assert_eq!(code, Some(0));
    let tail = p.stderr_tail();
    assert!(tail.text.len() <= 4096);
    assert!(tail.text.contains("KONIEC-STDERR"));
    assert!(tail.total_bytes >= 10 * 1024 * 1024);
}

#[test]
fn utf8_split_between_flushes_decodes_correctly() {
    let p = StreamingProcess::spawn(&spec(&["split-utf8"]), Options::default()).expect("spawn");
    assert_eq!(drain(&p).0, vec!["zażółć".to_string()]);
}

#[test]
fn process_that_ignores_stdin_still_finishes_with_a_large_prompt() {
    let mut s = spec(&["ignore-stdin-close"]);
    s.stdin = "x".repeat(5 * 1024 * 1024);
    let p = StreamingProcess::spawn(&s, Options::default()).expect("spawn");
    assert_eq!(drain(&p).0, vec!["ok".to_string()]);
}

#[test]
fn missing_program_is_not_found() {
    let mut s = spec(&[]);
    s.program = PathBuf::from("ten-program-nie-istnieje-agent-cli");
    assert!(matches!(
        StreamingProcess::spawn(&s, Options::default()),
        Err(SpawnError::NotFound(_))
    ));
}

#[test]
fn clear_env_leaves_only_the_given_variables() {
    let mut s = spec(&["print-env"]);
    s.env = vec![("TUKTUK_TRACK_WORKFLOW".into(), "1:e1".into())];
    std::env::set_var("SECRET_X_AGENT_CLI", "tajne");
    let options = Options {
        clear_env: true,
        ..Options::default()
    };
    let p = StreamingProcess::spawn(&s, options).expect("spawn");
    let names = drain(&p).0;
    assert!(names.contains(&"TUKTUK_TRACK_WORKFLOW".to_string()));
    assert!(!names.contains(&"SECRET_X_AGENT_CLI".to_string()));
}

#[test]
fn hang_is_killed_and_confirmed() {
    let p = StreamingProcess::spawn(&spec(&["hang"]), Options::default()).expect("spawn");
    assert_eq!(
        p.events().recv_timeout(WAIT).expect("start"),
        ProcessEvent::Line("start".into())
    );
    assert_eq!(p.kill(Duration::from_secs(10)), KillResult::Confirmed);
    assert!(p.has_exited());
    #[cfg(windows)]
    assert_dead_soon(p.pid());
}

#[cfg(windows)]
#[test]
fn killing_the_tree_removes_the_grandchild() {
    let dir = unique_dir("tree");
    let pidfile = dir.join("pid.txt");
    let p = StreamingProcess::spawn(
        &spec(&["spawn-child-and-hang", pidfile.to_str().expect("utf8")]),
        Options::default(),
    )
    .expect("spawn");
    let grandchild: u32 = wait_for_file(&pidfile).trim().parse().expect("pid");
    assert!(process_alive(grandchild));
    assert_eq!(p.kill(Duration::from_secs(10)), KillResult::Confirmed);
    assert_dead_soon(grandchild);
}

#[cfg(windows)]
#[test]
fn cmd_wrapper_in_a_path_with_spaces_keeps_args_and_the_tree_dies() {
    let dir = unique_dir("agent ż test");
    let wrapper_dir = dir.join("agent ż test");
    std::fs::create_dir_all(&wrapper_dir).expect("katalog");
    let wrapper = wrapper_dir.join("fake.cmd");
    std::fs::write(&wrapper, format!("@\"{FAKE}\" %*\r\n")).expect("wrapper");

    let mut s = spec(&["print-args", "a b", "c d"]);
    s.program = wrapper.clone();
    let p = StreamingProcess::spawn(&s, Options::default()).expect("spawn cmd");
    let (lines, _, code) = drain(&p);
    assert_eq!(lines, vec!["\"a b\"".to_string(), "\"c d\"".to_string()]);
    assert_eq!(code, Some(0));

    let pidfile = dir.join("pid.txt");
    let mut tree = spec(&["spawn-child-and-hang", pidfile.to_str().expect("utf8")]);
    tree.program = wrapper;
    let p = StreamingProcess::spawn(&tree, Options::default()).expect("spawn cmd");
    let grandchild: u32 = wait_for_file(&pidfile).trim().parse().expect("pid");
    assert_eq!(p.kill(Duration::from_secs(10)), KillResult::Confirmed);
    assert_dead_soon(grandchild);
}

/// Schemat w kształcie `json_schema(Review)` (agent-cli nie zależy od workflow-core).
const SCHEMA_ARG: &str = r#"{"additionalProperties":false,"properties":{"checkedArtifactHash":{"maxLength":20000,"type":"string"},"findings":{"items":{"additionalProperties":false,"properties":{"description":{"maxLength":20000,"type":"string"}},"required":["description"],"type":"object"},"maxItems":200,"type":"array"},"verdict":{"enum":["approved","changes_requested"],"type":"string"}},"required":["verdict","findings","checkedArtifactHash"],"type":"object"}"#;

#[cfg(windows)]
#[test]
fn cmd_wrapper_passes_quotes_and_the_json_schema_argument_unchanged() {
    let dir = unique_dir("agent cmd quotes");
    let wrapper = dir.join("fake quotes.cmd");
    std::fs::write(&wrapper, format!("@\"{FAKE}\" %*\r\n")).expect("wrapper");
    let args = [
        SCHEMA_ARG,
        r#"a "cytat" b"#,
        r#"{"mcpServers":{}}"#,
        "koniec\\",
    ];
    let mut s = spec(&["print-args"]);
    s.args.extend(args.iter().map(|a| a.to_string()));
    s.program = wrapper;
    // Rust escapuje argumenty .cmd (BatBadBut); schemat z cudzysłowami przechodzi bez zmian.
    let p = StreamingProcess::spawn(&s, Options::default()).expect("spawn cmd");
    let (lines, _, code) = drain(&p);
    let expected: Vec<String> = args
        .iter()
        .map(|a| serde_json::to_string(a).expect("json"))
        .collect();
    assert_eq!(lines, expected);
    assert_eq!(code, Some(0));
}

/// Kończenie drzewa po PID, które zawsze się nie udaje (jak `taskkill` bez uprawnień).
fn failing_kill_tree(_pid: u32) -> bool {
    false
}

#[test]
fn without_a_job_a_failed_tree_kill_is_unconfirmed() {
    let options = Options {
        attach_job: false,
        kill_tree: failing_kill_tree,
        ..Options::default()
    };
    let p = StreamingProcess::spawn(&spec(&["hang"]), options).expect("spawn");
    assert!(p.job_error().is_some());
    assert_eq!(
        p.events().recv_timeout(WAIT).expect("start"),
        ProcessEvent::Line("start".into())
    );
    // Bezpośredni proces kończy child.kill(), ale drzewa nie potwierdzono.
    assert_eq!(p.kill(Duration::from_secs(2)), KillResult::Unconfirmed);
}

#[cfg(windows)]
#[test]
fn without_a_job_a_successful_taskkill_confirms_and_removes_the_grandchild() {
    let dir = unique_dir("nojob");
    let pidfile = dir.join("pid.txt");
    let options = Options {
        attach_job: false,
        ..Options::default()
    };
    let p = StreamingProcess::spawn(
        &spec(&["spawn-child-and-hang", pidfile.to_str().expect("utf8")]),
        options,
    )
    .expect("spawn");
    let grandchild: u32 = wait_for_file(&pidfile).trim().parse().expect("pid");
    assert_eq!(p.kill(Duration::from_secs(10)), KillResult::Confirmed);
    assert_dead_soon(grandchild);
}

#[cfg(windows)]
#[test]
fn with_a_job_the_process_is_attached_and_kill_waits_for_an_empty_job() {
    let p = StreamingProcess::spawn(&spec(&["hang"]), Options::default()).expect("spawn");
    assert_eq!(p.job_error(), None);
    assert_eq!(p.kill(Duration::from_secs(10)), KillResult::Confirmed);
}

/// `Exited` mówi tylko o bezpośrednim procesie: po niepotwierdzonym `kill` zdarzenie
/// przychodzi, choć wnuk żyje, a ponowny `kill` nadal nie potwierdza końca drzewa.
#[cfg(windows)]
#[test]
fn exited_after_an_unconfirmed_kill_does_not_mean_the_tree_ended() {
    let dir = unique_dir("unconfirmed-tree");
    let pidfile = dir.join("pid.txt");
    let options = Options {
        attach_job: false,
        kill_tree: failing_kill_tree,
        ..Options::default()
    };
    let p = StreamingProcess::spawn(
        &spec(&["spawn-child-and-hang", pidfile.to_str().expect("utf8")]),
        options,
    )
    .expect("spawn");
    let grandchild: u32 = wait_for_file(&pidfile).trim().parse().expect("pid");
    assert_eq!(p.kill(Duration::from_secs(1)), KillResult::Unconfirmed);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "brak Exited");
        if let Ok(ProcessEvent::Exited(_)) = p.events().recv_timeout(Duration::from_millis(100)) {
            break;
        }
    }
    assert!(p.has_exited());
    assert!(process_alive(grandchild), "wnuk powinien jeszcze żyć");
    assert_eq!(p.kill(Duration::from_millis(500)), KillResult::Unconfirmed);
    // Sprzątanie: test nie zostawia wnuka.
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/PID", &grandchild.to_string()])
        .output();
    assert_dead_soon(grandchild);
}
