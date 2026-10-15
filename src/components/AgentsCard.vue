<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { onAgentsChanged, readAgents, type Agent } from "../agents";
import { duration } from "../store";

defineProps<{ now: number }>();
const agents = ref<Agent[]>([]);
const labels: Record<Agent["kind"], string> = {
  claude: "Claude",
  codex: "Codex",
};
let alive = true;
let unlisten: (() => void) | undefined;
onMounted(async () => {
  const stop = await onAgentsChanged((list) => (agents.value = list));
  if (!alive) return stop();
  unlisten = stop;
  const list = await readAgents().catch(() => []);
  if (alive) agents.value = list;
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
});
</script>

<template>
  <section class="dashboard-card dashboard-top" aria-label="Agenci AI">
    <h2>Agenci AI</h2>
    <div
      v-for="agent in agents"
      :key="agent.kind + agent.cwd + agent.since"
      class="dashboard-count"
    >
      <span
        ><span class="agent-dot" aria-hidden="true" />{{ labels[agent.kind] }}
        · {{ agent.name }}</span
      ><strong class="mono">{{ duration(Math.max(0, now - agent.since)) }}</strong>
    </div>
    <p v-if="!agents.length" class="report-note">
      Żaden agent teraz nie pracuje.
    </p>
  </section>
</template>
