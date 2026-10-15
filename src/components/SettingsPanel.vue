<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import ShortcutInput from "./ShortcutInput.vue";
import TemplateEditor from "./TemplateEditor.vue";
import CalendarSettings from "./CalendarSettings.vue";
import { openView, type SettingsSection } from "../navigation";
import {
  defaultSettings,
  onSettingsChanged,
  onShortcutsChanged,
  readAppInfo,
  readSettings,
  writeSettings,
  type AppInfo,
  type AppSettings,
} from "../settings";
import {
  loadTemplates,
  readAutostart,
  templates,
  writeAutostart,
  type Autostart,
} from "../store";
/** Widoczna sekcja; zakładka Ustawienia pokazuje jedną naraz. */
defineProps<{ section: SettingsSection }>();
/** Formularz nowego szablonu w sekcji Szablony. */
const addingTemplate = ref(false);
const settings = ref<AppSettings>({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
});
const info = ref<AppInfo>({
  version: "",
  shortcutErrors: { quick: "", toggle: "" },
});
/** Błąd zapisu skrótu (walidacja) pokazywany przy polu, nie w ogólnym alercie. */
const shortcutSaveError = ref({ quick: "", toggle: "" });
const autostart = ref<Autostart>({ enabled: false, hidden: false });
const busy = ref(false),
  error = ref("");
const weekDays = [
  { day: 1, name: "Pon" },
  { day: 2, name: "Wt" },
  { day: 3, name: "Śr" },
  { day: 4, name: "Czw" },
  { day: 5, name: "Pt" },
  { day: 6, name: "Sob" },
  { day: 7, name: "Nd" },
];
const hm = (minutes: number) =>
  `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
const weekTarget = computed(() =>
  hm(settings.value.targetDayMinutes * settings.value.workDays.length),
);
async function save(patch: Partial<AppSettings>) {
  busy.value = true;
  error.value = "";
  try {
    settings.value = await writeSettings(patch);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    // Pole wraca do zapisanej wartości.
    const saved = await readSettings().catch(() => settings.value);
    settings.value = { ...saved, workDays: [...saved.workDays] };
  } finally {
    busy.value = false;
  }
}
const checked = (e: Event) => (e.target as HTMLInputElement).checked;
const text = (e: Event) => (e.target as HTMLInputElement).value;
const number = (e: Event) => {
  const value = (e.target as HTMLInputElement).value;
  return value === "" ? NaN : Number(value);
};
async function saveShortcut(field: "quick" | "toggle", value: string) {
  busy.value = true;
  shortcutSaveError.value = { quick: "", toggle: "" };
  try {
    settings.value = await writeSettings(
      field === "quick" ? { shortcutQuick: value } : { shortcutToggle: value },
    );
  } catch (e) {
    shortcutSaveError.value[field] = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const shortcutError = (field: "quick" | "toggle") =>
  shortcutSaveError.value[field] || info.value.shortcutErrors[field];
function toggleDay(day: number, on: boolean) {
  const days = settings.value.workDays.filter((d) => d !== day);
  void save({ workDays: on ? [...days, day] : days });
}
/** Autostart: stan z rejestru, „schowane” jako ustawienie autostartHidden. */
async function saveAutostart(field: keyof Autostart, e: Event) {
  const input = e.target as HTMLInputElement;
  const next = { ...autostart.value, [field]: input.checked };
  busy.value = true;
  error.value = "";
  try {
    autostart.value = await writeAutostart(next.enabled, next.hidden);
    settings.value = { ...settings.value, autostartHidden: autostart.value.hidden };
  } catch (err) {
    error.value = String(err instanceof Error ? err.message : err);
    autostart.value = await readAutostart().catch(() => autostart.value);
    // Odczytana wartość bywa taka sama jak przed kliknięciem, więc Vue nie
    // poprawi pola samo; cofamy je ręcznie do stanu z rejestru i ustawień.
    input.checked = autostart.value[field];
  } finally {
    busy.value = false;
  }
}
let unlisten: (() => void) | undefined;
let unlistenShortcuts: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  try {
    const stop = await onSettingsChanged((s) => (settings.value = s));
    // Panel mógł zostać odmontowany w trakcie rejestracji nasłuchu.
    if (!alive) return stop();
    unlisten = stop;
    const stopShortcuts = await onShortcutsChanged(
      (errors) => (info.value = { ...info.value, shortcutErrors: errors }),
    );
    if (!alive) return stopShortcuts();
    unlistenShortcuts = stopShortcuts;
    settings.value = await readSettings();
    info.value = await readAppInfo();
    autostart.value = await readAutostart();
    await loadTemplates();
  } catch (e) {
    if (alive) error.value = String(e);
  }
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlistenShortcuts?.();
});
</script>
<template>
  <section class="dashboard-card settings-panel">
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <template v-if="section === 'work'">
    <h2>Czas pracy</h2>
    <label class="inline-check"
      ><input
        :checked="settings.idleEnabled"
        type="checkbox"
        :disabled="busy"
        @change="save({ idleEnabled: checked($event) })"
      />Zapytaj o czas po co najmniej 10 minutach bezczynności</label
    >
    <p class="hint">
      Decydujesz, czy zachować czas, czy zakończyć pomiar na początku przerwy.
      Nic nie jest obcinane automatycznie.
    </p>
    <label class="inline-check"
      ><input
        :checked="settings.idleAgents"
        type="checkbox"
        :disabled="busy || !settings.idleEnabled"
        @change="save({ idleAgents: checked($event) })"
      />Praca agentów AI (Claude Code, Codex) liczy się jako aktywność</label
    >
    <label class="inline-check"
      ><input
        :checked="settings.agentSuggest"
        type="checkbox"
        :disabled="busy"
        @change="save({ agentSuggest: checked($event) })"
      />Proponuj przełączenie na zadanie, nad którym pracuje agent AI</label
    >
    <p class="hint">
      Po 2 minutach pracy agenta nad innym zadaniem niż mierzone. Timer nigdy
      nie przełącza się sam.
    </p>
    <template v-if="settings.agentMutedFolders.length">
      <p class="hint">Bez propozycji dla folderów (z podfolderami):</p>
      <ul class="muted-folders" aria-label="Foldery bez propozycji agentów">
        <li v-for="folder in settings.agentMutedFolders" :key="folder">
          <code class="mono">{{ folder }}</code
          ><button
            class="text-link"
            :disabled="busy"
            :aria-label="`Proponuj znowu dla ${folder}`"
            @click="
              save({
                agentMutedFolders: settings.agentMutedFolders.filter(
                  (f) => f !== folder,
                ),
              })
            "
          >
            Proponuj znowu
          </button>
        </li>
      </ul>
    </template>
    <div class="planning-fields settings-fields">
      <label
        >Cel dzienny (minuty)<input
          :value="settings.targetDayMinutes"
          type="number"
          min="0"
          max="1440"
          step="15"
          :disabled="busy"
          @change="save({ targetDayMinutes: number($event) })"
      /></label>
      <label
        >Początek dnia<input
          :value="settings.dayStart"
          type="time"
          :disabled="busy"
          @change="save({ dayStart: text($event) })"
      /></label>
    </div>
    <fieldset class="settings-days">
      <legend>Dni robocze</legend>
      <label v-for="d in weekDays" :key="d.day" class="inline-check"
        ><input
          type="checkbox"
          :checked="settings.workDays.includes(d.day)"
          :disabled="busy"
          :aria-label="'Dzień roboczy: ' + d.name"
          @change="toggleDay(d.day, checked($event))"
        />{{ d.name }}</label
      >
    </fieldset>
    <label class="inline-check"
      ><input
        :checked="settings.holidaysInTarget"
        type="checkbox"
        :disabled="busy"
        @change="save({ holidaysInTarget: checked($event) })"
      />Uwzględniaj święta w celu</label
    >
    <p class="hint">
      Cel dnia {{ hm(settings.targetDayMinutes) }}, cel tygodnia
      {{ weekTarget }}.
    </p>
    <p v-if="settings.holidaysInTarget" class="hint">
      Polskie święta ustawowe, także Wielkanoc i Boże Ciało, nie są dniami
      roboczymi: cel tygodnia i miesiąca jest wtedy niższy.
    </p>
    </template>
    <template v-if="section === 'focus'">
    <h2>Skupienie</h2>
    <div class="planning-fields settings-fields">
      <label
        >Blok skupienia (minuty)<input
          :value="settings.focusBlockMinutes"
          type="number"
          min="1"
          max="240"
          :disabled="busy"
          @change="save({ focusBlockMinutes: number($event) })"
      /></label>
      <label
        >Przerwa (minuty)<input
          :value="settings.focusBreakMinutes"
          type="number"
          min="1"
          max="60"
          :disabled="busy"
          @change="save({ focusBreakMinutes: number($event) })"
      /></label>
    </div>
    <div class="planning-actions">
      <button
        v-for="m in [25, 50]"
        :key="'b' + m"
        class="today-button"
        :aria-pressed="settings.focusBlockMinutes === m"
        :disabled="busy"
        @click="save({ focusBlockMinutes: m })"
      >
        Blok {{ m }} min</button
      ><button
        v-for="m in [5, 10]"
        :key="'p' + m"
        class="today-button"
        :aria-pressed="settings.focusBreakMinutes === m"
        :disabled="busy"
        @click="save({ focusBreakMinutes: m })"
      >
        Przerwa {{ m }} min
      </button>
    </div>
    <label class="inline-check"
      ><input
        :checked="settings.focusBreakStops"
        type="checkbox"
        :disabled="busy"
        @change="save({ focusBreakStops: checked($event) })"
      />Przerwa zatrzymuje pomiar</label
    >
    <p class="hint">
      „Skupienie” na pasku bieżącego pomiaru albo w widgecie zaczyna blok i
      uruchamia pomiar, jeśli nie trwa. W czasie bloku powiadomienia i
      propozycje agentów czekają i przychodzą po bloku w jednym zbiorczym. Stop
      albo inne zadanie kończy blok jako nieukończony.
    </p>
    </template>
    <template v-if="section === 'windows'">
    <h2>Okna</h2>
    <label class="inline-check"
      ><input
        :checked="settings.widgetEnabled"
        type="checkbox"
        :disabled="busy"
        @change="save({ widgetEnabled: checked($event) })"
      />Pokazuj mini-widget zawsze na wierzchu</label
    >
    <label class="inline-check"
      ><input
        :checked="settings.widgetBackgroundOnly"
        type="checkbox"
        :disabled="busy || !settings.widgetEnabled"
        @change="save({ widgetBackgroundOnly: checked($event) })"
      />Tylko gdy główne okno jest schowane lub bez fokusu</label
    >
    <p class="hint">
      Mini-widget pokazuje bieżący pomiar z przyciskami Start/Stop i ⇄. X na
      widgecie chowa go do końca sesji; przywrócisz go przełącznikiem w
      nagłówku.
    </p>
    </template>
    <template v-if="section === 'jira'">
    <h2>Jira</h2>
    <label class="inline-check"
      ><input
        :checked="settings.jiraAutoImport"
        type="checkbox"
        :disabled="busy"
        @change="save({ jiraAutoImport: checked($event) })"
      />Dodawaj nowe zgłoszenia przypisane do mnie</label
    >
    <p class="hint">
      Co 5 minut, gdy konto Jiry jest połączone. Nowe zgłoszenie trafia do „Do
      zaplanowania” z powiadomieniem; obecne przypisania przy pierwszym
      sprawdzeniu nie są importowane. Status z Jiry widać na kafelku zadania.
    </p>
    <p class="hint">
      Konto Jiry (adres, e-mail, token API) i tablice ustawiasz w widoku Jira.
    </p>
    <button class="action-button" @click="openView({ view: 'jira' })">
      Połączenie z Jirą
    </button>
    </template>
    <template v-if="section === 'calendar'">
    <h2>Kalendarz</h2>
    <CalendarSettings />
    <label class="inline-check"
      ><input
        :checked="settings.calendarShowTitles"
        type="checkbox"
        :disabled="busy"
        @change="save({ calendarShowTitles: checked($event) })"
      />Pokazuj tytuły spotkań</label
    >
    <p class="hint">
      Wyłączone: zamiast tytułu widać „Spotkanie”, np. przy udostępnianiu
      ekranu. Spotkania są tylko do odczytu; czas spotkania dodajesz sam.
    </p>
    </template>
    <template v-if="section === 'notifications'">
    <h2>Powiadomienia</h2>
    <label class="inline-check"
      ><input
        :checked="settings.notifyDue"
        type="checkbox"
        :disabled="busy"
        @change="save({ notifyDue: checked($event) })"
      />Przypominaj o terminach na dziś i przekroczonych</label
    >
    <div class="planning-fields settings-fields">
      <label
        >Długi pomiar bez Stop (minuty, 0 wyłącza)<input
          :value="settings.notifyLongMinutes"
          type="number"
          min="0"
          max="1440"
          step="15"
          :disabled="busy"
          @change="save({ notifyLongMinutes: number($event) })"
      /></label>
      <label
        >Wieczorne „pomiar nadal trwa”<input
          :value="settings.notifyEvening"
          type="time"
          :disabled="busy || !settings.notifyEveningEnabled"
          @change="save({ notifyEvening: text($event) })"
      /></label>
    </div>
    <label class="inline-check"
      ><input
        :checked="settings.notifyEveningEnabled"
        type="checkbox"
        :disabled="busy"
        @change="save({ notifyEveningEnabled: checked($event) })"
      />Przypominaj wieczorem, że pomiar nadal trwa</label
    >
    <label class="inline-check"
      ><input
        :checked="settings.notifyTarget"
        type="checkbox"
        :disabled="busy"
        @change="save({ notifyTarget: checked($event) })"
      />Powiadom o osiągnięciu celu dnia</label
    >
    <p class="hint">
      Powiadomienia Windows: terminy przy starcie i o 09:00, długi pomiar raz na
      przedział, wieczór o wybranej godzinie, cel raz dziennie. Każde pojawia się
      najwyżej raz, także po ponownym uruchomieniu. Potwierdzenie skrótu
      Start/Stop pokazuje się, gdy widget jest schowany.
    </p>
    </template>
    <template v-if="section === 'templates'">
    <h2>Szablony</h2>
    <p class="hint">
      Wpisz /nazwa w polu „Nowe zadanie” albo w szybkim przełączaniu, np.
      /review WMS-123. W tytule {klucz} to klucz Jiry, a {tytuł} to tytuł z
      linii albo zgłoszenia. Pola wpisane w linii (~, #, !) wygrywają z
      szablonem. Szablon zapiszesz też z rozwiniętego zadania.
    </p>
    <div class="template-list" aria-label="Szablony zadań">
      <TemplateEditor v-for="t in templates" :key="t.id" :template="t" />
      <p v-if="!templates.length && !addingTemplate" class="empty-detail">
        Brak szablonów.
      </p>
      <TemplateEditor
        v-if="addingTemplate"
        :template="null"
        @done="addingTemplate = false"
      />
    </div>
    <button
      v-if="!addingTemplate"
      class="action-button"
      @click="addingTemplate = true"
    >
      Nowy szablon
    </button>
    </template>
    <template v-if="section === 'shortcuts'">
    <h2>Skróty</h2>
    <p>
      Ctrl+K — szybkie przełączanie w aplikacji.<br />{{
        settings.shortcutQuick
      }}
      — szybkie przełączanie z dowolnego okna.<br />{{
        settings.shortcutToggle
      }}
      — Start/Stop z dowolnego okna: zatrzymuje pomiar albo wznawia ostatnio
      mierzone zadanie.
    </p>
    <p class="hint">
      Kliknij pole i naciśnij nową kombinację z Ctrl, Alt, Shift lub Win. Esc
      anuluje.
    </p>
    <ShortcutInput
      :model-value="settings.shortcutQuick"
      label="Szybkie przełączanie (globalnie)"
      :default-value="defaultSettings.shortcutQuick"
      :error="shortcutError('quick')"
      :disabled="busy"
      @update:model-value="saveShortcut('quick', $event)"
    />
    <ShortcutInput
      :model-value="settings.shortcutToggle"
      label="Start/Stop (globalnie)"
      :default-value="defaultSettings.shortcutToggle"
      :error="shortcutError('toggle')"
      :disabled="busy"
      @update:model-value="saveShortcut('toggle', $event)"
    />
    </template>
    <template v-if="section === 'startup'">
    <h2>Uruchamianie</h2>
    <label class="inline-check"
      ><input
        :checked="autostart.enabled"
        type="checkbox"
        :disabled="busy"
        @change="saveAutostart('enabled', $event)"
      />Uruchamiaj z Windows</label
    >
    <label class="inline-check"
      ><input
        :checked="autostart.hidden"
        type="checkbox"
        :disabled="busy || !autostart.enabled"
        @change="saveAutostart('hidden', $event)"
      />Uruchamiaj schowane do traya</label
    >
    <p class="hint">
      Przy starcie schowanym okno się nie pokazuje; otworzysz je ikoną w
      trayu. Wyłączenie wpisu w Menedżerze zadań też będzie tu widoczne.
    </p>
    </template>
    <small v-if="info.version">Todo Track {{ info.version }}</small>
  </section>
</template>
