<script setup lang="ts">
/**
 * Nowe zgłoszenie w Jirze (J2) z opcjonalnym zadaniem w Todo Track; własne
 * `busy` i błąd. Formularz pokazuje się po pobraniu projektów wybranych
 * tablic; typ z wymaganymi polami spoza formularza odsyła do Jiry.
 */
import { computed, onMounted, ref } from "vue";
import { ExternalLink } from "lucide-vue-next";
import {
  jiraCreate,
  jiraIssueTypeFields,
  jiraIssueTypes,
  jiraOpen,
  jiraProjects,
  type JiraCreated,
  type JiraIssueType,
  type JiraProject,
} from "../../store";

/** Projekt bieżącej tablicy wybierany na start (jeśli jest na liście). */
const props = defineProps<{ preferredProject?: string }>();
const emit = defineEmits<{ created: [created: JiraCreated]; close: [] }>();
const ready = ref(false);
const projects = ref<JiraProject[]>([]);
const types = ref<JiraIssueType[]>([]);
const form = ref({
  project: "",
  issueTypeId: "",
  summary: "",
  description: "",
  assignMe: true,
  createTask: true,
  plannedDay: "",
});
const extraRequired = ref<string[]>([]);
const createUrl = ref("");
const busy = ref(false),
  error = ref("");
async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function loadTypes() {
  types.value = [];
  extraRequired.value = [];
  form.value.issueTypeId = "";
  if (!form.value.project) return;
  types.value = await jiraIssueTypes(form.value.project);
  form.value.issueTypeId = types.value[0]?.id ?? "";
  await loadFields();
}
async function loadFields() {
  extraRequired.value = [];
  if (!form.value.project || !form.value.issueTypeId) return;
  const fields = await jiraIssueTypeFields(
    form.value.project,
    form.value.issueTypeId,
  );
  extraRequired.value = fields.extraRequired;
  createUrl.value = fields.createUrl;
}
const typeName = computed(
  () => types.value.find((t) => t.id === form.value.issueTypeId)?.name ?? "",
);
onMounted(() =>
  run(async () => {
    projects.value = await jiraProjects();
    form.value.project =
      projects.value.find((p) => p.key === props.preferredProject)?.key ??
      projects.value[0]?.key ??
      "";
    ready.value = true;
    await loadTypes();
  }),
);
const submit = () =>
  run(async () => {
    const f = form.value;
    emit(
      "created",
      await jiraCreate({ ...f, plannedDay: f.createTask ? f.plannedDay : "" }),
    );
  });
</script>

<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <form
    v-if="ready"
    class="jira-create"
    aria-label="Nowe zgłoszenie"
    @submit.prevent="submit"
    @keydown.esc.stop="emit('close')"
  >
    <h2>Nowe zgłoszenie</h2>
    <div class="jira-create-fields">
      <label
        >Projekt<select
          v-model="form.project"
          aria-label="Projekt"
          :disabled="busy"
          @change="run(loadTypes)"
        >
          <option v-for="p in projects" :key="p.key" :value="p.key">
            {{ p.key }} ({{ p.boards.join(", ") }})
          </option>
        </select></label
      ><label
        >Typ<select
          v-model="form.issueTypeId"
          aria-label="Typ zgłoszenia"
          :disabled="busy"
          @change="run(loadFields)"
        >
          <option v-for="t in types" :key="t.id" :value="t.id">
            {{ t.name }}
          </option>
        </select></label
      ><label class="wide"
        >Tytuł<input
          v-model="form.summary"
          aria-label="Tytuł zgłoszenia"
          maxlength="255"
          required
          :disabled="busy"
      /></label>
      <label class="wide"
        >Opis (opcjonalnie)<textarea
          v-model="form.description"
          aria-label="Opis zgłoszenia"
          rows="3"
          maxlength="5000"
          :disabled="busy"
      /></label>
    </div>
    <div class="jira-create-options">
      <label class="check-label"
        ><input v-model="form.assignMe" type="checkbox" />Przypisz do
        mnie</label
      ><label class="check-label"
        ><input v-model="form.createTask" type="checkbox" />Utwórz też
        zadanie w Todo Track</label
      ><label v-if="form.createTask" class="check-label"
        >Dzień planu<input
          v-model="form.plannedDay"
          type="date"
          aria-label="Dzień planu zadania"
      /></label>
    </div>
    <p v-if="extraRequired.length" class="jira-move-error" role="alert">
      Typ „{{ typeName }}” wymaga pól, których Todo Track nie wypełnia:
      {{ extraRequired.join(", ") }}.
      <button
        type="button"
        class="text-link"
        @click="run(() => jiraOpen(createUrl))"
      >
        Utwórz w Jirze <ExternalLink :size="12" />
      </button>
    </p>
    <div class="connection-actions">
      <button type="button" class="secondary-button" @click="emit('close')">
        Anuluj</button
      ><button
        class="primary-button"
        :disabled="busy || !!extraRequired.length || !form.summary.trim()"
      >
        Utwórz zgłoszenie
      </button>
    </div>
  </form>
</template>
