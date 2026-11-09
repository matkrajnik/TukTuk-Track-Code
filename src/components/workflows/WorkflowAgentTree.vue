<script setup lang="ts">
/**
 * Drzewo orkiestrator → subagenci (E8.6): stan tekstem i ikoną, bieżąca czynność
 * i licznik. Tylko interpolacja tekstu, bez v-html. Licznik jest regionem
 * `aria-live`, więc czytnik ogłasza zmiany liczby aktywnych subagentów.
 */
import { computed } from "vue";
import {
  buildActivityTree,
  type ExecutionStatus,
  type LaneState,
  type StoredEvent,
} from "../../workflowRuns";

const props = defineProps<{
  events: StoredEvent[];
  executionId: string;
  /** Status kroku orkiestratora: po stanie końcowym żaden tor nie jest „w toku”. */
  status?: ExecutionStatus;
  title?: string;
}>();

const tree = computed(() =>
  buildActivityTree(props.events, props.executionId, props.status),
);
const active = computed(
  () => tree.value.subagents.filter((s) => s.state === "running").length,
);

const STATE_LABELS: Record<LaneState, { text: string; icon: string }> = {
  running: { text: "w toku", icon: "●" },
  done: { text: "zakończony", icon: "✓" },
  failed: { text: "błąd", icon: "✕" },
  interrupted: { text: "przerwany", icon: "■" },
};
</script>

<template>
  <section class="wf-tree" :aria-label="title ?? 'Drzewo agentów'">
    <p class="wf-tree-count" aria-live="polite">
      Subagenci: {{ active }} aktywnych / {{ tree.subagents.length }} łącznie
    </p>
    <p v-if="tree.truncated" class="hint" role="note">
      Osiągnięto limit zapisanych zdarzeń kroku; bieżące czynności nie są już aktualizowane.
    </p>
    <ul class="wf-tree-list">
      <li :data-orchestrator-state="tree.orchestrator.state">
        <span class="wf-tree-state"
          ><span aria-hidden="true">{{ STATE_LABELS[tree.orchestrator.state].icon }}</span>
          {{ STATE_LABELS[tree.orchestrator.state].text }}</span
        >
        <span class="wf-tree-name">{{ tree.orchestrator.label }}</span>
        <span v-if="tree.orchestrator.current" class="wf-tree-current">
          — {{ tree.orchestrator.current }}
        </span>
        <ul v-if="tree.subagents.length">
          <li v-for="lane in tree.subagents" :key="lane.id" :data-state="lane.state">
            <span class="wf-tree-state"
              ><span aria-hidden="true">{{ STATE_LABELS[lane.state].icon }}</span>
              {{ STATE_LABELS[lane.state].text }}</span
            >
            <span class="wf-tree-name">{{ lane.label }}</span>
            <span v-if="lane.current" class="wf-tree-current">— {{ lane.current }}</span>
          </li>
        </ul>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.wf-tree-list,
.wf-tree-list ul {
  list-style: none;
  margin: 0;
  padding-left: var(--space-3);
}
.wf-tree-list {
  padding-left: 0;
}
.wf-tree-name {
  font-weight: 600;
}
.wf-tree-state {
  margin-right: var(--space-2);
  font-size: 0.85em;
}
li[data-state="failed"] .wf-tree-state {
  font-weight: 700;
}
</style>
