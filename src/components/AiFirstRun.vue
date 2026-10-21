<script setup lang="ts">
/**
 * Dialog pierwszego uruchomienia asystenta AI (A1): wybór agenta (Claude Code,
 * Codex tylko gdy wykryty, „Nie używaj”). Wybór trafia do ustawienia `aiEngine`
 * i dialog nie wraca. Zamknięcie (Esc, kliknięcie poza oknem) nic nie zapisuje
 * i aplikacja zapyta przy następnym starcie.
 */
import { ref } from "vue";
import ConfirmDialog from "./ConfirmDialog.vue";
import { writeSettings } from "../settings";
import type { AiDetected } from "../api/ai";

defineProps<{ detected: AiDetected }>();
const emit = defineEmits<{ close: []; chosen: [engine: string] }>();
const busy = ref(false);
const error = ref("");

async function choose(engine: "claude" | "codex" | "off") {
  busy.value = true;
  error.value = "";
  try {
    await writeSettings({ aiEngine: engine });
    emit("chosen", engine);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <ConfirmDialog
    title="Asystent AI"
    label="Asystent AI"
    :error="error"
    @close="emit('close')"
  >
    <p>
      Asystent może podpowiadać przypisania, pomagać zamknąć dzień i odpowiadać
      na pytania o Twoje zadania. Korzysta z Twojej subskrypcji Claude Code albo
      Codexa, bez klucza API.
    </p>
    <ul class="hint">
      <li>Imiona i nazwiska są ukrywane, zanim tekst trafi do modelu.</li>
      <li>
        Asystent tylko proponuje: nic nie zmienia się w pomiarach, Timesheet,
        Jirze ani Redmine bez Twojego kliknięcia.
      </li>
      <li>Wybór zmienisz w Ustawieniach → Asystent AI.</li>
    </ul>
    <div class="modal-actions">
      <button
        v-if="detected.claude"
        :disabled="busy"
        :title="'Wykryto wersję ' + detected.claude.version"
        @click="choose('claude')"
      >
        Claude Code
      </button>
      <button
        v-if="detected.codex"
        :disabled="busy"
        :title="'Wykryto wersję ' + detected.codex.version"
        @click="choose('codex')"
      >
        Codex
      </button>
      <button :disabled="busy" @click="choose('off')">Nie używaj</button>
    </div>
  </ConfirmDialog>
</template>
