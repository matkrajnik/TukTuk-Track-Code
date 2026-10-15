<script setup lang="ts">
import { ref, watch } from "vue";
import { Download } from "lucide-vue-next";
import { exportReport, type ExportFormat } from "../store";

/**
 * Eksport CSV/XLSX przedziałów z podanych dni. W aplikacji otwiera okno
 * zapisu; w podglądzie CSV pokazuje się jako tekst, a XLSX jest niedostępny.
 */
const props = defineProps<{ days: string[]; name: string; label: string }>();

const busy = ref(false);
const status = ref("");
const text = ref("");
watch(
  () => props.days.join(","),
  () => {
    status.value = "";
    text.value = "";
  },
);

async function run(format: ExportFormat) {
  busy.value = true;
  status.value = "";
  text.value = "";
  try {
    const result = await exportReport(format, props.days, props.name);
    if (!result) return;
    if ("path" in result) status.value = "Zapisano eksport: " + result.path;
    else {
      text.value = result.text;
      status.value =
        "Podgląd: plik nie jest zapisywany, poniżej treść CSV.";
    }
  } catch (error) {
    status.value = error instanceof Error ? error.message : String(error);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="export-actions" role="group" :aria-label="label">
    <button
      class="today-button"
      :disabled="busy || !days.length"
      @click="run('csv')"
    >
      <Download :size="14" aria-hidden="true" /> Eksport CSV</button
    ><button
      class="today-button"
      :disabled="busy || !days.length"
      @click="run('xlsx')"
    >
      <Download :size="14" aria-hidden="true" /> Eksport XLSX
    </button>
    <p class="export-status" role="status">{{ status }}</p>
    <textarea
      v-if="text"
      class="export-text"
      aria-label="Treść eksportu CSV"
      :value="text"
      readonly
      spellcheck="false"
    />
  </div>
</template>
