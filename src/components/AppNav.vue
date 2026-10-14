<script lang="ts">
export type View =
  | "tasks"
  | "reports"
  | "jira"
  | "dashboard"
  | "journal"
  | "library";
</script>

<script setup lang="ts">
import { ref, watch } from "vue";
import {
  ArrowDownToLine,
  CalendarDays,
  Check,
  ExternalLink,
  ListTodo,
  Moon,
  Sun,
} from "lucide-vue-next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktop } from "../store";

const view = defineModel<View>("view", { required: true });
const emit = defineEmits<{ quick: []; error: [message: string] }>();

const theme = ref<"dark" | "light">("dark");
try {
  if (localStorage.getItem("todo-track-theme") === "light")
    theme.value = "light";
} catch {}
watch(
  theme,
  (value) => {
    document.documentElement.dataset.theme = value;
    try {
      localStorage.setItem("todo-track-theme", value);
    } catch {}
  },
  { immediate: true },
);

async function hide() {
  try {
    await getCurrentWindow().hide();
  } catch (e) {
    emit("error", String(e));
  }
}
</script>

<template>
  <header class="topbar">
    <a class="brand" href="#" @click.prevent="view = 'tasks'"
      ><span class="brand-icon"><Check :size="18" /></span>todo<span
        >track</span
      ></a
    >
    <nav aria-label="Nawigacja główna">
      <button
        :class="{ selected: view === 'tasks' }"
        :aria-pressed="view === 'tasks'"
        @click="view = 'tasks'"
      >
        <ListTodo :size="16" />Zadania
      </button>
      <button
        :class="{ selected: view === 'dashboard' }"
        :aria-pressed="view === 'dashboard'"
        @click="view = 'dashboard'"
      >
        Dashboard
      </button>
      <button
        :class="{ selected: view === 'journal' }"
        :aria-pressed="view === 'journal'"
        @click="view = 'journal'"
      >
        Plan i historia
      </button>
      <button
        :class="{ selected: view === 'reports' }"
        :aria-pressed="view === 'reports'"
        @click="view = 'reports'"
      >
        <CalendarDays :size="16" />Raporty dzienne
      </button>
      <button
        :class="{ selected: view === 'jira' }"
        :aria-pressed="view === 'jira'"
        @click="view = 'jira'"
      >
        <ExternalLink :size="16" />Jira
      </button>
      <button
        :class="{ selected: view === 'library' }"
        @click="view = 'library'"
      >
        Historia i kopie
      </button>
    </nav>
    <div class="header-actions">
      <button
        class="today-button"
        aria-label="Szybkie przełączanie"
        title="Ctrl+K / Ctrl+Shift+Space"
        @click="emit('quick')"
      >
        ⇄
      </button>
      <span v-if="!desktop" class="preview-label">Podgląd</span>
      <button
        class="icon-button"
        :aria-label="theme === 'dark' ? 'Włącz tryb jasny' : 'Włącz tryb ciemny'"
        :title="theme === 'dark' ? 'Tryb jasny' : 'Tryb ciemny'"
        @click="theme = theme === 'dark' ? 'light' : 'dark'"
      >
        <Sun v-if="theme === 'dark'" :size="18" /><Moon v-else :size="18" />
      </button>
      <button
        v-if="desktop"
        class="icon-button"
        aria-label="Schowaj do traya"
        title="Schowaj do traya"
        @click="hide"
      >
        <ArrowDownToLine :size="18" />
      </button>
    </div>
  </header>
</template>
