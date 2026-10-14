<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import ShortcutInput from "./ShortcutInput.vue";
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
    <p class="hint">
      Cel dnia {{ hm(settings.targetDayMinutes) }}, cel tygodnia
      {{ weekTarget }}.
    </p>

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
    <p class="hint">Mini-widget pojawi się w jednej z kolejnych wersji.</p>

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
          :disabled="busy"
          @change="save({ notifyEvening: text($event) })"
      /></label>
    </div>
    <p class="hint">Powiadomienia Windows pojawią się w jednej z kolejnych wersji.</p>

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

    <h2>Uruchamianie</h2>
    <label class="inline-check"
      ><input
        :checked="settings.autostartHidden"
        type="checkbox"
        :disabled="busy"
        @change="save({ autostartHidden: checked($event) })"
      />Przy autostarcie uruchamiaj schowane do traya</label
    >
    <p class="hint">Uruchamianie z Windows pojawi się w jednej z kolejnych wersji.</p>
    <small v-if="info.version">Todo Track {{ info.version }}</small>
  </section>
</template>
