/**
 * Edycja grafu procesu w edytorze Workflows: czyste funkcje bez mutacji
 * (każda zwraca nowy graf). Reguły poprawności sprawdza walidator
 * (`api/workflowValidate.ts`, w aplikacji także Rust); tu tylko operacje.
 */
import {
  requiredAccess,
  outputFields,
  type AgentRole,
  type CompareOp,
  type ConditionConfig,
  type FieldType,
  type NodeKind,
  type NodeType,
  type Port,
  type WorkflowGraph,
  type WorkflowNode,
} from "./api/workflows";

/** Niezapisany stan edytora jednego procesu. */
export interface EditorDraft {
  name: string;
  description: string;
  graph: WorkflowGraph;
}

/**
 * Niezapisane zmiany edytora zostawione przy przejściu do innego widoku
 * (na czas sesji okna): wracają przy ponownym otwarciu procesu, jeśli od
 * tego czasu nikt nie zapisał nowszej rewizji szkicu.
 */
export const unsavedDrafts = new Map<
  number,
  { revision: number; draft: EditorDraft }
>();

/** Typy kroków, które można dodać (Start jest jeden i istnieje od początku). */
export const addableTypes = ["agent", "condition", "approval", "end"] as const;
export type AddableType = (typeof addableTypes)[number];

/** Domyślny limit kroku agenta dodanego w edytorze: 30 minut. */
const DEFAULT_TIMEOUT_SECS = 30 * 60;
/** Odstęp nowego kroku od skrajnego prawego na canvasie. */
const NEW_NODE_GAP = 220;

export const roleNames: Record<AgentRole, string> = {
  analysis: "Analiza",
  implementation: "Implementacja",
  fix: "Poprawki",
  review: "Review",
};

export const opNames: Record<CompareOp, string> = {
  equals: "jest równe",
  notEquals: "jest różne od",
  greaterThan: "jest większe niż",
  lessThan: "jest mniejsze niż",
};

export const fieldNames: Record<string, string> = {
  summary: "podsumowanie",
  riskCount: "liczba ryzyk",
  changedFileCount: "liczba zmienionych plików",
  verdict: "werdykt",
  findingCount: "liczba uwag",
};

export const verdictNames: Record<string, string> = {
  approved: "zaakceptowane",
  changes_requested: "wymaga poprawek",
};

/** Identyfikator nowego kroku: typ i pierwszy wolny numer. */
export function newNodeId(graph: WorkflowGraph, type: NodeType): string {
  const taken = new Set(graph.nodes.map((n) => n.id));
  for (let i = 1; ; i++) {
    const id = `${type}-${i}`;
    if (!taken.has(id)) return id;
  }
}

function defaultKind(graph: WorkflowGraph, type: AddableType): NodeKind {
  switch (type) {
    case "agent":
      return {
        type,
        role: "analysis",
        provider: "claude",
        model: null,
        instruction: "",
        access: requiredAccess("analysis"),
        timeoutSecs: DEFAULT_TIMEOUT_SECS,
      };
    case "condition": {
      const review = graph.nodes.find(
        (n) => n.kind.type === "agent" && n.kind.role === "review",
      );
      return {
        type,
        field: { nodeId: review?.id ?? "", field: "verdict" },
        op: "equals",
        value: "approved",
        repairLimit: null,
      };
    }
    case "approval":
      return { type, description: "" };
    case "end":
      return { type, outcome: "succeeded" };
  }
}

const defaultLabels: Record<AddableType, string> = {
  agent: "Nowy agent",
  condition: "Nowy warunek",
  approval: "Akceptacja",
  end: "Koniec",
};

/** Dodaje krok na prawo od dotychczasowych; zwraca graf i id kroku. */
export function addNode(
  graph: WorkflowGraph,
  type: AddableType,
): { graph: WorkflowGraph; id: string } {
  const id = newNodeId(graph, type);
  const x =
    Math.max(0, ...graph.nodes.map((n) => n.position.x)) + NEW_NODE_GAP;
  const node: WorkflowNode = {
    id,
    label: defaultLabels[type],
    position: { x, y: 0 },
    kind: defaultKind(graph, type),
  };
  return { graph: { ...graph, nodes: [...graph.nodes, node] }, id };
}

/** Usuwa krok razem z jego połączeniami. */
export function removeNode(graph: WorkflowGraph, id: string): WorkflowGraph {
  return {
    ...graph,
    nodes: graph.nodes.filter((n) => n.id !== id),
    edges: graph.edges.filter((e) => e.source !== id && e.target !== id),
  };
}

export function updateNode(
  graph: WorkflowGraph,
  id: string,
  change: (node: WorkflowNode) => WorkflowNode,
): WorkflowGraph {
  return {
    ...graph,
    nodes: graph.nodes.map((n) => (n.id === id ? change(n) : n)),
  };
}

/** Cel wyjścia `port` kroku `source` (pierwsze połączenie albo null). */
export function connectionTarget(
  graph: WorkflowGraph,
  source: string,
  port: Port,
): string | null {
  return (
    graph.edges.find((e) => e.source === source && e.sourcePort === port)
      ?.target ?? null
  );
}

/**
 * Ustawia jedno połączenie wyjścia `port`: zastępuje dotychczasowe,
 * `target = null` odłącza wyjście.
 */
export function connect(
  graph: WorkflowGraph,
  source: string,
  port: Port,
  target: string | null,
): WorkflowGraph {
  const others = graph.edges.filter(
    (e) => !(e.source === source && e.sourcePort === port),
  );
  if (target === null) return { ...graph, edges: others };
  const taken = new Set(others.map((e) => e.id));
  let id = `${source}-${port}-${target}`;
  for (let i = 2; taken.has(id); i++) id = `${source}-${port}-${target}-${i}`;
  return {
    ...graph,
    edges: [...others, { id, source, sourcePort: port, target }],
  };
}

/** Kroki, do których może prowadzić wyjście (wszystkie poza Startem i sobą). */
export function connectableTargets(
  graph: WorkflowGraph,
  source: string,
): WorkflowNode[] {
  return graph.nodes.filter(
    (n) => n.id !== source && n.kind.type !== "trigger",
  );
}

export interface FieldOption {
  nodeId: string;
  field: string;
  type: FieldType;
  label: string;
}

/** Pola wyników agentów, które może porównać warunek. */
export function conditionFields(graph: WorkflowGraph): FieldOption[] {
  return graph.nodes.flatMap((n) =>
    n.kind.type === "agent"
      ? outputFields[n.kind.role].map(([field, type]) => ({
          nodeId: n.id,
          field,
          type,
          label: `${n.label}: ${fieldNames[field] ?? field}`,
        }))
      : [],
  );
}

/** Operatory zgodne z typem pola (większe/mniejsze tylko dla liczb). */
export function opsFor(type: FieldType | undefined): CompareOp[] {
  return type?.type === "number"
    ? ["equals", "notEquals", "greaterThan", "lessThan"]
    : ["equals", "notEquals"];
}

/** Wartość domyślna warunku po zmianie pola. */
export function defaultValue(type: FieldType): ConditionConfig["value"] {
  switch (type.type) {
    case "choice":
      return type.options[0] ?? "";
    case "number":
      return 0;
    case "boolean":
      return true;
    case "text":
      return "";
  }
}

/** Zmiana roli agenta ustawia wymagany profil dostępu (8.3 planu). */
export function withRole(kind: NodeKind, role: AgentRole): NodeKind {
  return kind.type === "agent"
    ? { ...kind, role, access: requiredAccess(role) }
    : kind;
}
