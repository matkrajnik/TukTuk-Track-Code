<script setup lang="ts">
/**
 * „Otwórz worktree w Orce” w rozwiniętym kafelku zadania ze zgłoszeniem
 * (Jira albo Redmine; PLAN_BACKLOG.md, B9). Przycisk tylko otwiera dialog
 * z podglądem polecenia; Orka nie dostaje niczego bez kliknięcia w dialogu.
 */
import { computed, ref } from "vue";
import { GitFork } from "lucide-vue-next";
import OrcaWorktreeDialog from "./OrcaWorktreeDialog.vue";
import type { Task } from "../../store";
import { issueRef } from "../../issue";

const props = defineProps<{ task: Task; todayKey: string }>();
const open = ref(false);
const issue = computed(() => issueRef(props.task));
</script>

<template>
  <div v-if="issue" class="task-orca">
    <button
      type="button"
      class="action-button"
      :aria-label="'Otwórz worktree w Orce: ' + issue.key"
      :aria-expanded="open"
      @click="open = true"
    >
      <GitFork :size="14" />Otwórz worktree w Orce
    </button>
    <OrcaWorktreeDialog
      v-if="open"
      :task="task"
      :today-key="todayKey"
      @close="open = false"
    />
  </div>
</template>
