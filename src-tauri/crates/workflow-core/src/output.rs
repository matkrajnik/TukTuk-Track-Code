//! Typowane wyniki agentów i ewaluacja warunków (PLAN_WDROZENIA.md, 5.2 i 7.4).
//!
//! Wynik agenta to JSON zgodny ze schematem roli. Warunek porównuje skalarne
//! pole wyniku z literałem; brak wartości albo zły typ to błąd, nigdy
//! domyślne „review w porządku”.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{AgentRole, CompareOp, ConditionConfig};

/// Najwięcej elementów w każdej liście wyniku.
pub const MAX_OUTPUT_ITEMS: usize = 200;
/// Najdłuższy pojedynczy tekst wyniku w znakach.
pub const MAX_OUTPUT_TEXT_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOutput {
    pub summary: String,
    pub plan: Vec<String>,
    pub acceptance_criteria: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub log_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImplementationOutput {
    pub summary: String,
    pub changed_files: Vec<String>,
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approved,
    ChangesRequested,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Approved => "approved",
            Verdict::ChangesRequested => "changes_requested",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub file: String,
    pub location: String,
    pub severity: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewOutput {
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
    pub checked_artifact_hash: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentOutput {
    Analysis(AnalysisOutput),
    Implementation(ImplementationOutput),
    Review(ReviewOutput),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputError(pub String);

impl fmt::Display for OutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Wynik agenta jest niepoprawny: {}", self.0)
    }
}

impl std::error::Error for OutputError {}

fn check_text(name: &str, text: &str) -> Result<(), OutputError> {
    if text.chars().count() > MAX_OUTPUT_TEXT_CHARS {
        return Err(OutputError(format!("pole {name} jest za długie")));
    }
    Ok(())
}

fn check_list(name: &str, items: &[String]) -> Result<(), OutputError> {
    if items.len() > MAX_OUTPUT_ITEMS {
        return Err(OutputError(format!("lista {name} ma za dużo elementów")));
    }
    items.iter().try_for_each(|item| check_text(name, item))
}

fn parse<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, OutputError> {
    serde_json::from_value(value.clone()).map_err(|e| OutputError(e.to_string()))
}

/// Parsuje wynik według roli. Rola Fix używa schematu implementacji.
pub fn parse_output(role: AgentRole, value: &Value) -> Result<AgentOutput, OutputError> {
    match role {
        AgentRole::Analysis => {
            let out: AnalysisOutput = parse(value)?;
            check_text("summary", &out.summary)?;
            check_list("plan", &out.plan)?;
            check_list("acceptanceCriteria", &out.acceptance_criteria)?;
            check_list("risks", &out.risks)?;
            Ok(AgentOutput::Analysis(out))
        }
        AgentRole::Implementation | AgentRole::Fix => {
            let out: ImplementationOutput = parse(value)?;
            check_text("summary", &out.summary)?;
            check_list("changedFiles", &out.changed_files)?;
            if out.checks.len() > MAX_OUTPUT_ITEMS {
                return Err(OutputError("lista checks ma za dużo elementów".into()));
            }
            out.checks
                .iter()
                .try_for_each(|c| check_text("checks", &c.name))?;
            Ok(AgentOutput::Implementation(out))
        }
        AgentRole::Review => {
            let out: ReviewOutput = parse(value)?;
            check_text("checkedArtifactHash", &out.checked_artifact_hash)?;
            if out.findings.len() > MAX_OUTPUT_ITEMS {
                return Err(OutputError("lista findings ma za dużo elementów".into()));
            }
            for f in &out.findings {
                check_text("findings", &f.description)?;
            }
            Ok(AgentOutput::Review(out))
        }
    }
}

impl AgentOutput {
    /// Znormalizowany JSON wyniku (to trafia do zdarzeń i bazy).
    pub fn to_value(&self) -> Value {
        match self {
            AgentOutput::Analysis(o) => json!(o),
            AgentOutput::Implementation(o) => json!(o),
            AgentOutput::Review(o) => json!(o),
        }
    }

    /// Skalarne pola dostępne dla warunku: dokładnie te z `AgentRole::output_fields`.
    pub fn field(&self, name: &str) -> Option<Value> {
        match (self, name) {
            (AgentOutput::Analysis(o), "summary") => Some(json!(o.summary)),
            (AgentOutput::Analysis(o), "riskCount") => Some(json!(o.risks.len())),
            (AgentOutput::Implementation(o), "summary") => Some(json!(o.summary)),
            (AgentOutput::Implementation(o), "changedFileCount") => {
                Some(json!(o.changed_files.len()))
            }
            (AgentOutput::Review(o), "verdict") => Some(json!(o.verdict.as_str())),
            (AgentOutput::Review(o), "findingCount") => Some(json!(o.findings.len())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionError {
    MissingValue,
    TypeMismatch,
}

impl fmt::Display for ConditionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConditionError::MissingValue => {
                f.write_str("Warunek nie ma wartości do porównania (brak wyniku kroku).")
            }
            ConditionError::TypeMismatch => f.write_str(
                "Warunek porównuje wartości różnych typów albo nieobsługiwanym operatorem.",
            ),
        }
    }
}

impl std::error::Error for ConditionError {}

/// Typowane porównanie bez wyrażeń i eval. Brak wartości lub zły typ to błąd.
pub fn evaluate(
    condition: &ConditionConfig,
    value: Option<&Value>,
) -> Result<bool, ConditionError> {
    let actual = value.ok_or(ConditionError::MissingValue)?;
    let expected = &condition.value;
    match condition.op {
        CompareOp::Equals => equal(actual, expected),
        CompareOp::NotEquals => equal(actual, expected).map(|eq| !eq),
        CompareOp::GreaterThan => numbers(actual, expected).map(|(a, e)| a > e),
        CompareOp::LessThan => numbers(actual, expected).map(|(a, e)| a < e),
    }
}

fn numbers(actual: &Value, expected: &Value) -> Result<(f64, f64), ConditionError> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => a
            .as_f64()
            .zip(e.as_f64())
            .ok_or(ConditionError::TypeMismatch),
        _ => Err(ConditionError::TypeMismatch),
    }
}

fn equal(actual: &Value, expected: &Value) -> Result<bool, ConditionError> {
    match (actual, expected) {
        (Value::String(a), Value::String(e)) => Ok(a == e),
        (Value::Bool(a), Value::Bool(e)) => Ok(a == e),
        (Value::Number(_), Value::Number(_)) => numbers(actual, expected).map(|(a, e)| a == e),
        _ => Err(ConditionError::TypeMismatch),
    }
}
