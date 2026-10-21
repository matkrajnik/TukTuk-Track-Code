<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Pencil,
} from "lucide-vue-next";
import ManualTime from "../ManualTime.vue";
import { duration, type Session, type Snapshot } from "../store";
import { dayBounds, dayKey, inDay, stamp } from "../time";
import {
  byTotal,
  dayRows,
  overtimeOf,
  rowsTotal,
  tsText,
} from "../report";
import TaskSummary from "../components/task/TaskSummary.vue";
import { dayTarget, hm, useSettings } from "../target";
import { copyReport } from "../clipboard";
import MultiDayReport from "../components/MultiDayReport.vue";
import MonthReport from "../components/MonthReport.vue";
import TsSendDialog from "../components/TsSendDialog.vue";
import { useTsSend } from "../composables/useTsSend";
import { useTsProjectMap } from "../composables/useTsProjectMap";
import { tsEntries } from "../timesheet";
import { openView } from "../navigation";

const props = defineProps<{
  data: Snapshot;
  now: number;
  busy: boolean;
  /** Licznik przejść „pokaż raport dnia” z innych widoków (App.vue). */
  openRequest?: number;
}>();
const day = defineModel<string>("day", { required: true });
const emit = defineEmits<{
  updated: [data: Snapshot];
  session: [session: Session];
}>();

const reportExpanded = ref(new Set<number>());
const tsVisible = ref(false);
const tsTextArea = ref<HTMLTextAreaElement | null>(null);
const tsCopyStatus = ref("");
const tsIncludeNotes = ref(true);
/** Tryb: raport jednego dnia, TS z dowolnie wybranych dni albo miesiąc. */
const mode = ref<"single" | "multi" | "month">("single");

const todayKey = computed(() => dayKey(props.now));
const report = computed(() =>
  dayRows(props.data, day.value, props.now).sort(byTotal),
);
const reportTotal = computed(() => rowsTotal(report.value));
/**
 * Nadgodziny dnia (B5): praca ponad normę dnia (cel dzienny; w dzień wolny
 * lub święto cała praca), ta sama reguła co podział w „Wyślij do TS” (B4).
 */
const settings = useSettings();
const reportOvertime = computed(() =>
  overtimeOf(reportTotal.value, day.value, settings.value),
);
const reportNorm = computed(() => dayTarget(settings.value, day.value));
/** Zmierzony czas zadań raportu (wszystkie dni) do planu i procentu na liście. */
const taskTimes = computed(() => {
  const ids = new Set(report.value.map((row) => row.task.id));
  const out = new Map<number, number>();
  for (const s of props.data.sessions)
    if (ids.has(s.taskId))
      out.set(
        s.taskId,
        (out.get(s.taskId) ?? 0) +
          Math.max(0, (s.endedAt ?? props.now) - s.startedAt),
      );
  return out;
});
const tsReport = computed(() =>
  tsText([day.value], props.data, props.now, {
    includeNotes: tsIncludeNotes.value,
  }),
);
const reportLabel = computed(() =>
  new Intl.DateTimeFormat("pl-PL", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(dayBounds(day.value)[0]),
);
/** Wysyłka do TS (1.8): przycisk, znacznik wysyłki i podgląd. */
const tsMap = useTsProjectMap();
const tsReportHours = computed(
  () => tsEntries(day.value, props.data, props.now, tsMap.value).totalHours,
);
const {
  account: tsAccount,
  enabled: tsEnabled,
  connected: tsConnected,
  marker: tsMarker,
  changed: tsChanged,
  markSent: tsMarkSent,
} = useTsSend(
  () => day.value,
  () => tsReportHours.value,
);
const tsOpen = ref(false);
watch(day, () => (tsOpen.value = false));
watch([day, tsVisible], () => {
  tsCopyStatus.value = "";
});
// Jawne przejście do raportu dnia z innego widoku (np. z Dashboardu)
// pokazuje raport jednego dnia. Zwykła zmiana dnia (np. po północy) nie
// zmienia trybu.
watch(
  () => props.openRequest,
  () => {
    mode.value = "single";
  },
);
function openDay(value: string) {
  day.value = value;
  mode.value = "single";
}

async function copyTsReport() {
  tsCopyStatus.value = "";
  tsCopyStatus.value = await copyReport(tsReport.value, tsTextArea.value);
}
function shiftDay(offset: number) {
  const d = new Date(dayBounds(day.value)[0]);
  d.setDate(d.getDate() + offset);
  day.value = dayKey(d.getTime());
}
function chooseDay(event: Event) {
  const value = (event.target as HTMLInputElement).value;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value) && Number.isFinite(dayBounds(value)[0]))
    day.value = value;
  else (event.target as HTMLInputElement).value = day.value;
}
function toggle(id: number) {
  if (reportExpanded.value.has(id)) reportExpanded.value.delete(id);
  else reportExpanded.value.add(id);
}
</script>

<template>
  <div class="page-heading">
    <h1>Raporty dzienne</h1>
    <div v-if="mode === 'single'" class="date-picker">
      <button
        class="icon-button"
        aria-label="Poprzedni dzień"
        @click="shiftDay(-1)"
      >
        <ChevronLeft :size="18" /></button
      ><input
        type="date"
        aria-label="Dzień raportu"
        :value="day"
        @change="chooseDay"
      /><button
        class="icon-button"
        aria-label="Następny dzień"
        @click="shiftDay(1)"
      >
        <ChevronRight :size="18" /></button
      ><button class="today-button" @click="day = todayKey">Dzisiaj</button>
      <button
        class="today-button"
        :aria-expanded="tsVisible"
        aria-controls="ts-report"
        @click="tsVisible = !tsVisible"
      >
        Raport TS
      </button>
    </div>
  </div>
  <div class="tabs journal-tabs" role="group" aria-label="Tryb raportu">
    <button
      :class="{ selected: mode === 'single' }"
      :aria-pressed="mode === 'single'"
      @click="mode = 'single'"
    >
      Jeden dzień</button
    ><button
      :class="{ selected: mode === 'multi' }"
      :aria-pressed="mode === 'multi'"
      @click="mode = 'multi'"
    >
      Wybrane dni</button
    ><button
      :class="{ selected: mode === 'month' }"
      :aria-pressed="mode === 'month'"
      @click="mode = 'month'"
    >
      Miesiąc
    </button>
  </div>
  <MonthReport
    v-if="mode === 'month'"
    :data="data"
    :now="now"
    @open="openDay"
  />
  <MultiDayReport
    v-else-if="mode === 'multi'"
    :data="data"
    :now="now"
    @open="openDay"
    @updated="emit('updated', $event)"
  />
  <template v-else>
    <section class="report-summary" aria-label="Podsumowanie dnia">
      <div>
        <p>{{ reportLabel }}</p>
        <h2>Przepracowany czas</h2>
        <p
          v-if="reportOvertime > 0"
          class="report-overtime"
          :title="
            reportNorm > 0
              ? 'Praca ponad normę dnia ' + hm(reportNorm)
              : 'Dzień wolny: cała praca to nadgodziny'
          "
        >
          Nadgodziny: <span class="mono">{{ hm(reportOvertime) }}</span>
        </p>
      </div>
      <strong class="mono report-total">{{ duration(reportTotal) }}</strong>
    </section>
    <ManualTime
      :tasks="data.tasks"
      :day="day"
      :sessions="data.sessions"
      @updated="emit('updated', $event)"
    />
    <p class="report-note">
      Czas przypisany do wybranego dnia. Przedziały przez północ są dzielone
      między dni.
    </p>
    <section
      v-if="tsVisible"
      id="ts-report"
      class="ts-report"
      aria-label="Raport TS"
    >
      <div class="ts-heading">
        <h2>Raport TS</h2>
        <div class="ts-heading-actions">
          <button class="today-button" @click="copyTsReport">
            Kopiuj raport</button
          ><button
            v-if="tsEnabled && tsConnected"
            class="today-button"
            :disabled="busy || day > todayKey"
            :title="
              day > todayKey
                ? 'Dnia z przyszłości nie można wysłać do TS'
                : undefined
            "
            @click="tsOpen = true"
          >
            Wyślij do TS
          </button>
        </div>
      </div>
      <p v-if="tsEnabled && tsAccount && !tsConnected" class="report-note">
        <button
          type="button"
          class="text-link"
          @click="openView({ view: 'settings', section: 'timesheet' })"
        >
          Połącz konto TS w Ustawieniach</button
        >, żeby wysłać raport do TS.
      </p>
      <p v-if="tsMarker" class="report-note ts-sent-marker">
        {{ tsMarker
        }}<span v-if="tsChanged" class="ts-sent-changed">
          · raport zmienił się od wysyłki</span
        >
      </p>
      <label class="inline-check"
        ><input v-model="tsIncludeNotes" type="checkbox" />Dołącz notatki z tego
        dnia</label
      >
      <p class="report-note">
        Godziny dziesiętne, np. 1 godz. 30 min = 1,50 h. Zaokrąglenie do dwóch
        miejsc; suma liczona z pełnego czasu.
      </p>
      <p
        v-if="
          report.some((row) =>
            row.sessions.some((session) => session.endedAt === null),
          )
        "
        class="report-note"
      >
        Trwający pomiar jest uwzględniony do bieżącej chwili.
      </p>
      <textarea
        ref="tsTextArea"
        aria-label="Treść raportu TS"
        :value="tsReport"
        readonly
        spellcheck="false"
        @focus="($event.target as HTMLTextAreaElement).select()"
      />
      <p class="ts-copy-status" role="status">{{ tsCopyStatus }}</p>
      <TsSendDialog
        v-if="tsOpen"
        :data="data"
        :days="[day]"
        :now="now"
        @close="tsOpen = false"
        @sent="tsMarkSent"
        @updated="emit('updated', $event)"
      />
    </section>
    <div class="report-list">
      <article v-for="row in report" :key="row.task.id" class="report-card">
        <button
          class="report-expand"
          :aria-expanded="reportExpanded.has(row.task.id)"
          :aria-label="'Przedziały raportu: ' + row.task.title"
          :aria-controls="'report-' + row.task.id"
          @click="toggle(row.task.id)"
        >
          <TaskSummary
            :task="row.task"
            :running="row.sessions.some((s) => s.endedAt === null)"
            :now="now"
            :today-key="todayKey"
            :task-time="taskTimes.get(row.task.id) ?? 0"
            :note="row.task.details?.notes?.[day]" /><strong class="mono">{{
            duration(row.total)
          }}</strong
          ><ChevronDown
            :size="16"
            :class="{ rotated: reportExpanded.has(row.task.id) }"
          />
        </button>
        <div
          v-if="reportExpanded.has(row.task.id)"
          :id="'report-' + row.task.id"
          class="task-details"
        >
          <div class="details-heading">
            <span>Pełny przedział</span><span>Czas w tym dniu</span>
          </div>
          <div
            v-for="session in row.sessions"
            :key="session.id"
            class="session-row"
          >
            <div class="interval">
              <span>{{ stamp(session.startedAt) }}</span
              ><span class="interval-arrow">→</span
              ><span>{{
                session.endedAt === null ? "W trakcie" : stamp(session.endedAt)
              }}</span>
            </div>
            <span class="mono session-duration">{{
              duration(inDay(session, day, now))
            }}</span
            ><button
              class="icon-button"
              :aria-label="'Edytuj przedział ' + session.id"
              :disabled="busy || session.endedAt === null"
              :title="
                session.endedAt === null
                  ? 'Zatrzymaj pomiar przed edycją'
                  : 'Edytuj przedział'
              "
              @click="emit('session', session)"
            >
              <Pencil :size="14" /></button
            ><button
              v-if="session.endedAt === null"
              class="icon-button"
              :aria-label="'Popraw start przedziału ' + session.id"
              title="Popraw godzinę startu trwającego pomiaru"
              :disabled="busy"
              @click="emit('session', session)"
            >
              <Clock3 :size="14" />
            </button>
          </div>
        </div>
      </article>
      <p v-if="!report.length" class="empty-state">
        Brak zarejestrowanej pracy w tym dniu.
      </p>
    </div>
  </template>
</template>
