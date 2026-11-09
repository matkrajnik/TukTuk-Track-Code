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
//!
//! Orkiestrator (tryb dynamiczny, E8b; rozpoznanie na Claude Code 2.1.319, zob.
//! docs/workflows/E8_ROZPOZNANIE.md): zawsze `--restricted` (także read-only),
//! do `--tools` dochodzi `Agent` (subagenci dziedziczą `--restricted` i `--tools`,
//! więc nie mają powłoki i piszą tylko w katalogu roboczym), a
//! `--forward-subagent-text` przekazuje tekst subagentów do dziennika.
//! Narzędzia `Workflow` nie dodajemy nigdy: w `-p` z `dontAsk` jest odrzucane.

use serde_json::Value;
use workflow_core::model::{AccessProfile, AgentRole};

/// Pusta konfiguracja MCP: agent nie dostaje żadnych serwerów.
pub const EMPTY_MCP: &str = r#"{"mcpServers":{}}"#;

/// Narzędzie delegowania do subagentów (w `system/init` bywa raportowane jako `Task`).
pub const SUBAGENT_TOOL: &str = "Agent";

/// Argumenty bez promptu (prompt idzie przez stdin).
pub fn args(
    schema: &Value,
    session_id: &str,
    model: Option<&str>,
    access: AccessProfile,
    role: AgentRole,
) -> Vec<String> {
    let orchestrator = role == AgentRole::Orchestrator;
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
    let tools = |base: &str| {
        if orchestrator {
            format!("{base},{SUBAGENT_TOOL}")
        } else {
            base.to_owned()
        }
    };
    let profile: Vec<String> = match access {
        AccessProfile::ReadOnly => {
            let mut profile = Vec::new();
            if orchestrator {
                profile.push("--restricted".to_owned());
            }
            profile.extend([
                "--tools".to_owned(),
                tools("Read,Grep,Glob"),
                "--permission-mode".to_owned(),
                "dontAsk".to_owned(),
            ]);
            profile
        }
        AccessProfile::WorkspaceWrite => vec![
            "--restricted".to_owned(),
            "--tools".to_owned(),
            tools("Read,Grep,Glob,Edit,Write"),
            "--permission-mode".to_owned(),
            "acceptEdits".to_owned(),
        ],
    };
    args.extend(profile);
    if orchestrator {
        args.push("--forward-subagent-text".to_owned());
    }
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
