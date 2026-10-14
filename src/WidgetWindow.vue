<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { GripVertical, Play, Square, X } from "lucide-vue-next";
import QuickSwitch from "./QuickSwitch.vue";
import {
  desktop,
  dismissWidget,
  duration,
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
const subline = computed(
  () =>
    error.value ||
    feedback.value ||
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
  await refresh();
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
      <small :class="{ 'widget-error': !!error }" role="status">{{
        subline
      }}</small>
    </button>
    <span
      v-if="active"
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
