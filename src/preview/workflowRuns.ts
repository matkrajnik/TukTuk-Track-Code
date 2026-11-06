/**
 * Podgląd przebiegów Workflows: jeden deterministyczny, przykładowy przebieg
 * szablonu „Analiza → implementacja → review” czekający na zgodę (repozytorium
 * z kalkulator.js). Wyniki mają kształt `workflow_core::samples`. Start,
 * decyzja i anulowanie rzucają komunikat, bo podgląd nie uruchamia agentów.
 */
import {
  PREVIEW_NO_AGENTS,
  type DecideRunRequest,
  type DecideRunResult,
  type DiffArtifact,
  type RunDetail,
  type RunFilter,
  type RunSummary,
  type StartRunRequest,
  type StartRunResult,
  type StoredEvent,
} from "../api/workflowRuns";
import { analysisImplementationReview } from "../api/workflowTemplates";
import { access } from "./workflows";

const BACKSLASH = String.fromCharCode(92);
const TRAILING_SEPARATORS = /[\\/]+$/;

const RUN_ID = 1;
const T0 = Date.UTC(2026, 10, 5, 9, 0, 0);
const HASH = "a".repeat(63) + "1";
const DIFF = [
  "diff --git a/src/kalkulator.js b/src/kalkulator.js",
  "--- a/src/kalkulator.js",
  "+++ b/src/kalkulator.js",
  "@@ -1,3 +1,3 @@",
  " function subtract(a, b) {",
  "-  return a + b;",
  "+  return a - b;",
  " }",
  "",
].join("\n");

const summary: RunSummary = {
  id: RUN_ID,
  workflowId: 1,
  workflowName: "Analiza → implementacja → review (przykład)",
  version: 1,
  status: "waitingForApproval",
  taskId: null,
  taskTitle: "Naprawa odejmowania w kalkulatorze",
  taskExists: false,
  repo: "C:\\Projekty\\kalkulator-probny",
  createdAt: T0,
  startedAt: T0 + 1_000,
  finishedAt: null,
  errorCode: null,
  waitingApproval: true,
};

const executions: RunDetail["executions"] = [
  ["analysis", "completed", 5_000, {
    summary: "Odejmowanie zwraca sumę.",
    plan: ["Zmienić operator w subtract"],
    acceptanceCriteria: ["subtract(5, 3) === 2"],
    risks: [],
  }],
  ["implementation", "completed", 20_000, {
    summary: "Poprawiono operator.",
    changedFiles: ["src/kalkulator.js"],
    checks: [{ name: "node test", passed: true, logRef: null }],
  }],
  ["review", "completed", 30_000, {
    verdict: "approved",
    findings: [],
    checkedArtifactHash: HASH,
  }],
  ["approval", "waiting", 31_000, null],
].map(([nodeId, status, at, output], i) => ({
  id: `e-${i + 1}`,
  nodeId: nodeId as string,
  iteration: 1,
  attempt: 1,
  status: status as RunDetail["executions"][number]["status"],
  startedAt: T0 + (at as number) - 4_000,
  finishedAt: status === "completed" ? T0 + (at as number) : null,
  exitCode: status === "completed" ? 0 : null,
  errorCode: null,
  error: null,
  output: output as Record<string, unknown> | null,
  providerSessionId: null,
}));

function eventList(): StoredEvent[] {
  const make = (
    sequence: number,
    type: string,
    at: number,
    payload: Record<string, unknown>,
    executionId: string | null = null,
  ): StoredEvent => ({
    runId: RUN_ID,
    sequence,
    executionId,
    type,
    timestamp: T0 + at,
    payload: { type, ...payload },
  });
  return [
    make(1, "runStateChanged", 0, { from: "queued", to: "queued", code: null }),
    make(2, "runStateChanged", 1_000, { from: "queued", to: "running", code: null }),
    make(3, "message", 2_000, { text: "Analizuję repozytorium." }, "e-1"),
    make(4, "nodeStateChanged", 5_000, { status: "completed" }, "e-1"),
    make(5, "message", 12_000, { text: "Poprawiam operator odejmowania." }, "e-2"),
    make(6, "nodeStateChanged", 20_000, { status: "completed" }, "e-2"),
    make(7, "nodeStateChanged", 30_000, { status: "completed" }, "e-3"),
    make(8, "approvalRequested", 31_000, { approvalId: "ap-1", artifactHash: HASH }, "e-4"),
    make(9, "runStateChanged", 31_000, {
      from: "running",
      to: "waitingForApproval",
      code: null,
    }),
  ];
}

export function start(_request: StartRunRequest): Promise<StartRunResult> {
  return Promise.reject(PREVIEW_NO_AGENTS);
}

export function decide(_command: DecideRunRequest): Promise<DecideRunResult> {
  return Promise.reject(PREVIEW_NO_AGENTS);
}

export function cancel(_runId: number): Promise<void> {
  return Promise.reject(PREVIEW_NO_AGENTS);
}

/** Klucz repozytorium jak `runs::query::repo_key` (bez kanonikalizacji dysku). */
function repoKey(path: string): string {
  return path.trim().replaceAll("/", BACKSLASH).replace(TRAILING_SEPARATORS, "").toLowerCase();
}

export async function list(filter: RunFilter): Promise<RunSummary[]> {
  // Przykładowy przebieg należy do demonstracji odblokowanego modułu: w
  // zablokowanym podglądzie historii nie ma (prawdziwy host pokazuje ją bez klucza).
  if (!(await access()).unlocked) return [];
  const statuses = filter.status ?? [];
  const repo = filter.repo?.trim() ? repoKey(filter.repo) : null;
  const match =
    (statuses.length === 0 || statuses.includes(summary.status)) &&
    (filter.beforeId == null || summary.id < filter.beforeId) &&
    (filter.workflowId == null || filter.workflowId === summary.workflowId) &&
    (repo == null || repo === repoKey(summary.repo ?? ""));
  return match ? [{ ...summary }] : [];
}

export async function get(runId: number): Promise<RunDetail> {
  if (runId !== RUN_ID) throw "Nie znaleziono przebiegu.";
  return {
    summary: { ...summary },
    goal: "Napraw odejmowanie w kalkulatorze",
    graph: analysisImplementationReview(),
    definitionHash: "przyklad",
    revision: eventList().length,
    workspace: "C:\\Dane\\workflows\\worktrees\\1",
    baseCommit: "0123456789abcdef0123456789abcdef01234567",
    error: null,
    executions: executions.map((e) => ({ ...e })),
    approvals: [
      {
        id: "ap-1",
        executionId: "e-4",
        status: "pending",
        artifactHash: HASH,
        comment: null,
        decidedAt: null,
        artifactAvailable: true,
      },
    ],
    agentMs: 15_000,
    waitingMs: 0,
    tokens: null,
  };
}

export async function events(
  runId: number,
  after: number,
  limit: number,
): Promise<StoredEvent[]> {
  if (runId !== RUN_ID) return [];
  return eventList()
    .filter((e) => e.sequence > after)
    .slice(0, Math.min(Math.max(limit, 1), 500));
}

export async function artifact(
  runId: number,
  hash: string,
): Promise<DiffArtifact> {
  if (runId !== RUN_ID || hash !== HASH) {
    throw "Nie znaleziono artefaktu tego przebiegu.";
  }
  return { text: DIFF, redacted: 0, truncated: false, size: DIFF.length, hash: HASH };
}
