//! Preflight dostawców i profili dostępu (PLAN_WDROZENIA.md, 8.3, E4).
//!
//! `preflight` jest czysta: dostaje wykryte CLI i tekst `config.toml` Codexa.
//! Nie wywołuje modeli, więc zalogowania nie sprawdza (wychodzi przy pierwszym
//! kroku jako `NotLoggedIn`).
#![cfg_attr(not(test), allow(dead_code))]

use std::path::PathBuf;

use serde::Serialize;
use workflow_core::model::{AccessProfile, NodeKind, Provider};
use workflow_core::{FailureCode, WorkflowGraph};

use crate::ai::Detected;

/// Przetestowane wersje (E0): Claude Code 2.1.318, codex-cli 0.163.0.
const TESTED_CLAUDE: [u64; 3] = [2, 1, 318];
const TESTED_CODEX: [u64; 3] = [0, 163, 0];

/// Dozwolone wartości `[windows] sandbox` w config.toml Codexa. Domyślnie tylko
/// `elevated` (jednorazowa konfiguracja administratora); lista do decyzji użytkownika.
pub const ALLOWED_CODEX_SANDBOXES: [&str; 1] = ["elevated"];

const WARN_UNVERIFIED: &str = "niezweryfikowana wersja";
const MSG_CLAUDE_WRITE: &str =
    "Claude nie ma w MVP egzekwowanego ograniczenia zapisu do katalogu roboczego.";
const MSG_CODEX_NO_SANDBOX: &str = "Codex nie ma skonfigurowanego sandboxa Windows. Ustaw [windows] sandbox = \"elevated\" w config.toml Codexa.";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Enforcement {
    /// Claude read-only: narzędzia tylko do odczytu + wykrywanie zmian.
    CliControl,
    CodexSandbox,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Blocked {
    pub code: FailureCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeReadiness {
    pub node_id: String,
    pub provider: Provider,
    pub access: AccessProfile,
    pub cli_found: bool,
    pub version: Option<String>,
    pub supported: bool,
    pub enforcement: Enforcement,
    pub warning: Option<String>,
    pub blocked: Option<Blocked>,
}

fn parse_semver(text: &str) -> Option<[u64; 3]> {
    let mut parts = text
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u64>().ok());
    Some([
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ])
}

/// `Ok(ostrzeżenie)` dla wersji obsługiwanej, `Err(komunikat)` dla nieobsługiwanej.
fn check_version(provider: Provider, version: &str) -> Result<Option<String>, String> {
    let tested = match provider {
        Provider::Claude => TESTED_CLAUDE,
        Provider::Codex => TESTED_CODEX,
    };
    let Some(found) = parse_semver(version) else {
        return Err(format!("Nie rozpoznano wersji programu agenta: {version}."));
    };
    if found[0] != tested[0] || found < tested {
        return Err(format!(
            "Wersja {version} nie jest obsługiwana (przetestowano {}.{}.{} i nowsze z tej samej serii).",
            tested[0], tested[1], tested[2]
        ));
    }
    Ok((found > tested).then(|| WARN_UNVERIFIED.to_owned()))
}

/// `Ok(true)`, gdy config ma dozwoloną wartość `[windows] sandbox`.
pub fn codex_sandbox_configured(config: Option<&str>) -> Result<bool, String> {
    let Some(text) = config else {
        return Ok(false);
    };
    let value: toml::Value = text
        .parse()
        .map_err(|_| "Plik config.toml Codexa jest uszkodzony (niepoprawny TOML).".to_owned())?;
    Ok(value
        .get("windows")
        .and_then(|w| w.get("sandbox"))
        .and_then(|s| s.as_str())
        .is_some_and(|s| ALLOWED_CODEX_SANDBOXES.contains(&s)))
}

fn block(code: FailureCode, message: impl Into<String>) -> Option<Blocked> {
    Some(Blocked {
        code,
        message: message.into(),
    })
}

fn readiness(
    node_id: &str,
    provider: Provider,
    access: AccessProfile,
    detected: &Detected,
    codex_config: Result<Option<&str>, &str>,
) -> NodeReadiness {
    let cli = match provider {
        Provider::Claude => detected.claude.as_ref(),
        Provider::Codex => detected.codex.as_ref(),
    };
    let mut out = NodeReadiness {
        node_id: node_id.to_owned(),
        provider,
        access,
        cli_found: cli.is_some(),
        version: cli.map(|c| c.version.clone()),
        supported: false,
        enforcement: Enforcement::None,
        warning: None,
        blocked: None,
    };
    let Some(cli) = cli else {
        out.blocked = block(FailureCode::CliMissing, FailureCode::CliMissing.message());
        return out;
    };
    match check_version(provider, &cli.version) {
        Ok(warning) => {
            out.supported = true;
            out.warning = warning;
        }
        Err(message) => {
            out.blocked = block(FailureCode::UnsupportedVersion, message);
            return out;
        }
    }
    match (provider, access) {
        (Provider::Claude, AccessProfile::WorkspaceWrite) => {
            out.blocked = block(FailureCode::PermissionViolation, MSG_CLAUDE_WRITE);
        }
        (Provider::Claude, AccessProfile::ReadOnly) => out.enforcement = Enforcement::CliControl,
        (Provider::Codex, _) => match codex_config
            .map_err(str::to_owned)
            .and_then(codex_sandbox_configured)
        {
            Ok(true) => out.enforcement = Enforcement::CodexSandbox,
            Ok(false) => {
                out.blocked = block(FailureCode::PermissionViolation, MSG_CODEX_NO_SANDBOX)
            }
            Err(message) => out.blocked = block(FailureCode::PermissionViolation, message),
        },
    }
    out
}

/// Gotowość każdego węzła Agent grafu.
pub fn preflight(
    graph: &WorkflowGraph,
    detected: &Detected,
    codex_config: Option<&str>,
) -> Vec<NodeReadiness> {
    evaluate(graph, detected, Ok(codex_config))
}

/// Czy graf ma węzeł Agent z Codexem.
pub fn uses_codex(graph: &WorkflowGraph) -> bool {
    graph
        .nodes
        .iter()
        .any(|n| matches!(&n.kind, NodeKind::Agent(c) if c.provider == Provider::Codex))
}

/// Jak [`preflight`], ale config.toml czyta `read` i tylko wtedy, gdy graf używa
/// Codexa. Błąd odczytu blokuje węzły Codexa, nie całe sprawdzenie.
pub fn preflight_reading_config(
    graph: &WorkflowGraph,
    detected: &Detected,
    read: impl FnOnce() -> Result<Option<String>, String>,
) -> Vec<NodeReadiness> {
    if !uses_codex(graph) {
        return evaluate(graph, detected, Ok(None));
    }
    match read() {
        Ok(text) => evaluate(graph, detected, Ok(text.as_deref())),
        Err(message) => evaluate(graph, detected, Err(&message)),
    }
}

fn evaluate(
    graph: &WorkflowGraph,
    detected: &Detected,
    codex_config: Result<Option<&str>, &str>,
) -> Vec<NodeReadiness> {
    graph
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::Agent(config) => Some(readiness(
                &node.id,
                config.provider,
                config.access,
                detected,
                codex_config,
            )),
            _ => None,
        })
        .collect()
}

/// `%CODEX_HOME%\config.toml` albo `%USERPROFILE%\.codex\config.toml`.
pub fn codex_config_path() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("CODEX_HOME") {
        return Some(PathBuf::from(home).join("config.toml"));
    }
    std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".codex").join("config.toml"))
}

/// Tekst config.toml; brak pliku to `Ok(None)`, inny błąd odczytu to komunikat.
pub fn read_codex_config() -> Result<Option<String>, String> {
    let Some(path) = codex_config_path() else {
        return Ok(None);
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Nie można odczytać config.toml Codexa: {e}")),
    }
}

#[cfg(test)]
mod tests;
