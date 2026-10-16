<script setup lang="ts">
import { onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  desktop,
  dismissMovePrompt,
  jiraTransition,
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
  readVisibleMovePrompts,
} from "../jiramove";
import { onFocusChanged } from "../focus";

/**
 * Pytanie o przesunięcie zgłoszenia w Jirze (J1): przy starcie pracy nad
 * zgłoszeniem „Do zrobienia” i po ukończeniu zadania. Pomiar trwa niezależnie
 * od odpowiedzi; zgłoszenie zmienia się tylko po kliknięciu.
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
const move = (taskId: number, url: string, transitionId: string) =>
  run(taskId, url, async () => {
    emit("updated", await jiraTransition(taskId, transitionId));
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
async function openIssue(url: string) {
  try {
    if (desktop) await invoke("jira_open", { url });
  } catch (e) {
    emit("error", String(e));
  }
}
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
    aria-label="Przesunięcie zgłoszenia w Jirze"
  >
    <h2>Przesunąć {{ p.key }} do „{{ moveTargetName(p) }}”?</h2>
    <p>
      „{{ p.title }}” ma w Jirze status {{ p.status }}. Pomiar już trwa;
      zgłoszenie zmieni się tylko po kliknięciu.
      <template v-if="!p.transitionId">Wybierz przejście:</template>
    </p>
    <div class="planning-actions">
      <button
        v-if="p.transitionId"
        class="today-button"
        :disabled="busy"
        :aria-label="`Przesuń ${p.key} do ${moveTargetName(p)}`"
        @click="move(p.taskId, p.url, p.transitionId)"
      >
        Przesuń</button
      ><button
        v-for="t in p.transitionId ? [] : p.transitions"
        :key="t.id"
        class="today-button"
        :disabled="busy"
        :aria-label="`Przesuń do: ${t.toStatus}`"
        @click="move(p.taskId, p.url, t.id)"
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
      <button v-if="p.url" class="text-link" @click="openIssue(p.url)">
        Otwórz {{ p.key }} w Jirze
      </button>
    </p>
  </section>
  <section
    v-for="p in donePrompts"
    :key="'done-' + p.taskId"
    class="idle-banner jira-move-banner"
    aria-label="Przesunięcie zgłoszenia po ukończeniu"
  >
    <h2>Ukończone. Przesunąć {{ p.key }} dalej?</h2>
    <p>Wybierz przejście dla „{{ p.title }}” albo zostaw zgłoszenie bez zmian.</p>
    <div class="planning-actions">
      <button
        v-for="t in p.transitions"
        :key="t.id"
        class="today-button"
        :disabled="busy"
        :aria-label="`Przesuń do: ${t.toStatus}`"
        @click="move(p.taskId, p.url, t.id)"
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
        @click="openIssue(failure.url)"
      >
        Otwórz {{ p.key }} w Jirze
      </button>
    </p>
  </section>
</template>
