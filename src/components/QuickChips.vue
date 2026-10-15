<script setup lang="ts">
import { computed } from "vue";
import { X } from "lucide-vue-next";
import { blockingError, type Parsed, type Token } from "../quickadd";
import { openSettings, sectionInMessage } from "../navigation";
import {
  correctionTitle,
  usuallyText,
  type CorrectionOf,
} from "../estimates";

/** Rozpoznane zapisy pod polem szybkiego dodawania; kliknięcie usuwa zapis. */
const props = defineProps<{
  parsed: Parsed;
  /** Poprawka szacunków: podpowiedź „zwykle ~…” obok chipa szacunku. */
  correction?: CorrectionOf;
}>();
const emit = defineEmits<{ remove: [token: Token]; settings: [] }>();
const blocking = computed(() => blockingError(props.parsed));
/** Błąd wskazujący „Ustawienia → X” (np. nieznany szablon) prowadzi do sekcji. */
const section = computed(() =>
  blocking.value ? sectionInMessage(blocking.value) : null,
);
/** „zwykle ~2 h 40 min” dla szacunku z linii i projektu z linii. */
const usual = computed(() => {
  const c = props.correction?.(props.parsed.fields.project);
  const minutes = props.parsed.fields.estimateMinutes ?? 0;
  return c
    ? { text: usuallyText(minutes, c), title: correctionTitle(c) }
    : { text: "", title: "" };
});
function openSection() {
  if (!section.value) return;
  openSettings(section.value);
  emit("settings");
}
</script>

<template>
  <div
    v-if="parsed.tokens.length"
    class="quick-chips"
    aria-label="Rozpoznane pola"
  >
    <template
      v-for="token in parsed.tokens"
      :key="`${token.start}-${token.text}`"
    >
      <button
        type="button"
        class="quick-chip"
        :class="[token.kind, { invalid: token.kind === 'error' }]"
        :title="token.error ?? `Usuń ${token.text}`"
        :aria-label="`Usuń: ${token.label}`"
        @mousedown.prevent
        @click="emit('remove', token)"
      >
        {{ token.label }}<X :size="11" />
      </button>
      <span
        v-if="token.kind === 'estimate' && usual.text"
        class="quick-chip-hint"
        :title="usual.title"
        >· {{ usual.text }}</span
      >
    </template>
    <span v-if="blocking" class="quick-chip-error" role="alert">{{
      blocking
    }}</span>
    <button
      v-if="section"
      type="button"
      class="text-link quick-chip-settings"
      @click="openSection"
    >
      Otwórz ustawienia
    </button>
  </div>
</template>
