<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { LogicalSize, getCurrentWindow } from "@tauri-apps/api/window";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  ArrowRightCircle,
  Bot,
  Coffee,
  GripVertical,
  Play,
  SkipForward,
  Square,
  Timer,
  X,
} from "lucide-vue-next";
import {
  countdown,
  focusAction,
  refreshFocus,
  useFocus,
  type FocusAction,
} from "./focus";
import {
  onSuggestionsChanged,
  readSuggestions,
  signalOnly,
  sourceLabel,
  switchToSuggestion,
  type Suggestion,
} from "./agents";
import QuickSwitch from "./QuickSwitch.vue";
import {
  inSystem,
  performMove,
  promptSystem,
  readVisibleMovePrompts,
} from "./jiramove";
import { issueLabel } from "./issue";
import {
  desktop,
  dismissWidget,
  change,
  duration,
  moveTargetName,
  onDataChanged,
  onMovePromptsChanged,
  type JiraMovePrompt,
  openMainWindow,
  pinnedTasks,
  read,
  resumeCandidate,
  toggleMeasurement,
  type Snapshot,
  type Task,
} from "./store";
import { issueRef } from "./issue";
import { clock, dayBounds, dayKey, hm, inDay, plural } from "./time";
import { dayGoal } from "./capacity";
import { dayTarget, useSettings } from "./target";
import { useMeetings } from "./composables/useMeetings";

const data = ref<Snapshot>({ tasks: [], sessions: [] });
const now = ref(Date.now());
const busy = ref(false);
const error = ref("");
const feedback = ref("");
/** Podgląd: X chowa widget na stronie (w aplikacji chowa okno). */
const hidden = ref(false);
const quickPreview = ref(false);

const active = computed(() =>
  data.value.sessions.find((s) => s.endedAt === null),
);
const activeTask = computed(() =>
  data.value.tasks.find((t) => t.id === active.value?.taskId),
);
const resume = computed(() =>
  active.value ? undefined : resumeCandidate(data.value),
);
/** Przypięte zadania („w toku”): przełączanie jednym kliknięciem. */
const pins = computed(() => pinnedTasks(data.value));
/** Klucz zgłoszenia (Jira albo `#1234` z Redmine) zamiast tytułu, jak w trayu. */
const pinLabel = (t: Task) => issueRef(t)?.key || t.title;
async function switchToPin(id: number, title: string) {
  if (busy.value || id === active.value?.taskId) return;
  busy.value = true;
  error.value = "";
  try {
    data.value = await change("start", id);
    showFeedback(`Start: ${title}`);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
/** Okno 320×64, z rzędem przypiętych 320×96 (jak `.widget-card.has-pins`). */
watch(
  () => pins.value.length > 0,
  (hasPins) => {
    if (!desktop) return;
    getCurrentWindow()
      .setSize(new LogicalSize(320, hasPins ? 96 : 64))
      .catch((e) => (error.value = String(e)));
  },
  { immediate: true },
);
/** Propozycje przełączenia z pracy agentów AI (bez bieżącego zadania). */
const suggestions = ref<Suggestion[]>([]);
const proposals = computed(() =>
  suggestions.value.filter(
    (s) => s.task.taskId == null || s.task.taskId !== active.value?.taskId,
  ),
);
const proposalName = (s: Suggestion) => s.task.title || s.task.key || "";
/** „2 zadania”, „5 zadań” (jak `notify::tasks_word`). */
const tasksWord = (n: number) =>
  `${n} ${plural(n, "zadanie", "zadania", "zadań")}`;
/** Linia propozycji: jedno zadanie z nazwy, kilka jako liczba. */
const proposalLine = computed(() =>
  proposals.value.length === 1
    ? `${signalOnly(proposals.value[0]) ? sourceLabel(proposals.value[0].agents[0].kind) : "Agent"}: ${proposalName(proposals.value[0])}`
    : proposals.value.length
      ? `${proposals.value.every(signalOnly) ? "Propozycje" : "Agenci"}: ${tasksWord(proposals.value.length)} do przełączenia`
      : "",
);
async function loadSuggestions() {
  try {
    suggestions.value = await readSuggestions();
  } catch {
    suggestions.value = [];
  }
}
/** Jedno zadanie lokalne: „Przełącz” od razu; inaczej główne okno z banerem. */
async function useProposal() {
  const only = proposals.value.length === 1 ? proposals.value[0] : null;
  if (!only || only.task.taskId == null) return showMain();
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    data.value = await switchToSuggestion(only.task);
    showFeedback(`Start: ${proposalName(only)}`);
    await loadSuggestions();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
/** Pytanie o przesunięcie zgłoszenia w Jirze albo Redmine przy starcie pracy (J1, R4). */
const movePrompts = ref<JiraMovePrompt[]>([]);
const moveLine = computed(() =>
  movePrompts.value.length
    ? `${issueLabel(promptSystem(movePrompts.value[0]))}: przesunąć ${movePrompts.value[0].key} do „${moveTargetName(movePrompts.value[0])}”?`
    : "",
);
async function loadMovePrompts() {
  try {
    movePrompts.value = await readVisibleMovePrompts();
  } catch {
    movePrompts.value = [];
  }
}
/** „Przesuń” z widgetu przy wybranym przejściu; inaczej główne okno z banerem. */
async function useMovePrompt() {
  const p = movePrompts.value[0];
  if (!p?.transitionId) return showMain();
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    data.value = await performMove(promptSystem(p), p.taskId, p.transitionId);
    showFeedback(
      `${issueLabel(promptSystem(p))}: ${p.key} → ${moveTargetName(p)}`,
    );
    await loadMovePrompts();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    // Pytanie zostaje z błędem: baner w głównym oknie ma link do zgłoszenia.
    await showMain();
  } finally {
    busy.value = false;
  }
}
/** Tryb skupienia: odliczanie bloku zamiast zegara pomiaru, przerwa i pytania. */
const focus = useFocus();
const focusLine = computed(() => {
  const f = focus.value;
  if (f.block) return `Skupienie do ${clock(f.block.end)}`;
  if (f.ended) return "Koniec bloku: przerwa czy kolejny?";
  if (f.pause) return `Przerwa do ${clock(f.pause.until)}`;
  if (f.resume) return `Wracasz do: ${f.resume.title}?`;
  return "";
});
/** Skupienie startuje na mierzonym zadaniu, a bez pomiaru na ostatnim. */
const focusTarget = computed(() => activeTask.value ?? resume.value);
async function runFocus(action: FocusAction, taskId: number | null = null) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await focusAction(action, taskId);
    await refresh();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
/**
 * Suma dnia jak w trayu (B11): „5:10 / 8:00” (w wąskim widgecie bez słowa
 * „dziś”, które jest w podpowiedzi), cel pomniejszony o spotkania
 * (`dayGoal`, jak `capacity::goal_ms_in`), bez celu w dzień wolny. Zamknięte
 * przedziały liczone przy zmianie danych, co sekundę tylko trwający pomiar (B6).
 */
const settings = useSettings();
const today = computed(() => dayKey(now.value));
const meetings = useMeetings(() => dayBounds(today.value));
const closedToday = computed(() =>
  data.value.sessions.reduce(
    (sum, s) => (s.endedAt === null ? sum : sum + inDay(s, today.value, 0)),
    0,
  ),
);
const minuteNow = computed(() => Math.floor(now.value / 60_000) * 60_000);
const todayGoal = computed(() =>
  dayGoal(
    dayTarget(settings.value, today.value),
    meetings.events.value,
    data.value.sessions,
    today.value,
    minuteNow.value,
  ),
);
const dayLine = computed(() => {
  const total =
    closedToday.value +
    (active.value ? inDay(active.value, today.value, now.value) : 0);
  return `${hm(total)}${todayGoal.value > 0 ? ` / ${hm(todayGoal.value)}` : ""}`;
});
/** Podpowiedź podlinii z sumą dnia: „Dziś: 5:10 z 8:00”. */
const dayTitle = computed(() =>
  activeTask.value && subline.value.startsWith(dayLine.value)
    ? `Dziś: ${dayLine.value.replace(" / ", " z ")}`
    : undefined,
);
const subline = computed(
  () =>
    error.value ||
    feedback.value ||
    focusLine.value ||
    proposalLine.value ||
    moveLine.value ||
    (activeTask.value
      ? [dayLine.value, activeTask.value.details?.project]
          .filter(Boolean)
          .join(" · ")
      : resume.value
        ? `Ostatnio: ${resume.value.title}`
        : "Wybierz zadanie przyciskiem ⇄"),
);

function updateTheme() {
  try {
    document.documentElement.dataset.theme =
      localStorage.getItem("todo-track-theme") === "light" ? "light" : "dark";
  } catch {}
}
updateTheme();

async function refresh() {
  try {
    data.value = await read();
    now.value = Date.now();
    error.value = "";
    void refreshFocus();
    void loadMovePrompts();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  }
}

let feedbackTimer: ReturnType<typeof setTimeout> | undefined;
function showFeedback(text: string) {
  feedback.value = text;
  clearTimeout(feedbackTimer);
  feedbackTimer = setTimeout(() => (feedback.value = ""), 4000);
}

async function openQuick() {
  error.value = "";
  if (!desktop) {
    quickPreview.value = true;
    return;
  }
  try {
    await invoke("quick_open");
  } catch (e) {
    error.value = String(e);
  }
}

async function toggle() {
  // Po przerwie „Start” odpowiada na „Wracasz do X?”.
  if (!active.value && focus.value.resume)
    return runFocus("resume", focus.value.resume.taskId);
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const result = await toggleMeasurement();
    if (result.action === "quick") await openQuick();
    else if (result.action === "stopped")
      showFeedback(`Stop: ${result.title} · ${duration(result.elapsedMs)}`);
    else showFeedback(`Start: ${result.title}`);
    await refresh();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}

async function showMain() {
  try {
    await openMainWindow();
  } catch (e) {
    error.value = String(e);
  }
}

async function dismiss() {
  try {
    await dismissWidget(true);
    if (!desktop) hidden.value = true;
  } catch (e) {
    error.value = String(e);
  }
}
async function restore() {
  await dismissWidget(false);
  hidden.value = false;
}

const unsubscribers: UnlistenFn[] = [];
let tick: ReturnType<typeof setInterval>;
onMounted(async () => {
  document.documentElement.classList.add("widget-root");
  window.addEventListener("storage", updateTheme);
  tick = setInterval(() => (now.value = Date.now()), 1000);
  if (desktop) {
    try {
      unsubscribers.push(await onDataChanged(refresh));
      unsubscribers.push(
        await listen<string>("toggle-feedback", (e) => showFeedback(e.payload)),
      );
      unsubscribers.push(
        await listen("widget-shown", () => {
          updateTheme();
          void refresh();
        }),
      );
    } catch (e) {
      error.value = String(e);
    }
  }
  try {
    unsubscribers.push(
      await onSuggestionsChanged(() => void loadSuggestions()),
    );
    unsubscribers.push(
      await onMovePromptsChanged(() => void loadMovePrompts()),
    );
  } catch {}
  await refresh();
  await loadSuggestions();
});
onUnmounted(() => {
  clearInterval(tick);
  clearTimeout(feedbackTimer);
  window.removeEventListener("storage", updateTheme);
  unsubscribers.forEach((fn) => fn());
});
</script>

<template>
  <section
    v-if="!hidden"
    id="widget"
    class="widget-card"
    :class="{ running: !!active, 'has-pins': pins.length > 0 }"
    aria-label="Mini-widget"
    data-tauri-drag-region
  >
    <span class="widget-grip" title="Przeciągnij" data-tauri-drag-region
      ><GripVertical :size="16"
    /></span>
    <button
      class="widget-task"
      aria-label="Pokaż główne okno"
      :title="activeTask?.title ?? 'Pokaż główne okno'"
      @click="showMain"
    >
      <strong>{{ activeTask?.title ?? "Brak pomiaru" }}</strong>
      <small
        :class="{
          'widget-error': !!error,
          'widget-suggest':
            !error && !feedback && !!(proposalLine || moveLine),
        }"
        :title="dayTitle"
        role="status"
        >{{ subline }}</small
      >
    </button>
    <button
      v-if="proposals.length"
      class="icon-button widget-suggest"
      :aria-label="
        proposals.length === 1 && proposals[0].task.taskId != null
          ? `Przełącz na: ${proposalName(proposals[0])}`
          : 'Pokaż propozycje agentów'
      "
      :title="proposalLine"
      :disabled="busy"
      @click="useProposal"
    >
      <Bot :size="16" />
    </button>
    <button
      v-else-if="movePrompts.length"
      class="icon-button widget-suggest"
      :aria-label="
        movePrompts[0].transitionId
          ? `Przesuń ${movePrompts[0].key} do ${moveTargetName(movePrompts[0])}`
          : `Pokaż pytanie o przesunięcie ${inSystem(promptSystem(movePrompts[0]))}`
      "
      :title="moveLine"
      :disabled="busy"
      @click="useMovePrompt"
    >
      <ArrowRightCircle :size="16" />
    </button>
    <template v-if="focus.ended">
      <button
        class="icon-button widget-focus"
        aria-label="Przerwa"
        title="Przerwa"
        :disabled="busy"
        @click="runFocus('break')"
      >
        <Coffee :size="16" /></button
      ><button
        class="icon-button widget-focus"
        aria-label="Kolejny blok"
        title="Kolejny blok skupienia"
        :disabled="busy"
        @click="runFocus('next')"
      >
        <SkipForward :size="16" />
      </button>
    </template>
    <button
      v-else-if="!focus.block && !focus.pause && focusTarget"
      class="icon-button widget-focus"
      aria-label="Rozpocznij blok skupienia"
      :title="`Skupienie: ${focusTarget.title}`"
      :disabled="busy"
      @click="runFocus('start', focusTarget.id)"
    >
      <Timer :size="16" />
    </button>
    <span
      v-if="focus.block"
      class="widget-clock widget-focus mono"
      aria-label="Pozostało w bloku skupienia"
      :title="active ? `Pomiar: ${duration(now - active.startedAt)}` : ''"
      >{{ countdown(focus.block.end - now) }}</span
    >
    <span
      v-else-if="focus.pause"
      class="widget-clock widget-focus mono"
      aria-label="Pozostało z przerwy"
      >{{ countdown(focus.pause.until - now) }}</span
    >
    <span
      v-else-if="active"
      class="widget-clock mono"
      aria-label="Czas bieżącego pomiaru"
      >{{ duration(now - active.startedAt) }}</span
    >
    <button
      v-if="active"
      class="icon-button widget-toggle"
      aria-label="Zatrzymaj pomiar"
      title="Stop"
      :disabled="busy"
      @click="toggle"
    >
      <Square :size="16" />
    </button>
    <button
      v-else
      class="icon-button widget-toggle"
      aria-label="Wznów ostatnie zadanie"
      :title="resume ? `Start: ${resume.title}` : 'Start'"
      :disabled="busy"
      @click="toggle"
    >
      <Play :size="16" />
    </button>
    <button
      class="icon-button"
      aria-label="Szybkie przełączanie"
      title="Szybkie przełączanie"
      @click="openQuick"
    >
      ⇄
    </button>
    <button
      class="icon-button"
      aria-label="Schowaj widget"
      title="Schowaj do końca sesji"
      @click="dismiss"
    >
      <X :size="15" />
    </button>
    <div
      v-if="pins.length"
      class="widget-pins"
      role="group"
      aria-label="Przypięte zadania"
    >
      <button
        v-for="t in pins"
        :key="t.id"
        class="widget-pin"
        :class="{ current: t.id === active?.taskId }"
        :aria-pressed="t.id === active?.taskId"
        :aria-label="
          t.id === active?.taskId
            ? `Mierzone: ${t.title}`
            : `Przełącz na: ${t.title}`
        "
        :title="t.title"
        :disabled="busy"
        @click="switchToPin(t.id, t.title)"
      >
        {{ pinLabel(t) }}
      </button>
    </div>
  </section>
  <p v-else class="widget-hidden-note">
    Mini-widget schowany do końca sesji.
    <button class="text-link" @click="restore">Pokaż widget</button>
  </p>
  <div v-if="quickPreview" class="modal-overlay">
    <QuickSwitch
      @close="quickPreview = false"
      @updated="
        data = $event;
        quickPreview = false;
      "
    />
  </div>
</template>
