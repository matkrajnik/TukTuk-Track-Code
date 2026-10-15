<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import {
  Archive,
  BookmarkPlus,
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
import ReorderHandle from "./ReorderHandle.vue";
import {
  desktop,
  duration,
  saveTemplate,
  templateFromTask,
  type Session,
  type Snapshot,
  type Task,
} from "../store";
import { dayText, stamp } from "../time";
import { hhmm } from "../target";
import { openSettings } from "../navigation";
import { estimateMs, overEstimate, percent } from "../estimates";

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
  move: [delta: -1 | 1];
  dragstart: [event: DragEvent];
  dragend: [];
}>();

const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
const taskTime = computed(() =>
  props.sessions.reduce((sum, s) => sum + elapsed(s), 0),
);
const estimate = computed(() => estimateMs(props.task));
/** „Dziś 14:00”, „pt., 16 paź” – kiedy zadanie jest zaplanowane. */
const plannedLabel = computed(() => {
  const d = props.task.details;
  if (!d?.plannedDay || props.task.done) return "";
  const day =
    d.plannedDay === props.todayKey ? "Dziś" : dayText(d.plannedDay);
  return d.plannedStart ? `${day} ${d.plannedStart}` : day;
});
/** „Zapisz jako szablon”: nazwa wpisywana w kafelku, reszta z zadania. */
const templateName = ref<string | null>(null);
const templateSaved = ref("");
const templateBusy = ref(false);
const templateInput = ref<HTMLInputElement | null>(null);
async function openTemplateForm() {
  templateSaved.value = "";
  templateName.value = "";
  await nextTick();
  templateInput.value?.focus();
}
async function saveAsTemplate() {
  const name = templateName.value?.trim() ?? "";
  if (!name || templateBusy.value) return;
  templateBusy.value = true;
  emit("error", "");
  try {
    await saveTemplate(null, name, templateFromTask(props.task));
    templateName.value = null;
    templateSaved.value = `Zapisano szablon „${name}”. Użyj go, wpisując /${name} w polu „Nowe zadanie”; zmienisz go w`;
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    templateBusy.value = false;
  }
}
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
        <span class="task-name-status">
          <span class="task-title">{{ task.title }}</span>
          <span
            v-if="
              task.jiraKey ||
              task.details?.project ||
              plannedLabel ||
              (!task.done && (task.details?.waiting || running))
            "
            class="task-meta"
          >
            <span v-if="running" class="task-status task-live">W trakcie</span>
            <span v-if="task.details?.project" class="task-chip">{{
              task.details.project
            }}</span>
            <span
              v-if="plannedLabel"
              class="task-chip task-planned"
              :class="{ today: task.details?.plannedDay === todayKey }"
              :title="'Zaplanowane: ' + task.details?.plannedDay"
              >{{ plannedLabel }}</span
            >
            <span v-if="task.jiraKey" class="task-jira-key mono">{{
              task.jiraKey
            }}</span>
            <span
              v-if="task.jiraKey && task.jiraStatus"
              class="task-jira-status"
              :title="'Status w Jirze: ' + task.jiraStatus"
              >{{ task.jiraStatus }}</span
            >
            <span v-if="task.details?.waiting && !task.done" class="task-status"
              >Czekam na: {{ task.details.waiting }}</span
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
          > </span
        ><span class="task-duration mono" :class="{ zero: !taskTime }">{{
          duration(taskTime)
        }}</span
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
      <div class="details-actions" role="group" aria-label="Akcje zadania">
        <button
          v-if="task.jiraUrl"
          class="action-button"
          :aria-label="'Otwórz w Jirze: ' + task.jiraKey"
          @click="openJira(task.jiraUrl)"
        >
          <ExternalLink :size="14" />{{ task.jiraKey }}
        </button>
        <span class="details-actions-spacer" />
        <button
          class="action-button"
          :aria-label="'Zapisz jako szablon: ' + task.title"
          title="Tytuł, szacunek, kroki listy, projekt i priorytet jako szablon /nazwa"
          :aria-expanded="templateName !== null"
          @click="templateName === null ? openTemplateForm() : (templateName = null)"
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
      <form
        v-if="templateName !== null"
        class="template-save"
        @submit.prevent="saveAsTemplate"
        @keydown.esc.stop="templateName = null"
      >
        <label
          >Nazwa szablonu
          <span class="template-slash">/</span
          ><input
            ref="templateInput"
            v-model="templateName"
            :aria-label="'Nazwa szablonu: ' + task.title"
            placeholder="np. review"
            maxlength="60"
        /></label>
        <button
          class="primary-button"
          :disabled="templateBusy || !templateName.trim()"
        >
          Zapisz szablon
        </button>
        <button type="button" class="action-button" @click="templateName = null">
          Anuluj
        </button>
      </form>
      <p v-if="templateSaved" class="hint template-saved" role="status">
        {{ templateSaved }}
        <button
          type="button"
          class="text-link"
          @click="openSettings('templates')"
        >
          Ustawieniach → Szablony</button
        >.
      </p>
      <h3 class="details-heading">Przedziały pracy</h3>
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
        :show-priority="false"
        @updated="emit('updated', $event)"
      />
    </div>
  </article>
</template>
