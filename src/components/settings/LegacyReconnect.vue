<script setup lang="ts">
/**
 * „Połącz ponownie” w sekcji modułu, którego logowania nie udało się przenieść
 * z poprzedniej nazwy aplikacji (N1, `legacy_status.reconnect`).
 */
import { onMounted, onUnmounted, ref } from "vue";
import { readLegacyStatus } from "../../legacyapi";

const props = defineProps<{
  /** Rodzaj poświadczenia: `jira`, `calendar`, `timesheet`, `redmine`. */
  kind: string;
}>();

const visible = ref(false);
let alive = true;
onMounted(() => {
  readLegacyStatus()
    .then((status) => {
      if (alive) visible.value = (status?.reconnect ?? []).includes(props.kind);
    })
    .catch(() => {});
});
onUnmounted(() => {
  alive = false;
});
</script>
<template>
  <p v-if="visible" class="hint legacy-reconnect" role="status" aria-label="Połącz ponownie">
    Połącz ponownie: logowania z poprzedniej wersji aplikacji nie udało się przenieść.
    Zaloguj się jeszcze raz poniżej.
  </p>
</template>
<style scoped>
.legacy-reconnect {
  color: var(--warning);
}
</style>
