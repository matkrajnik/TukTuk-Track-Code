<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  CalendarPlus,
  Plus,
  Scissors,
  Search,
  Square,
  Timer,
} from "lucide-vue-next";
import { countdown, focusAction, useFocus } from "../focus";
import TaskCard from "../components/TaskCard.vue";
import MeetingEditor from "../components/MeetingEditor.vue";
import SearchSelect from "../components/SearchSelect.vue";
import { offerAfterDone } from "../jiramove";
import {
  change,
  duration,
  jiraConnected,
  isManualMeeting,
  meetingOccupies,
  meetingTitle,
  loadTemplates,
  onTemplatesChanged,
  quickAdd,
  quickTask,
  redmineConnected,
  refreshJiraConnected,
  refreshRedmineConnected,
  onRedmineChanged,
  templates,
  setPriority,
  type CalendarEvent,
  type NavigateRequest,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import { clock, dayBounds, dayKey, dayText, inDay } from "../time";
import { taskMatches } from "../issue";
import {
  doneMoments,
  groupByDoneDay,
  recentDoneFrom,
  RECENT_DONE_DAYS,
} from "../done";
import { dayGoal } from "../capacity";
import { corrections } from "../estimates";
import { useMeetings } from "../composables/useMeetings";
import { sessionsByTask } from "../report";
import { sortTasks } from "../order";
import TargetBar from "../components/TargetBar.vue";
import { dayTarget, hhmm, useSettings } from "../target";
import { useReorder } from "../composables/useReorder";
import QuickChips from "../components/QuickChips.vue";
import {
  blockingError,
  complete,
  hasFields,
  parse,
  removeToken,
  type Token,
} from "../quickadd";

const props = defineProps<{
  data: Snapshot;
  now: number;
  busy: boolean;
  loading: boolean;
  /** Żądanie z powiadomienia (`navigate`); nowy obiekt = nowe żądanie. */
  request?: NavigateRequest | null;
}>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
  edit: [task: Task];
  remove: [task: Task];
  session: [session: Session];
  split: [session: Session];
}>();

const working = ref(false);
const busy = computed(() => props.busy || working.value);
const newTitle = ref("");
const query = ref("");
const filter = ref("active");
const projectFilter = ref("");
const todayOnly = ref(false);
/**
 * Tylko po kliknięciu powiadomienia o terminach (`filter: "due"`): „Dzisiaj”
 * obejmuje też terminy dziś i zaległe. Ręczne wyłączenie filtra przywraca
 * zwykłe znaczenie (plan na dziś i praca z dziś).
 */
const dueIncluded = ref(false);
watch(todayOnly, (on) => {
  if (!on) dueIncluded.value = false;
});
const expanded = ref(new Set<number>());

const projects = computed(() =>
  [
    ...new Set(
      props.data.tasks
        .map((t) => t.details?.project)
        .filter((p): p is string => Boolean(p)),
    ),
  ].sort((a, b) => a.localeCompare(b, "pl")),
);
/** Pozycje filtru projektu: dwie stałe na początku, potem projekty zadań. */
const projectFilterOptions = computed(() => [
  { value: "", label: "Wszystkie projekty" },
  { value: "__none__", label: "Bez projektu" },
  ...projects.value.map((project) => ({ value: project, label: project })),
]);
const active = computed(() =>
  props.data.sessions.find((s) => s.endedAt === null),
);
const activeTask = computed(() =>
  props.data.tasks.find((t) => t.id === active.value?.taskId),
);
const startedTaskIds = computed(
  () => new Set(props.data.sessions.map((session) => session.taskId)),
);
const todayKey = computed(() => dayKey(props.now));
/** Zamknięte przedziały dnia zależą tylko od danych i dnia (B6). */
const todayClosed = computed(() =>
  props.data.sessions.reduce(
    (sum, s) =>
      s.endedAt === null ? sum : sum + inDay(s, todayKey.value, 0),
    0,
  ),
);
const today = computed(
  () =>
    todayClosed.value +
    (active.value ? inDay(active.value, todayKey.value, props.now) : 0),
);
const settings = useSettings();
/** Spotkania dziś: lista nadchodzących i cel dnia pomniejszony o spotkania. */
const calendar = useMeetings(() => dayBounds(todayKey.value));
/** Cel mierzonego czasu: cel dnia minus spotkania niepokryte pomiarem. */
const todayTarget = computed(() =>
  dayGoal(
    dayTarget(settings.value, todayKey.value),
    calendar.events.value,
    props.data.sessions,
    todayKey.value,
    props.now,
  ),
);
/** Nadchodzące (i trwające) spotkania dziś, które zajmują czas. */
const upcomingMeetings = computed(() => {
  const [, end] = dayBounds(todayKey.value);
  return calendar.events.value
    .filter((e) => meetingOccupies(e) && e.end > props.now && e.start < end)
    .map((e) => ({
      event: e,
      time: e.allDay
        ? "Cały dzień"
        : clock(e.start) + "–" + clock(e.end),
      title: meetingTitle(e, settings.value.calendarShowTitles),
      now: e.start <= props.now,
      manual: isManualMeeting(e),
    }));
});
/** Ręczne spotkanie (B12): `null` = nowe, spotkanie = edycja; `false` = zamknięte. */
const meetingEditor = ref<CalendarEvent | null | false>(false);
/**
 * Przedziały pogrupowane po zadaniu (od najnowszego), liczone raz na zmianę
 * danych zamiast filtrowania wszystkich przedziałów dla każdej karty co sekundę
 * (B6). Ta sama tablica dla zadania między odświeżeniami danych.
 */
const taskSessionMap = computed(() => sessionsByTask(props.data.sessions));
const noSessions: Session[] = [];
/**
 * Zadania z czasem dziś (filtr „Dziś”): zależy od danych i dnia, nie od
 * sekundnika. Trwający pomiar liczy się, gdy zaczął się przed końcem dnia.
 */
const todayTaskIds = computed(() => {
  const end = dayBounds(todayKey.value)[1];
  return new Set(
    props.data.sessions
      .filter((s) => inDay(s, todayKey.value, end) > 0)
      .map((s) => s.taskId),
  );
});
/**
 * Zadania po filtrach i wyszukiwaniu (tytuł, projekt, klucz zgłoszenia), także
 * ukończone dawniej niż 30 dni; `visible` ukrywa te ostatnie bez „Pokaż starsze”.
 */
const matching = computed(() =>
  props.data.tasks.filter(
    (t) =>
      !t.archived &&
      (filter.value === "all" ||
        (filter.value === "done" ? t.done : !t.done)) &&
      (!projectFilter.value ||
        (projectFilter.value === "__none__"
          ? !t.details?.project
          : t.details?.project === projectFilter.value)) &&
      (!todayOnly.value ||
        t.details?.plannedDay === todayKey.value ||
        // Termin dziś lub po terminie: tylko po kliknięciu powiadomienia o terminach.
        (dueIncluded.value &&
          !t.done &&
          !!t.details?.dueDay &&
          t.details.dueDay <= todayKey.value) ||
        todayTaskIds.value.has(t.id)) &&
      taskMatches(t, query.value),
  ),
);
/** „Pokaż starsze”: ukończone zadania sprzed ostatnich 30 dni. */
const showOlder = ref(false);
/** Bieżąca minuta: zmienia się co 60 s, więc zależne wyliczenia nie liczą się co sekundę. */
const minute = computed(() => Math.floor(props.now / 60_000) * 60_000);
/** Chwile ukończenia; trwający pomiar ukończonego zadania nie istnieje, więc wystarczy minuta. */
const doneAt = computed(() =>
  doneMoments(props.data.tasks, props.data.sessions, minute.value),
);
const recentFrom = computed(() => recentDoneFrom(todayKey.value));
const recentDone = (t: Task) =>
  dayKey(doneAt.value.get(t.id) ?? t.createdAt) >= recentFrom.value;
const visible = computed(() =>
  matching.value.filter((t) => !t.done || showOlder.value || recentDone(t)),
);
/** Ukończone zadania ukryte przez limit 30 dni (po filtrach i wyszukiwaniu). */
const olderDone = computed(
  () => matching.value.filter((t) => t.done && !recentDone(t)).length,
);
const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
const taskGroups = computed(() =>
  [
    {
      name: "Rozpoczęte",
      tasks: visible.value.filter(
        (t) => !t.done && !t.details?.waiting && startedTaskIds.value.has(t.id),
      ),
    },
    // „Czekam na” zaraz pod rozpoczętymi (P3): zablokowane widać od razu.
    {
      name: "Czekam na…",
      tasks: visible.value.filter((t) => !t.done && t.details?.waiting),
    },
    {
      name: "Nierozpoczęte",
      tasks: visible.value.filter(
        (t) =>
          !t.done && !t.details?.waiting && !startedTaskIds.value.has(t.id),
      ),
    },
    { name: "Ukończone", tasks: visible.value.filter((t) => t.done) },
  ]
    .filter((group) => group.tasks.length)
    .map((group) => {
      const tasks = sortTasks(group.tasks, active.value?.taskId);
      // Ukończone: podgrupy po dniu ukończenia (najnowszy dzień pierwszy);
      // ręczna kolejność działa w obrębie jednego dnia.
      const parts =
        group.name === "Ukończone"
          ? groupByDoneDay(tasks, doneAt.value)
          : [{ day: "", tasks }];
      return { ...group, tasks, parts };
    }),
);

const reorder = useReorder({
  scope: "tasks",
  all: () => props.data.tasks,
  activeTaskId: () => active.value?.taskId,
  busy: working,
  updated: (data) => emit("updated", data),
  error: (message) => emit("error", message),
});

/**
 * Kliknięcie powiadomienia: lista bez wyszukiwania i filtra projektu, „Do zrobienia”,
 * opcjonalnie „Dzisiaj: plan i praca”, rozwinięte zadanie i przewinięcie do niego
 * albo do wskazanej grupy.
 */
watch(
  () => props.request,
  async (request) => {
    if (!request) return;
    filter.value = "active";
    showOlder.value = false;
    query.value = "";
    projectFilter.value = "";
    todayOnly.value = request.filter === "today" || request.filter === "due";
    dueIncluded.value = request.filter === "due";
    if (request.task != null) expanded.value.add(request.task);
    // „Popraw” w karcie czatu: pole „Nowe zadanie” wypełnione składnią szybkiego dodawania.
    if (request.newTask) newTitle.value = request.newTask;
    await nextTick();
    const target =
      (request.task != null &&
        document.querySelector(`[aria-controls="sessions-${request.task}"]`)) ||
      (request.section &&
        [...document.querySelectorAll<HTMLElement>("section.task-group")].find(
          (s) => s.getAttribute("aria-label") === request.section,
        ));
    if (request.newTask) newInput.value?.focus();
    else if (target) target.scrollIntoView?.({ block: "nearest" });
  },
  { immediate: true },
);

function toggle(id: number) {
  if (expanded.value.has(id)) expanded.value.delete(id);
  else expanded.value.add(id);
}
async function act(
  action: string,
  id: number | null = null,
  title: string | null = null,
) {
  if (busy.value) return false;
  working.value = true;
  emit("error", "");
  try {
    const before = props.data.tasks.find((t) => t.id === id);
    emit("updated", await change(action, id, title));
    // Ukończenie zadania z kluczem Jiry: propozycja przesunięcia (J1).
    if (action === "toggle" && before && !before.done)
      void offerAfterDone(before);
    return true;
  } catch (e) {
    emit("error", String(e));
    return false;
  } finally {
    working.value = false;
  }
}
/** Tryb skupienia na pasku bieżącego pomiaru. */
const focus = useFocus();
const focusBlock = computed(() =>
  focus.value.block && focus.value.block.taskId === active.value?.taskId
    ? focus.value.block
    : null,
);
async function focusRun(action: "start" | "stop") {
  if (busy.value) return;
  working.value = true;
  emit("error", "");
  try {
    await focusAction(action);
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    working.value = false;
  }
}
const templateNames = computed(() => templates.value.map((t) => t.name));
let stopTemplates: (() => void) | undefined;
let mounted = true;
// Połączenie albo rozłączenie Redmine (Ustawienia, inne okno): `#1234` w
// szybkim dodawaniu zmienia znaczenie od razu.
const stopRedmine = onRedmineChanged(() => void refreshRedmineConnected());
onMounted(async () => {
  void refreshJiraConnected();
  void refreshRedmineConnected();
  try {
    const stop = await onTemplatesChanged();
    if (!mounted) return stop();
    stopTemplates = stop;
    await loadTemplates();
  } catch {
    // Bez listy szablonów dodawanie działa dalej; `/nazwa` pokaże wtedy błąd
    // w chipie, a lista wróci przy następnym otwarciu widoku.
  }
});
onUnmounted(() => {
  mounted = false;
  stopTemplates?.();
  stopRedmine();
});
/** Poprawka szacunków (podpowiedzi „zwykle ~…”); przeliczana co minutę. */
const correctionOf = computed(() => corrections(props.data, minute.value));
const newInput = ref<HTMLInputElement | null>(null);
const parsed = computed(() =>
  parse(newTitle.value, todayKey.value, {
    projects: projects.value,
    templates: templateNames.value,
    jiraEnabled: jiraConnected.value,
    redmineEnabled: redmineConnected.value,
  }),
);
/** Bez zapisów składni dodanie działa dokładnie jak wcześniej (`mutate add`). */
const canAdd = computed(
  () =>
    !blockingError(parsed.value) &&
    (!!parsed.value.title.trim() ||
      !!parsed.value.fields.jiraKey ||
      parsed.value.fields.redmineId != null ||
      !!parsed.value.fields.template),
);
async function add() {
  const result = parsed.value;
  const blocking = blockingError(result);
  if (blocking) {
    emit("error", blocking);
    return;
  }
  if (!hasFields(result)) {
    const raw = result.title === newTitle.value.trim();
    if (await act("add", null, raw ? newTitle.value : result.title))
      newTitle.value = "";
    return;
  }
  if (busy.value) return;
  working.value = true;
  emit("error", "");
  try {
    emit("updated", await quickAdd(quickTask(result)));
    newTitle.value = "";
  } catch (e) {
    emit("error", String(e));
  } finally {
    working.value = false;
  }
}
function removeChip(token: Token) {
  newTitle.value = removeToken(newTitle.value, token);
  newInput.value?.focus();
}
function tab(e: KeyboardEvent) {
  const input = e.target as HTMLInputElement;
  if (e.shiftKey || input.selectionStart !== input.selectionEnd) return;
  const done = complete(input.value, input.selectionStart ?? 0, {
    projects: projects.value,
    templates: templateNames.value,
  });
  if (!done) return;
  e.preventDefault();
  newTitle.value = done.text;
  void nextTick(() => input.setSelectionRange(done.caret, done.caret));
}
async function priority(task: Task, e: Event) {
  working.value = true;
  try {
    emit(
      "updated",
      await setPriority(task.id, Number((e.target as HTMLSelectElement).value)),
    );
  } catch (err) {
    emit("error", String(err));
    (e.target as HTMLSelectElement).value = String(task.details?.priority ?? 0);
  } finally {
    working.value = false;
  }
}
</script>

<template>
  <div class="page-heading">
    <h1>Zadania</h1>
    <div class="daily-summary">
      <div>Dzisiaj <strong class="mono">{{ duration(today) }}</strong></div>
      <div v-if="todayTarget" class="daily-target">
        <TargetBar
          :total="today"
          :target="todayTarget"
          label="Postęp do celu dnia"
        /><span class="mono target-text"
          >{{ hhmm(today) }} / {{ hhmm(todayTarget) }}</span
        >
      </div>
    </div>
  </div>
  <div v-if="active" class="active-bar" aria-label="Bieżący pomiar">
    <span class="status-dot" /><span class="active-title">{{
      activeTask?.title
    }}</span
    ><span class="mono focus-clock">{{ duration(elapsed(active)) }}</span
    ><span
      v-if="focusBlock"
      class="focus-countdown mono"
      aria-label="Pozostało w bloku skupienia"
      :title="`Blok skupienia do ${clock(focusBlock.end)}`"
      ><Timer :size="12" />{{ countdown(focusBlock.end - now) }}</span
    ><button
      v-if="focusBlock"
      class="stop-button"
      aria-label="Przerwij blok skupienia"
      title="Blok zakończy się jako nieukończony; pomiar trwa dalej"
      :disabled="busy"
      @click="focusRun('stop')"
    >
      Zakończ blok</button
    ><button
      v-else
      class="stop-button"
      aria-label="Rozpocznij blok skupienia"
      title="Blok skupienia na tym zadaniu (długość w Ustawieniach)"
      :disabled="busy"
      @click="focusRun('start')"
    >
      <Timer :size="12" />Skupienie</button
    ><button
      class="stop-button"
      aria-label="Podziel bieżący pomiar"
      title="Podziel pomiar w wybranym momencie"
      :disabled="busy"
      @click="emit('split', active)"
    >
      <Scissors :size="12" />Podziel</button
    ><button class="stop-button" :disabled="busy" @click="act('stop')">
      <Square :size="12" />Stop
    </button>
  </div>
  <form class="add-task" @submit.prevent="add">
    <Plus :size="18" /><input
      ref="newInput"
      v-model="newTitle"
      aria-label="Nowe zadanie"
      placeholder="Nowe zadanie…"
      title="Składnia: WMS-123 · ~30m · @pt · do:20.10 · ! / !niski · #projekt · /szablon · \ dosłownie"
      maxlength="240"
      :disabled="loading"
      @keydown.tab="tab"
    /><button
      class="primary-button"
      :disabled="busy || loading || !newTitle.trim() || !canAdd"
    >
      Dodaj zadanie
    </button>
  </form>
  <QuickChips
    :parsed="parsed"
    :correction="correctionOf"
    @remove="removeChip"
  />
  <div class="list-toolbar">
    <div class="tabs" aria-label="Filtr zadań">
      <button
        v-for="tab in [
          { id: 'active', name: 'Do zrobienia' },
          { id: 'done', name: 'Ukończone' },
          { id: 'all', name: 'Wszystkie' },
        ]"
        :key="tab.id"
        :class="{ selected: filter === tab.id }"
        :aria-pressed="filter === tab.id"
        @click="filter = tab.id"
      >
        {{ tab.name }}
      </button>
    </div>
    <label class="search"
      ><Search :size="15" /><input
        v-model="query"
        aria-label="Szukaj zadań"
        placeholder="Szukaj…"
    /></label>
  </div>
  <div class="organization-filters">
    <label class="inline-check"
      ><input v-model="todayOnly" type="checkbox" />Dzisiaj: plan i praca</label
    >
    <span v-if="dueIncluded" class="due-included"
      >+ terminy dziś i zaległe</span
    >
    <label
      >Projekt
      <SearchSelect
        v-model="projectFilter"
        class="toolbar-select"
        label="Filtr projektu"
        :options="projectFilterOptions"
      /></label
    >
  </div>
  <section
    v-if="todayOnly"
    class="today-meetings"
    aria-label="Spotkania dziś"
  >
    <div class="today-meetings-head">
      <h2 class="task-group-heading">
        Spotkania dziś <span>{{ upcomingMeetings.length }}</span>
      </h2>
      <button
        type="button"
        class="today-button"
        @click="meetingEditor = null"
      >
        <CalendarPlus :size="14" />Dodaj spotkanie
      </button>
    </div>
    <ul v-if="upcomingMeetings.length">
      <li
        v-for="m in upcomingMeetings"
        :key="m.event.id"
        :class="{ tentative: m.event.showAs === 'tentative', now: m.now }"
      >
        <span class="mono">{{ m.time }}</span
        ><span class="today-meeting-title"
          ><button
            v-if="m.manual"
            type="button"
            class="meeting-edit"
            :aria-label="'Edytuj spotkanie: ' + m.title"
            @click="meetingEditor = m.event"
          >
            {{ m.title }}</button
          ><template v-else>{{ m.title }}</template></span
        ><span v-if="m.manual" class="meeting-manual">ręczne</span
        ><small v-if="m.now">trwa</small
        ><small v-else-if="m.event.showAs === 'tentative'">wstępnie</small>
      </li>
    </ul>
    <p v-else class="hint">Brak nadchodzących spotkań dziś.</p>
  </section>
  <MeetingEditor
    v-if="meetingEditor !== false"
    :meeting="meetingEditor"
    :day="todayKey"
    :tasks="data.tasks"
    :now="now"
    @close="meetingEditor = false"
    @saved="meetingEditor = false"
  />
  <div class="task-list" :aria-busy="busy || loading">
    <section
      v-for="group in taskGroups"
      :key="group.name"
      class="task-group"
      :aria-label="group.name"
    >
      <h2 class="task-group-heading">
        {{ group.name }} <span>{{ group.tasks.length }}</span>
      </h2>
      <template v-for="part in group.parts" :key="part.day">
      <h3 v-if="part.day" class="done-day-heading">
        {{ dayText(part.day) }}
      </h3>
      <TaskCard
        v-for="task in part.tasks"
        :key="task.id"
        :task="task"
        :sessions="taskSessionMap.get(task.id) ?? noSessions"
        :running="active?.taskId === task.id"
        :started="startedTaskIds.has(task.id)"
        :expanded="expanded.has(task.id)"
        :busy="busy"
        :now="now"
        :today-key="todayKey"
        :projects="projects"
        :correction="correctionOf"
        v-bind="reorder.dropProps(part.tasks, task, group.name + part.day)"
        @move="!busy && reorder.move(part.tasks, task.id, $event)"
        @dragstart="reorder.start($event, task.id, group.name + part.day)"
        @dragend="reorder.end()"
        @toggle="toggle(task.id)"
        @act="act($event, task.id)"
        @priority="priority(task, $event)"
        @edit="emit('edit', task)"
        @remove="emit('remove', task)"
        @session="emit('session', $event)"
        @updated="emit('updated', $event)"
        @error="emit('error', $event)"
      />
      </template>
    </section>
    <p
      v-if="!loading && filter !== 'active' && (olderDone || showOlder)"
      class="done-older"
    >
      <span class="hint">{{
        showOlder
          ? "Wszystkie ukończone zadania."
          : `Ukończone z ostatnich ${RECENT_DONE_DAYS} dni.`
      }}</span
      ><button
        type="button"
        class="secondary-button"
        :aria-pressed="showOlder"
        @click="showOlder = !showOlder"
      >
        {{
          showOlder
            ? `Tylko ostatnie ${RECENT_DONE_DAYS} dni`
            : `Pokaż starsze (${olderDone})`
        }}
      </button>
    </p>
    <p v-if="loading" class="empty-state">Wczytywanie…</p>
    <p v-else-if="!visible.length" class="empty-state">
      {{
        query
          ? "Brak pasujących zadań."
          : olderDone
            ? `Brak zadań ukończonych w ostatnich ${RECENT_DONE_DAYS} dniach.`
            : "Brak zadań w tym widoku."
      }}
    </p>
  </div>
</template>
