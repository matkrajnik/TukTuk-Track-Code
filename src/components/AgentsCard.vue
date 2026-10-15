<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import {
  onAgentsChanged,
  readAgents,
  type Agent,
  type AgentTask,
} from "../agents";
import { duration } from "../store";

defineProps<{ now: number }>();
const agents = ref<Agent[]>([]);
const labels: Record<Agent["kind"], string> = {
  claude: "Claude",
  codex: "Codex",
};
/** Opis dopasowanego zadania; bez dopasowania sam klucz albo nic. */
function taskLabel(agent: Agent): string {
  const task: AgentTask | null | undefined = agent.task;
  if (!task) return agent.key ? `${agent.key}: brak zadania` : "";
  if (task.source === "jira")
    return `Jira ${task.key}${task.title ? `: ${task.title}` : ""} (bez zadania)`;
  const title = `zadanie „${task.title}”`;
  return task.source === "folder" ? `${title} (folder)` : title;
}
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
  <section class="dashboard-card" aria-label="Agenci AI">
    <h2>Agenci AI</h2>
    <template
      v-for="agent in agents"
      :key="agent.kind + agent.cwd + agent.since"
    >
      <div class="dashboard-count">
        <span
          ><span class="agent-dot" aria-hidden="true" />{{
            labels[agent.kind]
          }}
          · {{ agent.name }}</span
        ><strong class="mono">{{
          duration(Math.max(0, now - agent.since))
        }}</strong>
      </div>
      <p
        v-if="agent.branch || taskLabel(agent)"
        class="agent-work"
        :aria-label="`Praca agenta ${agent.name}`"
      >
        <span v-if="agent.branch" class="agent-branch"
          >{{ agent.detached ? "odłączony HEAD" : "gałąź" }}
          <code class="mono">{{ agent.branch }}</code></span
        >
        <span v-if="taskLabel(agent)" class="agent-task">{{
          taskLabel(agent)
        }}</span>
      </p>
    </template>
    <p v-if="!agents.length" class="report-note">
      Żaden agent teraz nie pracuje.
    </p>
  </section>
</template>
