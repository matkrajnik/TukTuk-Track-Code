<script setup lang="ts">
import type { JiraFieldValue } from "../store";

/**
 * Chipy pól z Jiry (Ustawienia → Jira → „Pola Jiry na kafelku”): etykiety
 * jako osobne chipy bez nazwy pola, inne pola jako „Nazwa: wartość”.
 * Używane w kafelku zadania, na karcie tygodnia i na kartach widoku Jira.
 */
defineProps<{ fields?: JiraFieldValue[] | null }>();
const labels = (f: JiraFieldValue) =>
  f.value
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
</script>

<template>
  <template v-for="f in fields ?? []" :key="f.id">
    <template v-if="f.id === 'labels'">
      <span
        v-for="label in labels(f)"
        :key="f.id + ':' + label"
        class="jira-field jira-field-label"
        :title="f.name + ': ' + f.value"
        >{{ label }}</span
      >
    </template>
    <span v-else class="jira-field" :title="f.name + ': ' + f.value"
      ><span class="jira-field-name">{{ f.name }}:</span> {{ f.value }}</span
    >
  </template>
</template>
