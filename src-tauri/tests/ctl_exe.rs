//! `tuktuk-track-ctl.exe` jako proces bez działającej aplikacji. Nazwa
//! użytkownika w środowisku jest unikalna, więc potok aplikacji (także
//! prawdziwej, jeśli działa) nie istnieje.
use serde_json::Value;
use std::process::Command;

fn ctl(args: &[&str]) -> (i32, Value) {
    let output = Command::new(env!("CARGO_BIN_EXE_tuktuk-track-ctl"))
        .args(args)
        .env("USERNAME", format!("ctl-exe-test-{}", std::process::id()))
        .output()
        .expect("nie udało się uruchomić tuktuk-track-ctl");
    let reply = serde_json::from_slice(&output.stdout).unwrap_or(Value::Null);
    (output.status.code().unwrap_or(-1), reply)
}

#[test]
fn reports_that_the_app_is_not_running() {
    let (code, reply) = ctl(&["status"]);
    assert_eq!(code, 3);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["code"], "NOT_RUNNING");
}

#[test]
fn rejects_bad_usage_with_exit_code_2() {
    let (code, reply) = ctl(&["start", "X"]);
    assert_eq!(code, 2);
    assert_eq!(reply["code"], "BAD_REQUEST");
}

#[test]
fn help_exits_with_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_tuktuk-track-ctl"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("tuktuk-track-ctl add"));
}
