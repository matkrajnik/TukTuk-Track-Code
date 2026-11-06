<script setup lang="ts">
/**
 * Workflows (PLAN_WDROZENIA.md, 4.1–4.3, etap E2): przegląd i lista definicji
 * procesów z edytorem. Moduł widać dopiero po odblokowaniu kluczem; backend
 * i tak sprawdza dostęp w każdej komendzie. Uruchamianie procesów, monitor
 * i akceptacje dochodzą w etapach E3–E5, więc nie ma tu przycisku „Uruchom”.
 */
import { computed, nextTick, onMounted, ref, watch } from "vue";
import ConfirmDialog from "../components/ConfirmDialog.vue";
import WorkflowRunForm from "../components/workflows/WorkflowRunForm.vue";
import WorkflowEditor from "../components/workflows/WorkflowEditor.vue";
import type { View } from "../components/AppNav.vue";
import {
  archiveWorkflow,
  createWorkflow,
  duplicateWorkflow,
  exportWorkflow,
  importWorkflow,
  listWorkflows,
  type WorkflowSummary,
} from "../workflows";
import { TEMPLATE_NAME } from "../api/workflowTemplates";

const props = defineProps<{ section: "overview" | "definitions" }>();
const emit = defineEmits<{ open: [view: View]; run: [runId: number] }>();
/** Proces, dla którego otwarto formularz uruchomienia. */
const runFor = ref<number | null>(null);

/** Ile procesów pokazuje Przegląd w „Ostatnio zmienione”. */
const RECENT = 5;

const list = ref<WorkflowSummary[]>([]);
const loading = ref(true);
const error = ref("");
const status = ref("");
const showArchived = ref(false);
const editing = ref<number | null>(null);

type Dialog =
  | { kind: "new"; template: boolean }
  | { kind: "import" }
  | { kind: "export"; name: string; text: string }
  | null;
const dialog = ref<Dialog>(null);
const dialogError = ref("");
const dialogBusy = ref(false);
const newName = ref("");
const newDescription = ref("");
const importText = ref("");
// Import: fokus w polu JSON (ConfirmDialog zaczyna od pierwszego pola input/przycisku).
watch(
  () => dialog.value?.kind,
  async (kind) => {
    if (kind !== "import") return;
    await nextTick();
    await nextTick();
    document.getElementById("wf-import-text")?.focus();
  },
);

const fail = (e: unknown) => String(e instanceof Error ? e.message : e);

async function refresh() {
  error.value = "";
  try {
    list.value = await listWorkflows(showArchived.value);
  } catch (e) {
    error.value = fail(e);
  } finally {
    loading.value = false;
  }
}
onMounted(refresh);
watch(showArchived, refresh);

const active = computed(() => list.value.filter((w) => w.archivedAt === null));
const published = computed(
  () => active.value.filter((w) => w.publishedVersion !== null).length,
);
const pending = computed(
  () => active.value.filter((w) => w.unpublishedChanges).length,
);
const recent = computed(() => active.value.slice(0, RECENT));

const formatDate = (ms: number) =>
  new Date(ms).toLocaleString("pl-PL", {
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });

function stateText(w: WorkflowSummary) {
  if (w.archivedAt !== null) return "W archiwum";
  if (w.publishedVersion === null) return "Szkic, nieopublikowany";
  return w.unpublishedChanges
    ? `v${w.publishedVersion} • zmiany od publikacji`
    : `v${w.publishedVersion} • opublikowany`;
}

function openNew(template: boolean) {
  dialog.value = { kind: "new", template };
  newName.value = template ? TEMPLATE_NAME : "";
  newDescription.value = "";
  dialogError.value = "";
}

function openImport() {
  dialog.value = { kind: "import" };
  dialogError.value = "";
}

async function runDialog(action: () => Promise<void>) {
  dialogBusy.value = true;
  dialogError.value = "";
  try {
    await action();
  } catch (e) {
    dialogError.value = fail(e);
  } finally {
    dialogBusy.value = false;
  }
}

/** Edytor otwiera się w Definicjach; z Przeglądu najpierw zmiana widoku. */
const pendingEdit = ref<number | null>(null);
function edit(id: number) {
  if (props.section === "definitions") editing.value = id;
  else {
    pendingEdit.value = id;
    emit("open", "workflow-definitions");
  }
}
watch(
  () => props.section,
  (section) => {
    editing.value = section === "definitions" ? pendingEdit.value : null;
    pendingEdit.value = null;
  },
);

async function create() {
  const current = dialog.value;
  if (current?.kind !== "new") return;
  await runDialog(async () => {
    const created = await createWorkflow(
      newName.value,
      newDescription.value,
      current.template,
    );
    dialog.value = null;
    await refresh();
    edit(created.id);
  });
}

async function runImport() {
  await runDialog(async () => {
    const imported = await importWorkflow(importText.value);
    dialog.value = null;
    importText.value = "";
    await refresh();
    status.value = `Zaimportowano „${imported.name}” jako szkic.`;
  });
}

async function act(action: () => Promise<unknown>, message: string) {
  error.value = "";
  status.value = "";
  try {
    await action();
    await refresh();
    status.value = message;
  } catch (e) {
    error.value = fail(e);
  }
}

function toggleArchive(w: WorkflowSummary) {
  const archive = w.archivedAt === null;
  void act(
    () => archiveWorkflow(w.id, archive),
    archive
      ? `Przeniesiono „${w.name}” do archiwum.`
      : `Przywrócono „${w.name}”.`,
  );
}

async function openExport(w: WorkflowSummary) {
  error.value = "";
  try {
    const text = await exportWorkflow(w.id);
    dialog.value = { kind: "export", name: w.name, text };
    dialogError.value = "";
  } catch (e) {
    error.value = fail(e);
  }
}

async function copyExport() {
  const current = dialog.value;
  if (current?.kind !== "export") return;
  try {
    await navigator.clipboard.writeText(current.text);
    status.value = `Skopiowano JSON procesu „${current.name}”.`;
    dialog.value = null;
  } catch {
    dialogError.value =
      "Nie udało się skopiować. Zaznacz tekst i skopiuj go ręcznie (Ctrl+C).";
  }
}

async function closeEditor() {
  editing.value = null;
  await refresh();
}

const dialogTitle = computed(() => {
  const current = dialog.value;
  if (current?.kind === "new")
    return current.template ? "Nowy proces z szablonu" : "Nowy proces";
  if (current?.kind === "import") return "Import procesu";
  return `Eksport: ${current?.name ?? ""}`;
});
const dialogLabel = computed(() => {
  switch (dialog.value?.kind) {
    case "new":
      return "Nowy proces";
    case "import":
      return "Import procesu";
    default:
      return "Eksport procesu";
  }
});
</script>

<template>
  <div class="workflows-view">
    <WorkflowEditor
      v-if="section === 'definitions' && editing !== null"
      :id="editing"
      @close="closeEditor"
      @changed="refresh"
    />
    <template v-else>
      <div class="page-heading">
        <h1>
          {{ section === "overview" ? "Workflows" : "Definicje procesów" }}
        </h1>
      </div>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <p v-if="status" class="hint" role="status">{{ status }}</p>

      <template v-if="section === 'overview'">
        <div class="wf-tiles">
          <div class="dashboard-card wf-tile">
            <span class="wf-tile-value">{{ active.length }}</span>
            <span>definicji</span>
          </div>
          <div class="dashboard-card wf-tile">
            <span class="wf-tile-value">{{ published }}</span>
            <span>opublikowanych</span>
          </div>
          <div class="dashboard-card wf-tile">
            <span class="wf-tile-value">{{ pending }}</span>
            <span>ze zmianami do publikacji</span>
          </div>
        </div>
        <div class="wf-toolbar">
          <button type="button" class="primary-button" @click="openNew(true)">
            Nowy z szablonu
          </button>
          <button type="button" class="today-button" @click="openNew(false)">
            Pusty proces
          </button>
          <button
            type="button"
            class="today-button"
            @click="emit('open', 'workflow-definitions')"
          >
            Wszystkie definicje
          </button>
        </div>
        <section class="dashboard-card" aria-labelledby="wf-recent">
          <h2 id="wf-recent">Ostatnio zmienione</h2>
          <p v-if="!loading && !recent.length" class="empty-state">
            Nie masz jeszcze procesów. Zacznij od szablonu „{{ TEMPLATE_NAME }}”.
          </p>
          <ul v-else class="wf-recent">
            <li v-for="w in recent" :key="w.id">
              <button type="button" class="text-link" @click="edit(w.id)">
                {{ w.name }}
              </button>
              <span class="hint">{{ stateText(w) }}</span>
            </li>
          </ul>
        </section>
        <p class="hint">
          Opublikowany proces uruchomisz z listy definicji; przebiegi i zgody
          zobaczysz w „Uruchomieniach”.
        </p>
      </template>

      <template v-else>
        <div class="wf-toolbar">
          <button type="button" class="primary-button" @click="openNew(true)">
            Z szablonu
          </button>
          <button type="button" class="today-button" @click="openNew(false)">
            Nowy
          </button>
          <button type="button" class="today-button" @click="openImport">
            Importuj JSON
          </button>
          <label class="inline-check wf-archived">
            <input v-model="showArchived" type="checkbox" /> Pokaż archiwum
          </label>
        </div>
        <p v-if="!loading && !list.length" class="empty-state">
          Brak procesów. Utwórz pierwszy z szablonu „{{ TEMPLATE_NAME }}”.
        </p>
        <table v-else-if="list.length" class="wf-table">
          <thead>
            <tr>
              <th scope="col">Nazwa</th>
              <th scope="col">Stan</th>
              <th scope="col">Ostatnia zmiana</th>
              <th scope="col"><span class="visually-hidden">Akcje</span></th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="w in list" :key="w.id">
              <td>
                <strong>{{ w.name }}</strong>
                <div v-if="w.description" class="hint">{{ w.description }}</div>
              </td>
              <td>{{ stateText(w) }}</td>
              <td>{{ formatDate(w.updatedAt) }}</td>
              <td class="wf-row-actions">
                <button
                  v-if="w.archivedAt === null && w.publishedVersion !== null"
                  type="button"
                  class="primary-button"
                  :aria-label="`Uruchom ${w.name}`"
                  @click="runFor = w.id"
                >
                  Uruchom
                </button>
                <button
                  v-if="w.archivedAt === null"
                  type="button"
                  class="today-button"
                  :aria-label="`Edytuj ${w.name}`"
                  @click="edit(w.id)"
                >
                  Edytuj
                </button>
                <button
                  type="button"
                  class="today-button"
                  :aria-label="`Duplikuj ${w.name}`"
                  @click="
                    act(
                      () => duplicateWorkflow(w.id),
                      `Utworzono kopię „${w.name}”.`,
                    )
                  "
                >
                  Duplikuj
                </button>
                <button
                  type="button"
                  class="today-button"
                  :aria-label="`Eksportuj ${w.name}`"
                  @click="openExport(w)"
                >
                  Eksportuj
                </button>
                <button
                  type="button"
                  class="today-button"
                  :aria-label="
                    w.archivedAt === null
                      ? `Archiwizuj ${w.name}`
                      : `Przywróć ${w.name}`
                  "
                  @click="toggleArchive(w)"
                >
                  {{ w.archivedAt === null ? "Archiwizuj" : "Przywróć" }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </template>
    </template>

    <WorkflowRunForm
      v-if="runFor !== null"
      :workflow-id="runFor"
      @close="runFor = null"
      @started="
        (id) => {
          runFor = null;
          emit('run', id);
        }
      "
    />
    <!-- Okno montowane na czas otwarcia (useModal w ConfirmDialog): fokus
         w oknie, pułapka Tab, Esc i powrót fokusu do przycisku, który je otworzył. -->
    <ConfirmDialog
      v-if="dialog"
      :title="dialogTitle"
      :label="dialogLabel"
      :error="dialogError"
      :busy="dialogBusy"
      @close="dialog = null"
    >
        <template v-if="dialog.kind === 'new'">
          <form class="settings-fields" @submit.prevent="create">
            <label>
              Nazwa
              <input v-model="newName" required maxlength="120" />
            </label>
            <label>
              Opis
              <textarea v-model="newDescription" rows="3" maxlength="2000" />
            </label>
            <p class="hint">
              {{
                dialog.template
                  ? "Claude analizuje, Codex implementuje, Claude robi review; do trzech rund poprawek i decyzja użytkownika."
                  : "Pusty proces ze Startem połączonym z Końcem."
              }}
            </p>
            <div class="modal-actions">
              <button
                type="button"
                :disabled="dialogBusy"
                @click="dialog = null"
              >
                Anuluj
              </button>
              <button
                type="submit"
                class="primary-button"
                :disabled="dialogBusy || !newName.trim()"
              >
                Utwórz
              </button>
            </div>
          </form>
        </template>
        <template v-else-if="dialog.kind === 'import'">
          <form class="settings-fields" @submit.prevent="runImport">
            <label>
              JSON procesu
              <textarea
                id="wf-import-text"
                v-model="importText"
                rows="10"
                spellcheck="false"
              />
            </label>
            <p class="hint">
              Import tworzy nieopublikowany szkic i niczego nie uruchamia.
            </p>
            <div class="modal-actions">
              <button
                type="button"
                :disabled="dialogBusy"
                @click="dialog = null"
              >
                Anuluj
              </button>
              <button
                type="submit"
                class="primary-button"
                :disabled="dialogBusy || !importText.trim()"
              >
                Importuj
              </button>
            </div>
          </form>
        </template>
        <template v-else>
          <label class="settings-fields">
            JSON procesu
            <textarea
              :value="dialog.text"
              rows="12"
              readonly
              spellcheck="false"
            />
          </label>
          <div class="modal-actions">
            <button type="button" @click="dialog = null">Zamknij</button>
            <button type="button" class="primary-button" @click="copyExport">
              Kopiuj
            </button>
          </div>
        </template>
    </ConfirmDialog>
  </div>
</template>

<style scoped>
.workflows-view {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}
.wf-tiles {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
  gap: var(--space-3);
}
.wf-tile {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.wf-tile-value {
  font-size: var(--fs-2xl);
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.wf-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-2);
}
.wf-archived {
  margin-left: auto;
}
.wf-recent {
  margin: 0;
  padding: 0;
  list-style: none;
}
.wf-recent li {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-1) 0;
}
.wf-table {
  width: 100%;
  border-collapse: collapse;
}
.wf-table th,
.wf-table td {
  padding: var(--space-2);
  border-bottom: 1px solid var(--line);
  text-align: left;
  vertical-align: top;
}
.wf-table th {
  color: var(--muted);
  font-size: var(--fs-sm);
  font-weight: 600;
}
.wf-row-actions {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: var(--space-1);
}
.modal textarea {
  width: 100%;
  font-family: ui-monospace, monospace;
  font-size: var(--fs-sm);
}
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}
</style>
