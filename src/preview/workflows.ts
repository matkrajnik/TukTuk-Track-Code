/**
 * Podgląd modułu Workflows (`workflows::store` i komendy w Rust) w localStorage.
 * Reguły i komunikaty jak w Rust: rewizja szkicu, niezmienne wersje, archiwum,
 * import jako nieopublikowany szkic. Klucz dostępu sprawdzany tym samym skrótem
 * SHA-256 co `workflows::access` (sam klucz nie występuje w kodzie).
 */
import {
  WORKFLOW_SCHEMA_VERSION,
  type NodeReadiness,
  type WorkflowAccess,
  type WorkflowDetail,
  type WorkflowGraph,
  type WorkflowIssue,
  type WorkflowSummary,
  type WorkflowVersion,
} from "../api/workflows";
import { validateWorkflow } from "../api/workflowValidate";
import { parseWorkflowGraph } from "../api/workflowParse";
import {
  analysisImplementationReview,
  blankWorkflow,
} from "../api/workflowTemplates";

export const previewWorkflowsKey = "tuktuk-track-preview-workflows-v1";

/** `workflows::access::WORKFLOWS_KEY_SHA256` jako tekst szesnastkowy. */
const KEY_SHA256 =
  "201507eb208ea694e028361cb97ae12527d9a9c3d5d7649ef6e3d584ff9f4095";
const MAX_NAME_CHARS = 120;
const MAX_DESCRIPTION_CHARS = 2_000;
const MAX_IMPORT_BYTES = 1_000_000;
/** `store::MAX_DRAFT_BYTES`. */
const MAX_DRAFT_BYTES = 1_000_000;
const COPY_SUFFIX = " (kopia)";
const EXPORT_FORMAT = "tuktuk-track-workflow";
const EXPORT_FORMAT_VERSION = 1;
const LOCKED =
  "Moduł Workflows nie jest odblokowany. Wpisz klucz dostępu w Ustawieniach.";
const CONFLICT =
  "Proces zmienił się w innym oknie. Odśwież go przed zapisem, żeby nie nadpisać zmian.";
const NOT_A_FILE = "To nie jest poprawny plik procesu TukTuk Track.";

interface StoredVersion extends WorkflowVersion {
  graph: WorkflowGraph;
}

interface StoredWorkflow {
  id: number;
  name: string;
  description: string;
  draft: WorkflowGraph;
  draftRevision: number;
  createdAt: number;
  updatedAt: number;
  archivedAt: number | null;
  versions: StoredVersion[];
}

interface State {
  unlocked: boolean;
  nextId: number;
  workflows: StoredWorkflow[];
}

function read(): State {
  try {
    const value = JSON.parse(localStorage.getItem(previewWorkflowsKey) ?? "{}");
    return {
      unlocked: value?.unlocked === true,
      nextId: typeof value?.nextId === "number" ? value.nextId : 1,
      workflows: Array.isArray(value?.workflows) ? value.workflows : [],
    };
  } catch {
    return { unlocked: false, nextId: 1, workflows: [] };
  }
}

function write(state: State): void {
  try {
    localStorage.setItem(previewWorkflowsKey, JSON.stringify(state));
  } catch {
    // Podgląd bez localStorage: stan żyje do przeładowania.
  }
}

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(text),
  );
  return [...new Uint8Array(digest)]
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

const byteLength = (text: string) => new TextEncoder().encode(text).length;

/** Kanoniczny tekst grafu jak `store::to_json` (z limitem rozmiaru). */
function toJson(graph: WorkflowGraph): string {
  const text = JSON.stringify(graph);
  if (byteLength(text) > MAX_DRAFT_BYTES)
    throw new Error("Proces jest za duży (limit 1 MB).");
  return text;
}

/** Graf z wejścia (UI, import) w postaci kanonicznej, jak serde w Rust. */
function canonical(graph: unknown, prefix: string): WorkflowGraph {
  try {
    return parseWorkflowGraph(graph);
  } catch (e) {
    throw new Error(`${prefix}: ${e instanceof Error ? e.message : e}`);
  }
}

const graphHash = (graph: WorkflowGraph) => sha256(toJson(graph));

function cleanName(name: string): string {
  const trimmed = name.trim();
  if (!trimmed) throw new Error("Proces musi mieć nazwę.");
  if ([...trimmed].length > MAX_NAME_CHARS)
    throw new Error(
      `Nazwa procesu może mieć najwyżej ${MAX_NAME_CHARS} znaków.`,
    );
  return trimmed;
}

function cleanDescription(description: string): string {
  const trimmed = description.trim();
  if ([...trimmed].length > MAX_DESCRIPTION_CHARS)
    throw new Error(
      `Opis procesu może mieć najwyżej ${MAX_DESCRIPTION_CHARS} znaków.`,
    );
  return trimmed;
}

function checkSchema(graph: WorkflowGraph): void {
  if (graph.schemaVersion !== WORKFLOW_SCHEMA_VERSION)
    throw new Error(
      `Nieobsługiwana wersja formatu procesu: ${graph.schemaVersion}.`,
    );
}

function unlocked(): State {
  const state = read();
  if (!state.unlocked) throw new Error(LOCKED);
  return state;
}

function find(state: State, id: number): StoredWorkflow {
  const workflow = state.workflows.find((w) => w.id === id);
  if (!workflow) throw new Error("Nie znaleziono procesu.");
  return workflow;
}

async function summary(w: StoredWorkflow): Promise<WorkflowSummary> {
  const latest = w.versions[w.versions.length - 1];
  return {
    id: w.id,
    name: w.name,
    description: w.description,
    draftRevision: w.draftRevision,
    createdAt: w.createdAt,
    updatedAt: w.updatedAt,
    archivedAt: w.archivedAt,
    publishedVersion: latest?.version ?? null,
    publishedAt: latest?.createdAt ?? null,
    unpublishedChanges: !latest || (await graphHash(w.draft)) !== latest.hash,
  };
}

async function detail(w: StoredWorkflow): Promise<WorkflowDetail> {
  return {
    ...(await summary(w)),
    draft: w.draft,
    issues: validateWorkflow(w.draft),
    versions: [...w.versions]
      .reverse()
      .map(({ id, version, hash, createdAt }) => ({
        id,
        version,
        hash,
        createdAt,
      })),
  };
}

function insert(
  state: State,
  name: string,
  description: string,
  graph: WorkflowGraph,
): StoredWorkflow {
  // Kolejność kontroli jak `store::create`: nazwa, opis, format, rozmiar.
  const cleanedName = cleanName(name);
  const cleanedDescription = cleanDescription(description);
  checkSchema(graph);
  const draft = canonical(graph, "Nie udało się zapisać grafu");
  toJson(draft);
  const now = Date.now();
  const workflow: StoredWorkflow = {
    id: state.nextId,
    name: cleanedName,
    description: cleanedDescription,
    draft,
    draftRevision: 1,
    createdAt: now,
    updatedAt: now,
    archivedAt: null,
    versions: [],
  };
  write({
    ...state,
    nextId: state.nextId + 1,
    workflows: [...state.workflows, workflow],
  });
  return workflow;
}

function replace(state: State, next: StoredWorkflow): void {
  write({
    ...state,
    workflows: state.workflows.map((w) => (w.id === next.id ? next : w)),
  });
}

export async function access(): Promise<WorkflowAccess> {
  return { unlocked: read().unlocked };
}

export async function unlock(key: string): Promise<WorkflowAccess> {
  if ((await sha256(key.trim().toUpperCase())) !== KEY_SHA256)
    throw new Error("Nieprawidłowy klucz dostępu.");
  write({ ...read(), unlocked: true });
  return { unlocked: true };
}

export async function list(
  includeArchived: boolean,
): Promise<WorkflowSummary[]> {
  const shown = unlocked()
    .workflows.filter((w) => includeArchived || w.archivedAt === null)
    .sort((a, b) => b.updatedAt - a.updatedAt || b.id - a.id);
  return Promise.all(shown.map(summary));
}

export async function get(id: number): Promise<WorkflowDetail> {
  return detail(find(unlocked(), id));
}

export async function create(
  name: string,
  description: string,
  fromTemplate: boolean,
): Promise<WorkflowDetail> {
  const graph = fromTemplate ? analysisImplementationReview() : blankWorkflow();
  return detail(insert(unlocked(), name, description, graph));
}

export async function saveDraft(
  id: number,
  expectedRevision: number,
  name: string,
  description: string,
  graph: WorkflowGraph,
): Promise<WorkflowDetail> {
  const state = unlocked();
  // Kolejność jak `store::save_draft`: dane wejściowe, potem stan procesu.
  const cleanedName = cleanName(name);
  const cleanedDescription = cleanDescription(description);
  checkSchema(graph);
  const draft = canonical(graph, "Nie udało się zapisać grafu");
  toJson(draft);
  const workflow = find(state, id);
  if (workflow.archivedAt !== null)
    throw new Error("Proces jest w archiwum; przywróć go, żeby edytować.");
  if (workflow.draftRevision !== expectedRevision) throw new Error(CONFLICT);
  const next: StoredWorkflow = {
    ...workflow,
    name: cleanedName,
    description: cleanedDescription,
    draft,
    draftRevision: workflow.draftRevision + 1,
    updatedAt: Date.now(),
  };
  replace(state, next);
  return detail(next);
}

export async function validate(graph: WorkflowGraph): Promise<WorkflowIssue[]> {
  unlocked();
  return validateWorkflow(graph);
}

export async function publish(
  id: number,
  expectedRevision: number,
): Promise<WorkflowDetail> {
  const state = unlocked();
  const workflow = find(state, id);
  if (workflow.archivedAt !== null)
    throw new Error("Proces jest w archiwum; przywróć go przed publikacją.");
  if (workflow.draftRevision !== expectedRevision) throw new Error(CONFLICT);
  const issues = validateWorkflow(workflow.draft);
  if (issues.length)
    throw new Error(
      `Proces ma błędy (${issues.length}); popraw je przed publikacją.`,
    );
  const hash = await graphHash(workflow.draft);
  // Po `await` stan mógł się zmienić (Rust robi to w jednej transakcji):
  // czytamy go ponownie i sprawdzamy rewizję przed zapisem.
  const fresh = read();
  const current = find(fresh, id);
  if (current.draftRevision !== expectedRevision) throw new Error(CONFLICT);
  const latest = current.versions[current.versions.length - 1];
  // Ponowna publikacja bez zmian zwraca istniejącą wersję (podwójne kliknięcie).
  if (latest?.hash === hash) return detail(current);
  const now = Date.now();
  const next: StoredWorkflow = {
    ...current,
    updatedAt: now,
    versions: [
      ...current.versions,
      {
        id: current.id * 1000 + (latest?.version ?? 0) + 1,
        version: (latest?.version ?? 0) + 1,
        hash,
        createdAt: now,
        graph: structuredClone(current.draft),
      },
    ],
  };
  replace(fresh, next);
  return detail(next);
}

export async function archive(
  id: number,
  archived: boolean,
): Promise<WorkflowDetail> {
  const state = unlocked();
  const now = Date.now();
  const next: StoredWorkflow = {
    ...find(state, id),
    archivedAt: archived ? now : null,
    updatedAt: now,
  };
  replace(state, next);
  return detail(next);
}

export async function duplicate(id: number): Promise<WorkflowDetail> {
  const state = unlocked();
  const source = find(state, id);
  // Skraca nazwę źródła, żeby dopisek zawsze się zmieścił (`store::duplicate`).
  const base = [...source.name]
    .slice(0, MAX_NAME_CHARS - [...COPY_SUFFIX].length)
    .join("");
  const name = `${base}${COPY_SUFFIX}`;
  return detail(insert(state, name, source.description, source.draft));
}

export async function exportWorkflow(id: number): Promise<string> {
  const workflow = find(unlocked(), id);
  return JSON.stringify(
    {
      format: EXPORT_FORMAT,
      formatVersion: EXPORT_FORMAT_VERSION,
      name: workflow.name,
      description: workflow.description,
      graph: workflow.draft,
    },
    null,
    2,
  );
}

function parseImport(text: string): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(text);
    if (value && typeof value === "object" && !Array.isArray(value))
      return value as Record<string, unknown>;
  } catch {
    // Niżej ten sam komunikat co dla złej struktury.
  }
  throw new Error(NOT_A_FILE);
}

export async function importWorkflow(text: string): Promise<WorkflowDetail> {
  const state = unlocked();
  if (byteLength(text) > MAX_IMPORT_BYTES)
    throw new Error("Plik procesu jest za duży (limit 1 MB).");
  // Jak serde w `store::import`: najpierw kształt pliku, potem format.
  const file = parseImport(text);
  const prefix = NOT_A_FILE.replace(/\.$/, "");
  if (
    typeof file.format !== "string" ||
    typeof file.formatVersion !== "number" ||
    typeof file.name !== "string"
  )
    throw new Error(NOT_A_FILE);
  const graph = canonical(file.graph, prefix);
  if (
    file.format !== EXPORT_FORMAT ||
    file.formatVersion !== EXPORT_FORMAT_VERSION
  )
    throw new Error("Nieobsługiwany format pliku procesu.");
  const description =
    typeof file.description === "string" ? file.description : "";
  return detail(insert(state, file.name, description, graph));
}

/** Podgląd nie uruchamia agentów, więc nie udaje gotowości żadnego węzła. */
export const PREVIEW_PREFLIGHT_BLOCK = "Podgląd nie uruchamia agentów";

/**
 * Podgląd zawsze zwraca blokadę: nie ma tu haków testowych podmieniających
 * wynik (te są w zastępczym IPC `tests/fixtures/tauri.ts`).
 */
export async function preflight(versionId: number): Promise<NodeReadiness[]> {
  const state = unlocked();
  const stored = state.workflows
    .flatMap((w) => w.versions)
    .find((v) => v.id === versionId);
  if (!stored) throw new Error("Nie znaleziono wersji procesu.");
  return stored.graph.nodes.flatMap((node) =>
    node.kind.type === "agent"
      ? [
          {
            nodeId: node.id,
            provider: node.kind.provider,
            access: node.kind.access,
            cliFound: false,
            version: null,
            supported: false,
            enforcement: "none" as const,
            warning: null,
            blocked: {
              code: "cliMissing" as const,
              message: PREVIEW_PREFLIGHT_BLOCK,
            },
          },
        ]
      : [],
  );
}
