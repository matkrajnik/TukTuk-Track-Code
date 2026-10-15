<script setup lang="ts">
import { computed, ref } from "vue";
import { meetingOccupies, meetingTitle, type Session, type Task } from "../store";
import {
  axisRange,
  colorVar,
  comparison,
  hourTicks,
  planBlocks,
  workBlocks,
} from "../dayplan";
import { meetingSpans } from "../capacity";
import { corrections, plannedMs } from "../estimates";
import { hhmm, signedDiff, targetDiffMinutes, useSettings } from "../target";
import { dayBounds, dayKey } from "../time";
import { useMeetings } from "../composables/useMeetings";
import { useAgentTime } from "../composables/useAgentTime";
import { agentName } from "../agenttime";

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

const settings = useSettings();
/** Spotkania dnia; tor „Spotkania” tylko przy połączonym kalendarzu. */
const calendar = useMeetings(() => dayBounds(props.day));
const showMeetings = computed(() => calendar.connected.value);
const meetings = computed(() => {
  const [from, to] = dayBounds(props.day);
  return calendar.events.value
    .filter((e) => meetingOccupies(e) && e.start < to && e.end > from)
    .map((e) => ({
      event: e,
      start: Math.max(from, e.start),
      end: Math.min(to, e.end),
    }));
});
/** Przedziały spotkań, które omijają bloki planu (`meetingSpans` pomija całodniowe). */
const busy = computed(() => meetingSpans(calendar.events.value, props.day));
const titles = computed(() => new Map(props.tasks.map((t) => [t.id, t.title])));
const title = (id: number) => titles.value.get(id) ?? "Zadanie";
/** „Planuj z poprawką szacunków”: bloki planu mają długość szacunku z poprawką. */
const minute = computed(() => Math.floor(props.now / 60_000) * 60_000);
const minutesOf = computed(() => {
  if (!settings.value.planWithCorrection) return undefined;
  const of = corrections(
    { tasks: props.tasks, sessions: props.sessions },
    minute.value,
  );
  return (task: Task) => plannedMs(task, of) / 60_000;
});
const plan = computed(() =>
  planBlocks(
    props.tasks,
    props.day,
    props.dayStart,
    busy.value,
    minutesOf.value,
  ),
);
const work = computed(() => workBlocks(props.sessions, props.day, props.now));
/** Czas agentów (W1): tor „Agenci” pod „Rzeczywistością”, tylko gdy agenci pracowali. */
const agentTime = useAgentTime(() => dayBounds(props.day));
const agents = computed(() => {
  const [from, to] = dayBounds(props.day);
  return agentTime.time.value.periods
    .filter((p) => p.start < to && p.end > from)
    .map((p) => ({
      period: p,
      start: Math.max(from, p.start),
      end: Math.min(to, p.end),
    }));
});
const showAgents = computed(() => agents.value.length > 0);
const range = computed(() =>
  axisRange(props.day, props.dayStart, plan.value, work.value, [
    ...meetings.value.filter((m) => !m.event.allDay),
    ...agents.value,
  ]),
);
/** Przesunięcie torów Plan i Rzeczywistość w dół, gdy jest tor Spotkania. */
const top = computed(() => (showMeetings.value ? 38 : 0));
/** Wysokość toru „Agenci” (0, gdy go nie ma). */
const extra = computed(() => (showAgents.value ? 38 : 0));
/** Etykiety torów od góry; wiersze siatki jak tory w SVG. */
const lanes = computed(() => [
  ...(showMeetings.value ? ["Spotkania"] : []),
  "Plan",
  "Rzeczywistość",
  ...(showAgents.value ? ["Agenci"] : []),
]);
const labelRows = computed(
  () => "22px " + lanes.value.map(() => "28px").join(" 10px "),
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
    ...agents.value.flatMap((b) =>
      b.period.taskId === null ? [] : [b.period.taskId],
    ),
  ].filter((id) => !seen.has(id) && Boolean(seen.add(id)));
  return ids.map((id) => ({ id, title: title(id) }));
});

const tip = ref<{ text: string; x: number } | null>(null);
function show(text: string, start: number, end: number) {
  tip.value = { text, x: (pct(start) + pct(end)) / 2 };
}
const planText = (b: (typeof plan.value)[number]) =>
  `Plan: ${b.task.title} · ${span(b.start, b.end)}${b.parts > 1 ? ` · część ${b.part} z ${b.parts}` : ""}${b.estimated ? "" : " · bez szacunku"}`;
const workText = (b: (typeof work.value)[number]) =>
  `Praca: ${title(b.session.taskId)} · ${clock(b.start)}–${b.running ? "trwa" : clock(b.end)}`;
const folderName = (folder: string) =>
  folder.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
const agentText = (b: (typeof agents.value)[number]) => {
  const folder = folderName(b.period.folder);
  return `Agent: ${agentName(b.period.agent)} · ${
    b.period.taskId === null ? "bez zadania" : title(b.period.taskId)
  } · ${span(b.period.start, b.period.end)}${folder ? " · " + folder : ""}`;
};
const unmatchedAgents = computed(() =>
  agents.value.some((b) => b.period.taskId === null),
);
const meetingText = (m: (typeof meetings.value)[number]) =>
  `Spotkanie: ${meetingTitle(m.event, settings.value.calendarShowTitles)} · ${
    m.event.allDay ? "cały dzień" : span(m.event.start, m.event.end)
  }${m.event.showAs === "tentative" ? " · wstępnie" : ""}`;
</script>
<template>
  <div class="day-timeline">
    <div class="dtl-chart">
      <div
        class="dtl-labels"
        :style="{ gridTemplateRows: labelRows }"
        aria-hidden="true"
      >
        <span
          v-for="(lane, i) in lanes"
          :key="lane"
          :style="{ gridRow: 2 + 2 * i }"
          >{{ lane }}</span
        >
      </div>
      <div class="dtl-plot">
        <svg
          class="dtl-svg"
          role="group"
          aria-label="Oś planu i rzeczywistości"
          :height="96 + top + extra"
          width="100%"
          @mouseleave="tip = null"
        >
          <g class="dtl-grid">
            <template v-for="(t, i) in ticks" :key="t">
              <line
                :x1="pct(t) + '%'"
                :x2="pct(t) + '%'"
                y1="17"
                :y2="92 + top + extra"
              />
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
          <rect
            v-if="showMeetings"
            class="dtl-track"
            x="0"
            y="22"
            width="100%"
            height="28"
          />
          <rect
            v-for="m in meetings"
            :key="'m' + m.event.id"
            class="dtl-block dtl-meeting"
            :class="{
              tentative: m.event.showAs === 'tentative',
              allday: m.event.allDay,
            }"
            :x="pct(m.start) + '%'"
            y="23"
            :width="width(m.start, m.end) + '%'"
            height="26"
            rx="4"
            tabindex="0"
            role="img"
            :aria-label="meetingText(m)"
            @mouseenter="show(meetingText(m), m.start, m.end)"
            @focus="show(meetingText(m), m.start, m.end)"
            @blur="tip = null"
          />
          <rect
            class="dtl-track"
            x="0"
            :y="22 + top"
            width="100%"
            height="28"
          />
          <rect
            class="dtl-track"
            x="0"
            :y="60 + top"
            width="100%"
            height="28"
          />
          <rect
            v-for="b in plan"
            :key="'p' + b.task.id + '-' + b.part"
            class="dtl-block dtl-plan"
            :class="{ dashed: !b.estimated, done: b.task.done }"
            :x="pct(b.start) + '%'"
            :y="23 + top"
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
            :y="61 + top"
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
          <rect
            v-if="showAgents"
            class="dtl-track"
            x="0"
            :y="98 + top"
            width="100%"
            height="28"
          />
          <rect
            v-for="b in agents"
            :key="'a' + b.period.id"
            class="dtl-block dtl-agent"
            :class="{ unmatched: b.period.taskId === null }"
            :x="pct(b.start) + '%'"
            :y="99 + top"
            :width="width(b.start, b.end) + '%'"
            height="26"
            rx="4"
            :style="
              b.period.taskId === null
                ? undefined
                : { fill: colorVar(b.period.taskId) }
            "
            tabindex="0"
            role="img"
            :aria-label="agentText(b)"
            @mouseenter="show(agentText(b), b.start, b.end)"
            @focus="show(agentText(b), b.start, b.end)"
            @blur="tip = null"
          />
          <template v-for="b in work" :key="'e' + b.session.id">
            <line
              v-if="b.running"
              class="dtl-live"
              :x1="pct(b.end) + '%'"
              :x2="pct(b.end) + '%'"
              :y1="58 + top"
              :y2="90 + top"
            />
          </template>
          <line
            v-if="showNow"
            class="dtl-now"
            :x1="pct(now) + '%'"
            :x2="pct(now) + '%'"
            y1="17"
            :y2="92 + top + extra"
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
    <ul
      v-if="legend.length || meetings.length || agents.length"
      class="dtl-legend"
      aria-label="Legenda osi dnia"
    >
      <li v-for="item in legend" :key="item.id">
        <span class="dtl-swatch" :style="{ background: colorVar(item.id) }" />{{
          item.title
        }}
      </li>
      <li v-if="meetings.length" class="dtl-legend-meetings">
        <span class="dtl-swatch" style="background: var(--meeting)" />Spotkania
        ({{ meetings.length }})
      </li>
      <li v-if="unmatchedAgents" class="dtl-legend-agents">
        <span class="dtl-swatch dtl-agent-swatch" />Agent bez zadania
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
            ><template v-else>{{ hhmm(row.plan) }}</template>
          </td>
          <td class="mono">
            <span v-if="row.status === 'unstarted'" class="dtl-status"
              >niezaczęte</span
            ><template v-else>{{ hhmm(row.actual) }}</template>
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
          <td class="mono">{{ hhmm(totals.plan) }}</td>
          <td class="mono">
            {{ hhmm(totals.actual)
            }}<span
              v-if="totals.all !== totals.actual"
              class="dtl-status dtl-all"
              title="Cała praca dnia, także spoza planu i bez szacunku"
            >
              · łącznie {{ hhmm(totals.all) }}</span
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
