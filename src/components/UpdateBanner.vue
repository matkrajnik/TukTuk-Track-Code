<script setup lang="ts">
import { computed } from "vue";
import { Download } from "lucide-vue-next";
import {
  available,
  installError,
  installing,
  installUpdate,
  postpone,
  postponed,
} from "../updater";

/**
 * Baner „Dostępna jest nowa wersja” (styl Fluent InfoBar jak pozostałe banery):
 * numer wersji, notatki z wydania, „Zainstaluj i uruchom ponownie” z paskiem
 * postępu pobierania oraz „Później”.
 */
const mb = (bytes: number) => (bytes / 1_048_576).toFixed(1).replace(".", ",");
const progressText = computed(() => {
  const p = installing.value;
  if (!p) return "";
  if (p.finished) return "Instalowanie… Aplikacja uruchomi się ponownie.";
  if (p.total) {
    const percent = Math.min(100, Math.round((p.downloaded / p.total) * 100));
    return `Pobieranie… ${mb(p.downloaded)} z ${mb(p.total)} MB (${percent}%)`;
  }
  return `Pobieranie… ${mb(p.downloaded)} MB`;
});
</script>

<template>
  <section
    v-if="available && !postponed"
    class="idle-banner update-banner"
    aria-label="Dostępna aktualizacja"
  >
    <Download class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Dostępna jest nowa wersja Todo Track {{ available.version }}</h2>
    <p v-if="available.notes" class="update-notes">{{ available.notes }}</p>
    <p v-else>
      Aktualizacja pobierze się i zainstaluje bez utraty zadań, historii i
      ustawień.
    </p>
    <div v-if="installing" class="update-progress" role="status">
      <progress
        aria-label="Postęp pobierania"
        :max="installing.total ?? undefined"
        :value="
          installing.finished
            ? (installing.total ?? undefined)
            : installing.total
              ? installing.downloaded
              : undefined
        "
      ></progress>
      <span>{{ progressText }}</span>
    </div>
    <p v-if="installError" class="jira-move-error" role="alert">
      {{ installError }}
    </p>
    <div class="planning-actions">
      <button
        class="primary-button"
        :disabled="!!installing"
        @click="installUpdate()"
      >
        Zainstaluj i uruchom ponownie</button
      ><button class="today-button" :disabled="!!installing" @click="postpone()">
        Później
      </button>
    </div>
  </section>
</template>
