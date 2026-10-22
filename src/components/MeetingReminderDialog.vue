<script setup lang="ts">
/**
 * Przypomnienie o spotkaniu z Outlooka (C3): spotkania z kalendarza są tylko do
 * odczytu, więc okno zmienia wyłącznie nadpisanie przypomnienia tego spotkania
 * (`meeting_reminder_set`). Ręczne spotkania mają to pole w `MeetingEditor`.
 */
import { onMounted, ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import MeetingReminderField from "./MeetingReminderField.vue";
import {
  meetingReminderList,
  meetingReminderSet,
  type MeetingReminderValue,
} from "../remindersapi";
import { meetingTitle, type CalendarEvent } from "../store";
import { clock } from "../time";
import { useSettings } from "../target";

const props = defineProps<{ meeting: CalendarEvent }>();
const emit = defineEmits<{ close: []; saved: [] }>();
const settings = useSettings();
const value = ref<MeetingReminderValue>("default");
const initial = ref<MeetingReminderValue>("default");
/** Wczytywanie nadpisania: blokuje pole i „Zapisz”, ale nie „Anuluj”, żeby
 * fokus okna (useModal) i Esc działały od pierwszej chwili. */
const loading = ref(true);
const busy = ref(false);
const error = ref("");
onMounted(async () => {
  try {
    const found = (await meetingReminderList()).find(
      (o) => o.meetingId === props.meeting.id,
    );
    initial.value = value.value = found ? found.minutes : "default";
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
});
async function save() {
  if (busy.value || loading.value) return;
  busy.value = true;
  error.value = "";
  try {
    if (value.value !== initial.value)
      await meetingReminderSet(props.meeting.id, value.value);
    emit("saved");
  } catch (e) {
    error.value = String(e);
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    title="Przypomnienie o spotkaniu"
    :error="error"
    :busy="busy"
    @close="emit('close')"
  >
    <form class="meeting-form" @submit.prevent="save">
      <p class="meeting-reminder-subject">
        <strong>{{
          meetingTitle(meeting, settings.calendarShowTitles)
        }}</strong>
        <span class="mono"
          >{{ clock(meeting.start) }}–{{ clock(meeting.end) }}</span
        >
      </p>
      <label for="meeting-reminder">Przypomnienie</label>
      <MeetingReminderField v-model="value" :disabled="busy || loading" />
      <p v-if="!settings.meetingReminderEnabled" class="hint">
        Przypomnienia o spotkaniach są wyłączone w Ustawienia → Powiadomienia;
        po ich włączeniu obowiązuje to, co wybierzesz tutaj.
      </p>
      <p class="hint">
        Spotkania z Outlooka są tylko do odczytu; zmieniasz tylko to, czy i
        kiedy aplikacja o nim przypomni.
      </p>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="emit('close')">
          Anuluj</button
        ><button class="primary-button" :disabled="busy || loading">
          Zapisz
        </button>
      </div>
    </form>
  </ConfirmDialog>
</template>
