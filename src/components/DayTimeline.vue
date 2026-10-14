<script setup lang="ts">
import { computed, ref } from "vue";
import type { Session, Task } from "../store";
import {
  axisRange,
  colorVar,
  comparison,
  hourTicks,
  planBlocks,
  workBlocks,
} from "../dayplan";
import { hm, signedDiff, targetDiffMinutes } from "../target";
import { dayKey } from "../time";

const props = defineProps<{
  tasks: Task[];
  sessions: Session[];
  day: string;
  now: number;
  dayStart: string;
}>();
const emit = defineEmits<{
  /** Klik w blok rzeczywisty: edycja przedziału w SessionEditor. */
  edit: [session: Session];
  /** Klik w blok planu: rozwinięcie TaskPlanning zadania. */
  plan: [taskId: number];
}>();

const titles = computed(() => new Map(props.tasks.map((t) => [t.id, t.title])));
const title = (id: number) => titles.value.get(id) ?? "Zadanie";
const plan = computed(() => planBlocks(props.tasks, props.day, props.dayStart));
const work = computed(() => workBlocks(props.sessions, props.day, props.now));
const range = computed(() =>
  axisRange(props.day, props.dayStart, plan.value, work.value),
);
const ticks = computed(() => hourTicks(...range.value));
// Przy długiej osi etykieta co drugą godzinę, żeby mieściła się w 760 px.
const labelEvery = computed(() => (ticks.value.length > 13 ? 2 : 1));
const pct = (at: number) => {
  const [from, to] = range.value;
  return ((Math.min(Math.max(at, from), to) - from) / (to - from)) * 100;
};
const width = (start: number, end: number) =>
  Math.max(0.4, pct(end) - pct(start));
const clock = (at: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
const span = (start: number, end: number) => `${clock(start)}–${clock(end)}`;
const showNow = computed(
  () =>
    dayKey(props.now) === props.day &&
    props.now >= range.value[0] &&
    props.now <= range.value[1],
);

const rows = computed(() =>
  comparison(props.tasks, props.sessions, props.day, props.now, plan.value),
);
/** Wiersz z szacunkiem: tylko taki ma różnicę i wchodzi do różnicy „Razem”. */
const comparable = (r: (typeof rows.value)[number]) =>
  r.status !== "unplanned" && !r.noEstimate;
const totals = computed(() => {
  const planTotal = rows.value.reduce((sum, r) => sum + r.plan, 0);
  const actual = rows.value
    .filter(comparable)
    .reduce((sum, r) => sum + r.actual, 0);
  // Cała praca dnia (także spoza planu i bez szacunku) pokazywana osobno.
  const all = rows.value.reduce((sum, r) => sum + r.actual, 0);
  return { plan: planTotal, actual, all };
});
const over = (actual: number, planned: number) =>
  targetDiffMinutes(actual, planned) > 0;
const legend = computed(() => {
  const seen = new Set<number>();
  const ids = [
    ...plan.value.map((b) => b.task.id),
    ...work.value.map((b) => b.session.taskId),
  ].filter((id) => !seen.has(id) && Boolean(seen.add(id)));
  return ids.map((id) => ({ id, title: title(id) }));
});

const tip = ref<{ text: string; x: number } | null>(null);
function show(text: string, start: number, end: number) {
  tip.value = { text, x: (pct(start) + pct(end)) / 2 };
}
const planText = (b: (typeof plan.value)[number]) =>
  `Plan: ${b.task.title} · ${span(b.start, b.end)}${b.estimated ? "" : " · bez szacunku"}`;
const workText = (b: (typeof work.value)[number]) =>
  `Praca: ${title(b.session.taskId)} · ${clock(b.start)}–${b.running ? "trwa" : clock(b.end)}`;
</script>
<template>
  <div class="day-timeline">
    <div class="dtl-chart">
      <div class="dtl-labels" aria-hidden="true">
        <span>Plan</span><span>Rzeczywistość</span>
      </div>
      <div class="dtl-plot">
        <svg
          class="dtl-svg"
          role="group"
          aria-label="Oś planu i rzeczywistości"
          height="96"
          width="100%"
          @mouseleave="tip = null"
        >
          <g class="dtl-grid">
            <template v-for="(t, i) in ticks" :key="t">
              <line :x1="pct(t) + '%'" :x2="pct(t) + '%'" y1="17" y2="92" />
              <text
                v-if="i % labelEvery === 0"
                :x="pct(t) + '%'"
                y="11"
                :text-anchor="
                  i === 0 ? 'start' : i === ticks.length - 1 ? 'end' : 'middle'
                "
              >
                {{ clock(t) }}
              </text>
            </template>
          </g>
          <rect class="dtl-track" x="0" y="22" width="100%" height="28" />
          <rect class="dtl-track" x="0" y="60" width="100%" height="28" />
          <rect
            v-for="b in plan"
            :key="'p' + b.task.id"
            class="dtl-block dtl-plan"
            :class="{ dashed: !b.estimated, done: b.task.done }"
            :x="pct(b.start) + '%'"
            y="23"
            :width="width(b.start, b.end) + '%'"
            height="26"
            rx="4"
            :style="{ fill: colorVar(b.task.id), stroke: colorVar(b.task.id) }"
            tabindex="0"
            role="button"
            :aria-label="planText(b)"
            @mouseenter="show(planText(b), b.start, b.end)"
            @focus="show(planText(b), b.start, b.end)"
            @blur="tip = null"
            @click="emit('plan', b.task.id)"
            @keydown.enter.prevent="emit('plan', b.task.id)"
            @keydown.space.prevent="emit('plan', b.task.id)"
          />
          <rect
            v-for="b in work"
            :key="'w' + b.session.id"
            class="dtl-block dtl-work"
            :class="{ running: b.running }"
            :x="pct(b.start) + '%'"
            y="61"
            :width="width(b.start, b.end) + '%'"
            height="26"
            rx="4"
            :style="{ fill: colorVar(b.session.taskId) }"
            tabindex="0"
            role="button"
            :aria-label="workText(b)"
            @mouseenter="show(workText(b), b.start, b.end)"
            @focus="show(workText(b), b.start, b.end)"
            @blur="tip = null"
            @click="emit('edit', b.session)"
            @keydown.enter.prevent="emit('edit', b.session)"
            @keydown.space.prevent="emit('edit', b.session)"
          />
          <template v-for="b in work" :key="'e' + b.session.id">
            <line
              v-if="b.running"
              class="dtl-live"
              :x1="pct(b.end) + '%'"
              :x2="pct(b.end) + '%'"
              y1="58"
              y2="90"
            />
          </template>
          <line
            v-if="showNow"
            class="dtl-now"
            :x1="pct(now) + '%'"
            :x2="pct(now) + '%'"
            y1="17"
            y2="92"
          />
        </svg>
        <div
          v-if="tip"
          class="dtl-tip"
          role="tooltip"
          :style="{ left: Math.min(88, Math.max(12, tip.x)) + '%' }"
        >
          {{ tip.text }}
        </div>
      </div>
    </div>
    <ul v-if="legend.length" class="dtl-legend" aria-label="Legenda osi dnia">
      <li v-for="item in legend" :key="item.id">
        <span class="dtl-swatch" :style="{ background: colorVar(item.id) }" />{{
          item.title
        }}
      </li>
    </ul>
    <p v-else class="report-note">Brak planu i pracy w tym dniu.</p>
    <table
      v-if="rows.length"
      class="dtl-table"
      aria-label="Plan i rzeczywistość"
    >
      <thead>
        <tr>
          <th scope="col">Zadanie</th>
          <th scope="col">Plan</th>
          <th scope="col">Rzeczywistość</th>
          <th scope="col">Różnica</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.task.id">
          <th scope="row">
            <span
              class="dtl-swatch"
              :style="{ background: colorVar(row.task.id) }"
            />{{ row.task.title }}
          </th>
          <td class="mono">
            <span v-if="row.status === 'unplanned'" class="dtl-status"
              >nieplanowane</span
            ><span v-else-if="row.noEstimate" class="dtl-status"
              >bez szacunku</span
            ><template v-else>{{ hm(row.plan) }}</template>
          </td>
          <td class="mono">
            <span v-if="row.status === 'unstarted'" class="dtl-status"
              >niezaczęte</span
            ><template v-else>{{ hm(row.actual) }}</template>
          </td>
          <td
            class="mono"
            :class="{
              'dtl-over': comparable(row) && over(row.actual, row.plan),
            }"
          >
            {{ comparable(row) ? signedDiff(row.actual, row.plan) : "—" }}
          </td>
        </tr>
      </tbody>
      <tfoot>
        <tr>
          <th scope="row">Razem</th>
          <td class="mono">{{ hm(totals.plan) }}</td>
          <td class="mono">
            {{ hm(totals.actual)
            }}<span
              v-if="totals.all !== totals.actual"
              class="dtl-status dtl-all"
              title="Cała praca dnia, także spoza planu i bez szacunku"
            >
              · łącznie {{ hm(totals.all) }}</span
            >
          </td>
          <td
            class="mono"
            :class="{ 'dtl-over': over(totals.actual, totals.plan) }"
          >
            {{ signedDiff(totals.actual, totals.plan) }}
          </td>
        </tr>
      </tfoot>
    </table>
  </div>
</template>
