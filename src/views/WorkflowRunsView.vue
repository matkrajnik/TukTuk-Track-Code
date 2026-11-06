<script setup lang="ts">
/**
 * Uruchomienia Workflows (E5): historia przebiegów z filtrami i monitor wybranego.
 * Historia działa także bez klucza po stronie Rust; widok i tak pokazuje się dopiero
 * po odblokowaniu (jak reszta modułu).
 */
import { onMounted, ref, watch } from "vue";
import WorkflowFaults from "../components/workflows/WorkflowFaults.vue";
import WorkflowRunMonitor from "../components/workflows/WorkflowRunMonitor.vue";
import {
  RUN_STATUS_LABELS,
  listWorkflowRuns,
  type RunFilter,
  type RunStatus,
  type RunSummary,
} from "../workflowRuns";

const props = defineProps<{ runId?: number | null }>();

const PAGE = 20;
const runs = ref<RunSummary[]>([]);
const selected = ref<number | null>(props.runId ?? null);
const status = ref<RunStatus | "">("");
const repo = ref("");
const taskId = ref("");
const day = ref("");
const more = ref(false);
const error = ref("");
const loading = ref(true);

function filter(beforeId: number | null): RunFilter {
  const from = day.value ? new Date(`${day.value}T00:00:00`).getTime() : null;
  return {
    status: status.value ? [status.value] : [],
    repo: repo.value.trim() || null,
    taskId: taskId.value ? Number(taskId.value) : null,
    from,
    to: from === null ? null : from + 24 * 3_600_000 - 1,
    beforeId,
    limit: PAGE,
  };
}

async function load(append: boolean) {
  error.value = "";
  loading.value = true;
  try {
    const before = append ? (runs.value[runs.value.length - 1]?.id ?? null) : null;
    const page = await listWorkflowRuns(filter(before));
    runs.value = append ? [...runs.value, ...page] : page;
    more.value = page.length === PAGE;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

function date(ms: number): string {
  return new Date(ms).toLocaleString("pl-PL");
}

watch(() => props.runId, (id) => {
  if (id != null) selected.value = id;
});
watch([status, repo, taskId, day], () => void load(false));
onMounted(() => load(false));
</script>

<template>
  <div class="wf-runs">
    <h1>Uruchomienia</h1>
    <WorkflowFaults />
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <form class="wf-filters" @submit.prevent="load(false)">
      <label>
        Status
        <select v-model="status">
          <option value="">Wszystkie</option>
          <option v-for="(v, k) in RUN_STATUS_LABELS" :key="k" :value="k">
            {{ v.label }}
          </option>
        </select>
      </label>
      <label>Repozytorium <input v-model="repo" /></label>
      <label>Zadanie (id) <input v-model="taskId" inputmode="numeric" /></label>
      <label>Dzień <input v-model="day" type="date" /></label>
    </form>
    <p v-if="!loading && !runs.length" class="empty-state">
      Brak przebiegów dla tych filtrów.
    </p>
    <table v-else class="wf-table">
      <thead>
        <tr>
          <th scope="col">Proces</th>
          <th scope="col">Status</th>
          <th scope="col">Zadanie</th>
          <th scope="col">Utworzony</th>
          <th scope="col"><span class="visually-hidden">Akcje</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in runs" :key="r.id" :class="{ selected: r.id === selected }">
          <td>{{ r.workflowName }} (v{{ r.version }})</td>
          <td>
            <span aria-hidden="true">{{ RUN_STATUS_LABELS[r.status].icon }}</span>
            {{ RUN_STATUS_LABELS[r.status].label }}
          </td>
          <td>{{ r.taskTitle ?? "—" }}</td>
          <td>{{ date(r.createdAt) }}</td>
          <td>
            <button
              type="button"
              class="today-button"
              :aria-label="`Otwórz przebieg ${r.id}`"
              @click="selected = r.id"
            >
              Otwórz
            </button>
          </td>
        </tr>
      </tbody>
    </table>
    <button v-if="more" type="button" class="today-button" @click="load(true)">
      Więcej
    </button>
    <WorkflowRunMonitor v-if="selected !== null" :key="selected" :run-id="selected" />
  </div>
</template>
