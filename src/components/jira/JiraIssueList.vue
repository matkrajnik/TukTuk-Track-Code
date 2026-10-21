<script setup lang="ts">
/** Lista „Przypisane do mnie” widoku Jira (dane i odświeżanie w `JiraView`). */
import { ExternalLink, Play, Square } from "lucide-vue-next";
import JiraFieldChips from "../JiraFieldChips.vue";
import IssueTaskMarker from "../IssueTaskMarker.vue";
import type { JiraListIssue } from "../../store";
import { issueStartTitle, type IssueTaskState } from "../../issueTask";

defineProps<{
  /** Zgłoszenia po filtrze wyszukiwania. */
  issues: JiraListIssue[];
  /** Są wybrane tablice (inaczej bez podsumowania). */
  hasBoards: boolean;
  busy: boolean;
  loaded: boolean;
  /** Ostrzeżenia pobierania: wtedy bez komunikatu o pustej liście. */
  warned: boolean;
  query: string;
  updatedAt: string;
  activeUrl?: string | null;
  /** Lokalne zadania zgłoszeń po kluczu (B5): znacznik i ostrzeżenie. */
  tasks?: Map<string, IssueTaskState>;
}>();
const emit = defineEmits<{ open: [url: string]; start: [key: string]; stop: [] }>();
</script>

<template>
  <p v-if="hasBoards" class="jira-result-info">
    Przypisane do mnie · {{ issues.length }} zgłoszeń<span v-if="updatedAt">
      · Odświeżono {{ updatedAt }}</span
    >
  </p>
  <p v-if="busy" class="hint" role="status">Proszę czekać…</p>
  <div class="jira-issues" :aria-busy="busy">
    <article
      v-for="issue in issues"
      :key="issue.key"
      class="jira-issue"
      :class="{
        isRunning: activeUrl === issue.url,
        hasTask: tasks?.get(issue.key)?.kind === 'task',
      }"
    >
      <div class="jira-issue-body">
        <div class="jira-issue-meta">
          <button
            class="text-link"
            :aria-label="'Otwórz w Jirze: ' + issue.key"
            :disabled="busy"
            @click="emit('open', issue.url)"
          >
            {{ issue.key }}<ExternalLink :size="12" /></button
          ><span class="jira-status">{{ issue.status }}</span
          ><IssueTaskMarker :state="tasks?.get(issue.key)" />
        </div>
        <h2>{{ issue.summary }}</h2>
        <p>{{ issue.boards.join(" · ") }}</p>
        <div v-if="issue.fields?.length" class="jira-issue-fields">
          <JiraFieldChips :fields="issue.fields" />
        </div>
      </div>
      <button
        v-if="activeUrl === issue.url"
        class="row-timer stop"
        :disabled="busy"
        :aria-label="'Zatrzymaj: ' + issue.key"
        @click="emit('stop')"
      >
        <Square :size="12" />Stop</button
      ><button
        v-else
        class="row-timer"
        :disabled="busy"
        :aria-label="'Rozpocznij: ' + issue.key"
        :title="issueStartTitle(tasks?.get(issue.key))"
        @click="emit('start', issue.key)"
      >
        <Play :size="12" />Rozpocznij
      </button>
    </article>
  </div>
  <p v-if="loaded && !issues.length && !busy && !warned" class="empty-state">
    {{
      query
        ? "Brak zgłoszeń pasujących do wyszukiwania."
        : "Brak przypisanych Ci zgłoszeń na wybranych tablicach."
    }}
  </p>
</template>
