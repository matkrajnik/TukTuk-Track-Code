<script setup lang="ts">
import { ref } from "vue";
import {
  ArrowRightCircle,
  Ellipsis,
  ExternalLink,
  Play,
  Square,
} from "lucide-vue-next";
import {
  duration,
  initials,
  priorityBand,
  redmineIssueTransitions,
  type RedmineIssue,
  type RedmineStatus,
} from "../store";
import { dueChip } from "../issue";
import { useSettings } from "../target";
import RedmineIssueActions from "./RedmineIssueActions.vue";
import IssueTaskMarker from "./IssueTaskMarker.vue";
import { issueStartTitle, type IssueTaskState } from "../issueTask";

/**
 * Karta zgłoszenia w widoku Redmine (PLAN_1.9.md, R3): numer, temat, tracker,
 * projekt (podprojekt albo projekt spoza śledzonych), status, priorytet
 * kolorem pasma jak w Jirze, osoba przypisana (inicjały, gdy nie ja), wersja
 * docelowa, termin, czas zmierzony lokalnie i plan, „Otwórz w Redmine” oraz
 * „Rozpocznij” / „Stop”. Menu akcji (R4): „Przesuń do…” (dozwolone statusy;
 * zmianę wykonuje widok, który przenosi kartę do grupy nowego statusu),
 * „Przypisz do mnie”, „Dodaj komentarz” i „Kopiuj nazwę gałęzi”.
 */
const props = defineProps<{
  issue: RedmineIssue;
  /** Identyfikator mojego konta Redmine (inicjały tylko dla innych osób). */
  me: number;
  running: boolean;
  busy: boolean;
  /** Pokaż nazwę projektu (podprojekt albo projekt spoza śledzonych). */
  showProject: boolean;
  /** Czas zmierzony w Todo Track i dzień planu lokalnego zadania. */
  local?: { ms: number; planned: string };
  /** Lokalne zadanie zgłoszenia (B5): znacznik albo ostrzeżenie o koszu/archiwum. */
  taskState?: IssueTaskState;
}>();
const emit = defineEmits<{
  start: [id: number];
  stop: [];
  open: [url: string];
  move: [issue: RedmineIssue, status: RedmineStatus];
  assigned: [issue: RedmineIssue | null];
}>();
const settings = useSettings();

const menu = ref(false);
/** Dozwolone statusy („Przesuń do…”); `null` = lista zamknięta. */
const statuses = ref<RedmineStatus[] | null>(null);
const loading = ref(false);
const menuError = ref("");
async function toggleStatuses() {
  if (statuses.value !== null) {
    statuses.value = null;
    return;
  }
  if (loading.value) return;
  loading.value = true;
  menuError.value = "";
  try {
    statuses.value = await redmineIssueTransitions(props.issue.id);
  } catch (e) {
    menuError.value = String(e instanceof Error ? e.message : e);
  } finally {
    loading.value = false;
  }
}
function moveTo(status: RedmineStatus) {
  statuses.value = null;
  menu.value = false;
  emit("move", props.issue, status);
}

const band = () => {
  const value = priorityBand(props.issue.priority);
  return value === 1 ? "high" : value === -1 ? "low" : "";
};
</script>
<template>
  <article
    class="jira-card redmine-card"
    :class="{ isRunning: running }"
    :aria-label="`#${issue.id} ${issue.subject}`"
  >
    <div class="jira-card-meta">
      <span class="mono redmine-number">#{{ issue.id }}</span
      ><span v-if="issue.tracker" class="jira-tag redmine-tracker">{{
        issue.tracker
      }}</span
      ><span
        v-if="showProject && issue.project"
        class="jira-tag redmine-project"
        :title="'Projekt: ' + issue.project"
        >{{ issue.project }}</span
      ><span
        v-if="issue.priority"
        class="jira-tag jira-priority"
        :class="band()"
        :title="'Priorytet: ' + issue.priority"
        >{{ issue.priority }}</span
      >
      <span
        v-if="issue.assignedToId !== me"
        class="jira-avatar"
        :title="issue.assignedTo || 'Nieprzypisane'"
        :aria-label="
          issue.assignedTo ? 'Przypisane: ' + issue.assignedTo : 'Nieprzypisane'
        "
        >{{ issue.assignedTo ? initials(issue.assignedTo) : "–" }}</span
      >
    </div>
    <h3>{{ issue.subject }}</h3>
    <div class="jira-card-local">
      <span class="jira-status redmine-status">{{ issue.status }}</span
      ><span v-if="issue.version" class="jira-tag redmine-version"
        >wersja {{ issue.version }}</span
      ><span v-if="issue.dueDate" class="jira-tag redmine-due"
        >termin {{ dueChip(issue.dueDate) }}</span
      ><span v-if="running" class="task-status task-live">mierzone teraz</span
      ><span v-if="local?.ms" class="mono" title="Czas zmierzony w Todo Track">{{
        duration(local.ms)
      }}</span
      ><span v-if="local?.planned" class="task-chip task-planned"
        >zaplanowane na {{ local.planned }}</span
      ><IssueTaskMarker :state="taskState" />
    </div>
    <div class="jira-card-actions">
      <button
        class="text-link"
        :aria-label="'Otwórz w Redmine: #' + issue.id"
        :disabled="busy"
        @click="emit('open', issue.url)"
      >
        Otwórz w Redmine<ExternalLink :size="12" /></button
      ><button
        v-if="running"
        class="row-timer stop"
        :disabled="busy"
        :aria-label="'Zatrzymaj: #' + issue.id"
        @click="emit('stop')"
      >
        <Square :size="12" />Stop</button
      ><button
        v-else
        class="row-timer"
        :disabled="busy"
        :aria-label="'Rozpocznij: #' + issue.id"
        :title="issueStartTitle(taskState)"
        @click="emit('start', issue.id)"
      >
        <Play :size="12" />Rozpocznij</button
      ><button
        class="icon-button redmine-menu-button"
        :aria-label="'Akcje: #' + issue.id"
        :aria-expanded="menu"
        title="Akcje zgłoszenia"
        @click="menu = !menu"
      >
        <Ellipsis :size="15" />
      </button>
    </div>
    <div v-if="menu" class="jira-card-menu redmine-card-menu">
      <div
        v-if="settings.redmineWrite"
        class="jira-issue-actions-row"
        role="group"
        :aria-label="'Status w Redmine: #' + issue.id"
      >
        <button
          type="button"
          class="action-button"
          :disabled="busy || loading"
          :aria-expanded="statuses !== null"
          :aria-label="'Przesuń do: #' + issue.id"
          @click="toggleStatuses"
        >
          <ArrowRightCircle :size="14" />Przesuń do…
        </button>
        <template v-if="statuses !== null">
          <button
            v-for="s in statuses"
            :key="s.id"
            type="button"
            class="action-button"
            :disabled="busy"
            :aria-label="`Przesuń #${issue.id} do: ${s.name}`"
            @click="moveTo(s)"
          >
            {{ s.name }}
          </button>
          <span v-if="!statuses.length" class="hint"
            >Redmine nie ma dozwolonych statusów dla #{{ issue.id }}.</span
          >
        </template>
      </div>
      <p v-if="menuError" class="jira-move-error" role="alert">
        {{ menuError }}
      </p>
      <RedmineIssueActions
        :issue-id="issue.id"
        :me="issue.assignedToId === me"
        @assigned="emit('assigned', $event)"
      />
    </div>
  </article>
</template>
