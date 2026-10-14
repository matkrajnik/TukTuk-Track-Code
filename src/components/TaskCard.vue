<script setup lang="ts">
import { computed } from "vue";
import {
  Check,
  ChevronDown,
  ExternalLink,
  Pencil,
  Play,
  Square,
  Trash2,
} from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";
import TaskPlanning from "../TaskPlanning.vue";
import {
  desktop,
  duration,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import { stamp } from "../time";

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
}>();

const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
const taskTime = computed(() =>
  props.sessions.reduce((sum, s) => sum + elapsed(s), 0),
);
async function openJira(url: string) {
  try {
    if (desktop) await invoke("jira_open", { url });
  } catch (e) {
    emit("error", String(e));
  }
}
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
        :aria-label="'Przedziały: ' + task.title"
        :aria-expanded="expanded"
        :aria-controls="'sessions-' + task.id"
        @click="emit('toggle')"
      >
        <span class="task-name-status">
          <span class="task-title">{{ task.title }}</span>
          <span
            v-if="
              task.jiraKey ||
              (!task.done && (task.details?.waiting || started))
            "
            class="task-meta"
          >
            <span v-if="task.jiraKey" class="task-jira-key mono">{{
              task.jiraKey
            }}</span>
            <span
              v-if="!task.done && !task.details?.waiting && started"
              class="task-status"
              >W trakcie</span
            >
            <span v-if="task.details?.waiting && !task.done" class="task-status"
              >Czekam na: {{ task.details.waiting }}</span
            >
          </span>
          <span v-if="task.details?.estimateMinutes" class="task-status"
            >Plan {{ duration(task.details.estimateMinutes * 60000) }} ·
            {{
              Math.round(
                (taskTime / (task.details.estimateMinutes * 60000)) * 100,
              )
            }}%</span
          >
          <span
            v-if="task.details?.dueDay && !task.done"
            class="task-deadline"
            :class="{ overdue: task.details.dueDay < todayKey }"
            >Termin: {{ task.details.dueDay }}</span
          > </span
        ><span class="task-duration mono">{{ duration(taskTime) }}</span
        ><ChevronDown :size="16" :class="{ rotated: expanded }" />
      </button>
      <select
        class="inline-priority"
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
    <div v-if="expanded" :id="'sessions-' + task.id" class="task-details">
      <div class="details-heading">
        <span>Przedziały pracy</span>
        <div>
          <button
            class="today-button"
            :disabled="busy"
            @click="emit('act', 'archive')"
          >
            Archiwizuj
          </button>
          <button
            v-if="task.jiraUrl"
            class="text-link"
            :aria-label="'Otwórz w Jirze: ' + task.jiraKey"
            @click="openJira(task.jiraUrl)"
          >
            {{ task.jiraKey }}<ExternalLink :size="13" />
          </button>
          <button
            class="icon-button"
            :disabled="busy"
            :aria-label="'Edytuj: ' + task.title"
            title="Edytuj zadanie"
            @click="emit('edit')"
          >
            <Pencil :size="15" /></button
          ><button
            class="icon-button danger"
            :disabled="busy"
            :aria-label="'Usuń: ' + task.title"
            title="Usuń zadanie"
            @click="emit('remove')"
          >
            <Trash2 :size="15" />
          </button>
        </div>
      </div>
      <div v-for="session in sessions" :key="session.id" class="session-row">
        <div class="interval">
          <time :datetime="new Date(session.startedAt).toISOString()">{{
            stamp(session.startedAt)
          }}</time
          ><span class="interval-arrow">→</span
          ><time
            v-if="session.endedAt !== null"
            :datetime="new Date(session.endedAt).toISOString()"
            >{{ stamp(session.endedAt) }}</time
          ><span v-else class="running-label">W trakcie</span>
        </div>
        <span class="mono session-duration">{{
          duration(elapsed(session))
        }}</span>
        <button
          class="icon-button"
          :aria-label="'Edytuj przedział ' + session.id"
          :disabled="busy || session.endedAt === null"
          :title="
            session.endedAt === null
              ? 'Zatrzymaj pomiar przed edycją'
              : 'Edytuj przedział'
          "
          @click="emit('session', session)"
        >
          <Pencil :size="14" />
        </button>
      </div>
      <p v-if="!sessions.length" class="empty-detail">
        Brak przedziałów. Kliknij Start, aby rozpocząć pomiar.
      </p>
      <p v-if="running" class="hint">
        Zatrzymaj bieżący pomiar, aby edytować jego godziny.
      </p>
      <TaskPlanning
        :task="task"
        :projects="projects"
        @updated="emit('updated', $event)"
      />
    </div>
  </article>
</template>
