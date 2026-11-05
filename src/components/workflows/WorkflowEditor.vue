<script setup lang="ts">
/**
 * Edytor definicji procesu (PLAN_WDROZENIA.md, 4.3, etap E2): lista kroków
 * (pełna edycja klawiaturą), canvas z przeciąganiem, panel kroku i lista
 * problemów walidacji z przejściem do kroku. Zapis szkicu z oczekiwaną
 * rewizją (konflikt zamiast nadpisania), publikacja tylko poprawnego,
 * zapisanego grafu. Cofanie i ponawianie zmian w tej sesji edycji.
 * Uruchamianie procesów dochodzi w kolejnych etapach (E3–E5).
 */
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  onUnmounted,
  ref,
} from "vue";
import { ArrowLeft, Plus, Redo2, Undo2 } from "lucide-vue-next";
import ConfirmDialog from "../ConfirmDialog.vue";
import WorkflowGraphView from "./WorkflowGraphView.vue";
import WorkflowNodePanel from "./WorkflowNodePanel.vue";
import {
  getWorkflow,
  nodeTypeNames,
  publishWorkflow,
  saveWorkflowDraft,
  validateWorkflow,
  type WorkflowDetail,
  type WorkflowGraph,
} from "../../workflows";
import {
  addNode,
  addableTypes,
  unsavedDrafts,
  updateNode,
  type AddableType,
  type EditorDraft,
} from "../../workflowEdit";

const props = defineProps<{ id: number }>();
const emit = defineEmits<{ close: []; changed: [] }>();

/** Najwięcej zapamiętanych kroków cofania. */
const HISTORY_LIMIT = 100;

type Draft = EditorDraft;

const detail = ref<WorkflowDetail | null>(null);
const draft = ref<Draft | null>(null);
const saved = ref("");
const undoStack = ref<Draft[]>([]);
const redoStack = ref<Draft[]>([]);
const selected = ref<string | null>(null);
const busy = ref(false);
const error = ref("");
const conflict = ref(false);
const status = ref("");
const confirmLeave = ref(false);
const addType = ref<AddableType>("agent");

const snapshot = (d: Draft) => JSON.stringify(d);
const dirty = computed(
  () => draft.value !== null && snapshot(draft.value) !== saved.value,
);
const issues = computed(() =>
  draft.value ? validateWorkflow(draft.value.graph) : [],
);
/** Problemy przypisane do kroków (także przez połączenie, którego źródłem jest krok). */
const nodeIssues = computed(() => {
  const map = new Map<string, typeof issues.value>();
  const graph = draft.value?.graph;
  for (const issue of issues.value) {
    const nodeId =
      issue.nodeId ??
      graph?.edges.find((e) => e.id === issue.edgeId)?.source ??
      null;
    if (nodeId) map.set(nodeId, [...(map.get(nodeId) ?? []), issue]);
  }
  return map;
});
const problemNodes = computed(() => new Set(nodeIssues.value.keys()));
const selectedNode = computed(
  () => draft.value?.graph.nodes.find((n) => n.id === selected.value) ?? null,
);
const latestVersion = computed(() => detail.value?.versions[0] ?? null);
const publishBlocked = computed(() =>
  dirty.value
    ? "Najpierw zapisz zmiany."
    : issues.value.length
      ? "Popraw problemy, żeby opublikować."
      : detail.value && !detail.value.unpublishedChanges
        ? "Ta wersja jest już opublikowana."
        : "",
);

function load(value: WorkflowDetail) {
  detail.value = value;
  const next: Draft = {
    name: value.name,
    description: value.description,
    graph: structuredClone(value.draft),
  };
  draft.value = next;
  saved.value = snapshot(next);
  conflict.value = false;
  if (!next.graph.nodes.some((n) => n.id === selected.value))
    selected.value = next.graph.nodes[0]?.id ?? null;
}

/** Zostawione zmiany (wyjście do innego widoku) wracają, gdy rewizja się zgadza. */
function restoreUnsaved() {
  const pending = unsavedDrafts.get(props.id);
  unsavedDrafts.delete(props.id);
  if (!pending || !draft.value || !detail.value) return;
  if (pending.revision !== detail.value.draftRevision) {
    status.value =
      "Niezapisane zmiany z poprzedniego otwarcia pominięto: proces zapisano w międzyczasie.";
    return;
  }
  draft.value = pending.draft;
  fixSelection();
  status.value = "Przywrócono niezapisane zmiany z poprzedniego otwarcia.";
}

async function reload(restore = false) {
  busy.value = true;
  error.value = "";
  try {
    load(await getWorkflow(props.id));
    undoStack.value = [];
    redoStack.value = [];
    if (restore) restoreUnsaved();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}

/** Każda zmiana szkicu przechodzi tędy: jeden krok cofania. */
function change(next: Draft) {
  if (!draft.value) return;
  undoStack.value = [...undoStack.value, draft.value].slice(-HISTORY_LIMIT);
  redoStack.value = [];
  draft.value = next;
  status.value = "";
}
/** Wybrany krok musi istnieć (po usunięciu, cofnięciu, przywróceniu). */
function fixSelection() {
  const nodes = draft.value?.graph.nodes ?? [];
  if (!nodes.some((n) => n.id === selected.value))
    selected.value = nodes[0]?.id ?? null;
}
function changeGraph(graph: WorkflowGraph) {
  if (!draft.value) return;
  change({ ...draft.value, graph });
  fixSelection();
}
function undo() {
  const previous = undoStack.value.at(-1);
  if (!previous || !draft.value) return;
  redoStack.value = [...redoStack.value, draft.value];
  undoStack.value = undoStack.value.slice(0, -1);
  draft.value = previous;
  status.value = "";
  fixSelection();
}
function redo() {
  const next = redoStack.value.at(-1);
  if (!next || !draft.value) return;
  undoStack.value = [...undoStack.value, draft.value];
  redoStack.value = redoStack.value.slice(0, -1);
  draft.value = next;
  status.value = "";
  fixSelection();
}

function onKey(e: KeyboardEvent) {
  if (e.defaultPrevented || document.querySelector(".modal-overlay")) return;
  const target = e.target as HTMLElement | null;
  // W polach tekstowych działa natywne cofanie przeglądarki.
  if (target?.closest("input, textarea, select")) return;
  if (!(e.ctrlKey || e.metaKey)) return;
  const key = e.key.toLowerCase();
  if (key === "z" && !e.shiftKey) {
    e.preventDefault();
    undo();
  } else if (key === "y" || (key === "z" && e.shiftKey)) {
    e.preventDefault();
    redo();
  }
}

async function save() {
  if (!draft.value || !detail.value || busy.value) return;
  busy.value = true;
  error.value = "";
  // Zmiany wprowadzone w trakcie zapisu zostają niezapisane (porównanie z wysłanym stanem).
  const sent = draft.value;
  try {
    const next = await saveWorkflowDraft(
      props.id,
      detail.value.draftRevision,
      sent.name,
      sent.description,
      sent.graph,
    );
    detail.value = next;
    saved.value = snapshot(sent);
    status.value = "Zapisano szkic.";
    emit("changed");
  } catch (e) {
    const message = String(e instanceof Error ? e.message : e);
    error.value = message;
    conflict.value = message.includes("zmienił się w innym oknie");
  } finally {
    busy.value = false;
  }
}

async function publish() {
  if (!detail.value || busy.value || publishBlocked.value) return;
  busy.value = true;
  error.value = "";
  try {
    load(await publishWorkflow(props.id, detail.value.draftRevision));
    status.value = `Opublikowano wersję ${detail.value?.publishedVersion ?? ""}.`;
    emit("changed");
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}

function addStep() {
  if (!draft.value) return;
  const { graph, id } = addNode(draft.value.graph, addType.value);
  changeGraph(graph);
  void focusStep(id);
}

function moveNode(id: string, position: { x: number; y: number }) {
  if (!draft.value) return;
  changeGraph(updateNode(draft.value.graph, id, (n) => ({ ...n, position })));
}

async function focusStep(id: string) {
  if (!id) return;
  selected.value = id;
  await nextTick();
  document
    .querySelector<HTMLElement>(`[data-step="${CSS.escape(id)}"]`)
    ?.focus();
}

function back() {
  if (dirty.value) confirmLeave.value = true;
  else emit("close");
}

onMounted(() => {
  void reload(true);
  window.addEventListener("keydown", onKey);
});
// Przejście do innego widoku nie gubi zmian: zostają do ponownego otwarcia.
onBeforeUnmount(() => {
  if (dirty.value && draft.value && detail.value && !discarded)
    unsavedDrafts.set(props.id, {
      revision: detail.value.draftRevision,
      draft: draft.value,
    });
});
onUnmounted(() => window.removeEventListener("keydown", onKey));

/** „Porzuć zmiany” przy wyjściu: nic nie zostaje do przywrócenia. */
let discarded = false;
function discard() {
  discarded = true;
  unsavedDrafts.delete(props.id);
  emit("close");
}

const stepNames: Record<AddableType, string> = {
  agent: "Agent",
  condition: "Warunek",
  approval: "Akceptacja",
  end: "Koniec",
};
</script>

<template>
  <section class="wf-editor" aria-label="Edytor procesu">
    <header class="wf-editor-head">
      <button type="button" class="today-button" @click="back">
        <ArrowLeft :size="16" aria-hidden="true" /> Definicje
      </button>
      <template v-if="draft">
        <label class="wf-name">
          <span class="visually-hidden">Nazwa procesu</span>
          <input
            :value="draft.name"
            aria-label="Nazwa procesu"
            @change="
              change({
                ...draft,
                name: ($event.target as HTMLInputElement).value,
              })
            "
          />
        </label>
        <span class="wf-state" role="status">
          {{
            latestVersion
              ? `Opublikowano v${latestVersion.version}`
              : "Szkic"
          }}{{ dirty ? " • niezapisane zmiany" : "" }}{{
            !dirty && detail?.unpublishedChanges && latestVersion
              ? " • zmiany od publikacji"
              : ""
          }}
        </span>
        <div class="wf-editor-actions">
          <button
            type="button"
            class="icon-button"
            aria-label="Cofnij"
            title="Cofnij (Ctrl+Z)"
            :disabled="!undoStack.length"
            @click="undo"
          >
            <Undo2 :size="16" />
          </button>
          <button
            type="button"
            class="icon-button"
            aria-label="Ponów"
            title="Ponów (Ctrl+Y)"
            :disabled="!redoStack.length"
            @click="redo"
          >
            <Redo2 :size="16" />
          </button>
          <button
            type="button"
            class="today-button"
            :disabled="busy || !dirty"
            @click="save"
          >
            Zapisz
          </button>
          <button
            type="button"
            class="primary-button"
            :disabled="busy || !!publishBlocked"
            :title="publishBlocked || 'Utwórz niezmienną wersję procesu'"
            @click="publish"
          >
            Opublikuj
          </button>
        </div>
      </template>
    </header>

    <p v-if="error" class="error" role="alert">
      {{ error }}
      <button
        v-if="conflict"
        type="button"
        class="text-link"
        @click="reload()"
      >
        Wczytaj zapisaną wersję
      </button>
    </p>
    <p v-if="status" class="hint" role="status">{{ status }}</p>

    <div v-if="draft" class="wf-editor-body">
      <aside class="wf-steps" aria-label="Kroki procesu">
        <h3>Kroki</h3>
        <ol>
          <li v-for="node in draft.graph.nodes" :key="node.id">
            <button
              type="button"
              class="wf-step"
              :class="{ selected: node.id === selected }"
              :aria-pressed="node.id === selected"
              :data-step="node.id"
              @click="selected = node.id"
            >
              <span class="wf-step-type">{{ nodeTypeNames[node.kind.type] }}</span>
              <span class="wf-step-label">{{ node.label }}</span>
              <span
                v-if="nodeIssues.get(node.id)?.length"
                class="wf-step-problem"
                >{{ nodeIssues.get(node.id)?.length }} problem(y)</span
              >
            </button>
          </li>
        </ol>
        <div class="wf-add">
          <label>
            <span class="visually-hidden">Typ nowego kroku</span>
            <select v-model="addType" aria-label="Typ nowego kroku">
              <option v-for="t in addableTypes" :key="t" :value="t">
                {{ stepNames[t] }}
              </option>
            </select>
          </label>
          <button type="button" class="today-button" @click="addStep">
            <Plus :size="16" aria-hidden="true" /> Dodaj krok
          </button>
        </div>
        <label class="wf-description">
          Opis procesu
          <textarea
            :value="draft.description"
            rows="3"
            @change="
              change({
                ...draft,
                description: ($event.target as HTMLTextAreaElement).value,
              })
            "
          />
        </label>
      </aside>

      <WorkflowGraphView
        class="wf-graph"
        :graph="draft.graph"
        :selected="selected"
        :problems="problemNodes"
        @select="selected = $event"
        @move="moveNode"
      />

      <WorkflowNodePanel
        v-if="selectedNode"
        class="wf-panel"
        :graph="draft.graph"
        :node="selectedNode"
        :issues="nodeIssues.get(selectedNode.id) ?? []"
        @change="changeGraph"
      />
    </div>

    <section
      v-if="draft"
      class="wf-validation"
      aria-labelledby="wf-validation-title"
    >
      <h3 id="wf-validation-title">
        Walidacja:
        {{
          issues.length
            ? `${issues.length} ${issues.length === 1 ? "problem" : "problemy"}`
            : "brak problemów"
        }}
      </h3>
      <ul v-if="issues.length">
        <li v-for="(issue, i) in issues" :key="i">
          <span aria-hidden="true">⚠</span> {{ issue.message }}
          <button
            v-if="issue.nodeId || issue.edgeId"
            type="button"
            class="text-link"
            @click="
              focusStep(
                issue.nodeId ??
                  draft.graph.edges.find((e) => e.id === issue.edgeId)
                    ?.source ??
                  '',
              )
            "
          >
            Przejdź do kroku
          </button>
        </li>
      </ul>
    </section>

    <ConfirmDialog
      v-if="confirmLeave"
      title="Porzucić niezapisane zmiany?"
      confirm-label="Porzuć zmiany"
      @close="confirmLeave = false"
      @confirm="discard"
    >
      <p>Zmiany w tym procesie nie zostały zapisane.</p>
    </ConfirmDialog>
  </section>
</template>

<style scoped>
.wf-editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}
.wf-editor-head {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}
.wf-name input {
  min-width: 220px;
  font-size: var(--fs-md);
  font-weight: 600;
}
.wf-state {
  color: var(--muted);
  font-size: var(--fs-sm);
}
.wf-editor-actions {
  display: flex;
  gap: var(--space-1);
  margin-left: auto;
}
.wf-editor-body {
  display: grid;
  grid-template-columns: minmax(180px, 220px) minmax(0, 1fr) minmax(240px, 300px);
  gap: var(--space-3);
  align-items: start;
}
.wf-steps h3,
.wf-validation h3 {
  margin: 0 0 var(--space-2);
  font-size: var(--fs-sm);
}
.wf-steps ol {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.wf-step {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  width: 100%;
  height: auto;
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--line);
  text-align: left;
}
.wf-step.selected {
  border-color: var(--accent);
  background: var(--accent-bg);
}
.wf-step-type {
  color: var(--muted);
  font-size: var(--fs-xs);
}
.wf-step-label {
  font-weight: 600;
}
.wf-step-problem {
  color: var(--warning);
  font-size: var(--fs-xs);
}
.wf-add {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1);
  margin-top: var(--space-2);
}
.wf-add button {
  white-space: nowrap;
}
.wf-description {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: var(--space-3);
  font-size: var(--fs-sm);
}
.wf-description textarea {
  width: 100%;
  resize: vertical;
}
.wf-validation ul {
  margin: 0;
  padding-left: var(--space-4);
}
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}
@media (max-width: 1100px) {
  .wf-editor-body {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
