<script setup lang="ts">
/** Ustawienia → Timesheet: konto TS (`TimesheetSettings`) i przełącznik wysyłki. */
import LegacyReconnect from "./LegacyReconnect.vue";
import TimesheetSettings from "../TimesheetSettings.vue";
import type { AppSettings } from "../../settings";
import { checked, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Timesheet (TS)</h2>
  <LegacyReconnect kind="timesheet" />
  <TimesheetSettings>
    <template #send>
      <h3>Wysyłka</h3>
      <label class="inline-check"
        ><input
          :checked="settings.tsSend"
          type="checkbox"
          :disabled="busy"
          @change="save({ tsSend: checked($event) })"
        />Wysyłaj raport TS</label
      >
      <p class="hint">
        Wysyłka zawsze wymaga potwierdzenia w podglądzie („Zapisz w TS”).
        Wyłączone chowa przyciski „Wyślij do TS” i blokuje zapis w TS także
        w aplikacji, nie tylko w interfejsie.
      </p>
    </template>
  </TimesheetSettings>
</template>
