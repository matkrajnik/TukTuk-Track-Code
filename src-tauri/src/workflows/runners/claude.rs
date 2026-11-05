//! Argumenty Claude Code dla kroku (zaakceptowany zestaw flag, bloker 1 z E0).
//! Tryb read-only to „kontrola CLI” (narzędzia tylko do odczytu + wykrywanie
//! zmian worktree przez `GuardedRunner`), nie izolacja systemu plików.

use serde_json::Value;
use workflow_core::model::AccessProfile;

/// Pusta konfiguracja MCP: agent nie dostaje żadnych serwerów.
pub const EMPTY_MCP: &str = r#"{"mcpServers":{}}"#;

/// Argumenty bez promptu. `None`, gdy profil nie jest dostępny dla Claude (zapis do workspace).
pub fn args(
    schema: &Value,
    session_id: &str,
    model: Option<&str>,
    access: AccessProfile,
) -> Option<Vec<String>> {
    if access == AccessProfile::WorkspaceWrite {
        return None;
    }
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
            "--tools",
            "Read,Grep,Glob",
            "--permission-mode",
            "dontAsk",
        ]
        .iter()
        .map(|s| (*s).to_owned()),
    );
    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        args.push("--model".into());
        args.push(model.to_owned());
    }
    Some(args)
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
