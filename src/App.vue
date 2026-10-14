<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  Check,
  Play,
  Square,
  Plus,
  Search,
  ArrowDownToLine,
  Pencil,
  Trash2,
  X,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Sun,
  Moon,
  CalendarDays,
  ListTodo,
  Minus,
  Maximize2,
  Minimize2,
  ExternalLink,
} from "lucide-vue-next";
import JiraView from "./JiraView.vue";
import QuickSwitch from "./QuickSwitch.vue";
import LibraryView from "./LibraryView.vue";
import DashboardView from "./DashboardView.vue";
import TaskPlanning from "./TaskPlanning.vue";
import JournalView from "./JournalView.vue";
import ManualTime from "./ManualTime.vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  change,
  setPriority,
  editSession,
  read,
  desktop,
  duration,
  type Snapshot,
  type Task,
  type Session,
} from "./store";
import { dayKey, dayBounds, inDay, localInput } from "./time";

const data = ref<Snapshot>({ tasks: [], sessions: [] });
const now = ref(Date.now());
const busy = ref(false);
const loading = ref(true);
const error = ref("");
const newTitle = ref("");
const query = ref("");
const view = ref<
  "tasks" | "reports" | "jira" | "dashboard" | "journal" | "library"
>("tasks");
const projectFilter = ref("");
const todayOnly = ref(false);
const projects = computed(() =>
  [
    ...new Set(
      data.value.tasks
        .map((t) => t.details?.project)
        .filter((p): p is string => Boolean(p)),
    ),
  ].sort((a, b) => a.localeCompare(b, "pl")),
);
const maximized = ref(false);
async function windowAction(action: "minimize" | "maximize" | "close") {
  try {
    const window = getCurrentWindow();
    if (action === "minimize") await window.minimize();
    else if (action === "close") await window.close();
    else {
      await window.toggleMaximize();
      maximized.value = await window.isMaximized();
    }
  } catch (e) {
    error.value = String(e);
  }
}
async function openJira(url: string) {
  try {
    if (desktop) await invoke("jira_open", { url });
  } catch (e) {
    error.value = String(e);
  }
}
const filter = ref("active");
const expanded = ref(new Set<number>());
const reportExpanded = ref(new Set<number>());
const tsVisible = ref(false);
const tsTextArea = ref<HTMLTextAreaElement | null>(null);
const tsCopyStatus = ref("");
const selectedDay = ref(dayKey(now.value));
const editing = ref<Task | null>(null);
const editTitle = ref("");
const deleting = ref<Task | null>(null);
const editingSession = ref<Session | null>(null);
const editStart = ref("");
const editEnd = ref("");
const theme = ref<"dark" | "light">("dark");
try {
  if (localStorage.getItem("todo-track-theme") === "light")
    theme.value = "light";
} catch {}
watch(
  theme,
  (value) => {
    document.documentElement.dataset.theme = value;
    try {
      localStorage.setItem("todo-track-theme", value);
    } catch {}
  },
  { immediate: true },
);
const modalOpen = computed(() =>
  Boolean(editing.value || deleting.value || editingSession.value),
);
let previousFocus: HTMLElement | null = null;
watch(modalOpen, async (open) => {
  if (open) {
    previousFocus = document.activeElement as HTMLElement;
    await nextTick();
    document.querySelector<HTMLElement>(".modal input, .modal button")?.focus();
  } else previousFocus?.focus();
});
function closeModal() {
  if (busy.value) return;
  editing.value = null;
  deleting.value = null;
  editingSession.value = null;
  error.value = "";
}
function trapFocus(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const targets = [
    ...document.querySelectorAll<HTMLElement>(
      ".modal input:not(:disabled), .modal button:not(:disabled)",
    ),
  ];
  const first = targets[0],
    last = targets[targets.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
}
const active = computed(() =>
  data.value.sessions.find((s) => s.endedAt === null),
);
const activeTask = computed(() =>
  data.value.tasks.find((t) => t.id === active.value?.taskId),
);
const startedTaskIds = computed(
  () => new Set(data.value.sessions.map((session) => session.taskId)),
);
const visible = computed(() =>
  data.value.tasks.filter(
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
        data.value.sessions.some(
          (s) => s.taskId === t.id && inDay(s, todayKey.value, now.value) > 0,
        )) &&
      t.title
        .toLocaleLowerCase("pl")
        .includes(query.value.toLocaleLowerCase("pl")),
  ),
);
const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? now.value) - s.startedAt);
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
      tasks: group.tasks.sort(
        (a, b) =>
          Number(b.id === active.value?.taskId) -
            Number(a.id === active.value?.taskId) ||
          (b.details?.priority ?? 0) - (a.details?.priority ?? 0),
      ),
    })),
);
const taskSessions = (id: number) =>
  data.value.sessions
    .filter((s) => s.taskId === id)
    .sort((a, b) => b.startedAt - a.startedAt || b.id - a.id);
const taskTime = (id: number) =>
  taskSessions(id).reduce((sum, s) => sum + elapsed(s), 0);
const todayKey = computed(() => dayKey(now.value));
watch(todayKey, (day, previous) => {
  if (selectedDay.value === previous) selectedDay.value = day;
});
const today = computed(() =>
  data.value.sessions.reduce(
    (sum, s) => sum + inDay(s, todayKey.value, now.value),
    0,
  ),
);
const report = computed(() =>
  data.value.tasks
    .map((task) => {
      const sessions = taskSessions(task.id).filter(
        (s) =>
          inDay(s, selectedDay.value, now.value) > 0 ||
          (s.endedAt === null && dayKey(s.startedAt) === selectedDay.value),
      );
      return {
        task,
        sessions,
        total: sessions.reduce(
          (sum, s) => sum + inDay(s, selectedDay.value, now.value),
          0,
        ),
      };
    })
    .filter((row) => row.sessions.length)
    .sort((a, b) => b.total - a.total || a.task.id - b.task.id),
);
const reportTotal = computed(() =>
  report.value.reduce((sum, row) => sum + row.total, 0),
);
const decimalHours = (milliseconds: number) =>
  (milliseconds / 3_600_000).toFixed(2).replace(".", ",");
const tsIncludeNotes = ref(true);
const tsReport = computed(() =>
  [
    `Raport TS — ${selectedDay.value}`,
    "Zadanie\tCzas (h)",
    ...report.value.map(
      ({ task, total }) =>
        `${[task.title, tsIncludeNotes.value ? task.details?.notes?.[selectedDay.value] : ""].filter(Boolean).join(" — ").replace(/\s+/g, " ").trim()}\t${decimalHours(total)}`,
    ),
    `Razem\t${decimalHours(reportTotal.value)}`,
  ].join("\n"),
);
watch([selectedDay, tsVisible], () => {
  tsCopyStatus.value = "";
});
async function copyTsReport() {
  const text = tsReport.value;
  tsCopyStatus.value = "";
  try {
    await navigator.clipboard.writeText(text);
    tsCopyStatus.value = "Skopiowano raport.";
  } catch {
    tsTextArea.value?.focus();
    tsTextArea.value?.select();
    try {
      if (document.execCommand("copy")) {
        tsCopyStatus.value = "Skopiowano raport.";
        return;
      }
    } catch {}
    tsCopyStatus.value = "Zaznaczono raport. Naciśnij Ctrl+C, aby skopiować.";
  }
}
const timeLabel = (time: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(time);
const dayLabel = (time: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(time);
const stamp = (time: number) => `${dayLabel(time)}, ${timeLabel(time)}`;
const reportLabel = computed(() =>
  new Intl.DateTimeFormat("pl-PL", {
    weekday: "long",
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(dayBounds(selectedDay.value)[0]),
);
function shiftDay(offset: number) {
  const d = new Date(dayBounds(selectedDay.value)[0]);
  d.setDate(d.getDate() + offset);
  selectedDay.value = dayKey(d.getTime());
}
function chooseDay(event: Event) {
  const value = (event.target as HTMLInputElement).value;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value) && Number.isFinite(dayBounds(value)[0]))
    selectedDay.value = value;
  else (event.target as HTMLInputElement).value = selectedDay.value;
}
function toggle(set: Set<number>, id: number) {
  if (set.has(id)) set.delete(id);
  else set.add(id);
}
function openSession(session: Session) {
  error.value = "";
  editingSession.value = { ...session };
  editStart.value = localInput(session.startedAt);
  editEnd.value = localInput(session.endedAt!);
}
async function saveSession() {
  if (busy.value || !editingSession.value) return;
  busy.value = true;
  error.value = "";
  try {
    const s = editingSession.value;
    // Preserve milliseconds and the exact offset of unchanged timestamps (including DST folds).
    const start =
      editStart.value === localInput(s.startedAt)
        ? s.startedAt
        : new Date(editStart.value).getTime();
    const end =
      editEnd.value === localInput(s.endedAt!)
        ? s.endedAt!
        : new Date(editEnd.value).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end))
      throw new Error("Wprowadź poprawną datę i godzinę.");
    data.value = await editSession(s.id, start, end);
    now.value = Date.now();
    editingSession.value = null;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function refresh() {
  try {
    data.value = await read();
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
async function act(
  action: string,
  id: number | null = null,
  title: string | null = null,
) {
  if (busy.value) return false;
  busy.value = true;
  error.value = "";
  try {
    data.value = await change(action, id, title);
    now.value = Date.now();
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  } finally {
    busy.value = false;
  }
}
async function add() {
  if (await act("add", null, newTitle.value)) newTitle.value = "";
}
async function saveEdit() {
  if (editing.value && (await act("rename", editing.value.id, editTitle.value)))
    editing.value = null;
}
async function remove() {
  if (deleting.value && (await act("delete", deleting.value.id)))
    deleting.value = null;
}
async function hide() {
  try {
    await getCurrentWindow().hide();
  } catch (e) {
    error.value = String(e);
  }
}
const unsubscribers: UnlistenFn[] = [];
const quickPreview = ref(false);
const idleNotice = ref<{
  taskId: number;
  idleAt: number;
  returnedAt: number;
} | null>(null);
async function readIdle() {
  idleNotice.value = await invoke("idle_notice");
}
async function resolveIdle(trim: boolean) {
  try {
    data.value = await invoke("idle_resolve", { trim });
    idleNotice.value = null;
  } catch (e) {
    error.value = String(e);
  }
}
async function openQuick() {
  if (desktop) {
    try {
      await invoke("quick_open");
    } catch (e) {
      error.value = String(e);
    }
  } else quickPreview.value = true;
}
function shortcut(e: KeyboardEvent) {
  if (e.ctrlKey && e.key.toLowerCase() === "k") {
    e.preventDefault();
    void openQuick();
  }
}
async function priority(task: Task, e: Event) {
  busy.value = true;
  try {
    data.value = await setPriority(
      task.id,
      Number((e.target as HTMLSelectElement).value),
    );
  } catch (err) {
    error.value = String(err);
    (e.target as HTMLSelectElement).value = String(task.details?.priority ?? 0);
  } finally {
    busy.value = false;
  }
}
let tick: ReturnType<typeof setInterval>;
onMounted(async () => {
  window.addEventListener("keydown", shortcut);
  tick = setInterval(() => {
    now.value = Date.now();
  }, 1000);
  if (desktop) {
    try {
      maximized.value = await getCurrentWindow().isMaximized();
      unsubscribers.push(
        await getCurrentWindow().onResized(async () => {
          maximized.value = await getCurrentWindow().isMaximized();
        }),
      );
      unsubscribers.push(
        await listen("prompt-task", async () => {
          view.value = "tasks";
          closeModal();
          await nextTick();
          document
            .querySelector<HTMLInputElement>('input[aria-label="Nowe zadanie"]')
            ?.focus();
        }),
      );
      unsubscribers.push(await listen("data-changed", refresh));
      unsubscribers.push(
        await listen<string>("storage-error", (e) => {
          error.value = e.payload;
        }),
      );
    } catch (e) {
      error.value = String(e);
    }
  }
  if (desktop) {
    try {
      unsubscribers.push(await listen("idle-notice", readIdle));
      await readIdle();
    } catch {}
  }
  await refresh();
});
onUnmounted(() => {
  clearInterval(tick);
  window.removeEventListener("keydown", shortcut);
  unsubscribers.forEach((fn) => fn());
});
</script>

<template>
  <div class="app-shell">
    <div v-if="desktop" class="titlebar">
      <div data-tauri-drag-region class="titlebar-drag">Todo Track</div>
      <div class="window-controls">
        <button
          aria-label="Minimalizuj okno"
          title="Minimalizuj na pasek zadań"
          @click="windowAction('minimize')"
        >
          <Minus :size="15" />
        </button>
        <button
          :aria-label="
            maximized ? 'Przywróć rozmiar okna' : 'Maksymalizuj okno'
          "
          @click="windowAction('maximize')"
        >
          <Minimize2 v-if="maximized" :size="14" /><Maximize2
            v-else
            :size="14"
          />
        </button>
        <button
          class="window-close"
          aria-label="Zamknij okno do traya"
          title="Schowaj do traya"
          @click="windowAction('close')"
        >
          <X :size="17" />
        </button>
      </div>
    </div>
    <header class="topbar">
      <a class="brand" href="#" @click.prevent="view = 'tasks'"
        ><span class="brand-icon"><Check :size="18" /></span>todo<span
          >track</span
        ></a
      >
      <nav aria-label="Nawigacja główna">
        <button
          :class="{ selected: view === 'tasks' }"
          :aria-pressed="view === 'tasks'"
          @click="view = 'tasks'"
        >
          <ListTodo :size="16" />Zadania
        </button>
        <button
          :class="{ selected: view === 'dashboard' }"
          :aria-pressed="view === 'dashboard'"
          @click="view = 'dashboard'"
        >
          Dashboard
        </button>
        <button
          :class="{ selected: view === 'journal' }"
          :aria-pressed="view === 'journal'"
          @click="view = 'journal'"
        >
          Plan i historia
        </button>
        <button
          :class="{ selected: view === 'reports' }"
          :aria-pressed="view === 'reports'"
          @click="view = 'reports'"
        >
          <CalendarDays :size="16" />Raporty dzienne
        </button>
        <button
          :class="{ selected: view === 'jira' }"
          :aria-pressed="view === 'jira'"
          @click="view = 'jira'"
        >
          <ExternalLink :size="16" />Jira
        </button>
        <button
          :class="{ selected: view === 'library' }"
          @click="view = 'library'"
        >
          Historia i kopie
        </button>
      </nav>
      <div class="header-actions">
        <button
          class="today-button"
          aria-label="Szybkie przełączanie"
          title="Ctrl+K / Ctrl+Shift+Space"
          @click="openQuick"
        >
          ⇄
        </button>
        <span v-if="!desktop" class="preview-label">Podgląd</span>
        <button
          class="icon-button"
          :aria-label="
            theme === 'dark' ? 'Włącz tryb jasny' : 'Włącz tryb ciemny'
          "
          :title="theme === 'dark' ? 'Tryb jasny' : 'Tryb ciemny'"
          @click="theme = theme === 'dark' ? 'light' : 'dark'"
        >
          <Sun v-if="theme === 'dark'" :size="18" /><Moon v-else :size="18" />
        </button>
        <button
          v-if="desktop"
          class="icon-button"
          aria-label="Schowaj do traya"
          title="Schowaj do traya"
          @click="hide"
        >
          <ArrowDownToLine :size="18" />
        </button>
      </div>
    </header>
    <main>
      <section v-if="idleNotice" class="idle-banner" role="alert">
        <h2>Wracasz po przerwie?</h2>
        <p>
          {{ data.tasks.find((t) => t.id === idleNotice?.taskId)?.title }} ·
          brak aktywności od
          {{ new Date(idleNotice.idleAt).toLocaleTimeString("pl-PL") }}. Czy
          cały ten czas był pracą?
        </p>
        <div class="planning-actions">
          <button class="today-button" @click="resolveIdle(false)">
            Zachowaj cały czas</button
          ><button class="today-button" @click="resolveIdle(true)">
            Zatrzymaj na początku przerwy
          </button>
        </div>
      </section>
      <div v-if="error && !modalOpen" class="error" role="alert">
        {{ error
        }}<button aria-label="Zamknij komunikat" @click="error = ''">
          <X :size="16" />
        </button>
      </div>
      <template v-if="view === 'tasks'">
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
            ><input v-model="todayOnly" type="checkbox" />Dzisiaj: plan i
            praca</label
          >
          <label
            >Projekt
            <select v-model="projectFilter" aria-label="Filtr projektu">
              <option value="">Wszystkie projekty</option>
              <option value="__none__">Bez projektu</option>
              <option
                v-for="project in projects"
                :key="project"
                :value="project"
              >
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
            <article
              v-for="task in group.tasks"
              :key="task.id"
              class="task-card"
              :class="{
                isRunning: active?.taskId === task.id,
                isDone: task.done,
              }"
            >
              <div class="task-row">
                <button
                  class="checkbox"
                  :class="{ checked: task.done }"
                  :aria-label="
                    (task.done ? 'Przywróć: ' : 'Ukończ: ') + task.title
                  "
                  :disabled="busy"
                  @click="act('toggle', task.id)"
                >
                  <Check v-if="task.done" :size="14" />
                </button>
                <button
                  class="task-expand"
                  :aria-label="'Przedziały: ' + task.title"
                  :aria-expanded="expanded.has(task.id)"
                  :aria-controls="'sessions-' + task.id"
                  @click="toggle(expanded, task.id)"
                >
                  <span class="task-name-status">
                    <span class="task-title">{{ task.title }}</span>
                    <span
                      v-if="task.jiraKey || (!task.done && (task.details?.waiting || startedTaskIds.has(task.id)))"
                      class="task-meta"
                    >
                    <span v-if="task.jiraKey" class="task-jira-key mono">{{ task.jiraKey }}</span>
                    <span
                      v-if="
                        !task.done &&
                        !task.details?.waiting &&
                        startedTaskIds.has(task.id)
                      "
                      class="task-status"
                      >W trakcie</span
                    >
                    <span
                      v-if="task.details?.waiting && !task.done"
                      class="task-status"
                      >Czekam na: {{ task.details.waiting }}</span
                    >
                    </span>
                    <span
                      v-if="task.details?.estimateMinutes"
                      class="task-status"
                      >Plan
                      {{ duration(task.details.estimateMinutes * 60000) }} ·
                      {{
                        Math.round(
                          (taskTime(task.id) /
                            (task.details.estimateMinutes * 60000)) *
                            100,
                        )
                      }}%</span
                    >
                    <span
                      v-if="task.details?.dueDay && !task.done"
                      class="task-deadline"
                      :class="{ overdue: task.details.dueDay < todayKey }"
                      >Termin: {{ task.details.dueDay }}</span
                    > </span
                  ><span class="task-duration mono">{{
                    duration(taskTime(task.id))
                  }}</span
                  ><ChevronDown
                    :size="16"
                    :class="{ rotated: expanded.has(task.id) }"
                  />
                </button>
                <select
                  class="inline-priority"
                  :value="task.details?.priority ?? 0"
                  :aria-label="'Priorytet: ' + task.title"
                  :disabled="busy"
                  @change="priority(task, $event)"
                >
                  <option :value="1">↑ Wysoki</option>
                  <option :value="0">Normalny</option>
                  <option :value="-1">↓ Niski</option>
                </select>
                <button
                  v-if="!task.done"
                  class="row-timer"
                  :class="{ stop: active?.taskId === task.id }"
                  :aria-label="
                    (active?.taskId === task.id ? 'Stop: ' : 'Start: ') +
                    task.title
                  "
                  :disabled="busy"
                  @click="
                    act(active?.taskId === task.id ? 'stop' : 'start', task.id)
                  "
                >
                  <Square v-if="active?.taskId === task.id" :size="12" /><Play
                    v-else
                    :size="12"
                  />{{ active?.taskId === task.id ? "Stop" : "Start" }}
                </button>
                <span v-else class="done-label">Gotowe</span>
              </div>
              <div
                v-if="expanded.has(task.id)"
                :id="'sessions-' + task.id"
                class="task-details"
              >
                <div class="details-heading">
                  <span>Przedziały pracy</span>
                  <div>
                    <button
                      class="today-button"
                      :disabled="busy"
                      @click="act('archive', task.id)"
                    >
                      Archiwizuj
                    </button>
                    <button
                      v-if="task.jiraUrl"
                      class="text-link"
                      :aria-label="'Otwórz w Jirze: ' + task.jiraKey"
                      @click="openJira(task.jiraUrl)"
                    >
                      {{ task.jiraKey }}<ExternalLink :size="13" />
                    </button>
                    <button
                      class="icon-button"
                      :disabled="busy"
                      :aria-label="'Edytuj: ' + task.title"
                      title="Edytuj zadanie"
                      @click="
                        error = '';
                        editing = task;
                        editTitle = task.title;
                      "
                    >
                      <Pencil :size="15" /></button
                    ><button
                      class="icon-button danger"
                      :disabled="busy"
                      :aria-label="'Usuń: ' + task.title"
                      title="Usuń zadanie"
                      @click="
                        error = '';
                        deleting = task;
                      "
                    >
                      <Trash2 :size="15" />
                    </button>
                  </div>
                </div>
                <div
                  v-for="session in taskSessions(task.id)"
                  :key="session.id"
                  class="session-row"
                >
                  <div class="interval">
                    <time
                      :datetime="new Date(session.startedAt).toISOString()"
                      >{{ stamp(session.startedAt) }}</time
                    ><span class="interval-arrow">→</span
                    ><time
                      v-if="session.endedAt !== null"
                      :datetime="new Date(session.endedAt).toISOString()"
                      >{{ stamp(session.endedAt) }}</time
                    ><span v-else class="running-label">W trakcie</span>
                  </div>
                  <span class="mono session-duration">{{
                    duration(elapsed(session))
                  }}</span>
                  <button
                    class="icon-button"
                    :aria-label="'Edytuj przedział ' + session.id"
                    :disabled="busy || session.endedAt === null"
                    :title="
                      session.endedAt === null
                        ? 'Zatrzymaj pomiar przed edycją'
                        : 'Edytuj przedział'
                    "
                    @click="openSession(session)"
                  >
                    <Pencil :size="14" />
                  </button>
                </div>
                <p v-if="!taskSessions(task.id).length" class="empty-detail">
                  Brak przedziałów. Kliknij Start, aby rozpocząć pomiar.
                </p>
                <p v-if="active?.taskId === task.id" class="hint">
                  Zatrzymaj bieżący pomiar, aby edytować jego godziny.
                </p>
                <TaskPlanning
                  :task="task"
                  :projects="projects"
                  @updated="
                    data = $event;
                    now = Date.now();
                  "
                />
              </div>
            </article>
          </section>
          <p v-if="loading" class="empty-state">Wczytywanie…</p>
          <p v-else-if="!visible.length" class="empty-state">
            {{ query ? "Brak pasujących zadań." : "Brak zadań w tym widoku." }}
          </p>
        </div>
      </template>
      <LibraryView
        v-else-if="view === 'library'"
        :data="data"
        :now="now"
        @updated="data = $event"
        @report="
          selectedDay = $event;
          view = 'reports';
        "
      />
      <DashboardView
        v-else-if="view === 'dashboard'"
        :data="data"
        :now="now"
        @report="
          selectedDay = $event;
          view = 'reports';
        "
      />
      <JournalView
        v-else-if="view === 'journal'"
        :data="data"
        :now="now"
        @updated="
          data = $event;
          now = Date.now();
        "
        @edit="openSession"
        @report="
          selectedDay = $event;
          view = 'reports';
        "
      />
      <template v-else-if="view === 'reports'">
        <div class="page-heading">
          <h1>Raporty dzienne</h1>
          <div class="date-picker">
            <button
              class="icon-button"
              aria-label="Poprzedni dzień"
              @click="shiftDay(-1)"
            >
              <ChevronLeft :size="18" /></button
            ><input
              type="date"
              aria-label="Dzień raportu"
              :value="selectedDay"
              @change="chooseDay"
            /><button
              class="icon-button"
              aria-label="Następny dzień"
              @click="shiftDay(1)"
            >
              <ChevronRight :size="18" /></button
            ><button class="today-button" @click="selectedDay = todayKey">
              Dzisiaj
            </button>
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
        <section class="report-summary" aria-label="Podsumowanie dnia">
          <div>
            <p>{{ reportLabel }}</p>
            <h2>Przepracowany czas</h2>
          </div>
          <strong class="mono report-total">{{ duration(reportTotal) }}</strong>
        </section>
        <ManualTime
          :tasks="data.tasks"
          :day="selectedDay"
          @updated="
            data = $event;
            now = Date.now();
          "
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
            <button class="today-button" @click="copyTsReport">
              Kopiuj raport
            </button>
          </div>
          <label class="inline-check"
            ><input v-model="tsIncludeNotes" type="checkbox" />Dołącz notatki z
            tego dnia</label
          >
          <p class="report-note">
            Godziny dziesiętne, np. 1 godz. 30 min = 1,50 h. Zaokrąglenie do
            dwóch miejsc; suma liczona z pełnego czasu.
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
        </section>
        <div class="report-list">
          <article v-for="row in report" :key="row.task.id" class="report-card">
            <button
              class="report-expand"
              :aria-expanded="reportExpanded.has(row.task.id)"
              :aria-label="'Przedziały raportu: ' + row.task.title"
              :aria-controls="'report-' + row.task.id"
              @click="toggle(reportExpanded, row.task.id)"
            >
              <span class="task-title">{{ row.task.title }}</span
              ><strong class="mono">{{ duration(row.total) }}</strong
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
                    session.endedAt === null
                      ? "W trakcie"
                      : stamp(session.endedAt)
                  }}</span>
                </div>
                <span class="mono session-duration">{{
                  duration(inDay(session, selectedDay, now))
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
                  @click="openSession(session)"
                >
                  <Pencil :size="14" />
                </button>
              </div>
            </div>
          </article>
          <p v-if="!report.length" class="empty-state">
            Brak zarejestrowanej pracy w tym dniu.
          </p>
        </div>
      </template>
      <JiraView
        v-if="view === 'jira'"
        :active-url="activeTask?.jiraUrl"
        @updated="
          data = $event;
          now = Date.now();
        "
        @stop="act('stop')"
      />
    </main>
    <div
      v-if="modalOpen"
      class="modal-overlay"
      @click.self="closeModal"
      @keydown.esc="closeModal"
    >
      <section
        class="modal"
        role="dialog"
        aria-modal="true"
        :aria-label="
          editingSession
            ? 'Edytuj przedział'
            : editing
              ? 'Edytuj zadanie'
              : 'Usuń zadanie'
        "
        @keydown="trapFocus"
      >
        <h2>
          {{
            editingSession
              ? "Edytuj przedział"
              : editing
                ? "Edytuj zadanie"
                : "Usunąć zadanie?"
          }}
        </h2>
        <p v-if="error" class="error" role="alert">{{ error }}</p>
        <form v-if="editingSession" @submit.prevent="saveSession">
          <label for="session-start">Początek</label
          ><input
            id="session-start"
            v-model="editStart"
            type="datetime-local"
            step="1"
            required
            :disabled="busy"
          /><label for="session-end">Koniec</label
          ><input
            id="session-end"
            v-model="editEnd"
            type="datetime-local"
            step="1"
            required
            :disabled="busy"
          />
          <p class="hint">
            Czas lokalny. Zmiana przeliczy czas zadania i raporty dzienne.
          </p>
          <div class="modal-actions">
            <button type="button" :disabled="busy" @click="closeModal">
              Anuluj</button
            ><button class="primary-button" :disabled="busy">
              Zapisz przedział
            </button>
          </div>
        </form>
        <form v-else-if="editing" @submit.prevent="saveEdit">
          <label for="edit-title">Nazwa zadania</label
          ><input
            id="edit-title"
            v-model="editTitle"
            maxlength="240"
            :disabled="busy"
          />
          <div class="modal-actions">
            <button type="button" :disabled="busy" @click="closeModal">
              Anuluj</button
            ><button
              class="primary-button"
              :disabled="busy || !editTitle.trim()"
            >
              Zapisz zmiany
            </button>
          </div>
        </form>
        <template v-else
          ><p>
            „{{ deleting?.title }}” oraz cała historia czasu tego zadania trafią
            do kosza i znikną z raportów. Możesz je przywrócić w widoku Historia
            i kopie.
          </p>
          <div class="modal-actions">
            <button :disabled="busy" @click="closeModal">Anuluj</button
            ><button class="delete-button" :disabled="busy" @click="remove">
              Usuń zadanie
            </button>
          </div></template
        >
      </section>
    </div>
    <div v-if="quickPreview" class="modal-overlay">
      <QuickSwitch @close="quickPreview = false" @updated="data = $event" />
    </div>
  </div>
</template>
