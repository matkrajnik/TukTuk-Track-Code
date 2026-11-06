/**
 * Przebiegi Workflows (E5): w aplikacji komendy `workflow_*` z `run_commands.rs`,
 * w podglądzie symulator `preview/workflowRuns.ts` (jeden przykładowy przebieg;
 * start, decyzja i anulowanie są zablokowane).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop } from "./store";
import * as sim from "./preview/workflowRuns";
import type {
  DecideRunRequest,
  DecideRunResult,
  DiffArtifact,
  RunDetail,
  RunFilter,
  RunSummary,
  StartRunRequest,
  StartRunResult,
  StoredEvent,
} from "./api/workflowRuns";

export * from "./api/workflowRuns";

export const WORKFLOW_EVENT = "workflow-event";

/** Nowy identyfikator żądania: jeden na otwarcie okna albo panel zgody. */
export function newRequestId(): string {
  return crypto.randomUUID();
}

export async function startWorkflowRun(
  request: StartRunRequest,
): Promise<StartRunResult> {
  if (desktop) return invoke("workflow_start", { request });
  return sim.start(request);
}

export async function listWorkflowRuns(
  filter: RunFilter = {},
): Promise<RunSummary[]> {
  if (desktop) return invoke("workflow_runs", { filter });
  return sim.list(filter);
}

export async function getWorkflowRun(runId: number): Promise<RunDetail> {
  if (desktop) return invoke("workflow_run_get", { runId });
  return sim.get(runId);
}

export async function workflowRunEvents(
  runId: number,
  after: number,
  limit = 500,
): Promise<StoredEvent[]> {
  if (desktop) return invoke("workflow_run_events", { runId, after, limit });
  return sim.events(runId, after, limit);
}

export async function cancelWorkflowRun(runId: number): Promise<void> {
  if (desktop) return invoke("workflow_cancel", { runId });
  return sim.cancel(runId);
}

export async function decideWorkflowRun(
  command: DecideRunRequest,
): Promise<DecideRunResult> {
  if (desktop) return invoke("workflow_decide", { command });
  return sim.decide(command);
}

export async function readWorkflowArtifact(
  runId: number,
  artifactHash: string,
): Promise<DiffArtifact> {
  if (desktop) return invoke("workflow_artifact_read", { runId, artifactHash });
  return sim.artifact(runId, artifactHash);
}

/** Nasłuch `workflow-event` (w podglądzie brak strumienia: zwraca pustą funkcję). */
export async function listenWorkflowEvents(
  handler: (event: StoredEvent) => void,
): Promise<() => void> {
  if (desktop) return listen<StoredEvent>(WORKFLOW_EVENT, (e) => handler(e.payload));
  return () => {};
}
