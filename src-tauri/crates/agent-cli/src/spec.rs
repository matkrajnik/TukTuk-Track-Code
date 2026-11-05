use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Polecenie do uruchomienia.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Treść na stdin (prompt).
    pub stdin: String,
    /// Katalog roboczy (`None` = bieżący katalog aplikacji).
    pub cwd: Option<PathBuf>,
    /// Dodatkowe zmienne środowiska procesu.
    pub env: Vec<(String, String)>,
}

/// CREATE_NO_WINDOW: bez migającego okna konsoli.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Polecenie z potokami, katalogiem, środowiskiem i bez okna konsoli. Bez powłoki
/// i bez sklejania argumentów.
pub fn command(spec: &Spec) -> Command {
    command_with(spec, false)
}

/// Jak [`command`]; `clear_env` czyści środowisko procesu przed dodaniem `spec.env`.
pub fn command_with(spec: &Spec, clear_env: bool) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if clear_env {
        command.env_clear();
    }
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
