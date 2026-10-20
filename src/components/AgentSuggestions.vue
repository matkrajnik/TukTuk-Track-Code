<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { Bot } from "lucide-vue-next";
import {
  dismissSuggestion,
  muteSuggestionFolders,
  onSuggestionsChanged,
  readSuggestions,
  signalOnly,
  signalTitle,
  sourceLabel,
  switchToSuggestion,
  type AgentTask,
  type Suggestion,
} from "../agents";
import { bindAgentFolder, type Snapshot, type Task } from "../store";

/**
 * Baner propozycji przełączenia (wzór: baner bezczynności). Timer zmienia się
 * tylko po kliknięciu „Przełącz”.
 */
const props = defineProps<{ tasks: Task[]; activeTaskId: number | null }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const suggestions = ref<Suggestion[]>([]);
const busy = ref(false);
/** Zmiana timera ukrywa propozycję od razu, bez czekania na backend. */
const visible = computed(() =>
  suggestions.value.filter(
    (s) => s.task.taskId == null || s.task.taskId !== props.activeTaskId,
  ),
);
const kinds: Record<string, string> = {
  claude: "Claude",
  codex: "Codex",
  window: sourceLabel("window"),
  browser: sourceLabel("browser"),
};
const who = (s: Suggestion) =>
  s.agents.map((a) => `${kinds[a.kind] ?? a.kind} · ${a.name}`).join(", ");
/** Nazwa zadania do etykiet przycisków. */
const name = (task: AgentTask) =>
  task.title || task.key || "zadanie bez tytułu";
/** Zgłoszenie Redmine (`#1234`, R3) albo Jira: z którego systemu powstanie zadanie. */
const origin = (task: AgentTask) =>
  task.source === "redmine" || task.key?.startsWith("#") ? "Redmine" : "Jiry";
const label = (task: AgentTask) =>
  task.taskId == null
    ? `${task.key}${task.title ? `: ${task.title}` : ""} (zadanie powstanie z ${origin(task)})`
    : `„${task.title}”`;
const folders = (s: Suggestion) =>
  [...new Set(s.agents.map((a) => a.cwd).filter(Boolean))].join(", ");
/** Sygnał z przeglądarki albo Teams nie ma folderu: bez wyciszania folderu. */
const hasFolder = (s: Suggestion) => s.agents.some((a) => !!a.cwd);
const timer = computed(() => {
  const task = props.tasks.find((t) => t.id === props.activeTaskId);
  return task ? `Timer mierzy „${task.title}”.` : "Timer stoi.";
});

async function reload() {
  try {
    suggestions.value = await readSuggestions();
  } catch {
    suggestions.value = [];
  }
}
async function run(action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  try {
    await action();
    await reload();
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    busy.value = false;
  }
}
const switchTo = (s: Suggestion) =>
  run(async () => emit("updated", await switchToSuggestion(s.task)));
const dismiss = (s: Suggestion) =>
  run(async () => {
    suggestions.value = await dismissSuggestion(s);
  });
const mute = (s: Suggestion) => run(() => muteSuggestionFolders(s));
/** Tekst banera dla jednej propozycji: agent AI albo sygnał z okna (W3). */
const single = computed(() => {
  const s = visible.value[0];
  if (!s) return { title: "", text: "" };
  return signalOnly(s)
    ? {
        title: signalTitle(s),
        text: `${who(s)} pokazuje ${label(s.task)}.`,
      }
    : {
        title: "Agent pracuje nad innym zadaniem",
        text: `${who(s)} pracuje nad ${label(s.task)}.`,
      };
});
/** „Zapamiętaj dla tego folderu”: przypisuje foldery agentów do zadania X. */
const canRemember = (s: Suggestion) =>
  s.task.taskId != null && s.task.source !== "folder" && hasFolder(s);
const remember = (s: Suggestion) =>
  run(async () => {
    const taskId = s.task.taskId;
    if (taskId == null) return;
    for (const cwd of new Set(s.agents.map((a) => a.cwd).filter(Boolean)))
      await bindAgentFolder(cwd, taskId);
  });

watch(() => props.activeTaskId, reload);
let stop: (() => void) | undefined;
let alive = true;
let tick: ReturnType<typeof setInterval>;
onMounted(async () => {
  // Reguła 2 minut zależy od czasu; Rust odświeża co 5 s, podgląd co 15 s.
  tick = setInterval(reload, 15_000);
  try {
    const unlisten = await onSuggestionsChanged(() => void reload());
    if (!alive) return unlisten();
    stop = unlisten;
  } catch {}
  await reload();
});
onUnmounted(() => {
  alive = false;
  clearInterval(tick);
  stop?.();
});
</script>

<template>
  <section
    v-if="visible.length"
    class="idle-banner agent-banner"
    aria-label="Propozycja przełączenia"
  >
    <Bot class="infobar-icon" :size="20" aria-hidden="true" />
    <template v-if="visible.length === 1">
      <h2>{{ single.title }}</h2>
      <p>{{ single.text }} {{ timer }} Przełączyć pomiar?</p>
      <div class="planning-actions">
        <button
          class="primary-button"
          :disabled="busy"
          @click="switchTo(visible[0])"
        >
          Przełącz</button
        ><button
          class="today-button"
          :disabled="busy"
          @click="dismiss(visible[0])"
        >
          Nie teraz</button
        ><button
          v-if="hasFolder(visible[0])"
          class="today-button"
          :disabled="busy"
          :title="folders(visible[0])"
          @click="mute(visible[0])"
        >
          Nie proponuj dla tego folderu</button
        ><button
          v-if="canRemember(visible[0])"
          class="today-button"
          :disabled="busy"
          :title="folders(visible[0])"
          @click="remember(visible[0])"
        >
          Zapamiętaj dla tego folderu
        </button>
      </div>
    </template>
    <template v-else>
      <h2>
        {{
          visible.every(signalOnly)
            ? "Pracujesz nad innymi zadaniami?"
            : "Agenci pracują nad innymi zadaniami"
        }}
      </h2>
      <p>{{ timer }} Wybierz zadanie, na które przełączyć pomiar.</p>
      <ul class="agent-suggestions">
        <li
          v-for="s in visible"
          :key="s.task.taskId ?? s.task.key ?? ''"
          :aria-label="`Propozycja: ${name(s.task)}`"
        >
          <span
            ><strong>{{ label(s.task) }}</strong> · {{ who(s) }}</span
          >
          <span class="planning-actions">
            <button
              class="secondary-button"
              :disabled="busy"
              :aria-label="`Przełącz na: ${name(s.task)}`"
              @click="switchTo(s)"
            >
              Przełącz</button
            ><button
              class="today-button"
              :disabled="busy"
              :aria-label="`Nie teraz: ${name(s.task)}`"
              @click="dismiss(s)"
            >
              Nie teraz</button
            ><button
              v-if="hasFolder(s)"
              class="today-button"
              :disabled="busy"
              :title="folders(s)"
              :aria-label="`Nie proponuj dla folderu: ${folders(s)}`"
              @click="mute(s)"
            >
              Nie proponuj</button
            ><button
              v-if="canRemember(s)"
              class="today-button"
              :disabled="busy"
              :title="folders(s)"
              :aria-label="`Zapamiętaj folder dla: ${name(s.task)}`"
              @click="remember(s)"
            >
              Zapamiętaj
            </button>
          </span>
        </li>
      </ul>
    </template>
  </section>
</template>
