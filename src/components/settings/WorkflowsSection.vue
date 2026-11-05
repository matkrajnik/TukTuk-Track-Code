<script setup lang="ts">
/**
 * Ustawienia → Workflows: odblokowanie modułu kluczem dostępu
 * (docs/workflows/PLAN_WDROZENIA.md, 4.1.1). Bez klucza działa całe
 * dotychczasowe śledzenie; poprawny klucz trwale dodaje sekcję Workflows.
 */
import { nextTick, onMounted, ref } from "vue";
import { openView } from "../../navigation";
import {
  loadWorkflowAccess,
  unlockWithKey,
  workflowsUnlocked,
} from "../../workflowAccess";
import { message } from "./section";

const key = ref("");
const busy = ref(false);
const error = ref("");
/** Komunikat sukcesu tylko po odblokowaniu w tej sesji (jeden dla czytnika). */
const justUnlocked = ref(false);
const openButton = ref<HTMLButtonElement | null>(null);

onMounted(() => {
  void loadWorkflowAccess();
});

async function unlock() {
  if (busy.value || !key.value.trim()) return;
  busy.value = true;
  error.value = "";
  try {
    await unlockWithKey(key.value);
    key.value = "";
    justUnlocked.value = true;
    // Przycisk wysyłki znika razem z formularzem: fokus na następnej akcji.
    await nextTick();
    openButton.value?.focus();
  } catch (e) {
    error.value = message(e);
  } finally {
    busy.value = false;
  }
}

function open() {
  openView({ view: "workflows" });
}
</script>

<template>
  <h2>Workflows</h2>
  <p class="hint">
    Rozszerzenie TukTuka o procesy agentów: analiza, implementacja i review
    zadania w kolejnych krokach, z decyzją użytkownika na końcu. Śledzenie
    pracy działa tak samo z kluczem i bez niego.
  </p>
  <!-- Stały region statusu: czytnik ogłasza wpisany do niego komunikat sukcesu. -->
  <p class="workflows-unlocked" role="status">
    <template v-if="justUnlocked">Workflows odblokowane.</template>
    <template v-else-if="workflowsUnlocked"
      >Workflows są odblokowane na tym komputerze.</template
    >
  </p>
  <button
    v-if="workflowsUnlocked"
    ref="openButton"
    class="primary-button"
    @click="open"
  >
    Otwórz Workflows
  </button>
  <form v-else class="settings-fields workflows-unlock" @submit.prevent="unlock">
    <label>
      Klucz dostępu
      <input
        v-model="key"
        type="password"
        autocomplete="off"
        spellcheck="false"
        :disabled="busy"
        :aria-invalid="error ? 'true' : undefined"
        :aria-describedby="error ? 'workflows-key-error' : undefined"
      />
    </label>
    <p v-if="error" id="workflows-key-error" class="error" role="alert">
      {{ error }}
    </p>
    <button
      class="primary-button"
      type="submit"
      :disabled="busy || !key.trim()"
      :aria-busy="busy"
    >
      {{ busy ? "Sprawdzanie…" : "Odblokuj Workflows" }}
    </button>
  </form>
</template>

<style scoped>
.workflows-unlocked {
  color: var(--text);
  font-weight: 600;
}
.workflows-unlock {
  align-items: flex-start;
}
</style>
