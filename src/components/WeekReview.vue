<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Bot } from "lucide-vue-next";
import AiPreview from "./AiPreview.vue";
import {
  aiWeekDraft,
  aiWeekPreview,
  usePlanAssistEnabled,
  withDraft,
} from "../aiplan";
import {
  duration,
  saveWeekNote,
  weekNote,
  type Snapshot,
  type Task,
} from "../store";
import { dayBounds, dayKey, dayText, inDay } from "../time";
import { dayCapacity } from "../capacity";
import { isRecurring } from "../recurrence";
import { biggestMisses, estimateMs, percent, taskTotals } from "../estimates";
import { dayDistraction } from "../report";
import { useFocusBlocks } from "../focus";
import { agentName } from "../agenttime";
import { useMeetings } from "../composables/useMeetings";
import {
  sessionsSignature,
  useAgentTime,
} from "../composables/useAgentTime";
import {
  dayTarget,
  hhmm,
  isWorkDay,
  signedDiff,
  useSettings,
  weekDays,
} from "../target";

/**
 * Plan i historia → Tydzień → Przegląd (PLAN_1.7.md, W2): czas kontra cel
 * i spotkania, plan kontra wykonanie, rozjazdy szacunków, czas agentów,
 * rozproszenie dzień po dniu i „Wnioski” zapisywane w `week_notes`.
 * Widok tylko czyta dane; zapisuje jedynie wnioski.
 */
const props = defineProps<{ data: Snapshot; now: number; day: string }>();
const emit = defineEmits<{ report: [day: string] }>();
const settings = useSettings();
const days = computed(() => weekDays(props.day));
const monday = computed(() => days.value[0]);
const range = (): [number, number] => [
  dayBounds(days.value[0])[0],
  dayBounds(days.value[6])[1],
];
const calendar = useMeetings(range);
const { time: agents } = useAgentTime(range, () =>
  sessionsSignature(props.data.sessions),
);
const { blocks } = useFocusBlocks(range);

const taskMap = computed(
  () =>
    new Map<number, Task>(
      [...props.data.tasks, ...(props.data.trash ?? [])].map((t) => [t.id, t]),
    ),
);
const dayTotal = (key: string) =>
  props.data.sessions.reduce((sum, s) => sum + inDay(s, key, props.now), 0);

// --- Czas kontra cel i spotkania ---
const timeRows = computed(() =>
  days.value.map((key) => {
    const target = dayTarget(settings.value, key);
    const cap = dayCapacity(target, calendar.events.value, key);
    return {
      key,
      total: dayTotal(key),
      target,
      meetings: cap.meetings,
      capacity: cap.capacity,
      off: !!cap.off,
    };
  }),
);
const shownTimeRows = computed(() =>
  timeRows.value.filter((r) => r.target || r.total || r.meetings),
);
const sum = (pick: (r: (typeof timeRows.value)[number]) => number) =>
  timeRows.value.reduce((total, r) => total + pick(r), 0);
const weekTotal = computed(() => sum((r) => r.total));
const weekGoal = computed(() => sum((r) => r.target));
const weekMeetings = computed(() => sum((r) => r.meetings));
const weekCapacity = computed(() => sum((r) => r.capacity));

// --- Plan kontra wykonanie ---
/** Zadania z pracą w tym tygodniu. */
const worked = computed(() => {
  const ids = new Set<number>();
  for (const s of props.data.sessions)
    if (days.value.some((key) => inDay(s, key, props.now) > 0))
      ids.add(s.taskId);
  return ids;
});
const plan = computed(() => {
  const first = days.value[0];
  const last = days.value[6];
  // Wzorce cykli mają w `plannedDay` tylko początek cyklu; wystąpienia liczą się.
  const tasks = props.data.tasks.filter(
    (t) => !isRecurring(t.details) && (t.done || !t.archived),
  );
  const planned = tasks.filter((t) => {
    const d = t.details?.plannedDay ?? "";
    return d >= first && d <= last;
  });
  // Praca w tym tygodniu, a plan już na później: przeniesione dalej.
  const moved = tasks.filter(
    (t) =>
      !t.done &&
      (t.details?.plannedDay ?? "") > last &&
      worked.value.has(t.id),
  );
  return {
    planned: planned.length + moved.length,
    done: planned.filter((t) => t.done),
    open: planned.filter((t) => !t.done),
    moved,
  };
});

// --- Rozjazdy szacunków ---
const misses = computed(() => {
  const totals = taskTotals(props.data, props.now);
  const rows = props.data.tasks
    .filter((t) => worked.value.has(t.id) && estimateMs(t) > 0)
    .map((task) => {
      const time = totals.get(task.id) ?? { total: 0, last: 0 };
      const estimate = estimateMs(task);
      return {
        task,
        estimate,
        actual: time.total,
        ratio: time.total / estimate,
        lastDay: dayKey(time.last),
      };
    })
    // Otwarte zadanie z dużym szacunkiem i małą pracą nie jest jeszcze
    // rozjazdem; otwarte liczy się dopiero, gdy już przekroczyło szacunek.
    .filter((row) => row.task.done || row.actual > row.estimate);
  return biggestMisses(rows, 5);
});

// --- Czas agentów ---
const agentTotal = computed(
  () =>
    agents.value.totals.reduce((s, t) => s + t.agentMs, 0) +
    agents.value.unmatchedMs,
);
const agentParallel = computed(() =>
  agents.value.totals.reduce((s, t) => s + t.parallelMs, 0),
);
const agentTop = computed(() =>
  [...agents.value.totals]
    .sort((a, b) => b.agentMs - a.agentMs || a.taskId - b.taskId)
    .slice(0, 5),
);
const agentNames = computed(() =>
  [...new Set(agents.value.periods.map((p) => agentName(p.agent)))].join(", "),
);

// --- Rozproszenie dzień po dniu ---
const distraction = computed(() =>
  days.value
    .map((key) => ({
      key,
      ...dayDistraction(props.data.sessions, key, props.now, blocks.value),
    }))
    .filter((d) => d.measurements > 0 || isWorkDay(settings.value, d.key)),
);

// --- Wnioski ---
const notes = ref("");
const saved = ref("");
const notesStatus = ref("");
const notesError = ref("");
const saving = ref(false);
let request = 0;
watch(
  monday,
  async (week) => {
    const id = ++request;
    notesStatus.value = "";
    notesError.value = "";
    try {
      const text = await weekNote(week);
      if (id !== request) return;
      notes.value = text;
      saved.value = text;
    } catch (e) {
      if (id !== request) return;
      notes.value = "";
      saved.value = "";
      notesError.value = String(e);
    }
  },
  { immediate: true },
);
const dirty = computed(() => notes.value.trim() !== saved.value);
/**
 * „Szkic wniosków” (A6): jedno wywołanie modelu na kliknięcie. Szkic trafia do pola
 * „Wnioski” jako tekst do edycji (pod tym, co już tam jest); zapisuje dopiero
 * „Zapisz wnioski”.
 */
const assistEnabled = usePlanAssistEnabled();
const drafting = ref(false);
const draftError = ref("");
const draftMessage = ref("");
const draftPreview = computed(() => () => aiWeekPreview(monday.value));
// Nowy tydzień zaczyna bez komunikatów po szkicu poprzedniego.
watch(monday, () => {
  draftError.value = "";
  draftMessage.value = "";
});
async function draftNotes() {
  if (drafting.value) return;
  const week = monday.value;
  drafting.value = true;
  draftError.value = "";
  draftMessage.value = "";
  try {
    const result = await aiWeekDraft(week);
    // Tydzień zmienił się w trakcie wywołania: szkic nie pasuje do widoku.
    if (week !== monday.value) return;
    if (!result.text) {
      draftMessage.value = result.message;
      return;
    }
    notes.value = withDraft(notes.value, result.text);
    notesStatus.value = "";
    draftMessage.value =
      "Szkic asystenta jest w polu poniżej. Popraw go i zapisz — dopiero wtedy trafi do wniosków.";
  } catch (e) {
    if (week === monday.value)
      draftError.value = String(e instanceof Error ? e.message : e);
  } finally {
    drafting.value = false;
  }
}
async function saveNotes() {
  if (saving.value) return;
  saving.value = true;
  notesError.value = "";
  notesStatus.value = "";
  const week = monday.value;
  try {
    const text = await saveWeekNote(week, notes.value);
    if (week !== monday.value) return;
    notes.value = text;
    saved.value = text;
    notesStatus.value = "Zapisano";
  } catch (e) {
    notesError.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <section class="report-summary">
    <div>
      <p>{{ dayText(days[0]) }} — {{ dayText(days[6]) }}</p>
      <h2>Przegląd tygodnia</h2>
    </div>
    <div>
      <strong class="mono report-total">{{ duration(weekTotal) }}</strong>
      <p v-if="weekGoal" class="hint">
        Cel tygodnia {{ hhmm(weekGoal) }} · {{ signedDiff(weekTotal, weekGoal) }}
      </p>
    </div>
  </section>
  <div class="dashboard-columns even dashboard-top">
    <section class="dashboard-card" aria-label="Czas kontra cel i spotkania">
      <h2>Czas kontra cel i spotkania</h2>
      <p class="hint">
        Zmierzone {{ hhmm(weekTotal) }} · cel {{ hhmm(weekGoal) }} · spotkania
        {{ hhmm(weekMeetings) }} · zostawało na pracę
        {{ hhmm(weekCapacity) }}
      </p>
      <table class="estimate-table week-review-table">
        <thead>
          <tr>
            <th>Dzień</th>
            <th>Zmierzone</th>
            <th>Cel</th>
            <th>Spotkania</th>
            <th>Różnica</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in shownTimeRows" :key="row.key">
            <td>
              <button class="text-link" @click="emit('report', row.key)">
                {{ dayText(row.key) }}
              </button>
            </td>
            <td class="mono">{{ hhmm(row.total) }}</td>
            <td class="mono">{{ row.target ? hhmm(row.target) : "wolne" }}</td>
            <td class="mono">
              {{ row.off ? "poza biurem" : hhmm(row.meetings) }}
            </td>
            <td class="mono">
              {{ row.target ? signedDiff(row.total, row.target) : "" }}
            </td>
          </tr>
        </tbody>
      </table>
    </section>
    <section class="dashboard-card" aria-label="Plan kontra wykonanie">
      <h2>Plan kontra wykonanie</h2>
      <dl class="dashboard-status-grid">
        <div>
          <dt>Zaplanowane na tydzień</dt>
          <dd><strong>{{ plan.planned }}</strong></dd>
        </div>
        <div>
          <dt>Ukończone</dt>
          <dd><strong>{{ plan.done.length }}</strong></dd>
        </div>
        <div>
          <dt>Przeniesione dalej</dt>
          <dd><strong>{{ plan.moved.length }}</strong></dd>
        </div>
        <div>
          <dt>Niedokończone w planie</dt>
          <dd><strong>{{ plan.open.length }}</strong></dd>
        </div>
      </dl>
      <p class="report-note">
        Przeniesione dalej: zadania z pracą w tym tygodniu, których plan jest
        już na późniejszy dzień. Niedokończone: plan na ten tydzień, zadanie
        otwarte.
      </p>
      <div
        v-for="task in plan.moved"
        :key="'moved-' + task.id"
        class="journal-task-row"
      >
        <span>{{ task.title }}</span
        ><span class="hint"
          >Plan {{ dayText(task.details!.plannedDay) }}</span
        >
      </div>
      <div
        v-for="task in plan.open"
        :key="'open-' + task.id"
        class="journal-task-row"
      >
        <span>{{ task.title }}</span
        ><span class="hint"
          >Niedokończone · {{ dayText(task.details!.plannedDay) }}</span
        >
      </div>
    </section>
  </div>
  <div class="dashboard-columns even dashboard-top">
    <section class="dashboard-card" aria-label="Rozjazdy szacunków">
      <h2>Największe rozjazdy szacunków</h2>
      <p v-if="!misses.length" class="report-note">
        Brak zadań z szacunkiem i pracą w tym tygodniu.
      </p>
      <table v-else class="estimate-table week-review-table">
        <thead>
          <tr>
            <th>Zadanie</th>
            <th>Szacunek</th>
            <th>Faktycznie</th>
            <th>Wykonanie</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in misses" :key="row.task.id">
            <td>{{ row.task.title }}</td>
            <td class="mono">{{ hhmm(row.estimate) }}</td>
            <td class="mono">{{ hhmm(row.actual) }}</td>
            <td
              class="mono"
              :class="{ 'estimate-over': row.actual > row.estimate }"
            >
              {{ percent(row.actual, row.estimate) }}%
            </td>
          </tr>
        </tbody>
      </table>
    </section>
    <section class="dashboard-card" aria-label="Czas agentów w tygodniu">
      <h2>Czas agentów</h2>
      <p v-if="!agentTotal" class="report-note">
        Agenci AI nie pracowali w tym tygodniu.
      </p>
      <template v-else>
        <p class="hint">
          Razem {{ hhmm(agentTotal) }} · równolegle z Twoim pomiarem
          {{ hhmm(agentParallel) }}<template v-if="agentNames">
            · {{ agentNames }}</template
          >. Nie wlicza się do celu ani raportu TS.
        </p>
        <div
          v-for="row in agentTop"
          :key="row.taskId"
          class="dashboard-count"
        >
          <span>{{ taskMap.get(row.taskId)?.title ?? "Zadanie usunięte" }}</span
          ><strong class="mono">{{ hhmm(row.agentMs) }}</strong>
        </div>
        <div v-if="agents.unmatchedMs" class="dashboard-count">
          <span>Bez zadania</span
          ><strong class="mono">{{ hhmm(agents.unmatchedMs) }}</strong>
        </div>
      </template>
    </section>
  </div>
  <section
    class="dashboard-card dashboard-top"
    aria-label="Rozproszenie dzień po dniu"
  >
    <h2>Rozproszenie dzień po dniu</h2>
    <p class="report-note">
      Przełączenie to start innego zadania w ciągu 15 minut po poprzednim
      pomiarze. Pomiar jest nieprzerwany, dopóki mierzysz to samo zadanie.
    </p>
    <table class="estimate-table week-review-table">
      <thead>
        <tr>
          <th>Dzień</th>
          <th>Przełączenia</th>
          <th>Mediana pomiaru</th>
          <th>Najdłuższy pomiar</th>
          <th>Bloki skupienia</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in distraction" :key="row.key">
          <td>{{ dayText(row.key) }}</td>
          <td class="mono">{{ row.switches }}</td>
          <td class="mono">{{ hhmm(row.medianMs) }}</td>
          <td class="mono">{{ hhmm(row.longestMs) }}</td>
          <td class="mono">{{ row.focusDone }} / {{ row.focusStarted }}</td>
        </tr>
      </tbody>
    </table>
  </section>
  <section class="dashboard-card dashboard-top" aria-label="Wnioski">
    <h2>Wnioski</h2>
    <template v-if="assistEnabled">
      <div class="week-notes-assist">
        <button
          type="button"
          class="secondary-button"
          :disabled="drafting || saving"
          @click="draftNotes"
        >
          <Bot :size="14" />{{ drafting ? "Piszę szkic…" : "Szkic wniosków" }}
        </button>
      </div>
      <AiPreview :load="draftPreview" />
      <p v-if="draftError" class="error" role="alert">{{ draftError }}</p>
      <p v-if="draftMessage" class="hint" role="status">{{ draftMessage }}</p>
    </template>
    <form class="week-notes" @submit.prevent="saveNotes">
      <textarea
        v-model="notes"
        rows="5"
        maxlength="10000"
        aria-label="Wnioski z tygodnia"
        placeholder="Co poszło dobrze, co zmienić w przyszłym tygodniu?"
        @input="notesStatus = ''"
      />
      <div class="planning-actions">
        <button class="primary-button" :disabled="saving || !dirty">
          Zapisz wnioski
        </button>
        <span v-if="notesStatus" class="hint" role="status">{{
          notesStatus
        }}</span>
      </div>
      <p v-if="notesError" class="error" role="alert">{{ notesError }}</p>
    </form>
  </section>
</template>
