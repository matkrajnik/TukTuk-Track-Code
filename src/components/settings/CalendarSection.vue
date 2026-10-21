<script setup lang="ts">
/** Ustawienia → Kalendarz: konto Microsoft (`CalendarSettings`) i tytuły spotkań. */
import CalendarSettings from "../CalendarSettings.vue";
import type { AppSettings } from "../../settings";
import { checked, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Kalendarz</h2>
  <CalendarSettings />
  <label class="inline-check"
    ><input
      :checked="settings.calendarShowTitles"
      type="checkbox"
      :disabled="busy"
      @change="save({ calendarShowTitles: checked($event) })"
    />Pokazuj tytuły spotkań</label
  >
  <p class="hint">
    Wyłączone: zamiast tytułu spotkania z Outlooka widać „Spotkanie”, np. przy
    udostępnianiu ekranu. Spotkania z Outlooka są tylko do odczytu; czas
    spotkania dodajesz sam. Ręczne spotkania (spoza Outlooka, także bez
    połączonego konta) dodajesz przyciskiem „Dodaj spotkanie” na osi dnia,
    w Zadaniach → Dzisiaj i na tablicy tygodnia; ich temat jest widoczny zawsze.
  </p>
</template>
