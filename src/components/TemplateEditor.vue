<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  deleteTemplate,
  saveTemplate,
  type TaskTemplate,
} from "../store";

/** Edycja jednego szablonu w Ustawieniach → Szablony (`template` null = nowy). */
const props = defineProps<{ template: TaskTemplate | null }>();
const emit = defineEmits<{ done: [] }>();

const blank = () => ({
  name: props.template?.name ?? "",
  title: props.template?.title ?? "",
  estimate: props.template?.estimateMinutes
    ? String(props.template.estimateMinutes)
    : "",
  project: props.template?.project ?? "",
  priority: props.template?.priority ?? 0,
  steps: (props.template?.steps ?? []).join("\n"),
});
const draft = ref(blank());
watch(
  () => JSON.stringify(props.template),
  () => (draft.value = blank()),
);
const busy = ref(false),
  error = ref(""),
  saved = ref(""),
  confirming = ref(false);
/** Etykieta pól: „review” albo „nowego szablonu”. */
const who = computed(() =>
  props.template ? props.template.name : "nowego szablonu",
);

async function save() {
  busy.value = true;
  error.value = "";
  saved.value = "";
  try {
    // v-model na <input type="number"> zamienia wpisaną wartość na liczbę.
    const estimate = String(draft.value.estimate ?? "").trim();
    const minutes = estimate === "" ? 0 : Number(estimate);
    if (!Number.isInteger(minutes) || minutes < 0)
      throw new Error("Szacunek podaj w pełnych minutach.");
    await saveTemplate(props.template?.id ?? null, draft.value.name, {
      title: draft.value.title,
      estimateMinutes: minutes,
      steps: draft.value.steps.split("\n"),
      project: draft.value.project,
      priority: Number(draft.value.priority),
    });
    if (props.template) saved.value = "Zapisano.";
    else emit("done");
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function remove() {
  if (!props.template) return;
  busy.value = true;
  error.value = "";
  try {
    await deleteTemplate(props.template.id);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    busy.value = false;
  }
}
</script>

<template>
  <form
    class="template-editor"
    :aria-label="template ? 'Szablon ' + template.name : 'Nowy szablon'"
    @submit.prevent="save"
  >
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <div class="planning-fields settings-fields">
      <label
        >Nazwa (/nazwa)<input
          v-model="draft.name"
          :aria-label="'Nazwa: ' + who"
          maxlength="60"
          placeholder="np. review"
          :disabled="busy"
      /></label>
      <label class="template-title"
        >Tytuł<input
          v-model="draft.title"
          :aria-label="'Tytuł: ' + who"
          maxlength="240"
          placeholder="np. Code review {klucz}: {tytuł}"
          :disabled="busy"
      /></label>
      <label
        >Szacunek (minuty)<input
          v-model="draft.estimate"
          :aria-label="'Szacunek: ' + who"
          type="number"
          min="0"
          step="5"
          :disabled="busy"
      /></label>
      <label
        >Projekt<input
          v-model="draft.project"
          :aria-label="'Projekt: ' + who"
          maxlength="80"
          :disabled="busy"
      /></label>
      <label
        >Priorytet<select
          v-model.number="draft.priority"
          :aria-label="'Priorytet: ' + who"
          :disabled="busy"
        >
          <option :value="1">↑ Wysoki</option>
          <option :value="0">Normalny</option>
          <option :value="-1">↓ Niski</option>
        </select></label
      >
    </div>
    <label class="template-steps"
      >Kroki (jeden w wierszu)<textarea
        v-model="draft.steps"
        :aria-label="'Kroki: ' + who"
        rows="3"
        :disabled="busy"
      ></textarea>
    </label>
    <div class="template-actions">
      <button class="primary-button" :disabled="busy || !draft.name.trim()">
        {{ template ? "Zapisz szablon" : "Dodaj szablon" }}
      </button>
      <button
        v-if="!template"
        type="button"
        class="action-button"
        :disabled="busy"
        @click="emit('done')"
      >
        Anuluj
      </button>
      <template v-else-if="!confirming">
        <button
          type="button"
          class="action-button danger"
          :aria-label="'Usuń szablon ' + template.name"
          :disabled="busy"
          @click="confirming = true"
        >
          Usuń
        </button>
      </template>
      <template v-else>
        <span>Usunąć szablon /{{ template.name }}?</span>
        <button
          type="button"
          class="action-button danger"
          :disabled="busy"
          @click="remove"
        >
          Tak, usuń
        </button>
        <button
          type="button"
          class="action-button"
          :disabled="busy"
          @click="confirming = false"
        >
          Nie
        </button>
      </template>
      <span v-if="saved" class="hint" role="status">{{ saved }}</span>
    </div>
  </form>
</template>
