//! Argumenty Claude Code dla kroku (zaakceptowany zestaw flag, bloker 1 z E0).
//! Tryb read-only to „kontrola CLI” (narzędzia tylko do odczytu + wykrywanie
//! zmian worktree przez `GuardedRunner`), nie izolacja systemu plików.
//!
//! Krok z zapisem (decyzja użytkownika 2026-11-06, Claude Code 2.1.319+):
//! `--restricted` usuwa narzędzia uruchamiające kod (Bash, PowerShell) i WebFetch,
//! pomija ustawienia user/project/local, ogranicza narzędzia plików do katalogu
//! roboczego (cwd; bez `--add-dir`) i odmawia `bypassPermissions`. Narzędzia to
//! jawnie `Read,Grep,Glob,Edit,Write` (świadomie bez Bash: agent nie uruchomi
//! testów), `acceptEdits` zatwierdza edycje plików, a `--permission-prompts none`
//! odrzuca to, co wymagałoby człowieka (np. zapis ustawień, konfiguracji Git).
//! To nadal kontrola CLI, nie sandbox systemu.
//!
//! Read-only zostaje przy zestawie zweryfikowanym w E4 (bez `--restricted`):
//! `--tools Read,Grep,Glob` i tak nie daje narzędzi uruchamiających kod, a zmiana
//! zestawu wymagałaby ponownej weryfikacji na starszych wersjach (2.1.318).

use serde_json::Value;
use workflow_core::model::AccessProfile;

/// Pusta konfiguracja MCP: agent nie dostaje żadnych serwerów.
pub const EMPTY_MCP: &str = r#"{"mcpServers":{}}"#;

/// Argumenty bez promptu (prompt idzie przez stdin).
pub fn args(
    schema: &Value,
    session_id: &str,
    model: Option<&str>,
    access: AccessProfile,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        "--json-schema",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    args.push(schema.to_string());
    args.extend(
        [
            "--session-id",
            session_id,
            "--strict-mcp-config",
            "--mcp-config",
            EMPTY_MCP,
            "--permission-prompts",
            "none",
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    let profile: &[&str] = match access {
        AccessProfile::ReadOnly => &["--tools", "Read,Grep,Glob", "--permission-mode", "dontAsk"],
        AccessProfile::WorkspaceWrite => &[
            "--restricted",
            "--tools",
            "Read,Grep,Glob,Edit,Write",
            "--permission-mode",
            "acceptEdits",
        ],
    };
    args.extend(profile.iter().map(|s| (*s).to_owned()));
    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        args.push("--model".into());
        args.push(model.to_owned());
    }
    args
}

/// UUID v4 z systemowego generatora losowego.
pub fn new_session_id() -> Result<String, String> {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).map_err(|e| format!("Brak losowości systemowej: {e}"))?;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    ))
}
