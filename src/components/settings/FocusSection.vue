<script setup lang="ts">
/** Ustawienia → Skupienie: długość bloku i przerwy. */
import type { AppSettings } from "../../settings";
import { checked, number, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Skupienie</h2>
  <div class="planning-fields settings-fields">
    <label
      >Blok skupienia (minuty)<input
        :value="settings.focusBlockMinutes"
        type="number"
        min="1"
        max="240"
        :disabled="busy"
        @change="save({ focusBlockMinutes: number($event) })"
    /></label>
    <label
      >Przerwa (minuty)<input
        :value="settings.focusBreakMinutes"
        type="number"
        min="1"
        max="60"
        :disabled="busy"
        @change="save({ focusBreakMinutes: number($event) })"
    /></label>
  </div>
  <div class="planning-actions">
    <button
      v-for="m in [25, 50]"
      :key="'b' + m"
      class="today-button"
      :aria-pressed="settings.focusBlockMinutes === m"
      :disabled="busy"
      @click="save({ focusBlockMinutes: m })"
    >
      Blok {{ m }} min</button
    ><button
      v-for="m in [5, 10]"
      :key="'p' + m"
      class="today-button"
      :aria-pressed="settings.focusBreakMinutes === m"
      :disabled="busy"
      @click="save({ focusBreakMinutes: m })"
    >
      Przerwa {{ m }} min
    </button>
  </div>
  <label class="inline-check"
    ><input
      :checked="settings.focusBreakStops"
      type="checkbox"
      :disabled="busy"
      @change="save({ focusBreakStops: checked($event) })"
    />Przerwa zatrzymuje pomiar</label
  >
  <p class="hint">
    „Skupienie” na pasku bieżącego pomiaru albo w widgecie zaczyna blok i
    uruchamia pomiar, jeśli nie trwa. W czasie bloku powiadomienia i
    propozycje agentów czekają i przychodzą po bloku w jednym zbiorczym.
    Stop albo inne zadanie kończy blok jako nieukończony.
  </p>
</template>
