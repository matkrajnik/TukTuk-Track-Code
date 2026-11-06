/**
 * Moduł Workflows: definicje procesów agentów (etap E1, docs/workflows/).
 * W aplikacji komendy `workflow_*` w Rust (`workflows/mod.rs`), w podglądzie
 * symulator `preview/workflows.ts`. Backend sam sprawdza odblokowanie modułu.
 */
import { invoke } from "@tauri-apps/api/core";
import { desktop } from "./store";
import * as sim from "./preview/workflows";
import type {
  NodeReadiness,
  WorkflowAccess,
  WorkflowDetail,
  WorkflowGraph,
  WorkflowIssue,
  WorkflowSummary,
} from "./api/workflows";

export * from "./api/workflows";
export { validateWorkflow } from "./api/workflowValidate";

export async function workflowAccess(): Promise<WorkflowAccess> {
  if (desktop) return invoke("workflow_access");
  return sim.access();
}

/** Odblokowanie kluczem; błędny klucz kończy się wyjątkiem z komunikatem. */
export async function unlockWorkflows(key: string): Promise<WorkflowAccess> {
  if (desktop) return invoke("workflow_unlock", { key });
  return sim.unlock(key);
}

/**
 * Wyłączenie modułu (E7). Definicje i historia zostają, ponowne włączenie
 * wymaga klucza; przy przebiegach w toku backend odmawia z komunikatem.
 */
export async function lockWorkflows(): Promise<WorkflowAccess> {
  if (desktop) return invoke("workflow_lock");
  return sim.lock();
}

export async function listWorkflows(
  includeArchived = false,
): Promise<WorkflowSummary[]> {
  if (desktop) return invoke("workflow_list", { includeArchived });
  return sim.list(includeArchived);
}

export async function getWorkflow(id: number): Promise<WorkflowDetail> {
  if (desktop) return invoke("workflow_get", { id });
  return sim.get(id);
}

export async function createWorkflow(
  name: string,
  description: string,
  fromTemplate: boolean,
): Promise<WorkflowDetail> {
  if (desktop)
    return invoke("workflow_create", { name, description, fromTemplate });
  return sim.create(name, description, fromTemplate);
}

/** Zapis szkicu; nieaktualna `expectedRevision` kończy się konfliktem. */
export async function saveWorkflowDraft(
  id: number,
  expectedRevision: number,
  name: string,
  description: string,
  graph: WorkflowGraph,
): Promise<WorkflowDetail> {
  if (desktop)
    return invoke("workflow_save_draft", {
      id,
      expectedRevision,
      name,
      description,
      graph,
    });
  return sim.saveDraft(id, expectedRevision, name, description, graph);
}

export async function checkWorkflow(
  graph: WorkflowGraph,
): Promise<WorkflowIssue[]> {
  if (desktop) return invoke("workflow_validate", { graph });
  return sim.validate(graph);
}

export async function publishWorkflow(
  id: number,
  expectedRevision: number,
): Promise<WorkflowDetail> {
  if (desktop) return invoke("workflow_publish", { id, expectedRevision });
  return sim.publish(id, expectedRevision);
}

export async function archiveWorkflow(
  id: number,
  archived: boolean,
): Promise<WorkflowDetail> {
  if (desktop) return invoke("workflow_archive", { id, archived });
  return sim.archive(id, archived);
}

export async function duplicateWorkflow(id: number): Promise<WorkflowDetail> {
  if (desktop) return invoke("workflow_duplicate", { id });
  return sim.duplicate(id);
}

export async function exportWorkflow(id: number): Promise<string> {
  if (desktop) return invoke("workflow_export", { id });
  return sim.exportWorkflow(id);
}

export async function importWorkflow(text: string): Promise<WorkflowDetail> {
  if (desktop) return invoke("workflow_import", { text });
  return sim.importWorkflow(text);
}

/** Graf opublikowanej wersji (to, co uruchomi start), a nie szkic. */
export async function getWorkflowVersionGraph(
  versionId: number,
): Promise<WorkflowGraph> {
  if (desktop) return invoke("workflow_version_graph", { versionId });
  return sim.versionGraph(versionId);
}

/** Gotowość agentów opublikowanej wersji (CLI, wersje, sandbox); nie wywołuje modeli. */
export async function workflowPreflight(
  versionId: number,
): Promise<NodeReadiness[]> {
  if (desktop) return invoke("workflow_preflight", { versionId });
  return sim.preflight(versionId);
}
