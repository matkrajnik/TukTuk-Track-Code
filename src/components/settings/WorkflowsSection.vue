<script setup lang="ts">
/**
 * Ustawienia → Workflows: odblokowanie modułu kluczem dostępu
 * (docs/workflows/PLAN_WDROZENIA.md, 4.1.1). Bez klucza działa całe
 * dotychczasowe śledzenie; poprawny klucz trwale dodaje sekcję Workflows.
 */
import { nextTick, onMounted, ref } from "vue";
import { openView } from "../../navigation";
import ConfirmDialog from "../ConfirmDialog.vue";
import {
  loadWorkflowAccess,
  lockModule,
  unlockWithKey,
  workflowsUnlocked,
} from "../../workflowAccess";
import { listWorkflowRuns } from "../../workflowRuns";
import type { AppSettings } from "../../settings";
import { message, number, useSettingsSection } from "./section";

const settings = defineModel<AppSettings>("settings", { required: true });
const {
  busy: savingRetention,
  error: retentionError,
  save,
} = useSettingsSection(settings);

const key = ref("");
const busy = ref(false);
const error = ref("");
/** Komunikat sukcesu tylko po odblokowaniu w tej sesji (jeden dla czytnika). */
const justUnlocked = ref(false);
/** Komunikat po wyłączeniu w tej sesji (ten sam region statusu). */
const justLocked = ref(false);
const keyInput = ref<HTMLInputElement | null>(null);
const openButton = ref<HTMLButtonElement | null>(null);
const lockButton = ref<HTMLButtonElement | null>(null);
const confirmingLock = ref(false);
const lockBusy = ref(false);
const lockError = ref("");
/**
 * Przebiegi czekające na zgodę nie blokują wyłączenia, ale decyzja wymaga klucza:
 * okno ostrzega, że po wyłączeniu zostaje tylko ich zatrzymanie (bez klucza).
 */
const waitingApprovals = ref(0);

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
    justLocked.value = false;
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

async function askLock() {
  lockError.value = "";
  waitingApprovals.value = 0;
  confirmingLock.value = true;
  try {
    const waiting = await listWorkflowRuns({ status: ["waitingForApproval"] });
    waitingApprovals.value = waiting.length;
  } catch {
    // Brak listy nie blokuje wyłączenia: ostrzeżenie jest tylko pomocą,
    // a reguły (`workflow_lock`) i tak pilnuje Rust.
    waitingApprovals.value = 0;
  }
}

async function closeLockDialog() {
  confirmingLock.value = false;
  await nextTick();
  lockButton.value?.focus();
}

async function lock() {
  if (lockBusy.value) return;
  lockBusy.value = true;
  lockError.value = "";
  try {
    await lockModule();
    confirmingLock.value = false;
    justUnlocked.value = false;
    justLocked.value = true;
    // Przycisk wyłączenia znika razem z ustawieniami: fokus na polu klucza.
    await nextTick();
    keyInput.value?.focus();
  } catch (e) {
    // Odmowa (np. przebiegi w toku): moduł zostaje odblokowany, okno z błędem.
    lockError.value = message(e);
  } finally {
    lockBusy.value = false;
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
    <template v-else-if="justLocked && !workflowsUnlocked"
      >Workflows wyłączone. Historia i definicje zostają.</template
    >
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
  <div v-if="workflowsUnlocked" class="settings-fields">
    <p v-if="retentionError" class="error" role="alert">{{ retentionError }}</p>
    <label
      >Przechowywanie diffów zakończonych przebiegów (dni)<input
        :value="settings.workflowRetentionDays"
        type="number"
        min="7"
        max="365"
        :disabled="savingRetention"
        aria-describedby="workflows-retention-hint"
        @change="save({ workflowRetentionDays: number($event) })"
    /></label>
    <p id="workflows-retention-hint" class="hint">
      Starsze diffy usuwa aplikacja przy starcie; historia przebiegów i zgód
      zostaje.
    </p>
    <button ref="lockButton" type="button" @click="askLock">
      Wyłącz Workflows
    </button>
  </div>
  <form v-else class="settings-fields workflows-unlock" @submit.prevent="unlock">
    <label>
      Klucz dostępu
      <input
        ref="keyInput"
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
  <!-- Poza łańcuchem v-if/v-else powyżej: formularz klucza zostaje v-else bloku ustawień. -->
  <ConfirmDialog
    v-if="confirmingLock"
    title="Wyłączyć Workflows?"
    :error="lockError"
    :busy="lockBusy"
    confirm-label="Wyłącz Workflows"
    @close="closeLockDialog"
    @confirm="lock"
  >
    <p>Historia i definicje zostają. Ponowne włączenie wymaga klucza.</p>
    <p v-if="waitingApprovals > 0" class="lock-warning">
      Przebiegi czekające na zgodę: {{ waitingApprovals }}. Po wyłączeniu nie
      da się ich zatwierdzić ani odrzucić (tylko zatrzymać), a do tego czasu
      trzymają katalog roboczy.
    </p>
  </ConfirmDialog>
</template>

<style scoped>
.workflows-unlocked {
  color: var(--text);
  font-weight: 600;
}
.lock-warning {
  font-weight: 600;
}
.workflows-unlock {
  align-items: flex-start;
}
</style>
