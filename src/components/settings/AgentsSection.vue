<script setup lang="ts">
/**
 * Ustawienia → Agenci (B8): hooki Claude Code i Codexa, z których aplikacja wie,
 * że agent pracuje, czeka na zgodę albo skończył. Wyłącznik rejestruje i usuwa
 * wpisy w `~/.claude/settings.json` i `~/.codex/hooks.json` (obce wpisy, np. Orki,
 * zostają); poniżej stan rejestracji, zaufanie Codexa i ostatnie zdarzenia.
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  agentHooksStatus,
  externalApiInfo,
  orcaStatus,
  type AgentHooksStatus,
  type OrcaStatus,
} from "../../store";
import type { AppSettings } from "../../settings";
import { clock } from "../../time";
import { checked, message, text, useSettingsSection } from "./section";

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
      // Aplikacja widzi tylko, że wpis zaufania istnieje, nie czy pasuje do
      // bieżącego polecenia (skrót liczy Codex); po zmianie polecenia hooka
      // Codex wymaga ponownego zatwierdzenia.
      return "Codex: hooki zatwierdzone. Jeśli sesje Codexa nie pojawiają się w „Sesjach agentów”, zatwierdź hooki ponownie: /hooks w Codexie albo ustawienia hooków w Codex Desktop.";
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
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() ?? "";

/** Stan orca.cmd (B9): wykryta albo podana ścieżka. */
const orca = ref<OrcaStatus | null>(null);
async function loadOrca() {
  try {
    orca.value = await orcaStatus();
  } catch {
    orca.value = null;
  }
}
async function savePath(event: Event) {
  await save({ orcaPath: text(event).trim() });
  await loadOrca();
}
const orcaInfo = computed(() => {
  const o = orca.value;
  if (!o) return "";
  if (o.found) return `Znaleziono: ${o.path}`;
  return "Nie znaleziono orca.cmd. Zainstaluj Orkę albo podaj ścieżkę.";
});

/** Ścieżka tuktuk-track-ctl.exe (zewnętrzne sterowanie pomiarem). */
const ctlPath = ref<string | null>(null);
async function loadCtl() {
  try {
    ctlPath.value = (await externalApiInfo()).ctlPath;
  } catch {
    ctlPath.value = null;
  }
}

let timer: number | undefined;
onMounted(() => {
  void load();
  void loadOrca();
  void loadCtl();
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
    Claude Code i Codex uruchamiają krótki program tuktuk-track-hook.exe przy
    każdym zdarzeniu (nowe polecenie, użycie narzędzia, prośba o zgodę, koniec
    pracy). Program przekazuje aplikacji tylko identyfikator sesji, folder
    roboczy i nazwę zdarzenia (pierwsze polecenie sesji zostaje w pamięci, bez
    zapisu na dysku) i kończy się cicho, gdy aplikacja nie działa. Włączenie
    dopisuje wpisy do ~/.claude/settings.json i ~/.codex/hooks.json, wyłączenie
    usuwa tylko wpisy TukTuk Track; wpisy innych programów zostają.
  </p>
  <div role="status" aria-label="Stan hooków agentów" class="agent-hooks-status">
    <p>Claude Code: {{ registration(status.claude) }}</p>
    <p>Codex: {{ registration(status.codex) }}</p>
    <p v-if="trust" class="agent-hooks-trust" :class="status.codexTrust">
      {{ trust }}
    </p>
    <p v-if="settings.agentHooks && !status.exeFound" class="error">
      Brak programu tuktuk-track-hook.exe obok aplikacji. Zainstaluj aplikację
      ponownie.
    </p>
    <p v-if="status.error" class="error">{{ status.error }}</p>
    <p v-if="loadError" class="error" role="alert">{{ loadError }}</p>
  </div>
  <h3>Zewnętrzne aplikacje</h3>
  <label class="inline-check"
    ><input
      :checked="settings.externalApi"
      type="checkbox"
      :disabled="busy"
      @change="save({ externalApi: checked($event) })"
    />Pozwól innym programom sterować pomiarem</label
  >
  <p class="hint">
    Programy uruchomione na tym koncie Windows mogą dodać zadanie (i od razu
    uruchomić jego pomiar), zatrzymać pomiar i odczytać, co jest teraz mierzone.
    Służy do tego program tuktuk-track-ctl.exe, np.
    <code class="mono">tuktuk-track-ctl add "Przegląd PR" --start</code>, albo
    nazwany potok \\.\pipe\tuktuk-track-api-&lt;użytkownik&gt;. Start po
    zakończonym dniu wznawia dzień bez pytania. Wyłączone: każde żądanie jest
    odrzucane.
  </p>
  <p class="hint" role="status" aria-label="Stan tuktuk-track-ctl.exe">
    {{
      ctlPath
        ? `Program: ${ctlPath}`
        : "Program tuktuk-track-ctl.exe jest dostępny w zainstalowanej aplikacji."
    }}
  </p>
  <h3>Orca</h3>
  <p class="hint">
    Akcja „Otwórz worktree w Orce” na rozwiniętym kafelku zadania ze zgłoszeniem
    (Jira, Redmine) używa wiersza poleceń Orki (orca.cmd). Aplikacja czyta też
    listę worktree Orki, żeby rozpoznać zadanie agenta po folderze worktree
    (tylko odczyt, gdy Orca działa).
  </p>
  <div class="planning-fields settings-fields">
    <label
      >Ścieżka do orca.cmd<input
        :value="settings.orcaPath"
        maxlength="1024"
        :placeholder="orca?.defaultPath || 'Wykrywana automatycznie'"
        :disabled="busy"
        @change="savePath"
    /></label>
  </div>
  <p v-if="orcaInfo" class="hint" role="status" aria-label="Stan orca.cmd">
    {{ orcaInfo }}
  </p>
  <label class="inline-check"
    ><input
      :checked="settings.orcaAutostart"
      type="checkbox"
      :disabled="busy"
      @change="save({ orcaAutostart: checked($event) })"
    />Uruchamiaj Orkę, gdy nie działa</label
  >
  <p class="hint">
    Wyłączone (domyślnie): przy nieuruchomionej Orce dialog pokazuje „Orca nie
    działa” i nic nie tworzy. Włączone: aplikacja uruchomi Orkę (orca open)
    przy otwarciu dialogu worktree, gdy Orca nie działa; komunikat w dialogu
    znika po jej starcie.
  </p>
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
