<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
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
import { readVisibleMovePrompts } from "./jiramove";
import {
  desktop,
  dismissWidget,
  duration,
  jiraTransition,
  moveTargetName,
  onMovePromptsChanged,
  type JiraMovePrompt,
  openMainWindow,
  read,
  resumeCandidate,
  toggleMeasurement,
  type Snapshot,
} from "./store";

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
  `${n} ${n % 10 >= 2 && n % 10 <= 4 && !(n % 100 >= 12 && n % 100 <= 14) ? "zadania" : "zadań"}`;
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
/** Pytanie o przesunięcie zgłoszenia w Jirze przy starcie pracy (J1). */
const movePrompts = ref<JiraMovePrompt[]>([]);
const moveLine = computed(() =>
  movePrompts.value.length
    ? `Jira: przesunąć ${movePrompts.value[0].key} do „${moveTargetName(movePrompts.value[0])}”?`
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
    data.value = await jiraTransition(p.taskId, p.transitionId);
    showFeedback(`Jira: ${p.key} → ${moveTargetName(p)}`);
    await loadMovePrompts();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
/** Tryb skupienia: odliczanie bloku zamiast zegara pomiaru, przerwa i pytania. */
const focus = useFocus();
const hhmm = (ms: number) =>
  new Date(ms).toLocaleTimeString("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  });
const focusLine = computed(() => {
  const f = focus.value;
  if (f.block) return `Skupienie do ${hhmm(f.block.end)}`;
  if (f.ended) return "Koniec bloku: przerwa czy kolejny?";
  if (f.pause) return `Przerwa do ${hhmm(f.pause.until)}`;
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
const subline = computed(
  () =>
    error.value ||
    feedback.value ||
    focusLine.value ||
    proposalLine.value ||
    moveLine.value ||
    (activeTask.value
      ? activeTask.value.details?.project || "Pomiar trwa"
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
      unsubscribers.push(await listen("data-changed", refresh));
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
    :class="{ running: !!active }"
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
          : 'Pokaż pytanie o przesunięcie w Jirze'
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
