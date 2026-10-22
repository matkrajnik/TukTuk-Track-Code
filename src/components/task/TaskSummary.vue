<script setup lang="ts">
/**
 * Tytuł i chipy zwiniętego kafelka zadania: projekt, zadanie TS, plan,
 * zgłoszenie z polami, „Czekam na”, szacunek i termin. Prezentacyjny: używa go
 * kafelek na liście zadań i lista Raportu dnia (B5, z notatką dnia).
 */
import { computed } from "vue";
import JiraFieldChips from "../JiraFieldChips.vue";
import type { Task } from "../../store";
import { dayText, hhmm } from "../../time";
import { estimateMs, overEstimate, percent } from "../../estimates";
import { waitingAge } from "../../recurrence";
import { dueChip, issueRef, statusTitle } from "../../issue";
import { resolveTsTask } from "../../timesheet";
import { useTsProjectMap } from "../../composables/useTsProjectMap";

const props = defineProps<{
  task: Task;
  running: boolean;
  now: number;
  todayKey: string;
  /** Zmierzony czas zadania (ms). */
  taskTime: number;
  /** Notatka dnia (Raport dnia); kafelek listy jej nie pokazuje. */
  note?: string;
  /** Znacznik 🔔 z godziną najbliższego przypomnienia zadania (C3). */
  reminder?: { label: string; title: string } | null;
}>();

const estimate = computed(() => estimateMs(props.task));
/** „Dziś 14:00”, „pt., 16 paź” – kiedy zadanie jest zaplanowane. */
const plannedLabel = computed(() => {
  const d = props.task.details;
  if (!d?.plannedDay || props.task.done) return "";
  const day =
    d.plannedDay === props.todayKey ? "Dziś" : dayText(d.plannedDay);
  return d.plannedStart ? `${day} ${d.plannedStart}` : day;
});
/** Zadanie TS (jawne albo z domyślnego mapowania projektu) do chipa „TS: …”. */
const tsMap = useTsProjectMap();
const tsTask = computed(() => resolveTsTask(props.task, tsMap.value));
/** Zgłoszenie zadania (Jira albo Redmine): klucz, status, link na kafelku. */
const issue = computed(() => issueRef(props.task));
/** Chipy Redmine: tracker i termin „DD.MM” (stały zestaw, odpowiednik `JiraFieldChips`). */
const redmineDue = computed(() =>
  issue.value?.system === "redmine" ? dueChip(props.task.redmineDue) : "",
);
/** „Czekam na”: od kiedy trwa blokada. */
const waitingSince = computed(() =>
  props.task.details?.waiting && !props.task.done
    ? waitingAge(props.task.details.waitingSince, props.now)
    : "",
);
</script>

<template>
  <span class="task-name-status">
    <span class="task-title">{{ task.title }}</span>
    <span
      v-if="
        issue ||
        task.details?.project ||
        tsTask ||
        plannedLabel ||
        reminder ||
        (!task.done && (task.details?.waiting || running))
      "
      class="task-meta"
    >
      <span v-if="running" class="task-status task-live">W trakcie</span>
      <span v-if="task.details?.project" class="task-chip">{{
        task.details.project
      }}</span>
      <span
        v-if="tsTask"
        class="task-chip task-ts"
        :title="
          tsTask.source === 'project'
            ? 'Zadanie TS z domyślnego mapowania projektu'
            : 'Zadanie TS wybrane dla tego zadania'
        "
        >TS: {{ tsTask.name
        }}<template v-if="tsTask.source === 'project'">
          (z projektu)</template
        ></span
      >
      <span
        v-if="plannedLabel"
        class="task-chip task-planned"
        :class="{ today: task.details?.plannedDay === todayKey }"
        :title="'Zaplanowane: ' + task.details?.plannedDay"
        >{{ plannedLabel }}</span
      >
      <span
        v-if="reminder"
        class="task-chip task-reminder"
        :title="reminder.title"
        >🔔 {{ reminder.label }}</span
      >
      <span
        v-if="issue"
        class="task-issue-key mono"
        :class="`task-${issue.system}-key`"
        >{{ issue.key }}</span
      >
      <span
        v-if="issue?.status"
        class="task-issue-status"
        :class="`task-${issue.system}-status`"
        :title="statusTitle(issue)"
        >{{ issue.status }}</span
      >
      <JiraFieldChips v-if="task.jiraKey" :fields="task.jiraFields" />
      <span
        v-if="issue?.system === 'redmine' && task.redmineTracker"
        class="task-chip task-redmine-tracker"
        title="Tracker w Redmine"
        >{{ task.redmineTracker }}</span
      >
      <span
        v-if="redmineDue"
        class="task-chip task-redmine-due"
        :title="'Termin w Redmine: ' + task.redmineDue"
        >termin {{ redmineDue }}</span
      >
      <span v-if="task.details?.waiting && !task.done" class="task-status"
        >Czekam na: {{ task.details.waiting
        }}<template v-if="waitingSince"> · {{ waitingSince }}</template></span
      >
    </span>
    <span v-if="task.details?.estimateMinutes" class="task-status"
      >Plan {{ hhmm(estimate) }} ·
      <span :class="{ 'estimate-over': overEstimate(taskTime, estimate) }"
        >{{ percent(taskTime, estimate) }}%</span
      ></span
    >
    <span
      v-if="task.details?.dueDay && !task.done"
      class="task-deadline"
      :class="{ overdue: task.details.dueDay < todayKey }"
      >Termin: {{ dayText(task.details.dueDay) }}</span
    ><span v-if="note?.trim()" class="task-day-note"
      >Notatka: {{ note.trim() }}</span
    > </span
  >
</template>
