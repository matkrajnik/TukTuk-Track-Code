/**
 * Walidacja grafu: przeniesienie 1:1 `workflow_core::validation` (Rust).
 * Aplikacja zawsze sprawdza graf w Rust; ta kopia służy podglądowi
 * i natychmiastowej informacji w edytorze. Zmiana reguł w Rust wymaga
 * zmiany tutaj (te same kody, komunikaty i kolejność problemów).
 */
import {
  MAX_EDGES,
  MAX_INSTRUCTION_CHARS,
  MAX_NODES,
  MAX_REPAIR_ROUNDS,
  MAX_TIMEOUT_SECS,
  MIN_TIMEOUT_SECS,
  WORKFLOW_SCHEMA_VERSION,
  nodeTypeNames,
  outputFields,
  outputPorts,
  portNames,
  requiredAccess,
  type ConditionConfig,
  type FieldType,
  type IssueCode,
  type WorkflowGraph,
  type WorkflowIssue,
  type WorkflowNode,
} from "./workflows";

const graphIssue = (code: IssueCode, message: string): WorkflowIssue => ({
  code,
  message,
});
const nodeIssue = (
  code: IssueCode,
  nodeId: string,
  message: string,
): WorkflowIssue => ({ code, nodeId, message });
const edgeIssue = (
  code: IssueCode,
  edgeId: string,
  message: string,
): WorkflowIssue => ({ code, edgeId, message });

export function validateWorkflow(graph: WorkflowGraph): WorkflowIssue[] {
  if (graph.schemaVersion !== WORKFLOW_SCHEMA_VERSION)
    return [
      graphIssue(
        "unsupportedSchema",
        `Nieobsługiwana wersja formatu procesu: ${graph.schemaVersion} (obsługiwana: ${WORKFLOW_SCHEMA_VERSION}).`,
      ),
    ];
  if (graph.nodes.length > MAX_NODES || graph.edges.length > MAX_EDGES)
    return [
      graphIssue(
        "tooLarge",
        `Proces może mieć najwyżej ${MAX_NODES} kroków i ${MAX_EDGES} połączeń.`,
      ),
    ];
  const issues: WorkflowIssue[] = [];
  checkIds(graph, issues);
  checkTriggerAndEnd(graph, issues);
  checkEdges(graph, issues);
  if (issues.length) return issues;
  checkPorts(graph, issues);
  const index = buildIndex(graph);
  checkReachability(graph, index, issues);
  checkCycles(graph, index, issues);
  for (const node of graph.nodes) checkNodeConfig(node, index, issues);
  return issues;
}

function checkIds(graph: WorkflowGraph, issues: WorkflowIssue[]): void {
  const seen = new Set<string>();
  for (const node of graph.nodes) {
    if (!node.id.trim())
      issues.push(graphIssue("emptyId", "Krok bez identyfikatora."));
    else if (seen.has(node.id))
      issues.push(
        nodeIssue(
          "duplicateId",
          node.id,
          `Identyfikator kroku „${node.id}” występuje więcej niż raz.`,
        ),
      );
    else seen.add(node.id);
    if (!node.label.trim())
      issues.push(nodeIssue("emptyLabel", node.id, "Krok musi mieć nazwę."));
  }
  const seenEdges = new Set<string>();
  for (const edge of graph.edges) {
    if (!edge.id.trim())
      issues.push(graphIssue("emptyId", "Połączenie bez identyfikatora."));
    else if (seenEdges.has(edge.id))
      issues.push(
        edgeIssue(
          "duplicateId",
          edge.id,
          `Identyfikator połączenia „${edge.id}” występuje więcej niż raz.`,
        ),
      );
    else seenEdges.add(edge.id);
  }
}

function checkTriggerAndEnd(graph: WorkflowGraph, issues: WorkflowIssue[]): void {
  const triggers = graph.nodes.filter((n) => n.kind.type === "trigger");
  if (!triggers.length)
    issues.push(graphIssue("missingTrigger", "Proces musi mieć krok Start."));
  for (const extra of triggers.slice(1))
    issues.push(
      nodeIssue(
        "multipleTriggers",
        extra.id,
        "Proces może mieć tylko jeden krok Start.",
      ),
    );
  if (!graph.nodes.some((n) => n.kind.type === "end"))
    issues.push(
      graphIssue("missingEnd", "Proces musi mieć co najmniej jeden krok Koniec."),
    );
}

function findNode(graph: WorkflowGraph, id: string): WorkflowNode | undefined {
  return graph.nodes.find((n) => n.id === id);
}

function checkEdges(graph: WorkflowGraph, issues: WorkflowIssue[]): void {
  for (const edge of graph.edges) {
    const source = findNode(graph, edge.source);
    const target = findNode(graph, edge.target);
    if (!source || !target) {
      issues.push(
        edgeIssue(
          "danglingEdge",
          edge.id,
          "Połączenie wskazuje nieistniejący krok.",
        ),
      );
      continue;
    }
    if (!outputPorts(source.kind).includes(edge.sourcePort))
      issues.push(
        edgeIssue(
          "invalidPort",
          edge.id,
          `Krok typu ${nodeTypeNames[source.kind.type]} nie ma wyjścia „${portNames[edge.sourcePort]}”.`,
        ),
      );
    if (target.kind.type === "trigger")
      issues.push(
        edgeIssue(
          "edgeIntoTrigger",
          edge.id,
          "Do kroku Start nie może prowadzić żadne połączenie.",
        ),
      );
  }
}

function checkPorts(graph: WorkflowGraph, issues: WorkflowIssue[]): void {
  for (const node of graph.nodes)
    for (const port of outputPorts(node.kind)) {
      const count = graph.edges.filter(
        (e) => e.source === node.id && e.sourcePort === port,
      ).length;
      if (count === 0)
        issues.push(
          nodeIssue(
            "missingConnection",
            node.id,
            `Wyjście „${portNames[port]}” kroku „${node.label}” nie jest połączone.`,
          ),
        );
      else if (count > 1)
        issues.push(
          nodeIssue(
            "duplicateConnection",
            node.id,
            `Wyjście „${portNames[port]}” kroku „${node.label}” ma więcej niż jedno połączenie.`,
          ),
        );
    }
}

interface Index {
  trigger: string | undefined;
  nodes: Map<string, WorkflowNode>;
  successors: Map<string, string[]>;
  predecessors: Map<string, string[]>;
}

function buildIndex(graph: WorkflowGraph): Index {
  const nodes = new Map(graph.nodes.map((n) => [n.id, n]));
  const successors = new Map<string, string[]>();
  const predecessors = new Map<string, string[]>();
  for (const edge of graph.edges) {
    successors.set(edge.source, [
      ...(successors.get(edge.source) ?? []),
      edge.target,
    ]);
    predecessors.set(edge.target, [
      ...(predecessors.get(edge.target) ?? []),
      edge.source,
    ]);
  }
  const trigger = graph.nodes.find((n) => n.kind.type === "trigger")?.id;
  return { trigger, nodes, successors, predecessors };
}

function reach(
  links: Map<string, string[]>,
  starts: readonly string[],
): Set<string> {
  const seen = new Set(starts);
  const stack = [...starts];
  while (stack.length) {
    const current = stack.pop() as string;
    for (const next of links.get(current) ?? [])
      if (!seen.has(next)) {
        seen.add(next);
        stack.push(next);
      }
  }
  return seen;
}

/** Czy `from` leży na każdej drodze od Startu do `to` (`Index::dominates`). */
function dominates(index: Index, from: string, to: string): boolean {
  const trigger = index.trigger;
  if (trigger === undefined) return false;
  if (from === trigger) return true;
  const seen = new Set([trigger]);
  const stack = [trigger];
  while (stack.length) {
    const current = stack.pop() as string;
    for (const next of index.successors.get(current) ?? [])
      if (next !== from && !seen.has(next)) {
        seen.add(next);
        stack.push(next);
      }
  }
  return !seen.has(to);
}

function checkReachability(
  graph: WorkflowGraph,
  index: Index,
  issues: WorkflowIssue[],
): void {
  const trigger = graph.nodes.find((n) => n.kind.type === "trigger");
  if (!trigger) return;
  const reachable = reach(index.successors, [trigger.id]);
  const ends = graph.nodes.filter((n) => n.kind.type === "end").map((n) => n.id);
  const finishing = reach(index.predecessors, ends);
  for (const node of graph.nodes) {
    if (!reachable.has(node.id))
      issues.push(
        nodeIssue(
          "unreachable",
          node.id,
          `Do kroku „${node.label}” nie prowadzi żadna droga od Startu.`,
        ),
      );
    else if (!finishing.has(node.id))
      issues.push(
        nodeIssue(
          "noPathToEnd",
          node.id,
          `Z kroku „${node.label}” nie da się dojść do kroku Koniec.`,
        ),
      );
  }
}

function checkCycles(
  graph: WorkflowGraph,
  index: Index,
  issues: WorkflowIssue[],
): void {
  const inLoop = new Set<string>();
  for (const component of stronglyConnected(graph, index)) {
    const isCycle =
      component.length > 1 ||
      (index.successors.get(component[0]) ?? []).includes(component[0]);
    if (!isCycle) continue;
    for (const id of component) inLoop.add(id);
    const issue = checkRepairLoop(graph, component);
    if (issue) issues.push(issue);
  }
  for (const node of graph.nodes)
    if (node.kind.type === "condition")
      checkRepairLimit(node, node.kind, inLoop.has(node.id), issues);
}

function checkRepairLoop(
  graph: WorkflowGraph,
  component: string[],
): WorkflowIssue | null {
  const members = new Set(component);
  const nodes = component
    .map((id) => findNode(graph, id))
    .filter((n): n is WorkflowNode => n !== undefined);
  const conditions = nodes.filter((n) => n.kind.type === "condition");
  const first = component[0];
  if (conditions.length !== 1)
    return nodeIssue(
      "invalidCycle",
      first,
      "Pętla jest dozwolona tylko jako blok poprawek z jednym warunkiem.",
    );
  if (nodes.some((n) => n.kind.type !== "condition" && n.kind.type !== "agent"))
    return nodeIssue(
      "invalidCycle",
      first,
      "Blok poprawek może zawierać tylko kroki agentów i jeden warunek.",
    );
  const condition = conditions[0];
  const inside = graph.edges.filter(
    (e) => e.source === condition.id && members.has(e.target),
  ).length;
  if (inside !== 1)
    return nodeIssue(
      "invalidCycle",
      condition.id,
      "Warunek bloku poprawek musi mieć jedno wyjście w pętli i jedno poza nią.",
    );
  return null;
}

function checkRepairLimit(
  node: WorkflowNode,
  config: ConditionConfig,
  inLoop: boolean,
  issues: WorkflowIssue[],
): void {
  const limit = config.repairLimit;
  if (limit === null && inLoop)
    issues.push(
      nodeIssue(
        "invalidRepairLimit",
        node.id,
        `Warunek pętli poprawek musi mieć limit rund (1–${MAX_REPAIR_ROUNDS}).`,
      ),
    );
  else if (limit !== null && inLoop && (limit < 1 || limit > MAX_REPAIR_ROUNDS))
    issues.push(
      nodeIssue(
        "invalidRepairLimit",
        node.id,
        `Limit rund poprawek musi być od 1 do ${MAX_REPAIR_ROUNDS}.`,
      ),
    );
  else if (limit !== null && !inLoop)
    issues.push(
      nodeIssue(
        "repairLimitOutsideLoop",
        node.id,
        "Limit rund ma sens tylko w warunku zamykającym blok poprawek.",
      ),
    );
}

/** Silnie spójne składowe (Tarjan, iteracyjnie), kolejność jak w Rust. */
function stronglyConnected(graph: WorkflowGraph, index: Index): string[][] {
  const order = new Map<string, number>();
  const low = new Map<string, number>();
  const onStack = new Set<string>();
  const stack: string[] = [];
  const components: string[][] = [];
  let counter = 0;
  const visit = (id: string) => {
    order.set(id, counter);
    low.set(id, counter);
    counter += 1;
    stack.push(id);
    onStack.add(id);
  };
  for (const root of graph.nodes.map((n) => n.id)) {
    if (order.has(root)) continue;
    const frames: [string, number][] = [[root, 0]];
    visit(root);
    while (frames.length) {
      const frame = frames[frames.length - 1];
      const [node, position] = frame;
      const successors = index.successors.get(node) ?? [];
      if (position < successors.length) {
        frame[1] = position + 1;
        const child = successors[position];
        if (!order.has(child)) {
          visit(child);
          frames.push([child, 0]);
        } else if (onStack.has(child))
          low.set(node, Math.min(low.get(node)!, order.get(child)!));
        continue;
      }
      frames.pop();
      const parent = frames[frames.length - 1];
      if (parent)
        low.set(parent[0], Math.min(low.get(parent[0])!, low.get(node)!));
      if (low.get(node) === order.get(node)) {
        const component: string[] = [];
        for (;;) {
          const member = stack.pop() as string;
          onStack.delete(member);
          component.push(member);
          if (member === node) break;
        }
        components.push(component);
      }
    }
  }
  return components;
}

function checkNodeConfig(
  node: WorkflowNode,
  index: Index,
  issues: WorkflowIssue[],
): void {
  const kind = node.kind;
  if (kind.type === "agent") {
    if (!kind.instruction.trim())
      issues.push(
        nodeIssue(
          "emptyInstruction",
          node.id,
          `Agent „${node.label}” nie ma instrukcji.`,
        ),
      );
    if ([...kind.instruction].length > MAX_INSTRUCTION_CHARS)
      issues.push(
        nodeIssue(
          "instructionTooLong",
          node.id,
          `Instrukcja może mieć najwyżej ${MAX_INSTRUCTION_CHARS} znaków.`,
        ),
      );
    if (kind.model !== null && !kind.model.trim())
      issues.push(
        nodeIssue(
          "emptyModel",
          node.id,
          "Nazwa modelu jest pusta; usuń ją, by wybrać model przy uruchomieniu.",
        ),
      );
    if (kind.timeoutSecs < MIN_TIMEOUT_SECS || kind.timeoutSecs > MAX_TIMEOUT_SECS)
      issues.push(
        nodeIssue(
          "invalidTimeout",
          node.id,
          `Limit czasu musi wynosić od ${MIN_TIMEOUT_SECS / 60} min do ${MAX_TIMEOUT_SECS / 3600} h.`,
        ),
      );
    if (kind.access !== requiredAccess(kind.role))
      issues.push(
        nodeIssue(
          "accessNotAllowed",
          node.id,
          "Analiza i review działają tylko z odczytem; zapis mają implementacja i poprawki.",
        ),
      );
  } else if (kind.type === "condition") checkCondition(node, kind, index, issues);
}

function checkCondition(
  node: WorkflowNode,
  condition: ConditionConfig,
  index: Index,
  issues: WorkflowIssue[],
): void {
  const source = index.nodes.get(condition.field.nodeId);
  if (!source || source.kind.type !== "agent") {
    issues.push(
      nodeIssue(
        "invalidFieldRef",
        node.id,
        "Warunek musi wskazywać wynik kroku agenta.",
      ),
    );
    return;
  }
  if (!dominates(index, source.id, node.id))
    issues.push(
      nodeIssue(
        "fieldNotAvailable",
        node.id,
        `Wynik kroku „${source.label}” nie jest dostępny przed tym warunkiem.`,
      ),
    );
  const field = outputFields[source.kind.role].find(
    ([name]) => name === condition.field.field,
  );
  if (!field) {
    issues.push(
      nodeIssue(
        "unknownField",
        node.id,
        `Wynik kroku „${source.label}” nie ma pola „${condition.field.field}”.`,
      ),
    );
    return;
  }
  const fieldType = field[1];
  const ordered = condition.op === "greaterThan" || condition.op === "lessThan";
  if (ordered && fieldType.type !== "number")
    issues.push(
      nodeIssue(
        "invalidOperator",
        node.id,
        "Porównanie „większe/mniejsze” działa tylko dla liczb.",
      ),
    );
  if (!valueMatches(fieldType, condition.value))
    issues.push(
      nodeIssue(
        "typeMismatch",
        node.id,
        `Wartość warunku nie pasuje do typu pola „${condition.field.field}”.`,
      ),
    );
}

function valueMatches(
  fieldType: FieldType,
  value: ConditionConfig["value"],
): boolean {
  switch (fieldType.type) {
    case "text":
      return typeof value === "string";
    case "number":
      return typeof value === "number";
    case "boolean":
      return typeof value === "boolean";
    case "choice":
      return typeof value === "string" && fieldType.options.includes(value);
  }
}
