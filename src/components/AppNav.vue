<script lang="ts">
export type View =
  | "tasks"
  | "reports"
  | "jira"
  | "redmine"
  | "dashboard"
  | "agents"
  | "journal"
  | "library"
  | "settings";
</script>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  ArrowDownToLine,
  ArrowLeftRight,
  CalendarDays,
  CalendarRange,
  ClipboardList,
  History,
  LayoutDashboard,
  ListTodo,
  Bot,
  Menu,
  Moon,
  PictureInPicture2,
  Settings,
  Square,
  SquareTerminal,
  Sun,
  Sunrise,
  Sunset,
  Ticket,
} from "lucide-vue-next";
import BrandIcon from "./BrandIcon.vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  desktop,
  dismissWidget,
  onWidgetStatus,
  readWidgetStatus,
  type WidgetStatus,
} from "../store";
import { onSettingsChanged, writeSettings } from "../settings";
import {
  boardCounts,
  onAgentBoardChanged,
  readAgentBoard,
  type BoardCounts,
} from "../agentboard";

const view = defineModel<View>("view", { required: true });
/** Trwający pomiar: pokazywany w pasku na każdym widoku poza Zadaniami. */
/**
 * B13: `dayEnded` = „Dzień zakończony 16:40 · 7:55” po „Zakończ dzień” (pasek
 * pokazuje wtedy „Wznów dzień” na każdym widoku); przy pomiarze przycisk dnia
 * stoi obok Stop.
 */
defineProps<{
  running?: { title: string; elapsed: string } | null;
  /** Czat (A3): pozycja „Asystent” pod Ustawieniami, tylko gdy czat jest dostępny. */
  chatEnabled?: boolean;
  chatOpen?: boolean;
  dayEnded?: string | null;
}>();
const emit = defineEmits<{
  quick: [];
  chat: [];
  stop: [];
  endDay: [];
  resumeDay: [];
  error: [message: string];
}>();

// Tryb panelu nawigacji (Windows NavigationView): progi 641 i 1008 px.
const PANE_KEY = "tuktuk-track-nav-collapsed";
const narrowQuery = window.matchMedia("(max-width: 640px)");
const midQuery = window.matchMedia("(max-width: 1007px)");
const narrow = ref(narrowQuery.matches);
const mid = ref(midQuery.matches);
/** Ręczny wybór (zwinięty / rozwinięty); null = według szerokości okna. */
const collapsed = ref<boolean | null>(null);
try {
  const saved = localStorage.getItem(PANE_KEY);
  if (saved === "1" || saved === "0") collapsed.value = saved === "1";
} catch {}
const paneCompact = computed(
  () => !narrow.value && (collapsed.value ?? mid.value),
);
/** Panel nad treścią w trybie LeftMinimal (poniżej 641 px). */
const paneOpen = ref(false);
const toggleLabel = computed(() =>
  narrow.value
    ? paneOpen.value
      ? "Zamknij nawigację"
      : "Pokaż nawigację"
    : paneCompact.value
      ? "Rozwiń nawigację"
      : "Zwiń nawigację",
);
async function togglePane() {
  if (narrow.value) {
    paneOpen.value = !paneOpen.value;
    await nextTick();
    if (paneOpen.value)
      document.querySelector<HTMLElement>(".nav-pane .nav-item")?.focus();
    return;
  }
  collapsed.value = !paneCompact.value;
  try {
    localStorage.setItem(PANE_KEY, collapsed.value ? "1" : "0");
  } catch {}
}
function go(target: View) {
  view.value = target;
  if (paneOpen.value) {
    paneOpen.value = false;
    document.querySelector<HTMLElement>(".topbar-menu")?.focus();
  }
}
const onNarrow = (e: MediaQueryListEvent) => {
  narrow.value = e.matches;
  if (!e.matches) paneOpen.value = false;
};
const onMid = (e: MediaQueryListEvent) => (mid.value = e.matches);
narrowQuery.addEventListener("change", onNarrow);
midQuery.addEventListener("change", onMid);
onUnmounted(() => {
  narrowQuery.removeEventListener("change", onNarrow);
  midQuery.removeEventListener("change", onMid);
});

const theme = ref<"dark" | "light">("dark");
try {
  if (localStorage.getItem("tuktuk-track-theme") === "light")
    theme.value = "light";
} catch {}
watch(
  theme,
  (value) => {
    document.documentElement.dataset.theme = value;
    try {
      localStorage.setItem("tuktuk-track-theme", value);
    } catch {}
  },
  { immediate: true },
);

// Przełącznik mini-widgetu: włącza ustawienie, gdy widget jest wyłączony, i
// przywraca widget schowany przyciskiem X. Wyłączenie chowa go do końca sesji,
// bez zmiany ustawienia (na stałe wyłącza się go w Ustawieniach).
const widget = ref<WidgetStatus>({
  enabled: false,
  backgroundOnly: false,
  dismissed: false,
});
const widgetOn = computed(() => widget.value.enabled && !widget.value.dismissed);
const widgetTitle = computed(() =>
  widgetOn.value
    ? "Schowaj mini-widget do końca sesji"
    : "Pokaż mini-widget zawsze na wierzchu",
);
const widgetBusy = ref(false);
async function toggleWidget() {
  if (widgetBusy.value) return;
  widgetBusy.value = true;
  try {
    if (widgetOn.value) widget.value = await dismissWidget(true);
    else {
      if (!widget.value.enabled) await writeSettings({ widgetEnabled: true });
      widget.value = await dismissWidget(false);
    }
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    widgetBusy.value = false;
  }
}
// Licznik tablicy sesji agentów: nieprzejrzane czekające i zakończone.
const agentCounts = ref<BoardCounts>({ waiting: 0, done: 0 });
const agentsPending = computed(
  () => agentCounts.value.waiting + agentCounts.value.done,
);
const agentsLabel = computed(() => {
  const { waiting, done } = agentCounts.value;
  const parts = [
    waiting ? `czeka ${waiting}` : "",
    done ? `skończone ${done}` : "",
  ].filter(Boolean);
  return parts.length ? `Sesje agentów: ${parts.join(", ")}` : "Sesje agentów";
});
const unsubscribers: (() => void)[] = [];
let unmounted = false;
onMounted(async () => {
  try {
    // Zdarzenie, które przyszło w trakcie odczytu, jest nowsze od jego wyniku.
    let changed = false;
    const off = await onAgentBoardChanged((list) => {
      changed = true;
      agentCounts.value = boardCounts(list);
    });
    if (unmounted) off();
    else unsubscribers.push(off);
    const list = await readAgentBoard();
    if (!changed) agentCounts.value = boardCounts(list);
  } catch (e) {
    // Licznik jest pomocniczy: bez niego pozycja nawigacji działa dalej.
    console.warn("Nie udało się odczytać tablicy sesji agentów:", e);
  }
});
onMounted(async () => {
  try {
    const subscriptions = [
      await onWidgetStatus((status) => (widget.value = status)),
      await onSettingsChanged((s) => {
        // Jak `Status::after_settings_change` w Rust: ponowne włączenie
        // w ustawieniach (false -> true) kasuje schowanie X.
        widget.value = {
          ...widget.value,
          dismissed: widget.value.dismissed && widget.value.enabled,
          enabled: s.widgetEnabled,
          backgroundOnly: s.widgetBackgroundOnly,
        };
      }),
    ];
    if (unmounted) subscriptions.forEach((off) => off());
    else unsubscribers.push(...subscriptions);
    widget.value = await readWidgetStatus();
  } catch (e) {
    // Stan przełącznika jest pomocniczy: bez niego przycisk zostaje wyłączony,
    // a błąd nie zasłania komunikatów widoku. Błąd kliknięcia jest pokazywany.
    console.warn("Nie udało się odczytać stanu mini-widgetu:", e);
  }
});
onUnmounted(() => {
  unmounted = true;
  unsubscribers.forEach((off) => off());
});

// Zamknięcie zamiast hide(): przechodzi przez CloseRequested w Ruście, który
// chowa okno do traya i zapisuje jego pozycję i rozmiar na dysk.
async function hide() {
  try {
    await getCurrentWindow().close();
  } catch (e) {
    emit("error", String(e));
  }
}
</script>

<template>
  <!-- Lewy panel według Windows NavigationView: rozwinięty od 1008 px, same
       ikony (LeftCompact) przy 641–1007 px, schowany pod przyciskiem menu
       (LeftMinimal) poniżej. Przycisk menu zwija i rozwija panel ręcznie;
       wybór jest zapamiętany. Ustawienia na dole panelu jak w Windows. -->
  <div
    v-if="paneOpen"
    class="nav-scrim"
    aria-hidden="true"
    @click="paneOpen = false"
  />
  <aside
    class="nav-pane"
    :class="{ compact: paneCompact, open: paneOpen }"
    @keydown.esc="paneOpen && go(view)"
  >
    <div class="nav-pane-inner">
    <div class="nav-pane-head">
      <button
        class="icon-button nav-toggle"
        :aria-label="toggleLabel"
        :aria-expanded="narrow ? paneOpen : !paneCompact"
        @click="togglePane"
      >
        <Menu :size="20" />
      </button>
      <a class="brand" href="#" @click.prevent="go('tasks')"
        ><span class="brand-icon"><BrandIcon :size="20" /></span
        ><span class="brand-text">tuktuk<span>track</span></span></a
      >
    </div>
    <nav aria-label="Nawigacja główna">
      <button
        class="nav-item"
        :class="{ selected: view === 'tasks' }"
        :aria-current="view === 'tasks' ? 'page' : undefined"
        :title="paneCompact ? 'Zadania' : undefined"
        @click="go('tasks')"
      >
        <ListTodo :size="20" aria-hidden="true" /><span class="nav-label">Zadania</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'dashboard' }"
        :aria-current="view === 'dashboard' ? 'page' : undefined"
        :title="paneCompact ? 'Dashboard' : undefined"
        @click="go('dashboard')"
      >
        <LayoutDashboard :size="20" aria-hidden="true" /><span class="nav-label">Dashboard</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'agents' }"
        :aria-current="view === 'agents' ? 'page' : undefined"
        :aria-label="agentsLabel"
        :title="paneCompact ? agentsLabel : undefined"
        @click="go('agents')"
      >
        <SquareTerminal :size="20" aria-hidden="true" /><span class="nav-label"
          >Sesje agentów</span
        ><span
          v-if="agentsPending > 0"
          class="nav-badge"
          :class="{ 'nav-badge-waiting': agentCounts.waiting > 0 }"
          aria-hidden="true"
          >{{ agentsPending }}</span
        >
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'journal' }"
        :aria-current="view === 'journal' ? 'page' : undefined"
        :title="paneCompact ? 'Plan i historia' : undefined"
        @click="go('journal')"
      >
        <CalendarRange :size="20" aria-hidden="true" /><span class="nav-label">Plan i historia</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'reports' }"
        :aria-current="view === 'reports' ? 'page' : undefined"
        :title="paneCompact ? 'Raporty dzienne' : undefined"
        @click="go('reports')"
      >
        <CalendarDays :size="20" aria-hidden="true" /><span class="nav-label">Raporty dzienne</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'jira' }"
        :aria-current="view === 'jira' ? 'page' : undefined"
        :title="paneCompact ? 'Jira' : undefined"
        @click="go('jira')"
      >
        <Ticket :size="20" aria-hidden="true" /><span class="nav-label">Jira</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'redmine' }"
        :aria-current="view === 'redmine' ? 'page' : undefined"
        :title="paneCompact ? 'Redmine' : undefined"
        @click="go('redmine')"
      >
        <ClipboardList :size="20" aria-hidden="true" /><span class="nav-label">Redmine</span>
      </button>
      <button
        class="nav-item"
        :class="{ selected: view === 'library' }"
        :aria-current="view === 'library' ? 'page' : undefined"
        :title="paneCompact ? 'Historia i kopie' : undefined"
        @click="go('library')"
      >
        <History :size="20" aria-hidden="true" /><span class="nav-label">Historia i kopie</span>
      </button>
      <button
        class="nav-item nav-footer"
        :class="{ selected: view === 'settings' }"
        :aria-current="view === 'settings' ? 'page' : undefined"
        title="Ustawienia (Ctrl+,)"
        @click="go('settings')"
      >
        <Settings :size="20" aria-hidden="true" /><span class="nav-label">Ustawienia</span>
      </button>
      <button
        v-if="chatEnabled"
        class="nav-item nav-assistant"
        :class="{ selected: chatOpen }"
        :aria-pressed="chatOpen"
        :title="chatOpen ? 'Schowaj asystenta (Ctrl+J)' : 'Asystent AI (Ctrl+J)'"
        @click="emit('chat')"
      >
        <Bot :size="20" aria-hidden="true" /><span class="nav-label">Asystent</span>
      </button>
    </nav>
    </div>
  </aside>
  <header class="topbar">
    <button
      class="icon-button nav-toggle topbar-menu"
      aria-label="Pokaż nawigację"
      :aria-expanded="paneOpen"
      @click="togglePane"
    >
      <Menu :size="20" />
    </button>
    <a class="brand topbar-brand" href="#" @click.prevent="go('tasks')"
      ><span class="brand-icon"><BrandIcon :size="20" /></span
    ></a>
    <div class="header-actions">
      <!-- Wzorzec paska timera z Toggl Track i Clockify: mierzone zadanie i Stop
           są widoczne na każdym widoku. Nazwa i czas idą przez atrybuty, żeby nie
           dublować tekstu widoku; dostępna nazwa jest w aria-label. -->
      <div
        v-if="running && view !== 'tasks'"
        class="topbar-timer"
        role="group"
        aria-label="Bieżący pomiar w pasku"
      >
        <button
          class="topbar-timer-task"
          :data-title="running.title"
          :data-elapsed="running.elapsed"
          :aria-label="`Mierzone teraz: ${running.title}. Pokaż w Zadaniach`"
          :title="running.title"
          @click="view = 'tasks'"
        >
          <span class="status-dot" aria-hidden="true" />
        </button>
        <button
          class="topbar-timer-stop"
          aria-label="Zatrzymaj mierzone zadanie"
          title="Stop"
          @click="emit('stop')"
        >
          <Square :size="14" /></button
        ><button
          v-if="dayEnded"
          class="topbar-timer-stop topbar-timer-day"
          aria-label="Wznów dzień"
          title="Wznów dzień"
          @click="emit('resumeDay')"
        >
          <Sunrise :size="14" /></button
        ><button
          v-else
          class="topbar-timer-stop topbar-timer-day"
          aria-label="Zakończ dzień"
          title="Zakończ dzień"
          @click="emit('endDay')"
        >
          <Sunset :size="14" />
        </button>
      </div>
      <div
        v-else-if="dayEnded && !running"
        class="topbar-timer topbar-day-ended"
        role="group"
        aria-label="Dzień zakończony w pasku"
      >
        <span class="topbar-day-text" role="status">{{ dayEnded }}</span
        ><button
          class="topbar-timer-stop topbar-timer-day"
          aria-label="Wznów dzień"
          title="Wznów dzień"
          @click="emit('resumeDay')"
        >
          <Sunrise :size="14" />
        </button>
      </div>
      <span
        v-if="!desktop"
        class="preview-label"
        title="Dane zapisywane tylko w tej przeglądarce; funkcje Windows są wyłączone"
        >Podgląd</span
      >
      <button
        class="quick-button"
        aria-label="Szybkie przełączanie"
        title="Szybkie przełączanie zadania (Ctrl+K, globalnie Ctrl+Shift+Space)"
        @click="emit('quick')"
      >
        <ArrowLeftRight :size="15" /><kbd>Ctrl K</kbd>
      </button>
      <button
        class="icon-button widget-switch"
        :class="{ on: widgetOn }"
        aria-label="Mini-widget"
        :aria-pressed="widgetOn"
        :title="widgetTitle"
        :disabled="widgetBusy"
        @click="toggleWidget"
      >
        <PictureInPicture2 :size="18" />
      </button>
      <button
        class="icon-button"
        :aria-label="theme === 'dark' ? 'Włącz tryb jasny' : 'Włącz tryb ciemny'"
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
</template>
