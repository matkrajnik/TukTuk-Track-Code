//! Walidacja definicji procesu (PLAN_WDROZENIA.md, 5.1).
//!
//! `validate` zwraca pełną listę problemów, żeby edytor mógł wskazać każdy
//! węzeł. Pusta lista oznacza graf gotowy do publikacji i uruchomienia.
//! Ta sama funkcja chroni komendy IPC, więc błędny graf nie przejdzie
//! także z pominięciem interfejsu.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use serde_json::Value;

use crate::model::{
    CompareOp, ConditionConfig, FieldType, Node, NodeKind, WorkflowGraph, MAX_EDGES,
    MAX_INSTRUCTION_CHARS, MAX_NODES, MAX_REPAIR_ROUNDS, MAX_TIMEOUT_SECS, MIN_TIMEOUT_SECS,
    SCHEMA_VERSION,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IssueCode {
    UnsupportedSchema,
    TooLarge,
    EmptyId,
    DuplicateId,
    EmptyLabel,
    MissingTrigger,
    MultipleTriggers,
    MissingEnd,
    DanglingEdge,
    InvalidPort,
    EdgeIntoTrigger,
    MissingConnection,
    DuplicateConnection,
    Unreachable,
    NoPathToEnd,
    InvalidCycle,
    InvalidRepairLimit,
    RepairLimitOutsideLoop,
    EmptyInstruction,
    InstructionTooLong,
    EmptyModel,
    InvalidTimeout,
    AccessNotAllowed,
    InvalidFieldRef,
    UnknownField,
    FieldNotAvailable,
    InvalidOperator,
    TypeMismatch,
}

/// Problem w grafie z miejscem i komunikatem po polsku.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub code: IssueCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_id: Option<String>,
    pub message: String,
}

impl Issue {
    fn graph(code: IssueCode, message: impl Into<String>) -> Self {
        Self {
            code,
            node_id: None,
            edge_id: None,
            message: message.into(),
        }
    }

    fn node(code: IssueCode, node: &str, message: impl Into<String>) -> Self {
        Self {
            node_id: Some(node.to_owned()),
            ..Self::graph(code, message)
        }
    }

    fn edge(code: IssueCode, edge: &str, message: impl Into<String>) -> Self {
        Self {
            edge_id: Some(edge.to_owned()),
            ..Self::graph(code, message)
        }
    }
}

pub fn validate(graph: &WorkflowGraph) -> Vec<Issue> {
    if graph.schema_version != SCHEMA_VERSION {
        return vec![Issue::graph(
            IssueCode::UnsupportedSchema,
            format!(
                "Nieobsługiwana wersja formatu procesu: {} (obsługiwana: {SCHEMA_VERSION}).",
                graph.schema_version
            ),
        )];
    }
    if graph.nodes.len() > MAX_NODES || graph.edges.len() > MAX_EDGES {
        return vec![Issue::graph(
            IssueCode::TooLarge,
            format!("Proces może mieć najwyżej {MAX_NODES} kroków i {MAX_EDGES} połączeń."),
        )];
    }

    let mut issues = Vec::new();
    check_ids(graph, &mut issues);
    check_trigger_and_end(graph, &mut issues);
    check_edges(graph, &mut issues);
    // Dalsze reguły zakładają spójne identyfikatory i krawędzie.
    if !issues.is_empty() {
        return issues;
    }
    check_ports(graph, &mut issues);
    let graph_index = Index::new(graph);
    check_reachability(graph, &graph_index, &mut issues);
    check_cycles(graph, &graph_index, &mut issues);
    for node in &graph.nodes {
        check_node_config(node, &graph_index, &mut issues);
    }
    issues
}

fn check_ids(graph: &WorkflowGraph, issues: &mut Vec<Issue>) {
    let mut seen = HashSet::new();
    for node in &graph.nodes {
        if node.id.trim().is_empty() {
            issues.push(Issue::graph(IssueCode::EmptyId, "Krok bez identyfikatora."));
        } else if !seen.insert(node.id.as_str()) {
            issues.push(Issue::node(
                IssueCode::DuplicateId,
                &node.id,
                format!(
                    "Identyfikator kroku „{}” występuje więcej niż raz.",
                    node.id
                ),
            ));
        }
        if node.label.trim().is_empty() {
            issues.push(Issue::node(
                IssueCode::EmptyLabel,
                &node.id,
                "Krok musi mieć nazwę.",
            ));
        }
    }
    let mut seen_edges = HashSet::new();
    for edge in &graph.edges {
        if edge.id.trim().is_empty() {
            issues.push(Issue::graph(
                IssueCode::EmptyId,
                "Połączenie bez identyfikatora.",
            ));
        } else if !seen_edges.insert(edge.id.as_str()) {
            issues.push(Issue::edge(
                IssueCode::DuplicateId,
                &edge.id,
                format!(
                    "Identyfikator połączenia „{}” występuje więcej niż raz.",
                    edge.id
                ),
            ));
        }
    }
}

fn check_trigger_and_end(graph: &WorkflowGraph, issues: &mut Vec<Issue>) {
    let triggers: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Trigger))
        .collect();
    match triggers.as_slice() {
        [] => issues.push(Issue::graph(
            IssueCode::MissingTrigger,
            "Proces musi mieć krok Start.",
        )),
        [_] => {}
        [_, rest @ ..] => {
            for extra in rest {
                issues.push(Issue::node(
                    IssueCode::MultipleTriggers,
                    &extra.id,
                    "Proces może mieć tylko jeden krok Start.",
                ));
            }
        }
    }
    if !graph
        .nodes
        .iter()
        .any(|n| matches!(n.kind, NodeKind::End(_)))
    {
        issues.push(Issue::graph(
            IssueCode::MissingEnd,
            "Proces musi mieć co najmniej jeden krok Koniec.",
        ));
    }
}

fn check_edges(graph: &WorkflowGraph, issues: &mut Vec<Issue>) {
    for edge in &graph.edges {
        let (Some(source), Some(target)) = (graph.node(&edge.source), graph.node(&edge.target))
        else {
            issues.push(Issue::edge(
                IssueCode::DanglingEdge,
                &edge.id,
                "Połączenie wskazuje nieistniejący krok.",
            ));
            continue;
        };
        if !source.kind.output_ports().contains(&edge.source_port) {
            issues.push(Issue::edge(
                IssueCode::InvalidPort,
                &edge.id,
                format!(
                    "Krok typu {} nie ma wyjścia „{}”.",
                    source.kind.name(),
                    edge.source_port.name()
                ),
            ));
        }
        if matches!(target.kind, NodeKind::Trigger) {
            issues.push(Issue::edge(
                IssueCode::EdgeIntoTrigger,
                &edge.id,
                "Do kroku Start nie może prowadzić żadne połączenie.",
            ));
        }
    }
}

fn check_ports(graph: &WorkflowGraph, issues: &mut Vec<Issue>) {
    for node in &graph.nodes {
        for &port in node.kind.output_ports() {
            let count = graph
                .edges
                .iter()
                .filter(|e| e.source == node.id && e.source_port == port)
                .count();
            match count {
                1 => {}
                0 => issues.push(Issue::node(
                    IssueCode::MissingConnection,
                    &node.id,
                    format!(
                        "Wyjście „{}” kroku „{}” nie jest połączone.",
                        port.name(),
                        node.label
                    ),
                )),
                _ => issues.push(Issue::node(
                    IssueCode::DuplicateConnection,
                    &node.id,
                    format!(
                        "Wyjście „{}” kroku „{}” ma więcej niż jedno połączenie.",
                        port.name(),
                        node.label
                    ),
                )),
            }
        }
    }
}

/// Węzły po identyfikatorze i listy sąsiedztwa po zweryfikowanych krawędziach.
struct Index<'g> {
    trigger: Option<&'g str>,
    nodes: HashMap<&'g str, &'g Node>,
    successors: HashMap<&'g str, Vec<&'g str>>,
    predecessors: HashMap<&'g str, Vec<&'g str>>,
}

impl<'g> Index<'g> {
    fn new(graph: &'g WorkflowGraph) -> Self {
        let nodes = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut predecessors: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in &graph.edges {
            successors
                .entry(edge.source.as_str())
                .or_default()
                .push(edge.target.as_str());
            predecessors
                .entry(edge.target.as_str())
                .or_default()
                .push(edge.source.as_str());
        }
        let trigger = graph
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Trigger))
            .map(|n| n.id.as_str());
        Self {
            trigger,
            nodes,
            successors,
            predecessors,
        }
    }

    fn reach(&self, starts: &[&'g str], forward: bool) -> HashSet<&'g str> {
        let links = if forward {
            &self.successors
        } else {
            &self.predecessors
        };
        let mut seen: HashSet<&str> = starts.iter().copied().collect();
        let mut stack: Vec<&str> = starts.to_vec();
        while let Some(current) = stack.pop() {
            for &next in links.get(current).map(Vec::as_slice).unwrap_or(&[]) {
                if seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        seen
    }

    /// Czy `from` leży na każdej drodze od Startu do `to` (dominacja).
    /// Wtedy wynik `from` na pewno istnieje, gdy wykonanie dojdzie do `to`;
    /// sama osiągalność nie wystarcza (krok z innej gałęzi albo krok za
    /// warunkiem w pętli poprawek, który w pierwszej rundzie jeszcze nie działał).
    fn dominates(&self, from: &'g str, to: &str) -> bool {
        let Some(trigger) = self.trigger else {
            return false;
        };
        if from == trigger {
            return true;
        }
        let mut seen: HashSet<&str> = HashSet::from([trigger]);
        let mut stack = vec![trigger];
        while let Some(current) = stack.pop() {
            for &next in self
                .successors
                .get(current)
                .map(Vec::as_slice)
                .unwrap_or(&[])
            {
                if next != from && seen.insert(next) {
                    stack.push(next);
                }
            }
        }
        !seen.contains(to)
    }
}

fn check_reachability(graph: &WorkflowGraph, index: &Index, issues: &mut Vec<Issue>) {
    let Some(trigger) = graph
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Trigger))
    else {
        return;
    };
    let reachable = index.reach(&[trigger.id.as_str()], true);
    let ends: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::End(_)))
        .map(|n| n.id.as_str())
        .collect();
    let finishing = index.reach(&ends, false);
    for node in &graph.nodes {
        if !reachable.contains(node.id.as_str()) {
            issues.push(Issue::node(
                IssueCode::Unreachable,
                &node.id,
                format!(
                    "Do kroku „{}” nie prowadzi żadna droga od Startu.",
                    node.label
                ),
            ));
        } else if !finishing.contains(node.id.as_str()) {
            issues.push(Issue::node(
                IssueCode::NoPathToEnd,
                &node.id,
                format!("Z kroku „{}” nie da się dojść do kroku Koniec.", node.label),
            ));
        }
    }
}

/// Jedyny dopuszczalny cykl to blok poprawek (5.1): warunek z limitem rund
/// i wyłącznie kroki agentów, z jednym wyjściem warunku w pętli i jednym poza nią.
fn check_cycles(graph: &WorkflowGraph, index: &Index, issues: &mut Vec<Issue>) {
    let mut in_loop: HashSet<&str> = HashSet::new();
    for component in strongly_connected(graph, index) {
        let is_cycle = component.len() > 1
            || index
                .successors
                .get(component[0])
                .is_some_and(|next| next.contains(&component[0]));
        if !is_cycle {
            continue;
        }
        in_loop.extend(component.iter().copied());
        if let Err(issue) = check_repair_loop(graph, &component) {
            issues.push(issue);
        }
    }
    for node in &graph.nodes {
        if let NodeKind::Condition(config) = &node.kind {
            check_repair_limit(node, config, in_loop.contains(node.id.as_str()), issues);
        }
    }
}

fn check_repair_loop(graph: &WorkflowGraph, component: &[&str]) -> Result<(), Issue> {
    let members: HashSet<&str> = component.iter().copied().collect();
    let nodes: Vec<&Node> = component.iter().filter_map(|id| graph.node(id)).collect();
    let conditions: Vec<&Node> = nodes
        .iter()
        .copied()
        .filter(|n| matches!(n.kind, NodeKind::Condition(_)))
        .collect();
    let first = component[0];
    let invalid = |message: &str| Issue::node(IssueCode::InvalidCycle, first, message);
    let [condition] = conditions.as_slice() else {
        return Err(invalid(
            "Pętla jest dozwolona tylko jako blok poprawek z jednym warunkiem.",
        ));
    };
    if nodes
        .iter()
        .any(|n| !matches!(n.kind, NodeKind::Condition(_) | NodeKind::Agent(_)))
    {
        return Err(invalid(
            "Blok poprawek może zawierać tylko kroki agentów i jeden warunek.",
        ));
    }
    let inside = graph
        .edges
        .iter()
        .filter(|e| e.source == condition.id && members.contains(e.target.as_str()))
        .count();
    if inside != 1 {
        return Err(Issue::node(
            IssueCode::InvalidCycle,
            &condition.id,
            "Warunek bloku poprawek musi mieć jedno wyjście w pętli i jedno poza nią.",
        ));
    }
    Ok(())
}

fn check_repair_limit(
    node: &Node,
    config: &ConditionConfig,
    in_loop: bool,
    issues: &mut Vec<Issue>,
) {
    match (config.repair_limit, in_loop) {
        (None, true) => issues.push(Issue::node(
            IssueCode::InvalidRepairLimit,
            &node.id,
            format!("Warunek pętli poprawek musi mieć limit rund (1–{MAX_REPAIR_ROUNDS})."),
        )),
        (Some(limit), true) if !(1..=MAX_REPAIR_ROUNDS).contains(&limit) => {
            issues.push(Issue::node(
                IssueCode::InvalidRepairLimit,
                &node.id,
                format!("Limit rund poprawek musi być od 1 do {MAX_REPAIR_ROUNDS}."),
            ))
        }
        (Some(_), false) => issues.push(Issue::node(
            IssueCode::RepairLimitOutsideLoop,
            &node.id,
            "Limit rund ma sens tylko w warunku zamykającym blok poprawek.",
        )),
        _ => {}
    }
}

/// Silnie spójne składowe (Tarjan, iteracyjnie, bez rekursji).
fn strongly_connected<'g>(graph: &'g WorkflowGraph, index: &Index<'g>) -> Vec<Vec<&'g str>> {
    let mut order: HashMap<&str, usize> = HashMap::new();
    let mut low: HashMap<&str, usize> = HashMap::new();
    let mut on_stack: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut components = Vec::new();
    let mut counter = 0;

    for root in graph.nodes.iter().map(|n| n.id.as_str()) {
        if order.contains_key(root) {
            continue;
        }
        // Ramka: węzeł i pozycja następnego sąsiada do odwiedzenia.
        let mut frames: Vec<(&str, usize)> = vec![(root, 0)];
        order.insert(root, counter);
        low.insert(root, counter);
        counter += 1;
        stack.push(root);
        on_stack.insert(root);

        while let Some(&(node, position)) = frames.last() {
            let successors = index.successors.get(node).map(Vec::as_slice).unwrap_or(&[]);
            if let Some(&child) = successors.get(position) {
                if let Some(top) = frames.last_mut() {
                    top.1 += 1;
                }
                if !order.contains_key(child) {
                    order.insert(child, counter);
                    low.insert(child, counter);
                    counter += 1;
                    stack.push(child);
                    on_stack.insert(child);
                    frames.push((child, 0));
                } else if on_stack.contains(child) {
                    let lowest = low[node].min(order[child]);
                    low.insert(node, lowest);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                let lowest = low[parent].min(low[node]);
                low.insert(parent, lowest);
            }
            if low[node] == order[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack.remove(member);
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                components.push(component);
            }
        }
    }
    components
}

fn check_node_config(node: &Node, index: &Index, issues: &mut Vec<Issue>) {
    match &node.kind {
        NodeKind::Agent(agent) => {
            if agent.instruction.trim().is_empty() {
                issues.push(Issue::node(
                    IssueCode::EmptyInstruction,
                    &node.id,
                    format!("Agent „{}” nie ma instrukcji.", node.label),
                ));
            }
            if agent.instruction.chars().count() > MAX_INSTRUCTION_CHARS {
                issues.push(Issue::node(
                    IssueCode::InstructionTooLong,
                    &node.id,
                    format!("Instrukcja może mieć najwyżej {MAX_INSTRUCTION_CHARS} znaków."),
                ));
            }
            if agent.model.as_deref().is_some_and(|m| m.trim().is_empty()) {
                issues.push(Issue::node(
                    IssueCode::EmptyModel,
                    &node.id,
                    "Nazwa modelu jest pusta; usuń ją, by wybrać model przy uruchomieniu.",
                ));
            }
            if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&agent.timeout_secs) {
                issues.push(Issue::node(
                    IssueCode::InvalidTimeout,
                    &node.id,
                    format!(
                        "Limit czasu musi wynosić od {} min do {} h.",
                        MIN_TIMEOUT_SECS / 60,
                        MAX_TIMEOUT_SECS / 3600
                    ),
                ));
            }
            if agent.access != agent.role.required_access() {
                issues.push(Issue::node(
                    IssueCode::AccessNotAllowed,
                    &node.id,
                    "Analiza i review działają tylko z odczytem; zapis mają implementacja i poprawki.",
                ));
            }
        }
        NodeKind::Condition(condition) => check_condition(node, condition, index, issues),
        NodeKind::Trigger | NodeKind::Approval(_) | NodeKind::End(_) => {}
    }
}

fn check_condition(
    node: &Node,
    condition: &ConditionConfig,
    index: &Index,
    issues: &mut Vec<Issue>,
) {
    let Some(source) = index.nodes.get(condition.field.node_id.as_str()).copied() else {
        issues.push(Issue::node(
            IssueCode::InvalidFieldRef,
            &node.id,
            "Warunek musi wskazywać wynik kroku agenta.",
        ));
        return;
    };
    let NodeKind::Agent(agent) = &source.kind else {
        issues.push(Issue::node(
            IssueCode::InvalidFieldRef,
            &node.id,
            "Warunek musi wskazywać wynik kroku agenta.",
        ));
        return;
    };
    if !index.dominates(source.id.as_str(), &node.id) {
        issues.push(Issue::node(
            IssueCode::FieldNotAvailable,
            &node.id,
            format!(
                "Wynik kroku „{}” nie jest dostępny przed tym warunkiem.",
                source.label
            ),
        ));
    }
    let Some(field_type) = agent.role.output_field(&condition.field.field) else {
        issues.push(Issue::node(
            IssueCode::UnknownField,
            &node.id,
            format!(
                "Wynik kroku „{}” nie ma pola „{}”.",
                source.label, condition.field.field
            ),
        ));
        return;
    };
    let ordered = matches!(condition.op, CompareOp::GreaterThan | CompareOp::LessThan);
    if ordered && field_type != FieldType::Number {
        issues.push(Issue::node(
            IssueCode::InvalidOperator,
            &node.id,
            "Porównanie „większe/mniejsze” działa tylko dla liczb.",
        ));
    }
    if !value_matches(field_type, &condition.value) {
        issues.push(Issue::node(
            IssueCode::TypeMismatch,
            &node.id,
            format!(
                "Wartość warunku nie pasuje do typu pola „{}”.",
                condition.field.field
            ),
        ));
    }
}

fn value_matches(field_type: FieldType, value: &Value) -> bool {
    match field_type {
        FieldType::Text => value.is_string(),
        FieldType::Number => value.is_number(),
        FieldType::Boolean => value.is_boolean(),
        FieldType::Choice(options) => value.as_str().is_some_and(|v| options.contains(&v)),
    }
}
