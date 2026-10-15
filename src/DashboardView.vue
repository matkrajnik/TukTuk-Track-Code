<script setup lang="ts">
import { computed } from "vue";
import { duration, type Snapshot } from "./store";
import { dayKey, dayBounds, inDay } from "./time";
import { compareTasks } from "./order";
import { atRisk, percent } from "./estimates";
import TargetBar from "./components/TargetBar.vue";
import EstimateAccuracy from "./components/EstimateAccuracy.vue";
import AgentsCard from "./components/AgentsCard.vue";
import {
  dayTarget,
  hhmm,
  isoWeekday,
  useSettings,
  weekTarget,
} from "./target";

const props = defineProps<{ data: Snapshot; now: number }>();
defineEmits<{ report: [day: string] }>();
const settings = useSettings();
const today = computed(() => dayKey(props.now));
const dayTotal = (key: string) =>
  props.data.sessions.reduce((sum, s) => sum + inDay(s, key, props.now), 0);
const todayTarget = computed(() => dayTarget(settings.value, today.value));
/** Bieżący tydzień kalendarzowy, poniedziałek–niedziela. */
const thisWeek = computed(() => {
  const monday = new Date(dayBounds(today.value)[0]);
  monday.setDate(monday.getDate() - isoWeekday(today.value) + 1);
  return Array.from({ length: 7 }, (_, i) => {
    const date = new Date(monday);
    date.setDate(monday.getDate() + i);
    return dayKey(+date);
  });
});
const thisWeekTotal = computed(() =>
  thisWeek.value.reduce((sum, key) => sum + dayTotal(key), 0),
);
const thisWeekTarget = computed(() => weekTarget(settings.value));
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
    };
  }),
);
const weekTotal = computed(() =>
  days.value.reduce((sum, day) => sum + day.total, 0),
);
const workedDays = computed(
  () => days.value.filter((day) => day.total >= 60_000).length,
);
const risky = computed(() => atRisk(props.data, props.now));
const maxDay = computed(() =>
  Math.max(1, ...days.value.map((day) => Math.max(day.total, day.target))),
);
const monthTotal = computed(() => {
  const date = new Date(props.now);
  const start = +new Date(date.getFullYear(), date.getMonth(), 1);
  return props.data.sessions.reduce(
    (sum, s) =>
      sum +
      Math.max(
        0,
        Math.min(s.endedAt ?? props.now, props.now) -
          Math.max(s.startedAt, start),
      ),
    0,
  );
});
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
const topTasks = computed(() =>
  props.data.tasks
    .map((task) => ({
      task,
      total: props.data.sessions
        .filter((s) => s.taskId === task.id)
        .reduce((sum, s) => sum + inDay(s, today.value, props.now), 0),
    }))
    .filter((row) => row.total > 0)
    .sort((a, b) => b.total - a.total || compareTasks()(a.task, b.task))
    .slice(0, 5),
);
</script>

<template>
  <div class="page-heading">
    <h1>Dashboard</h1>
    <button class="today-button" @click="$emit('report', today)">
      Raport dzisiejszy
    </button>
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
      <span v-else class="target-note">Dzień wolny</span>
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
    <section
      class="dashboard-card"
      aria-label="Czas pracy w ostatnich 7 dniach"
    >
      <h2>Czas pracy · ostatnie 7 dni</h2>
      <p class="report-note">Wybierz dzień, aby otworzyć jego raport.</p>
      <button
        v-for="day in days"
        :key="day.key"
        class="dashboard-day"
        :aria-label="'Raport ' + day.key + ': ' + duration(day.total)"
        @click="$emit('report', day.key)"
      >
        <span>{{ day.label }}</span
        ><span class="dashboard-bar"
          ><span :style="{ width: `${(day.total / maxDay) * 100}%` }" /><span
            v-if="day.target"
            class="target-line"
            :style="{ left: `${(day.target / maxDay) * 100}%` }"
        /></span
        ><span class="mono" :class="{ 'dashboard-zero': !day.total }">{{
          duration(day.total)
        }}</span>
      </button>
    </section>
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
  <AgentsCard :now="now" />
  </div>
  <EstimateAccuracy
    :data="data"
    :now="now"
    @report="$emit('report', $event)"
  />
</template>
