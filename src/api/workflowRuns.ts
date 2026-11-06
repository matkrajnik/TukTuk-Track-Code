/**
 * Przebiegi modułu Workflows (E5): typy i czyste reguły wspólne dla aplikacji
 * i podglądu. Kształty odpowiadają `workflows::runs::query` i `workflow_core`
 * (camelCase); zmiana po stronie Rust wymaga zmiany tutaj i w fixture IPC.
 */
import type { WorkflowFailureCode, WorkflowGraph } from "./workflows";

export type RunStatus =
  | "queued"
  | "running"
  | "waitingForApproval"
  | "cancelling"
  | "cancelled"
  | "succeeded"
  | "rejected"
  | "failed"
  | "interrupted";

export type ExecutionStatus =
  | "pending"
  | "running"
  | "waiting"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted"
  | "rejected"
  | "skipped";

export type ApprovalStatus = "pending" | "approved" | "rejected" | "invalidated";

export interface RunSummary {
  id: number;
  workflowId: number;
  workflowName: string;
  version: number;
  status: RunStatus;
  taskId: number | null;
  /** Migawka z chwili startu: zostaje po usunięciu zadania. */
  taskTitle: string | null;
  taskExists: boolean;
  repo: string | null;
  createdAt: number;
  startedAt: number | null;
  finishedAt: number | null;
  errorCode: WorkflowFailureCode | null;
  waitingApproval: boolean;
}

export interface ExecutionView {
  id: string;
  nodeId: string;
  iteration: number;
  attempt: number;
  status: ExecutionStatus;
  startedAt: number | null;
  finishedAt: number | null;
  exitCode: number | null;
  errorCode: WorkflowFailureCode | null;
  error: string | null;
  output: Record<string, unknown> | null;
  providerSessionId: string | null;
}

export interface ApprovalView {
  id: string;
  executionId: string;
  status: ApprovalStatus;
  artifactHash: string;
  comment: string | null;
  decidedAt: number | null;
  artifactAvailable: boolean;
  /** Diff usunęła retencja (E6): widok mówi o tym zamiast błędu odczytu. */
  artifactPurged: boolean;
}

/** Stan odzyskania przerwanego przebiegu (`runs::query::RecoveryView`, E6). */
export interface RecoveryView {
  /** Katalog roboczy nadal zarezerwowany (blokada do potwierdzenia końca procesów). */
  workspaceReserved: boolean;
  /** Procesy z rejestru bez potwierdzonego końca. */
  processesUnconfirmed: number;
  /** PID tych procesów (użytkownik może zakończyć je ręcznie). */
  unconfirmedPids: number[];
  /** Liczba zapisanych prób odzyskania (zdarzenia `recovery`). */
  attempts: number;
}

/** Wynik `workflow_recover` (`recovery::RecoverOutcome`). */
export interface RecoverOutcome {
  processesConfirmed: boolean;
  workspaceReleased: boolean;
  alreadyRecovered: boolean;
  /** PID procesów bez potwierdzonego końca (rezerwacja zostaje). */
  unconfirmedPids: number[];
}

/** Jak `recovery::unconfirmed_note` w Rust: komunikat z PID procesów. */
export function unconfirmedNote(pids: readonly number[]): string {
  return `Nie potwierdzono końca procesu agenta (PID ${pids.join(", ")}); katalog roboczy pozostaje zablokowany. Zakończ proces ręcznie i spróbuj ponownie.`;
}

export interface RunDetail {
  summary: RunSummary;
  goal: string | null;
  graph: WorkflowGraph;
  definitionHash: string;
  /** Ostatnia sekwencja zdarzeń; podawana jako `expectedRevision` przy decyzji. */
  revision: number;
  workspace: string | null;
  baseCommit: string | null;
  error: string | null;
  executions: ExecutionView[];
  approvals: ApprovalView[];
  agentMs: number;
  waitingMs: number;
  /** Tokeny nie są zbierane: interfejs pokazuje „brak danych”. */
  tokens: number | null;
  /** Tylko dla przebiegu `interrupted`. */
  recovery: RecoveryView | null;
}

export interface StoredEvent {
  runId: number;
  sequence: number;
  executionId: string | null;
  type: string;
  timestamp: number;
  payload: Record<string, unknown>;
}

export interface RunFilter {
  status?: RunStatus[];
  workflowId?: number | null;
  taskId?: number | null;
  repo?: string | null;
  from?: number | null;
  to?: number | null;
  beforeId?: number | null;
  limit?: number | null;
}

export interface StartRunRequest {
  versionId: number;
  requestId: string;
  taskId?: number | null;
  repo: string;
  goal?: string | null;
}

export interface StartRunResult {
  runId: number;
  created: boolean;
}

export interface DecideRunRequest {
  runId: number;
  approvalId: string;
  decision: "approve" | "reject";
  requestId: string;
  expectedRevision: number;
  artifactHash: string;
  comment?: string | null;
}

export interface DecideRunResult {
  approvalStatus: ApprovalStatus;
  runStatus: RunStatus;
}

export interface DiffArtifact {
  /** Tekst do pokazania; słowa przypominające sekrety zamienione na `[ukryte]`. */
  text: string;
  /** Ile fragmentów zamaskowano (plik i skrót zostają surowe). */
  redacted: number;
  truncated: boolean;
  size: number;
  hash: string;
}

/** Typ zdarzenia odzyskania przerwanego przebiegu (`RunEvent::Reconciled`). */
export const RECOVERY_EVENT = "recovery";

/** Komunikat o stanie odzyskania przerwanego przebiegu dla monitora. */
export function recoveryMessage(view: RecoveryView): string {
  if (!view.workspaceReserved && view.processesUnconfirmed === 0)
    return "Przebieg odzyskany: katalog roboczy zwolniony. Przebieg nie jest wznawiany.";
  if (view.processesUnconfirmed > 0)
    return view.unconfirmedPids.length > 0
      ? unconfirmedNote(view.unconfirmedPids)
      : `Nie potwierdzono końca procesu agenta (${view.processesUnconfirmed}); katalog roboczy pozostaje zablokowany.`;
  return "Katalog roboczy jest zablokowany do świadomego odzyskania przebiegu.";
}

/** Stały prefiks błędu `workflow_decide`, gdy zmiany unieważniły zgodę. */
export const INVALIDATED_PREFIX = "Zgoda unieważniona: ";
/** Komunikat podglądu: start, decyzja i anulowanie nie uruchamiają agentów. */
export const PREVIEW_NO_AGENTS = "Podgląd nie uruchamia agentów.";
/** Najwięcej linii logu wyrenderowanych naraz (starsze za „Pokaż wcześniejsze”). */
export const MAX_LOG_LINES_RENDERED = 500;
/** Najdłuższa linia logu w znakach; reszta jest ucinana z wielokropkiem. */
export const MAX_LOG_LINE_CHARS = 2_000;
/** Limit komentarza do zgody (`MAX_APPROVAL_COMMENT_CHARS`). */
export const MAX_APPROVAL_COMMENT_CHARS = 2_000;

export const ACTIVE_STATUSES: RunStatus[] = [
  "queued",
  "running",
  "waitingForApproval",
  "cancelling",
];

export function isTerminal(status: RunStatus): boolean {
  return !ACTIVE_STATUSES.includes(status);
}

/** Etykieta i ikona: status nigdy nie jest przekazywany samym kolorem. */
export const RUN_STATUS_LABELS: Record<
  RunStatus,
  { label: string; icon: string }
> = {
  queued: { label: "W kolejce", icon: "…" },
  running: { label: "W toku", icon: "▶" },
  waitingForApproval: { label: "Czeka na zgodę", icon: "⏸" },
  cancelling: { label: "Zatrzymywanie", icon: "■" },
  cancelled: { label: "Zatrzymany", icon: "■" },
  succeeded: { label: "Zakończony", icon: "✓" },
  rejected: { label: "Odrzucony", icon: "✗" },
  failed: { label: "Błąd", icon: "!" },
  interrupted: { label: "Przerwany", icon: "↯" },
};

export const EXECUTION_STATUS_LABELS: Record<
  ExecutionStatus,
  { label: string; icon: string }
> = {
  pending: { label: "Oczekuje", icon: "…" },
  running: { label: "W toku", icon: "▶" },
  waiting: { label: "Czeka na zgodę", icon: "⏸" },
  completed: { label: "Gotowy", icon: "✓" },
  failed: { label: "Błąd", icon: "!" },
  cancelled: { label: "Zatrzymany", icon: "■" },
  interrupted: { label: "Przerwany", icon: "↯" },
  rejected: { label: "Odrzucony", icon: "✗" },
  skipped: { label: "Pominięty", icon: "–" },
};

/** Polskie komunikaty kodów błędów (lustro `FailureCode::message`). */
export const FAILURE_MESSAGES: Record<WorkflowFailureCode, string> = {
  cliMissing: "Nie znaleziono programu agenta (claude lub codex).",
  notLoggedIn: "Agent nie jest zalogowany.",
  unsupportedVersion: "Nieobsługiwana wersja programu agenta.",
  spawnFailed: "Nie udało się uruchomić programu agenta.",
  protocolError: "Agent zwrócił niezgodną z protokołem odpowiedź.",
  invalidOutput: "Wynik agenta nie pasuje do schematu roli.",
  timeout: "Krok agenta przekroczył limit czasu.",
  cancelled: "Krok został anulowany.",
  permissionViolation: "Agent naruszył przydzielone uprawnienia.",
  storageError: "Nie udało się zapisać stanu przebiegu.",
  reviewLimitExceeded:
    "Wyczerpano limit rund poprawek, a review nadal wskazuje problemy.",
  interrupted:
    "Przebieg został przerwany (zamknięcie aplikacji albo agent bez reakcji na anulowanie); koniec kroku nie jest potwierdzony.",
  invalidDefinition: "Definicja procesu jest niepoprawna.",
  workspaceUnavailable: "Nie udało się przygotować katalogu roboczego przebiegu.",
};

export interface MergedEvents {
  events: StoredEvent[];
  /** Ostatnia znana sekwencja przed luką (dociągnij zdarzenia po niej) albo `null`. */
  gapAfter: number | null;
}

/**
 * Scala znane zdarzenia z nowymi: sortuje po sekwencji, usuwa duplikaty i
 * wykrywa lukę. Przy luce zwraca tylko ciągły prefiks, a `gapAfter` mówi,
 * od której sekwencji dociągnąć brakujące.
 */
export function mergeEvents(
  known: StoredEvent[],
  incoming: StoredEvent[],
): MergedEvents {
  const bySequence = new Map<number, StoredEvent>();
  for (const event of [...known, ...incoming]) {
    if (!bySequence.has(event.sequence)) bySequence.set(event.sequence, event);
  }
  const sorted = [...bySequence.values()].sort(
    (a, b) => a.sequence - b.sequence,
  );
  const events: StoredEvent[] = [];
  let expected = 1;
  let gapAfter: number | null = null;
  for (const event of sorted) {
    if (event.sequence !== expected) {
      gapAfter = expected - 1;
      break;
    }
    events.push(event);
    expected += 1;
  }
  return { events, gapAfter };
}

/** Czas trwania w czytelnej postaci (`1 min 5 s`); `null` to „brak danych”. */
export function formatDuration(ms: number | null | undefined): string {
  if (ms == null) return "brak danych";
  const total = Math.max(0, Math.round(ms / 1000));
  const minutes = Math.floor(total / 60);
  const seconds = total % 60;
  if (minutes === 0) return `${seconds} s`;
  return `${minutes} min ${seconds} s`;
}
