<script setup lang="ts">
/**
 * Akcje przedziału na osi dnia: ikony Edytuj, Podziel, Usuń (nazwa dostępna
 * w `aria-label`, opis w `title`). Trwającego przedziału nie da się usunąć.
 */
import { Pencil, Scissors, Trash2 } from "lucide-vue-next";
import type { Session } from "../store";

defineProps<{ session: Session; label: string; busy: boolean }>();
const emit = defineEmits<{ edit: []; split: []; remove: [] }>();
</script>

<template>
  <span class="timeline-actions session-actions">
    <button
      class="today-button icon-only"
      :aria-label="'Edytuj przedział ' + label"
      :title="
        session.endedAt === null
          ? 'Popraw godzinę startu trwającego pomiaru'
          : 'Edytuj przedział'
      "
      @click="emit('edit')"
    >
      <Pencil :size="14" /></button
    ><button
      class="today-button icon-only"
      :aria-label="'Podziel przedział ' + session.id"
      title="Podziel przedział"
      @click="emit('split')"
    >
      <Scissors :size="14" /></button
    ><button
      v-if="session.endedAt !== null"
      class="today-button icon-only danger"
      :aria-label="'Usuń przedział ' + label"
      title="Usuń przedział"
      :disabled="busy"
      @click="emit('remove')"
    >
      <Trash2 :size="14" /></button
    ><!-- Trwający pomiar: puste miejsce po „Usuń”, czasy zostają w jednej kolumnie. -->
    <span v-else class="icon-slot" aria-hidden="true" />
  </span>
</template>

<style scoped>
.icon-slot {
  width: 32px;
  flex-shrink: 0;
}
.icon-only {
  width: 32px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
</style>
