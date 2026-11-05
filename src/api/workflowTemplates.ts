/**
 * Szablony procesów: przeniesienie 1:1 `workflow_core::template` (Rust).
 * Aplikacja tworzy procesy z szablonu w Rust; ta kopia służy podglądowi.
 */
import {
  MAX_REPAIR_ROUNDS,
  WORKFLOW_SCHEMA_VERSION,
  requiredAccess,
  type AgentRole,
  type NodeKind,
  type Port,
  type Provider,
  type WorkflowEdge,
  type WorkflowGraph,
  type WorkflowNode,
} from "./workflows";

/** Domyślny limit kroku agenta w szablonie: 30 minut. */
const TEMPLATE_TIMEOUT_SECS = 30 * 60;

export const TEMPLATE_NAME = "Analiza → implementacja → review";

const node = (
  id: string,
  label: string,
  x: number,
  y: number,
  kind: NodeKind,
): WorkflowNode => ({ id, label, position: { x, y }, kind });

const agent = (
  role: AgentRole,
  provider: Provider,
  instruction: string,
): NodeKind => ({
  type: "agent",
  role,
  provider,
  model: null,
  instruction,
  access: requiredAccess(role),
  timeoutSecs: TEMPLATE_TIMEOUT_SECS,
});

const edge = (source: string, sourcePort: Port, target: string): WorkflowEdge => ({
  id: `${source}-${target}`,
  source,
  sourcePort,
  target,
});

/** Nowy pusty proces: Start połączony z Końcem. */
export function blankWorkflow(): WorkflowGraph {
  return {
    schemaVersion: WORKFLOW_SCHEMA_VERSION,
    nodes: [
      node("start", "Start", 0, 0, { type: "trigger" }),
      node("done", "Koniec", 260, 0, { type: "end", outcome: "succeeded" }),
    ],
    edges: [edge("start", "out", "done")],
  };
}

/** Claude analizuje, Codex implementuje, Claude robi review; najwyżej trzy
 * rundy poprawek, potem decyzja człowieka. */
export function analysisImplementationReview(): WorkflowGraph {
  return {
    schemaVersion: WORKFLOW_SCHEMA_VERSION,
    nodes: [
      node("start", "Start", 0, 0, { type: "trigger" }),
      node(
        "analysis",
        "Analiza",
        220,
        0,
        agent(
          "analysis",
          "claude",
          "Przeanalizuj repozytorium pod kątem celu zadania. Zapisz plan zmian, kryteria akceptacji i ryzyka. Nie zmieniaj plików.",
        ),
      ),
      node(
        "implementation",
        "Implementacja",
        440,
        0,
        agent(
          "implementation",
          "codex",
          "Zrealizuj plan z analizy w tym katalogu roboczym. Uruchom testy, które dotyczą zmiany, i opisz ich wynik.",
        ),
      ),
      node(
        "review",
        "Review",
        660,
        0,
        agent(
          "review",
          "claude",
          "Zrób review zmian względem planu i kryteriów akceptacji. Wypisz problemy z plikiem, miejscem i wagą. Nie zmieniaj plików.",
        ),
      ),
      node("decision", "Review zaakceptowane?", 880, 0, {
        type: "condition",
        field: { nodeId: "review", field: "verdict" },
        op: "equals",
        value: "approved",
        repairLimit: MAX_REPAIR_ROUNDS,
      }),
      node(
        "fix",
        "Poprawki",
        660,
        180,
        agent(
          "fix",
          "codex",
          "Popraw problemy wskazane w ostatnim review. Nie zmieniaj zakresu zadania.",
        ),
      ),
      node("approval", "Decyzja użytkownika", 1100, 0, {
        type: "approval",
        description:
          "Przejrzyj diff i wynik review, potem zaakceptuj albo odrzuć zmiany.",
      }),
      node("done", "Zaakceptowano", 1320, -80, {
        type: "end",
        outcome: "succeeded",
      }),
      node("rejected", "Odrzucono", 1320, 80, {
        type: "end",
        outcome: "rejected",
      }),
    ],
    edges: [
      edge("start", "out", "analysis"),
      edge("analysis", "out", "implementation"),
      edge("implementation", "out", "review"),
      edge("review", "out", "decision"),
      edge("decision", "true", "approval"),
      edge("decision", "false", "fix"),
      edge("fix", "out", "review"),
      edge("approval", "approve", "done"),
      edge("approval", "reject", "rejected"),
    ],
  };
}
