<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  change,
  closeDay,
  detailsOf,
  duration,
  saveDetails,
  meetingOccupies,
  meetingTitle,
  type Snapshot,
} from "./store";
import { dayBounds, dayKey, dayText, inDay } from "./time";
import { dayRows, decimalHours as decimal, rowsTotal, tsText } from "./report";
import { copyReport } from "./clipboard";
import { dayGoal } from "./capacity";
import { useMeetings } from "./composables/useMeetings";
import ManualTime from "./ManualTime.vue";
import {
  dayTarget,
  targetDiffMinutes,
  targetMessage,
  useSettings,
} from "./target";
const reportArea = ref<HTMLTextAreaElement | null>(null);
const props = defineProps<{ data: Snapshot; day: string; now: number }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  report: [day: string];
}>();
const busy = ref(false),
  error = ref(""),
  message = ref(""),
  notes = ref<Record<number, string>>({});
watch(
  () => props.day,
  () => {
    notes.value = Object.fromEntries(
      props.data.tasks.map((t) => [t.id, t.details?.notes?.[props.day] ?? ""]),
    );
  },
  { immediate: true },
);
watch(
  () => props.data,
  () => {
    for (const t of props.data.tasks)
      if (!(t.id in notes.value))
        notes.value[t.id] = t.details?.notes?.[props.day] ?? "";
  },
);
const rows = computed(() =>
  dayRows(props.data, props.day, props.now).filter((r) => r.total > 0),
);
const total = computed(() => rowsTotal(rows.value));
const settings = useSettings();
/** Spotkania dnia: sekcja „Spotkania” i cel dnia pomniejszony o spotkania. */
const calendar = useMeetings(() => dayBounds(props.day));
/** Cel mierzonego czasu: cel dnia minus spotkania niepokryte pomiarem. */
const target = computed(() =>
  dayGoal(
    dayTarget(settings.value, props.day),
    calendar.events.value,
    props.data.sessions,
    props.day,
    props.now,
  ),
);
const meetingClock = (at: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  }).format(at);
/**
 * Spotkania dnia, które zajmują czas (bez całodniowych). Przy każdym albo
 * „Dodaj jako czas”, albo ostrzeżenie o nakładaniu na pomiar (także w koszu
 * i trwający), albo informacja, że spotkanie jeszcze się nie skończyło.
 */
const meetings = computed(() => {
  const [from, to] = dayBounds(props.day);
  const all = [...props.data.sessions, ...(props.data.trashSessions ?? [])];
  return calendar.events.value
    .filter((e) => meetingOccupies(e) && !e.allDay && e.start < to && e.end > from)
    .map((e) => ({
      event: e,
      title: meetingTitle(e, settings.value.calendarShowTitles),
      time: `${meetingClock(e.start)}–${meetingClock(e.end)}`,
      overlaps: all.some(
        (s) => s.startedAt < e.end && (s.endedAt === null || s.endedAt > e.start),
      ),
      future: e.end > props.now,
    }));
});
/** Żądanie otwarcia „Dodaj czas ręcznie” z godzinami spotkania. */
const preset = ref<{ start: number; end: number; key: string } | null>(null);
function addMeeting(m: (typeof meetings.value)[number]) {
  preset.value = {
    start: m.event.start,
    end: m.event.end,
    key: m.event.id + ":" + Date.now(),
  };
}
const activeTasks = computed(() => props.data.tasks.filter((t) => !t.archived));
/** Informacja o celu dnia; nie blokuje zamknięcia. */
const targetText = computed(() => targetMessage(total.value, target.value));
const unfinished = computed(() =>
  props.data.tasks.filter(
    (t) => !t.done && !t.archived && t.details?.plannedDay === props.day,
  ),
);
const active = computed(() =>
  props.data.sessions.some(
    (s) => s.endedAt === null && inDay(s, props.day, props.now) > 0,
  ),
);
const gaps = computed(() => {
  const [start, end] = dayBounds(props.day);
  const spans = props.data.sessions
    .filter((s) => inDay(s, props.day, props.now) > 0)
    .map((s) => [
      Math.max(start, s.startedAt),
      Math.min(end, s.endedAt ?? props.now),
    ])
    .sort((a, b) => a[0] - b[0]);
  let cursor = 0;
  const result: number[][] = [];
  for (const [a, b] of spans) {
    if (cursor && a > cursor) result.push([cursor, a]);
    cursor = Math.max(cursor, b);
  }
  return result;
});
const report = computed(() =>
  tsText([props.day], props.data, props.now, {
    hoursHeader: "Godziny",
    trim: false,
    skipEmpty: true,
    order: "snapshot",
  }),
);
async function run(fn: () => Promise<void>) {
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function saveNotes() {
  await run(async () => {
    let next = props.data;
    for (const r of rows.value) {
      const detail = detailsOf(r.task);
      if ((detail.notes[props.day] ?? "") !== notes.value[r.task.id]) {
        detail.notes[props.day] = notes.value[r.task.id] ?? "";
        next = await saveDetails(r.task.id, detail);
      }
    }
    emit("updated", next);
    message.value = "Zapisano notatki.";
  });
}
async function move() {
  await run(async () => {
    const date = new Date(dayBounds(props.day)[0]);
    date.setDate(date.getDate() + 1);
    let next = props.data;
    for (const task of unfinished.value)
      next = await saveDetails(task.id, {
        ...detailsOf(task),
        plannedDay: dayKey(+date),
      });
    emit("updated", next);
  });
}
</script>
<template>
  <section class="dashboard-card">
    <div class="ts-heading">
      <h2>Zamknięcie dnia · {{ dayText(day) }}</h2>
      <strong>{{ duration(total) }} · {{ decimal(total) }} h</strong>
    </div>
    <p v-if="data.closedDays?.includes(day)" role="status">
      Dzień oznaczony jako sprawdzony. Historię nadal możesz edytować.
    </p>
    <p
      v-if="targetText"
      class="close-target"
      :class="{ over: targetDiffMinutes(total, target) > 0 }"
    >
      {{ targetText }}
    </p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="message" role="status">{{ message }}</p>
    <p v-if="active">
      Pomiar dla tego dnia nadal trwa. Zatrzymaj go przed zamknięciem dnia.
      <button
        class="today-button"
        :disabled="busy"
        @click="run(async () => emit('updated', await change('stop')))"
      >
        Zatrzymaj pomiar
      </button>
    </p>
    <h3>Co było robione?</h3>
    <div v-for="row in rows" :key="row.task.id" class="close-note">
      <label
        >{{ row.task.title }} · {{ duration(row.total)
        }}<textarea
          v-model="notes[row.task.id]"
          :aria-label="'Notatka: ' + row.task.title"
          maxlength="4000"
          :placeholder="'Uzupełnij opis pracy z ' + day"
        />
      </label>
    </div>
    <p v-if="!rows.length" class="report-note">Brak czasu w tym dniu.</p>
    <button
      v-if="rows.length"
      class="today-button"
      :disabled="busy"
      @click="saveNotes"
    >
      Zapisz notatki dnia
    </button>
    <h3>Przerwy między zapisanymi przedziałami</h3>
    <p v-for="gap in gaps" :key="gap[0]">
      {{ new Date(gap[0]).toLocaleTimeString("pl-PL") }} –
      {{ new Date(gap[1]).toLocaleTimeString("pl-PL") }} ·
      {{ duration(gap[1] - gap[0]) }}
    </p>
    <p class="hint">
      {{
        gaps.length
          ? "Przerwy mogą być prawidłowe. Jeśli brakuje pracy, dopisz czas w raporcie."
          : "Brak luk między zarejestrowanymi przedziałami."
      }}
      Nie oceniamy czasu przed pierwszym i po ostatnim wpisie.
    </p>
    <button class="today-button" @click="emit('report', day)">
      Otwórz raport i popraw czas
    </button>
    <template v-if="meetings.length">
      <h3>Spotkania</h3>
      <ul class="close-meetings" aria-label="Spotkania dnia">
        <li v-for="m in meetings" :key="m.event.id">
          <span class="mono">{{ m.time }}</span
          ><span class="close-meeting-title">{{ m.title }}</span>
          <span v-if="m.future" class="hint">Jeszcze się nie skończyło</span
          ><span
            v-else-if="m.overlaps"
            class="close-meeting-warning"
            role="note"
            >Nakłada się na zapisany pomiar</span
          ><button
            v-else
            class="today-button"
            :aria-label="'Dodaj jako czas: ' + m.title + ' ' + m.time"
            @click="addMeeting(m)"
          >
            Dodaj jako czas
          </button>
        </li>
      </ul>
      <p class="hint">
        Spotkanie nie dodaje się samo. „Dodaj jako czas” otwiera ręczne dodanie
        czasu z godzinami spotkania; zadanie wybierasz sam.
      </p>
      <ManualTime
        :tasks="activeTasks"
        :day="day"
        :preset="preset"
        remember-key="todo-track-meeting-task"
        @updated="emit('updated', $event)"
      />
    </template>
    <h3>Niedokończony plan</h3>
    <p>{{ unfinished.length }} zadań do przeniesienia na następny dzień.</p>
    <button
      class="today-button"
      :disabled="busy || !unfinished.length"
      @click="move"
    >
      Przenieś niedokończone na jutro
    </button>
    <h3>Raport TS</h3>
    <textarea
      ref="reportArea"
      class="ts-output"
      aria-label="Raport TS zamknięcia dnia"
      readonly
      :value="report"
    />
    <div class="planning-actions">
      <button
        class="today-button"
        @click="
          run(async () => {
            message = await copyReport(report, reportArea);
          })
        "
      >
        Kopiuj raport TS</button
      ><button
        class="today-button"
        :disabled="busy || active || day > dayKey(now)"
        @click="
          run(async () =>
            emit(
              'updated',
              await closeDay(day, !data.closedDays?.includes(day)),
            ),
          )
        "
      >
        {{
          data.closedDays?.includes(day)
            ? "Otwórz dzień ponownie"
            : "Oznacz dzień jako sprawdzony"
        }}
      </button>
    </div>
  </section>
</template>
