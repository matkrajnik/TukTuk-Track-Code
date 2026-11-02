//! `tuktuk-track-ctl.exe`: sterowanie pomiarem z innych programów i skryptów.
//!
//! `tuktuk-track-ctl add "Tytuł" --start`, `stop`, `status` (albo `--json '{…}'`)
//! wysyła jedno żądanie do działającej aplikacji nazwanym potokiem
//! `\\.\pipe\tuktuk-track-api-<użytkownik>` (protokół: `api_wire.rs`) i wypisuje
//! odpowiedź jako jeden wiersz JSON. Kod wyjścia: `api_wire::exit`.
//!
//! Nic nie trafia na dysk ani do logów.
#[path = "../api_wire.rs"]
mod api_wire;
#[path = "../native_msg.rs"]
mod native_msg;

use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let (reply, code) = match api_wire::parse_args(&args) {
        Ok(api_wire::Cli::Help) => {
            println!("{}", api_wire::USAGE);
            std::process::exit(api_wire::exit::OK);
        }
        Ok(api_wire::Cli::Request(request)) => {
            let reply = api_wire::call(&api_wire::pipe_name(), &request);
            let code = api_wire::exit_code(&reply);
            (reply, code)
        }
        Err(message) => {
            eprintln!("{}", api_wire::USAGE);
            (
                api_wire::error(api_wire::code::BAD_REQUEST, message),
                api_wire::exit::USAGE,
            )
        }
    };
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{reply}");
    let _ = out.flush();
    std::process::exit(code);
}
