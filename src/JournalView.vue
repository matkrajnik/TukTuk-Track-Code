<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { ChevronLeft, ChevronRight } from "lucide-vue-next";
import {
  change,
  detailsOf,
  duration,
  saveDetails,
  type Snapshot,
  type Task,
  type Session,
} from "./store";
import { dayKey, dayBounds, dayText, inDay } from "./time";
import { plannedTasks } from "./dayplan";
import {
  dayTarget,
  hhmm,
  signedDiff,
  targetDiffMinutes,
  useSettings,
  weekTarget,
} from "./target";
import ManualTime from "./ManualTime.vue";
import TaskPlanning from "./TaskPlanning.vue";
import DayClose from "./DayClose.vue";
import ReorderHandle from "./components/ReorderHandle.vue";
import DayTimeline from "./components/DayTimeline.vue";
import WeekBoard from "./components/WeekBoard.vue";
import { useReorder } from "./composables/useReorder";
const props = defineProps<{
  data: Snapshot;
  now: number;
  /** Żądanie z powiadomienia: otwórz Zamknięcie dnia na dziś. */
  dayClose?: boolean;
}>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  edit: [session: Session];
  split: [session: Session];
  report: [day: string];
  dayCloseShown: [];
}>();
const day = ref(dayKey(props.now));
const closing = ref(false);
const weekly = ref(false),
  busy = ref(false),
  error = ref("");
const expanded = ref(0);
/** Zakładka Tydzień: tablica (domyślnie) albo dotychczasowe podsumowanie. */
const weekView = ref<"board" | "summary">("board");
const settings = useSettings();
watch(
  () => props.dayClose,
  (open) => {
    if (!open) return;
    day.value = dayKey(props.now);
    weekly.value = false;
    closing.value = true;
    emit("dayCloseShown");
  },
  { immediate: true },
);
/** Klik w blok planu na osi: rozwinięcie planu zadania na liście planu dnia. */
async function openPlan(taskId: number) {
  expanded.value = taskId;
  await nextTick();
  const card = document.getElementById("plan-task-" + taskId);
  card?.scrollIntoView({ block: "nearest" });
  card?.querySelector<HTMLElement>(".journal-name")?.focus();
}
const projects = computed(() => [
  ...new Set(
    props.data.tasks
      .map((t) => t.details?.project)
      .filter((p): p is string => Boolean(p)),
  ),
]);
const taskMap = computed(() => new Map(props.data.tasks.map((t) => [t.id, t])));
/** Ta sama kolejność co bloki toru „Plan” na osi dnia. */
const planned = computed(() => plannedTasks(props.data.tasks, day.value));
/**
 * Zakończone zadania stoją zawsze pod niezakończonymi, więc przesuwanie
 * działa tylko w obrębie jednej z tych części (jak grupy na liście zadań).
 */
function planPart(task: Task) {
  return planned.value.filter((t) => t.done === task.done);
}
function planList(task: Task) {
  return task.done ? "plan-done" : "plan-open";
}
const overduePlan = computed(() =>
  props.data.tasks.filter(
    (t) =>
      !t.archived &&
      !t.done &&
      t.details?.plannedDay &&
      t.details.plannedDay < day.value,
  ),
);
const unplanned = computed(() =>
  props.data.tasks.filter(
    (t) => !t.archived && !t.done && t.details?.plannedDay !== day.value,
  ),
);
const reorder = useReorder({
  scope: "plan",
  all: () => props.data.tasks,
  activeTaskId: () => null,
  busy,
  updated: (data) => emit("updated", data),
  error: (message) => (error.value = message),
});
const selectedTask = ref(0);
async function plan(task: Task, target: string) {
  await run(() =>
    saveDetails(task.id, { ...detailsOf(task), plannedDay: target }),
  );
  selectedTask.value = 0;
}
async function run(action: () => Promise<Snapshot>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    emit("updated", await action());
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
function chooseDay(event: Event) {
  const input = event.target as HTMLInputElement;
  if (
    /^\d{4}-\d{2}-\d{2}$/.test(input.value) &&
    Number.isFinite(dayBounds(input.value)[0])
  )
    day.value = input.value;
  else input.value = day.value;
}
function shift(amount: number) {
  const d = new Date(dayBounds(day.value)[0]);
  d.setDate(d.getDate() + amount);
  day.value = dayKey(+d);
}
function nextDay() {
  const d = new Date(dayBounds(day.value)[0]);
  d.setDate(d.getDate() + 1);
  return dayKey(+d);
}
const intervals = computed(() => {
  const [start, end] = dayBounds(day.value);
  return props.data.sessions
    .filter((s) => inDay(s, day.value, props.now) > 0)
    .map((s) => ({
      session: s,
      start: Math.max(start, s.startedAt),
      end: Math.min(end, s.endedAt ?? props.now),
    }))
    .sort((a, b) => a.start - b.start);
});
const timeline = computed(() => {
  const result: {
    key: string;
    start: number;
    end: number;
    session?: Session;
  }[] = [];
  let previous = 0;
  for (const interval of intervals.value) {
    if (previous && interval.start > previous)
      result.push({
        key: `gap-${interval.session.id}`,
        start: previous,
        end: interval.start,
      });
    result.push({ key: `session-${interval.session.id}`, ...interval });
    previous = Math.max(previous, interval.end);
  }
  return result;
});
const clock = (at: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(at);
const week = computed(() => {
  const d = new Date(dayBounds(day.value)[0]);
  d.setDate(d.getDate() - ((d.getDay() + 6) % 7));
  return Array.from({ length: 7 }, (_, i) => {
    const date = new Date(d);
    date.setDate(d.getDate() + i);
    const key = dayKey(+date);
    return {
      key,
      total: props.data.sessions.reduce(
        (sum, s) => sum + inDay(s, key, props.now),
        0,
      ),
      target: dayTarget(settings.value, key),
    };
  });
});
const weekGoal = computed(() => weekTarget(settings.value));
const weekTasks = computed(() =>
  props.data.tasks
    .map((task) => ({
      task,
      total: props.data.sessions
        .filter((s) => s.taskId === task.id)
        .reduce(
          (sum, s) =>
            sum +
            week.value.reduce(
              (part, d) => part + inDay(s, d.key, props.now),
              0,
            ),
          0,
        ),
    }))
    .filter((row) => row.total > 0)
    .sort((a, b) => b.total - a.total),
);
const weekProjects = computed(() => {
  const sums = new Map<string, number>();
  weekTasks.value.forEach((row) => {
    const name = row.task.details?.project || "Bez projektu";
    sums.set(name, (sums.get(name) ?? 0) + row.total);
  });
  return [...sums].sort((a, b) => b[1] - a[1]);
});
const weekTotal = computed(() =>
  week.value.reduce((sum, d) => sum + d.total, 0),
);
const carry = computed(() => weekTasks.value.filter((row) => !row.task.done));
</script>
<template>
  <div class="page-heading">
    <h1>Plan i historia</h1>
    <div class="date-picker">
      <button
        class="icon-button"
        aria-label="Poprzedni okres"
        @click="shift(weekly ? -7 : -1)"
      >
        <ChevronLeft :size="18" /></button
      ><input
        :value="day"
        type="date"
        aria-label="Dzień planu"
        required
        @change="chooseDay"
      /><button
        class="icon-button"
        aria-label="Następny okres"
        @click="shift(weekly ? 7 : 1)"
      >
        <ChevronRight :size="18" /></button
      ><button class="today-button" @click="day = dayKey(now)">Dzisiaj</button>
    </div>
  </div>
  <div class="tabs journal-tabs" role="group" aria-label="Widok planu">
    <button
      :class="{ selected: !weekly && !closing }"
      @click="
        weekly = false;
        closing = false;
      "
    >
      Dzień</button
    ><button
      :class="{ selected: weekly && !closing }"
      @click="
        weekly = true;
        closing = false;
      "
    >
      Tydzień
    </button>
    <button
      :class="{ selected: closing }"
      @click="
        closing = true;
        weekly = false;
      "
    >
      Zamknięcie dnia
    </button>
  </div>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <DayClose
    v-if="closing"
    :data="data"
    :day="day"
    :now="now"
    @updated="emit('updated', $event)"
    @report="emit('report', $event)"
  />
  <template v-else-if="!weekly">
    <section class="dashboard-card" aria-label="Plan dnia">
      <h2>Plan na {{ dayText(day) }}</h2>
      <p class="hint">
        Szacowany czas planu:
        {{
          hhmm(
            planned.reduce(
              (sum, t) => sum + (t.details?.estimateMinutes ?? 0) * 60000,
              0,
            ),
          )
        }}
      </p>
      <form
        class="planning-actions"
        @submit.prevent="
          selectedTask &&
          taskMap.get(selectedTask) &&
          plan(taskMap.get(selectedTask)!, day)
        "
      >
        <select v-model.number="selectedTask" aria-label="Zadanie do planu">
          <option :value="0">Wybierz zadanie</option>
          <option v-for="task in unplanned" :key="task.id" :value="task.id">
            {{ task.title }}
          </option></select
        ><button class="today-button" :disabled="busy || !selectedTask">
          Dodaj do planu
        </button>
      </form>
      <p v-if="!planned.length" class="report-note">
        Brak zaplanowanych zadań na ten dzień.
      </p>
      <article
        v-for="task in planned"
        :key="task.id"
        :id="'plan-task-' + task.id"
        class="journal-task"
        v-bind="reorder.dropProps(planPart(task), task, planList(task))"
      >
        <div class="journal-task-row">
          <ReorderHandle
            :title="task.title"
            :task-id="task.id"
            scope="plan"
            :disabled="busy"
            @move="reorder.move(planPart(task), task.id, $event)"
            @dragstart="reorder.start($event, task.id, planList(task))"
            @dragend="reorder.end()"
          />
          <button
            class="journal-name"
            @click="expanded = expanded === task.id ? 0 : task.id"
          >
            {{ task.title
            }}<small>{{
              task.done
                ? "Ukończone"
                : task.details?.waiting
                  ? "Czekam na: " + task.details.waiting
                  : "Zaplanowane"
            }}</small></button
          ><button
            v-if="!task.done"
            class="today-button"
            :disabled="busy"
            @click="
              run(() =>
                change(
                  data.sessions.some(
                    (s) => s.taskId === task.id && s.endedAt === null,
                  )
                    ? 'stop'
                    : 'start',
                  task.id,
                ),
              )
            "
          >
            {{
              data.sessions.some(
                (s) => s.taskId === task.id && s.endedAt === null,
              )
                ? "Stop"
                : "Start"
            }}</button
          ><button
            v-if="!task.done"
            class="today-button"
            :disabled="busy"
            @click="plan(task, nextDay())"
          >
            Przełóż na {{ dayText(nextDay()) }}</button
          ><button
            class="today-button"
            :disabled="busy"
            @click="plan(task, '')"
          >
            Usuń z planu
          </button>
        </div>
        <TaskPlanning
          v-if="expanded === task.id"
          :task="task"
          :projects="projects"
          @updated="emit('updated', $event)"
        />
      </article>
      <details v-if="overduePlan.length" class="overdue-plans">
        <summary>
          Niedokończone z wcześniejszych planów ({{ overduePlan.length }})
        </summary>
        <div
          v-for="task in overduePlan"
          :key="task.id"
          class="journal-task-row"
        >
          <span>{{ task.title }} · {{ dayText(task.details!.plannedDay) }}</span
          ><button
            class="today-button"
            :disabled="busy"
            @click="plan(task, day)"
          >
            Przenieś na ten dzień
          </button>
        </div>
      </details>
    </section>
    <section
      class="dashboard-card dashboard-top"
      aria-label="Plan kontra rzeczywistość"
    >
      <h2>Plan kontra rzeczywistość</h2>
      <DayTimeline
        :tasks="data.tasks"
        :sessions="data.sessions"
        :day="day"
        :now="now"
        :day-start="settings.dayStart"
        @edit="emit('edit', $event)"
        @plan="openPlan"
      />
    </section>
    <section class="dashboard-card dashboard-top" aria-label="Oś dnia">
      <div class="ts-heading">
        <h2>
          Oś dnia ·
          {{ duration(intervals.reduce((sum, i) => sum + i.end - i.start, 0)) }}
        </h2>
        <button class="today-button" @click="emit('report', day)">
          Raport dzienny / TS
        </button>
      </div>
      <ManualTime
        :tasks="data.tasks"
        :day="day"
        @updated="emit('updated', $event)"
      />
      <p class="report-note">
        Luki pokazujemy między zapisanymi pomiarami. Przedziały przez północ są
        przycięte do wybranego dnia.
      </p>
      <p v-if="!timeline.length" class="report-note">
        Brak zarejestrowanej pracy.
      </p>
      <div
        v-for="item in timeline"
        :key="item.key"
        class="timeline-row"
        :class="{ gap: !item.session }"
      >
        <span class="mono"
          >{{ clock(item.start) }} →
          {{ item.session?.endedAt === null ? "teraz" : clock(item.end) }}</span
        ><span>{{
          item.session
            ? taskMap.get(item.session.taskId)?.title
            : "Luka w zapisie"
        }}</span
        ><strong class="mono">{{ duration(item.end - item.start) }}</strong
        ><span class="timeline-actions"
          ><button
            v-if="item.session"
            class="today-button"
            :disabled="item.session.endedAt === null"
            :title="
              item.session.endedAt === null
                ? 'Zatrzymaj pomiar przed edycją'
                : undefined
            "
            @click="emit('edit', item.session)"
          >
            Edytuj</button
          ><button
            v-if="item.session"
            class="today-button"
            :aria-label="'Podziel przedział ' + item.session.id"
            @click="emit('split', item.session)"
          >
            Podziel
          </button></span
        >
      </div>
      <div
        v-for="task in data.tasks.filter((t) => t.details?.notes?.[day])"
        :key="task.id"
        class="daily-note"
      >
        <strong>{{ task.title }}</strong>
        <p>{{ task.details?.notes?.[day] }}</p>
      </div>
    </section>
  </template>
  <template v-else>
    <div class="tabs week-tabs" role="group" aria-label="Widok tygodnia">
      <button
        :class="{ selected: weekView === 'board' }"
        :aria-pressed="weekView === 'board'"
        @click="weekView = 'board'"
      >
        Tablica</button
      ><button
        :class="{ selected: weekView === 'summary' }"
        :aria-pressed="weekView === 'summary'"
        @click="weekView = 'summary'"
      >
        Podsumowanie
      </button>
    </div>
    <WeekBoard
      v-if="weekView === 'board'"
      :data="data"
      :now="now"
      :day="day"
      @updated="emit('updated', $event)"
    />
  </template>
  <template v-if="weekly && !closing && weekView === 'summary'">
    <section class="report-summary">
      <div>
        <p>{{ dayText(week[0].key) }} — {{ dayText(week[6].key) }}</p>
        <h2>Podsumowanie tygodnia</h2>
      </div>
      <div>
        <strong class="mono report-total">{{ duration(weekTotal) }}</strong>
        <p v-if="weekGoal" class="hint">
          Cel tygodnia {{ hhmm(weekGoal) }} ·
          {{ signedDiff(weekTotal, weekGoal) }}
        </p>
      </div>
    </section>
    <div class="dashboard-columns dashboard-top">
      <section class="dashboard-card">
        <h2>Czas według dni</h2>
        <button
          v-for="d in week"
          :key="d.key"
          class="weekly-day"
          @click="emit('report', d.key)"
        >
          <span>{{ dayText(d.key) }}</span
          ><span
            class="mono target-diff"
            :class="{ over: targetDiffMinutes(d.total, d.target) > 0 }"
            :title="
              d.target
                ? 'Różnica względem celu ' + hhmm(d.target)
                : 'Dzień wolny'
            "
            >{{
              d.target || d.total >= 60000
                ? signedDiff(d.total, d.target)
                : "wolne"
            }}</span
          ><strong class="mono">{{ duration(d.total) }}</strong>
        </button>
      </section>
      <section class="dashboard-card">
        <h2>Projekty / obszary</h2>
        <div
          v-for="[name, total] in weekProjects"
          :key="name"
          class="dashboard-count"
        >
          <span>{{ name }}</span
          ><strong class="mono">{{ duration(total) }}</strong>
        </div>
        <p v-if="!weekProjects.length" class="report-note">
          Brak pracy w tym tygodniu.
        </p>
      </section>
    </div>
    <section class="dashboard-card dashboard-top">
      <h2>Zadania z pracą w tym tygodniu</h2>
      <div v-for="row in weekTasks" :key="row.task.id" class="dashboard-count">
        <span
          >{{ row.task.title }} ·
          {{
            row.task.done
              ? "Ukończone"
              : row.task.details?.waiting
                ? "Czekam na…"
                : "W trakcie"
          }}</span
        ><strong class="mono">{{ duration(row.total) }}</strong>
      </div>
      <p v-if="!weekTasks.length" class="report-note">
        Brak zarejestrowanej pracy.
      </p>
    </section>
    <section class="dashboard-card dashboard-top">
      <h2>Sprawy do kontynuacji ({{ carry.length }})</h2>
      <p class="report-note">
        Zadania z tego tygodnia, które obecnie nie są ukończone. Na następny
        poniedziałek przeniesiesz je na tablicy: „Przenieś na…”.
      </p>
      <div v-for="row in carry" :key="row.task.id" class="journal-task-row">
        <span>{{ row.task.title }}</span
        ><span class="hint">{{
          row.task.details?.plannedDay
            ? "Plan " + dayText(row.task.details.plannedDay)
            : "Bez planu"
        }}</span>
      </div>
    </section>
  </template>
</template>
