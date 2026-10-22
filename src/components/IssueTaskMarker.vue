<script setup lang="ts">
/**
 * Znacznik lokalnego zadania zgłoszenia na listach Jiry i Redmine (B5):
 * „W zadaniach · dziś 01:20” albo ostrzeżenie, że zadanie jest w archiwum
 * lub w koszu i „Rozpocznij” je przywróci (bez duplikatu).
 */
import { computed } from "vue";
import { TriangleAlert, ListChecks } from "lucide-vue-next";
import {
  issueTaskMarker,
  issueTaskWarning,
  type IssueTaskState,
} from "../issueTask";

const props = defineProps<{ state?: IssueTaskState }>();
const warning = computed(() =>
  props.state ? issueTaskWarning(props.state) : "",
);
</script>

<template>
  <span
    v-if="state?.kind === 'task'"
    class="issue-task-marker"
    :title="'Zadanie w TukTuk Track: ' + state.task.title"
    ><ListChecks :size="12" />{{ issueTaskMarker(state) }}</span
  ><span v-else-if="warning" class="issue-task-warning" role="note"
    ><TriangleAlert :size="12" />{{ warning }}</span
  >
</template>
