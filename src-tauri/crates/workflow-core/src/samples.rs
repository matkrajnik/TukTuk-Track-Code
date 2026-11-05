//! Przykładowe poprawne wyniki ról agentów (dla testów silnika i hosta).

use serde_json::{json, Value};

pub fn analysis() -> Value {
    json!({"summary": "analiza", "plan": ["krok"], "acceptanceCriteria": ["kryterium"],
           "risks": ["ryzyko"]})
}

pub fn implementation() -> Value {
    json!({"summary": "zmiany", "changedFiles": ["a.rs"],
           "checks": [{"name": "cargo test", "passed": true}]})
}

pub fn review_approved() -> Value {
    json!({"verdict": "approved", "findings": [], "checkedArtifactHash": "h"})
}

pub fn review_changes() -> Value {
    json!({"verdict": "changes_requested",
           "findings": [{"file": "a.rs", "location": "1", "severity": "high",
                         "description": "popraw"}],
           "checkedArtifactHash": "h"})
}
