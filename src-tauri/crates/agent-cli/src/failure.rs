use serde_json::Value;

/// Powód nieudanego wywołania po stronie CLI.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    NotLoggedIn,
    Limit,
    /// Inny błąd CLI (treść nie trafia do interfejsu).
    Other,
    /// Odpowiedź nie jest JSON-em.
    Unreadable,
}

/// Rozpoznaje błąd po tekście zwróconym przez CLI (limit przed logowaniem:
/// komunikat o limicie bywa długi i zawiera słowa o koncie).
pub fn classify(text: &str) -> Failure {
    let lower = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has(&[
        "usage limit",
        "limit reached",
        "rate limit",
        "rate_limit",
        "quota",
        "hit your limit",
        "too many requests",
        "429",
        "overloaded_limit",
    ]) {
        Failure::Limit
    } else if has(&[
        "not logged in",
        "not authenticated",
        "/login",
        "invalid api key",
        "authentication",
        "unauthorized",
        "401",
        "log in",
        "login",
        "sign in",
    ]) {
        Failure::NotLoggedIn
    } else {
        Failure::Other
    }
}

/// Wersja z wyjścia `--version`: pierwszy token wyglądający jak numer
/// (`2.1.304 (Claude Code)`, `codex-cli 0.162.1`), a bez niego pierwsza linia.
pub fn parse_version(output: &str) -> String {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty());
    let Some(line) = line else {
        return String::new();
    };
    line.split_whitespace()
        .map(|t| t.trim_start_matches('v'))
        .find(|t| {
            t.chars().next().is_some_and(|c| c.is_ascii_digit())
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                && t.contains('.')
        })
        .map(str::to_string)
        .unwrap_or_else(|| line.chars().take(60).collect())
}

/// Tekst z jednego zdarzenia `codex exec --json`: wiadomość agenta albo błąd.
pub fn codex_event_text(event: &Value) -> Option<(bool, String)> {
    let kind = event["type"].as_str().unwrap_or_default();
    let item = &event["item"];
    let msg = &event["msg"];
    if kind == "error" || kind == "turn.failed" || kind == "stream_error" {
        let text = event["message"]
            .as_str()
            .or_else(|| event["error"]["message"].as_str())
            .or_else(|| event["error"].as_str())
            .unwrap_or("error");
        return Some((true, text.to_string()));
    }
    let message_item = if item["type"] == "agent_message" {
        Some(item)
    } else if msg["type"] == "agent_message" {
        Some(msg)
    } else {
        None
    }?;
    let text = message_item["text"]
        .as_str()
        .or_else(|| message_item["message"].as_str())?;
    Some((false, text.to_string()))
}
