<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  dismissSuggestion,
  muteSuggestionFolders,
  onSuggestionsChanged,
  readSuggestions,
  switchToSuggestion,
  type AgentTask,
  type Suggestion,
} from "../agents";
import type { Snapshot, Task } from "../store";

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
const kinds = { claude: "Claude", codex: "Codex" } as const;
const who = (s: Suggestion) =>
  s.agents.map((a) => `${kinds[a.kind] ?? a.kind} · ${a.name}`).join(", ");
/** Nazwa zadania do etykiet przycisków. */
const name = (task: AgentTask) =>
  task.title || task.key || "zadanie bez tytułu";
const label = (task: AgentTask) =>
  task.taskId == null
    ? `${task.key}${task.title ? `: ${task.title}` : ""} (zadanie powstanie z Jiry)`
    : `„${task.title}”`;
const folders = (s: Suggestion) =>
  [...new Set(s.agents.map((a) => a.cwd))].join(", ");
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
    <template v-if="visible.length === 1">
      <h2>Agent pracuje nad innym zadaniem</h2>
      <p>
        {{ who(visible[0]) }} pracuje nad {{ label(visible[0].task) }}.
        {{ timer }} Przełączyć pomiar?
      </p>
      <div class="planning-actions">
        <button
          class="today-button"
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
          class="today-button"
          :disabled="busy"
          :title="folders(visible[0])"
          @click="mute(visible[0])"
        >
          Nie proponuj dla tego folderu
        </button>
      </div>
    </template>
    <template v-else>
      <h2>Agenci pracują nad innymi zadaniami</h2>
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
              class="today-button"
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
              class="today-button"
              :disabled="busy"
              :title="folders(s)"
              :aria-label="`Nie proponuj dla folderu: ${folders(s)}`"
              @click="mute(s)"
            >
              Nie proponuj
            </button>
          </span>
        </li>
      </ul>
    </template>
  </section>
</template>
