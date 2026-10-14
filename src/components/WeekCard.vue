<script setup lang="ts">
import { nextTick, ref } from "vue";
import type { Task } from "../store";
import { hm } from "../target";
import ReorderHandle from "./ReorderHandle.vue";

/**
 * Karta zadania na tablicy tygodnia: tytuł, projekt, szacunek z procentem
 * wykonania, znacznik Jiry, status oraz menu „Przenieś na…” (alternatywa
 * przeciągania, używana też w testach).
 */
const props = defineProps<{
  task: Task;
  days: { key: string; long: string }[];
  nextMonday: string;
  percent: number | null;
  busy: boolean;
  /** Pokazuje dzień planu (zaległe plany sprzed tygodnia). */
  showPlannedDay?: boolean;
}>();
const emit = defineEmits<{
  move: [day: string];
  reorder: [delta: -1 | 1];
  dragstart: [event: DragEvent];
  dragend: [];
}>();
const open = ref(false);
const menu = ref<HTMLElement | null>(null);
const toggle = ref<HTMLButtonElement | null>(null);
const current = () => props.task.details?.plannedDay ?? "";

const items = () => [
  ...(menu.value?.querySelectorAll<HTMLElement>(
    '[role="menuitem"]:not([disabled])',
  ) ?? []),
];
async function openMenu() {
  open.value = !open.value;
  if (!open.value) return;
  await nextTick();
  items()[0]?.focus();
}
function close(focusButton: boolean) {
  open.value = false;
  if (focusButton) toggle.value?.focus();
}
function choose(day: string) {
  close(true);
  emit("move", day);
}
function focusOut(event: FocusEvent) {
  const to = event.relatedTarget as Node | null;
  if (open.value && to && !(event.currentTarget as HTMLElement).contains(to))
    open.value = false;
}
function arrows(event: KeyboardEvent) {
  const list = items();
  if (!list.length) return;
  const index = list.indexOf(document.activeElement as HTMLElement);
  const step = event.key === "ArrowDown" ? 1 : -1;
  list[(index + step + list.length) % list.length].focus();
}
</script>

<template>
  <article
    class="week-card"
    :class="{ done: task.done }"
    :aria-label="task.title"
  >
    <div class="week-card-top">
      <ReorderHandle
        :title="task.title"
        :task-id="task.id"
        scope="week"
        :disabled="busy"
        @move="emit('reorder', $event)"
        @dragstart="emit('dragstart', $event)"
        @dragend="emit('dragend')"
      />
      <strong class="week-card-title">{{ task.title }}</strong>
    </div>
    <div class="week-card-meta">
      <span v-if="showPlannedDay" class="task-status"
        >Plan {{ task.details?.plannedDay }}</span
      >
      <span v-if="task.details?.project">{{ task.details.project }}</span>
      <span v-if="task.jiraKey" class="task-jira-key mono">{{
        task.jiraKey
      }}</span>
      <span
        v-if="task.details?.estimateMinutes"
        class="mono"
        :class="{ 'week-over-estimate': (percent ?? 0) > 100 }"
        >{{ hm(task.details.estimateMinutes * 60_000) }} · {{ percent }}%</span
      >
      <span v-if="task.done" class="task-status">Ukończone</span>
      <span v-else-if="task.details?.waiting" class="task-status"
        >Czekam na: {{ task.details.waiting }}</span
      >
    </div>
    <div class="week-move" @focusout="focusOut">
      <button
        ref="toggle"
        class="today-button"
        type="button"
        :data-week-move="task.id"
        aria-haspopup="menu"
        :aria-expanded="open"
        :aria-label="'Przenieś na…: ' + task.title"
        :disabled="busy"
        @click="openMenu"
      >
        Przenieś na…
      </button>
      <div
        v-if="open"
        ref="menu"
        class="week-menu"
        role="menu"
        :aria-label="'Przenieś na: ' + task.title"
        @keydown.esc.prevent="close(true)"
        @keydown.down.prevent="arrows"
        @keydown.up.prevent="arrows"
      >
        <button
          v-for="d in days"
          :key="d.key"
          role="menuitem"
          type="button"
          :disabled="d.key === current()"
          @click="choose(d.key)"
        >
          {{ d.long }} {{ d.key }}
        </button>
        <button
          role="menuitem"
          type="button"
          :disabled="nextMonday === current()"
          @click="choose(nextMonday)"
        >
          Następny poniedziałek {{ nextMonday }}
        </button>
        <button
          v-if="current()"
          role="menuitem"
          type="button"
          @click="choose('')"
        >
          Do zaplanowania
        </button>
      </div>
    </div>
  </article>
</template>
