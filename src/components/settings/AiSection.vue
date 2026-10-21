<script setup lang="ts">
/**
 * Ustawienia → Asystent AI (A1): silnik (Claude Code, Codex albo wyłączony),
 * wykryte wersje i „Sprawdź ponownie”, ręczne ścieżki, modele, „Ukrywaj te
 * nazwy” (filtr danych osobowych, A0), przełączniki tytułów spotkań i okien,
 * przełączniki funkcji oraz „Ostatnie wywołania” bez treści. Asystent korzysta
 * z subskrypcji Claude Code albo Codexa i tylko proponuje.
 */
import { computed, onMounted, ref, watch } from "vue";
import AiPreview from "../AiPreview.vue";
import {
  aiCalls,
  aiDetect,
  aiEngines,
  aiErrorLabel,
  aiStatus,
  aiTest,
  aiTestPrompt,
  type AiCall,
  type AiCli,
  type AiStatus,
} from "../../store";
import type { AppSettings } from "../../settings";
import { clock } from "../../time";
import { checked, message, text, useSettingsSection } from "./section";

const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);

const status = ref<AiStatus | null>(null);
const calls = ref<AiCall[]>([]);
const loadError = ref("");
const checking = ref(false);
const testing = ref(false);
/** Wynik „Przetestuj silnik”: komunikat albo błąd po polsku. */
const testResult = ref<{ ok: boolean; text: string } | null>(null);

const installLinks = {
  claude: "https://docs.claude.com/en/docs/claude-code/overview",
  codex: "https://github.com/openai/codex",
};

async function load(recheck = false) {
  checking.value = true;
  try {
    status.value = recheck ? await aiDetect() : await aiStatus();
    calls.value = await aiCalls();
    loadError.value = "";
  } catch (e) {
    loadError.value = message(e);
  } finally {
    checking.value = false;
  }
}
onMounted(() => void load());
// Zmiana silnika zmienia „aktywny”; ścieżki wymagają ponownego wykrycia.
watch(
  () => settings.value.aiEngine,
  () => void load(),
);

const found = computed(
  () => !!(status.value?.detected.claude || status.value?.detected.codex),
);
const detectedOf = (id: string): AiCli | null =>
  (id === "claude" ? status.value?.detected.claude : status.value?.detected.codex) ??
  null;
const versionLine = (id: string, name: string) => {
  if (!status.value) return `${name}: sprawdzanie…`;
  const cli = detectedOf(id);
  return cli
    ? `${name}: wersja ${cli.version} (${cli.path})`
    : `${name}: nie znaleziono`;
};
const engineValue = computed(() => settings.value.aiEngine);
/** Wybrany, ale niewykryty silnik: funkcje AI nie działają. */
const engineMissing = computed(
  () =>
    !!status.value &&
    (engineValue.value === "claude" || engineValue.value === "codex") &&
    !detectedOf(engineValue.value),
);

async function savePath(patch: Partial<AppSettings>) {
  await save(patch);
  await load(true);
}
const hiddenNames = computed(() => settings.value.aiHiddenNames.join("\n"));
function saveHidden(event: Event) {
  void save({ aiHiddenNames: (event.target as HTMLTextAreaElement).value.split("\n") });
}

async function runTest() {
  testing.value = true;
  testResult.value = null;
  try {
    testResult.value = { ok: true, text: await aiTest() };
  } catch (e) {
    testResult.value = { ok: false, text: message(e) };
  } finally {
    testing.value = false;
    try {
      calls.value = await aiCalls();
    } catch {
      /* dziennik odświeży się przy następnym otwarciu */
    }
  }
}
</script>

<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Asystent AI</h2>
  <p class="hint">
    Asystent korzysta z Twojej subskrypcji Claude Code albo Codexa (bez klucza
    API) i tylko proponuje: nic nie trafia do pomiarów, Timesheet, Jiry ani
    Redmine bez Twojego kliknięcia. Zanim tekst trafi do modelu, imiona i
    nazwiska są ukrywane. Bez wybranego silnika żadna funkcja AI nie działa.
  </p>
  <div
    role="status"
    aria-label="Stan asystenta AI"
    class="agent-hooks-status"
  >
    <p v-if="status && !found" class="error">
      Nie znaleziono Claude Code ani Codexa. Zainstaluj
      <a :href="installLinks.claude" target="_blank" rel="noreferrer"
        >Claude Code</a
      >
      albo
      <a :href="installLinks.codex" target="_blank" rel="noreferrer">Codex</a>,
      albo podaj ścieżkę poniżej.
    </p>
    <p>{{ versionLine("claude", "Claude Code") }}</p>
    <p>{{ versionLine("codex", "Codex") }}</p>
    <p v-if="engineMissing" class="error">
      Wybrany silnik nie jest wykryty, więc funkcje AI nie działają.
    </p>
    <p v-if="loadError" class="error" role="alert">{{ loadError }}</p>
  </div>
  <div class="planning-fields settings-fields">
    <label
      >Silnik asystenta AI<select
        :value="engineValue"
        :disabled="busy"
        @change="save({ aiEngine: text($event) })"
      >
        <option v-if="engineValue === ''" value="" disabled>Nie wybrano</option>
        <option
          v-for="e in aiEngines"
          :key="e.id"
          :value="e.id"
          :disabled="!detectedOf(e.id) && engineValue !== e.id"
        >
          {{ e.name }}{{ !detectedOf(e.id) ? " (nie znaleziono)" : "" }}
        </option>
        <option value="off">Wyłączony</option>
      </select></label
    >
  </div>
  <p>
    <button :disabled="checking" @click="load(true)">Sprawdź ponownie</button>
  </p>
  <h3>Ścieżki</h3>
  <p class="hint">
    Aplikacja szuka claude w %USERPROFILE%\.local\bin i w PATH, a codex w PATH,
    w npm global i w %LOCALAPPDATA%\Programs\OpenAI. Ścieżka ręczna jest
    sprawdzana na końcu.
  </p>
  <div class="planning-fields settings-fields">
    <label
      >Ścieżka do claude<input
        :value="settings.aiClaudePath"
        maxlength="1024"
        placeholder="Wykrywana automatycznie"
        :disabled="busy"
        @change="savePath({ aiClaudePath: text($event) })"
    /></label>
    <label
      >Ścieżka do codex<input
        :value="settings.aiCodexPath"
        maxlength="1024"
        placeholder="Wykrywana automatycznie"
        :disabled="busy"
        @change="savePath({ aiCodexPath: text($event) })"
    /></label>
  </div>
  <h3>Modele</h3>
  <div class="planning-fields settings-fields">
    <label
      >Model szybki<input
        :value="settings.aiModelFast"
        maxlength="80"
        placeholder="Domyślny model CLI"
        :disabled="busy"
        @change="save({ aiModelFast: text($event) })"
    /></label>
    <label
      >Model główny<input
        :value="settings.aiModelMain"
        maxlength="80"
        placeholder="Domyślny model CLI"
        :disabled="busy"
        @change="save({ aiModelMain: text($event) })"
    /></label>
  </div>
  <p class="hint">
    „Szybki” służy dopasowaniom i krótkim pytaniom o dane, „główny”
    Zamknięciu dnia i dłuższym rozmowom. Puste pole oznacza domyślny model
    wybranego CLI. Asystent dzieli limit subskrypcji z Twoją pracą w Claude
    Code albo Codexie.
  </p>
  <h3>Dane osobowe</h3>
  <label class="settings-statuses"
    >Ukrywaj te nazwy (jedna w wierszu)<textarea
      :value="hiddenNames"
      rows="4"
      :disabled="busy"
      @change="saveHidden"
    />
  </label>
  <p class="hint">
    Imiona i nazwiska osób z Jiry oraz Twoje własne są ukrywane
    automatycznie; tu dopisz pozostałe (klientów, współpracowników). Dopasowanie
    nie zależy od wielkości liter i obejmuje polską odmianę. Ograniczenia:
    nazwisko spoza tej listy wpisane ręcznie w tytuł albo notatkę przejdzie do
    modelu, a nazwisko będące zwykłym słowem (np. „Marzec”) jest ukrywane
    wszędzie.
  </p>
  <label class="inline-check"
    ><input
      :checked="settings.aiSendMeetingTitles"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiSendMeetingTitles: checked($event) })"
    />Wysyłaj tytuły spotkań</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.aiSendWindowTitles"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiSendWindowTitles: checked($event) })"
    />Wysyłaj tytuły okien</label
  >
  <p class="hint">
    Domyślnie wyłączone: tytuły spotkań i okien nie wychodzą do modelu w ogóle,
    nie tylko po filtrze.
  </p>
  <h3>Funkcje</h3>
  <label class="inline-check"
    ><input
      :checked="settings.aiMcp"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiMcp: checked($event) })"
    />Serwer MCP dla agentów</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.aiChat"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiChat: checked($event) })"
    />Czat</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.aiAssignSuggest"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiAssignSuggest: checked($event) })"
    />Podpowiedzi przypisań</label
  >
  <label class="inline-check"
    ><input
      :checked="settings.aiDayClose"
      type="checkbox"
      :disabled="busy"
      @change="save({ aiDayClose: checked($event) })"
    />Asystent w Zamknięciu dnia</label
  >
  <p class="hint">
    Funkcje działają tylko przy wybranym i wykrytym silniku; model jest wołany
    wyłącznie na zdarzenie (nigdy w pętli), a jego propozycje czekają na
    „Zastosuj” albo „Odrzuć”.
  </p>
  <h3>Próba</h3>
  <p>
    <button :disabled="testing || !status?.active" @click="runTest">
      Przetestuj silnik
    </button>
  </p>
  <p
    v-if="testResult"
    :class="testResult.ok ? 'hint' : 'error'"
    :role="testResult.ok ? 'status' : 'alert'"
  >
    {{ testResult.text }}
  </p>
  <AiPreview :text="aiTestPrompt" />
  <h3>Ostatnie wywołania</h3>
  <ul
    v-if="calls.length"
    class="agent-hooks-events"
    aria-label="Ostatnie wywołania asystenta"
  >
    <li v-for="(c, i) in calls" :key="i">
      <span class="mono">{{ clock(c.at, true) }}</span>
      {{ c.kind }} · {{ c.length }} znaków ·
      {{ c.ok ? "ok" : "błąd: " + aiErrorLabel(c.error) }}
      <span v-if="c.cached" class="hint"> (z pamięci podręcznej)</span>
    </li>
  </ul>
  <p v-else class="hint">
    Jeszcze nie było wywołań. Lista pokazuje 30 ostatnich (rodzaj, czas,
    długość i wynik, bez treści) i służy do diagnostyki.
  </p>
</template>
