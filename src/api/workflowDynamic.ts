/**
 * Tryb dynamiczny (E8): 1:1 z `workflow_core::dynamic` (Rust). Te same id, etykiety,
 * instrukcje i kształt grafu; zmiana tu wymaga zmiany w `dynamic.rs` i odwrotnie.
 *
 * Graf dynamiczny powstaje z `DynamicSpec` (`buildDynamic`), a walidator porównuje
 * kształt grafu z `buildDynamic(specOf(graph))` (pozycje na canvasie są ignorowane).
 */
import {
  DEFAULT_SUBAGENTS_HINT,
  DYNAMIC_TIMEOUT_SECS,
  MAX_REPAIR_ROUNDS,
  WORKFLOW_SCHEMA_VERSION,
  type AccessProfile,
  type NodeKind,
  type Port,
  type WorkflowEdge,
  type WorkflowGraph,
  type WorkflowNode,
} from "./workflows";

export const DYNAMIC_TEMPLATE_NAME = "Dynamiczny (Claude)";

export const ORCHESTRATOR_ID = "orchestrator";
export const REVIEW_ID = "review";
const FIX_ID = "fix";
const DECISION_ID = "decision";
const APPROVAL_ID = "approval";

/** Limit czasu stałych kroków review i poprawek: 30 minut. */
export const DYNAMIC_STEP_TIMEOUT_SECS = 30 * 60;

export const DYNAMIC_DEFAULT_INSTRUCTION =
  "Zrealizuj cel przebiegu. Rozbij pracę na niezależne części i zleć je subagentom narzędziem Agent, a ich wyniki zbierz i sprawdź. Opisz, co zostało zrobione.";
export const DYNAMIC_REVIEW_INSTRUCTION =
  "Zrób review zmian względem celu przebiegu i wyniku orkiestratora. Wypisz problemy z plikiem, miejscem i wagą. Nie zmieniaj plików.";
export const DYNAMIC_FIX_INSTRUCTION =
  "Popraw problemy wskazane w ostatnim review. Nie zmieniaj zakresu zadania.";
const APPROVAL_WITH_REVIEW =
  "Przejrzyj diff i wynik review, potem zaakceptuj albo odrzuć zmiany.";
const APPROVAL_WITHOUT_REVIEW =
  "Przejrzyj wynik orkiestratora i zmiany w katalogu roboczym, potem zaakceptuj albo odrzuć.";

export interface DynamicSpec {
  instruction: string;
  model: string | null;
  timeoutSecs: number;
  access: AccessProfile;
  /** Wskazówka w prompcie (1–8), nie limit egzekwowany przez CLI. */
  maxSubagents: number;
  /** Review i poprawki po orkiestratorze (wymaga zapisu). */
  review: boolean;
}

export function defaultDynamicSpec(): DynamicSpec {
  return {
    instruction: DYNAMIC_DEFAULT_INSTRUCTION,
    model: null,
    timeoutSecs: DYNAMIC_TIMEOUT_SECS,
    access: "workspaceWrite",
    maxSubagents: DEFAULT_SUBAGENTS_HINT,
    review: true,
  };
}

const node = (
  id: string,
  label: string,
  x: number,
  kind: NodeKind,
  y = 0,
): WorkflowNode => ({ id, label, position: { x, y }, kind });

const edge = (source: string, sourcePort: Port, target: string): WorkflowEdge => ({
  id: `${source}-${target}`,
  source,
  sourcePort,
  target,
});

const claudeAgent = (
  role: "orchestrator" | "review" | "fix",
  model: string | null,
  instruction: string,
  access: AccessProfile,
  timeoutSecs: number,
  maxSubagents?: number,
): NodeKind => ({
  type: "agent",
  role,
  provider: "claude",
  model,
  instruction,
  access,
  timeoutSecs,
  ...(maxSubagents === undefined ? {} : { maxSubagents }),
});

/** Kanoniczny graf procesu dynamicznego dla parametrów. */
export function buildDynamic(spec: DynamicSpec): WorkflowGraph {
  const nodes: WorkflowNode[] = [
    node("start", "Start", 0, { type: "trigger" }),
    node(
      ORCHESTRATOR_ID,
      "Orkiestrator",
      220,
      claudeAgent(
        "orchestrator",
        spec.model,
        spec.instruction,
        spec.access,
        spec.timeoutSecs,
        spec.maxSubagents,
      ),
    ),
  ];
  const edges: WorkflowEdge[] = [edge("start", "out", ORCHESTRATOR_ID)];
  let approvalX: number;
  if (spec.review) {
    nodes.push(
      node(
        REVIEW_ID,
        "Review",
        440,
        claudeAgent(
          "review",
          spec.model,
          DYNAMIC_REVIEW_INSTRUCTION,
          "readOnly",
          DYNAMIC_STEP_TIMEOUT_SECS,
        ),
      ),
      node(DECISION_ID, "Review zaakceptowane?", 660, {
        type: "condition",
        field: { nodeId: REVIEW_ID, field: "verdict" },
        op: "equals",
        value: "approved",
        repairLimit: MAX_REPAIR_ROUNDS,
      }),
      node(
        FIX_ID,
        "Poprawki",
        440,
        claudeAgent(
          "fix",
          spec.model,
          DYNAMIC_FIX_INSTRUCTION,
          "workspaceWrite",
          DYNAMIC_STEP_TIMEOUT_SECS,
        ),
        180,
      ),
    );
    edges.push(
      edge(ORCHESTRATOR_ID, "out", REVIEW_ID),
      edge(REVIEW_ID, "out", DECISION_ID),
      edge(DECISION_ID, "true", APPROVAL_ID),
      edge(DECISION_ID, "false", FIX_ID),
      edge(FIX_ID, "out", REVIEW_ID),
    );
    approvalX = 880;
  } else {
    edges.push(edge(ORCHESTRATOR_ID, "out", APPROVAL_ID));
    approvalX = 440;
  }
  nodes.push(
    node(APPROVAL_ID, "Decyzja użytkownika", approvalX, {
      type: "approval",
      description: spec.review ? APPROVAL_WITH_REVIEW : APPROVAL_WITHOUT_REVIEW,
    }),
    node("done", "Zaakceptowano", approvalX + 220, { type: "end", outcome: "succeeded" }, -80),
    node("rejected", "Odrzucono", approvalX + 220, { type: "end", outcome: "rejected" }, 80),
  );
  edges.push(
    edge(APPROVAL_ID, "approve", "done"),
    edge(APPROVAL_ID, "reject", "rejected"),
  );
  return { schemaVersion: WORKFLOW_SCHEMA_VERSION, nodes, edges };
}

/** Czy graf zawiera krok orkiestratora (tryb dynamiczny). */
export function isDynamic(graph: WorkflowGraph): boolean {
  return graph.nodes.some(
    (n) => n.kind.type === "agent" && n.kind.role === "orchestrator",
  );
}

/** Parametry odczytane z grafu; `null`, gdy nie ma kanonicznego orkiestratora. */
export function specOf(graph: WorkflowGraph): DynamicSpec | null {
  const found = graph.nodes.find((n) => n.id === ORCHESTRATOR_ID);
  const kind = found?.kind;
  if (!kind || kind.type !== "agent" || kind.role !== "orchestrator") return null;
  return {
    instruction: kind.instruction,
    model: kind.model,
    timeoutSecs: kind.timeoutSecs,
    access: kind.access,
    maxSubagents: kind.maxSubagents ?? DEFAULT_SUBAGENTS_HINT,
    review: graph.nodes.some((n) => n.id === REVIEW_ID),
  };
}

/**
 * Stabilny JSON bez pozycji; `undefined` i brak klucza to to samo, a `model: null`
 * też oznacza brak (jak `Option::None` po stronie Rust).
 */
function shapeKey(graph: WorkflowGraph): string {
  const canon = (value: unknown): unknown => {
    if (Array.isArray(value)) return value.map(canon);
    if (value && typeof value === "object") {
      return Object.fromEntries(
        Object.entries(value as Record<string, unknown>)
          .filter(([key, v]) => v !== undefined && key !== "position")
          .filter(([key, v]) => !(key === "model" && v === null))
          .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
          .map(([key, v]) => [key, canon(v)]),
      );
    }
    return value;
  };
  return JSON.stringify(canon(graph));
}

export function sameShape(a: WorkflowGraph, b: WorkflowGraph): boolean {
  return shapeKey(a) === shapeKey(b);
}

/**
 * Czy graf jest dokładnie tym, co zbudowałby `buildDynamic` z jego parametrów.
 * Dostawca orkiestratora i brak `maxSubagents` są normalizowane (dostawcę
 * sprawdza osobna reguła, a brak pola oznacza wartość domyślną).
 */
export function hasCanonicalShape(graph: WorkflowGraph): boolean {
  const spec = specOf(graph);
  if (!spec) return false;
  const normalized: WorkflowGraph = {
    ...graph,
    nodes: graph.nodes.map((n) =>
      n.kind.type === "agent" && n.kind.role === "orchestrator"
        ? {
            ...n,
            kind: { ...n.kind, provider: "claude", maxSubagents: spec.maxSubagents },
          }
        : n,
    ),
  };
  return sameShape(normalized, buildDynamic(spec));
}
