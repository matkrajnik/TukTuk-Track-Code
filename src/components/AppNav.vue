<script lang="ts">
export type View =
  | "tasks"
  | "reports"
  | "jira"
  | "dashboard"
  | "journal"
  | "library"
  | "settings";
</script>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  ArrowDownToLine,
  ArrowLeftRight,
  CalendarDays,
  CalendarRange,
  Check,
  History,
  LayoutDashboard,
  ListTodo,
  Moon,
  PictureInPicture2,
  Settings,
  Sun,
  Ticket,
} from "lucide-vue-next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  desktop,
  dismissWidget,
  onWidgetStatus,
  readWidgetStatus,
  type WidgetStatus,
} from "../store";
import { onSettingsChanged, writeSettings } from "../settings";

const view = defineModel<View>("view", { required: true });
const emit = defineEmits<{
  quick: [];
  error: [message: string];
}>();

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
const unsubscribers: (() => void)[] = [];
let unmounted = false;
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
        <LayoutDashboard :size="16" />Dashboard
      </button>
      <button
        :class="{ selected: view === 'journal' }"
        :aria-pressed="view === 'journal'"
        @click="view = 'journal'"
      >
        <CalendarRange :size="16" />Plan i historia
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
        <Ticket :size="16" />Jira
      </button>
      <button
        :class="{ selected: view === 'library' }"
        :aria-pressed="view === 'library'"
        @click="view = 'library'"
      >
        <History :size="16" />Historia i kopie
      </button>
      <button
        :class="{ selected: view === 'settings' }"
        :aria-pressed="view === 'settings'"
        title="Ustawienia (Ctrl+,)"
        @click="view = 'settings'"
      >
        <Settings :size="16" />Ustawienia
      </button>
    </nav>
    <div class="header-actions">
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
