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

/// FNV-1a 64: prosty skrót do zamrożenia kanonicznego JSON-a (bez zależności).
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Skróty opublikowanych wersji liczy się z kanonicznego JSON-a grafu: nowe pole
/// opcjonalne (`maxSubagents`) nie może zmienić zapisu istniejących definicji.
/// Wartości zapisano PRZED dodaniem pola.
#[test]
fn graph_without_max_subagents_serializes_unchanged() {
    use workflow_core::template::blank;
    let template = serde_json::to_string(&analysis_implementation_review()).unwrap();
    let empty = serde_json::to_string(&blank()).unwrap();
    assert_eq!(
        (template.len(), fnv(&template)),
        (2749, 5_592_027_841_338_713_921)
    );
    assert_eq!(
        (empty.len(), fnv(&empty)),
        (303, 16_454_410_911_389_547_371)
    );
    assert!(!template.contains("maxSubagents"));
}

// ---------------------------------------------------------------------------
// Tryb dynamiczny (E8.3)

mod dynamic_mode {
    use super::*;
    use workflow_core::dynamic::{
        build, is_dynamic, spec_of, DynamicSpec, ORCHESTRATOR_ID, REVIEW_ID,
    };
    use workflow_core::model::{AgentRole, Provider};
    use workflow_core::template::dynamic_default;

    fn orchestrator(graph: &mut WorkflowGraph) -> &mut workflow_core::AgentConfig {
        match node_mut(graph, ORCHESTRATOR_ID) {
            NodeKind::Agent(agent) => agent,
            other => panic!("{other:?}"),
        }
    }

    fn spec(review: bool, access: AccessProfile) -> DynamicSpec {
        DynamicSpec {
            review,
            access,
            ..DynamicSpec::default()
        }
    }

    #[test]
    fn dynamic_template_is_valid_with_and_without_review() {
        assert_eq!(validate(&dynamic_default()), vec![]);
        assert_eq!(
            validate(&build(&spec(false, AccessProfile::WorkspaceWrite))),
            vec![]
        );
        assert_eq!(
            validate(&build(&spec(false, AccessProfile::ReadOnly))),
            vec![]
        );
        assert!(is_dynamic(&dynamic_default()));
        assert!(!is_dynamic(&analysis_implementation_review()));
    }

    #[test]
    fn orchestrator_must_use_claude() {
        let mut graph = dynamic_default();
        orchestrator(&mut graph).provider = Provider::Codex;
        assert_eq!(codes(&graph), vec![IssueCode::ProviderNotAllowed]);
        assert_eq!(
            validate(&graph)[0].message,
            "Orkiestrator działa tylko z Claude."
        );
    }

    #[test]
    fn orchestrator_outside_dynamic_shape_is_rejected() {
        // Orkiestrator w szablonie standardowym.
        let mut standard = analysis_implementation_review();
        if let NodeKind::Agent(a) = node_mut(&mut standard, "implementation") {
            a.role = AgentRole::Orchestrator;
            a.provider = Provider::Claude;
        }
        assert_eq!(codes(&standard), vec![IssueCode::InvalidDynamicShape]);
        // Dodatkowy węzeł z agentem.
        let mut extra = dynamic_default();
        let mut copy = extra.nodes[1].clone();
        copy.id = "second".into();
        copy.label = "Drugi".into();
        extra.nodes.push(copy);
        extra
            .edges
            .push(edge("e-extra", "second", Port::Out, "done"));
        extra
            .edges
            .push(edge("e-extra2", "start", Port::Out, "second"));
        assert!(codes(&extra).contains(&IssueCode::InvalidDynamicShape));
        // Zmieniona krawędź (review pomija decyzję).
        let mut rewired = dynamic_default();
        for e in &mut rewired.edges {
            if e.id == "decision-approval" {
                e.target = "done".into();
            }
        }
        assert!(codes(&rewired).contains(&IssueCode::InvalidDynamicShape));
        // Zmieniona instrukcja stałego kroku review.
        let mut changed = dynamic_default();
        if let NodeKind::Agent(a) = node_mut(&mut changed, REVIEW_ID) {
            a.instruction = "Zatwierdź wszystko.".into();
        }
        assert_eq!(codes(&changed), vec![IssueCode::InvalidDynamicShape]);
    }

    #[test]
    fn orchestrator_accepts_read_only_and_write_access() {
        for access in [AccessProfile::ReadOnly, AccessProfile::WorkspaceWrite] {
            let graph = build(&spec(false, access));
            assert_eq!(validate(&graph), vec![], "{access:?}");
        }
    }

    #[test]
    fn review_in_dynamic_requires_write_access() {
        let graph = build(&spec(true, AccessProfile::ReadOnly));
        let issues = validate(&graph);
        assert_eq!(
            issues.iter().map(|i| i.code).collect::<Vec<_>>(),
            vec![IssueCode::AccessNotAllowed]
        );
        assert_eq!(
            issues[0].message,
            "Review w trybie dynamicznym wymaga dostępu z zapisem."
        );
    }

    #[test]
    fn subagent_limit_only_on_orchestrator_and_in_range() {
        for bad in [0, 9] {
            let mut graph = dynamic_default();
            orchestrator(&mut graph).max_subagents = Some(bad);
            assert!(
                codes(&graph).contains(&IssueCode::InvalidSubagentLimit),
                "{bad}"
            );
        }
        for good in [1, 8] {
            let mut graph = dynamic_default();
            orchestrator(&mut graph).max_subagents = Some(good);
            assert_eq!(validate(&graph), vec![], "{good}");
        }
        // Brak pola na orkiestratorze jest poprawny.
        let mut none = dynamic_default();
        orchestrator(&mut none).max_subagents = None;
        assert_eq!(validate(&none), vec![]);
        // Pole na innej roli.
        let mut standard = analysis_implementation_review();
        if let NodeKind::Agent(a) = node_mut(&mut standard, "implementation") {
            a.max_subagents = Some(3);
        }
        assert_eq!(codes(&standard), vec![IssueCode::InvalidSubagentLimit]);
    }

    #[test]
    fn dynamic_spec_round_trips_through_graph() {
        for s in [
            spec(true, AccessProfile::WorkspaceWrite),
            spec(false, AccessProfile::ReadOnly),
            DynamicSpec {
                model: Some("opus".into()),
                max_subagents: 5,
                timeout_secs: 7200,
                instruction: "Inna instrukcja".into(),
                ..DynamicSpec::default()
            },
        ] {
            assert_eq!(spec_of(&build(&s)), Some(s.clone()));
            // Pozycje są ignorowane.
            let mut moved = build(&s);
            for n in &mut moved.nodes {
                n.position.x += 13.0;
            }
            assert_eq!(spec_of(&moved), Some(s));
            assert_eq!(validate(&moved), vec![]);
        }
        assert_eq!(spec_of(&analysis_implementation_review()), None);
    }

    #[test]
    fn standard_roles_keep_their_required_access() {
        for (role, allowed) in [
            (AgentRole::Analysis, vec![AccessProfile::ReadOnly]),
            (AgentRole::Review, vec![AccessProfile::ReadOnly]),
            (
                AgentRole::Implementation,
                vec![AccessProfile::WorkspaceWrite],
            ),
            (AgentRole::Fix, vec![AccessProfile::WorkspaceWrite]),
        ] {
            assert_eq!(role.allowed_access(), allowed.as_slice());
            assert_eq!(allowed, vec![role.required_access()]);
        }
        let mut graph = analysis_implementation_review();
        if let NodeKind::Agent(a) = node_mut(&mut graph, "implementation") {
            a.access = AccessProfile::ReadOnly;
        }
        assert_eq!(codes(&graph), vec![IssueCode::AccessNotAllowed]);
    }
}
