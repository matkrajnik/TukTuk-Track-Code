<script setup lang="ts">
/**
 * Start przebiegu (E5): wersja, zadanie, cel i repozytorium. Modele i uprawnienia
 * kroków są tylko do odczytu (zmiana wymaga edycji i publikacji), a „Uruchom” jest
 * nieaktywny przy blokadzie preflightu. Kroki, modele i gotowość pochodzą z grafu
 * uruchamianej (najnowszej opublikowanej) wersji, nie ze szkicu. Identyfikator
 * żądania powstaje raz na próbę: podwójne kliknięcie go nie zmienia, błąd startu tak.
 * Bez `workflowId` (start z karty zadania) najpierw wybiera się opublikowany proces.
 */
import { computed, onMounted, ref, watch } from "vue";
import ConfirmDialog from "../ConfirmDialog.vue";
import WorkflowReadiness from "./WorkflowReadiness.vue";
import {
  getWorkflow,
  getWorkflowVersionGraph,
  listWorkflows,
  workflowPreflight,
  type WorkflowDetail,
  type WorkflowSummary,
  type WorkflowGraph,
} from "../../workflows";
import { newRequestId, startWorkflowRun } from "../../workflowRuns";
import { read as readSnapshot, type Task } from "../../store";

const props = defineProps<{ workflowId?: number | null; taskId?: number | null }>();
const emit = defineEmits<{ close: []; started: [runId: number] }>();

const workflows = ref<WorkflowSummary[]>([]);
const workflowId = ref<number | null>(props.workflowId ?? null);
const detail = ref<WorkflowDetail | null>(null);
const graph = ref<WorkflowGraph | null>(null);
const tasks = ref<Task[]>([]);
const taskId = ref<number | null>(props.taskId ?? null);
const goal = ref("");
const repo = ref("");
const error = ref("");
const busy = ref(false);
const blocked = ref(false);
/**
 * Jeden identyfikator na próbę startu: podwójne kliknięcie nie tworzy drugiego
 * przebiegu. Po błędzie (np. worktree) host oznaczył przebieg jako nieudany, więc
 * ponowienie potrzebuje nowego identyfikatora, inaczej wróciłby tamten przebieg.
 */
const requestId = ref(newRequestId());

/** Najnowsza opublikowana wersja: `versions` jest od najnowszej (`store::get`). */
const version = computed(() => detail.value?.versions[0] ?? null);

const agents = computed(() =>
  (graph.value?.nodes ?? []).flatMap((n) =>
    n.kind.type === "agent"
      ? [
          {
            id: n.id,
            label: n.label,
            provider: n.kind.provider,
            model: n.kind.model ?? "domyślny",
            access: n.kind.access,
          },
        ]
      : [],
  ),
);

function pickTask() {
  const task = tasks.value.find((t) => t.id === taskId.value);
  if (task && !goal.value.trim()) goal.value = task.title;
}

/** Numer ostatniego wczytania: odpowiedź dla poprzednio wybranego procesu jest pomijana. */
let loadSeq = 0;

async function loadWorkflow(id: number | null) {
  const seq = ++loadSeq;
  detail.value = null;
  graph.value = null;
  if (id === null) return;
  // Blokada do czasu odczytu gotowości: „Uruchom” nie jest aktywne przed preflightem.
  blocked.value = true;
  try {
    const found = await getWorkflow(id);
    if (seq !== loadSeq) return;
    detail.value = found;
    const latest = found.versions[0];
    if (!latest) return;
    const versionGraph = await getWorkflowVersionGraph(latest.id);
    const readiness = await workflowPreflight(latest.id);
    if (seq !== loadSeq) return;
    graph.value = versionGraph;
    blocked.value = readiness.some((r) => r.blocked);
  } catch (e) {
    if (seq === loadSeq) error.value = e instanceof Error ? e.message : String(e);
  }
}

watch(workflowId, (id) => {
  error.value = "";
  void loadWorkflow(id);
});

onMounted(async () => {
  try {
    if (props.workflowId == null) {
      workflows.value = (await listWorkflows()).filter(
        (w) => w.archivedAt === null && w.publishedVersion !== null,
      );
    }
    await loadWorkflow(workflowId.value);
    tasks.value = (await readSnapshot()).tasks.filter((t) => !t.done);
    pickTask();
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
});

const canStart = computed(
  () =>
    !busy.value &&
    !blocked.value &&
    version.value !== null &&
    repo.value.trim() !== "",
);

async function submit() {
  if (!canStart.value || !version.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await startWorkflowRun({
      versionId: version.value.id,
      requestId: requestId.value,
      taskId: taskId.value,
      repo: repo.value.trim(),
      goal: goal.value.trim() || null,
    });
    emit("started", result.runId);
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
    requestId.value = newRequestId();
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    :title="`Uruchom: ${detail?.name ?? 'proces'}`"
    label="Uruchom proces"
    :error="error"
    :busy="busy"
    @close="emit('close')"
  >
    <form class="settings-fields" @submit.prevent="submit">
      <label v-if="props.workflowId == null">
        Proces
        <select v-model="workflowId">
          <option :value="null">Wybierz opublikowany proces</option>
          <option v-for="w in workflows" :key="w.id" :value="w.id">
            {{ w.name }} (v{{ w.publishedVersion }})
          </option>
        </select>
      </label>
      <p v-if="props.workflowId == null && !workflows.length" class="hint">
        Brak opublikowanych procesów. Opublikuj proces w Definicjach.
      </p>
      <p v-if="detail && !version" class="hint">
        Proces nie ma opublikowanej wersji. Opublikuj go w edytorze.
      </p>
      <label>
        Zadanie (opcjonalnie)
        <select v-model="taskId" @change="pickTask">
          <option :value="null">Bez zadania</option>
          <option v-for="t in tasks" :key="t.id" :value="t.id">
            {{ t.title }}
          </option>
        </select>
      </label>
      <label>
        Cel przebiegu
        <textarea v-model="goal" rows="3" maxlength="20000" />
      </label>
      <label>
        Repozytorium (katalog główny Git)
        <input v-model="repo" placeholder="C:\Projekty\moje-repo" />
      </label>
      <section v-if="graph" aria-label="Kroki agentów (tylko do odczytu)">
        <h3>Kroki agentów</h3>
        <ul class="hint">
          <li v-for="a in agents" :key="a.id">
            {{ a.label }}: {{ a.provider }}, model {{ a.model }}, dostęp
            {{ a.access }}
          </li>
        </ul>
        <p class="hint">
          Modele i uprawnienia zmienisz w definicji (edycja i publikacja).
        </p>
      </section>
      <WorkflowReadiness
        v-if="version && graph"
        :version-id="version.id"
        :graph="graph"
      />
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="emit('close')">
          Anuluj
        </button>
        <button type="submit" class="primary-button" :disabled="!canStart">
          Uruchom
        </button>
      </div>
    </form>
  </ConfirmDialog>
</template>
