/**
 * Podgląd przebiegów Workflows: jeden deterministyczny, przykładowy przebieg
 * szablonu „Analiza → implementacja → review” czekający na zgodę (repozytorium
 * z kalkulator.js) oraz drugi, przerwany przebieg z zarezerwowanym katalogiem
 * roboczym do pokazania odzyskania (E6). Wyniki mają kształt
 * `workflow_core::samples`. Start, decyzja, wznowienie i anulowanie rzucają komunikat, bo
 * podgląd nie uruchamia agentów; odzyskanie tylko zwalnia przykładową rezerwację.
 */
import {
  PREVIEW_NO_AGENTS,
  type DecideRunRequest,
  type DecideRunResult,
  type DiffArtifact,
  type RecoverOutcome,
  type RunDetail,
  type RunFilter,
  type RunStatus,
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
/** Przykładowy przebieg przerwany (awaria aplikacji w trakcie implementacji). */
const INTERRUPTED_ID = 2;
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

const interruptedSummary: RunSummary = {
  ...summary,
  id: INTERRUPTED_ID,
  workflowName: "Analiza → implementacja → review (przerwany)",
  status: "interrupted",
  createdAt: T0 - 3_600_000,
  startedAt: T0 - 3_599_000,
  finishedAt: T0 - 3_580_000,
  errorCode: "interrupted",
  waitingApproval: false,
};

/** Stan odzyskania przykładu (w pamięci strony; przeładowanie zaczyna od nowa). */
let interruptedRecovered = false;

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
  // Komunikat runnera jak w hoście (`RunEvent::Message`): tekst w `payload.payload`.
  const message = (sequence: number, at: number, text: string, executionId: string) =>
    make(sequence, "message", at, { executionId, payload: { text } }, executionId);
  return [
    make(1, "runStateChanged", 0, { from: "queued", to: "queued", code: null }),
    make(2, "runStateChanged", 1_000, { from: "queued", to: "running", code: null }),
    message(3, 2_000, "Analizuję repozytorium.", "e-1"),
    make(4, "nodeStateChanged", 5_000, { status: "completed" }, "e-1"),
    message(5, 12_000, "Poprawiam operator odejmowania.", "e-2"),
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

/** Statusy blokujące wyłączenie modułu, jak `backup::ACTIVE_STATUSES` w Rust. */
const ACTIVE_STATUSES: readonly RunSummary["status"][] = ["queued", "running", "cancelling"];

/**
 * Liczba przebiegów w toku w symulatorze (jak `backup::active_work`): czekający
 * na zgodę nie blokuje. Bez sprawdzania klucza, bo woła ją wyłączenie modułu.
 */
export function activeRunCount(): number {
  return [interruptedSummary, summary].filter((s) => ACTIVE_STATUSES.includes(s.status))
    .length;
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

/** Wznowienie uruchamia agentów, więc podgląd je odrzuca (jak start). */
export function resume(_runId: number, _requestId: string): Promise<RunStatus> {
  return Promise.reject(PREVIEW_NO_AGENTS);
}

/**
 * Jak `recovery::recover`: tylko przerwany przebieg. Przykład nie ma procesów,
 * więc odzyskanie zwalnia jego rezerwację i dopisuje zdarzenie `recovery`;
 * kolejne wywołanie nic nie zmienia.
 */
export async function recover(runId: number): Promise<RecoverOutcome> {
  if (runId !== RUN_ID && runId !== INTERRUPTED_ID) throw "Nie znaleziono przebiegu.";
  if (runId !== INTERRUPTED_ID) throw "Odzyskać można tylko przerwany przebieg.";
  const alreadyRecovered = interruptedRecovered;
  interruptedRecovered = true;
  return {
    processesConfirmed: true,
    workspaceReleased: true,
    alreadyRecovered,
    unconfirmedPids: [],
  };
}

function interruptedEvents(): StoredEvent[] {
  const make = (
    sequence: number,
    type: string,
    payload: Record<string, unknown>,
    executionId: string | null = null,
  ): StoredEvent => ({
    runId: INTERRUPTED_ID,
    sequence,
    executionId,
    type,
    timestamp: interruptedSummary.createdAt + sequence * 1_000,
    payload: { type, ...payload },
  });
  const list = [
    make(1, "runStateChanged", { from: "queued", to: "queued", code: null }),
    make(2, "runStateChanged", { from: "queued", to: "running", code: null }),
    make(3, "nodeStateChanged", { status: "completed" }, "e-1"),
    make(4, "nodeStateChanged", { status: "interrupted" }, "e-2"),
    make(5, "runStateChanged", { from: "running", to: "interrupted", code: "interrupted" }),
  ];
  if (interruptedRecovered)
    list.push(
      make(6, "recovery", { processesConfirmed: true, workspaceReleased: true }),
    );
  return list;
}

function interruptedDetail(): RunDetail {
  return {
    summary: { ...interruptedSummary },
    goal: "Napraw odejmowanie w kalkulatorze",
    graph: analysisImplementationReview(),
    definitionHash: "przyklad",
    revision: interruptedEvents().length,
    workspace: "C:\\Dane\\workflows\\worktrees\\2",
    baseCommit: "0123456789abcdef0123456789abcdef01234567",
    error: "Aplikacja zakończyła się w trakcie kroku; krok nie jest wznawiany.",
    executions: executions.slice(0, 2).map((e, i) =>
      i === 1
        ? { ...e, status: "interrupted", finishedAt: null, exitCode: null, output: null }
        : { ...e },
    ),
    approvals: [],
    agentMs: 9_000,
    waitingMs: 0,
    tokens: null,
    recovery: {
      workspaceReserved: !interruptedRecovered,
      processesUnconfirmed: 0,
      unconfirmedPids: [],
      attempts: interruptedRecovered ? 1 : 0,
    },
  };
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
  const match = (s: RunSummary) =>
    (statuses.length === 0 || statuses.includes(s.status)) &&
    (filter.beforeId == null || s.id < filter.beforeId) &&
    (filter.workflowId == null || filter.workflowId === s.workflowId) &&
    (repo == null || repo === repoKey(s.repo ?? ""));
  // Od najnowszego (jak `runs::query::list`): przerwany przykład ma większe id.
  return [interruptedSummary, summary].filter(match).map((s) => ({ ...s }));
}

export async function get(runId: number): Promise<RunDetail> {
  if (runId === INTERRUPTED_ID) return interruptedDetail();
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
        artifactPurged: false,
      },
    ],
    agentMs: 15_000,
    waitingMs: 0,
    tokens: null,
    recovery: null,
  };
}

export async function events(
  runId: number,
  after: number,
  limit: number,
): Promise<StoredEvent[]> {
  if (runId !== RUN_ID && runId !== INTERRUPTED_ID) return [];
  return (runId === RUN_ID ? eventList() : interruptedEvents())
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
