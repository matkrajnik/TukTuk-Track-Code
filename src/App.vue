<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { X } from "lucide-vue-next";
import TitleBar from "./components/TitleBar.vue";
import AppNav, { type View } from "./components/AppNav.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import SessionEditor, { type SessionTab } from "./components/SessionEditor.vue";
import IdleBanner from "./components/IdleBanner.vue";
import TasksView from "./views/TasksView.vue";
import ReportsView from "./views/ReportsView.vue";
import JiraView from "./JiraView.vue";
import QuickSwitch from "./QuickSwitch.vue";
import LibraryView from "./LibraryView.vue";
import DashboardView from "./DashboardView.vue";
import JournalView from "./JournalView.vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  change,
  read,
  desktop,
  onNavigate,
  type NavigateRequest,
  type Snapshot,
  type Task,
  type Session,
} from "./store";
import { dayKey } from "./time";

const data = ref<Snapshot>({ tasks: [], sessions: [] });
const now = ref(Date.now());
const busy = ref(false);
const loading = ref(true);
const error = ref("");
const view = ref<View>("tasks");
/** Zakładka „Historia i kopie”; przycisk ustawień w pasku otwiera „settings”. */
const libraryTab = ref("history");
function openSettings() {
  libraryTab.value = "settings";
  view.value = "library";
}
const selectedDay = ref(dayKey(now.value));
const editing = ref<Task | null>(null);
const editTitle = ref("");
const deleting = ref<Task | null>(null);
const editingSession = ref<Session | null>(null);
const sessionTab = ref<SessionTab>("hours");
const quickPreview = ref(false);
const modalOpen = computed(() =>
  Boolean(editing.value || deleting.value || editingSession.value),
);
const activeTask = computed(() => {
  const active = data.value.sessions.find((s) => s.endedAt === null);
  return data.value.tasks.find((t) => t.id === active?.taskId);
});
const todayKey = computed(() => dayKey(now.value));
watch(todayKey, (day, previous) => {
  if (selectedDay.value === previous) selectedDay.value = day;
});

function updated(next: Snapshot) {
  data.value = next;
  now.value = Date.now();
}
/** Licznik jawnych przejść do raportu dnia; ReportsView przełącza wtedy tryb. */
const reportRequest = ref(0);
function showReport(day: string) {
  selectedDay.value = day;
  reportRequest.value++;
  view.value = "reports";
}
function closeModal() {
  if (busy.value) return;
  editing.value = null;
  deleting.value = null;
  editingSession.value = null;
  error.value = "";
}
function openSession(session: Session, tab: SessionTab = "hours") {
  error.value = "";
  sessionTab.value = tab;
  editingSession.value = { ...session };
}
function openSplit(session: Session) {
  openSession(session, "split");
}
function openEdit(task: Task) {
  error.value = "";
  editing.value = task;
  editTitle.value = task.title;
}
function openDelete(task: Task) {
  error.value = "";
  deleting.value = task;
}
/** Żądania z powiadomień Windows (`navigate`) dla Zadań i Zamknięcia dnia. */
const tasksRequest = ref<NavigateRequest | null>(null);
const journalDayClose = ref(false);
const views: View[] = [
  "tasks",
  "reports",
  "jira",
  "dashboard",
  "journal",
  "library",
];
/**
 * Kliknięcie treści powiadomienia lub „Podziel…”: Rust już pokazał okno, tu
 * ustawiamy widok, filtr, rozwinięcie i edytor na zakładce Podziel.
 * Niczego nie zapisuje i nie przełącza timera.
 */
async function navigate(request: NavigateRequest) {
  const target = views.find((v) => v === request.view);
  if (!target) return;
  if (!busy.value) closeModal();
  view.value = target;
  if (target === "tasks") tasksRequest.value = { ...request };
  if (target === "journal" && request.dayClose) journalDayClose.value = true;
  if (request.split != null) {
    await refresh();
    const session =
      data.value.sessions.find((s) => s.id === request.split) ??
      data.value.sessions.find((s) => s.endedAt === null);
    if (session && !modalOpen.value) openSplit(session);
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
    updated(await change(action, id, title));
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  } finally {
    busy.value = false;
  }
}
async function saveEdit() {
  if (editing.value && (await act("rename", editing.value.id, editTitle.value)))
    editing.value = null;
}
async function remove() {
  if (deleting.value && (await act("delete", deleting.value.id)))
    deleting.value = null;
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

const unsubscribers: UnlistenFn[] = [];
let tick: ReturnType<typeof setInterval>;
onMounted(async () => {
  window.addEventListener("keydown", shortcut);
  tick = setInterval(() => {
    now.value = Date.now();
  }, 1000);
  try {
    unsubscribers.push(await onNavigate((request) => void navigate(request)));
  } catch (e) {
    error.value = String(e);
  }
  if (desktop) {
    try {
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
    <TitleBar v-if="desktop" @error="error = $event" />
    <AppNav
      v-model:view="view"
      @quick="openQuick"
      @settings="openSettings"
      @error="error = $event"
    />
    <main>
      <IdleBanner
        :tasks="data.tasks"
        @updated="data = $event"
        @error="error = $event"
      />
      <div v-if="error && !modalOpen" class="error" role="alert">
        {{ error
        }}<button aria-label="Zamknij komunikat" @click="error = ''">
          <X :size="16" />
        </button>
      </div>
      <KeepAlive :include="['TasksView', 'ReportsView']">
        <TasksView
          v-if="view === 'tasks'"
          :data="data"
          :now="now"
          :busy="busy"
          :loading="loading"
          :request="tasksRequest"
          @updated="updated"
          @error="error = $event"
          @edit="openEdit"
          @remove="openDelete"
          @session="openSession"
          @split="openSplit"
        />
        <LibraryView
          v-else-if="view === 'library'"
          v-model:tab="libraryTab"
          :data="data"
          :now="now"
          @updated="data = $event"
          @report="showReport"
        />
        <DashboardView
          v-else-if="view === 'dashboard'"
          :data="data"
          :now="now"
          @report="showReport"
        />
        <JournalView
          v-else-if="view === 'journal'"
          :data="data"
          :now="now"
          :day-close="journalDayClose"
          @day-close-shown="journalDayClose = false"
          @updated="updated"
          @edit="openSession"
          @split="openSplit"
          @report="showReport"
        />
        <ReportsView
          v-else-if="view === 'reports'"
          v-model:day="selectedDay"
          :open-request="reportRequest"
          :data="data"
          :now="now"
          :busy="busy"
          @updated="updated"
          @session="openSession"
        />
      </KeepAlive>
      <JiraView
        v-if="view === 'jira'"
        :active-url="activeTask?.jiraUrl"
        @updated="updated"
        @stop="act('stop')"
      />
    </main>
    <SessionEditor
      v-if="editingSession"
      v-model:busy="busy"
      :session="editingSession"
      :sessions="data.sessions"
      :tasks="data.tasks"
      :now="now"
      :tab="sessionTab"
      @updated="
        updated($event);
        editingSession = null;
      "
      @close="closeModal"
    />
    <ConfirmDialog
      v-else-if="editing"
      title="Edytuj zadanie"
      :error="error"
      :busy="busy"
      @close="closeModal"
    >
      <form @submit.prevent="saveEdit">
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
          ><button class="primary-button" :disabled="busy || !editTitle.trim()">
            Zapisz zmiany
          </button>
        </div>
      </form>
    </ConfirmDialog>
    <ConfirmDialog
      v-else-if="deleting"
      title="Usunąć zadanie?"
      label="Usuń zadanie"
      :error="error"
      :busy="busy"
      confirm-label="Usuń zadanie"
      @close="closeModal"
      @confirm="remove"
    >
      <p>
        „{{ deleting.title }}” oraz cała historia czasu tego zadania trafią do
        kosza i znikną z raportów. Możesz je przywrócić w widoku Historia i
        kopie.
      </p>
    </ConfirmDialog>
    <div v-if="quickPreview" class="modal-overlay">
      <QuickSwitch @close="quickPreview = false" @updated="data = $event" />
    </div>
  </div>
</template>
