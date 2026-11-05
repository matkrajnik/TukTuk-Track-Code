/**
 * Odczyt grafu z niezaufanego JSON jak deserializacja serde w Rust
 * (`workflow_core::model`): sprawdza kształt i wartości wyliczeń, uzupełnia
 * domyślne pola (`model`, `repairLimit` → null, `position` → 0,0), odrzuca
 * nieznane pola i zwraca obiekt w kolejności pól z typów Rust. Dzięki temu
 * skrót JSON w podglądzie nie zależy od kolejności kluczy wejścia.
 */
import type {
  AccessProfile,
  AgentRole,
  CompareOp,
  EndOutcome,
  NodeKind,
  Port,
  Provider,
  WorkflowEdge,
  WorkflowGraph,
  WorkflowNode,
} from "./workflows";

const U32_MAX = 4_294_967_295;

const PORTS: readonly Port[] = ["out", "true", "false", "approve", "reject"];
const PROVIDERS: readonly Provider[] = ["claude", "codex"];
const ROLES: readonly AgentRole[] = ["analysis", "implementation", "fix", "review"];
const ACCESS: readonly AccessProfile[] = ["readOnly", "workspaceWrite"];
const OPS: readonly CompareOp[] = ["equals", "notEquals", "greaterThan", "lessThan"];
const OUTCOMES: readonly EndOutcome[] = ["succeeded", "rejected", "failed"];
const NODE_TYPES = ["trigger", "agent", "condition", "approval", "end"] as const;

type Json = Record<string, unknown>;

function fail(path: string, expected: string): never {
  throw new Error(`pole ${path}: oczekiwano ${expected}`);
}

function object(value: unknown, path: string): Json {
  if (!value || typeof value !== "object" || Array.isArray(value))
    fail(path, "obiektu");
  return value as Json;
}

function text(value: unknown, path: string): string {
  if (typeof value !== "string") fail(path, "tekstu");
  return value;
}

function u32(value: unknown, path: string): number {
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0 ||
    value > U32_MAX
  )
    fail(path, "liczby całkowitej od 0");
  return value;
}

function finite(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value))
    fail(path, "liczby");
  return value;
}

function oneOf<T extends string>(
  value: unknown,
  options: readonly T[],
  path: string,
): T {
  if (typeof value !== "string" || !options.includes(value as T))
    fail(path, options.join(" | "));
  return value as T;
}

function array(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) fail(path, "listy");
  return value;
}

function optional<T>(
  value: unknown,
  parse: (v: unknown) => T,
): T | null {
  return value === undefined || value === null ? null : parse(value);
}

function parseKind(value: unknown, path: string): NodeKind {
  const kind = object(value, path);
  const type = oneOf(kind.type, NODE_TYPES, `${path}.type`);
  switch (type) {
    case "trigger":
      return { type };
    case "agent":
      return {
        type,
        role: oneOf(kind.role, ROLES, `${path}.role`),
        provider: oneOf(kind.provider, PROVIDERS, `${path}.provider`),
        model: optional(kind.model, (v) => text(v, `${path}.model`)),
        instruction: text(kind.instruction, `${path}.instruction`),
        access: oneOf(kind.access, ACCESS, `${path}.access`),
        timeoutSecs: u32(kind.timeoutSecs, `${path}.timeoutSecs`),
      };
    case "condition": {
      const field = object(kind.field, `${path}.field`);
      if (!("value" in kind)) fail(`${path}.value`, "wartości");
      return {
        type,
        field: {
          nodeId: text(field.nodeId, `${path}.field.nodeId`),
          field: text(field.field, `${path}.field.field`),
        },
        op: oneOf(kind.op, OPS, `${path}.op`),
        // Jak `serde_json::Value`: dowolny JSON; zły typ zgłasza walidator.
        value: kind.value as string | number | boolean | null,
        repairLimit: optional(kind.repairLimit, (v) =>
          u32(v, `${path}.repairLimit`),
        ),
      };
    }
    case "approval":
      return {
        type,
        description: text(kind.description, `${path}.description`),
      };
    case "end":
      return {
        type,
        outcome: oneOf(kind.outcome, OUTCOMES, `${path}.outcome`),
      };
  }
}

function parseNode(value: unknown, path: string): WorkflowNode {
  const node = object(value, path);
  const position =
    node.position === undefined
      ? { x: 0, y: 0 }
      : object(node.position, `${path}.position`);
  return {
    id: text(node.id, `${path}.id`),
    label: text(node.label, `${path}.label`),
    position: {
      x: finite(position.x, `${path}.position.x`),
      y: finite(position.y, `${path}.position.y`),
    },
    kind: parseKind(node.kind, `${path}.kind`),
  };
}

function parseEdge(value: unknown, path: string): WorkflowEdge {
  const edge = object(value, path);
  return {
    id: text(edge.id, `${path}.id`),
    source: text(edge.source, `${path}.source`),
    sourcePort: oneOf(edge.sourcePort, PORTS, `${path}.sourcePort`),
    target: text(edge.target, `${path}.target`),
  };
}

/** Graf w kanonicznej postaci albo wyjątek z opisem pierwszego złego pola. */
export function parseWorkflowGraph(value: unknown): WorkflowGraph {
  const graph = object(value, "graph");
  return {
    schemaVersion: u32(graph.schemaVersion, "graph.schemaVersion"),
    nodes: array(graph.nodes, "graph.nodes").map((n, i) =>
      parseNode(n, `graph.nodes[${i}]`),
    ),
    edges: array(graph.edges, "graph.edges").map((e, i) =>
      parseEdge(e, `graph.edges[${i}]`),
    ),
  };
}
