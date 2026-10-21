<script setup lang="ts">
/**
 * Widok Jira: konto, wybrane tablice, lista „Przypisane do mnie” i tablica.
 * Części w `components/jira/`: połączenie (`JiraConnectionForm`), wybór
 * tablic (`JiraBoardPicker`), nowe zgłoszenie (`JiraCreateForm`), lista
 * (`JiraIssueList`) i tablica (`JiraBoard`). Formularze mają własne `busy`
 * i błąd; tu zostaje odczyt listy i tablicy.
 */
import { computed, onMounted, ref, watch } from "vue";
import { Columns3, List, Plus, RefreshCw, Search, Settings2 } from "lucide-vue-next";
import {
  desktop,
  jiraAccount,
  jiraBoard,
  jiraConnected,
  jiraIssues,
  jiraOpen,
  jiraStart,
  type JiraAccount,
  type JiraBoardRef,
  type JiraBoardView,
  type JiraCreated,
  type JiraListIssue,
  type JiraScope,
  type Snapshot,
} from "./store";
import { openSettings } from "./navigation";
import { useSettings } from "./target";
import { clock, dayKey, dayText } from "./time";
import JiraConnectionForm from "./components/jira/JiraConnectionForm.vue";
import JiraBoardPicker from "./components/jira/JiraBoardPicker.vue";
import JiraCreateForm from "./components/jira/JiraCreateForm.vue";
import JiraIssueList from "./components/jira/JiraIssueList.vue";
import JiraBoard from "./components/jira/JiraBoard.vue";

const props = defineProps<{
  activeUrl?: string | null;
  /** Dane zadań: czas zmierzony lokalnie i plan na kartach tablicy. */
  data?: Snapshot;
  now?: number;
}>();
const emit = defineEmits<{ updated: [data: Snapshot]; stop: [] }>();
const appSettings = useSettings();
/** Główny przełącznik zapisów: wyłączony = tablica tylko do odczytu. */
const writable = computed(() => appSettings.value.jiraWrite !== false);

const settings = ref<JiraAccount>({
  config: {
    site: "https://asistonwms.atlassian.net",
    email: "",
    displayName: "",
    boards: [],
  },
  connected: false,
});
// Szybkie dodawanie rozpoznaje klucze Jiry tylko przy połączonym koncie.
watch(
  () => settings.value.connected,
  (connected) => {
    if (desktop) jiraConnected.value = connected;
  },
);
const busy = ref(false),
  error = ref(""),
  warnings = ref<string[]>([]);
const editingConnection = ref(false),
  pickingBoards = ref(false),
  /** Każde otwarcie wyboru tablic zaczyna od zapisanych tablic. */
  pickerKey = ref(0);
const issues = ref<JiraListIssue[]>([]),
  query = ref(""),
  openOnly = ref(true),
  loaded = ref(false),
  updatedAt = ref("");
const visible = computed(() => {
  const q = query.value.trim().toLocaleLowerCase("pl");
  return issues.value.filter((i) =>
    `${i.key} ${i.summary}`.toLocaleLowerCase("pl").includes(q),
  );
});

/** Zapamiętany widok: lista albo tablica, tablica i zakres. */
const prefsKey = "todo-track-jira-view-v1";
interface Prefs {
  mode: "list" | "board";
  boardId: number | null;
  scope: JiraScope;
}
function loadPrefs(): Prefs {
  try {
    const raw = JSON.parse(localStorage.getItem(prefsKey) ?? "{}");
    return {
      mode: raw.mode === "board" ? "board" : "list",
      boardId: typeof raw.boardId === "number" ? raw.boardId : null,
      scope: raw.scope === "all" ? "all" : "mine",
    };
  } catch {
    return { mode: "list", boardId: null, scope: "mine" };
  }
}
const prefs = loadPrefs();
const mode = ref<Prefs["mode"]>(prefs.mode),
  boardId = ref<number | null>(prefs.boardId),
  scope = ref<JiraScope>(prefs.scope);
watch([mode, boardId, scope], () => {
  try {
    localStorage.setItem(
      prefsKey,
      JSON.stringify({
        mode: mode.value,
        boardId: boardId.value,
        scope: scope.value,
      }),
    );
  } catch {
    /* tylko wygoda */
  }
});

const board = ref<JiraBoardView | null>(null);
const notice = ref("");

/** Lokalne zadania zgłoszeń: czas, pomiar teraz i dzień planu. */
const local = computed(() => {
  const out = new Map<
    string,
    { ms: number; running: boolean; planned: string }
  >();
  const data = props.data;
  if (!data) return out;
  const now = props.now ?? Date.now();
  for (const task of data.tasks) {
    if (!task.jiraKey) continue;
    let ms = 0;
    let running = false;
    for (const s of data.sessions) {
      if (s.taskId !== task.id) continue;
      ms += Math.max(0, (s.endedAt ?? now) - s.startedAt);
      if (s.endedAt === null) running = true;
    }
    const day = task.done ? "" : (task.details?.plannedDay ?? "");
    out.set(task.jiraKey, {
      ms,
      running,
      planned: day
        ? day === dayKey(now)
          ? "dziś"
          : dayText(day)
        : "",
    });
  }
  return out;
});

async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const stamp = () => clock(Date.now());
async function fetchIssues() {
  issues.value = [];
  warnings.value = [];
  loaded.value = false;
  updatedAt.value = "";
  if (!settings.value.config.boards.length) return;
  const result = await jiraIssues(openOnly.value);
  issues.value = result.issues;
  warnings.value = result.warnings;
  loaded.value = true;
  updatedAt.value = stamp();
}
async function fetchBoard(refresh = false) {
  board.value = null;
  updatedAt.value = "";
  const list = settings.value.config.boards;
  if (!list.length) return;
  if (!list.some((b) => b.id === boardId.value)) boardId.value = list[0].id;
  board.value = await jiraBoard(
    boardId.value!,
    scope.value,
    openOnly.value,
    refresh,
  );
  updatedAt.value = stamp();
}
const refresh = (force = false) =>
  mode.value === "board" ? fetchBoard(force) : fetchIssues();
function setMode(next: Prefs["mode"]) {
  if (mode.value === next) return;
  mode.value = next;
  notice.value = "";
  void run(() => refresh());
}
function setScope(next: JiraScope) {
  if (scope.value === next) return;
  scope.value = next;
  void run(() => fetchBoard());
}
/** Wybór tablic od zapisanych (lista tablic ładuje się w `JiraBoardPicker`). */
function chooseBoards() {
  pickerKey.value++;
  pickingBoards.value = true;
}
function connected(account: JiraAccount) {
  void run(async () => {
    settings.value = account;
    editingConnection.value = false;
    if (!settings.value.config.boards.length) chooseBoards();
    else await refresh();
  });
}
function disconnected(account: JiraAccount) {
  settings.value = account;
  issues.value = [];
  board.value = null;
  warnings.value = [];
  loaded.value = false;
  updatedAt.value = "";
  editingConnection.value = false;
  pickingBoards.value = false;
}
function boardsSaved(boards: JiraBoardRef[]) {
  void run(async () => {
    settings.value.config.boards = boards;
    pickingBoards.value = false;
    await refresh();
  });
}
async function start(key: string) {
  await run(async () => emit("updated", await jiraStart(key)));
}
async function open(url: string) {
  await run(async () => {
    await jiraOpen(url);
  });
}

// --- Nowe zgłoszenie (formularz w `JiraCreateForm`) -------------------------
const creating = ref(false);
function toggleCreate() {
  if (!creating.value) notice.value = "";
  creating.value = !creating.value;
}
function created(result: JiraCreated) {
  void run(async () => {
    creating.value = false;
    notice.value = `Utworzono ${result.key}${
      result.snapshot ? " i zadanie w Todo Track" : ""
    }.`;
    if (result.warning)
      error.value = `Utworzono ${result.key}. ${result.warning}`;
    if (result.snapshot) emit("updated", result.snapshot);
    await refresh();
  });
}

onMounted(async () => {
  await run(async () => {
    settings.value = await jiraAccount();
    if (settings.value.connected) {
      if (settings.value.config.boards.length) await refresh();
      else chooseBoards();
    }
  });
});
</script>
<template>
  <section class="jira-view">
    <div class="page-heading">
      <h1>Jira</h1>
      <div v-if="settings.connected" class="jira-actions">
        <span class="hint account-name">{{ settings.config.displayName }}</span
        ><button class="text-link" @click="openSettings('jira')">
          Ustawienia → Jira</button
        ><button
          v-if="desktop"
          class="icon-button"
          aria-label="Ustawienia połączenia Jiry"
          title="Ustawienia połączenia"
          :disabled="busy"
          @click="editingConnection = !editingConnection"
        >
          <Settings2 :size="18" />
        </button>
      </div>
    </div>
    <p v-if="!desktop" class="hint jira-preview-note">
      Podgląd: symulowana Jira z przykładowymi zgłoszeniami.
    </p>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <JiraConnectionForm
      v-if="!settings.connected || editingConnection"
      :account="settings"
      @connected="connected"
      @disconnected="disconnected"
    />
    <template v-if="settings.connected">
      <div class="jira-toolbar">
        <div class="jira-board-chips">
          <span v-for="b in settings.config.boards" :key="b.id">{{
            b.name
          }}</span
          ><span v-if="!settings.config.boards.length" class="hint"
            >Brak wybranych tablic</span
          >
        </div>
        <button
          v-if="settings.config.boards.length"
          class="secondary-button"
          :disabled="busy || !writable"
          :aria-expanded="creating"
          :title="
            writable
              ? 'Nowe zgłoszenie w projekcie wybranej tablicy'
              : 'Zmiany w Jirze są wyłączone w Ustawieniach → Jira'
          "
          @click="toggleCreate"
        >
          <Plus :size="15" />Nowe zgłoszenie</button
        ><button
          class="secondary-button"
          :disabled="busy"
          @click="chooseBoards"
        >
          Wybierz tablice
        </button>
      </div>
      <JiraBoardPicker
        v-if="pickingBoards"
        :key="pickerKey"
        :selected="settings.config.boards"
        @saved="boardsSaved"
        @cancel="pickingBoards = false"
      />
      <JiraCreateForm
        v-if="creating"
        :preferred-project="board?.project"
        @created="created"
        @close="creating = false"
      />
      <div
        v-if="settings.config.boards.length"
        class="jira-search-toolbar"
      >
        <div class="jira-mode" role="group" aria-label="Widok zgłoszeń">
          <button
            type="button"
            :aria-pressed="mode === 'list'"
            :disabled="busy"
            @click="setMode('list')"
          >
            <List :size="14" />Lista</button
          ><button
            type="button"
            :aria-pressed="mode === 'board'"
            :disabled="busy"
            @click="setMode('board')"
          >
            <Columns3 :size="14" />Tablica
          </button>
        </div>
        <label class="search"
          ><Search :size="15" /><input
            v-model="query"
            :aria-label="
              mode === 'board' ? 'Szukaj na tablicy' : 'Szukaj moich zgłoszeń'
            "
            placeholder="Klucz lub nazwa zgłoszenia…" /></label
        ><label
          v-if="mode === 'list' || scope === 'mine'"
          class="check-label"
          ><input
            v-model="openOnly"
            type="checkbox"
            :disabled="busy"
            @change="run(() => refresh())"
          />Tylko otwarte</label
        ><button
          class="secondary-button"
          :disabled="busy"
          @click="run(() => refresh(true))"
        >
          <RefreshCw :size="15" />Odśwież
        </button>
      </div>
      <div
        v-if="mode === 'board' && settings.config.boards.length"
        class="jira-board-toolbar"
      >
        <label class="check-label"
          >Tablica<select
            :value="boardId ?? settings.config.boards[0]?.id"
            aria-label="Tablica Jiry"
            :disabled="busy"
            @change="
              boardId = Number(($event.target as HTMLSelectElement).value);
              run(() => fetchBoard());
            "
          >
            <option
              v-for="b in settings.config.boards"
              :key="b.id"
              :value="b.id"
            >
              {{ b.name }}
            </option>
          </select></label
        >
        <div class="jira-mode" role="group" aria-label="Zakres tablicy">
          <button
            type="button"
            :aria-pressed="scope === 'mine'"
            :disabled="busy"
            @click="setScope('mine')"
          >
            Moje</button
          ><button
            type="button"
            :aria-pressed="scope === 'all'"
            :disabled="busy"
            @click="setScope('all')"
          >
            Cała tablica
          </button>
        </div>
      </div>
      <div v-if="warnings.length && mode === 'list'" class="error" role="alert">
        <div>
          <strong>Nie wszystkie tablice zostały pobrane.</strong>
          <p v-for="warning in warnings" :key="warning">{{ warning }}</p>
        </div>
      </div>
      <p v-if="notice" class="hint jira-notice" role="status">{{ notice }}</p>
      <JiraIssueList
        v-if="mode === 'list'"
        :issues="visible"
        :has-boards="settings.config.boards.length > 0"
        :busy="busy"
        :loaded="loaded"
        :warned="warnings.length > 0"
        :query="query"
        :updated-at="updatedAt"
        :active-url="activeUrl"
        @open="open"
        @start="start"
        @stop="emit('stop')"
      />
      <JiraBoard
        v-else-if="board"
        :board="board"
        :scope="scope"
        :busy="busy"
        :writable="writable"
        :query="query"
        :updated-at="updatedAt"
        :active-url="activeUrl"
        :local="local"
        @open="open"
        @start="start"
        @stop="emit('stop')"
        @updated="emit('updated', $event)"
        @notice="notice = $event"
      />
      <p v-else-if="mode === 'board' && busy" class="hint" role="status">
        Proszę czekać…
      </p>
    </template>
  </section>
</template>
