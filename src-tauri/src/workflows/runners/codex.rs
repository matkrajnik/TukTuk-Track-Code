//! Argumenty Codexa dla kroku. Pliki schematu i wyniku leżą poza worktree (7.2).

use std::path::Path;

use workflow_core::model::AccessProfile;

/// Argumenty `codex exec`; prompt idzie na stdin (`-`). `Err`, gdy plik schematu
/// albo wyniku leży w worktree użytkownika.
pub fn args(
    worktree: &Path,
    schema_file: &Path,
    out_file: &Path,
    model: Option<&str>,
    access: AccessProfile,
) -> Result<Vec<String>, String> {
    for file in [schema_file, out_file] {
        if file.starts_with(worktree) {
            return Err("Plik pomocniczy agenta nie może leżeć w katalogu roboczym.".into());
        }
    }
    let sandbox = match access {
        AccessProfile::ReadOnly => "read-only",
        AccessProfile::WorkspaceWrite => "workspace-write",
    };
    let mut args: Vec<String> = vec!["exec".into(), "--json".into(), "-C".into()];
    args.push(worktree.to_string_lossy().into_owned());
    args.extend(["-s".into(), sandbox.into()]);
    args.extend([
        "-c".into(),
        "mcp_servers={}".into(),
        "-c".into(),
        "notify=[]".into(),
    ]);
    args.push("--output-schema".into());
    args.push(schema_file.to_string_lossy().into_owned());
    args.push("-o".into());
    args.push(out_file.to_string_lossy().into_owned());
    if let Some(model) = model.filter(|m| !m.trim().is_empty()) {
        args.push("-m".into());
        args.push(model.to_owned());
    }
    args.push("-".into());
    Ok(args)
}
