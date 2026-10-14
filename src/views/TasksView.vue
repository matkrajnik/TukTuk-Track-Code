<script setup lang="ts">
import { computed, ref } from "vue";
import { Plus, Scissors, Search, Square } from "lucide-vue-next";
import TaskCard from "../components/TaskCard.vue";
import {
  change,
  duration,
  setPriority,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import { dayKey, inDay } from "../time";
import { taskSessions } from "../report";
import { sortTasks } from "../order";

const props = defineProps<{
  data: Snapshot;
  now: number;
  busy: boolean;
  loading: boolean;
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
const today = computed(() =>
  props.data.sessions.reduce(
    (sum, s) => sum + inDay(s, todayKey.value, props.now),
    0,
  ),
);
const visible = computed(() =>
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
        props.data.sessions.some(
          (s) => s.taskId === t.id && inDay(s, todayKey.value, props.now) > 0,
        )) &&
      t.title
        .toLocaleLowerCase("pl")
        .includes(query.value.toLocaleLowerCase("pl")),
  ),
);
const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
const taskGroups = computed(() =>
  [
    {
      name: "W trakcie",
      tasks: visible.value.filter(
        (t) => !t.done && !t.details?.waiting && startedTaskIds.value.has(t.id),
      ),
    },
    {
      name: "Do zaplanowania",
      tasks: visible.value.filter(
        (t) =>
          !t.done && !t.details?.waiting && !startedTaskIds.value.has(t.id),
      ),
    },
    {
      name: "Czekam na…",
      tasks: visible.value.filter((t) => !t.done && t.details?.waiting),
    },
    { name: "Ukończone", tasks: visible.value.filter((t) => t.done) },
  ]
    .filter((group) => group.tasks.length)
    .map((group) => ({
      ...group,
      tasks: sortTasks(group.tasks, active.value?.taskId),
    })),
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
    emit("updated", await change(action, id, title));
    return true;
  } catch (e) {
    emit("error", String(e));
    return false;
  } finally {
    working.value = false;
  }
}
async function add() {
  if (await act("add", null, newTitle.value)) newTitle.value = "";
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
      Dzisiaj <strong class="mono">{{ duration(today) }}</strong>
    </div>
  </div>
  <div v-if="active" class="active-bar" aria-label="Bieżący pomiar">
    <span class="status-dot" /><span class="active-title">{{
      activeTask?.title
    }}</span
    ><span class="mono focus-clock">{{ duration(elapsed(active)) }}</span
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
      v-model="newTitle"
      aria-label="Nowe zadanie"
      placeholder="Nowe zadanie…"
      maxlength="240"
      :disabled="loading"
    /><button
      class="primary-button"
      :disabled="busy || loading || !newTitle.trim()"
    >
      Dodaj zadanie
    </button>
  </form>
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
    <label
      >Projekt
      <select v-model="projectFilter" aria-label="Filtr projektu">
        <option value="">Wszystkie projekty</option>
        <option value="__none__">Bez projektu</option>
        <option v-for="project in projects" :key="project" :value="project">
          {{ project }}
        </option>
      </select></label
    >
  </div>
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
      <TaskCard
        v-for="task in group.tasks"
        :key="task.id"
        :task="task"
        :sessions="taskSessions(data, task.id)"
        :running="active?.taskId === task.id"
        :started="startedTaskIds.has(task.id)"
        :expanded="expanded.has(task.id)"
        :busy="busy"
        :now="now"
        :today-key="todayKey"
        :projects="projects"
        @toggle="toggle(task.id)"
        @act="act($event, task.id)"
        @priority="priority(task, $event)"
        @edit="emit('edit', task)"
        @remove="emit('remove', task)"
        @session="emit('session', $event)"
        @updated="emit('updated', $event)"
        @error="emit('error', $event)"
      />
    </section>
    <p v-if="loading" class="empty-state">Wczytywanie…</p>
    <p v-else-if="!visible.length" class="empty-state">
      {{ query ? "Brak pasujących zadań." : "Brak zadań w tym widoku." }}
    </p>
  </div>
</template>
