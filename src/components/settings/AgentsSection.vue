<script setup lang="ts">
/**
 * Ustawienia → Agenci (B8): hooki Claude Code i Codexa, z których aplikacja wie,
 * że agent pracuje, czeka na zgodę albo skończył. Wyłącznik rejestruje i usuwa
 * wpisy w `~/.claude/settings.json` i `~/.codex/hooks.json` (obce wpisy, np. Orki,
 * zostają); poniżej stan rejestracji, zaufanie Codexa i ostatnie zdarzenia.
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { agentHooksStatus, type AgentHooksStatus } from "../../store";
import type { AppSettings } from "../../settings";
import { clock } from "../../time";
import { checked, message, useSettingsSection } from "./section";

const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);

const empty: AgentHooksStatus = {
  enabled: false,
  exeFound: true,
  error: "",
  claude: { found: false, file: "", registration: "missing", error: "" },
  codex: { found: false, file: "", registration: "missing", error: "" },
  codexTrust: "none",
  events: [],
};
const status = ref<AgentHooksStatus>(empty);
const loadError = ref("");

async function load() {
  try {
    status.value = await agentHooksStatus();
    loadError.value = "";
  } catch (e) {
    loadError.value = message(e);
  }
}

/** Opis rejestracji w pliku jednego agenta. */
function registration(file: AgentHooksStatus["claude"]): string {
  if (file.error) return file.error;
  if (!file.found) return "nie znaleziono (agent nie jest zainstalowany)";
  if (!settings.value.agentHooks) return "wyłączone";
  if (file.registration === "current") return "zarejestrowane";
  if (file.registration === "stale")
    return "wymaga aktualizacji (zapisze się w ciągu minuty)";
  return "niezarejestrowane (zapisze się w ciągu minuty)";
}
const trust = computed(() => {
  const s = status.value;
  if (!settings.value.agentHooks || !s.codex.found) return "";
  switch (s.codexTrust) {
    case "trusted":
      return "Codex: hooki zatwierdzone.";
    case "partial":
      return "Codex: część hooków czeka na zatwierdzenie. Otwórz Codexa i wpisz /hooks, żeby zatwierdzić pozostałe.";
    case "untrusted":
      return "Codex: hooki czekają na zatwierdzenie. Otwórz Codexa i wpisz /hooks, żeby je zatwierdzić; do tego czasu sesje Codexa nie są widoczne.";
    default:
      return "";
  }
});
const folderName = (cwd: string) =>
  cwd
    .replace(/[\/]+$/, "")
    .split(/[\/]/)
    .pop() ?? "";

let timer: number | undefined;
onMounted(() => {
  void load();
  // Zdarzenia przychodzą co kilka sekund.
  timer = window.setInterval(() => void load(), 5_000);
});
onUnmounted(() => window.clearInterval(timer));
watch(
  () => settings.value.agentHooks,
  () => void load(),
);
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Agenci</h2>
  <label class="inline-check"
    ><input
      :checked="settings.agentHooks"
      type="checkbox"
      :disabled="busy"
      @change="save({ agentHooks: checked($event) })"
    />Hooki agentów (Claude Code, Codex)</label
  >
  <p class="hint">
    Claude Code i Codex uruchamiają krótki program todo-track-hook.exe przy
    każdym zdarzeniu (nowe polecenie, użycie narzędzia, prośba o zgodę, koniec
    pracy). Program przekazuje aplikacji tylko identyfikator sesji, folder
    roboczy i nazwę zdarzenia (pierwsze polecenie sesji zostaje w pamięci, bez
    zapisu na dysku) i kończy się cicho, gdy aplikacja nie działa. Włączenie
    dopisuje wpisy do ~/.claude/settings.json i ~/.codex/hooks.json, wyłączenie
    usuwa tylko wpisy Todo Track; wpisy innych programów zostają.
  </p>
  <div role="status" aria-label="Stan hooków agentów" class="agent-hooks-status">
    <p>Claude Code: {{ registration(status.claude) }}</p>
    <p>Codex: {{ registration(status.codex) }}</p>
    <p v-if="trust" class="agent-hooks-trust" :class="status.codexTrust">
      {{ trust }}
    </p>
    <p v-if="settings.agentHooks && !status.exeFound" class="error">
      Brak programu todo-track-hook.exe obok aplikacji. Zainstaluj aplikację
      ponownie.
    </p>
    <p v-if="status.error" class="error">{{ status.error }}</p>
    <p v-if="loadError" class="error" role="alert">{{ loadError }}</p>
  </div>
  <h3>Ostatnie zdarzenia</h3>
  <ul
    v-if="status.events.length"
    class="agent-hooks-events"
    aria-label="Ostatnie zdarzenia agentów"
  >
    <li v-for="(e, i) in status.events" :key="i">
      <span class="mono">{{ clock(e.at, true) }}</span>
      {{ e.agent === "claude" ? "Claude" : "Codex" }} · {{ e.event }}
      <span v-if="e.detail" class="hint">({{ e.detail }})</span>
      <span v-if="e.cwd" class="hint"> · {{ folderName(e.cwd) }}</span>
    </li>
  </ul>
  <p v-else class="hint">
    Jeszcze nie przyszło żadne zdarzenie. Lista pokazuje 30 ostatnich; służy do
    diagnostyki.
  </p>
</template>
