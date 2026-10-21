<script setup lang="ts">
/** Ustawienia → Czas pracy: bezczynność, propozycje, cel i dni robocze. */
import { computed } from "vue";
import BrowserExtensionStatus from "../BrowserExtensionStatus.vue";
import type { AppSettings } from "../../settings";
import { hm } from "../../time";
import { checked, number, text, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
const weekDays = [
  { day: 1, name: "Pon" },
  { day: 2, name: "Wt" },
  { day: 3, name: "Śr" },
  { day: 4, name: "Czw" },
  { day: 5, name: "Pt" },
  { day: 6, name: "Sob" },
  { day: 7, name: "Nd" },
];
/** Cel w minutach jako G:MM. */
const minutes = (value: number) => hm(value * 60_000);
const weekTarget = computed(() =>
  minutes(settings.value.targetDayMinutes * settings.value.workDays.length),
);
function toggleDay(day: number, on: boolean) {
  const days = settings.value.workDays.filter((d) => d !== day);
  void save({ workDays: on ? [...days, day] : days });
}
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
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
  <label class="inline-check"
    ><input
      :checked="settings.windowSuggest"
      type="checkbox"
      :disabled="busy"
      @change="save({ windowSuggest: checked($event) })"
    />Podpowiadaj zadanie na podstawie aktywnego okna</label
  >
  <p class="hint">
    Co 5 sekund aplikacja czyta tytuł i nazwę programu okna na pierwszym
    planie: klucz Jiry z przeglądarki, Teams lub Outlooka i folder projektu
    z VS Code, Cursora lub JetBrains. Tytuły zostają tylko w pamięci, nie
    trafiają do bazy ani logów. Propozycja przychodzi po 2 minutach przy tym
    samym zadaniu.
  </p>
  <label class="inline-check"
    ><input
      :checked="settings.browserExtension"
      type="checkbox"
      :disabled="busy"
      @change="save({ browserExtension: checked($event) })"
    />Rozszerzenie przeglądarki (Chrome i Edge)</label
  >
  <BrowserExtensionStatus :enabled="settings.browserExtension" />
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
    Cel dnia {{ minutes(settings.targetDayMinutes) }}, cel tygodnia
    {{ weekTarget }}.
  </p>
  <p v-if="settings.holidaysInTarget" class="hint">
    Polskie święta ustawowe, także Wielkanoc i Boże Ciało, nie są dniami
    roboczymi: cel tygodnia i miesiąca jest wtedy niższy.
  </p>
  <label class="inline-check"
    ><input
      :checked="settings.planWithCorrection"
      type="checkbox"
      :disabled="busy"
      @change="save({ planWithCorrection: checked($event) })"
    />Planuj z poprawką szacunków</label
  >
  <p class="hint">
    Poprawka to mediana stosunku czasu rzeczywistego do szacunku ukończonych
    zadań z 90 dni (projektu albo wszystkich, co najmniej 5 zadań). Bez tej
    opcji jest tylko podpowiedzią „zwykle ~…”; włączona liczy się w
    pojemności tablicy tygodnia, osi planu i „Rozłóż”.
  </p>
</template>
