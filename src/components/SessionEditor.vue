<script setup lang="ts">
import { ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import { editSession, type Session, type Snapshot } from "../store";
import { localInput } from "../time";

const props = defineProps<{ session: Session }>();
const emit = defineEmits<{ updated: [data: Snapshot]; close: [] }>();
const busy = ref(false);
const error = ref("");
const editStart = ref(localInput(props.session.startedAt));
const editEnd = ref(localInput(props.session.endedAt!));

function close() {
  if (!busy.value) emit("close");
}
async function save() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    const s = props.session;
    // Preserve milliseconds and the exact offset of unchanged timestamps (including DST folds).
    const start =
      editStart.value === localInput(s.startedAt)
        ? s.startedAt
        : new Date(editStart.value).getTime();
    const end =
      editEnd.value === localInput(s.endedAt!)
        ? s.endedAt!
        : new Date(editEnd.value).getTime();
    if (!Number.isFinite(start) || !Number.isFinite(end))
      throw new Error("Wprowadź poprawną datę i godzinę.");
    emit("updated", await editSession(s.id, start, end));
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    title="Edytuj przedział"
    :error="error"
    :busy="busy"
    @close="close"
  >
    <form @submit.prevent="save">
      <label for="session-start">Początek</label
      ><input
        id="session-start"
        v-model="editStart"
        type="datetime-local"
        step="1"
        required
        :disabled="busy"
      /><label for="session-end">Koniec</label
      ><input
        id="session-end"
        v-model="editEnd"
        type="datetime-local"
        step="1"
        required
        :disabled="busy"
      />
      <p class="hint">
        Czas lokalny. Zmiana przeliczy czas zadania i raporty dzienne.
      </p>
      <div class="modal-actions">
        <button type="button" :disabled="busy" @click="close">
          Anuluj</button
        ><button class="primary-button" :disabled="busy">
          Zapisz przedział
        </button>
      </div>
    </form>
  </ConfirmDialog>
</template>
