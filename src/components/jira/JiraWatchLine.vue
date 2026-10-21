<script setup lang="ts">
/**
 * Linia stanu obserwacji przypisań Jiry (B5): „Ostatnio sprawdzono 14:05”,
 * a po nieudanym sprawdzeniu (wygasły token, 401, brak sieci) także
 * „· błąd: …”. Widok Jira i Ustawienia → Jira. Bez linii, gdy obserwacja
 * jest wyłączona albo jeszcze nic nie sprawdziła.
 */
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  emptyJiraWatch,
  jiraWatchStatus,
  onJiraWatch,
  type JiraWatchStatus,
} from "../../store";
import { useSettings } from "../../target";
import { clock, dayKey, dayText } from "../../time";

const settings = useSettings();
const status = ref<JiraWatchStatus>(emptyJiraWatch());

async function load() {
  try {
    status.value = await jiraWatchStatus();
  } catch {
    /* bez linii stanu, gdy odczyt się nie udał */
  }
}
/** Godzina dzisiaj, z dniem dla wcześniejszych dni. */
const when = (time: number) =>
  dayKey(time) === dayKey(Date.now())
    ? clock(time)
    : `${dayText(dayKey(time))} ${clock(time)}`;

const text = computed(() => {
  const s = status.value;
  if (!s.lastOk && !s.lastError) return "";
  const checked = s.lastOk
    ? `Ostatnio sprawdzono ${when(s.lastOk)}`
    : "Jeszcze nie sprawdzono";
  return s.lastError ? `${checked} · błąd: ${s.lastError}` : checked;
});

let stop: (() => void) | null = null;
let timer = 0;
let alive = true;
onMounted(async () => {
  await load();
  // Odmontowany w trakcie pierwszego odczytu: bez interwału i nasłuchu.
  if (!alive) return;
  // Obserwacja sprawdza co 5 min; odczyt co minutę łapie też podgląd.
  timer = window.setInterval(load, 60_000);
  const off = await onJiraWatch((next) => {
    if (next) status.value = next;
    else void load();
  });
  if (alive) stop = off;
  else off();
});
onBeforeUnmount(() => {
  alive = false;
  window.clearInterval(timer);
  stop?.();
});
</script>

<template>
  <p
    v-if="settings.jiraAutoImport && text"
    class="hint jira-watch-line"
    :class="{ 'jira-watch-error': !!status.lastError }"
    role="status"
    :title="
      status.lastErrorAt
        ? 'Błąd obserwacji przypisań o ' + when(status.lastErrorAt)
        : 'Obserwacja przypisań sprawdza Jirę co 5 minut'
    "
  >
    {{ text }}
  </p>
</template>
