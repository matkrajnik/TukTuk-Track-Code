<script setup lang="ts">
/**
 * Kafelek zadania na liście: wiersz z akcjami i rozwinięte szczegóły. Części
 * w `components/task/`: tytuł i chipy (`TaskSummary`), propozycja „Czekam na”
 * (`TaskWaitingProposal`), zmiany zgłoszenia (`useIssueMoves`,
 * `TaskIssueMoves`), szablon z zadania (`TaskTemplateForm`) i przedziały
 * (`TaskSessions`).
 */
import { computed, ref } from "vue";
import {
  Archive,
  ArrowRightCircle,
  BookmarkPlus,
  Check,
  ChevronDown,
  ExternalLink,
  LockOpen,
  Pencil,
  Pin,
  PinOff,
  Play,
  RotateCcw,
  Square,
  Trash2,
} from "lucide-vue-next";
import TaskPlanning from "../TaskPlanning.vue";
import ReorderHandle from "./ReorderHandle.vue";
import AgentTaskTime from "./AgentTaskTime.vue";
import JiraIssueActions from "./JiraIssueActions.vue";
import RedmineIssueActions from "./RedmineIssueActions.vue";
import TsTaskField from "./TsTaskField.vue";
import TaskSummary from "./task/TaskSummary.vue";
import TaskWaitingProposal from "./task/TaskWaitingProposal.vue";
import TaskIssueMoves from "./task/TaskIssueMoves.vue";
import TaskTemplateForm from "./task/TaskTemplateForm.vue";
import TaskSessions from "./task/TaskSessions.vue";
import { useIssueMoves } from "./task/issueMoves";
import {
  duration,
  jiraConnected,
  redmineConnected,
  setPinned,
  setWaiting,
  type WaitingChange,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import type { CorrectionOf } from "../estimates";
import { useSettings } from "../target";
import { waitingProposal } from "../recurrence";
import { issueRef } from "../issue";

const props = defineProps<{
  task: Task;
  /** Przedziały zadania, od najnowszego. */
  sessions: Session[];
  running: boolean;
  started: boolean;
  expanded: boolean;
  busy: boolean;
  now: number;
  todayKey: string;
  projects: string[];
  /** Poprawka szacunków dla podpowiedzi w rozwiniętym kafelku. */
  correction?: CorrectionOf;
}>();
const emit = defineEmits<{
  toggle: [];
  act: [action: string];
  priority: [event: Event];
  edit: [];
  remove: [];
  session: [session: Session];
  updated: [data: Snapshot];
  error: [message: string];
  move: [delta: -1 | 1];
  dragstart: [event: DragEvent];
  dragend: [];
}>();
const message = (e: unknown) => String(e instanceof Error ? e.message : e);

const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
/** Zamknięte przedziały liczone raz na zmianę danych (B6); co sekundę tylko trwający. */
const closedTime = computed(() =>
  props.sessions.reduce(
    (sum, s) => (s.endedAt === null ? sum : sum + elapsed(s)),
    0,
  ),
);
const openSession = computed(() =>
  props.sessions.find((s) => s.endedAt === null),
);
const taskTime = computed(
  () => closedTime.value + (openSession.value ? elapsed(openSession.value) : 0),
);
/** „Zapisz jako szablon”: formularz pod paskiem akcji. */
const templateOpen = ref(false);
/** Zgłoszenie zadania (Jira albo Redmine): klucz, status, link na kafelku. */
const issue = computed(() => issueRef(props.task));
/**
 * Propozycja „Czekam na” ze statusu zgłoszenia (lista statusów czekania
 * zależy od systemu zgłoszenia).
 */
const settings = useSettings();
const proposal = computed(() =>
  waitingProposal(
    props.task,
    (issue.value?.system === "redmine"
      ? settings.value.waitingRedmineStatuses
      : settings.value.waitingJiraStatuses) ?? [],
  ),
);
const waitingBusy = ref(false);
async function changeWaiting(change: WaitingChange) {
  if (waitingBusy.value) return;
  waitingBusy.value = true;
  emit("error", "");
  try {
    emit("updated", await setWaiting(props.task.id, change));
  } catch (e) {
    emit("error", message(e));
  } finally {
    waitingBusy.value = false;
  }
}
/** Przypięcie do widgetu („w toku”): przyciski przełączania jednym kliknięciem. */
const pinned = computed(() => (props.task.details?.pinnedAt ?? 0) > 0);
const pinBusy = ref(false);
async function togglePin() {
  if (pinBusy.value) return;
  pinBusy.value = true;
  emit("error", "");
  try {
    emit("updated", await setPinned(props.task.id, !pinned.value));
  } catch (e) {
    emit("error", message(e));
  } finally {
    pinBusy.value = false;
  }
}
const moves = useIssueMoves(
  () => props.task,
  (data) => emit("updated", data),
  (text) => emit("error", text),
);
/** Zapisy do systemu zgłoszenia zadania włączone (`jiraWrite` / `redmineWrite`). */
const issueWrite = computed(() =>
  issue.value?.system === "redmine"
    ? settings.value.redmineWrite
    : settings.value.jiraWrite,
);
</script>

<template>
  <article
    class="task-card"
    :class="{
      isRunning: running,
      isDone: task.done,
    }"
  >
    <div class="task-row">
      <ReorderHandle
        :title="task.title"
        :task-id="task.id"
        scope="tasks"
        :disabled="busy"
        @move="emit('move', $event)"
        @dragstart="emit('dragstart', $event)"
        @dragend="emit('dragend')"
      />
      <button
        class="checkbox"
        :class="{ checked: task.done }"
        :aria-label="(task.done ? 'Przywróć: ' : 'Ukończ: ') + task.title"
        :disabled="busy"
        @click="emit('act', 'toggle')"
      >
        <Check v-if="task.done" :size="14" />
      </button>
      <button
        class="task-expand"
        title="Szczegóły: przedziały pracy, plan, notatki i akcje"
        :aria-label="'Przedziały: ' + task.title"
        :aria-expanded="expanded"
        :aria-controls="'sessions-' + task.id"
        @click="emit('toggle')"
      >
        <TaskSummary
          :task="task"
          :running="running"
          :now="now"
          :today-key="todayKey"
          :task-time="taskTime" /><span
          class="task-duration mono"
          :class="{ zero: !taskTime }"
          >{{ duration(taskTime) }}</span
        ><ChevronDown :size="16" :class="{ rotated: expanded }" />
      </button>
      <select
        class="inline-priority"
        :class="{
          high: (task.details?.priority ?? 0) > 0,
          low: (task.details?.priority ?? 0) < 0,
        }"
        :value="task.details?.priority ?? 0"
        :aria-label="'Priorytet: ' + task.title"
        :disabled="busy"
        @change="emit('priority', $event)"
      >
        <option :value="1">↑ Wysoki</option>
        <option :value="0">Normalny</option>
        <option :value="-1">↓ Niski</option>
      </select>
      <button
        v-if="task.details?.waiting && !task.done"
        class="action-button row-unblock"
        :aria-label="'Odblokuj: ' + task.title"
        title="Zdejmij blokadę „Czekam na” bez startu pomiaru"
        :disabled="busy || waitingBusy"
        @click="changeWaiting({ action: 'unblock' })"
      >
        <LockOpen :size="14" />Odblokuj
      </button>
      <button
        v-if="!task.done"
        class="row-timer"
        :class="{ stop: running }"
        :aria-label="(running ? 'Stop: ' : 'Start: ') + task.title"
        :disabled="busy"
        @click="emit('act', running ? 'stop' : 'start')"
      >
        <Square v-if="running" :size="12" /><Play v-else :size="12" />{{
          running ? "Stop" : "Start"
        }}
      </button>
      <span v-else class="done-label">Gotowe</span>
    </div>
    <TaskWaitingProposal
      v-if="proposal && issue"
      :title="task.title"
      :issue="issue"
      :proposal="proposal"
      :disabled="busy || waitingBusy"
      @change="changeWaiting"
    />
    <div v-if="expanded" :id="'sessions-' + task.id" class="task-details">
      <div class="details-actions" role="group" aria-label="Akcje zadania">
        <button
          v-if="issue?.url"
          class="action-button"
          :aria-label="`Otwórz w ${issue.system === 'jira' ? 'Jirze' : 'Redmine'}: ${issue.key}`"
          @click="moves.openIssue(issue)"
        >
          <ExternalLink :size="14" />{{ issue.key }}
        </button>
        <button
          v-if="task.jiraKey && settings.jiraWrite"
          class="action-button"
          :aria-label="'Przesuń w Jirze: ' + task.jiraKey"
          :aria-expanded="moves.jiraList !== null"
          :disabled="moves.busy"
          title="Wybierz przejście zgłoszenia w Jirze"
          @click="moves.openJiraList"
        >
          <ArrowRightCircle :size="14" />Przesuń w Jirze…
        </button>
        <button
          v-if="issue?.system === 'redmine' && settings.redmineWrite"
          class="action-button"
          :aria-label="'Przesuń w Redmine: ' + issue.key"
          :aria-expanded="moves.redmineList !== null"
          :disabled="moves.busy"
          title="Wybierz nowy status zgłoszenia w Redmine"
          @click="moves.openRedmineList"
        >
          <ArrowRightCircle :size="14" />Przesuń w Redmine…
        </button>
        <span class="details-actions-spacer" />
        <button
          v-if="!task.done"
          class="action-button"
          :aria-label="
            (pinned ? 'Odepnij od widgetu: ' : 'Przypnij do widgetu: ') +
            task.title
          "
          :aria-pressed="pinned"
          title="Przypięte zadania („w toku”) są w widgecie przyciskami przełączania"
          :disabled="busy || pinBusy"
          @click="togglePin"
        >
          <PinOff v-if="pinned" :size="14" /><Pin v-else :size="14" />{{
            pinned ? "Odepnij" : "Przypnij do widgetu"
          }}
        </button>
        <button
          class="action-button"
          :aria-label="'Zapisz jako szablon: ' + task.title"
          title="Tytuł, szacunek, kroki listy, projekt i priorytet jako szablon /nazwa"
          :aria-expanded="templateOpen"
          @click="templateOpen = !templateOpen"
        >
          <BookmarkPlus :size="14" />Zapisz jako szablon
        </button>
        <button
          class="action-button"
          :disabled="busy"
          @click="emit('act', 'archive')"
        >
          <Archive :size="14" />Archiwizuj
        </button>
        <button
          class="action-button"
          :disabled="busy"
          :aria-label="'Edytuj: ' + task.title"
          title="Edytuj zadanie"
          @click="emit('edit')"
        >
          <Pencil :size="14" />Zmień nazwę</button
        ><button
          class="action-button danger"
          :disabled="busy"
          :aria-label="'Usuń: ' + task.title"
          title="Usuń zadanie"
          @click="emit('remove')"
        >
          <Trash2 :size="14" />Usuń
        </button>
      </div>
      <TaskIssueMoves :task="task" :issue="issue" :moves="moves" />
      <JiraIssueActions
        v-if="task.jiraKey && jiraConnected"
        :issue-key="task.jiraKey"
        :summary="task.title"
      />
      <RedmineIssueActions
        v-if="issue?.system === 'redmine' && task.redmineId && redmineConnected"
        :issue-id="task.redmineId"
      />
      <p
        v-if="issue && task.details?.jiraNoAsk && issueWrite"
        class="hint jira-no-ask"
      >
        Bez pytania o przesunięcie {{ issue.key }} przy starcie pracy.
        <button
          type="button"
          class="action-button"
          :disabled="moves.busy"
          :aria-label="'Pytaj o przesunięcie: ' + task.title"
          @click="moves.askAgain"
        >
          <RotateCcw :size="14" />Pytaj znowu
        </button>
      </p>
      <TaskTemplateForm
        v-model:open="templateOpen"
        :task="task"
        @error="emit('error', $event)"
      />
      <TaskSessions
        :sessions="sessions"
        :running="running"
        :busy="busy"
        :now="now"
        @session="emit('session', $event)"
      />
      <AgentTaskTime :task="task" :sessions="sessions" :own="taskTime" />
      <TsTaskField
        :task="task"
        @updated="emit('updated', $event)"
        @error="emit('error', $event)"
      />
      <TaskPlanning
        :task="task"
        :projects="projects"
        :show-priority="false"
        :correction="correction"
        @updated="emit('updated', $event)"
      />
    </div>
  </article>
</template>
