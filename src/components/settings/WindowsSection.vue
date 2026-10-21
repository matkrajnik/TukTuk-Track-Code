<script setup lang="ts">
/** Ustawienia → Okna: mini-widget. */
import type { AppSettings } from "../../settings";
import { checked, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Okna</h2>
  <label class="inline-check"
    ><input
      :checked="settings.widgetEnabled"
      type="checkbox"
      :disabled="busy"
      @change="save({ widgetEnabled: checked($event) })"
    />Pokazuj mini-widget zawsze na wierzchu</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.widgetBackgroundOnly"
      type="checkbox"
      :disabled="busy || !settings.widgetEnabled"
      @change="save({ widgetBackgroundOnly: checked($event) })"
    />Tylko gdy główne okno jest schowane lub bez fokusu</label
  >
  <p class="hint">
    Mini-widget pokazuje bieżący pomiar z przyciskami Start/Stop i ⇄. X na
    widgecie chowa go do końca sesji; przywrócisz go przełącznikiem w
    nagłówku.
  </p>
</template>
