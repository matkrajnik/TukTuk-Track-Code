<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { planTask, type Snapshot, type Task } from "../store";
import { dayBounds, dayKey, inDay } from "../time";
import { compareTasks } from "../order";
import { plannedTasks } from "../dayplan";
import { dayTarget, hm, useSettings } from "../target";
import { useReorder } from "../composables/useReorder";
import WeekCard from "./WeekCard.vue";

/**
 * Tablica tygodnia Pon–Nd: kolumna „Do zaplanowania” i siedem dni.
 * Przeniesienie między kolumnami (przeciąganie albo menu „Przenieś na…”)
 * woła `planTask`, który zmienia tylko `plannedDay` i `order`; kolejność
 * w obrębie kolumny zmienia `reorderTasks`, tak jak w planie dnia.
 */
const props = defineProps<{ data: Snapshot; now: number; day: string }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const settings = useSettings();
const busy = ref(false);
const error = ref("");
const search = ref("");

const shortNames = ["Pon", "Wt", "Śr", "Czw", "Pt", "Sob", "Nd"];
const longNames = [
  "Poniedziałek",
  "Wtorek",
  "Środa",
  "Czwartek",
  "Piątek",
  "Sobota",
  "Niedziela",
];
/** Identyfikatory list dla przeciągania; dni mają za identyfikator swój klucz. */
const BACKLOG = "backlog";
const OVERDUE = "overdue";

/** Obciążenie dnia: zielony < 90%, bursztynowy 90–100%, czerwony > 100%. */
function level(estimate: number, target: number) {
  if (target <= 0) return estimate > 0 ? "over" : "ok";
  const load = estimate / target;
  return load > 1 ? "over" : load >= 0.9 ? "near" : "ok";
}

const days = computed(() => {
  const monday = new Date(dayBounds(props.day)[0]);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  const today = dayKey(props.now);
  return Array.from({ length: 7 }, (_, i) => {
    const date = new Date(monday);
    date.setDate(monday.getDate() + i);
    const key = dayKey(+date);
    const tasks = plannedTasks(props.data.tasks, key);
    const estimate = tasks.reduce(
      (sum, t) => sum + (t.details?.estimateMinutes ?? 0) * 60_000,
      0,
    );
    const target = dayTarget(settings.value, key);
    return {
      key,
      short: shortNames[i],
      long: longNames[i],
      date: `${key.slice(8, 10)}.${key.slice(5, 7)}`,
      tasks,
      estimate,
      target,
      capacity: target ? hm(target) : "wolne",
      width:
        target > 0
          ? Math.min(100, (estimate / target) * 100)
          : estimate
            ? 100
            : 0,
      level: level(estimate, target),
      actual: props.data.sessions.reduce(
        (sum, s) => sum + inDay(s, key, props.now),
        0,
      ),
      today: key === today,
    };
  });
});
const weekStart = computed(() => days.value[0].key);
const weekEnd = computed(() => days.value[6].key);
const nextMonday = computed(() => dayKey(dayBounds(weekEnd.value)[1]));
const menuDays = computed(() =>
  days.value.map((d) => ({ key: d.key, long: d.long })),
);

function matches(task: Task) {
  const query = search.value.trim().toLocaleLowerCase("pl");
  return (
    !query ||
    [task.title, task.details?.project ?? "", task.jiraKey ?? ""].some((text) =>
      text.toLocaleLowerCase("pl").includes(query),
    )
  );
}
const isOpen = (t: Task) => !t.archived && !t.done;
/** Wszystkie zadania bez planu (bez filtra wyszukiwania), do liczenia kolejności. */
const backlogAll = computed(() =>
  props.data.tasks
    .filter((t) => isOpen(t) && !t.details?.plannedDay)
    .sort(compareTasks()),
);
const backlog = computed(() => backlogAll.value.filter(matches));
/** Zaległe: otwarte zadania zaplanowane przed tym tygodniem. */
const overdue = computed(() =>
  props.data.tasks
    .filter(
      (t) =>
        isOpen(t) &&
        !!t.details?.plannedDay &&
        t.details.plannedDay < weekStart.value,
    )
    .sort(
      (a, b) =>
        a.details!.plannedDay.localeCompare(b.details!.plannedDay) ||
        compareTasks()(a, b),
    )
    .filter(matches),
);

const spent = computed(() => {
  const map = new Map<number, number>();
  props.data.sessions.forEach((s) =>
    map.set(
      s.taskId,
      (map.get(s.taskId) ?? 0) +
        Math.max(0, (s.endedAt ?? props.now) - s.startedAt),
    ),
  );
  return map;
});
function percent(task: Task) {
  const estimate = (task.details?.estimateMinutes ?? 0) * 60_000;
  return estimate > 0
    ? Math.round(((spent.value.get(task.id) ?? 0) / estimate) * 100)
    : null;
}

const reorder = useReorder({
  scope: "week",
  all: () => props.data.tasks,
  activeTaskId: () => null,
  busy,
  updated: (data) => emit("updated", data),
  error: (message) => (error.value = message),
});
/** Zakończone stoją pod niezakończonymi, więc przesuwa się w obrębie jednej części. */
const part = (tasks: Task[], task: Task) =>
  tasks.filter((t) => t.done === task.done);

/**
 * Kolejność po przeniesieniu do kolumny: na koniec pasma priorytetu w tej
 * kolumnie; w pustym paśmie zadanie zachowuje swoją kolejność.
 */
function appendOrder(task: Task, day: string): number {
  const column = day ? plannedTasks(props.data.tasks, day) : backlogAll.value;
  const priority = task.details?.priority ?? 0;
  const band = column.filter(
    (t) => t.id !== task.id && (t.details?.priority ?? 0) === priority,
  );
  const order = band.length
    ? Math.max(...band.map((t) => t.details?.order ?? 0)) + 1
    : (task.details?.order ?? 0);
  return Math.min(1_000_000, Math.max(0, order));
}

/**
 * `focus`: po przeniesieniu z menu karta trafia do innej kolumny (nowa
 * instancja komponentu), więc fokus wraca na jej przycisk „Przenieś na…”.
 */
async function moveTo(task: Task, day: string, focus = false) {
  if ((task.details?.plannedDay ?? "") === day || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    emit("updated", await planTask(task.id, day, appendOrder(task, day)));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
  if (!focus) return;
  await nextTick();
  document
    .querySelector<HTMLElement>(`[data-week-move="${task.id}"]`)
    ?.focus();
}

// Przeciąganie między kolumnami. Upuszczenie na kartę tej samej listy
// obsługuje `useReorder` (kolejność); każde inne trafia do kolumny.
const dragging = ref<{ task: Task; list: string } | null>(null);
const overColumn = ref("");
function dragStart(event: DragEvent, task: Task, list: string) {
  reorder.start(event, task.id, list);
  if (!event.defaultPrevented) dragging.value = { task, list };
}
function dragEnd() {
  reorder.end();
  dragging.value = null;
  overColumn.value = "";
}
/**
 * `lists`: listy należące do kolumny. Zaległe stoją w kolumnie Do
 * zaplanowania, ale to osobna lista: upuszczenie zaległej karty na resztę
 * kolumny zdejmuje plan, a w obrębie sekcji „Zaległe” nic nie zmienia.
 */
function columnProps(id: string, lists: string[], day: string) {
  const accepts = (event: DragEvent) =>
    !!dragging.value &&
    !lists.includes(dragging.value.list) &&
    (dragging.value.task.details?.plannedDay ?? "") !== day &&
    !(
      dragging.value.list === OVERDUE &&
      (event.target as Element | null)?.closest?.(".week-overdue")
    );
  return {
    class: { "drop-column": overColumn.value === id },
    onDragover: (event: DragEvent) => {
      if (!accepts(event)) {
        if (overColumn.value === id) overColumn.value = "";
        return;
      }
      event.preventDefault();
      if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
      overColumn.value = id;
    },
    onDragleave: (event: DragEvent) => {
      const to = event.relatedTarget as Node | null;
      if (
        overColumn.value === id &&
        !(event.currentTarget as HTMLElement).contains(to)
      )
        overColumn.value = "";
    },
    onDrop: (event: DragEvent) => {
      if (!accepts(event)) return;
      event.preventDefault();
      const task = dragging.value!.task;
      dragEnd();
      void moveTo(task, day);
    },
  };
}
</script>

<template>
  <section class="week-board-wrap" aria-label="Tablica tygodnia">
    <p class="hint">
      {{ weekStart }} — {{ weekEnd }} · Przeciągnij kartę za uchwyt ⋮⋮ do innego
      dnia albo użyj „Przenieś na…”.
    </p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <div class="week-board">
      <section
        class="week-column week-backlog"
        aria-label="Do zaplanowania"
        v-bind="columnProps(BACKLOG, [BACKLOG], '')"
      >
        <header class="week-column-head">
          <h3>Do zaplanowania</h3>
          <span class="hint mono">{{ backlogAll.length }}</span>
        </header>
        <input
          v-model="search"
          class="week-search"
          type="search"
          placeholder="Szukaj…"
          aria-label="Szukaj w Do zaplanowania"
        />
        <ul class="week-cards">
          <li
            v-for="task in backlog"
            :key="task.id"
            v-bind="reorder.dropProps(backlog, task, BACKLOG)"
          >
            <WeekCard
              :task="task"
              :days="menuDays"
              :next-monday="nextMonday"
              :percent="percent(task)"
              :busy="busy"
              @move="moveTo(task, $event, true)"
              @reorder="reorder.move(backlog, task.id, $event)"
              @dragstart="dragStart($event, task, BACKLOG)"
              @dragend="dragEnd"
            />
          </li>
        </ul>
        <details v-if="overdue.length" class="week-overdue" open>
          <summary>Zaległe ({{ overdue.length }})</summary>
          <ul class="week-cards">
            <li v-for="task in overdue" :key="task.id">
              <WeekCard
                :task="task"
                :days="menuDays"
                :next-monday="nextMonday"
                :percent="percent(task)"
                :busy="busy"
                show-planned-day
                @move="moveTo(task, $event, true)"
                @dragstart="dragStart($event, task, OVERDUE)"
                @dragend="dragEnd"
              />
            </li>
          </ul>
        </details>
        <p v-if="!backlog.length && !overdue.length" class="report-note">
          {{ search ? "Brak pasujących zadań." : "Wszystko zaplanowane." }}
        </p>
      </section>
      <section
        v-for="d in days"
        :key="d.key"
        class="week-column"
        :class="{ today: d.today, free: !d.target }"
        :aria-label="d.long + ' ' + d.key"
        v-bind="columnProps(d.key, [d.key], d.key)"
      >
        <header class="week-column-head">
          <h3>
            {{ d.short }} <span class="mono">{{ d.date }}</span>
          </h3>
          <span
            class="week-estimate mono"
            title="Suma szacunków względem pojemności dnia"
            >{{ hm(d.estimate) }} / {{ d.capacity }}</span
          >
        </header>
        <div
          class="week-capacity"
          :class="d.level"
          role="progressbar"
          :aria-label="'Pojemność: ' + d.long + ' ' + d.key"
          aria-valuemin="0"
          aria-valuemax="100"
          :aria-valuenow="Math.round(d.width)"
          :aria-valuetext="hm(d.estimate) + ' / ' + d.capacity"
        >
          <span :style="{ width: d.width + '%' }"></span>
        </div>
        <p class="week-actual hint">
          Praca <span class="mono">{{ hm(d.actual) }}</span>
        </p>
        <ul class="week-cards">
          <li
            v-for="task in d.tasks"
            :key="task.id"
            v-bind="reorder.dropProps(part(d.tasks, task), task, d.key)"
          >
            <WeekCard
              :task="task"
              :days="menuDays"
              :next-monday="nextMonday"
              :percent="percent(task)"
              :busy="busy"
              @move="moveTo(task, $event, true)"
              @reorder="reorder.move(part(d.tasks, task), task.id, $event)"
              @dragstart="dragStart($event, task, d.key)"
              @dragend="dragEnd"
            />
          </li>
        </ul>
      </section>
    </div>
  </section>
</template>
