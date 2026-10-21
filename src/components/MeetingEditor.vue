<script setup lang="ts">
/**
 * Ręczne spotkanie (B12): dodanie albo edycja spotkania spoza Outlooka.
 * Temat, dzień, od–do (domyślnie najbliższa pełna godzina, 30 min) albo
 * „Cały dzień”, typ i opcjonalne zadanie. Przy edycji „Usuń” z potwierdzeniem.
 * Spotkania z Outlooka są tylko do odczytu i tu nie trafiają.
 */
import { computed, ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import SearchSelect from "./SearchSelect.vue";
import {
  MEETING_SUBJECT_MAX,
  meetingAdd,
  meetingDelete,
  meetingTypes,
  meetingUpdate,
  type CalendarEvent,
  type Task,
} from "../store";
import { clock, dayKey } from "../time";

const props = defineProps<{
  /** Edytowane ręczne spotkanie; brak = nowe. */
  meeting?: CalendarEvent | null;
  /** Dzień nowego spotkania (`RRRR-MM-DD`). */
  day: string;
  /** Początek nowego spotkania, np. z kliknięcia w tor „Spotkania”. */
  start?: number | null;
  tasks: Task[];
  now: number;
}>();
const emit = defineEmits<{ close: []; saved: [meeting: CalendarEvent] }>();

const HALF_HOUR = 30 * 60_000;
/** Najbliższa pełna godzina od teraz (najpóźniej 23:00) w dniu `day`. */
function defaultStart(day: string) {
  const at = new Date(props.now);
  const hour = Math.min(23, at.getHours() + (at.getMinutes() || at.getSeconds() ? 1 : 0));
  return +new Date(`${day}T${String(hour).padStart(2, "0")}:00`);
}
const editing = props.meeting ?? null;
const initialStart = editing?.start ?? props.start ?? defaultStart(props.day);
const initialEnd = editing
  ? editing.end
  : Math.min(initialStart + HALF_HOUR, +new Date(`${dayKey(initialStart)}T23:59`));

const subject = ref(editing?.subject ?? "");
const day = ref(dayKey(initialStart));
const from = ref(clock(initialStart));
const to = ref(editing?.allDay ? clock(initialStart + HALF_HOUR) : clock(initialEnd));
const allDay = ref(editing?.allDay ?? false);
const showAs = ref(editing?.showAs ?? "busy");
const taskId = ref<number>(editing?.taskId ?? 0);
const busy = ref(false);
const error = ref("");
const confirming = ref(false);

const taskOptions = computed(() => [
  { value: 0, label: "Bez zadania" },
  ...props.tasks
    .filter((t) => (!t.archived && !t.done) || t.id === taskId.value)
    .map((t) => ({ value: t.id, label: t.title })),
]);

function close() {
  if (!busy.value) emit("close");
}
async function save() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const start = +new Date(`${day.value}T${allDay.value ? "00:00" : from.value}`);
    const end = allDay.value ? start : +new Date(`${day.value}T${to.value}`);
    if (!Number.isFinite(start) || !Number.isFinite(end))
      throw new Error("Wprowadź poprawny dzień i godziny.");
    const input = {
      start,
      end,
      subject: subject.value,
      showAs: showAs.value,
      allDay: allDay.value,
      taskId: taskId.value || null,
    };
    emit(
      "saved",
      editing ? await meetingUpdate(editing.id, input) : await meetingAdd(input),
    );
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
async function remove() {
  if (!editing || busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await meetingDelete(editing.id);
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    v-if="confirming"
    title="Usunąć spotkanie?"
    :error="error"
    :busy="busy"
    confirm-label="Usuń"
    @close="confirming = false"
    @confirm="remove"
  >
    <p>„{{ subject }}” zniknie z osi dnia i pojemności.</p>
  </ConfirmDialog>
  <ConfirmDialog
    v-else
    :title="editing ? 'Edytuj spotkanie' : 'Dodaj spotkanie'"
    :error="error"
    :busy="busy"
    @close="close"
  >
    <form class="meeting-form" @submit.prevent="save">
      <label for="meeting-subject">Temat</label
      ><input
        id="meeting-subject"
        v-model="subject"
        type="text"
        :maxlength="MEETING_SUBJECT_MAX"
        required
        :disabled="busy"
        placeholder="np. Telefon z klientem"
      />
      <label for="meeting-day">Dzień</label
      ><input
        id="meeting-day"
        v-model="day"
        type="date"
        required
        :disabled="busy"
      />
      <label class="meeting-allday"
        ><input v-model="allDay" type="checkbox" :disabled="busy" />Cały
        dzień</label
      >
      <div v-if="!allDay" class="meeting-hours">
        <div>
          <label for="meeting-from">Od</label
          ><input
            id="meeting-from"
            v-model="from"
            type="time"
            required
            :disabled="busy"
          />
        </div>
        <div>
          <label for="meeting-to">Do</label
          ><input
            id="meeting-to"
            v-model="to"
            type="time"
            required
            :disabled="busy"
          />
        </div>
      </div>
      <label for="meeting-type">Typ</label
      ><select
        id="meeting-type"
        v-model="showAs"
        class="meeting-type"
        :disabled="busy"
      >
        <option v-for="t in meetingTypes" :key="t.value" :value="t.value">
          {{ t.label }}
        </option>
      </select>
      <label for="meeting-task">Zadanie (opcjonalnie)</label
      ><SearchSelect
        v-model="taskId"
        input-id="meeting-task"
        label="Zadanie spotkania"
        placeholder="Bez zadania"
        :disabled="busy"
        :options="taskOptions"
      />
      <p class="hint">
        Ręczne spotkanie zmniejsza pojemność dnia, a plan dnia je omija. Zadanie
        podpowie się w „Dodaj jako czas” w Zamknięciu dnia.
      </p>
      <div class="modal-actions">
        <button
          v-if="editing"
          type="button"
          class="delete-button meeting-delete"
          :disabled="busy"
          @click="confirming = true"
        >
          Usuń</button
        ><button type="button" :disabled="busy" @click="close">Anuluj</button
        ><button class="primary-button" :disabled="busy">Zapisz</button>
      </div>
    </form>
  </ConfirmDialog>
</template>
