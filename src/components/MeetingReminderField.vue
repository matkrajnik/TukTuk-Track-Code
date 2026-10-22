<script setup lang="ts">
/**
 * Pole „Przypomnienie” spotkania (C3): Domyślne (z ustawień) / 1 / 5 / 10 / 15
 * / 30 min / Bez przypomnienia. Wartość to nadpisanie dla jednego spotkania
 * (`meeting_reminder_set`): minuty, `null` (bez przypomnienia) albo `"default"`.
 */
import { computed } from "vue";
import {
  meetingReminderMinutes,
  type MeetingReminderValue,
} from "../remindersapi";
import { useSettings } from "../target";

const model = defineModel<MeetingReminderValue>({ required: true });
withDefaults(defineProps<{ disabled?: boolean; inputId?: string }>(), {
  disabled: false,
  inputId: "meeting-reminder",
});
const settings = useSettings();
const defaultLabel = computed(() =>
  settings.value.meetingReminderEnabled
    ? `Domyślne (${settings.value.meetingReminderMinutes} min)`
    : "Domyślne (wyłączone)",
);
const text = computed({
  get: () =>
    model.value === "default"
      ? "default"
      : model.value === null
        ? "off"
        : String(model.value),
  set: (v: string) => {
    model.value = v === "default" ? "default" : v === "off" ? null : Number(v);
  },
});
</script>

<template>
  <select
    :id="inputId"
    v-model="text"
    class="meeting-type"
    :disabled="disabled"
  >
    <option value="default">{{ defaultLabel }}</option>
    <option v-for="m in meetingReminderMinutes" :key="m" :value="String(m)">
      {{ m }} min
    </option>
    <option value="off">Bez przypomnienia</option>
  </select>
</template>
