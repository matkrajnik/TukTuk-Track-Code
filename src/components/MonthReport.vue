<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
import { duration, type Snapshot, type Task } from "../store";
import { dayBounds, dayKey } from "../time";
import { addToDays, closedDayTotals, decimalHours } from "../report";
import { holidays } from "../holidays";
import TargetBar from "./TargetBar.vue";
import {
  dayTarget,
  daysTarget,
  hhmm,
  isoWeekday,
  monthDays,
  targetHoliday,
  useSettings,
} from "../target";

/**
 * Raport miesięczny: suma względem celu miesiąca (dni robocze bez świąt),
 * tabele według projektu i zadania oraz wykres dni z linią celu.
 */
const props = defineProps<{ data: Snapshot; now: number }>();
const emit = defineEmits<{ open: [day: string] }>();
const settings = useSettings();

const monthOf = (day: string) => day.slice(0, 7);
const month = ref(monthOf(dayKey(props.now)));
const days = computed(() => monthDays(month.value));
const label = computed(() => {
  const [y, m] = month.value.split("-").map(Number);
  return new Intl.DateTimeFormat("pl-PL", {
    month: "long",
    year: "numeric",
  }).format(new Date(y, m - 1, 1));
});

function shiftMonth(offset: number) {
  const [y, m] = month.value.split("-").map(Number);
  month.value = monthOf(dayKey(new Date(y, m - 1 + offset, 1).getTime()));
}
function chooseMonth(event: Event) {
  const input = event.target as HTMLInputElement;
  if (/^\d{4}-\d{2}$/.test(input.value)) month.value = input.value;
  else input.value = month.value;
}

/** Zadania ze snapshotu (bez kosza) i trwający pomiar. */
const taskIds = computed(() => new Set(props.data.tasks.map((t) => t.id)));
const active = computed(() =>
  props.data.sessions.find(
    (s) => s.endedAt === null && taskIds.value.has(s.taskId),
  ),
);
const closedTotals = computed(() => closedDayTotals(props.data));
const activeTotals = computed(() => {
  const totals = new Map<string, number>();
  if (active.value) addToDays(totals, active.value.startedAt, props.now);
  return totals;
});
const totalOf = (day: string) =>
  (closedTotals.value.get(day) ?? 0) + (activeTotals.value.get(day) ?? 0);

const rows = computed(() =>
  days.value.map((day) => ({
    day,
    label: new Intl.DateTimeFormat("pl-PL", {
      weekday: "short",
      day: "numeric",
    }).format(dayBounds(day)[0]),
    total: totalOf(day),
    target: dayTarget(settings.value, day),
    holiday: targetHoliday(settings.value, day),
    weekend: isoWeekday(day) > 5,
  })),
);
const total = computed(() => rows.value.reduce((sum, d) => sum + d.total, 0));
const target = computed(() => daysTarget(settings.value, days.value));
const workDays = computed(() => rows.value.filter((d) => d.target > 0).length);
/** Święta w dniach tygodnia z ustawień, które obniżają cel miesiąca. */
const freeHolidays = computed(() =>
  settings.value.holidaysInTarget
    ? holidays(Number(month.value.slice(0, 4))).filter(
        (h) =>
          monthOf(h.date) === month.value &&
          settings.value.workDays.includes(isoWeekday(h.date)),
      )
    : [],
);
/** Odmiana: 1 dzień roboczy, 2–4 dni robocze, 5+ dni roboczych (12–14 też). */
const workDaysWord = computed(() => {
  const n = workDays.value;
  if (n === 1) return "dzień roboczy";
  const ones = n % 10;
  const tens = n % 100;
  return ones >= 2 && ones <= 4 && (tens < 12 || tens > 14)
    ? "dni robocze"
    : "dni roboczych";
});
const missing = computed(() => Math.max(0, target.value - total.value));
const over = computed(() => Math.max(0, total.value - target.value));
const maxDay = computed(() =>
  Math.max(60_000, ...rows.value.map((d) => Math.max(d.total, d.target))),
);
const dayMonth = (day: string) =>
  new Intl.DateTimeFormat("pl-PL", { day: "numeric", month: "short" }).format(
    dayBounds(day)[0],
  );

/** Czas zadań w miesiącu: przedziały przycięte do granic miesiąca (lokalne północe). */
const taskRows = computed(() => {
  const start = dayBounds(days.value[0])[0];
  const end = dayBounds(days.value[days.value.length - 1])[1];
  const sums = new Map<number, number>();
  for (const s of props.data.sessions) {
    if (!taskIds.value.has(s.taskId)) continue;
    const part =
      Math.min(s.endedAt ?? props.now, end) - Math.max(s.startedAt, start);
    if (part > 0) sums.set(s.taskId, (sums.get(s.taskId) ?? 0) + part);
  }
  return props.data.tasks
    .filter((t) => sums.has(t.id))
    .map((task: Task) => ({
      task,
      project: task.details?.project?.trim() ?? "",
      total: sums.get(task.id) ?? 0,
    }))
    .sort((a, b) => b.total - a.total || a.task.id - b.task.id);
});
const projectRows = computed(() => {
  const map = new Map<string, { project: string; total: number; tasks: number }>();
  for (const row of taskRows.value) {
    const key = row.project.toLocaleLowerCase("pl");
    const entry = map.get(key) ?? { project: row.project, total: 0, tasks: 0 };
    entry.total += row.total;
    entry.tasks += 1;
    map.set(key, entry);
  }
  return [...map.values()].sort(
    (a, b) => b.total - a.total || a.project.localeCompare(b.project, "pl"),
  );
});
const share = (part: number) =>
  total.value > 0 ? `${Math.round((part / total.value) * 100)}%` : "0%";
const running = computed(
  () =>
    !!active.value &&
    days.value.some((d) => (activeTotals.value.get(d) ?? 0) > 0),
);
</script>

<template>
  <section class="monthly-report" aria-label="Raport miesięczny">
    <div class="monthly-picker date-picker">
      <button
        class="icon-button"
        aria-label="Poprzedni miesiąc"
        @click="shiftMonth(-1)"
      >
        <ChevronLeft :size="18" /></button
      ><input
        type="month"
        aria-label="Miesiąc raportu"
        :value="month"
        @change="chooseMonth"
      /><button
        class="icon-button"
        aria-label="Następny miesiąc"
        @click="shiftMonth(1)"
      >
        <ChevronRight :size="18" /></button
      ><button class="today-button" @click="month = monthOf(dayKey(now))">
        Bieżący miesiąc
      </button>
    </div>

    <section class="report-summary monthly-summary" aria-label="Podsumowanie miesiąca">
      <div class="monthly-summary-text">
        <p class="monthly-label">{{ label }}</p>
        <h2>Przepracowany czas</h2>
        <dl class="monthly-split">
          <div>
            <dt>Przepracowane</dt>
            <dd class="mono">{{ hhmm(total) }}</dd>
          </div>
          <div>
            <dt>{{ over > 0 ? "Ponad cel" : "Do celu" }}</dt>
            <dd class="mono">{{ hhmm(over > 0 ? over : missing) }}</dd>
          </div>
          <div>
            <dt>Cel miesiąca</dt>
            <dd class="mono">{{ hhmm(target) }}</dd>
          </div>
        </dl>
        <TargetBar
          v-if="target > 0"
          :total="total"
          :target="target"
          label="Miesiąc względem celu"
        />
        <p class="report-note monthly-target-note">
          {{ workDays }} {{ workDaysWord }} ×
          {{ hhmm(settings.targetDayMinutes * 60_000) }}<template
            v-if="freeHolidays.length"
            >; bez świąt:
            <span
              v-for="(h, i) in freeHolidays"
              :key="h.date"
              class="monthly-holiday"
              >{{ dayMonth(h.date) }} {{ h.name
              }}{{ i < freeHolidays.length - 1 ? ", " : "" }}</span
            ></template
          >.
          <template v-if="running">
            Trwający pomiar jest uwzględniony do bieżącej chwili.</template
          >
        </p>
      </div>
      <strong class="mono report-total">{{ duration(total) }}</strong>
    </section>

    <div class="monthly-tables">
      <section class="dashboard-card" aria-label="Według projektu">
        <h2>Według projektu</h2>
        <table v-if="projectRows.length" class="estimate-table monthly-table">
          <thead>
            <tr>
              <th>Projekt</th>
              <th>Zadania</th>
              <th>Czas</th>
              <th>Udział</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in projectRows" :key="row.project">
              <td>{{ row.project || "Bez projektu" }}</td>
              <td class="mono">{{ row.tasks }}</td>
              <td class="mono">{{ hhmm(row.total) }}</td>
              <td class="mono">{{ share(row.total) }}</td>
            </tr>
          </tbody>
          <tfoot>
            <tr>
              <th>Razem</th>
              <td class="mono">{{ taskRows.length }}</td>
              <td class="mono">{{ hhmm(total) }}</td>
              <td></td>
            </tr>
          </tfoot>
        </table>
        <p v-else class="report-note">Brak zarejestrowanej pracy w tym miesiącu.</p>
      </section>
      <section class="dashboard-card" aria-label="Według zadania">
        <h2>Według zadania</h2>
        <table v-if="taskRows.length" class="estimate-table monthly-table">
          <thead>
            <tr>
              <th>Zadanie</th>
              <th>Klucz Jiry</th>
              <th>Projekt</th>
              <th>Czas</th>
              <th>Godziny</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in taskRows" :key="row.task.id">
              <td>{{ row.task.title }}</td>
              <td class="mono">{{ row.task.jiraKey ?? "" }}</td>
              <td>{{ row.project }}</td>
              <td class="mono">{{ hhmm(row.total) }}</td>
              <td class="mono">{{ decimalHours(row.total) }}</td>
            </tr>
          </tbody>
          <tfoot>
            <tr>
              <th>Razem</th>
              <td></td>
              <td></td>
              <td class="mono">{{ hhmm(total) }}</td>
              <td class="mono">{{ decimalHours(total) }}</td>
            </tr>
          </tfoot>
        </table>
        <p v-else class="report-note">Brak zarejestrowanej pracy w tym miesiącu.</p>
      </section>
    </div>

    <section class="dashboard-card" aria-label="Czas pracy w dniach miesiąca">
      <h2>Dni miesiąca</h2>
      <p class="report-note">
        Linia oznacza cel dnia. Wybierz dzień, aby otworzyć jego raport.
      </p>
      <button
        v-for="d in rows"
        :key="d.day"
        class="monthly-day"
        :class="{ free: !d.target, holiday: !!d.holiday }"
        :aria-label="'Raport ' + d.day + ': ' + duration(d.total)"
        :title="d.holiday ?? undefined"
        @click="emit('open', d.day)"
      >
        <span class="monthly-day-label"
          >{{ d.label
          }}<span v-if="d.holiday" class="monthly-day-holiday">
            · {{ d.holiday }}</span
          ></span
        ><span class="dashboard-bar"
          ><span
            :class="{ over: d.target > 0 && d.total > d.target }"
            :style="{ width: `${(d.total / maxDay) * 100}%` }" /><span
            v-if="d.target"
            class="target-line"
            :style="{ left: `${(d.target / maxDay) * 100}%` }"
        /></span
        ><span class="mono" :class="{ 'dashboard-zero': !d.total }">{{
          hhmm(d.total)
        }}</span>
      </button>
    </section>
  </section>
</template>
