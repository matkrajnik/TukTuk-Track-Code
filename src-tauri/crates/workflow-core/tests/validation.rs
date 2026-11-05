//! Walidacja grafu (PLAN_WDROZENIA.md, 11: warstwa „Graf”).

use serde_json::json;
use workflow_core::model::{AccessProfile, CompareOp, MAX_REPAIR_ROUNDS};
use workflow_core::template::analysis_implementation_review;
use workflow_core::{validate, Edge, IssueCode, NodeKind, Port, WorkflowGraph};

fn codes(graph: &WorkflowGraph) -> Vec<IssueCode> {
    validate(graph).into_iter().map(|i| i.code).collect()
}

fn node_mut<'g>(graph: &'g mut WorkflowGraph, id: &str) -> &'g mut NodeKind {
    &mut graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .expect("węzeł szablonu")
        .kind
}

fn edge(id: &str, source: &str, port: Port, target: &str) -> Edge {
    Edge {
        id: id.to_owned(),
        source: source.to_owned(),
        source_port: port,
        target: target.to_owned(),
    }
}

#[test]
fn template_is_valid() {
    assert_eq!(validate(&analysis_implementation_review()), vec![]);
}

#[test]
fn template_round_trips_through_json() {
    let graph = analysis_implementation_review();
    let text = serde_json::to_string(&graph).unwrap();
    let back: WorkflowGraph = serde_json::from_str(&text).unwrap();
    assert_eq!(back, graph);
    assert!(text.contains(r#""type":"condition""#));
    assert!(text.contains(r#""repairLimit":3"#));
}

#[test]
fn rejects_unknown_schema_version() {
    let mut graph = analysis_implementation_review();
    graph.schema_version = 99;
    assert_eq!(codes(&graph), vec![IssueCode::UnsupportedSchema]);
}

#[test]
fn rejects_missing_trigger_and_end() {
    let graph = WorkflowGraph {
        schema_version: 1,
        nodes: vec![],
        edges: vec![],
    };
    assert_eq!(
        codes(&graph),
        vec![IssueCode::MissingTrigger, IssueCode::MissingEnd]
    );
}

#[test]
fn rejects_second_trigger() {
    let mut graph = analysis_implementation_review();
    let mut extra = graph.nodes[0].clone();
    extra.id = "start2".into();
    graph.nodes.push(extra);
    graph
        .edges
        .push(edge("e-extra", "start2", Port::Out, "analysis"));
    assert!(codes(&graph).contains(&IssueCode::MultipleTriggers));
}

#[test]
fn rejects_duplicate_ids() {
    let mut graph = analysis_implementation_review();
    graph.nodes[2].id = "analysis".into();
    assert!(codes(&graph).contains(&IssueCode::DuplicateId));
}

#[test]
fn rejects_dangling_edge_and_wrong_port() {
    let mut graph = analysis_implementation_review();
    graph
        .edges
        .push(edge("ghost", "analysis", Port::Out, "nowhere"));
    graph
        .edges
        .push(edge("bad-port", "analysis", Port::True, "review"));
    let found = codes(&graph);
    assert!(found.contains(&IssueCode::DanglingEdge));
    assert!(found.contains(&IssueCode::InvalidPort));
}

#[test]
fn rejects_edge_into_trigger() {
    let mut graph = analysis_implementation_review();
    graph
        .edges
        .push(edge("back", "analysis", Port::Out, "start"));
    assert!(codes(&graph).contains(&IssueCode::EdgeIntoTrigger));
}

#[test]
fn reports_missing_and_duplicate_connection() {
    let mut graph = analysis_implementation_review();
    graph.edges.retain(|e| e.id != "approval-rejected");
    graph
        .edges
        .push(edge("dup", "analysis", Port::Out, "review"));
    let issues = validate(&graph);
    assert!(issues.iter().any(
        |i| i.code == IssueCode::MissingConnection && i.node_id.as_deref() == Some("approval")
    ));
    assert!(issues
        .iter()
        .any(|i| i.code == IssueCode::DuplicateConnection
            && i.node_id.as_deref() == Some("analysis")));
}

#[test]
fn reports_orphan_node_by_id() {
    let mut graph = analysis_implementation_review();
    let mut orphan = graph.nodes[1].clone();
    orphan.id = "orphan".into();
    graph.nodes.push(orphan);
    graph.edges.push(edge("o", "orphan", Port::Out, "done"));
    let issues = validate(&graph);
    assert!(issues
        .iter()
        .any(|i| i.code == IssueCode::Unreachable && i.node_id.as_deref() == Some("orphan")));
}

#[test]
fn rejects_cycle_without_condition() {
    let mut graph = analysis_implementation_review();
    // Implementacja wraca do analizy: pętla bez warunku i bez limitu.
    graph.edges.retain(|e| e.id != "implementation-review");
    graph
        .edges
        .push(edge("loop", "implementation", Port::Out, "analysis"));
    assert!(codes(&graph).contains(&IssueCode::InvalidCycle));
}

#[test]
fn repair_loop_needs_limit_within_range() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.repair_limit = None;
    assert!(codes(&graph).contains(&IssueCode::InvalidRepairLimit));

    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.repair_limit = Some(MAX_REPAIR_ROUNDS + 1);
    assert!(codes(&graph).contains(&IssueCode::InvalidRepairLimit));
}

#[test]
fn repair_limit_outside_loop_is_rejected() {
    let mut graph = analysis_implementation_review();
    // Poprawki kończą proces zamiast wracać do review: brak pętli.
    graph.edges.retain(|e| e.id != "fix-review");
    graph
        .edges
        .push(edge("fix-end", "fix", Port::Out, "rejected"));
    assert!(codes(&graph).contains(&IssueCode::RepairLimitOutsideLoop));
}

#[test]
fn loop_must_not_contain_approval() {
    let mut graph = analysis_implementation_review();
    graph.edges.retain(|e| e.id != "approval-rejected");
    graph
        .edges
        .push(edge("again", "approval", Port::Reject, "fix"));
    assert!(codes(&graph).contains(&IssueCode::InvalidCycle));
}

#[test]
fn reviewer_cannot_write() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Agent(agent) = node_mut(&mut graph, "review") else {
        unreachable!()
    };
    agent.access = AccessProfile::WorkspaceWrite;
    let issues = validate(&graph);
    assert!(issues
        .iter()
        .any(|i| i.code == IssueCode::AccessNotAllowed && i.node_id.as_deref() == Some("review")));
}

#[test]
fn agent_needs_instruction_timeout_and_nonempty_model() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Agent(agent) = node_mut(&mut graph, "analysis") else {
        unreachable!()
    };
    agent.instruction = "  ".into();
    agent.timeout_secs = 5;
    agent.model = Some(" ".into());
    let found = codes(&graph);
    assert!(found.contains(&IssueCode::EmptyInstruction));
    assert!(found.contains(&IssueCode::InvalidTimeout));
    assert!(found.contains(&IssueCode::EmptyModel));
}

#[test]
fn condition_checks_field_operator_and_value_type() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.value = json!("maybe");
    assert!(codes(&graph).contains(&IssueCode::TypeMismatch));

    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.value = json!("approved");
    condition.op = CompareOp::GreaterThan;
    assert!(codes(&graph).contains(&IssueCode::InvalidOperator));

    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.op = CompareOp::Equals;
    condition.field.field = "score".into();
    assert!(codes(&graph).contains(&IssueCode::UnknownField));
}

#[test]
fn condition_must_reference_agent_result() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.field.node_id = "approval".into();
    assert!(codes(&graph).contains(&IssueCode::InvalidFieldRef));

    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    // Analiza poprzedza warunek, ale jej wynik nie ma pola `verdict`.
    condition.field.node_id = "analysis".into();
    assert!(codes(&graph).contains(&IssueCode::UnknownField));
}

#[test]
fn condition_cannot_read_agent_not_on_every_path() {
    let mut graph = analysis_implementation_review();
    // Poprawki leżą za warunkiem w pętli: w pierwszej rundzie nie mają wyniku.
    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.field.node_id = "fix".into();
    condition.field.field = "changedFileCount".into();
    condition.op = CompareOp::GreaterThan;
    condition.value = json!(0);
    let issues = validate(&graph);
    assert!(issues.iter().any(
        |i| i.code == IssueCode::FieldNotAvailable && i.node_id.as_deref() == Some("decision")
    ));
}

#[test]
fn number_condition_on_finding_count_is_valid() {
    let mut graph = analysis_implementation_review();
    let NodeKind::Condition(condition) = node_mut(&mut graph, "decision") else {
        unreachable!()
    };
    condition.field.field = "findingCount".into();
    condition.op = CompareOp::LessThan;
    condition.value = json!(1);
    assert_eq!(validate(&graph), vec![]);
}

#[test]
fn issues_serialize_in_camel_case() {
    let mut graph = analysis_implementation_review();
    graph.nodes[1].label = String::new();
    let text = serde_json::to_string(&validate(&graph)).unwrap();
    assert!(text.contains(r#""code":"emptyLabel""#));
    assert!(text.contains(r#""nodeId":"analysis""#));
}
