<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
import { duration, type Snapshot, type Task } from "../store";
import { issueRef } from "../issue";
import { dayBounds, dayKey, plural } from "../time";
import {
  addToDays,
  closedDayTotals,
  decimalHours,
  overtimeOf,
} from "../report";
import { holidays } from "../holidays";
import { dayHours, dayMarkOf } from "../days";
import TargetBar from "./TargetBar.vue";
import ExportButtons from "./ExportButtons.vue";
import { sessionsSignature, useAgentTime } from "../composables/useAgentTime";
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
/** Po północy (i nowym miesiącu) bieżący miesiąc przechodzi dalej; wybrany ręcznie zostaje. */
const todayKey = computed(() => dayKey(props.now));
watch(todayKey, (today, previous) => {
  if (month.value === monthOf(previous)) month.value = monthOf(today);
});
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
    /** Nadgodziny dnia (B5): ponad normę dnia, w dzień wolny cała praca. */
    overtime: overtimeOf(totalOf(day), day, settings.value),
    label: new Intl.DateTimeFormat("pl-PL", {
      weekday: "short",
      day: "numeric",
    }).format(dayBounds(day)[0]),
    total: totalOf(day),
    target: dayTarget(settings.value, day),
    holiday: targetHoliday(settings.value, day),
    weekend: isoWeekday(day) > 5,
    /** Początek i koniec dnia z „Rozpocznij / Zakończ dzień” (B13). */
    hours: dayHours(dayMarkOf(props.data, day)),
    resumed: (dayMarkOf(props.data, day)?.resumed ?? 0) > 0,
  })),
);
const total = computed(() => rows.value.reduce((sum, d) => sum + d.total, 0));
/** Suma nadgodzin dni miesiąca (każdy dzień względem swojej normy). */
const overtimeTotal = computed(() =>
  rows.value.reduce((sum, d) => sum + d.overtime, 0),
);
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
const workDaysWord = computed(() =>
  plural(workDays.value, "dzień roboczy", "dni robocze", "dni roboczych"),
);
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
/**
 * Czas agentów w miesiącu (W1): kolumny „Agent” i „Równolegle” w tabeli zadań,
 * tylko gdy agenci pracowali nad zadaniami. Nie zmienia sum, celu ani eksportu.
 */
const monthRange = (): [number, number] => [
  dayBounds(days.value[0])[0],
  dayBounds(days.value[days.value.length - 1])[1],
];
const agentTime = useAgentTime(monthRange, () =>
  sessionsSignature(props.data.sessions),
);
const agentTotals = computed(
  () =>
    new Map(
      agentTime.time.value.totals
        .filter((t) => taskIds.value.has(t.taskId))
        .map((t) => [t.taskId, t]),
    ),
);
const showAgents = computed(() => agentTotals.value.size > 0);
/** Wiersze tabeli zadań: zadania z Twoim czasem, potem tylko z czasem agentów. */
const taskTableRows = computed(() => {
  const own = new Set(taskRows.value.map((r) => r.task.id));
  const agentOnly = props.data.tasks
    .filter((t) => !own.has(t.id) && agentTotals.value.has(t.id))
    .map((task: Task) => ({
      task,
      project: task.details?.project?.trim() ?? "",
      total: 0,
    }))
    .sort(
      (a, b) =>
        (agentTotals.value.get(b.task.id)?.agentMs ?? 0) -
          (agentTotals.value.get(a.task.id)?.agentMs ?? 0) ||
        a.task.id - b.task.id,
    );
  return [...taskRows.value, ...agentOnly];
});
const agentSum = computed(() =>
  [...agentTotals.value.values()].reduce(
    (sum, t) => ({
      agent: sum.agent + t.agentMs,
      parallel: sum.parallel + t.parallelMs,
    }),
    { agent: 0, parallel: 0 },
  ),
);
const agentOf = (id: number) => agentTotals.value.get(id);
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
        <p
          class="monthly-overtime-summary"
          title="Suma pracy ponad normę każdego dnia (w dzień wolny i święto cała praca)"
        >
          Nadgodziny w miesiącu:
          <span class="mono monthly-overtime-total">{{
            hhmm(overtimeTotal)
          }}</span>
        </p>
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
        <ExportButtons
          :days="days"
          :name="'TukTuk-Track-' + month"
          label="Eksport miesiąca"
        />
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
        <table
          v-if="taskTableRows.length"
          class="estimate-table monthly-table"
        >
          <thead>
            <tr>
              <th>Zadanie</th>
              <th>Klucz Jiry</th>
              <th>Projekt</th>
              <th>Czas</th>
              <th>Godziny</th>
              <th
                v-if="showAgents"
                title="Czas pracy agentów AI nad zadaniem (poza celem i raportem TS)"
              >
                Agent
              </th>
              <th
                v-if="showAgents"
                title="Część czasu agentów nakładająca się na Twój pomiar tego zadania"
              >
                Równolegle
              </th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="row in taskTableRows" :key="row.task.id">
              <td>{{ row.task.title }}</td>
              <td class="mono">{{ issueRef(row.task)?.key ?? "" }}</td>
              <td>{{ row.project }}</td>
              <td class="mono">{{ hhmm(row.total) }}</td>
              <td class="mono">{{ decimalHours(row.total) }}</td>
              <td v-if="showAgents" class="mono monthly-agent">
                {{ agentOf(row.task.id) ? hhmm(agentOf(row.task.id)!.agentMs) : "" }}
              </td>
              <td v-if="showAgents" class="mono monthly-agent">
                {{
                  agentOf(row.task.id)
                    ? hhmm(agentOf(row.task.id)!.parallelMs)
                    : ""
                }}
              </td>
            </tr>
          </tbody>
          <tfoot>
            <tr>
              <th>Razem</th>
              <td></td>
              <td></td>
              <td class="mono">{{ hhmm(total) }}</td>
              <td class="mono">{{ decimalHours(total) }}</td>
              <td v-if="showAgents" class="mono monthly-agent">
                {{ hhmm(agentSum.agent) }}
              </td>
              <td v-if="showAgents" class="mono monthly-agent">
                {{ hhmm(agentSum.parallel) }}
              </td>
            </tr>
          </tfoot>
        </table>
        <p v-else class="report-note">Brak zarejestrowanej pracy w tym miesiącu.</p>
        <p v-if="showAgents" class="report-note">
          Czas agentów AI jest osobną informacją: nie wlicza się do celu, raportu
          TS ani eksportu.
        </p>
      </section>
    </div>

    <section class="dashboard-card" aria-label="Czas pracy w dniach miesiąca">
      <h2>Dni miesiąca</h2>
      <p class="report-note">
        Linia oznacza cel dnia, fragment słupka ponad nią to nadgodziny.
        Wybierz dzień, aby otworzyć jego raport.
      </p>
      <div class="monthly-grid monthly-day-head" aria-hidden="true">
        <span>Dzień</span><span></span><span>Godziny dnia</span><span>Czas</span
        ><span>Nadgodziny</span>
      </div>
      <button
        v-for="d in rows"
        :key="d.day"
        class="monthly-day"
        :class="{ free: !d.target, holiday: !!d.holiday }"
        :aria-label="'Raport ' + d.day + ': ' + duration(d.total)"
        :title="
          [d.holiday, d.overtime > 0 ? 'Nadgodziny: ' + hhmm(d.overtime) : '']
            .filter(Boolean)
            .join(' · ') || undefined
        "
        @click="emit('open', d.day)"
      >
        <span class="monthly-day-label"
          >{{ d.label
          }}<span v-if="d.holiday" class="monthly-day-holiday">
            · {{ d.holiday }}</span
          ></span
        ><span class="dashboard-bar split"
          ><span
            :style="{ width: `${((d.total - d.overtime) / maxDay) * 100}%` }" /><span
            v-if="d.overtime > 0"
            class="overtime"
            :style="{ width: `${(d.overtime / maxDay) * 100}%` }" /><span
            v-if="d.target"
            class="target-line"
            :style="{ left: `${(d.target / maxDay) * 100}%` }"
        /></span
        ><span
          class="mono monthly-hours"
          :class="{ 'dashboard-zero': !d.hours }"
          :title="d.resumed ? 'Dzień wznawiany' : undefined"
          >{{ d.hours || "–" }}</span
        ><span class="mono" :class="{ 'dashboard-zero': !d.total }">{{
          hhmm(d.total)
        }}</span
        ><span
          class="mono monthly-overtime"
          :class="{ 'dashboard-zero': !d.overtime }"
          >{{ d.overtime > 0 ? hhmm(d.overtime) : "–" }}</span
        >
      </button>
      <div class="monthly-grid monthly-day-foot">
        <span>Razem</span><span></span><span></span
        ><span class="mono">{{ hhmm(total) }}</span
        ><span
          class="mono monthly-overtime"
          :class="{ 'dashboard-zero': !overtimeTotal }"
          >{{ hhmm(overtimeTotal) }}</span
        >
      </div>
    </section>
  </section>
</template>
