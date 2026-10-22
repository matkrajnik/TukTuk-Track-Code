<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  Bot,
  CalendarArrowUp,
  Copy,
  FileText,
  Plus,
  Save,
  Send,
  Square,
} from "lucide-vue-next";
import {
  change,
  closeDay,
  duration,
  MIN_GAP_MS,
  planTasks,
  saveDayNotes,
  isManualMeeting,
  meetingOccupies,
  meetingTitle,
  type Snapshot,
} from "./store";
import { clock, dayBounds, dayKey, dayText, inDay } from "./time";
import { nextWorkDay, planAll } from "./review";
import { dayRows, decimalHours as decimal, rowsTotal, tsText } from "./report";
import { copyReport } from "./clipboard";
import { dayGoal } from "./capacity";
import { useMeetings } from "./composables/useMeetings";
import ManualTime from "./ManualTime.vue";
import DayControls from "./components/DayControls.vue";
import { dayHoursLine, dayMarkOf } from "./days";
import TsSendDialog from "./components/TsSendDialog.vue";
import { useTsSend } from "./composables/useTsSend";
import { useTsProjectMap } from "./composables/useTsProjectMap";
import { tsEntries } from "./timesheet";
import { openView } from "./navigation";
import { openChat, useChatEnabled } from "./aichat";
import { appendedNote } from "./aiproposals";
import { useDayAssistEnabled, type TsDescriptionHint } from "./aiday";
import DayAssistant from "./components/DayAssistant.vue";
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
/** Godziny dnia z „Rozpocznij / Zakończ dzień” (B13); nadgodziny liczą się z pomiarów. */
const dayLine = computed(() => dayHoursLine(dayMarkOf(props.data, props.day)));
const settings = useSettings();
/** Czat (A3): „Zapytaj asystenta” z dniem jako kontekstem. */
const chatEnabled = useChatEnabled();
/** Asystent Zamknięcia dnia (A5): sekcja „Asystent” i proponowane opisy TS. */
const assistEnabled = useDayAssistEnabled();
const tsHints = ref<TsDescriptionHint[]>([]);
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
      manual: isManualMeeting(e),
      time: `${clock(e.start)}–${clock(e.end)}`,
      overlaps: all.some(
        (s) => s.startedAt < e.end && (s.endedAt === null || s.endedAt > e.start),
      ),
      future: e.end > props.now,
    }));
});
/** Żądanie otwarcia „Dodaj czas ręcznie” z godzinami spotkania. */
const preset = ref<{
  start: number;
  end: number;
  key: string;
  taskId: number | null;
} | null>(null);
function addMeeting(m: (typeof meetings.value)[number]) {
  preset.value = {
    start: m.event.start,
    end: m.event.end,
    key: m.event.id + ":" + Date.now(),
    taskId: m.event.taskId ?? null,
  };
}
const activeTasks = computed(() => props.data.tasks.filter((t) => !t.archived));
/** Informacja o celu dnia; nie blokuje zamknięcia. */
const targetText = computed(() => targetMessage(total.value, target.value));
/** Niedokończony plan idzie na następny dzień roboczy (piątek → poniedziałek). */
const nextDay = computed(() => nextWorkDay(settings.value, props.day));
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
    if (cursor && a - cursor >= MIN_GAP_MS) result.push([cursor, a]);
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
/** Wysyłka do TS (1.8): przycisk, znacznik wysyłki i podgląd. */
const tsMap = useTsProjectMap();
const tsReportHours = computed(
  () => tsEntries(props.day, props.data, props.now, tsMap.value).totalHours,
);
const {
  account: tsAccount,
  enabled: tsEnabled,
  connected: tsConnected,
  marker: tsMarker,
  changed: tsChanged,
  markSent: tsMarkSent,
} = useTsSend(
  () => props.day,
  () => tsReportHours.value,
);
const tsOpen = ref(false);
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
/** Zmienione notatki dnia jednym wywołaniem `save_day_notes` (B6). */
async function saveNotes() {
  await run(async () => {
    const entries = rows.value
      .filter(
        (r) =>
          (r.task.details?.notes?.[props.day] ?? "") !==
          (notes.value[r.task.id] ?? ""),
      )
      .map((r) => ({ id: r.task.id, note: notes.value[r.task.id] ?? "" }));
    if (entries.length) emit("updated", await saveDayNotes(props.day, entries));
    message.value = "Zapisano notatki.";
  });
}
/**
 * „Dodaj do notatki” z asystenta (A5): dopisuje zdanie do notatki dnia zadania
 * (do pola z Zamknięcia dnia, więc razem z niezapisanymi zmianami tego pola) i
 * zapisuje je jednym `save_day_notes`. Przy błędzie pola wracają do poprzednich
 * wartości, a błąd trafia do karty asystenta.
 */
async function appendNotes(items: { taskId: number; text: string }[]) {
  const before = { ...notes.value };
  for (const item of items)
    notes.value[item.taskId] = appendedNote(notes.value[item.taskId], item.text);
  const ids = [...new Set(items.map((i) => i.taskId))];
  try {
    emit(
      "updated",
      await saveDayNotes(
        props.day,
        ids.map((id) => ({ id, note: notes.value[id] })),
      ),
    );
    message.value = "Dopisano do notatek dnia.";
  } catch (e) {
    notes.value = before;
    throw e;
  }
}
/**
 * „Przenieś niedokończone” jak „Wszystkie na dziś” w Rano (B6): `planTasks`
 * zmienia tylko dzień planu i kolejność (na końcu planu dnia docelowego),
 * więc nie cofa równoległej zmiany innych pól zadania.
 */
async function move() {
  await run(async () => {
    if (!unfinished.value.length) return;
    emit(
      "updated",
      await planTasks(planAll(props.data.tasks, unfinished.value, nextDay.value)),
    );
  });
}
</script>
<template>
  <section class="dashboard-card">
    <div class="ts-heading">
      <h2>Zamknięcie dnia · {{ dayText(day) }}</h2>
      <strong>{{ duration(total) }} · {{ decimal(total) }} h</strong>
    </div>
    <p v-if="dayLine" class="day-hours">{{ dayLine }}</p>
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
    <p v-if="chatEnabled" class="close-assistant">
      <button
        class="today-button"
        aria-label="Zapytaj asystenta o ten dzień"
        title="Otwórz czat asystenta z tym dniem jako kontekstem"
        @click="openChat({ day })"
      >
        <Bot :size="12" />Zapytaj asystenta
      </button>
    </p>
    <p v-if="active">
      Pomiar dla tego dnia nadal trwa. Zatrzymaj go przed zamknięciem dnia.
      <button
        class="today-button"
        :disabled="busy"
        @click="run(async () => emit('updated', await change('stop')))"
      >
        <Square :size="12" />Zatrzymaj pomiar
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
          :placeholder="'Uzupełnij opis pracy z ' + dayText(day)"
        />
      </label>
    </div>
    <p v-if="!rows.length" class="report-note">Brak czasu w tym dniu.</p>
    <button
      v-if="rows.length"
      class="secondary-button close-step-action"
      :disabled="busy"
      @click="saveNotes"
    >
      <Save :size="14" />Zapisz notatki dnia
    </button>
    <h3>Przerwy między zapisanymi przedziałami</h3>
    <p v-for="gap in gaps" :key="gap[0]">
      {{ clock(gap[0], true) }} –
      {{ clock(gap[1], true) }} ·
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
      <FileText :size="14" />Otwórz raport i popraw czas
    </button>
    <template v-if="meetings.length">
      <h3>Spotkania</h3>
      <ul class="close-meetings" aria-label="Spotkania dnia">
        <li v-for="m in meetings" :key="m.event.id">
          <span class="mono">{{ m.time }}</span
          ><span class="close-meeting-title"
            >{{ m.title
            }}<template v-if="m.manual">
              <span class="meeting-manual">ręczne</span></template
            ></span
          >
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
            <Plus :size="14" />Dodaj jako czas
          </button>
        </li>
      </ul>
      <p class="hint">
        Spotkanie nie dodaje się samo. „Dodaj jako czas” otwiera ręczne dodanie
        czasu z godzinami spotkania; zadanie wybierasz sam (ręczne spotkanie
        z zadaniem podpowiada je od razu).
      </p>
      <ManualTime
        :tasks="activeTasks"
        :day="day"
        :sessions="data.sessions"
        :preset="preset"
        remember-key="todo-track-meeting-task"
        @updated="emit('updated', $event)"
      />
    </template>
    <DayAssistant
      v-if="assistEnabled"
      :data="data"
      :day="day"
      :append-notes="appendNotes"
      @updated="emit('updated', $event)"
      @hints="tsHints = $event"
    />
    <h3>Niedokończony plan</h3>
    <p>
      {{ unfinished.length }} zadań do przeniesienia na następny dzień roboczy
      ({{ dayText(nextDay) }}).
    </p>
    <button
      class="secondary-button close-step-action"
      :disabled="busy || !unfinished.length"
      @click="move"
    >
      <CalendarArrowUp :size="14" />Przenieś niedokończone na
      {{ dayText(nextDay) }}
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
        class="secondary-button"
        @click="
          run(async () => {
            message = await copyReport(report, reportArea);
          })
        "
      >
        <Copy :size="14" />Kopiuj raport TS</button
      ><button
        v-if="tsEnabled && tsConnected"
        class="secondary-button"
        :disabled="busy || day > dayKey(now)"
        :title="
          day > dayKey(now)
            ? 'Dnia z przyszłości nie można wysłać do TS'
            : undefined
        "
        @click="tsOpen = true"
      >
        <Send :size="14" />Wyślij do TS</button
      ><button
        class="primary-button"
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
    <!-- B13: „Zakończ dzień” / „Wznów dzień” pod „Oznacz dzień jako sprawdzony”,
         który oznacza zgodność historii, a nie koniec pracy. -->
    <DayControls
      v-if="day === dayKey(now)"
      class="close-day-controls"
      :data="data"
      :now="now"
      @updated="emit('updated', $event)"
    />
    <p v-if="tsEnabled && tsAccount && !tsConnected" class="hint">
      <button
        type="button"
        class="text-link"
        @click="openView({ view: 'settings', section: 'timesheet' })"
      >
        Połącz konto TS w Ustawieniach</button
      >, żeby wysłać raport do TS.
    </p>
    <p v-if="tsMarker" class="hint ts-sent-marker">
      {{ tsMarker
      }}<span v-if="tsChanged" class="ts-sent-changed">
        · raport zmienił się od wysyłki</span
      >
    </p>
    <TsSendDialog
      v-if="tsOpen"
      :data="data"
      :days="[day]"
      :now="now"
      :hints="tsHints"
      @close="tsOpen = false"
      @sent="tsMarkSent"
      @updated="emit('updated', $event)"
    />
  </section>
</template>
