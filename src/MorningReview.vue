<script setup lang="ts">
import { computed, ref } from "vue";
import { TriangleAlert } from "lucide-vue-next";
import {
  change,
  meetingOccupies,
  meetingTitle,
  planTask,
  planTasks,
  type Snapshot,
  type Task,
} from "./store";
import { writeSettings } from "./settings";
import { offerAfterDone } from "./jiramove";
import { issueRef } from "./issue";
import { clock, dayBounds, dayKey, dayText } from "./time";
import { plannedTasks } from "./dayplan";
import { dayCapacity } from "./capacity";
import { corrections, plannedMs } from "./estimates";
import { useMeetings } from "./composables/useMeetings";
import { dayTarget, hhmm, hm, targetHoliday, useSettings } from "./target";
import {
  dueTasks,
  newJiraTasks,
  nextOrder,
  nextWorkDay,
  planAll,
  previousWorkDay,
  reviewSince,
  unfinishedTasks,
} from "./review";

/**
 * Plan i historia → Rano („Przegląd dnia”, PLAN_1.7.md P2). Wszystko dotyczy
 * dzisiejszego dnia. Zmiany planu robią tylko przyciski; timer nie zmienia się
 * nigdy (poza „Ukończ”, które jak wszędzie zatrzymuje pomiar tego zadania).
 */
const props = defineProps<{ data: Snapshot; now: number }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const settings = useSettings();
const busy = ref(false),
  error = ref(""),
  message = ref("");

const today = computed(() => dayKey(props.now));
const previous = computed(() => previousWorkDay(settings.value, today.value));
const next = computed(() => nextWorkDay(settings.value, today.value));
const reviewed = computed(() => settings.value.morningReviewDay === today.value);

const unfinished = computed(() =>
  unfinishedTasks(props.data.tasks, today.value),
);
const due = computed(() =>
  dueTasks(
    props.data.tasks,
    today.value,
    new Set(unfinished.value.map((t) => t.id)),
  ),
);
const since = computed(() => reviewSince(settings.value, today.value));
const fresh = computed(() =>
  newJiraTasks(
    props.data.tasks,
    since.value,
    today.value,
    new Set([...unfinished.value, ...due.value].map((t) => t.id)),
  ),
);

/** Krok 4: plan dnia w ręcznej kolejności, spotkania i pojemność. */
const plan = computed(() => plannedTasks(props.data.tasks, today.value));
const calendar = useMeetings(() => dayBounds(today.value));
const meetings = computed(() => {
  const [from, to] = dayBounds(today.value);
  return calendar.events.value
    .filter((e) => meetingOccupies(e) && !e.allDay && e.start < to && e.end > from)
    .sort((a, b) => a.start - b.start)
    .map((e) => ({
      id: e.id,
      time: `${clock(Math.max(from, e.start))}–${clock(Math.min(to, e.end))}`,
      title: meetingTitle(e, settings.value.calendarShowTitles),
      tentative: e.showAs === "tentative",
    }));
});
/** Mnożniki poprawki szacunków; liczone raz na minutę. */
const minute = computed(() => Math.floor(props.now / 60_000) * 60_000);
const correctionOf = computed(() => corrections(props.data, minute.value));
const capacity = computed(() => {
  const goal = dayTarget(settings.value, today.value);
  const cap = dayCapacity(goal, calendar.events.value, today.value);
  const of = settings.value.planWithCorrection ? correctionOf.value : undefined;
  const estimate = plan.value.reduce((sum, t) => sum + plannedMs(t, of), 0);
  const free = cap.off !== null || goal <= 0;
  const scale = free ? 0 : goal;
  const meetingsWidth = scale ? Math.min(100, (cap.meetings / scale) * 100) : 0;
  const load = cap.capacity > 0 ? estimate / cap.capacity : estimate ? 2 : 0;
  return {
    ...cap,
    goal,
    estimate,
    free,
    meetingsWidth,
    width: scale
      ? Math.min(100 - meetingsWidth, (estimate / scale) * 100)
      : estimate
        ? 100
        : 0,
    load,
    level: load > 1 ? "over" : load >= 0.9 ? "near" : "ok",
    text: free
      ? (targetHoliday(settings.value, today.value) ??
        (cap.off
          ? "Poza biurem: " +
            meetingTitle(cap.off, settings.value.calendarShowTitles)
          : "Dzień wolny"))
      : `Cel ${hm(goal)} · spotkania ${hm(cap.meetings)} · zostaje ${hm(cap.capacity)}`,
  };
});

async function run(action: () => Promise<Snapshot | void>, done = "") {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    const result = await action();
    if (result) emit("updated", result);
    message.value = done;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
const toDay = (task: Task, day: string) =>
  run(() => planTask(task.id, day, day ? nextOrder(props.data.tasks, day) : 0));
const finish = (task: Task) =>
  run(async () => {
    const data = await change("toggle", task.id);
    if (!task.done) void offerAfterDone(task);
    return data;
  });
const allToday = () =>
  run(
    () =>
      planTasks(planAll(props.data.tasks, unfinished.value, today.value)),
    "Przeniesiono niedokończone na dziś.",
  );
const startDay = () =>
  run(async () => {
    await writeSettings({
      morningReviewDay: today.value,
      morningReviewAt: props.now,
    });
  }, "Dzień rozpoczęty. Miłej pracy!");

const dueText = (task: Task) =>
  task.details!.dueDay === today.value
    ? "Termin dziś"
    : "Po terminie · " + dayText(task.details!.dueDay);
const planText = (task: Task) =>
  task.details?.plannedDay === today.value
    ? "W planie dnia"
    : task.details?.plannedDay
      ? "Plan " + dayText(task.details.plannedDay)
      : "Do zaplanowania";
const reviewedAt = computed(() =>
  settings.value.morningReviewAt > 0
    ? clock(settings.value.morningReviewAt)
    : "",
);
</script>

<template>
  <section class="dashboard-card morning-review" aria-label="Przegląd dnia">
    <div class="ts-heading">
      <h2>Przegląd dnia · {{ dayText(today) }}</h2>
      <span v-if="reviewed" class="hint" role="status"
        >Dzień rozpoczęty{{ reviewedAt ? " o " + reviewedAt : "" }}</span
      >
    </div>
    <p class="hint">
      Przejrzyj niedokończone, terminy i nowe zgłoszenia, a potem kliknij
      „Zaczynam dzień”. Nic nie zmienia się samo, a timer działa tylko po Twoim
      kliknięciu Start.
    </p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="message" role="status">{{ message }}</p>

    <section
      class="morning-step"
      aria-label="Niedokończone z poprzedniego dnia roboczego"
    >
      <div class="ts-heading">
        <h3>
          1. Niedokończone z poprzedniego dnia roboczego ({{ dayText(previous) }})
        </h3>
        <button
          v-if="unfinished.length"
          class="today-button"
          :disabled="busy"
          @click="allToday"
        >
          Wszystkie na dziś
        </button>
      </div>
      <p v-if="!unfinished.length" class="report-note">
        Wszystko z wcześniejszych planów jest zrobione albo przeniesione.
      </p>
      <div
        v-for="task in unfinished"
        :key="task.id"
        class="journal-task-row morning-row"
      >
        <span
          >{{ task.title
          }}<small class="hint"
            >Plan {{ dayText(task.details!.plannedDay)
            }}{{
              task.details?.waiting ? " · Czekam na: " + task.details.waiting : ""
            }}</small
          ></span
        ><button
          class="today-button"
          :disabled="busy"
          :aria-label="'Na dziś: ' + task.title"
          @click="toDay(task, today)"
        >
          Na dziś</button
        ><button
          class="today-button"
          :disabled="busy"
          :aria-label="'Jutro: ' + task.title"
          :title="'Przenieś na ' + dayText(next)"
          @click="toDay(task, next)"
        >
          Jutro</button
        ><button
          class="today-button"
          :disabled="busy"
          :aria-label="'Do zaplanowania: ' + task.title"
          @click="toDay(task, '')"
        >
          Do zaplanowania</button
        ><button
          class="today-button"
          :disabled="busy"
          :aria-label="'Ukończ: ' + task.title"
          @click="finish(task)"
        >
          Ukończ
        </button>
      </div>
    </section>

    <section class="morning-step" aria-label="Terminy">
      <h3>2. Terminy: dziś i zaległe</h3>
      <p v-if="!due.length" class="report-note">Brak terminów na dziś.</p>
      <div v-for="task in due" :key="task.id" class="journal-task-row morning-row">
        <span
          >{{ task.title
          }}<small
            class="hint"
            :class="{ 'estimate-over': task.details!.dueDay < today }"
            >{{ dueText(task) }} · {{ planText(task) }}</small
          ></span
        ><button
          v-if="task.details?.plannedDay !== today"
          class="today-button"
          :disabled="busy"
          :aria-label="'Na dziś: ' + task.title"
          @click="toDay(task, today)"
        >
          Na dziś
        </button>
      </div>
    </section>

    <section class="morning-step" aria-label="Nowe z Jiry">
      <h3>3. Nowe z Jiry od ostatniego przeglądu</h3>
      <p v-if="!fresh.length" class="report-note">
        Brak nowych zgłoszeń przypisanych do Ciebie.
      </p>
      <div
        v-for="task in fresh"
        :key="task.id"
        class="journal-task-row morning-row"
      >
        <span
          ><span class="mono">{{ issueRef(task)?.key }}</span> {{ task.title
          }}<small class="hint">{{
            [issueRef(task)?.status, planText(task)].filter(Boolean).join(" · ")
          }}</small></span
        ><button
          class="today-button"
          :disabled="busy"
          :aria-label="'Na dziś: ' + task.title"
          @click="toDay(task, today)"
        >
          Na dziś
        </button>
      </div>
    </section>

    <section class="morning-step" aria-label="Dziś">
      <h3>4. Dziś</h3>
      <div
        class="week-capacity morning-capacity"
        :class="capacity.level"
        role="progressbar"
        aria-label="Pojemność dnia"
        aria-valuemin="0"
        aria-valuemax="100"
        :aria-valuenow="Math.round(Math.min(1, capacity.load) * 100)"
        :aria-valuetext="
          hhmm(capacity.estimate) +
          ' / ' +
          (capacity.free ? 'wolne' : hhmm(capacity.capacity))
        "
        :title="capacity.text"
      >
        <span
          v-if="capacity.meetingsWidth > 0"
          class="week-capacity-meetings"
          :style="{ width: capacity.meetingsWidth + '%' }"
        ></span
        ><span :style="{ width: capacity.width + '%' }"></span>
      </div>
      <p class="hint morning-capacity-text">
        Plan <span class="mono">{{ hhmm(capacity.estimate) }}</span> ·
        {{ capacity.text }}
        <template v-if="settings.planWithCorrection">
          · szacunki z poprawką</template
        >
      </p>
      <!-- Ostrzeżenie o przeładowanym dniu słowami, nie tylko kolorem paska
           (Sunsama: kontrola obciążenia w planowaniu dnia; WCAG 1.4.1). -->
      <p
        v-if="capacity.level === 'over' && !capacity.free"
        class="morning-over"
      >
        <TriangleAlert :size="16" aria-hidden="true" />Plan jest o
        {{ hm(capacity.estimate - capacity.capacity) }} dłuższy niż czas na
        pracę. Przenieś część zadań na inny dzień w kroku 1 albo na tablicy
        tygodnia.
      </p>
      <h4>Spotkania</h4>
      <ul v-if="meetings.length" class="morning-list" aria-label="Spotkania dziś">
        <li v-for="m in meetings" :key="m.id">
          <span class="mono">{{ m.time }}</span> {{ m.title
          }}<small v-if="m.tentative" class="hint"> · wstępnie</small>
        </li>
      </ul>
      <p v-else class="report-note">
        {{
          calendar.connected.value
            ? "Brak spotkań dziś."
            : "Kalendarz nie jest połączony."
        }}
      </p>
      <h4>Plan dnia</h4>
      <ol v-if="plan.length" class="morning-list" aria-label="Plan dnia dziś">
        <li v-for="task in plan" :key="task.id" :class="{ done: task.done }">
          {{ task.title
          }}<small class="hint"
            >{{ task.done ? " · ukończone" : ""
            }}{{
              task.details?.estimateMinutes
                ? " · ~" + hm(task.details.estimateMinutes * 60_000)
                : ""
            }}</small
          >
        </li>
      </ol>
      <p v-else class="report-note">Brak zaplanowanych zadań na dziś.</p>
    </section>

    <div class="modal-actions morning-actions">
      <button class="primary-button" :disabled="busy" @click="startDay">
        Zaczynam dzień
      </button>
    </div>
  </section>
</template>
