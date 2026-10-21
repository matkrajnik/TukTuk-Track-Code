<script setup lang="ts">
/**
 * Rozwinięte listy „Przesuń w Jirze…” i „Przesuń w Redmine…” kafelka zadania
 * oraz błąd zmiany z linkiem do zgłoszenia (stan w `useIssueMoves`).
 */
import { ArrowRight, X } from "lucide-vue-next";
import type { Task } from "../../store";
import type { IssueRef } from "../../issue";
import type { IssueMoves } from "./issueMoves";

defineProps<{
  task: Task;
  issue: IssueRef | null;
  moves: IssueMoves;
}>();
</script>

<template>
  <div
    v-if="moves.jiraList !== null"
    class="jira-move-list"
    role="group"
    :aria-label="'Przejścia w Jirze: ' + task.jiraKey"
  >
    <span v-if="moves.jiraList.length"
      >{{ task.jiraKey }}<template v-if="task.jiraStatus">
        ({{ task.jiraStatus }})</template
      >:</span
    >
    <span v-else>Jira nie ma dostępnych przejść dla {{ task.jiraKey }}.</span>
    <button
      v-for="t in moves.jiraList"
      :key="t.id"
      class="action-button"
      :disabled="moves.busy"
      :aria-label="`Przesuń ${task.jiraKey} do: ${t.toStatus}`"
      @click="moves.moveJira(t)"
    >
      <ArrowRight :size="14" />{{
        t.name && t.name !== t.toStatus ? `${t.name} → ${t.toStatus}` : t.toStatus
      }}
    </button>
    <button class="action-button" @click="moves.closeJira()">
      <X :size="14" />Anuluj
    </button>
  </div>
  <div
    v-if="moves.redmineList !== null && issue?.system === 'redmine'"
    class="jira-move-list redmine-move-list"
    role="group"
    :aria-label="'Statusy w Redmine: ' + issue.key"
  >
    <span v-if="moves.redmineList.length"
      >{{ issue.key }}<template v-if="issue.status">
        ({{ issue.status }})</template
      >:</span
    >
    <span v-else>Redmine nie ma dozwolonych statusów dla {{ issue.key }}.</span>
    <button
      v-for="s in moves.redmineList"
      :key="s.id"
      class="action-button"
      :disabled="moves.busy"
      :aria-label="`Przesuń ${issue.key} do: ${s.name}`"
      @click="moves.moveRedmine(s)"
    >
      <ArrowRight :size="14" />{{ s.name }}
    </button>
    <button class="action-button" @click="moves.closeRedmine()">
      <X :size="14" />Anuluj
    </button>
  </div>
  <p v-if="moves.error" class="jira-move-error" role="alert">
    {{ moves.error }}
    <button
      v-if="task.jiraUrl"
      type="button"
      class="text-link"
      @click="moves.openJira(task.jiraUrl)"
    >
      Otwórz {{ task.jiraKey }} w Jirze
    </button>
    <button
      v-else-if="issue?.system === 'redmine' && issue.url"
      type="button"
      class="text-link"
      @click="moves.openIssue(issue)"
    >
      Otwórz {{ issue.key }} w Redmine
    </button>
  </p>
</template>
