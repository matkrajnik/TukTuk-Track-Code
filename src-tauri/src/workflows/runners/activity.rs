//! Argumenty narzędzi agenta do dziennika (E8.1): tylko allowlista kluczy,
//! ścieżki względem katalogu roboczego, wartości po `redact` i z limitem długości.
//!
//! Nigdy nie trafiają tu `prompt`, `content`, `old_string`, `new_string`,
//! `command` ani żaden nieznany klucz: dziennik pokazuje, co agent robi,
//! a nie treść plików i poleceń.

use std::path::Path;

use serde_json::{Map, Value};

use super::redact;

/// Najwięcej znaków jednej wartości argumentu w dzienniku.
pub const MAX_ARG_CHARS: usize = 200;

/// Klucze dozwolone dla narzędzia (pozostałe narzędzia, także MCP i Codex: żadne).
fn allowed_keys(tool: &str) -> &'static [&'static str] {
    match tool {
        "Read" | "Edit" | "Write" => &["file_path"],
        "Grep" => &["pattern", "path", "glob"],
        "Glob" => &["pattern", "path"],
        "Agent" | "Task" => &["description", "subagent_type"],
        _ => &[],
    }
}

fn is_path_key(key: &str) -> bool {
    matches!(key, "file_path" | "path")
}

fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_ARG_CHARS {
        return text.to_owned();
    }
    let mut short: String = text.chars().take(MAX_ARG_CHARS).collect();
    short.push('…');
    short
}

fn normalize(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    let trimmed = slashed.trim_end_matches('/');
    if trimmed.is_empty() && slashed.starts_with('/') {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn is_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with('/')
        || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
}

fn has_parent_segment(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}

/// Ścieżka do dziennika: `(tekst, poza_katalogiem)`. Spoza katalogu roboczego
/// zostaje tylko nazwa pliku.
fn display_path(raw: &str, worktree: &Path) -> (String, bool) {
    let path = normalize(raw);
    let root = normalize(&worktree.to_string_lossy());
    let outside = |path: &str| {
        let name = path.rsplit('/').next().unwrap_or(path);
        (name.to_owned(), true)
    };
    if has_parent_segment(&path) {
        return outside(&path);
    }
    if !is_absolute(&path) {
        return (path, false);
    }
    // Bez katalogu roboczego pusty prefiks pasowałby do każdej ścieżki.
    if root.is_empty() {
        return outside(&path);
    }
    let root_chars = root.chars().count();
    let prefix: String = path.chars().take(root_chars).collect();
    let rest = &path[prefix.len()..];
    if prefix.to_lowercase() != root.to_lowercase() {
        return outside(&path);
    }
    if rest.is_empty() {
        return (".".to_owned(), false);
    }
    match rest.strip_prefix('/') {
        Some(inside) if !inside.is_empty() => (inside.to_owned(), false),
        _ => outside(&path),
    }
}

/// Argumenty narzędzia po allowliście albo `None`, gdy nic nie zostaje.
pub fn tool_args(name: &str, input: &Value, worktree: &Path) -> Option<Value> {
    let mut out = Map::new();
    let mut outside = false;
    for key in allowed_keys(name) {
        let Some(raw) = input.get(*key).and_then(Value::as_str) else {
            continue;
        };
        let text = if is_path_key(key) {
            let (shown, is_outside) = display_path(raw, worktree);
            outside |= is_outside;
            shown
        } else {
            raw.to_owned()
        };
        let text = cut(&redact(&text));
        if !text.is_empty() {
            out.insert((*key).to_owned(), Value::String(text));
        }
    }
    if out.is_empty() {
        return None;
    }
    if outside {
        out.insert("outside".into(), Value::Bool(true));
    }
    Some(Value::Object(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wt() -> &'static Path {
        Path::new("C:\\Projekty\\repo")
    }

    #[test]
    fn args_allowlist_drops_prompt_and_unknown_keys() {
        let agent = tool_args(
            "Agent",
            &json!({"description": "Czytaj", "subagent_type": "general-purpose",
                    "prompt": "SEKRETNY PROMPT", "extra": "x"}),
            wt(),
        )
        .unwrap();
        assert_eq!(
            agent,
            json!({"description": "Czytaj", "subagent_type": "general-purpose"})
        );
        let edit = tool_args(
            "Edit",
            &json!({"file_path": "src/a.ts", "old_string": "stare", "new_string": "nowe"}),
            wt(),
        )
        .unwrap();
        assert_eq!(edit, json!({"file_path": "src/a.ts"}));
        let write = tool_args(
            "Write",
            &json!({"file_path": "a.txt", "content": "treść pliku"}),
            wt(),
        )
        .unwrap();
        assert_eq!(write, json!({"file_path": "a.txt"}));
        assert_eq!(tool_args("Bash", &json!({"command": "ls"}), wt()), None);
        assert_eq!(
            tool_args("mcp__x__y", &json!({"file_path": "a"}), wt()),
            None
        );
        assert_eq!(tool_args("Read", &json!({"file_path": 5}), wt()), None);
        assert_eq!(tool_args("Read", &json!({"file_path": ""}), wt()), None);
        let grep = tool_args(
            "Grep",
            &json!({"pattern": "foo", "path": "src", "glob": "*.ts", "output_mode": "content"}),
            wt(),
        )
        .unwrap();
        assert_eq!(
            grep,
            json!({"pattern": "foo", "path": "src", "glob": "*.ts"})
        );
    }

    #[test]
    fn paths_outside_worktree_are_marked() {
        let inside = tool_args(
            "Read",
            &json!({"file_path": "C:\\Projekty\\repo\\src\\a.ts"}),
            wt(),
        )
        .unwrap();
        assert_eq!(inside, json!({"file_path": "src/a.ts"}));
        let other_case = tool_args(
            "Read",
            &json!({"file_path": "c:/projekty/REPO/src/a.ts"}),
            wt(),
        )
        .unwrap();
        assert_eq!(other_case, json!({"file_path": "src/a.ts"}));
        let relative = tool_args("Read", &json!({"file_path": "src/a.ts"}), wt()).unwrap();
        assert_eq!(relative, json!({"file_path": "src/a.ts"}));
        let outside = tool_args("Read", &json!({"file_path": "C:\\inne\\x.txt"}), wt()).unwrap();
        assert_eq!(outside, json!({"file_path": "x.txt", "outside": true}));
        let parent = tool_args("Read", &json!({"file_path": "..\\poza.txt"}), wt()).unwrap();
        assert_eq!(parent, json!({"file_path": "poza.txt", "outside": true}));
        let sneaky = tool_args(
            "Read",
            &json!({"file_path": "C:\\Projekty\\repo\\..\\sekret\\k.txt"}),
            wt(),
        )
        .unwrap();
        assert_eq!(sneaky, json!({"file_path": "k.txt", "outside": true}));
        let sibling = tool_args(
            "Read",
            &json!({"file_path": "C:\\Projekty\\repo2\\a.txt"}),
            wt(),
        )
        .unwrap();
        assert_eq!(sibling, json!({"file_path": "a.txt", "outside": true}));
        let grep_dir =
            tool_args("Grep", &json!({"pattern": "x", "path": "/etc/ssh"}), wt()).unwrap();
        assert_eq!(grep_dir["path"], "ssh");
        assert_eq!(grep_dir["outside"], true);
    }

    #[test]
    fn empty_worktree_marks_every_absolute_path_outside() {
        let none = Path::new("");
        let unix = tool_args("Read", &json!({"file_path": "/etc/ssh/x"}), none).unwrap();
        assert_eq!(unix, json!({"file_path": "x", "outside": true}));
        let windows = tool_args("Read", &json!({"file_path": "C:\\inne\\k.txt"}), none).unwrap();
        assert_eq!(windows, json!({"file_path": "k.txt", "outside": true}));
        let relative = tool_args("Read", &json!({"file_path": "src/a.ts"}), none).unwrap();
        assert_eq!(relative, json!({"file_path": "src/a.ts"}));
    }

    #[test]
    fn long_and_secret_like_args_are_cut_and_redacted() {
        let long = "ab ".repeat(250);
        let args = tool_args("Grep", &json!({"pattern": long}), wt()).unwrap();
        let pattern = args["pattern"].as_str().unwrap();
        assert!(pattern.chars().count() <= MAX_ARG_CHARS + 1);
        assert!(pattern.ends_with('…'));
        let secret = "a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8s9T0";
        let args = tool_args("Grep", &json!({"pattern": secret}), wt()).unwrap();
        assert_eq!(args["pattern"], "[ukryte]");
        let polish = "ż".repeat(250);
        let args = tool_args("Glob", &json!({"pattern": polish}), wt()).unwrap();
        assert!(args["pattern"].as_str().unwrap().chars().count() <= MAX_ARG_CHARS + 1);
    }
}
