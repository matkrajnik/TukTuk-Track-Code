<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { change, read, desktop, type Snapshot } from "./store";
import { compareTasks } from "./order";
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
async function refresh() {
  try {
    data.value = await read();
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
    </div>
    <p class="hint">↑ ↓ wybór · Enter uruchamia · Esc zamyka</p>
  </section>
</template>
