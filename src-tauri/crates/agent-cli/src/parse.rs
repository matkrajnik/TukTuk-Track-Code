//! Parsery linii strumienia CLI. Wariant `_raw` zachowuje pełną nazwę narzędzia
//! i emituje `Parsed::Session`; asystent AI skraca nazwy w swojej warstwie.

use serde_json::{json, Value};

use crate::failure::{classify, codex_event_text, Failure};

/// Zdarzenie wyczytane z jednej linii strumienia CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum Parsed {
    Text(String),
    Tool {
        name: String,
        input: Value,
    },
    ToolDone {
        error: bool,
    },
    /// Koniec tury; tekst końcowy (gdy odpowiedź nie miała wcześniejszych akapitów).
    Done(String),
    Error(Failure),
    /// Identyfikator sesji CLI (tylko wariant surowy).
    Session(String),
}

/// Linia `stream-json` Claude Code z pełnymi nazwami narzędzi.
pub fn parse_claude_line_raw(line: &str) -> Vec<Parsed> {
    let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let content = event["message"]["content"].as_array();
    match event["type"].as_str().unwrap_or_default() {
        "system" if event["subtype"] == "init" => event["session_id"]
            .as_str()
            .map(|id| vec![Parsed::Session(id.to_string())])
            .unwrap_or_default(),
        "assistant" => content
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => block["text"]
                    .as_str()
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| Parsed::Text(t.to_string())),
                Some("tool_use") => Some(Parsed::Tool {
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    input: block["input"].clone(),
                }),
                _ => None,
            })
            .collect(),
        "user" => content
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "tool_result")
            .map(|block| Parsed::ToolDone {
                error: block["is_error"] == true,
            })
            .collect(),
        "result" => {
            let text = event["result"].as_str().unwrap_or_default();
            let subtype = event["subtype"].as_str().unwrap_or_default();
            if event["is_error"] == true || subtype.starts_with("error") {
                vec![Parsed::Error(classify(&format!("{text} {subtype}")))]
            } else {
                vec![Parsed::Done(text.to_string())]
            }
        }
        _ => Vec::new(),
    }
}

/// Zdarzenie z metadanymi: rodzic (linia subagenta) i identyfikator narzędzia.
#[derive(Clone, Debug, PartialEq)]
pub struct Traced {
    /// `parent_tool_use_id` linii; `None` dla linii głównego agenta.
    pub parent: Option<String>,
    /// `id` bloku `tool_use` albo `tool_use_id` bloku `tool_result`.
    pub tool_use_id: Option<String>,
    pub item: Parsed,
}

impl Traced {
    /// Zdarzenie głównego agenta bez identyfikatorów (np. Codex).
    pub fn root(item: Parsed) -> Self {
        Self {
            parent: None,
            tool_use_id: None,
            item,
        }
    }
}

/// Rodzic linii: tekst, a każda inna wartość niż null daje `Some`, żeby linia
/// o nieznanym kształcie nigdy nie udawała głównego agenta.
fn line_parent(event: &Value) -> Option<String> {
    match &event["parent_tool_use_id"] {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Jak [`parse_claude_line_raw`] (te same zdarzenia w tej samej kolejności),
/// ale z rodzicem i identyfikatorami narzędzi. Filtrowanie linii subagentów
/// należy do wywołującego.
pub fn parse_claude_line_traced(line: &str) -> Vec<Traced> {
    match serde_json::from_str::<Value>(line.trim()) {
        Ok(event) => parse_claude_value_traced(&event),
        Err(_) => Vec::new(),
    }
}

/// Jak [`parse_claude_line_traced`], dla linii już sparsowanej do JSON-a
/// (runner czyta z tej samej wartości także wynik i stan subagentów).
pub fn parse_claude_value_traced(event: &Value) -> Vec<Traced> {
    let parent = line_parent(event);
    let with = |tool_use_id: Option<String>, item: Parsed| Traced {
        parent: parent.clone(),
        tool_use_id,
        item,
    };
    let content = event["message"]["content"].as_array();
    match event["type"].as_str().unwrap_or_default() {
        "system" if event["subtype"] == "init" => event["session_id"]
            .as_str()
            .map(|id| vec![with(None, Parsed::Session(id.to_string()))])
            .unwrap_or_default(),
        "assistant" => content
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => block["text"]
                    .as_str()
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| with(None, Parsed::Text(t.to_string()))),
                Some("tool_use") => Some(with(
                    block["id"].as_str().map(str::to_string),
                    Parsed::Tool {
                        name: block["name"].as_str().unwrap_or_default().to_string(),
                        input: block["input"].clone(),
                    },
                )),
                _ => None,
            })
            .collect(),
        "user" => content
            .into_iter()
            .flatten()
            .filter(|block| block["type"] == "tool_result")
            .map(|block| {
                with(
                    block["tool_use_id"].as_str().map(str::to_string),
                    Parsed::ToolDone {
                        error: block["is_error"] == true,
                    },
                )
            })
            .collect(),
        "result" => {
            let text = event["result"].as_str().unwrap_or_default();
            let subtype = event["subtype"].as_str().unwrap_or_default();
            let item = if event["is_error"] == true || subtype.starts_with("error") {
                Parsed::Error(classify(&format!("{text} {subtype}")))
            } else {
                Parsed::Done(text.to_string())
            };
            vec![with(None, item)]
        }
        _ => Vec::new(),
    }
}

/// Linia JSONL `codex exec --json` z pełnymi nazwami narzędzi.
pub fn parse_codex_line_raw(line: &str) -> Vec<Parsed> {
    let Ok(event) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let kind = event["type"].as_str().unwrap_or_default();
    let item = &event["item"];
    if kind == "thread.started" {
        return event["thread_id"]
            .as_str()
            .map(|id| vec![Parsed::Session(id.to_string())])
            .unwrap_or_default();
    }
    if item["type"] == "mcp_tool_call" {
        return match kind {
            "item.started" => vec![Parsed::Tool {
                name: item["tool"].as_str().unwrap_or_default().to_string(),
                input: if item["arguments"].is_null() {
                    json!({})
                } else {
                    item["arguments"].clone()
                },
            }],
            "item.completed" => vec![Parsed::ToolDone {
                error: item["status"] == "failed" || !item["error"].is_null(),
            }],
            _ => Vec::new(),
        };
    }
    if kind == "turn.completed" {
        return vec![Parsed::Done(String::new())];
    }
    match codex_event_text(&event) {
        Some((true, text)) => vec![Parsed::Error(classify(&text))],
        Some((false, text)) if !text.trim().is_empty() => vec![Parsed::Text(text)],
        _ => Vec::new(),
    }
}
