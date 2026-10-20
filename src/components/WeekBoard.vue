<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import {
  detailsOf,
  meetingTitle,
  planTask,
  planTasks,
  type PlanEntry,
  type Snapshot,
  type Task,
} from "../store";
import { dayBounds, dayKey, dayText, inDay } from "../time";
import { issueRef } from "../issue";
import { compareTasks } from "../order";
import { plannedTasks } from "../dayplan";
import { dayTarget, hhmm, hm, targetHoliday, useSettings } from "../target";
import { dayCapacity } from "../capacity";
import {
  correctionTitle,
  corrections,
  plannedMs,
  usuallyText,
} from "../estimates";
import { spread, type SpreadResult } from "../spread";
import { useMeetings } from "../composables/useMeetings";
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
/** Spotkania widocznego tygodnia (pojemność dni). */
const calendar = useMeetings(() => {
  const monday = new Date(dayBounds(props.day)[0]);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  const next = new Date(monday);
  next.setDate(monday.getDate() + 7);
  return [+monday, +next];
});
const busy = ref(false);
const error = ref("");
const search = ref("");
/** Mnożniki poprawki szacunków; liczone raz na minutę, nie co sekundę. */
const minute = computed(() => Math.floor(props.now / 60_000) * 60_000);
const correctionOf = computed(() => corrections(props.data, minute.value));
/** Czas zadania w planie: szacunek, z poprawką przy „Planuj z poprawką”. */
const planned = (task: Task) =>
  plannedMs(
    task,
    settings.value.planWithCorrection ? correctionOf.value : undefined,
  );

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
    const estimate = tasks.reduce((sum, t) => sum + planned(t), 0);
    const goal = dayTarget(settings.value, key);
    // Pojemność dnia: cel minus spotkania; urlop całodniowy = dzień wolny.
    const cap = dayCapacity(goal, calendar.events.value, key);
    const target = cap.capacity;
    const meetingsWidth =
      goal > 0 && !cap.off ? Math.min(100, (cap.meetings / goal) * 100) : 0;
    const scale = goal > 0 && !cap.off ? goal : target;
    return {
      key,
      short: shortNames[i],
      long: longNames[i],
      date: `${key.slice(8, 10)}.${key.slice(5, 7)}`,
      tasks,
      estimate,
      target,
      holiday: targetHoliday(settings.value, key),
      off: cap.off
        ? meetingTitle(cap.off, settings.value.calendarShowTitles)
        : null,
      free: cap.off !== null || goal <= 0,
      capacity: cap.off || goal <= 0 ? "wolne" : hhmm(target),
      meetings: cap.meetings,
      meetingsWidth,
      // Podpowiedź przy spotkaniach: „Cel 8:00 · spotkania 2:30 · zostaje 5:30”.
      capacityTitle: cap.off
        ? `Poza biurem: ${meetingTitle(cap.off, settings.value.calendarShowTitles)} · dzień wolny`
        : cap.meetings > 0 && goal > 0
          ? `Cel ${hm(goal)} · spotkania ${hm(cap.meetings)} · zostaje ${hm(target)}`
          : "",
      // Szacunki względem pojemności (aria-valuenow).
      load:
        target > 0
          ? Math.min(100, (estimate / target) * 100)
          : estimate
            ? 100
            : 0,
      // Pasek: cała długość to cel dnia; spotkania z lewej, szacunki za nimi.
      width:
        scale > 0
          ? Math.min(100 - meetingsWidth, (estimate / scale) * 100)
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
    [task.title, task.details?.project ?? "", issueRef(task)?.key ?? ""].some(
      (text) => text.toLocaleLowerCase("pl").includes(query),
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

/** Podpowiedź „zwykle ~…” dla karty z szacunkiem (bez danych pusta). */
function usual(task: Task) {
  const c = correctionOf.value(task.details?.project);
  return {
    text: usuallyText(task.details?.estimateMinutes ?? 0, c),
    title: correctionTitle(c),
  };
}

// „Rozłóż”: podgląd zachłannego planu od dziś do końca widocznego tygodnia.
// Nic nie zapisuje się bez „Zastosuj”. Podgląd liczy się na bieżąco z danych
// (`computed`), więc zmiana w trakcie (zakończenie, przeniesienie, nowy
// szacunek, `data-changed` z innego okna) od razu go odświeża, a „Zastosuj”
// zapisuje dokładnie to, co widać.
const spreadOpen = ref(false);
const today = computed(() => dayKey(props.now));
/**
 * Dni widocznego tygodnia od dziś (tydzień w przeszłości nie ma takich dni,
 * tydzień w przyszłości ma wszystkie). Reszta bieżącego tygodnia należy do
 * jego własnej tablicy.
 */
const spreadDays = computed(() =>
  days.value.filter((d) => d.key >= today.value),
);
const preview = computed<SpreadResult | null>(() => {
  if (!spreadOpen.value) return null;
  // Kandydaci: „Do zaplanowania” i zaległe naprawdę, czyli zaplanowane przed
  // dziś (także na minione dni bieżącego tygodnia). Zadania zaplanowane na
  // dziś i później zostają tam, gdzie są, także przy tygodniu w przyszłości.
  const candidates = [
    ...backlogAll.value,
    ...props.data.tasks.filter(
      (t) =>
        isOpen(t) &&
        !!t.details?.plannedDay &&
        t.details.plannedDay < today.value,
    ),
  ];
  return spread(
    candidates,
    spreadDays.value.map((d) => ({
      day: d.key,
      capacity: d.free ? 0 : d.target,
      load: d.estimate,
    })),
    planned,
    today.value,
  );
});
function openSpread() {
  error.value = "";
  spreadOpen.value = true;
}
const dayInfo = (key: string) => days.value.find((d) => d.key === key)!;
watch(
  () => props.day,
  () => (spreadOpen.value = false),
);

/**
 * Kolejność dołożonych zadań: na koniec pasma priorytetu w dniu, jak przy
 * przeniesieniu; kolejne zadania z tego samego pasma stają za poprzednimi.
 */
function spreadEntries(result: SpreadResult): PlanEntry[] {
  const added = new Map<string, Task[]>();
  return result.placed.map(({ task, day }) => {
    const priority = task.details?.priority ?? 0;
    const column = [...plannedTasks(props.data.tasks, day), ...(added.get(day) ?? [])];
    const band = column.filter(
      (t) => t.id !== task.id && (t.details?.priority ?? 0) === priority,
    );
    const order = Math.min(
      1_000_000,
      Math.max(
        0,
        band.length
          ? Math.max(...band.map((t) => t.details?.order ?? 0)) + 1
          : (task.details?.order ?? 0),
      ),
    );
    added.set(day, [
      ...(added.get(day) ?? []),
      { ...task, details: { ...detailsOf(task), priority, order } },
    ]);
    return { id: task.id, day, order };
  });
}
async function applySpread() {
  if (!preview.value || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    emit("updated", await planTasks(spreadEntries(preview.value)));
    spreadOpen.value = false;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
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
/**
 * Komunikat dla czytnika ekranu po przeniesieniu karty (Atlassian: ogłoszenie
 * w regionie na żywo, skąd i dokąd; WCAG 4.1.3).
 */
const moved = ref("");
const columnName = (day: string) => (day ? dayText(day) : "Do zaplanowania");
async function moveTo(task: Task, day: string, focus = false) {
  const from = task.details?.plannedDay ?? "";
  if (from === day || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    emit("updated", await planTask(task.id, day, appendOrder(task, day)));
    moved.value = `„${task.title}” przeniesione z: ${columnName(from)} na: ${columnName(day)}.`;
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
    <p v-if="moved" class="sr-only" role="status">{{ moved }}</p>
    <div class="week-board-head">
      <p class="hint">
        {{ dayText(weekStart) }} — {{ dayText(weekEnd) }} · Przeciągnij kartę za uchwyt ⋮⋮ do innego
        dnia albo użyj „Przenieś na…”.
      </p>
      <button
        type="button"
        class="secondary-button"
        :disabled="busy || !spreadDays.length"
        :title="
          spreadDays.length
            ? 'Rozłóż zadania z „Do zaplanowania” i zaległe (zaplanowane przed dziś) na dni od dziś do końca tygodnia (podgląd przed zapisem)'
            : 'Ten tydzień już minął'
        "
        :aria-expanded="!!preview"
        @click="openSpread"
      >
        Rozłóż
      </button>
    </div>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <section
      v-if="preview"
      class="spread-preview dashboard-card"
      aria-label="Podgląd: Rozłóż"
    >
      <h3>Podgląd: Rozłóż</h3>
      <p class="hint">
        Zadania z „Do zaplanowania” i zaległe (zaplanowane przed dziś) według
        terminu, priorytetu i kolejności, na dni od dziś do końca tygodnia<template
          v-if="settings.planWithCorrection"
        >
          (szacunki z poprawką)</template
        >; bez szacunku 30 min. Nic nie zmieni się bez „Zastosuj”.
        <template v-if="preview.skipped.length">
          Pominięte (czekam na, cykliczne): {{ preview.skipped.length }}.
        </template>
      </p>
      <ul class="spread-days">
        <li
          v-for="d in preview.days"
          :key="d.day"
          :aria-label="'Rozłóż: ' + dayInfo(d.day).long + ' ' + d.day"
        >
          <div class="spread-day-head">
            <strong
              >{{ dayInfo(d.day).short }}
              <span class="mono">{{ dayInfo(d.day).date }}</span></strong
            >
            <span
              v-if="d.capacity > 0"
              class="mono"
              :title="'Przed / po rozłożeniu względem pojemności dnia'"
              >{{ hhmm(d.load) }} → {{ hhmm(d.after) }} /
              {{ hhmm(d.capacity) }}</span
            >
            <span v-else class="hint">wolne</span>
          </div>
          <ul v-if="d.tasks.length" class="spread-tasks">
            <li v-for="p in d.tasks" :key="p.task.id">
              {{ p.task.title }}
              <span class="mono hint">{{ hhmm(p.duration) }}</span>
            </li>
          </ul>
          <p v-else-if="d.capacity > 0" class="hint">Bez nowych zadań.</p>
        </li>
      </ul>
      <section
        v-if="preview.missed.length"
        class="spread-missed"
        aria-label="Nie zmieściło się"
      >
        <h4>Nie zmieściło się</h4>
        <ul>
          <li v-for="m in preview.missed" :key="m.task.id">
            {{ m.task.title }}
            <span class="mono hint">{{ hhmm(m.duration) }}</span> —
            {{ m.reason }}
          </li>
        </ul>
      </section>
      <p v-if="!preview.placed.length && !preview.missed.length" class="hint">
        Nie ma czego rozłożyć.
      </p>
      <div class="spread-actions">
        <button
          type="button"
          class="primary-button"
          :disabled="busy || !preview.placed.length"
          @click="applySpread"
        >
          Zastosuj
        </button>
        <button
          type="button"
          class="secondary-button"
          :disabled="busy"
          @click="spreadOpen = false"
        >
          Anuluj
        </button>
      </div>
    </section>
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
              :usual="usual(task)"
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
              :usual="usual(task)"
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
        :class="{ today: d.today, free: d.free }"
        :aria-label="d.long + ' ' + d.key"
        v-bind="columnProps(d.key, [d.key], d.key)"
      >
        <header class="week-column-head">
          <h3>
            {{ d.short }} <span class="mono">{{ d.date }}</span>
          </h3>
          <span v-if="d.holiday" class="week-holiday" :title="d.holiday"
            >{{ d.holiday }}</span
          >
          <span v-else-if="d.off" class="week-holiday" :title="d.capacityTitle"
            >{{ d.off }}</span
          >
          <span
            class="week-estimate mono"
            :title="
              d.capacityTitle || 'Suma szacunków względem pojemności dnia'
            "
            >{{ hhmm(d.estimate) }} / {{ d.capacity }}</span
          >
        </header>
        <div
          class="week-capacity"
          :class="d.level"
          role="progressbar"
          :aria-label="'Pojemność: ' + d.long + ' ' + d.key"
          aria-valuemin="0"
          aria-valuemax="100"
          :aria-valuenow="Math.round(d.load)"
          :aria-valuetext="hhmm(d.estimate) + ' / ' + d.capacity"
          :aria-description="d.capacityTitle || undefined"
          :title="d.capacityTitle || undefined"
        >
          <span
            v-if="d.meetingsWidth > 0"
            class="week-capacity-meetings"
            :style="{ width: d.meetingsWidth + '%' }"
          ></span
          ><span :style="{ width: d.width + '%' }"></span>
        </div>
        <p v-if="d.meetings > 0 && !d.off" class="week-meetings hint">
          Spotkania <span class="mono">{{ hhmm(d.meetings) }}</span>
        </p>
        <p class="week-actual hint">
          Praca <span class="mono">{{ hhmm(d.actual) }}</span>
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
              :usual="usual(task)"
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
