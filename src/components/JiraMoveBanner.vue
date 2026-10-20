<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { Ticket } from "lucide-vue-next";
import {
  dismissMovePrompt,
  moveTargetName,
  onMovePromptsChanged,
  setJiraNoAsk,
  type JiraMovePrompt,
  type JiraTransition,
  type Snapshot,
} from "../store";
import {
  donePrompts,
  dropDonePrompt,
  inSystem,
  openIssueUrl,
  performMove,
  promptSystem,
  readVisibleMovePrompts,
} from "../jiramove";
import type { IssueSystem } from "../issue";
import { onFocusChanged } from "../focus";

/**
 * Pytanie o przesunięcie zgłoszenia w Jirze (J1) albo w Redmine (PLAN_1.9.md,
 * R4): przy starcie pracy nad zgłoszeniem „Do zrobienia” (w Redmine: w statusie
 * początkowym) i po ukończeniu zadania. Pomiar trwa niezależnie od odpowiedzi;
 * zgłoszenie zmienia się tylko po kliknięciu.
 */
const props = defineProps<{
  /** Aktywny pomiar (przedział): zmiana sprawdza pytanie od razu. */
  activeSessionId: number | null;
}>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const prompts = ref<JiraMovePrompt[]>([]);
const busy = ref(false);
/** Błąd przejścia przy pytaniu: komunikat z linkiem do zgłoszenia. */
const failure = ref<{ taskId: number; text: string; url: string } | null>(
  null,
);

async function reload() {
  try {
    prompts.value = await readVisibleMovePrompts();
  } catch {
    prompts.value = [];
  }
}
async function run(taskId: number, url: string, action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  failure.value = null;
  try {
    await action();
    await reload();
  } catch (e) {
    failure.value = {
      taskId,
      url,
      text: String(e instanceof Error ? e.message : e),
    };
  } finally {
    busy.value = false;
  }
}
const move = (
  system: IssueSystem,
  taskId: number,
  url: string,
  transitionId: string,
) =>
  run(taskId, url, async () => {
    emit("updated", await performMove(system, taskId, transitionId));
    dropDonePrompt(taskId);
  });
const notNow = (p: JiraMovePrompt) =>
  run(p.taskId, p.url, async () => {
    prompts.value = await dismissMovePrompt(p.taskId);
  });
const never = (p: JiraMovePrompt) =>
  run(p.taskId, p.url, async () => {
    emit("updated", await setJiraNoAsk(p.taskId, true));
  });
function closeDone(taskId: number) {
  dropDonePrompt(taskId);
  if (failure.value?.taskId === taskId) failure.value = null;
}
async function openIssue(system: IssueSystem, url: string) {
  try {
    await openIssueUrl(system, url);
  } catch (e) {
    emit("error", String(e));
  }
}
/** Etykieta banera: osobna dla Jiry (bez zmian) i Redmine. */
const regionLabel = (system: IssueSystem) =>
  system === "redmine"
    ? "Przesunięcie zgłoszenia w Redmine"
    : "Przesunięcie zgłoszenia w Jirze";
const arrow = (t: JiraTransition) =>
  t.name && t.name !== t.toStatus ? `${t.name} → ${t.toStatus}` : t.toStatus;

watch(() => props.activeSessionId, reload);
let stops: (() => void)[] = [];
let alive = true;
onMounted(async () => {
  try {
    const list = [
      await onMovePromptsChanged(() => void reload()),
      // Po bloku skupienia pytanie wraca.
      await onFocusChanged(() => void reload()),
    ];
    if (!alive) return list.forEach((stop) => stop());
    stops = list;
  } catch {}
  await reload();
});
onUnmounted(() => {
  alive = false;
  stops.forEach((stop) => stop());
});
</script>

<template>
  <section
    v-for="p in prompts"
    :key="'start-' + p.taskId"
    class="idle-banner jira-move-banner"
    :class="{ 'redmine-move-banner': promptSystem(p) === 'redmine' }"
    :aria-label="regionLabel(promptSystem(p))"
  >
    <Ticket class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Przesunąć {{ p.key }} do „{{ moveTargetName(p) }}”?</h2>
    <p>
      „{{ p.title }}” ma {{ inSystem(promptSystem(p)) }} status
      {{ p.status }}. Pomiar już trwa; zgłoszenie zmieni się tylko po
      kliknięciu.
      <template v-if="!p.transitionId">{{
        promptSystem(p) === "redmine" ? "Wybierz status:" : "Wybierz przejście:"
      }}</template>
    </p>
    <div class="planning-actions">
      <button
        v-if="p.transitionId"
        class="primary-button"
        :disabled="busy"
        :aria-label="`Przesuń ${p.key} do ${moveTargetName(p)}`"
        @click="move(promptSystem(p), p.taskId, p.url, p.transitionId)"
      >
        Przesuń</button
      ><button
        v-for="t in p.transitionId ? [] : p.transitions"
        :key="t.id"
        class="secondary-button"
        :disabled="busy"
        :aria-label="`Przesuń do: ${t.toStatus}`"
        @click="move(promptSystem(p), p.taskId, p.url, t.id)"
      >
        {{ arrow(t) }}</button
      ><button class="today-button" :disabled="busy" @click="notNow(p)">
        Nie teraz</button
      ><button class="today-button" :disabled="busy" @click="never(p)">
        Nie pytaj dla tego zadania
      </button>
    </div>
    <p
      v-if="failure?.taskId === p.taskId || p.failure"
      class="jira-move-error"
      role="alert"
    >
      {{ failure?.taskId === p.taskId ? failure.text : p.failure }}
      <button
        v-if="p.url"
        class="text-link"
        @click="openIssue(promptSystem(p), p.url)"
      >
        Otwórz {{ p.key }} {{ inSystem(promptSystem(p)) }}
      </button>
    </p>
  </section>
  <section
    v-for="p in donePrompts"
    :key="'done-' + p.taskId"
    class="idle-banner jira-move-banner"
    aria-label="Przesunięcie zgłoszenia po ukończeniu"
  >
    <Ticket class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Ukończone. Przesunąć {{ p.key }} dalej?</h2>
    <p>
      Wybierz {{ p.system === "redmine" ? "status" : "przejście" }} dla „{{
        p.title
      }}” albo zostaw zgłoszenie bez zmian.
    </p>
    <div class="planning-actions">
      <button
        v-for="t in p.transitions"
        :key="t.id"
        class="secondary-button"
        :disabled="busy"
        :aria-label="`Przesuń do: ${t.toStatus}`"
        @click="move(p.system, p.taskId, p.url, t.id)"
      >
        {{ arrow(t) }}</button
      ><button class="today-button" :disabled="busy" @click="closeDone(p.taskId)">
        Nie teraz
      </button>
    </div>
    <p v-if="failure?.taskId === p.taskId" class="jira-move-error" role="alert">
      {{ failure.text }}
      <button
        v-if="failure.url"
        class="text-link"
        @click="openIssue(p.system, failure.url)"
      >
        Otwórz {{ p.key }} {{ inSystem(p.system) }}
      </button>
    </p>
  </section>
</template>
