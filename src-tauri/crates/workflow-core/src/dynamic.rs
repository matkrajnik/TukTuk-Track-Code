//! Tryb dynamiczny (E8, docs/workflows/E8_PLAN.md): jeden orkiestrator Claude,
//! który sam deleguje pracę do subagentów, w stałym układzie kroków.
//!
//! Graf dynamiczny nie jest swobodnie edytowalny: powstaje z `DynamicSpec`
//! (`build`), a walidator porównuje kształt grafu z `build(spec_of(graph))`.
//! `src/api/workflowDynamic.ts` jest kopią tych reguł 1:1.

use serde_json::json;

use crate::model::{
    AccessProfile, AgentConfig, AgentRole, ApprovalConfig, CompareOp, ConditionConfig, Edge,
    EndConfig, EndOutcome, FieldRef, Node, NodeKind, Port, Position, Provider, WorkflowGraph,
    DEFAULT_SUBAGENTS_HINT, DYNAMIC_TIMEOUT_SECS, MAX_REPAIR_ROUNDS, SCHEMA_VERSION,
};

pub const DYNAMIC_TEMPLATE_NAME: &str = "Dynamiczny (Claude)";

pub const ORCHESTRATOR_ID: &str = "orchestrator";
pub const REVIEW_ID: &str = "review";
const FIX_ID: &str = "fix";
const DECISION_ID: &str = "decision";
const APPROVAL_ID: &str = "approval";

/// Limit czasu stałych kroków review i poprawek w trybie dynamicznym: 30 minut.
pub const DYNAMIC_STEP_TIMEOUT_SECS: u32 = 30 * 60;

pub const DEFAULT_INSTRUCTION: &str = "Zrealizuj cel przebiegu. Rozbij pracę na niezależne części i zleć je subagentom narzędziem Agent, a ich wyniki zbierz i sprawdź. Opisz, co zostało zrobione.";
pub const REVIEW_INSTRUCTION: &str = "Zrób review zmian względem celu przebiegu i wyniku orkiestratora. Wypisz problemy z plikiem, miejscem i wagą. Nie zmieniaj plików.";
pub const FIX_INSTRUCTION: &str =
    "Popraw problemy wskazane w ostatnim review. Nie zmieniaj zakresu zadania.";
const APPROVAL_WITH_REVIEW: &str =
    "Przejrzyj diff i wynik review, potem zaakceptuj albo odrzuć zmiany.";
const APPROVAL_WITHOUT_REVIEW: &str =
    "Przejrzyj wynik orkiestratora i zmiany w katalogu roboczym, potem zaakceptuj albo odrzuć.";

/// Parametry procesu dynamicznego, jedyne pokrętła formularza.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicSpec {
    pub instruction: String,
    pub model: Option<String>,
    pub timeout_secs: u32,
    pub access: AccessProfile,
    /// Wskazówka w prompcie (1–8), nie limit egzekwowany przez CLI.
    pub max_subagents: u32,
    /// Review i poprawki po orkiestratorze (wymaga zapisu).
    pub review: bool,
}

impl Default for DynamicSpec {
    fn default() -> Self {
        Self {
            instruction: DEFAULT_INSTRUCTION.to_owned(),
            model: None,
            timeout_secs: DYNAMIC_TIMEOUT_SECS,
            access: AccessProfile::WorkspaceWrite,
            max_subagents: DEFAULT_SUBAGENTS_HINT,
            review: true,
        }
    }
}

fn node(id: &str, label: &str, x: f64, kind: NodeKind) -> Node {
    node_at(id, label, x, 0.0, kind)
}

fn node_at(id: &str, label: &str, x: f64, y: f64, kind: NodeKind) -> Node {
    Node {
        id: id.to_owned(),
        label: label.to_owned(),
        position: Position { x, y },
        kind,
    }
}

fn edge(source: &str, port: Port, target: &str) -> Edge {
    Edge {
        id: format!("{source}-{target}"),
        source: source.to_owned(),
        source_port: port,
        target: target.to_owned(),
    }
}

fn claude_agent(
    role: AgentRole,
    model: &Option<String>,
    instruction: &str,
    access: AccessProfile,
    timeout_secs: u32,
    max_subagents: Option<u32>,
) -> NodeKind {
    NodeKind::Agent(AgentConfig {
        role,
        provider: Provider::Claude,
        model: model.clone(),
        instruction: instruction.to_owned(),
        access,
        timeout_secs,
        max_subagents,
    })
}

fn end(outcome: EndOutcome) -> NodeKind {
    NodeKind::End(EndConfig { outcome })
}

/// Kanoniczny graf procesu dynamicznego dla parametrów.
///
/// start → orkiestrator → [review → decyzja (verdict == approved, 3 rundy),
/// „nie” → poprawki → review] → akceptacja → koniec / odrzucono.
pub fn build(spec: &DynamicSpec) -> WorkflowGraph {
    let orchestrator = node(
        ORCHESTRATOR_ID,
        "Orkiestrator",
        220.0,
        claude_agent(
            AgentRole::Orchestrator,
            &spec.model,
            &spec.instruction,
            spec.access,
            spec.timeout_secs,
            Some(spec.max_subagents),
        ),
    );
    let mut nodes = vec![node("start", "Start", 0.0, NodeKind::Trigger), orchestrator];
    let mut edges = vec![edge("start", Port::Out, ORCHESTRATOR_ID)];
    let approval_x;
    if spec.review {
        nodes.push(node(
            REVIEW_ID,
            "Review",
            440.0,
            claude_agent(
                AgentRole::Review,
                &spec.model,
                REVIEW_INSTRUCTION,
                AccessProfile::ReadOnly,
                DYNAMIC_STEP_TIMEOUT_SECS,
                None,
            ),
        ));
        nodes.push(node(
            DECISION_ID,
            "Review zaakceptowane?",
            660.0,
            NodeKind::Condition(ConditionConfig {
                field: FieldRef {
                    node_id: REVIEW_ID.to_owned(),
                    field: "verdict".to_owned(),
                },
                op: CompareOp::Equals,
                value: json!("approved"),
                repair_limit: Some(MAX_REPAIR_ROUNDS),
            }),
        ));
        nodes.push(node_at(
            FIX_ID,
            "Poprawki",
            440.0,
            180.0,
            claude_agent(
                AgentRole::Fix,
                &spec.model,
                FIX_INSTRUCTION,
                AccessProfile::WorkspaceWrite,
                DYNAMIC_STEP_TIMEOUT_SECS,
                None,
            ),
        ));
        edges.push(edge(ORCHESTRATOR_ID, Port::Out, REVIEW_ID));
        edges.push(edge(REVIEW_ID, Port::Out, DECISION_ID));
        edges.push(edge(DECISION_ID, Port::True, APPROVAL_ID));
        edges.push(edge(DECISION_ID, Port::False, FIX_ID));
        edges.push(edge(FIX_ID, Port::Out, REVIEW_ID));
        approval_x = 880.0;
    } else {
        edges.push(edge(ORCHESTRATOR_ID, Port::Out, APPROVAL_ID));
        approval_x = 440.0;
    }
    let description = if spec.review {
        APPROVAL_WITH_REVIEW
    } else {
        APPROVAL_WITHOUT_REVIEW
    };
    nodes.push(node(
        APPROVAL_ID,
        "Decyzja użytkownika",
        approval_x,
        NodeKind::Approval(ApprovalConfig {
            description: description.to_owned(),
        }),
    ));
    nodes.push(node_at(
        "done",
        "Zaakceptowano",
        approval_x + 220.0,
        -80.0,
        end(EndOutcome::Succeeded),
    ));
    nodes.push(node_at(
        "rejected",
        "Odrzucono",
        approval_x + 220.0,
        80.0,
        end(EndOutcome::Rejected),
    ));
    edges.push(edge(APPROVAL_ID, Port::Approve, "done"));
    edges.push(edge(APPROVAL_ID, Port::Reject, "rejected"));
    WorkflowGraph {
        schema_version: SCHEMA_VERSION,
        nodes,
        edges,
    }
}

/// Czy graf zawiera krok orkiestratora (tryb dynamiczny).
pub fn is_dynamic(graph: &WorkflowGraph) -> bool {
    graph
        .nodes
        .iter()
        .any(|n| matches!(&n.kind, NodeKind::Agent(a) if a.role == AgentRole::Orchestrator))
}

/// Parametry odczytane z grafu; `None`, gdy nie ma kanonicznego węzła orkiestratora.
/// Brak `maxSubagents` daje wartość domyślną. Pozycje i dostawca nie są odczytywane.
pub fn spec_of(graph: &WorkflowGraph) -> Option<DynamicSpec> {
    let NodeKind::Agent(agent) = &graph.node(ORCHESTRATOR_ID)?.kind else {
        return None;
    };
    if agent.role != AgentRole::Orchestrator {
        return None;
    }
    Some(DynamicSpec {
        instruction: agent.instruction.clone(),
        model: agent.model.clone(),
        timeout_secs: agent.timeout_secs,
        access: agent.access,
        max_subagents: agent.max_subagents.unwrap_or(DEFAULT_SUBAGENTS_HINT),
        review: graph.node(REVIEW_ID).is_some(),
    })
}

fn without_positions(graph: &WorkflowGraph) -> WorkflowGraph {
    let mut copy = graph.clone();
    for node in &mut copy.nodes {
        node.position = Position::default();
    }
    copy
}

/// Czy dwa grafy mają ten sam kształt: wszystko poza pozycjami na canvasie.
pub fn same_shape(a: &WorkflowGraph, b: &WorkflowGraph) -> bool {
    without_positions(a) == without_positions(b)
}

/// Czy graf jest dokładnie tym, co zbudowałby `build` z jego parametrów.
/// Dostawca orkiestratora i brak `maxSubagents` są normalizowane (dostawcę
/// sprawdza osobna reguła, a brak pola oznacza wartość domyślną).
pub fn has_canonical_shape(graph: &WorkflowGraph) -> bool {
    let Some(spec) = spec_of(graph) else {
        return false;
    };
    let mut normalized = graph.clone();
    for node in &mut normalized.nodes {
        if let NodeKind::Agent(agent) = &mut node.kind {
            if agent.role == AgentRole::Orchestrator {
                agent.provider = Provider::Claude;
                agent.max_subagents = Some(spec.max_subagents);
            }
        }
    }
    same_shape(&normalized, &build(&spec))
}
