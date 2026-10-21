<script setup lang="ts">
/** Ustawienia → Skróty: globalne skróty szybkiego przełączania i Start/Stop. */
import { ref } from "vue";
import ShortcutInput from "../ShortcutInput.vue";
import { defaultSettings, writeSettings, type AppSettings } from "../../settings";
import { message } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
/** Błędy rejestracji skrótów z aplikacji (`AppInfo.shortcutErrors`). */
const props = defineProps<{
  shortcutErrors: { quick: string; toggle: string };
}>();
const busy = ref(false);
/** Błąd zapisu skrótu (walidacja) pokazywany przy polu, nie w ogólnym alercie. */
const shortcutSaveError = ref({ quick: "", toggle: "" });
async function saveShortcut(field: "quick" | "toggle", value: string) {
  busy.value = true;
  shortcutSaveError.value = { quick: "", toggle: "" };
  try {
    settings.value = await writeSettings(
      field === "quick" ? { shortcutQuick: value } : { shortcutToggle: value },
    );
  } catch (e) {
    shortcutSaveError.value[field] = message(e);
  } finally {
    busy.value = false;
  }
}
const shortcutError = (field: "quick" | "toggle") =>
  shortcutSaveError.value[field] || props.shortcutErrors[field];
</script>
<template>
  <h2>Skróty</h2>
  <p>
    Ctrl+K — szybkie przełączanie w aplikacji.<br />{{
      settings.shortcutQuick
    }}
    — szybkie przełączanie z dowolnego okna.<br />{{
      settings.shortcutToggle
    }}
    — Start/Stop z dowolnego okna: zatrzymuje pomiar albo wznawia ostatnio
    mierzone zadanie.
  </p>
  <p class="hint">
    Kliknij pole i naciśnij nową kombinację z Ctrl, Alt, Shift lub Win. Esc
    anuluje.
  </p>
  <ShortcutInput
    :model-value="settings.shortcutQuick"
    label="Szybkie przełączanie (globalnie)"
    :default-value="defaultSettings.shortcutQuick"
    :error="shortcutError('quick')"
    :disabled="busy"
    @update:model-value="saveShortcut('quick', $event)"
  />
  <ShortcutInput
    :model-value="settings.shortcutToggle"
    label="Start/Stop (globalnie)"
    :default-value="defaultSettings.shortcutToggle"
    :error="shortcutError('toggle')"
    :disabled="busy"
    @update:model-value="saveShortcut('toggle', $event)"
  />
</template>
