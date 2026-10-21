<script setup lang="ts">
import { computed } from "vue";
import { duration, meetingTitle, type Snapshot } from "./store";
import { dayKey, dayBounds, inDay } from "./time";
import { compareTasks } from "./order";
import { atRisk, percent } from "./estimates";
import TargetBar from "./components/TargetBar.vue";
import EstimateAccuracy from "./components/EstimateAccuracy.vue";
import AgentsCard from "./components/AgentsCard.vue";
import { useFocus, useFocusBlocks } from "./focus";
import {
  averageDistraction,
  dayDistraction,
  dayTaskTotals,
  oneDecimal,
  overtimeOf,
} from "./report";
import { previousWorkDay } from "./review";
import { openSettings, openView } from "./navigation";
import { dayGoal, offDayEvent } from "./capacity";
import { useMeetings } from "./composables/useMeetings";
import {
  daysTarget,
  dayTarget,
  hhmm,
  hm,
  targetHoliday,
  useSettings,
  weekDays,
} from "./target";

const props = defineProps<{ data: Snapshot; now: number }>();
defineEmits<{ report: [day: string] }>();
const settings = useSettings();
/** Ukończone bloki skupienia dziś i w tym tygodniu. */
const focus = useFocus();
const today = computed(() => dayKey(props.now));
/**
 * Część zamknięta (B6): czas zamkniętych przedziałów według dnia i zadania,
 * liczony raz na zmianę danych. Co sekundę dochodzi tylko trwający pomiar.
 */
const active = computed(() =>
  props.data.sessions.find((s) => s.endedAt === null),
);
const closedByDay = computed(() =>
  dayTaskTotals(
    props.data.sessions.filter((s) => s.endedAt !== null),
    0,
  ),
);
const closedDayTotals = computed(
  () =>
    new Map(
      [...closedByDay.value].map(([day, tasks]) => [
        day,
        [...tasks.values()].reduce((sum, ms) => sum + ms, 0),
      ]),
    ),
);
const activeIn = (key: string) =>
  active.value ? inDay(active.value, key, props.now) : 0;
const dayTotal = (key: string) =>
  (closedDayTotals.value.get(key) ?? 0) + activeIn(key);
/**
 * Chwila dla statystyk zmieniających się wolno (rozproszenie, zagrożone
 * zadania): pełna minuta, nie wcześniej niż start trwającego pomiaru, więc
 * zależne wyliczenia liczą się raz na minutę albo przy zmianie danych (B6).
 */
const minuteNow = computed(() =>
  Math.max(
    Math.floor(props.now / 60_000) * 60_000,
    active.value?.startedAt ?? 0,
  ),
);
/** Spotkania dziś pomniejszają cel dnia (bez tych już zapisanych jako czas). */
const calendar = useMeetings(() => dayBounds(today.value));
const todayTarget = computed(() =>
  dayGoal(
    dayTarget(settings.value, today.value),
    calendar.events.value,
    props.data.sessions,
    today.value,
    props.now,
  ),
);
const todayOff = computed(() => offDayEvent(calendar.events.value, today.value));
/** Bieżący tydzień kalendarzowy, poniedziałek–niedziela. */
const thisWeek = computed(() => weekDays(today.value));
const thisWeekTotal = computed(() =>
  thisWeek.value.reduce((sum, key) => sum + dayTotal(key), 0),
);
const thisWeekTarget = computed(() =>
  daysTarget(settings.value, thisWeek.value),
);
const days = computed(() =>
  Array.from({ length: 7 }, (_, index) => {
    const date = new Date(dayBounds(today.value)[0]);
    date.setDate(date.getDate() - 6 + index);
    const key = dayKey(+date);
    return {
      key,
      label: new Intl.DateTimeFormat("pl-PL", {
        weekday: "short",
        day: "numeric",
        month: "short",
      }).format(date),
      total: dayTotal(key),
      target: dayTarget(settings.value, key),
      /** Nadgodziny (B5): praca ponad normę dnia, w dzień wolny cała praca. */
      overtime: overtimeOf(dayTotal(key), key, settings.value),
    };
  }),
);
const weekTotal = computed(() =>
  days.value.reduce((sum, day) => sum + day.total, 0),
);
const weekOvertime = computed(() =>
  days.value.reduce((sum, day) => sum + day.overtime, 0),
);
const workedDays = computed(
  () => days.value.filter((day) => day.total >= 60_000).length,
);
const risky = computed(() => atRisk(props.data, minuteNow.value));
const maxDay = computed(() =>
  Math.max(1, ...days.value.map((day) => Math.max(day.total, day.target))),
);
/** Początek bieżącego miesiąca (zmienia się z dniem, nie co sekundę). */
const monthStart = computed(() => {
  const [year, month] = today.value.split("-").map(Number);
  return +new Date(year, month - 1, 1);
});
const closedMonth = computed(() =>
  props.data.sessions.reduce(
    (sum, s) =>
      s.endedAt === null
        ? sum
        : sum + Math.max(0, s.endedAt - Math.max(s.startedAt, monthStart.value)),
    0,
  ),
);
const monthTotal = computed(
  () =>
    closedMonth.value +
    (active.value
      ? Math.max(
          0,
          props.now - Math.max(active.value.startedAt, monthStart.value),
        )
      : 0),
);
const counts = computed(() => {
  const started = new Set(props.data.sessions.map((s) => s.taskId));
  return [
    {
      label: "Rozpoczęte",
      count: props.data.tasks.filter(
        (t) => !t.archived && !t.done && !t.details?.waiting && started.has(t.id),
      ).length,
    },
    {
      label: "Nierozpoczęte",
      count: props.data.tasks.filter(
        (t) => !t.archived && !t.done && !t.details?.waiting && !started.has(t.id),
      ).length,
    },
    {
      label: "Ukończone",
      count: props.data.tasks.filter((t) => t.done).length,
    },
    {
      label: "Czekam na…",
      count: props.data.tasks.filter((t) => !t.done && t.details?.waiting)
        .length,
    },
  ];
});
/** Rozproszenie (W2): dziś i średnia z 7 poprzednich dni roboczych. */
const distractionDays = computed(() => {
  const list: string[] = [];
  let cursor = today.value;
  for (let i = 0; i < 7; i++) list.push((cursor = previousWorkDay(settings.value, cursor)));
  return list;
});
const { blocks: focusBlocks } = useFocusBlocks(() => [
  dayBounds(distractionDays.value[distractionDays.value.length - 1])[0],
  dayBounds(today.value)[1],
]);
const distractionToday = computed(() =>
  dayDistraction(
    props.data.sessions,
    today.value,
    minuteNow.value,
    focusBlocks.value,
  ),
);
const distractionAverage = computed(() =>
  averageDistraction(
    props.data.sessions,
    distractionDays.value,
    minuteNow.value,
    focusBlocks.value,
  ),
);
const topTasks = computed(() => {
  const totals = new Map(closedByDay.value.get(today.value));
  if (active.value)
    totals.set(
      active.value.taskId,
      (totals.get(active.value.taskId) ?? 0) + activeIn(today.value),
    );
  return props.data.tasks
    .map((task) => ({ task, total: totals.get(task.id) ?? 0 }))
    .filter((row) => row.total > 0)
    .sort((a, b) => b.total - a.total || compareTasks()(a.task, b.task))
    .slice(0, 5);
});
</script>

<template>
  <div class="page-heading">
    <h1>Dashboard</h1>
    <div class="planning-actions dashboard-heading-actions">
      <button
        class="today-button"
        @click="openView({ view: 'journal', section: 'morning' })"
      >
        Przegląd dnia</button
      ><button class="today-button" @click="$emit('report', today)">
        Raport dzisiejszy
      </button>
    </div>
  </div>
  <div class="dashboard-stats">
    <section class="dashboard-card">
      <h2>Dzisiaj</h2>
      <strong class="mono">{{ duration(days[6].total) }}</strong>
      <template v-if="todayTarget">
        <TargetBar
          :total="days[6].total"
          :target="todayTarget"
          label="Dzisiaj względem celu"
        /><span class="target-note mono"
          >{{ hhmm(days[6].total) }} / {{ hhmm(todayTarget) }}</span
        >
      </template>
      <span v-else class="target-note">{{
        targetHoliday(settings, today) ??
        (todayOff
          ? "Poza biurem: " + meetingTitle(todayOff, settings.calendarShowTitles)
          : dayTarget(settings, today) > 0
            ? "Spotkania wypełniają cały cel dnia"
            : "Dzień wolny")
      }}</span>
      <span v-if="days[6].overtime > 0" class="target-note dashboard-overtime"
        >Nadgodziny:
        <span class="mono">{{ hm(days[6].overtime) }}</span></span
      >
    </section>
    <section class="dashboard-card">
      <h2>Ten tydzień</h2>
      <span class="mono stat-value">{{ duration(thisWeekTotal) }}</span>
      <template v-if="thisWeekTarget">
        <TargetBar
          :total="thisWeekTotal"
          :target="thisWeekTarget"
          label="Ten tydzień względem celu"
        /><span class="target-note mono"
          >{{ hhmm(thisWeekTotal) }} / {{ hhmm(thisWeekTarget) }}</span
        >
      </template>
    </section>
    <section class="dashboard-card">
      <h2>Ostatnie 7 dni</h2>
      <strong class="mono">{{ duration(weekTotal) }}</strong>
      <span class="target-note"
        >Średnio
        <span class="mono">{{
          hhmm(workedDays ? weekTotal / workedDays : 0)
        }}</span>
        na dzień pracy</span
      >
    </section>
    <section class="dashboard-card">
      <h2>Ten miesiąc</h2>
      <strong class="mono">{{ duration(monthTotal) }}</strong>
      <span class="target-note">{{
        new Intl.DateTimeFormat("pl-PL", {
          month: "long",
          year: "numeric",
        }).format(now)
      }}</span>
    </section>
  </div>
  <div class="dashboard-columns">
    <div class="dashboard-side">
    <section
      class="dashboard-card"
      aria-label="Czas pracy w ostatnich 7 dniach"
    >
      <h2>Czas pracy · ostatnie 7 dni</h2>
      <p class="report-note">
        Wybierz dzień, aby otworzyć jego raport.<template v-if="weekOvertime > 0">
          Fragment słupka ponad normę dnia to nadgodziny.</template
        >
      </p>
      <p v-if="weekOvertime > 0" class="dashboard-week-overtime">
        Nadgodziny w tych 7 dniach:
        <span class="mono">{{ hm(weekOvertime) }}</span>
      </p>
      <button
        v-for="day in days"
        :key="day.key"
        class="dashboard-day"
        :aria-label="'Raport ' + day.key + ': ' + duration(day.total)"
        :title="day.overtime > 0 ? 'Nadgodziny: ' + hm(day.overtime) : undefined"
        @click="$emit('report', day.key)"
      >
        <span>{{ day.label }}</span
        ><span class="dashboard-bar split"
          ><span
            :style="{
              width: `${((day.total - day.overtime) / maxDay) * 100}%`,
            }" /><span
            v-if="day.overtime > 0"
            class="overtime"
            :style="{ width: `${(day.overtime / maxDay) * 100}%` }" /><span
            v-if="day.target"
            class="target-line"
            :style="{ left: `${(day.target / maxDay) * 100}%` }"
        /></span
        ><span class="mono" :class="{ 'dashboard-zero': !day.total }">{{
          duration(day.total)
        }}</span>
      </button>
    </section>
    <section class="dashboard-card" aria-label="Rozproszenie">
      <h2>Rozproszenie</h2>
      <table class="estimate-table distraction-table">
        <thead>
          <tr>
            <th></th>
            <th>Dziś</th>
            <th title="Średnia z 7 poprzednich dni roboczych z zapisaną pracą">
              Średnio (7 dni rob.)
            </th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <th scope="row">Przełączenia</th>
            <td class="mono">{{ distractionToday.switches }}</td>
            <td class="mono">{{ oneDecimal(distractionAverage.switches) }}</td>
          </tr>
          <tr>
            <th scope="row">Mediana pomiaru</th>
            <td class="mono">{{ hhmm(distractionToday.medianMs) }}</td>
            <td class="mono">{{ hhmm(distractionAverage.medianMs) }}</td>
          </tr>
          <tr>
            <th scope="row">Najdłuższy pomiar</th>
            <td class="mono">{{ hhmm(distractionToday.longestMs) }}</td>
            <td class="mono">{{ hhmm(distractionAverage.longestMs) }}</td>
          </tr>
          <tr>
            <th scope="row">Bloki skupienia</th>
            <td class="mono">
              {{ distractionToday.focusDone }} /
              {{ distractionToday.focusStarted }}
            </td>
            <td class="mono">
              {{ oneDecimal(distractionAverage.focusDone) }} /
              {{ oneDecimal(distractionAverage.focusStarted) }}
            </td>
          </tr>
        </tbody>
      </table>
      <p class="report-note">
        Przełączenie: start innego zadania w ciągu 15 min po poprzednim
        pomiarze. Bloki: ukończone / rozpoczęte.
      </p>
    </section>
    </div>
    <div class="dashboard-side">
      <section class="dashboard-card" aria-label="Zagrożone teraz">
        <h2>Zagrożone teraz</h2>
        <p v-if="!risky.length" class="report-note">
          Żadne otwarte zadanie nie przekroczyło szacunku.
        </p>
        <div v-for="row in risky" :key="row.task.id" class="dashboard-count">
          <span>{{ row.task.title }}</span
          ><strong class="mono"
            >{{ hhmm(row.actual) }} / {{ hhmm(row.estimate) }} ·
            <span class="estimate-over"
              >{{ percent(row.actual, row.estimate) }}%</span
            ></strong
          >
        </div>
      </section>
      <section class="dashboard-card" aria-label="Statusy zadań">
        <h2>Zadania</h2>
        <dl class="dashboard-status-grid">
          <div v-for="item in counts" :key="item.label">
            <dt>{{ item.label }}</dt>
            <dd><strong>{{ item.count }}</strong></dd>
          </div>
        </dl>
      </section>
      <section class="dashboard-card" aria-label="Bloki skupienia">
        <h2>Skupienie</h2>
        <dl class="dashboard-status-grid">
          <div>
            <dt>Ukończone bloki dziś</dt>
            <dd><strong>{{ focus.today }}</strong></dd>
          </div>
          <div>
            <dt>W tym tygodniu</dt>
            <dd><strong>{{ focus.week }}</strong></dd>
          </div>
        </dl>
        <button class="text-link" @click="openSettings('focus')">
          Długość bloku i przerwy
        </button>
      </section>
    </div>
  </div>
  <div class="dashboard-columns even dashboard-top">
  <section class="dashboard-card" aria-label="Najwięcej czasu dzisiaj">
    <h2>Najwięcej czasu dzisiaj</h2>
    <div v-for="row in topTasks" :key="row.task.id" class="dashboard-count">
      <span>{{ row.task.title }}</span
      ><strong class="mono">{{ duration(row.total) }}</strong>
    </div>
    <p v-if="!topTasks.length" class="report-note">
      Brak zarejestrowanej pracy dzisiaj.
    </p>
  </section>
  <AgentsCard :now="now" :tasks="data.tasks" />
  </div>
  <EstimateAccuracy
    :data="data"
    :now="now"
    @report="$emit('report', $event)"
  />
</template>
