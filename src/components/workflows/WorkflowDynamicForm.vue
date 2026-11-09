<script setup lang="ts">
/**
 * Formularz procesu dynamicznego (E8.5): jedyny sposób edycji grafu z orkiestratorem.
 * Każda zmiana buduje graf od nowa przez `buildDynamic`, więc kształt jest zawsze
 * kanoniczny (walidator odrzuca inny). Graf nie jest edytowalny na canvasie.
 */
import { computed } from "vue";
import {
  buildDynamic,
  defaultDynamicSpec,
  ORCHESTRATOR_ID,
  specOf,
  type DynamicSpec,
} from "../../api/workflowDynamic";
import {
  MAX_SUBAGENTS_HINT,
  MIN_SUBAGENTS_HINT,
  type AccessProfile,
  type WorkflowGraph,
  type WorkflowIssue,
} from "../../api/workflows";

const props = defineProps<{ graph: WorkflowGraph; issues: WorkflowIssue[] }>();
const emit = defineEmits<{ change: [graph: WorkflowGraph] }>();

const spec = computed<DynamicSpec>(() => specOf(props.graph) ?? defaultDynamicSpec());
const readOnly = computed(() => spec.value.access === "readOnly");
const minutes = computed(() => Math.round(spec.value.timeoutSecs / 60));
const problems = computed(() =>
  props.issues.filter((i) => i.nodeId === ORCHESTRATOR_ID || !i.nodeId),
);

function update(patch: Partial<DynamicSpec>) {
  emit("change", buildDynamic({ ...spec.value, ...patch }));
}

function setAccess(access: AccessProfile) {
  // Review wymaga zapisu: przy samym odczycie znika razem z poprawkami.
  update(access === "readOnly" ? { access, review: false } : { access });
}

/**
 * Liczba z pola albo `null`, gdy pole jest puste lub nie zawiera liczby. Wtedy pole
 * wraca do bieżącej wartości, a graf się nie zmienia (NaN zapisałby się jako `null`,
 * który Rust przyjmuje jako brak limitu, a walidator TS odrzuca).
 */
function number(event: Event, current: number): number | null {
  const input = event.target as HTMLInputElement;
  const value = Number(input.value);
  if (input.value.trim() === "" || !Number.isFinite(value)) {
    input.value = String(current);
    return null;
  }
  return value;
}

function setMinutes(event: Event) {
  const value = number(event, minutes.value);
  if (value !== null) update({ timeoutSecs: value * 60 });
}

function setSubagents(event: Event) {
  const value = number(event, spec.value.maxSubagents);
  if (value !== null) update({ maxSubagents: value });
}
</script>

<template>
  <form class="wf-dynamic settings-fields" aria-label="Proces dynamiczny" @submit.prevent>
    <p class="hint">
      Orkiestrator Claude sam deleguje pracę do subagentów. Układ kroków jest stały; zmieniasz
      tylko poniższe pola.
    </p>
    <label>
      Instrukcja orkiestratora
      <textarea
        :value="spec.instruction"
        rows="6"
        @change="update({ instruction: ($event.target as HTMLTextAreaElement).value })"
      />
    </label>
    <label>
      Model (puste: wybór przy uruchomieniu)
      <input
        :value="spec.model ?? ''"
        @change="update({ model: ($event.target as HTMLInputElement).value.trim() || null })"
      />
    </label>
    <label>
      Limit czasu (minuty)
      <input
        type="number"
        min="1"
        max="240"
        :value="minutes"
        @change="setMinutes"
      />
    </label>
    <fieldset>
      <legend>Dostęp</legend>
      <label class="inline-check">
        <input
          type="radio"
          name="dynamic-access"
          :checked="readOnly"
          @change="setAccess('readOnly')"
        />
        Tylko odczyt
      </label>
      <label class="inline-check">
        <input
          type="radio"
          name="dynamic-access"
          :checked="!readOnly"
          @change="setAccess('workspaceWrite')"
        />
        Odczyt i zapis w katalogu roboczym
      </label>
    </fieldset>
    <label>
      Liczba subagentów (wskazówka {{ MIN_SUBAGENTS_HINT }}–{{ MAX_SUBAGENTS_HINT }})
      <input
        type="number"
        :min="MIN_SUBAGENTS_HINT"
        :max="MAX_SUBAGENTS_HINT"
        :value="spec.maxSubagents"
        @change="setSubagents"
      />
    </label>
    <label class="inline-check">
      <input
        type="checkbox"
        :checked="spec.review"
        :disabled="readOnly"
        aria-describedby="wf-dynamic-review-hint"
        @change="update({ review: ($event.target as HTMLInputElement).checked })"
      />
      Dodaj review i poprawki
    </label>
    <p id="wf-dynamic-review-hint" class="hint">
      {{
        readOnly
          ? "Review i poprawki wymagają dostępu z zapisem."
          : "Po orkiestratorze Claude robi review i poprawia uwagi (do trzech rund)."
      }}
    </p>
    <ul v-if="problems.length" class="error" role="alert">
      <li v-for="(issue, i) in problems" :key="i">{{ issue.message }}</li>
    </ul>
  </form>
</template>
