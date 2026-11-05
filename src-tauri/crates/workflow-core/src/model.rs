//! Typy definicji procesu (PLAN_WDROZENIA.md, 5.1 i 7.4).
//!
//! JSON grafu jest wspólnym formatem szkicu, wersji opublikowanej, eksportu
//! i frontendu (`src/api/workflows.ts`). Pola są w camelCase.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Wersja formatu grafu. Zmiana formatu wymaga migracji zapisanych definicji.
pub const SCHEMA_VERSION: u32 = 1;

/// Najwięcej rund poprawek w bloku Condition → poprawki → review (5.1).
pub const MAX_REPAIR_ROUNDS: u32 = 3;

/// Limit czasu kroku agenta w sekundach: od minuty do czterech godzin.
pub const MIN_TIMEOUT_SECS: u32 = 60;
pub const MAX_TIMEOUT_SECS: u32 = 4 * 60 * 60;

/// Górna granica rozmiaru grafu; edytor ma być płynny przy 100 węzłach (11).
pub const MAX_NODES: usize = 200;
pub const MAX_EDGES: usize = 400;

/// Najdłuższa instrukcja agenta w znakach.
pub const MAX_INSTRUCTION_CHARS: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowGraph {
    pub schema_version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl WorkflowGraph {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }
}

/// Pozycja na canvasie. Nie ma znaczenia wykonawczego (5.1).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub position: Position,
    pub kind: NodeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum NodeKind {
    Trigger,
    Agent(AgentConfig),
    Condition(ConditionConfig),
    Approval(ApprovalConfig),
    End(EndConfig),
}

impl NodeKind {
    /// Porty wyjściowe typu węzła; każdy musi mieć dokładnie jedno połączenie.
    pub fn output_ports(&self) -> &'static [Port] {
        match self {
            NodeKind::Trigger | NodeKind::Agent(_) => &[Port::Out],
            NodeKind::Condition(_) => &[Port::True, Port::False],
            NodeKind::Approval(_) => &[Port::Approve, Port::Reject],
            NodeKind::End(_) => &[],
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            NodeKind::Trigger => "Start",
            NodeKind::Agent(_) => "Agent",
            NodeKind::Condition(_) => "Warunek",
            NodeKind::Approval(_) => "Akceptacja",
            NodeKind::End(_) => "Koniec",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Port {
    Out,
    True,
    False,
    Approve,
    Reject,
}

impl Port {
    pub fn name(self) -> &'static str {
        match self {
            Port::Out => "wyjście",
            Port::True => "tak",
            Port::False => "nie",
            Port::Approve => "akceptacja",
            Port::Reject => "odrzucenie",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub source_port: Port,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    Claude,
    Codex,
}

/// Rola agenta wyznacza schemat wyniku i dozwolony profil dostępu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentRole {
    Analysis,
    Implementation,
    Fix,
    Review,
}

impl AgentRole {
    /// Profil dostępu wymagany dla roli (8.3): analiza i review tylko czytają.
    pub fn required_access(self) -> AccessProfile {
        match self {
            AgentRole::Analysis | AgentRole::Review => AccessProfile::ReadOnly,
            AgentRole::Implementation | AgentRole::Fix => AccessProfile::WorkspaceWrite,
        }
    }

    /// Pola skalarne wyniku dostępne dla węzła Condition (7.4).
    pub fn output_fields(self) -> &'static [(&'static str, FieldType)] {
        const ANALYSIS: &[(&str, FieldType)] = &[
            ("summary", FieldType::Text),
            ("riskCount", FieldType::Number),
        ];
        const IMPLEMENTATION: &[(&str, FieldType)] = &[
            ("summary", FieldType::Text),
            ("changedFileCount", FieldType::Number),
        ];
        const REVIEW: &[(&str, FieldType)] = &[
            ("verdict", FieldType::Choice(REVIEW_VERDICTS)),
            ("findingCount", FieldType::Number),
        ];
        match self {
            AgentRole::Analysis => ANALYSIS,
            AgentRole::Implementation | AgentRole::Fix => IMPLEMENTATION,
            AgentRole::Review => REVIEW,
        }
    }

    pub fn output_field(self, name: &str) -> Option<FieldType> {
        self.output_fields()
            .iter()
            .find(|(field, _)| *field == name)
            .map(|(_, kind)| *kind)
    }
}

pub const REVIEW_VERDICTS: &[&str] = &["approved", "changes_requested"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessProfile {
    ReadOnly,
    WorkspaceWrite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub role: AgentRole,
    pub provider: Provider,
    /// Pusty model oznacza wybór w formularzu uruchomienia (4.4).
    #[serde(default)]
    pub model: Option<String>,
    pub instruction: String,
    pub access: AccessProfile,
    pub timeout_secs: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "type", content = "options", rename_all = "camelCase")]
pub enum FieldType {
    Text,
    Number,
    Boolean,
    Choice(&'static [&'static str]),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldRef {
    pub node_id: String,
    pub field: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompareOp {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
}

/// Typowane porównanie pola wyniku z literałem; bez wyrażeń i eval (4.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConditionConfig {
    pub field: FieldRef,
    pub op: CompareOp,
    pub value: Value,
    /// Ustawione tylko na warunku zamykającym blok poprawek.
    #[serde(default)]
    pub repair_limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalConfig {
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndOutcome {
    Succeeded,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndConfig {
    pub outcome: EndOutcome,
}
