<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import {
  onAgentsChanged,
  readAgents,
  type Agent,
  type AgentTask,
} from "../agents";
import { bindAgentFolder, duration, type Task } from "../store";
import { FolderCheck } from "lucide-vue-next";
import SearchSelect from "./SearchSelect.vue";

const props = defineProps<{ now: number; tasks: Task[] }>();
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
  const title = `zadanie „${task.title}”${task.source === "folder" ? " (folder)" : ""}`;
  return task.done ? `${title} (ukończone)` : title;
}
/** Zadania do wyboru przy przypisaniu folderu: do zrobienia, poza archiwum. */
const choices = computed(() =>
  props.tasks
    .filter((t) => !t.done && !t.archived)
    .sort((a, b) => a.title.localeCompare(b.title, "pl")),
);
/** Agent bez lokalnego zadania: można zapamiętać zadanie dla jego folderu. */
const unbound = (agent: Agent) => agent.task?.taskId == null && !!agent.cwd;
/** Wybór zadania i błąd przypisania dla folderu agenta. */
const picked = reactive<Record<string, number | "">>({});
const bindError = reactive<Record<string, string>>({});
async function bind(agent: Agent) {
  const taskId = picked[agent.cwd];
  if (!taskId) return;
  bindError[agent.cwd] = "";
  try {
    await bindAgentFolder(agent.cwd, taskId);
    delete picked[agent.cwd];
    agents.value = await readAgents();
  } catch (e) {
    bindError[agent.cwd] = String(e instanceof Error ? e.message : e);
  }
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
      <form
        v-if="unbound(agent) && choices.length"
        class="agent-bind"
        @submit.prevent="bind(agent)"
      >
        <SearchSelect
          v-model="picked[agent.cwd]"
          class="agent-bind-select"
          :label="`Zadanie dla folderu ${agent.cwd}`"
          placeholder="Wybierz zadanie dla tego folderu…"
          :title="agent.cwd"
          :options="choices.map((t) => ({ value: t.id, label: t.title }))"
        />
        <button
          class="today-button"
          type="submit"
          :disabled="!picked[agent.cwd]"
          :aria-label="`Zapamiętaj dla folderu ${agent.cwd}`"
        >
          <FolderCheck :size="14" />Zapamiętaj dla tego folderu
        </button>
        <span
          v-if="bindError[agent.cwd]"
          class="agent-bind-error"
          role="alert"
          >{{ bindError[agent.cwd] }}</span
        >
      </form>
    </template>
    <p v-if="!agents.length" class="report-note">
      Żaden agent teraz nie pracuje.
    </p>
  </section>
</template>
