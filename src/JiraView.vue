<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import {
  Columns3,
  Ellipsis,
  ExternalLink,
  List,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings2,
  Square,
} from "lucide-vue-next";
import {
  desktop,
  duration,
  initials,
  jiraAccount,
  jiraBoard,
  jiraBoardMove,
  jiraBoards,
  jiraConnect,
  jiraConnected,
  jiraCreate,
  jiraDisconnect,
  jiraIssues,
  jiraIssueTypeFields,
  jiraIssueTypes,
  jiraOpen,
  jiraProjects,
  jiraSelectBoards,
  jiraStart,
  read,
  type JiraAccount,
  type JiraBoardIssue,
  type JiraBoardRef,
  type JiraBoardView,
  type JiraIssueType,
  type JiraListIssue,
  type JiraMoveOption,
  type JiraPerson,
  type JiraProject,
  type JiraScope,
  type Snapshot,
} from "./store";
import { openSettings } from "./navigation";
import { useSettings } from "./target";
import { dayKey, dayText } from "./time";
import JiraIssueActions from "./components/JiraIssueActions.vue";

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
const site = ref(settings.value.config.site),
  email = ref(""),
  apiToken = ref("");
const busy = ref(false),
  error = ref(""),
  warnings = ref<string[]>([]);
const editingConnection = ref(false),
  pickingBoards = ref(false);
const boardName = ref(""),
  boards = ref<JiraBoardRef[]>([]),
  chosen = ref<JiraBoardRef[]>([]),
  nextStart = ref<number | null>(null);
const issues = ref<JiraListIssue[]>([]),
  query = ref(""),
  openOnly = ref(true),
  loaded = ref(false),
  updatedAt = ref("");
const matches = (text: string) =>
  text
    .toLocaleLowerCase("pl")
    .includes(query.value.trim().toLocaleLowerCase("pl"));
const visible = computed(() =>
  issues.value.filter((i) => matches(`${i.key} ${i.summary}`)),
);

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
const boardError = ref("");
const openMenu = ref<string | null>(null);
const dragging = ref<string | null>(null);
const dropTarget = ref<number | null>(null);
const pending = ref<string[]>([]);
/** Kilka przejść do kolumny: wybór użytkownika (karta czeka w nowej kolumnie). */
const choice = ref<{
  card: JiraBoardIssue;
  column: number;
  from: number;
  options: JiraMoveOption[];
} | null>(null);

const columnCards = computed(() =>
  (board.value?.columns ?? []).map((_, i) =>
    (board.value?.issues ?? []).filter(
      (c) => c.column === i && matches(`${c.key} ${c.summary}`),
    ),
  ),
);

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
const stamp = () =>
  new Date().toLocaleTimeString("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  });
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
  boardError.value = "";
  choice.value = null;
  openMenu.value = null;
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
async function connect() {
  await run(async () => {
    try {
      settings.value = await jiraConnect(site.value, email.value, apiToken.value);
    } finally {
      apiToken.value = "";
    }
    site.value = settings.value.config.site;
    email.value = settings.value.config.email;
    editingConnection.value = false;
    if (!settings.value.config.boards.length) {
      chosen.value = [];
      pickingBoards.value = true;
      await fetchBoards(false);
    } else await refresh();
  });
}
async function disconnect() {
  await run(async () => {
    settings.value = await jiraDisconnect();
    issues.value = [];
    board.value = null;
    boards.value = [];
    chosen.value = [];
    apiToken.value = "";
    email.value = "";
    warnings.value = [];
    loaded.value = false;
    updatedAt.value = "";
    editingConnection.value = false;
    pickingBoards.value = false;
  });
}
async function fetchBoards(more: boolean) {
  const page = await jiraBoards(
    boardName.value.trim(),
    more ? (nextStart.value ?? 0) : 0,
  );
  const combined = more ? [...boards.value, ...page.values] : page.values;
  boards.value = [...new Map(combined.map((b) => [b.id, b])).values()];
  nextStart.value = page.nextStart;
}
function select(b: JiraBoardRef) {
  if (chosen.value.some((x) => x.id === b.id))
    chosen.value = chosen.value.filter((x) => x.id !== b.id);
  else if (chosen.value.length < 20) chosen.value.push(b);
}
async function chooseBoards() {
  await run(async () => {
    chosen.value = [...settings.value.config.boards];
    pickingBoards.value = true;
    await fetchBoards(false);
  });
}
async function saveBoards() {
  await run(async () => {
    await jiraSelectBoards(chosen.value);
    settings.value.config.boards = [...chosen.value];
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

// --- Tablica: przejścia przez przeciągnięcie i „Przesuń do…” -------------
function onDragStart(event: DragEvent, card: JiraBoardIssue) {
  if (!writable.value || pending.value.includes(card.key)) {
    event.preventDefault();
    return;
  }
  dragging.value = card.key;
  event.dataTransfer?.setData("text/plain", card.key);
  if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
}
function onDragOver(event: DragEvent, column: number) {
  if (!dragging.value) return;
  event.preventDefault();
  if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
  dropTarget.value = column;
}
function onDrop(event: DragEvent, column: number) {
  event.preventDefault();
  const key = dragging.value || event.dataTransfer?.getData("text/plain");
  dragging.value = null;
  dropTarget.value = null;
  const card = board.value?.issues.find((c) => c.key === key);
  if (card) void moveCard(card, column);
}
function onDragEnd() {
  dragging.value = null;
  dropTarget.value = null;
}
/**
 * Optymistyczne przesunięcie karty; błąd, brak przejścia albo przejście
 * wymagające pól cofa kartę z komunikatem. Nic nie dzieje się samo: tylko
 * przeciągnięcie albo „Przesuń do…” wywołuje przejście w Jirze.
 */
async function moveCard(
  card: JiraBoardIssue,
  column: number,
  transitionId: string | null = null,
  from = card.column,
) {
  if (!writable.value || !board.value) return;
  if (card.column === column && transitionId === null) return;
  if (pending.value.includes(card.key)) return;
  const id = board.value.boardId;
  card.column = column;
  openMenu.value = null;
  choice.value = null;
  notice.value = "";
  boardError.value = "";
  pending.value = [...pending.value, card.key];
  const columnName = board.value.columns[column]?.name ?? "";
  try {
    const out = await jiraBoardMove(id, card.key, column, transitionId);
    if (out.result === "moved") {
      Object.assign(card, {
        status: out.status,
        statusId: out.statusId,
        category: out.category,
        column: out.column,
      });
      notice.value = `${card.key} → ${out.status}.`;
      emit("updated", await read());
    } else if (out.result === "choose") {
      choice.value = { card, column, from, options: out.options };
    } else {
      card.column = from;
      boardError.value = out.message;
      await jiraOpen(out.url);
    }
  } catch (e) {
    card.column = from;
    boardError.value = `Nie przesunięto ${card.key} do „${columnName}”. ${String(
      e instanceof Error ? e.message : e,
    )}`;
  } finally {
    pending.value = pending.value.filter((k) => k !== card.key);
  }
}
function cancelChoice() {
  if (choice.value) choice.value.card.column = choice.value.from;
  choice.value = null;
}
function assigned(card: JiraBoardIssue, person: JiraPerson) {
  card.assignee = { ...person };
}

// --- Nowe zgłoszenie ------------------------------------------------------
const creating = ref(false);
const projects = ref<JiraProject[]>([]);
const types = ref<JiraIssueType[]>([]);
const form = ref({
  project: "",
  issueTypeId: "",
  summary: "",
  description: "",
  assignMe: true,
  createTask: true,
  plannedDay: "",
});
const extraRequired = ref<string[]>([]);
const createUrl = ref("");
async function openCreate() {
  if (creating.value) {
    creating.value = false;
    return;
  }
  await run(async () => {
    notice.value = "";
    projects.value = await jiraProjects();
    const preferred = board.value?.project;
    form.value = {
      project:
        projects.value.find((p) => p.key === preferred)?.key ??
        projects.value[0]?.key ??
        "",
      issueTypeId: "",
      summary: "",
      description: "",
      assignMe: true,
      createTask: true,
      plannedDay: "",
    };
    creating.value = true;
    await loadTypes();
  });
}
async function loadTypes() {
  types.value = [];
  extraRequired.value = [];
  form.value.issueTypeId = "";
  if (!form.value.project) return;
  types.value = await jiraIssueTypes(form.value.project);
  form.value.issueTypeId = types.value[0]?.id ?? "";
  await loadFields();
}
async function loadFields() {
  extraRequired.value = [];
  if (!form.value.project || !form.value.issueTypeId) return;
  const fields = await jiraIssueTypeFields(
    form.value.project,
    form.value.issueTypeId,
  );
  extraRequired.value = fields.extraRequired;
  createUrl.value = fields.createUrl;
}
const typeName = computed(
  () => types.value.find((t) => t.id === form.value.issueTypeId)?.name ?? "",
);
async function submitCreate() {
  await run(async () => {
    const f = form.value;
    const created = await jiraCreate({
      ...f,
      plannedDay: f.createTask ? f.plannedDay : "",
    });
    creating.value = false;
    notice.value = `Utworzono ${created.key}${
      created.snapshot ? " i zadanie w Todo Track" : ""
    }.`;
    if (created.snapshot) emit("updated", created.snapshot);
    await refresh();
  });
}

onMounted(async () => {
  await run(async () => {
    settings.value = await jiraAccount();
    site.value = settings.value.config.site;
    email.value = settings.value.config.email;
    if (settings.value.connected) {
      if (settings.value.config.boards.length) await refresh();
      else {
        pickingBoards.value = true;
        await fetchBoards(false);
      }
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
    <form
      v-if="!settings.connected || editingConnection"
      class="jira-connection"
      @submit.prevent="connect"
    >
      <h2>
        {{ settings.connected ? "Połączenie z Jirą" : "Połącz swoje konto" }}
      </h2>
      <div class="connection-fields">
        <label
          >Adres Jiry<input
            v-model="site"
            type="url"
            required
            aria-label="Adres Jiry"
            :disabled="busy" /></label
        ><label
          >E-mail Atlassian<input
            v-model="email"
            type="email"
            required
            autocomplete="username"
            aria-label="E-mail Atlassian"
            :disabled="busy" /></label
        ><label
          >Token API<input
            v-model="apiToken"
            type="password"
            required
            autocomplete="off"
            aria-label="Token API"
            :disabled="busy"
        /></label>
      </div>
      <p class="hint">
        Użyj tokenu API bez zakresów. Token zostanie zapisany w magazynie
        poświadczeń Windows.
      </p>
      <div class="connection-actions">
        <button
          class="text-link"
          type="button"
          :disabled="busy"
          @click="
            open('https://id.atlassian.com/manage-profile/security/api-tokens')
          "
        >
          Utwórz token Atlassian <ExternalLink :size="13" /></button
        ><button
          v-if="settings.connected"
          type="button"
          class="danger"
          :disabled="busy"
          @click="disconnect"
        >
          Rozłącz konto</button
        ><button class="primary-button" :disabled="busy">
          {{ busy ? "Łączenie…" : "Połącz i sprawdź" }}
        </button>
      </div>
    </form>
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
          @click="openCreate"
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
      <section
        v-if="pickingBoards"
        class="board-picker"
        aria-label="Wybór tablic"
      >
        <form
          class="board-search"
          @submit.prevent="run(() => fetchBoards(false))"
        >
          <input
            v-model="boardName"
            aria-label="Szukaj tablicy"
            placeholder="Nazwa tablicy…"
            :disabled="busy"
          /><button class="secondary-button" :disabled="busy">
            Szukaj tablic
          </button>
        </form>
        <div class="chosen-boards">
          <button
            v-for="b in chosen"
            :key="b.id"
            :disabled="busy"
            :aria-label="'Odznacz tablicę: ' + b.name"
            @click="select(b)"
          >
            {{ b.name }} ×
          </button>
        </div>
        <div class="board-options">
          <label v-for="b in boards" :key="b.id"
            ><input
              type="checkbox"
              :checked="chosen.some((x) => x.id === b.id)"
              :disabled="
                busy ||
                (chosen.length >= 20 && !chosen.some((x) => x.id === b.id))
              "
              @change="select(b)"
            />{{ b.name }} <small>#{{ b.id }}</small></label
          >
          <p v-if="!boards.length && !busy" class="hint">
            Brak tablic pasujących do wyszukiwania.
          </p>
        </div>
        <div class="connection-actions">
          <button
            v-if="nextStart !== null"
            class="secondary-button"
            :disabled="busy"
            @click="run(() => fetchBoards(true))"
          >
            Pokaż więcej tablic</button
          ><span class="hint">Wybrano {{ chosen.length }} / 20</span
          ><button
            class="secondary-button"
            :disabled="busy"
            @click="pickingBoards = false"
          >
            Anuluj</button
          ><button
            class="primary-button"
            :disabled="busy"
            @click="saveBoards"
          >
            Zapisz tablice
          </button>
        </div>
      </section>
      <form
        v-if="creating"
        class="jira-create"
        aria-label="Nowe zgłoszenie"
        @submit.prevent="submitCreate"
        @keydown.esc.stop="creating = false"
      >
        <h2>Nowe zgłoszenie</h2>
        <div class="jira-create-fields">
          <label
            >Projekt<select
              v-model="form.project"
              aria-label="Projekt"
              :disabled="busy"
              @change="run(loadTypes)"
            >
              <option v-for="p in projects" :key="p.key" :value="p.key">
                {{ p.key }} ({{ p.boards.join(", ") }})
              </option>
            </select></label
          ><label
            >Typ<select
              v-model="form.issueTypeId"
              aria-label="Typ zgłoszenia"
              :disabled="busy"
              @change="run(loadFields)"
            >
              <option v-for="t in types" :key="t.id" :value="t.id">
                {{ t.name }}
              </option>
            </select></label
          ><label class="wide"
            >Tytuł<input
              v-model="form.summary"
              aria-label="Tytuł zgłoszenia"
              maxlength="255"
              required
              :disabled="busy"
          /></label>
          <label class="wide"
            >Opis (opcjonalnie)<textarea
              v-model="form.description"
              aria-label="Opis zgłoszenia"
              rows="3"
              maxlength="5000"
              :disabled="busy"
          /></label>
        </div>
        <div class="jira-create-options">
          <label class="check-label"
            ><input v-model="form.assignMe" type="checkbox" />Przypisz do
            mnie</label
          ><label class="check-label"
            ><input v-model="form.createTask" type="checkbox" />Utwórz też
            zadanie w Todo Track</label
          ><label v-if="form.createTask" class="check-label"
            >Dzień planu<input
              v-model="form.plannedDay"
              type="date"
              aria-label="Dzień planu zadania"
          /></label>
        </div>
        <p v-if="extraRequired.length" class="jira-move-error" role="alert">
          Typ „{{ typeName }}” wymaga pól, których Todo Track nie wypełnia:
          {{ extraRequired.join(", ") }}.
          <button type="button" class="text-link" @click="open(createUrl)">
            Utwórz w Jirze <ExternalLink :size="12" />
          </button>
        </p>
        <div class="connection-actions">
          <button
            type="button"
            class="secondary-button"
            @click="creating = false"
          >
            Anuluj</button
          ><button
            class="primary-button"
            :disabled="busy || !!extraRequired.length || !form.summary.trim()"
          >
            Utwórz zgłoszenie
          </button>
        </div>
      </form>
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
      <template v-if="mode === 'list'">
        <p v-if="settings.config.boards.length" class="jira-result-info">
          Przypisane do mnie · {{ visible.length }} zgłoszeń<span
            v-if="updatedAt"
          >
            · Odświeżono {{ updatedAt }}</span
          >
        </p>
        <p v-if="busy" class="hint" role="status">Proszę czekać…</p>
        <div class="jira-issues" :aria-busy="busy">
          <article
            v-for="issue in visible"
            :key="issue.key"
            class="jira-issue"
            :class="{ isRunning: props.activeUrl === issue.url }"
          >
            <div class="jira-issue-body">
              <div class="jira-issue-meta">
                <button
                  class="text-link"
                  :aria-label="'Otwórz w Jirze: ' + issue.key"
                  :disabled="busy"
                  @click="open(issue.url)"
                >
                  {{ issue.key }}<ExternalLink :size="12" /></button
                ><span class="jira-status">{{ issue.status }}</span>
              </div>
              <h2>{{ issue.summary }}</h2>
              <p>{{ issue.boards.join(" · ") }}</p>
            </div>
            <button
              v-if="props.activeUrl === issue.url"
              class="row-timer stop"
              :disabled="busy"
              :aria-label="'Zatrzymaj: ' + issue.key"
              @click="emit('stop')"
            >
              <Square :size="12" />Stop</button
            ><button
              v-else
              class="row-timer"
              :disabled="busy"
              :aria-label="'Rozpocznij: ' + issue.key"
              @click="start(issue.key)"
            >
              <Play :size="12" />Rozpocznij
            </button>
          </article>
        </div>
        <p
          v-if="loaded && !visible.length && !busy && !warnings.length"
          class="empty-state"
        >
          {{
            query
              ? "Brak zgłoszeń pasujących do wyszukiwania."
              : "Brak przypisanych Ci zgłoszeń na wybranych tablicach."
          }}
        </p>
      </template>
      <template v-else-if="board">
        <p class="jira-result-info">
          {{ scope === "mine" ? "Moje" : "Cała tablica" }} ·
          {{ board.issues.length }} zgłoszeń<span v-if="updatedAt">
            · Odświeżono {{ updatedAt }}</span
          >
        </p>
        <p v-if="board.truncated" class="hint jira-truncated" role="status">
          Tablica ma więcej niż {{ board.limit }} otwartych zgłoszeń — pokazano
          pierwsze {{ board.limit }}. Zawęź zakres do „Moje” albo szukaj po
          kluczu.
        </p>
        <p v-if="!writable" class="hint jira-read-only">
          Tablica tylko do odczytu: zmiany w Jirze są wyłączone (Ustawienia →
          Jira).
        </p>
        <p v-if="boardError" class="jira-move-error" role="alert">
          {{ boardError }}
        </p>
        <div
          v-if="choice"
          class="jira-move-list"
          role="group"
          :aria-label="'Wybierz przejście: ' + choice.card.key"
        >
          <span
            >{{ choice.card.key }} do „{{
              board.columns[choice.column]?.name
            }}”:</span
          >
          <button
            v-for="o in choice.options"
            :key="o.id"
            class="action-button"
            :aria-label="`Przesuń ${choice.card.key} do: ${o.toStatus}`"
            @click="
              moveCard(choice.card, choice.column, o.id, choice.from)
            "
          >
            {{ o.name && o.name !== o.toStatus ? `${o.name} → ${o.toStatus}` : o.toStatus }}
          </button>
          <button class="action-button" @click="cancelChoice">Anuluj</button>
        </div>
        <div class="jira-board" :aria-busy="busy || !!pending.length">
          <section
            v-for="(column, index) in board.columns"
            :key="index"
            class="jira-column"
            :class="{ dropping: dropTarget === index }"
            :aria-label="'Kolumna ' + column.name"
            @dragover="onDragOver($event, index)"
            @dragleave="dropTarget === index && (dropTarget = null)"
            @drop="onDrop($event, index)"
          >
            <header>
              <h2>{{ column.name }}</h2>
              <span
                class="jira-column-count"
                :aria-label="'Zgłoszeń w kolumnie: ' + columnCards[index].length"
                >{{ columnCards[index].length }}</span
              >
            </header>
            <article
              v-for="card in columnCards[index]"
              :key="card.key"
              class="jira-card"
              :class="{
                isRunning: props.activeUrl === card.url,
                isPending: pending.includes(card.key),
              }"
              :data-key="card.key"
              :draggable="writable && !pending.includes(card.key)"
              @dragstart="onDragStart($event, card)"
              @dragend="onDragEnd"
            >
              <div class="jira-card-meta">
                <button
                  class="text-link mono"
                  :aria-label="'Otwórz w Jirze: ' + card.key"
                  title="Otwórz w Jirze"
                  @click="open(card.url)"
                >
                  {{ card.key }}<ExternalLink :size="11" /></button
                ><span v-if="card.issueType" class="jira-tag">{{
                  card.issueType
                }}</span
                ><span
                  v-if="card.priority"
                  class="jira-tag jira-priority"
                  :class="card.priority.toLowerCase()"
                  :title="'Priorytet: ' + card.priority"
                  >{{ card.priority }}</span
                >
                <span
                  class="jira-avatar"
                  :title="card.assignee?.displayName ?? 'Nieprzypisane'"
                  :aria-label="
                    card.assignee
                      ? 'Przypisane: ' + card.assignee.displayName
                      : 'Nieprzypisane'
                  "
                  >{{ card.assignee ? initials(card.assignee.displayName) : "–" }}</span
                >
              </div>
              <h3>{{ card.summary }}</h3>
              <div class="jira-card-local">
                <span
                  v-if="props.activeUrl === card.url"
                  class="task-status task-live"
                  >mierzone teraz</span
                ><span
                  v-if="local.get(card.key)?.ms"
                  class="mono"
                  :title="'Czas zmierzony w Todo Track'"
                  >{{ duration(local.get(card.key)!.ms) }}</span
                ><span
                  v-if="local.get(card.key)?.planned"
                  class="task-chip task-planned"
                  >zaplanowane na {{ local.get(card.key)!.planned }}</span
                ><span
                  v-if="card.statusId === '' || !column.statusIds.includes(card.statusId)"
                  class="jira-status"
                  >{{ card.status }}</span
                >
              </div>
              <div class="jira-card-actions">
                <button
                  v-if="props.activeUrl === card.url"
                  class="row-timer stop"
                  :disabled="busy"
                  :aria-label="'Zatrzymaj: ' + card.key"
                  @click="emit('stop')"
                >
                  <Square :size="12" />Stop</button
                ><button
                  v-else
                  class="row-timer"
                  :disabled="busy"
                  :aria-label="'Rozpocznij: ' + card.key"
                  @click="start(card.key)"
                >
                  <Play :size="12" />Start</button
                ><button
                  class="icon-button"
                  :aria-label="'Akcje: ' + card.key"
                  :aria-expanded="openMenu === card.key"
                  title="Przesuń do…, przypisz, komentarz, gałąź"
                  @click="openMenu = openMenu === card.key ? null : card.key"
                >
                  <Ellipsis :size="16" />
                </button>
              </div>
              <div
                v-if="openMenu === card.key"
                class="jira-card-menu"
                @keydown.esc.stop="openMenu = null"
              >
                <div
                  v-if="writable"
                  class="jira-card-move"
                  role="group"
                  :aria-label="'Przesuń do: ' + card.key"
                >
                  <span class="hint">Przesuń do…</span>
                  <button
                    v-for="(target, ti) in board.columns"
                    v-show="ti !== card.column"
                    :key="ti"
                    class="action-button"
                    :disabled="pending.includes(card.key)"
                    :aria-label="`Przesuń ${card.key} do kolumny: ${target.name}`"
                    @click="moveCard(card, ti)"
                  >
                    {{ target.name }}
                  </button>
                </div>
                <JiraIssueActions
                  :issue-key="card.key"
                  :summary="card.summary"
                  @assigned="assigned(card, $event)"
                />
              </div>
            </article>
            <p v-if="!columnCards[index].length" class="hint jira-column-empty">
              Brak zgłoszeń
            </p>
          </section>
        </div>
      </template>
      <p v-else-if="mode === 'board' && busy" class="hint" role="status">
        Proszę czekać…
      </p>
    </template>
  </section>
</template>
