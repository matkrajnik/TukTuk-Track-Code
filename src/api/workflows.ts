/**
 * Moduł Workflows: typy grafu i definicji (lustro `workflow_core::model`
 * i `workflows::store` w Rust) oraz reguły wspólne aplikacji i podglądu.
 * Plan: docs/workflows/PLAN_WDROZENIA.md; decyzje: docs/workflows/E0_ANALIZA.md.
 */

/** `workflow_core::SCHEMA_VERSION`. */
export const WORKFLOW_SCHEMA_VERSION = 1;
/** `workflow_core::model::MAX_REPAIR_ROUNDS`. */
export const MAX_REPAIR_ROUNDS = 3;
export const MIN_TIMEOUT_SECS = 60;
export const MAX_TIMEOUT_SECS = 4 * 60 * 60;
export const MAX_NODES = 200;
export const MAX_EDGES = 400;
export const MAX_INSTRUCTION_CHARS = 20_000;

export type Port = "out" | "true" | "false" | "approve" | "reject";
export type Provider = "claude" | "codex";
export type AgentRole = "analysis" | "implementation" | "fix" | "review";
export type AccessProfile = "readOnly" | "workspaceWrite";
export type CompareOp = "equals" | "notEquals" | "greaterThan" | "lessThan";
export type EndOutcome = "succeeded" | "rejected" | "failed";

export interface AgentConfig {
  role: AgentRole;
  provider: Provider;
  /** `null`: model wybierany w formularzu uruchomienia. */
  model: string | null;
  instruction: string;
  access: AccessProfile;
  timeoutSecs: number;
}

export interface FieldRef {
  nodeId: string;
  field: string;
}

export interface ConditionConfig {
  field: FieldRef;
  op: CompareOp;
  value: string | number | boolean | null;
  /** Tylko na warunku zamykającym blok poprawek. */
  repairLimit: number | null;
}

export type NodeKind =
  | { type: "trigger" }
  | ({ type: "agent" } & AgentConfig)
  | ({ type: "condition" } & ConditionConfig)
  | { type: "approval"; description: string }
  | { type: "end"; outcome: EndOutcome };

export type NodeType = NodeKind["type"];

export interface WorkflowNode {
  id: string;
  label: string;
  position: { x: number; y: number };
  kind: NodeKind;
}

export interface WorkflowEdge {
  id: string;
  source: string;
  sourcePort: Port;
  target: string;
}

export interface WorkflowGraph {
  schemaVersion: number;
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export type IssueCode =
  | "unsupportedSchema"
  | "tooLarge"
  | "emptyId"
  | "duplicateId"
  | "emptyLabel"
  | "missingTrigger"
  | "multipleTriggers"
  | "missingEnd"
  | "danglingEdge"
  | "invalidPort"
  | "edgeIntoTrigger"
  | "missingConnection"
  | "duplicateConnection"
  | "unreachable"
  | "noPathToEnd"
  | "invalidCycle"
  | "invalidRepairLimit"
  | "repairLimitOutsideLoop"
  | "emptyInstruction"
  | "instructionTooLong"
  | "emptyModel"
  | "invalidTimeout"
  | "accessNotAllowed"
  | "invalidFieldRef"
  | "unknownField"
  | "fieldNotAvailable"
  | "invalidOperator"
  | "typeMismatch";

/** `workflow_core::validation::Issue`. */
export interface WorkflowIssue {
  code: IssueCode;
  nodeId?: string;
  edgeId?: string;
  message: string;
}

/** `workflows::store::Summary`. */
export interface WorkflowSummary {
  id: number;
  name: string;
  description: string;
  draftRevision: number;
  createdAt: number;
  updatedAt: number;
  archivedAt: number | null;
  publishedVersion: number | null;
  publishedAt: number | null;
  unpublishedChanges: boolean;
}

export interface WorkflowVersion {
  /** Identyfikator wersji (argument `workflowPreflight`). */
  id: number;
  version: number;
  hash: string;
  createdAt: number;
}

/** `workflows::store::Detail`. */
export interface WorkflowDetail extends WorkflowSummary {
  draft: WorkflowGraph;
  issues: WorkflowIssue[];
  versions: WorkflowVersion[];
}

export interface WorkflowAccess {
  unlocked: boolean;
}

export type FieldType =
  | { type: "text" }
  | { type: "number" }
  | { type: "boolean" }
  | { type: "choice"; options: readonly string[] };

export const REVIEW_VERDICTS = ["approved", "changes_requested"] as const;

/** `AgentRole::output_fields`: pola wyniku dostępne dla warunku. */
export const outputFields: Record<
  AgentRole,
  readonly (readonly [string, FieldType])[]
> = {
  analysis: [
    ["summary", { type: "text" }],
    ["riskCount", { type: "number" }],
  ],
  implementation: [
    ["summary", { type: "text" }],
    ["changedFileCount", { type: "number" }],
  ],
  fix: [
    ["summary", { type: "text" }],
    ["changedFileCount", { type: "number" }],
  ],
  review: [
    ["verdict", { type: "choice", options: REVIEW_VERDICTS }],
    ["findingCount", { type: "number" }],
  ],
};

/** `AgentRole::required_access`: analiza i review tylko czytają. */
export function requiredAccess(role: AgentRole): AccessProfile {
  return role === "analysis" || role === "review"
    ? "readOnly"
    : "workspaceWrite";
}

export function outputPorts(kind: NodeKind): readonly Port[] {
  switch (kind.type) {
    case "trigger":
    case "agent":
      return ["out"];
    case "condition":
      return ["true", "false"];
    case "approval":
      return ["approve", "reject"];
    case "end":
      return [];
  }
}

export const nodeTypeNames: Record<NodeType, string> = {
  trigger: "Start",
  agent: "Agent",
  condition: "Warunek",
  approval: "Akceptacja",
  end: "Koniec",
};

export const portNames: Record<Port, string> = {
  out: "wyjście",
  true: "tak",
  false: "nie",
  approve: "akceptacja",
  reject: "odrzucenie",
};

/** `workflows::preflight::Enforcement`. */
export type WorkflowEnforcement =
  | "cliControl"
  | "cliRestricted"
  | "codexSandbox"
  | "none";

/** Kody błędów kroku (`workflow_core::FailureCode`, camelCase). */
export type WorkflowFailureCode =
  | "cliMissing"
  | "notLoggedIn"
  | "unsupportedVersion"
  | "spawnFailed"
  | "protocolError"
  | "invalidOutput"
  | "timeout"
  | "cancelled"
  | "permissionViolation"
  | "storageError"
  | "reviewLimitExceeded"
  | "interrupted"
  | "invalidDefinition"
  | "workspaceUnavailable";

/** `workflows::preflight::NodeReadiness`: gotowość jednego węzła agenta. */
export interface NodeReadiness {
  nodeId: string;
  provider: Provider;
  access: AccessProfile;
  cliFound: boolean;
  version: string | null;
  supported: boolean;
  enforcement: WorkflowEnforcement;
  warning: string | null;
  blocked: { code: WorkflowFailureCode; message: string } | null;
}
