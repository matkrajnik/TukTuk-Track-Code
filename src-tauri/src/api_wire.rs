//! Zewnętrzne sterowanie pomiarem: protokół między innym programem a aplikacją.
//!
//! Wspólny plik aplikacji (`mod api_wire`) i programu `tuktuk-track-ctl`
//! (`src/bin/ctl.rs`, przez `#[path]`). Żądanie to obiekt JSON z polem `action`
//! (`add`, `stop`, `status`) w ramce `native_msg` (4 bajty długości + JSON),
//! wysłany nazwanym potokiem `\\.\pipe\tuktuk-track-api-<użytkownik>`. Odpowiedź
//! ma zawsze kopertę `{"ok":true,"data":{…}}` albo
//! `{"ok":false,"code":"…","error":"komunikat po polsku"}`. Logika żądań: `api.rs`.
//!
//! Nic nie trafia na dysk ani do logów.
#![allow(dead_code)]

use crate::native_msg;
use serde_json::{json, Map, Value};

/// Kody błędów w polu `code`.
pub mod code {
    /// Zewnętrzne sterowanie wyłączone w Ustawieniach.
    pub const DISABLED: &str = "DISABLED";
    /// Niepoprawny JSON, nieznana akcja albo pole.
    pub const BAD_REQUEST: &str = "BAD_REQUEST";
    /// Aplikacja odrzuciła dane (pusty tytuł, niepoprawny klucz Jiry itp.).
    pub const INVALID: &str = "INVALID";
    /// Błąd zapisu albo odczytu bazy.
    pub const STORAGE: &str = "STORAGE";
    /// Aplikacja nie działa (tylko klient; potok nie istnieje).
    pub const NOT_RUNNING: &str = "NOT_RUNNING";
    /// Potok istnieje, ale połączenie się nie udało (zajęty, przerwany, brak dostępu).
    pub const UNAVAILABLE: &str = "UNAVAILABLE";
}

/// Akcje przyjmowane przez aplikację.
pub const ACTIONS: [&str; 3] = ["add", "stop", "status"];

pub const NOT_RUNNING: &str = "TukTuk Track nie działa. Uruchom aplikację i spróbuj ponownie.";

/// Kody wyjścia `tuktuk-track-ctl`.
pub mod exit {
    pub const OK: i32 = 0;
    /// Aplikacja odpowiedziała błędem (`ok: false`).
    pub const REJECTED: i32 = 1;
    /// Niepoprawne wywołanie programu.
    pub const USAGE: i32 = 2;
    /// Aplikacja nie działa.
    pub const NOT_RUNNING: i32 = 3;
}

pub const USAGE: &str = "\
tuktuk-track-ctl: sterowanie pomiarem w działającym TukTuk Track.

Użycie:
  tuktuk-track-ctl status
  tuktuk-track-ctl stop
  tuktuk-track-ctl add <tytuł> [--start] [--jira KLUCZ] [--redmine NUMER]
  tuktuk-track-ctl --json '{\"action\":\"status\"}'

Odpowiedź to jeden wiersz JSON na stdout:
  {\"ok\":true,\"data\":{...}} albo {\"ok\":false,\"code\":\"...\",\"error\":\"...\"}
Kod wyjścia: 0 = ok, 1 = błąd z aplikacji, 2 = złe wywołanie, 3 = aplikacja nie działa albo nie odpowiada.
Wymaga włączenia: Ustawienia → Agenci → Zewnętrzne aplikacje.";

/// `\\.\pipe\tuktuk-track-api-<użytkownik>`.
pub fn pipe_name_for(user: &str) -> String {
    format!(
        r"\\.\pipe\tuktuk-track-api-{}",
        native_msg::sanitize_user(user)
    )
}

/// Potok bieżącego użytkownika (zmienna `USERNAME`).
pub fn pipe_name() -> String {
    pipe_name_for(&std::env::var("USERNAME").unwrap_or_default())
}

/// Odpowiedź udana.
pub fn ok(data: Value) -> Value {
    json!({ "ok": true, "data": data })
}

/// Odpowiedź z błędem.
pub fn error(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "code": code, "error": message.into() })
}

/// Wywołanie programu po rozpoznaniu argumentów.
#[derive(Debug, PartialEq)]
pub enum Cli {
    Help,
    Request(Value),
}

/// Argumenty `tuktuk-track-ctl` (bez nazwy programu) → żądanie do aplikacji.
pub fn parse_args(args: &[String]) -> Result<Cli, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("Brak polecenia.".into());
    };
    match command.as_str() {
        "help" | "--help" | "-h" | "/?" => Ok(Cli::Help),
        "status" | "stop" if rest.is_empty() => Ok(Cli::Request(json!({ "action": command }))),
        "status" | "stop" => Err(format!("Polecenie {command} nie przyjmuje argumentów.")),
        "add" => parse_add(rest).map(Cli::Request),
        "--json" => match rest {
            [raw] => match serde_json::from_str::<Value>(raw) {
                Ok(value @ Value::Object(_)) => Ok(Cli::Request(value)),
                _ => Err("--json wymaga obiektu JSON.".into()),
            },
            _ => Err("--json wymaga jednego argumentu.".into()),
        },
        other => Err(format!("Nieznane polecenie „{other}”.")),
    }
}

fn parse_add(args: &[String]) -> Result<Value, String> {
    let mut request = Map::new();
    request.insert("action".into(), json!("add"));
    let mut words: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--start" => {
                request.insert("start".into(), json!(true));
            }
            "--jira" => {
                let key = it.next().ok_or("--jira wymaga klucza zgłoszenia.")?;
                request.insert("jiraKey".into(), json!(key));
            }
            "--redmine" => {
                let raw = it.next().ok_or("--redmine wymaga numeru zgłoszenia.")?;
                let id: u64 = raw
                    .trim_start_matches('#')
                    .parse()
                    .map_err(|_| format!("Niepoprawny numer zgłoszenia Redmine „{raw}”."))?;
                request.insert("redmineId".into(), json!(id));
            }
            "--" => words.extend(it.by_ref().map(String::as_str)),
            flag if flag.starts_with("--") => return Err(format!("Nieznana opcja „{flag}”.")),
            word => words.push(word),
        }
    }
    request.insert("title".into(), json!(words.join(" ")));
    Ok(Value::Object(request))
}

/// Wysyła żądanie potokiem `pipe` i zwraca odpowiedź aplikacji. Brak potoku
/// (aplikacja nie działa) i niepoprawna odpowiedź też dają kopertę z błędem.
pub fn call(pipe: &str, request: &Value) -> Value {
    let body = serde_json::to_vec(request).unwrap_or_default();
    match native_msg::exchange(pipe, &body) {
        Ok(reply) => match serde_json::from_slice::<Value>(&reply) {
            Ok(value @ Value::Object(_)) => value,
            _ => error(code::STORAGE, "Niepoprawna odpowiedź aplikacji."),
        },
        Err(native_msg::FrameError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            error(code::NOT_RUNNING, NOT_RUNNING)
        }
        Err(e) => error(
            code::UNAVAILABLE,
            format!("TukTuk Track nie odpowiada ({e}). Spróbuj ponownie."),
        ),
    }
}

/// Kod wyjścia programu dla odpowiedzi.
pub fn exit_code(reply: &Value) -> i32 {
    if reply["ok"] == true {
        exit::OK
    } else if reply["code"] == code::NOT_RUNNING || reply["code"] == code::UNAVAILABLE {
        exit::NOT_RUNNING
    } else {
        exit::REJECTED
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_status_stop_and_help() {
        assert_eq!(
            parse_args(&args(&["status"])),
            Ok(Cli::Request(json!({"action": "status"})))
        );
        assert_eq!(
            parse_args(&args(&["stop"])),
            Ok(Cli::Request(json!({"action": "stop"})))
        );
        assert_eq!(parse_args(&args(&["--help"])), Ok(Cli::Help));
        assert!(parse_args(&args(&[])).is_err());
        assert!(parse_args(&args(&["stop", "teraz"])).is_err());
        assert!(parse_args(&args(&["start"])).is_err());
    }

    #[test]
    fn parses_add_with_words_and_flags() {
        assert_eq!(
            parse_args(&args(&[
                "add",
                "Przegląd",
                "PR",
                "--start",
                "--jira",
                "WMS-12",
                "--",
                "--nie-opcja"
            ])),
            Ok(Cli::Request(json!({
                "action": "add",
                "title": "Przegląd PR --nie-opcja",
                "start": true,
                "jiraKey": "WMS-12",
            })))
        );
        assert_eq!(
            parse_args(&args(&["add", "--redmine", "#1234"])),
            Ok(Cli::Request(
                json!({"action": "add", "title": "", "redmineId": 1234})
            ))
        );
        assert!(parse_args(&args(&["add", "X", "--redmine", "abc"])).is_err());
        assert!(parse_args(&args(&["add", "X", "--jira"])).is_err());
        assert!(parse_args(&args(&["add", "X", "--force"])).is_err());
    }

    #[test]
    fn parses_raw_json_object_only() {
        assert_eq!(
            parse_args(&args(&["--json", r#"{"action":"status"}"#])),
            Ok(Cli::Request(json!({"action": "status"})))
        );
        assert!(parse_args(&args(&["--json", "[1]"])).is_err());
        assert!(parse_args(&args(&["--json"])).is_err());
    }

    #[test]
    fn pipe_name_contains_sanitized_user() {
        assert_eq!(
            pipe_name_for("Jan Kowalski"),
            r"\\.\pipe\tuktuk-track-api-Jan_Kowalski"
        );
    }

    #[test]
    fn exit_codes_follow_the_envelope() {
        assert_eq!(exit_code(&ok(json!({}))), exit::OK);
        assert_eq!(exit_code(&error(code::INVALID, "x")), exit::REJECTED);
        assert_eq!(exit_code(&error(code::NOT_RUNNING, "x")), exit::NOT_RUNNING);
        assert_eq!(exit_code(&error(code::UNAVAILABLE, "x")), exit::NOT_RUNNING);
    }

    #[test]
    fn missing_pipe_means_not_running() {
        let pipe = pipe_name_for(&format!("api-missing-{}", std::process::id()));
        let reply = call(&pipe, &json!({"action": "status"}));
        assert_eq!(reply["code"], code::NOT_RUNNING);
        assert_eq!(reply["error"], NOT_RUNNING);
    }
}
