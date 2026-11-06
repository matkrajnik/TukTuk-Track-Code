<script setup lang="ts">
/**
 * Aktywne przebiegi Workflows na tablicy agentów (E5). Przebieg nie jest sesją
 * hooków (`agent_sessions`) i nie liczy czasu człowieka; tu tylko lista przebiegów
 * w toku, odświeżana przy zmianie stanu przebiegu (`runStateChanged`), nie przy
 * każdym komunikacie agenta.
 *
 * Bez klucza (odzyskiwanie dostępu, DoD 13 pkt 3): historia i anulowanie działają
 * bez odblokowania, więc sekcja pokazuje aktywne przebiegi z przyciskiem
 * „Zatrzymaj” (bez otwierania monitora). Bez aktywnych przebiegów sekcji nie ma.
 */
import { onMounted, onUnmounted, ref } from "vue";
import ConfirmDialog from "../ConfirmDialog.vue";
import {
  RUN_STATUS_LABELS,
  cancelWorkflowRun,
  listWorkflowRuns,
  listenWorkflowEvents,
  type RunStatus,
  type RunSummary,
} from "../../workflowRuns";
import {
  loadWorkflowAccess,
  openRunRequest,
  workflowsUnlocked,
} from "../../workflowAccess";

/** Stany „w toku” (bez stanów końcowych z `RunStatus::is_terminal`). */
const ACTIVE: RunStatus[] = [
  "queued",
  "running",
  "waitingForApproval",
  "paused",
  "cancelling",
];
const LIMIT = 20;

const runs = ref<RunSummary[]>([]);
const error = ref("");
const notice = ref("");
const stopping = ref<RunSummary | null>(null);
let alive = true;
let unlisten: (() => void) | null = null;
/** Numer odczytu: starsza odpowiedź nie nadpisuje nowszej. */
let seq = 0;

async function load() {
  const current = ++seq;
  try {
    const list = await listWorkflowRuns({ status: ACTIVE, limit: LIMIT });
    if (!alive || current !== seq) return;
    runs.value = list;
    error.value = "";
  } catch (e) {
    if (alive && current === seq)
      error.value = e instanceof Error ? e.message : String(e);
  }
}

function open(id: number) {
  openRunRequest.value = id;
}

async function stop() {
  const run = stopping.value;
  stopping.value = null;
  if (!run) return;
  try {
    await cancelWorkflowRun(run.id);
    notice.value = `Zatrzymywanie przebiegu ${run.id}…`;
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  }
  await load();
}

onMounted(async () => {
  await loadWorkflowAccess();
  if (!alive) return;
  const stopListening = await listenWorkflowEvents((event) => {
    if (event.type === "runStateChanged") void load();
  });
  if (!alive) {
    stopListening();
    return;
  }
  unlisten = stopListening;
  await load();
});

onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlisten = null;
});
</script>

<template>
  <section
    v-if="runs.length || (workflowsUnlocked && error)"
    class="dashboard-card board-section"
    aria-labelledby="board-workflow-runs-title"
  >
    <h2 id="board-workflow-runs-title">
      Przebiegi procesów w toku
      <span class="board-count">{{ runs.length }}</span>
    </h2>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="notice" class="hint" role="status">{{ notice }}</p>
    <p v-if="!workflowsUnlocked" class="hint">
      Moduł Workflows jest zablokowany: przebieg można tylko zatrzymać.
    </p>
    <ul class="board-list">
      <li v-for="run in runs" :key="run.id" class="board-item">
        <div class="board-line">
          <span class="board-name">
            {{ run.workflowName }}
            <template v-if="run.taskTitle">: {{ run.taskTitle }}</template>
          </span>
          <span class="board-time">
            <span aria-hidden="true">{{ RUN_STATUS_LABELS[run.status].icon }}</span>
            {{ RUN_STATUS_LABELS[run.status].label }}
          </span>
        </div>
        <div class="board-actions">
          <button
            v-if="workflowsUnlocked"
            type="button"
            class="text-link"
            :aria-label="`Otwórz przebieg ${run.id}`"
            @click="open(run.id)"
          >
            Otwórz przebieg
          </button>
          <button
            v-else-if="run.status !== 'cancelling'"
            type="button"
            class="text-link"
            :aria-label="`Zatrzymaj przebieg ${run.id}`"
            @click="stopping = run"
          >
            Zatrzymaj
          </button>
        </div>
      </li>
    </ul>
    <ConfirmDialog
      v-if="stopping"
      title="Zatrzymać przebieg?"
      confirm-label="Zatrzymaj"
      @close="stopping = null"
      @confirm="stop"
    >
      <p>Bieżący krok zostanie przerwany, a kolejne nie ruszą.</p>
    </ConfirmDialog>
  </section>
</template>
