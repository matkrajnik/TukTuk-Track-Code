//! Szablon „Analiza → implementacja → review” (PLAN_WDROZENIA.md, 1 i 4.2).

use serde_json::json;

use crate::model::{
    AgentConfig, AgentRole, ApprovalConfig, CompareOp, ConditionConfig, Edge, EndConfig,
    EndOutcome, FieldRef, Node, NodeKind, Port, Position, Provider, WorkflowGraph,
    MAX_REPAIR_ROUNDS, SCHEMA_VERSION,
};

/// Domyślny limit kroku agenta w szablonie: 30 minut.
const TEMPLATE_TIMEOUT_SECS: u32 = 30 * 60;

pub const TEMPLATE_NAME: &str = "Analiza → implementacja → review";

fn node(id: &str, label: &str, x: f64, y: f64, kind: NodeKind) -> Node {
    Node {
        id: id.to_owned(),
        label: label.to_owned(),
        position: Position { x, y },
        kind,
    }
}

fn agent(role: AgentRole, provider: Provider, instruction: &str) -> NodeKind {
    NodeKind::Agent(AgentConfig {
        role,
        provider,
        model: None,
        instruction: instruction.to_owned(),
        access: role.required_access(),
        timeout_secs: TEMPLATE_TIMEOUT_SECS,
    })
}

fn edge(source: &str, port: Port, target: &str) -> Edge {
    Edge {
        id: format!("{source}-{target}"),
        source: source.to_owned(),
        source_port: port,
        target: target.to_owned(),
    }
}

/// Nowy pusty proces: Start połączony z Końcem.
pub fn blank() -> WorkflowGraph {
    WorkflowGraph {
        schema_version: SCHEMA_VERSION,
        nodes: vec![
            node("start", "Start", 0.0, 0.0, NodeKind::Trigger),
            node(
                "done",
                "Koniec",
                260.0,
                0.0,
                NodeKind::End(EndConfig {
                    outcome: EndOutcome::Succeeded,
                }),
            ),
        ],
        edges: vec![edge("start", Port::Out, "done")],
    }
}

/// Claude analizuje, Codex implementuje, Claude robi review; najwyżej trzy
/// rundy poprawek, potem decyzja człowieka.
pub fn analysis_implementation_review() -> WorkflowGraph {
    let nodes = vec![
        node("start", "Start", 0.0, 0.0, NodeKind::Trigger),
        node(
            "analysis",
            "Analiza",
            220.0,
            0.0,
            agent(
                AgentRole::Analysis,
                Provider::Claude,
                "Przeanalizuj repozytorium pod kątem celu zadania. Zapisz plan zmian, \
                 kryteria akceptacji i ryzyka. Nie zmieniaj plików.",
            ),
        ),
        node(
            "implementation",
            "Implementacja",
            440.0,
            0.0,
            agent(
                AgentRole::Implementation,
                Provider::Codex,
                "Zrealizuj plan z analizy w tym katalogu roboczym. Uruchom testy, \
                 które dotyczą zmiany, i opisz ich wynik.",
            ),
        ),
        node(
            "review",
            "Review",
            660.0,
            0.0,
            agent(
                AgentRole::Review,
                Provider::Claude,
                "Zrób review zmian względem planu i kryteriów akceptacji. Wypisz \
                 problemy z plikiem, miejscem i wagą. Nie zmieniaj plików.",
            ),
        ),
        node(
            "decision",
            "Review zaakceptowane?",
            880.0,
            0.0,
            NodeKind::Condition(ConditionConfig {
                field: FieldRef {
                    node_id: "review".to_owned(),
                    field: "verdict".to_owned(),
                },
                op: CompareOp::Equals,
                value: json!("approved"),
                repair_limit: Some(MAX_REPAIR_ROUNDS),
            }),
        ),
        node(
            "fix",
            "Poprawki",
            660.0,
            180.0,
            agent(
                AgentRole::Fix,
                Provider::Codex,
                "Popraw problemy wskazane w ostatnim review. Nie zmieniaj zakresu zadania.",
            ),
        ),
        node(
            "approval",
            "Decyzja użytkownika",
            1100.0,
            0.0,
            NodeKind::Approval(ApprovalConfig {
                description: "Przejrzyj diff i wynik review, potem zaakceptuj albo odrzuć zmiany."
                    .to_owned(),
            }),
        ),
        node(
            "done",
            "Zaakceptowano",
            1320.0,
            -80.0,
            NodeKind::End(EndConfig {
                outcome: EndOutcome::Succeeded,
            }),
        ),
        node(
            "rejected",
            "Odrzucono",
            1320.0,
            80.0,
            NodeKind::End(EndConfig {
                outcome: EndOutcome::Rejected,
            }),
        ),
    ];
    let edges = vec![
        edge("start", Port::Out, "analysis"),
        edge("analysis", Port::Out, "implementation"),
        edge("implementation", Port::Out, "review"),
        edge("review", Port::Out, "decision"),
        edge("decision", Port::True, "approval"),
        edge("decision", Port::False, "fix"),
        edge("fix", Port::Out, "review"),
        edge("approval", Port::Approve, "done"),
        edge("approval", Port::Reject, "rejected"),
    ];
    WorkflowGraph {
        schema_version: SCHEMA_VERSION,
        nodes,
        edges,
    }
}
