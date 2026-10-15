<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  change,
  read,
  desktop,
  jiraConnected,
  quickAdd,
  quickTask,
  loadTemplates,
  refreshJiraConnected,
  templates,
  type Snapshot,
} from "./store";
import { compareTasks } from "./order";
import { dayKey } from "./time";
import {
  blockingError,
  complete,
  parse,
  removeToken,
  type Token,
} from "./quickadd";
import QuickChips from "./components/QuickChips.vue";
const emit = defineEmits<{ close: []; updated: [data: Snapshot] }>();
const data = ref<Snapshot>({ tasks: [], sessions: [] }),
  query = ref(""),
  error = ref(""),
  busy = ref(false),
  index = ref(0);
const input = ref<HTMLInputElement | null>(null);
let off: UnlistenFn | undefined;
const matches = computed(() =>
  data.value.tasks
    .filter(
      (t) =>
        !t.done &&
        !t.archived &&
        `${t.title} ${t.details?.project ?? ""}`
          .toLocaleLowerCase("pl")
          .includes(query.value.toLocaleLowerCase("pl")),
    )
    .sort(compareTasks())
    .slice(0, 30),
);
const projects = computed(() =>
  [
    ...new Set(
      data.value.tasks
        .map((t) => t.details?.project)
        .filter((p): p is string => Boolean(p)),
    ),
  ].sort((a, b) => a.localeCompare(b, "pl")),
);
const parsed = computed(() =>
  parse(query.value, dayKey(Date.now()), {
    projects: projects.value,
    templates: templates.value.map((t) => t.name),
    jiraEnabled: jiraConnected.value,
  }),
);
/** Brak wyników i jest co utworzyć: Enter tworzy i uruchamia, Shift+Enter tylko tworzy. */
const creatable = computed(
  () =>
    !matches.value.length &&
    !!query.value.trim() &&
    (!!parsed.value.title.trim() ||
      !!parsed.value.fields.jiraKey ||
      !!parsed.value.fields.template),
);
async function create(start: boolean) {
  if (busy.value) return;
  const blocking = blockingError(parsed.value);
  if (blocking) {
    error.value = blocking;
    return;
  }
  busy.value = true;
  try {
    emit("updated", await quickAdd(quickTask(parsed.value, start)));
    await close();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
function removeChip(token: Token) {
  query.value = removeToken(query.value, token);
  index.value = 0;
  input.value?.focus();
}
async function refresh() {
  try {
    data.value = await read();
    await refreshJiraConnected();
    // Bez listy szablonów przełączanie nadal działa (szablon byłby nieznany).
    await loadTemplates().catch(() => []);
    query.value = "";
    index.value = 0;
    error.value = "";
    document.documentElement.dataset.theme =
      localStorage.getItem("todo-track-theme") || "dark";
    await nextTick();
    input.value?.focus();
  } catch (e) {
    error.value = String(e);
  }
}
async function close() {
  if (desktop) await getCurrentWindow().hide();
  else emit("close");
}
async function start(id: number) {
  busy.value = true;
  try {
    emit("updated", await change("start", id));
    await close();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
function keys(e: KeyboardEvent) {
  if (e.key === "Escape") {
    e.preventDefault();
    void close();
  } else if (e.key === "ArrowDown") {
    e.preventDefault();
    index.value = Math.min(index.value + 1, matches.value.length - 1);
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    index.value = Math.max(0, index.value - 1);
  } else if (e.key === "Enter" && matches.value[index.value] && !busy.value) {
    e.preventDefault();
    void start(matches.value[index.value].id);
  } else if (e.key === "Enter" && creatable.value && !busy.value) {
    e.preventDefault();
    void create(!e.shiftKey);
  } else if (e.key === "Tab" && !e.shiftKey && e.target === input.value) {
    const el = input.value!;
    if (el.selectionStart !== el.selectionEnd) return;
    const done = complete(el.value, el.selectionStart ?? 0, {
      projects: projects.value,
      templates: templates.value.map((t) => t.name),
    });
    if (!done) return;
    e.preventDefault();
    query.value = done.text;
    void nextTick(() => el.setSelectionRange(done.caret, done.caret));
  }
}
onMounted(async () => {
  if (desktop) off = await listen("quick-open", refresh);
  await refresh();
});
onUnmounted(() => off?.());
</script>
<template>
  <section
    class="quick-switch"
    role="dialog"
    aria-label="Szybkie przełączanie"
    aria-modal="true"
    @keydown="keys"
  >
    <div class="ts-heading">
      <h2 data-tauri-drag-region>Szybkie przełączanie</h2>
      <button class="today-button" @click="close">Zamknij</button>
    </div>
    <input
      ref="input"
      v-model="query"
      aria-label="Znajdź zadanie do uruchomienia"
      placeholder="Wpisz nazwę zadania…"
      @input="index = 0"
    />
    <QuickChips :parsed="parsed" @remove="removeChip" />
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <div class="quick-results">
      <button
        v-for="(task, i) in matches"
        :key="task.id"
        class="quick-result"
        :class="{ selected: i === index }"
        :disabled="busy"
        @click="start(task.id)"
      >
        <span>{{ task.title }}</span
        ><small>{{ task.details?.project || "Bez projektu" }} · Start</small>
      </button>
      <p v-if="!matches.length" class="report-note">
        Brak pasujących nieukończonych zadań.
      </p>
      <div v-if="creatable" class="quick-create">
        <button
          class="quick-result selected"
          :disabled="busy"
          @click="create(true)"
        >
          <span>Utwórz i rozpocznij: {{ parsed.title || parsed.fields.jiraKey }}</span
          ><small>Enter</small>
        </button>
        <button class="quick-result" :disabled="busy" @click="create(false)">
          <span>Utwórz: {{ parsed.title || parsed.fields.jiraKey }}</span
          ><small>Shift+Enter</small>
        </button>
      </div>
    </div>
    <p class="hint">
      ↑ ↓ wybór · Enter uruchamia{{ creatable ? " · Shift+Enter tylko tworzy" : "" }} · Esc zamyka
    </p>
  </section>
</template>
