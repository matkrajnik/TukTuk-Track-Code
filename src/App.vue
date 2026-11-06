<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { X } from "lucide-vue-next";
import TitleBar from "./components/TitleBar.vue";
import AppNav, { type View } from "./components/AppNav.vue";
import ConfirmDialog from "./components/ConfirmDialog.vue";
import SessionEditor, { type SessionTab } from "./components/SessionEditor.vue";
import IdleBanner from "./components/IdleBanner.vue";
import GapBanner from "./components/GapBanner.vue";
import AgentSuggestions from "./components/AgentSuggestions.vue";
import AiProposals from "./components/AiProposals.vue";
import JiraMoveBanner from "./components/JiraMoveBanner.vue";
import FocusBanner from "./components/FocusBanner.vue";
import UpdateBanner from "./components/UpdateBanner.vue";
import DayDialogs from "./components/DayDialogs.vue";
import AiFirstRun from "./components/AiFirstRun.vue";
import AiChat from "./components/AiChat.vue";
import { chat, openChat, setChatOpen, startChat, useChatEnabled } from "./aichat";
import {
  confirmStart,
  dayMarkOf,
  dayStateOf,
  requestDayEnd,
  resumeDay,
} from "./days";
import { startAutoChecks } from "./updater";
import { refreshFocus } from "./focus";
import TasksView from "./views/TasksView.vue";
import ReportsView from "./views/ReportsView.vue";
import JiraView from "./JiraView.vue";
import RedmineView from "./RedmineView.vue";
import QuickSwitch from "./QuickSwitch.vue";
import LibraryView from "./LibraryView.vue";
import DashboardView from "./DashboardView.vue";
import AgentsView from "./views/AgentsView.vue";
import JournalView from "./JournalView.vue";
import SettingsView from "./views/SettingsView.vue";
import WorkflowsView from "./views/WorkflowsView.vue";
import WorkflowRunsView from "./views/WorkflowRunsView.vue";
import {
  loadWorkflowAccess,
  openRunRequest,
  workflowsUnlocked,
} from "./workflowAccess";
import {
  isSettingsSection,
  readSettingsSection,
  sectionInMessage,
  setNavigator,
  type SettingsSection,
} from "./navigation";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onSettingsChanged, parseShortcut, readSettings } from "./settings";
import {
  markMorningShown,
  morningShown,
  morningStartDue,
} from "./morningStart";
import {
  calendarEvents,
  toggleMeasurement,
  change,
  previewDataEvent,
  read,
  desktop,
  onDataChanged,
  onNavigate,
  aiStatus,
  type AiDetected,
  type NavigateRequest,
  type Snapshot,
  type Task,
  type Session,
  duration,
} from "./store";
import { clock, dayBounds, dayKey, hm, inDay } from "./time";
import { wideLayout } from "./layout";

const data = ref<Snapshot>({ tasks: [], sessions: [] });
const now = ref(Date.now());
const busy = ref(false);
const loading = ref(true);
const error = ref("");
/** Dialog pierwszego uruchomienia asystenta AI (A1): wykryte CLI, gdy trzeba zapytać. */
const aiAsk = ref<AiDetected | null>(null);
/** Pyta o agenta tylko, gdy jeszcze nie wybrano i wykryto Claude Code albo Codex. */
async function askAboutAi() {
  try {
    // Wybór już zapisany (także „Nie używaj”): bez wykrywania i bez uruchamiania CLI.
    if ((await readSettings()).aiEngine) return;
    const status = await aiStatus();
    if (status.ask) aiAsk.value = status.detected;
  } catch {
    // Bez wykrywania aplikacja działa bez dialogu (Ustawienia → Asystent AI).
  }
}
/** Czat (A3): panel po prawej, tylko przy wybranym silniku i włączonym `aiChat`. */
const chatEnabled = useChatEnabled();
const chatVisible = computed(() => chatEnabled.value && chat.open);
/** Ctrl+J: pokazuje albo chowa panel czatu (fokus w polu wiadomości po otwarciu). */
function toggleChat() {
  if (!chatEnabled.value) return;
  if (chat.open) setChatOpen(false);
  else openChat();
}
const view = ref<View>("tasks");
/** Przebieg otwierany w monitorze (z powiadomienia albo po starcie). */
const workflowRunId = ref<number | null>(null);
function openRun(id: number) {
  workflowRunId.value = id;
  view.value = "workflow-runs";
}
watch(openRunRequest, (id) => {
  if (id === null) return;
  openRunRequest.value = null;
  openRun(id);
});
/** Zakładka „Historia i kopie”. */
const libraryTab = ref("history");
/** Sekcja zakładki Ustawienia; zapamiętana między uruchomieniami. */
const settingsSection = ref<SettingsSection>(readSettingsSection());
/** Komunikat błędu wskazujący „Ustawienia → X” dostaje przycisk do tej sekcji. */
const errorSection = computed(() => sectionInMessage(error.value));
function openErrorSection() {
  if (!errorSection.value) return;
  void navigate({ view: "settings", section: errorSection.value });
  error.value = "";
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
const activeSessionId = computed(
  () => data.value.sessions.find((s) => s.endedAt === null)?.id ?? null,
);
/** Mierzone zadanie w pasku nawigacji (widoczne na każdym widoku poza Zadaniami). */
const running = computed(() => {
  const active = data.value.sessions.find((s) => s.endedAt === null);
  if (!active || !activeTask.value) return null;
  return {
    title: activeTask.value.title,
    elapsed: duration(now.value - active.startedAt),
  };
});
const todayKey = computed(() => dayKey(now.value));
/**
 * Pasek po „Zakończ dzień” (B13): „Dzień zakończony 16:40 · 7:55” z „Wznów
 * dzień”; po północy (nowy dzień bez wpisu) znika.
 */
const todayMark = computed(() => dayMarkOf(data.value, todayKey.value));
const closedToday = computed(() =>
  data.value.sessions.reduce(
    (sum, s) => (s.endedAt === null ? sum : sum + inDay(s, todayKey.value, 0)),
    0,
  ),
);
const dayEnded = computed(() => {
  const ended = todayMark.value?.endedAt;
  if (ended == null) return null;
  const active = data.value.sessions.find((s) => s.endedAt === null);
  const total =
    closedToday.value + (active ? inDay(active, todayKey.value, now.value) : 0);
  return `Dzień zakończony ${clock(ended)} · ${hm(total)}`;
});
async function endDay() {
  error.value = "";
  try {
    await requestDayEnd();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  }
}
async function resumeToday() {
  error.value = "";
  try {
    updated(await resumeDay());
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  }
}
// Stop albo zmiana zadania kończy blok skupienia: stan sprawdzamy po każdej zmianie danych.
watch(data, () => void refreshFocus());
watch(todayKey, (day, previous) => {
  if (selectedDay.value === previous) selectedDay.value = day;
});

/**
 * Przełączenie widoku zaczyna nowy widok od góry strony (B2). Widoki
 * zachowane w `KeepAlive` (Zadania, Raporty) wracają w miejsce, w którym je
 * zostawiono. Pozycję zapisujemy przed zmianą DOM, przewijamy po niej.
 */
const keptViews: View[] = ["tasks", "reports"];
const scrollPositions = new Map<View, number>();
watch(
  view,
  (_next, previous) => {
    if (keptViews.includes(previous))
      scrollPositions.set(previous, window.scrollY);
  },
  { flush: "pre" },
);
watch(
  view,
  (next) => {
    const top = keptViews.includes(next) ? (scrollPositions.get(next) ?? 0) : 0;
    window.scrollTo({ top, left: 0, behavior: "instant" });
  },
  { flush: "post" },
);

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
/**
 * Przejścia użytkownika (zakładka, link w widoku, powiadomienie, tray, skrót N).
 * Automatyczne Rano nie przełącza widoku, jeśli w trakcie jego sprawdzania
 * nastąpiło inne przejście. Żądanie spoza okna (powiadomienie, „Nowe zadanie”
 * z traya) przychodzi razem z pokazaniem okna, więc od razu oznacza Rano tego
 * dnia jako pokazane: pokazanie okna z celem nie kończy się na Rano.
 */
let navigations = 0;
let morningNavigating = false;
function userNavigated(external: boolean) {
  navigations++;
  if (external) markMorningShown(dayKey(Date.now()));
}
watch(
  view,
  () => {
    if (!morningNavigating) navigations++;
  },
  { flush: "sync" },
);
/** Żądania z powiadomień Windows (`navigate`) dla Zadań i Zamknięcia dnia. */
const tasksRequest = ref<NavigateRequest | null>(null);
const journalDayClose = ref(false);
/** Żądanie otwarcia Plan i historia → Rano (powiadomienie „Przegląd dnia”, Dashboard). */
const journalMorning = ref(false);
/** Żądanie otwarcia Plan i historia → Tydzień → Przegląd (powiadomienie „Przegląd tygodnia”). */
const journalWeekReview = ref(false);
const views: View[] = [
  "tasks",
  "reports",
  "jira",
  "redmine",
  "dashboard",
  "journal",
  "library",
  "settings",
  "agents",
  "workflows",
  "workflow-definitions",
  "workflow-runs",
];
/**
 * Kliknięcie treści powiadomienia lub „Podziel…”: Rust już pokazał okno, tu
 * ustawiamy widok, filtr, rozwinięcie i edytor na zakładce Podziel.
 * Niczego nie zapisuje i nie przełącza timera.
 */
async function navigate(
  request: NavigateRequest,
  source: "app" | "external" | "morning" = "app",
) {
  // Prośba o czat (Ctrl+K po „?”, z innego okna): otwiera panel bez zmiany widoku.
  if (request.chat && chatEnabled.value) {
    userNavigated(source === "external");
    openChat(request.chat);
  }
  const found = views.find((v) => v === request.view);
  if (!found) return;
  // Workflows przed odblokowaniem prowadzą do Ustawień → Workflows (klucz).
  const workflowsView =
    found === "workflows" ||
    found === "workflow-definitions" ||
    found === "workflow-runs";
  if (workflowsView) await loadWorkflowAccess();
  const locked = workflowsView && !workflowsUnlocked.value;
  const target: View = locked ? "settings" : found;
  if (locked) request = { ...request, section: "workflows" };
  if (found === "workflow-runs") workflowRunId.value = request.run ?? null;
  if (source !== "morning") userNavigated(source === "external");
  // Otwartego okna dialogowego nie zamykamy: niezapisane zmiany zostają, a
  // widok pod nim przełącza się normalnie. Edytor Podziel otwiera się tylko
  // wtedy, gdy żadne okno nie jest otwarte.
  // Żądanie Zadań ustawia nowe filtry, więc pozycja przewinięcia starej listy
  // nie ma sensu: nowa lista zaczyna się od góry (zadanie lub sekcję przewija
  // potem sam widok Zadań).
  if (target === "tasks") {
    scrollPositions.delete("tasks");
    if (view.value === "tasks")
      window.scrollTo({ top: 0, left: 0, behavior: "instant" });
  }
  view.value = target;
  if (target === "settings" && isSettingsSection(request.section))
    settingsSection.value = request.section;
  if (target === "tasks") tasksRequest.value = { ...request };
  if (target === "journal" && request.dayClose) journalDayClose.value = true;
  else if (target === "journal" && request.section === "morning")
    journalMorning.value = true;
  else if (target === "journal" && request.section === "week-review")
    journalWeekReview.value = true;
  if (request.split != null && !modalOpen.value) {
    await refresh();
    const session = data.value.sessions.find((s) => s.id === request.split);
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
/** Link „Przejdź do treści” (WCAG 2.4.1): fokus na początek widoku, bez zmiany adresu. */
function skipToMain() {
  document.getElementById("main")?.focus();
}
/** Fokus w polu „Nowe zadanie” (skrót N i zdarzenie `prompt-task` z traya). */
async function focusNewTask(external = false) {
  userNavigated(external);
  view.value = "tasks";
  await nextTick();
  document
    .querySelector<HTMLInputElement>('input[aria-label="Nowe zadanie"]')
    ?.focus();
}
/** Start/Stop jak skrót globalny (`toggle_measurement`); bez zadania → szybkie przełączanie. */
async function toggleShortcut() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    // Start po zakończeniu dnia pyta „Wznowić dzień?” (B13).
    if (
      !data.value.sessions.some((s) => s.endedAt === null) &&
      !(await confirmStart(data.value))
    )
      return;
    const result = await toggleMeasurement();
    if (result.action === "quick") void openQuick();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
  await refresh();
}
/** Pole tekstowe, lista albo edytowalny tekst: tam litera N to zwykłe pisanie. */
function typing(target: EventTarget | null): boolean {
  const el = target instanceof HTMLElement ? target : null;
  if (!el) return false;
  return (
    el.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(el.tagName) ||
    el.closest('[role="combobox"], [role="listbox"], [role="dialog"]') !== null
  );
}
/** Okno dialogowe na stronie (edycja, potwierdzenie, szybkie przełączanie). */
const dialogOpen = () =>
  modalOpen.value ||
  quickPreview.value ||
  document.querySelector('[role="dialog"], [aria-modal="true"]') !== null;
/**
 * Skróty globalne z Ustawień (postać kanoniczna, tylko aplikacja). Lokalny
 * Ctrl+Shift+S ustępuje globalnemu skrótowi o tej samej kombinacji, żeby jedno
 * naciśnięcie nie zatrzymało i od razu nie wznowiło pomiaru.
 */
const globalShortcuts = ref<string[]>([]);
function rememberShortcuts(s: {
  shortcutQuick: string;
  shortcutToggle: string;
}) {
  globalShortcuts.value = [s.shortcutQuick, s.shortcutToggle]
    .map(parseShortcut)
    .filter((v): v is string => v !== null);
}
const localToggle = parseShortcut("Control+Shift+S");
function shortcut(e: KeyboardEvent) {
  const key = e.key.toLowerCase();
  if (e.ctrlKey && key === "k") {
    e.preventDefault();
    void openQuick();
  } else if (e.ctrlKey && !e.altKey && !e.metaKey && e.key === ",") {
    e.preventDefault();
    void navigate({ view: "settings" });
  } else if (
    e.ctrlKey &&
    !e.altKey &&
    !e.metaKey &&
    !e.shiftKey &&
    key === "j" &&
    chatEnabled.value
  ) {
    e.preventDefault();
    if (!e.repeat) toggleChat();
  } else if (e.defaultPrevented) {
    return;
  } else if (
    e.ctrlKey &&
    e.shiftKey &&
    !e.altKey &&
    !e.metaKey &&
    e.code === "KeyS"
  ) {
    // Ta sama kombinacja jako skrót globalny: obsługuje ją Rust.
    if (desktop && globalShortcuts.value.includes(localToggle!)) return;
    e.preventDefault();
    if (!e.repeat && !dialogOpen()) void toggleShortcut();
  } else if (
    key === "n" &&
    !e.ctrlKey &&
    !e.altKey &&
    !e.metaKey &&
    !e.shiftKey &&
    !e.repeat &&
    !typing(e.target) &&
    !dialogOpen()
  ) {
    e.preventDefault();
    void focusNewTask();
  }
}

/** Okno widoczne dla użytkownika (w aplikacji: nie schowane do traya). */
async function windowVisible(): Promise<boolean> {
  if (document.visibilityState === "hidden") return false;
  if (!desktop) return true;
  try {
    return await getCurrentWindow().isVisible();
  } catch {
    return false;
  }
}
/**
 * Plan dnia rano (B11): pierwsze uruchomienie albo pokazanie okna w dniu
 * roboczym bez zapisanego przeglądu otwiera Plan i historia → Rano, raz
 * dziennie. Wołane przy starcie, przy pokazaniu okna i przy zmianie dnia;
 * okno schowane do traya czeka na pierwsze pokazanie. Niczego nie zapisuje
 * w bazie i nie zmienia timera.
 */
let morningPending = false;
async function openMorningIfDue() {
  const day = dayKey(Date.now());
  if (morningPending || morningShown(day)) return;
  morningPending = true;
  const seen = navigations;
  try {
    if (!(await windowVisible())) return;
    const settings = await readSettings();
    const [from, to] = dayBounds(day);
    const events =
      settings.openMorningOnStart && settings.morningReviewDay !== day
        ? await calendarEvents(from, to).catch(() => [])
        : [];
    if (!morningStartDue(settings, day, events)) return;
    // Zakończony dziś dzień (B13): Rano już się nie otwiera.
    if (dayStateOf(dayMarkOf(data.value, day)) === "ended") return;
    // Dzień mógł się zmienić w trakcie odczytu, a okno pokazać Rano gdzie indziej.
    if (dayKey(Date.now()) !== day || morningShown(day)) return;
    markMorningShown(day);
    // W trakcie sprawdzania użytkownik przeszedł gdzie indziej: zostaje tam.
    if (navigations !== seen) return;
    morningNavigating = true;
    try {
      await navigate({ view: "journal", section: "morning" }, "morning");
    } finally {
      morningNavigating = false;
    }
  } catch {
    /* bez ustawień zostaje bieżący widok */
  } finally {
    morningPending = false;
  }
}
// Zmiana dnia przy widocznym oknie nie przełącza widoku: Rano czeka na kolejne
// pokazanie okna albo powrót fokusu (onFocusChanged, visibilitychange, focus).
function onVisibility() {
  if (document.visibilityState === "visible") void openMorningIfDue();
}
function onWindowFocus() {
  void openMorningIfDue();
}

const unsubscribers: UnlistenFn[] = [];
let tick: ReturnType<typeof setInterval>;
onMounted(async () => {
  window.addEventListener("keydown", shortcut);
  setNavigator((request) => void navigate(request));
  try {
    unsubscribers.push(await startChat());
  } catch {
    // Bez nasłuchu czat nie dostanie odpowiedzi, reszta aplikacji działa.
  }
  tick = setInterval(() => {
    now.value = Date.now();
  }, 1000);
  try {
    unsubscribers.push(
      await onNavigate((request) => void navigate(request, "external")),
    );
  } catch (e) {
    error.value = String(e);
  }
  if (desktop) {
    try {
      unsubscribers.push(
        await listen("prompt-task", async () => {
          closeModal();
          await focusNewTask(true);
        }),
      );
      unsubscribers.push(await onDataChanged(refresh));
      unsubscribers.push(await onSettingsChanged(rememberShortcuts));
      void readSettings()
        .then(rememberShortcuts)
        .catch(() => {});
      // Pokazanie okna z traya (show + set_focus w Rust) albo powrót do okna.
      unsubscribers.push(
        await getCurrentWindow().onFocusChanged(({ payload }) => {
          if (payload) void openMorningIfDue();
        }),
      );
      unsubscribers.push(
        await listen<string>("storage-error", (e) => {
          error.value = e.payload;
        }),
      );
    } catch (e) {
      error.value = String(e);
    }
    // Sprawdzanie aktualizacji (updater.ts): brak sieci bez komunikatu.
    try {
      unsubscribers.push(await startAutoChecks());
    } catch {
      /* aplikacja działa bez sprawdzania aktualizacji */
    }
  } else {
    // Podgląd: symulowana Jira zmienia dane poza widokiem (priorytet, pola).
    window.addEventListener(previewDataEvent, refresh);
    // Podgląd: powrót do karty przeglądarki jak odzyskanie fokusu okna.
    window.addEventListener("focus", onWindowFocus);
    unsubscribers.push(() =>
      window.removeEventListener("focus", onWindowFocus),
    );
    unsubscribers.push(() =>
      window.removeEventListener(previewDataEvent, refresh),
    );
  }
  await refresh();
  document.addEventListener("visibilitychange", onVisibility);
  void openMorningIfDue();
  void askAboutAi();
});
onUnmounted(() => {
  clearInterval(tick);
  window.removeEventListener("keydown", shortcut);
  document.removeEventListener("visibilitychange", onVisibility);
  setNavigator(null);
  unsubscribers.forEach((fn) => fn());
});
</script>

<template>
  <div class="app-shell">
    <a class="skip-link" href="#main" @click.prevent="skipToMain"
      >Przejdź do treści</a
    >
    <TitleBar v-if="desktop" @error="error = $event" />
    <div
      class="app-body"
      :class="{ 'chat-open': chatVisible }"
      :style="chatVisible ? { '--chat-width': `${chat.width}px` } : undefined"
    >
    <AppNav
      v-model:view="view"
      :running="running"
      :day-ended="dayEnded"
      :chat-enabled="chatEnabled"
      :chat-open="chatVisible"
      @chat="toggleChat"
      @quick="openQuick"
      @stop="act('stop')"
      @end-day="endDay"
      @resume-day="resumeToday"
      @error="error = $event"
    />
    <main id="main" tabindex="-1" :class="{ wide: wideLayout }">
      <GapBanner
        :tasks="data.tasks"
        @updated="updated"
        @error="error = $event"
      />
      <IdleBanner
        :tasks="data.tasks"
        @updated="updated"
        @error="error = $event"
      />
      <FocusBanner
        :now="now"
        @updated="updated"
        @error="error = $event"
      />
      <AgentSuggestions
        :tasks="data.tasks"
        :active-task-id="activeTask?.id ?? null"
        @updated="updated"
        @error="error = $event"
      />
      <AiProposals
        :tasks="data.tasks"
        @updated="updated"
        @error="error = $event"
      />
      <JiraMoveBanner
        :active-session-id="activeSessionId"
        @updated="updated"
        @error="error = $event"
      />
      <UpdateBanner />
      <div v-if="error && !modalOpen" class="error" role="alert">
        {{ error
        }}<button
          v-if="errorSection"
          class="text-link error-link"
          @click="openErrorSection"
        >
          Otwórz ustawienia</button
        ><button aria-label="Zamknij komunikat" @click="error = ''">
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
          :morning="journalMorning"
          :week-review="journalWeekReview"
          @day-close-shown="journalDayClose = false"
          @morning-shown="journalMorning = false"
          @week-review-shown="journalWeekReview = false"
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
      <SettingsView
        v-if="view === 'settings'"
        v-model:section="settingsSection"
      />
      <JiraView
        v-else-if="view === 'jira'"
        :active-url="activeTask?.jiraUrl"
        :data="data"
        :now="now"
        @updated="updated"
        @stop="act('stop')"
      />
      <RedmineView
        v-else-if="view === 'redmine'"
        :data="data"
        :now="now"
        @updated="updated"
        @stop="act('stop')"
      />
      <AgentsView v-else-if="view === 'agents'" :now="now" />
      <WorkflowsView
        v-else-if="
          (view === 'workflows' || view === 'workflow-definitions') &&
          workflowsUnlocked
        "
        :section="view === 'workflows' ? 'overview' : 'definitions'"
        @open="navigate({ view: $event })"
        @run="openRun"
      />
      <WorkflowRunsView
        v-else-if="view === 'workflow-runs' && workflowsUnlocked"
        :run-id="workflowRunId"
      />
    </main>
    <AiChat
      v-if="chatVisible"
      :tasks="data.tasks"
      @updated="updated"
      @error="error = $event"
    />
    </div>
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
    <DayDialogs main :data="data" @updated="updated" @error="error = $event" />
    <AiFirstRun
      v-if="aiAsk"
      :detected="aiAsk"
      @close="aiAsk = null"
      @chosen="aiAsk = null"
    />
  </div>
</template>
