<script setup lang="ts">
/**
 * Formularz przypomnienia (C3): treść, dzień, godzina (domyślnie za godzinę,
 * zaokrąglona do 5 minut), powtarzanie (Jednorazowo / Codziennie / W dni
 * robocze / Co tydzień + dni tygodnia) i opcjonalne zadanie. Używają go
 * Ustawienia → Powiadomienia („Moje przypomnienia”) i „Przypomnij…” z kafelka
 * zadania (zadanie przypięte).
 */
import { computed, onMounted, ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import SearchSelect from "./SearchSelect.vue";
import {
  REMINDER_TEXT_MAX,
  defaultReminderAt,
  parseAt,
  reminderAdd,
  reminderRepeats,
  reminderUpdate,
  reminderWeekdays,
  type Reminder,
  type ReminderRepeat,
} from "../remindersapi";
import { read, type Task } from "../store";

const props = defineProps<{
  /** Edytowane przypomnienie; brak = nowe. */
  reminder?: Reminder | null;
  /** Zadanie przypięte do nowego przypomnienia („Przypomnij…” z kafelka). */
  task?: { id: number; title: string } | null;
}>();
const emit = defineEmits<{ close: []; saved: [reminder: Reminder] }>();

const editing = props.reminder ?? null;
const initial = parseAt(editing?.at ?? defaultReminderAt(Date.now()))!;
const text = ref(editing?.text ?? "");
const day = ref(initial.day);
const time = ref(initial.time);
const repeat = ref<ReminderRepeat>(editing?.repeat ?? "none");
const weekdays = ref<number[]>(
  editing?.weekdays.length ? [...editing.weekdays] : [],
);
const taskId = ref<number>(editing?.taskId ?? props.task?.id ?? 0);
const busy = ref(false);
const error = ref("");

/** Zadania do wyboru: wczytane po otwarciu, a do tego czasu tylko przypięte. */
const tasks = ref<Task[]>([]);
onMounted(async () => {
  try {
    tasks.value = (await read()).tasks;
  } catch {
    // Zostaje samo przypięte zadanie.
  }
});
const taskOptions = computed(() => {
  const known = tasks.value
    .filter((t) => (!t.archived && !t.done) || t.id === taskId.value)
    .map((t) => ({ value: t.id, label: t.title }));
  const pinned = props.task ?? (editing?.taskId ? { id: editing.taskId, title: editing.taskTitle ?? "" } : null);
  if (pinned && pinned.title && !known.some((o) => o.value === pinned.id))
    known.unshift({ value: pinned.id, label: pinned.title });
  return [{ value: 0, label: "Bez zadania" }, ...known];
});

function toggleDay(value: number, on: boolean) {
  weekdays.value = on
    ? [...new Set([...weekdays.value, value])].sort((a, b) => a - b)
    : weekdays.value.filter((d) => d !== value);
}
function close() {
  if (!busy.value) emit("close");
}
async function save() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const input = {
      text: text.value,
      at: `${day.value}T${time.value}`,
      repeat: repeat.value,
      weekdays: repeat.value === "weekly" ? weekdays.value : [],
      taskId: taskId.value || null,
      // Edycja wykonanego jednorazowego przypomnienia włącza je ponownie.
      enabled: editing ? editing.enabled || editing.repeat === "none" : true,
    };
    emit(
      "saved",
      editing
        ? await reminderUpdate(editing.id, input)
        : await reminderAdd(input),
    );
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    :title="editing ? 'Edytuj przypomnienie' : 'Dodaj przypomnienie'"
    :error="error"
    :busy="busy"
    @close="close"
  >
    <form class="meeting-form reminder-form" @submit.prevent="save">
      <label for="reminder-text">Treść</label
      ><input
        id="reminder-text"
        v-model="text"
        type="text"
        :maxlength="REMINDER_TEXT_MAX"
        required
        :disabled="busy"
        placeholder="np. Zadzwonić do klienta"
      />
      <div class="meeting-hours">
        <div>
          <label for="reminder-day">Dzień</label
          ><input
            id="reminder-day"
            v-model="day"
            type="date"
            required
            :disabled="busy"
          />
        </div>
        <div>
          <label for="reminder-time">Godzina</label
          ><input
            id="reminder-time"
            v-model="time"
            type="time"
            required
            :disabled="busy"
          />
        </div>
      </div>
      <label for="reminder-repeat">Powtarzanie</label
      ><select
        id="reminder-repeat"
        v-model="repeat"
        class="meeting-type"
        :disabled="busy"
      >
        <option v-for="r in reminderRepeats" :key="r.value" :value="r.value">
          {{ r.label }}
        </option>
      </select>
      <fieldset v-if="repeat === 'weekly'" class="reminder-weekdays">
        <legend>Dni tygodnia</legend>
        <label
          v-for="d in reminderWeekdays"
          :key="d.value"
          class="reminder-weekday"
          :title="d.name"
          ><input
            type="checkbox"
            :aria-label="d.name"
            :checked="weekdays.includes(d.value)"
            :disabled="busy"
            @change="toggleDay(d.value, ($event.target as HTMLInputElement).checked)"
          />{{ d.short }}</label
        >
      </fieldset>
      <label for="reminder-task">Zadanie (opcjonalnie)</label
      ><SearchSelect
        v-model="taskId"
        input-id="reminder-task"
        label="Zadanie przypomnienia"
        placeholder="Bez zadania"
        :disabled="busy"
        :options="taskOptions"
      />
      <p class="hint">
        Przypomnienie pokaże powiadomienie z przyciskami „Odłóż 10 min” i
        „Pokaż”. Nigdy nie uruchamia pomiaru czasu.
      </p>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="close">Anuluj</button
        ><button class="primary-button" :disabled="busy">Zapisz</button>
      </div>
    </form>
  </ConfirmDialog>
</template>
