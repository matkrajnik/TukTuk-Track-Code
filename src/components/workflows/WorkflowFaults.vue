<script setup lang="ts">
/**
 * Problemy zgłoszone przez silnik Workflows (np. po rekonsyliacji przy starcie).
 * Zdarzenie `workflow-fault` bywa wysłane, zanim okno zacznie nasłuch, więc
 * komponent najpierw rejestruje nasłuch, potem czyta snapshot z Rust i scala oba
 * źródła bez duplikatów. Działa bez klucza (DoD 13 pkt 3). Bez problemów nic nie
 * renderuje. „Ukryj” działa tylko do zamknięcia okna.
 */
import { onMounted, onUnmounted, ref } from "vue";
import {
  faultKey,
  onWorkflowFault,
  workflowFaults,
  type WorkflowFault,
} from "../../workflowRuns";
import { openRunRequest, workflowsUnlocked } from "../../workflowAccess";

const faults = ref<WorkflowFault[]>([]);
/** Ukryte w tej sesji okna; nie wracają przy kolejnym zdarzeniu o tym samym problemie. */
const hidden = new Set<string>();
let alive = true;
let unlisten: (() => void) | null = null;

function add(fault: WorkflowFault) {
  const key = faultKey(fault);
  if (hidden.has(key) || faults.value.some((f) => faultKey(f) === key)) return;
  faults.value = [...faults.value, fault];
}

function hide(fault: WorkflowFault) {
  hidden.add(faultKey(fault));
  faults.value = faults.value.filter((f) => faultKey(f) !== faultKey(fault));
}

onMounted(async () => {
  const stop = await onWorkflowFault(add);
  if (!alive) {
    stop();
    return;
  }
  unlisten = stop;
  try {
    for (const fault of await workflowFaults()) add(fault);
  } catch (e) {
    console.warn("Nie udało się odczytać problemów Workflows:", e);
  }
});

onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlisten = null;
});
</script>

<template>
  <section v-if="faults.length" class="wf-faults" role="alert" aria-label="Problemy Workflows">
    <h2>Problemy Workflows</h2>
    <ul>
      <li v-for="fault in faults" :key="faultKey(fault)">
        <span>{{ fault.message }}</span>
        <button
          v-if="fault.runId > 0 && workflowsUnlocked"
          type="button"
          @click="openRunRequest = fault.runId"
        >
          Otwórz przebieg {{ fault.runId }}
        </button>
        <button type="button" @click="hide(fault)">Ukryj</button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.wf-faults {
  border: 1px solid var(--danger, #b3261e);
  border-radius: 8px;
  padding: 8px 12px;
  margin: 8px 0;
}
.wf-faults h2 {
  font-size: 1rem;
  margin: 0 0 4px;
}
.wf-faults ul {
  list-style: none;
  margin: 0;
  padding: 0;
}
.wf-faults li {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  padding: 2px 0;
}
.wf-faults li span {
  flex: 1 1 16rem;
}
</style>
