<script setup lang="ts">
/**
 * „Pokaż, co zostanie wysłane” (A1): wspólny komponent funkcji AI (A3–A6 i
 * Ustawienia). Pokazuje tekst po filtrze danych osobowych (`ai_privacy`), czyli
 * dokładnie to, co dostanie model; samo otwarcie niczego nie wysyła ani nie
 * wywołuje modelu.
 */
import { ref, watch } from "vue";
import { aiRedactPreview } from "../store";

const props = defineProps<{
  /** Tekst przed filtrem (prompt, wiadomość czatu). */
  text: string;
}>();
const open = ref(false);
const sent = ref("");
const error = ref("");
/** Ostatnie żądanie: wolniejsza odpowiedź nie nadpisuje nowszej. */
let latest = 0;

async function load() {
  const id = ++latest;
  try {
    const result = await aiRedactPreview(props.text);
    if (id !== latest) return;
    sent.value = result;
    error.value = "";
  } catch (e) {
    if (id !== latest) return;
    sent.value = "";
    error.value = String(e instanceof Error ? e.message : e);
  }
}
function toggle(event: Event) {
  open.value = (event.target as HTMLDetailsElement).open;
  if (open.value) void load();
}
watch(
  () => props.text,
  () => {
    if (open.value) void load();
  },
);
</script>

<template>
  <details class="ai-preview" :open="open" @toggle="toggle">
    <summary>Pokaż, co zostanie wysłane</summary>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <pre v-else aria-label="Tekst wysyłany do modelu">{{ sent }}</pre>
    <p class="hint">
      To tekst po ukryciu nazwisk i imion; model nie dostaje niczego więcej.
    </p>
  </details>
</template>
