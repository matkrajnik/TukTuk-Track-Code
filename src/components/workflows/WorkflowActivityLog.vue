<script setup lang="ts">
/**
 * Dziennik agentów (E8.1): czytelne linie komunikatów i narzędzi z zapisanych
 * zdarzeń. Tylko interpolacja tekstu; narzędzia rozpoznaje klasa, ale sens
 * niesie sam tekst. Starsze linie za „Pokaż wcześniejsze”.
 */
import { computed, ref } from "vue";
import {
  MAX_LOG_LINES_RENDERED,
  buildActivityLog,
  type StoredEvent,
} from "../../workflowRuns";

const props = defineProps<{ events: StoredEvent[] }>();

const showAll = ref(false);
const lines = computed(() => buildActivityLog(props.events));
const hidden = computed(() =>
  showAll.value ? 0 : Math.max(0, lines.value.length - MAX_LOG_LINES_RENDERED),
);
const visible = computed(() =>
  showAll.value ? lines.value : lines.value.slice(-MAX_LOG_LINES_RENDERED),
);
</script>

<template>
  <div>
    <button v-if="hidden" type="button" class="text-link" @click="showAll = true">
      Pokaż wcześniejsze ({{ hidden }})
    </button>
    <ol class="wf-log" aria-label="Dziennik agentów">
      <li
        v-for="line in visible"
        :key="line.sequence"
        :class="{ 'wf-log-tool': line.kind === 'tool' }"
      >
        {{ line.text }}
      </li>
    </ol>
  </div>
</template>
