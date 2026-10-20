<script setup lang="ts">
import { ExternalLink, Play, Square } from "lucide-vue-next";
import {
  duration,
  initials,
  priorityBand,
  type RedmineIssue,
} from "../store";
import { dueChip } from "../issue";

/**
 * Karta zgłoszenia w widoku Redmine (PLAN_1.9.md, R3): numer, temat, tracker,
 * projekt (podprojekt albo projekt spoza śledzonych), status, priorytet
 * kolorem pasma jak w Jirze, osoba przypisana (inicjały, gdy nie ja), wersja
 * docelowa, termin, czas zmierzony lokalnie i plan, „Otwórz w Redmine” oraz
 * „Rozpocznij” / „Stop”. Menu akcji dochodzi w R4.
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
}>();
const emit = defineEmits<{ start: [id: number]; stop: []; open: [url: string] }>();

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
      >
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
        @click="emit('start', issue.id)"
      >
        <Play :size="12" />Rozpocznij
      </button>
      <!-- R4: menu akcji karty („Przesuń do…”, „Przypisz do mnie”, komentarz, gałąź). -->
    </div>
  </article>
</template>
