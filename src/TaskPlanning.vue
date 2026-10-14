<script setup lang="ts">
import { ref, watch } from "vue";
import { detailsOf, saveDetails, type Task, type Snapshot } from "./store";
import { dayKey } from "./time";
const props = defineProps<{ task: Task; projects: string[] }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const draft = ref(detailsOf(props.task));
const noteDay = ref(dayKey(Date.now()));
const message = ref("");
const busy = ref(false);
const dirty = ref(false);
const newCheck = ref("");
function addCheck() {
  if (newCheck.value.trim() && draft.value.checklist.length < 100) {
    draft.value.checklist.push({ text: newCheck.value.trim(), done: false });
    newCheck.value = "";
    dirty.value = true;
  }
}
const weekdays = ["Pon", "Wt", "Śr", "Czw", "Pt", "Sob", "Niedz"];
watch(
  () => props.task.details?.priority,
  (value) => {
    draft.value.priority = value ?? 0;
  },
);
watch(
  () => props.task.details,
  () => {
    if (!dirty.value) draft.value = structuredClone(detailsOf(props.task));
  },
);
async function save() {
  busy.value = true;
  message.value = "";
  try {
    const result = await saveDetails(props.task.id, draft.value);
    dirty.value = false;
    draft.value = structuredClone(
      detailsOf(result.tasks.find((t) => t.id === props.task.id)!),
    );
    emit("updated", result);
    message.value = "Zapisano plan i notatki.";
  } catch (e) {
    message.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <form
    class="planning-form"
    @submit.prevent="save"
    @input="
      dirty = true;
      message = '';
    "
    @change="dirty = true"
  >
    <h3>Plan i notatki</h3>
    <div class="planning-fields">
      <label
        >Priorytet<select
          v-model.number="draft.priority"
          aria-label="Priorytet"
        >
          <option :value="1">Wysoki</option>
          <option :value="0">Normalny</option>
          <option :value="-1">Niski</option>
        </select></label
      >
      <label
        >Planowany dzień<input v-model="draft.plannedDay" type="date"
      /></label>
      <label>Termin<input v-model="draft.dueDay" type="date" /></label>
      <label
        >Projekt / obszar<input
          v-model="draft.project"
          :list="'projects-' + task.id"
          maxlength="80"
          placeholder="np. WMS"
      /></label>
    </div>
    <datalist :id="'projects-' + task.id">
      <option v-for="project in projects" :key="project" :value="project" />
    </datalist>
    <div class="planning-fields">
      <label
        >Szacowany czas (minuty)<input
          v-model.number="draft.estimateMinutes"
          type="number"
          min="0"
          max="600000"
          step="1"
      /></label>
      <label
        >Początek cyklu<input v-model="draft.repeatFrom" type="date"
      /></label>
    </div>
    <fieldset class="repeat-options">
      <legend>Powtarzaj w wybrane dni</legend>
      <label v-for="(name, index) in weekdays" :key="name" class="inline-check"
        ><input
          v-model="draft.repeatDays"
          type="checkbox"
          :value="index + 1"
        />{{ name }}</label
      >
    </fieldset>
    <p v-if="draft.repeatDays.length" class="hint">
      Każda data tworzy osobne zadanie. Kopie mają wyzerowaną checklistę. Po
      przerwie uzupełniane są maksymalnie ostatnie 366 dni. Odznacz dni, aby
      wyłączyć cykl; archiwizacja też go wstrzymuje.
    </p>
    <div class="task-checklist">
      <h3>Lista kroków</h3>
      <div
        v-for="(item, index) in draft.checklist"
        :key="index"
        class="checklist-row"
      >
        <input
          v-model="item.done"
          type="checkbox"
          :aria-label="'Gotowe: ' + item.text"
        /><input
          v-model="item.text"
          :aria-label="'Krok ' + (index + 1)"
          maxlength="240"
          required
        /><button
          type="button"
          class="today-button"
          :aria-label="'Usuń krok ' + (index + 1)"
          @click="
            draft.checklist.splice(index, 1);
            dirty = true;
          "
        >
          Usuń
        </button>
      </div>
      <div class="planning-actions">
        <input
          v-model="newCheck"
          aria-label="Nowy krok"
          maxlength="240"
          placeholder="Dodaj krok…"
          @keydown.enter.prevent="addCheck"
        /><button
          type="button"
          class="today-button"
          :disabled="!newCheck.trim() || draft.checklist.length >= 100"
          @click="addCheck"
        >
          Dodaj krok
        </button>
      </div>
    </div>
    <label
      >Czekam na…<input
        v-model="draft.waiting"
        maxlength="300"
        placeholder="Puste pole oznacza brak blokady"
    /></label>
    <p v-if="draft.waiting" class="hint">
      Zapisanie blokady zatrzyma aktywny pomiar tego zadania. Start usunie
      blokadę.
    </p>
    <div class="note-heading">
      <label
        >Dzień notatki<input v-model="noteDay" type="date" required
      /></label>
    </div>
    <label
      >Co zrobiłem<textarea
        v-model="draft.notes[noteDay]"
        maxlength="4000"
        rows="3"
        placeholder="Krótki opis wykonanej pracy w wybranym dniu"
      />
    </label>
    <div class="planning-actions">
      <button class="primary-button" :disabled="busy">
        Zapisz plan i notatki</button
      ><span role="status">{{ message }}</span>
    </div>
  </form>
</template>
