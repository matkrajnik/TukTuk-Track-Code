<script setup lang="ts">
/** Ustawienia → Uruchamianie: autostart z Windows i aktualizacje. */
import { onMounted, onUnmounted, ref } from "vue";
import { RefreshCw } from "lucide-vue-next";
import { checkForUpdate } from "../../updater";
import type { AppSettings } from "../../settings";
import { readAutostart, writeAutostart, type Autostart } from "../../store";
import { readLegacyStatus } from "../../legacyapi";
import { checked, message, useSettingsSection } from "./section";
const settings = defineModel<AppSettings>("settings", { required: true });
const { busy, error, save } = useSettingsSection(settings);
/** Wersja aplikacji do komunikatu „Masz najnowszą wersję”. */
const props = defineProps<{ version: string }>();
const autostart = ref<Autostart>({ enabled: false, hidden: false });
/** Autostart: stan z rejestru, „schowane” jako ustawienie autostartHidden. */
async function saveAutostart(field: keyof Autostart, e: Event) {
  const input = e.target as HTMLInputElement;
  const next = { ...autostart.value, [field]: input.checked };
  busy.value = true;
  error.value = "";
  try {
    autostart.value = await writeAutostart(next.enabled, next.hidden);
    settings.value = {
      ...settings.value,
      autostartHidden: autostart.value.hidden,
    };
  } catch (err) {
    error.value = message(err);
    autostart.value = await readAutostart().catch(() => autostart.value);
    // Odczytana wartość bywa taka sama jak przed kliknięciem, więc Vue nie
    // poprawi pola samo; cofamy je ręcznie do stanu z rejestru i ustawień.
    input.checked = autostart.value[field];
  } finally {
    busy.value = false;
  }
}

/** „Sprawdź teraz” (Aktualizacje): wynik pod przyciskiem. */
const updateBusy = ref(false),
  updateMessage = ref("");
async function checkNow() {
  updateBusy.value = true;
  updateMessage.value = "";
  try {
    const result = await checkForUpdate();
    updateMessage.value =
      result.status === "latest" && props.version
        ? `Masz najnowszą wersję (${props.version}).`
        : result.message;
  } finally {
    updateBusy.value = false;
  }
}
/** Wynik migracji z poprzedniej nazwy aplikacji (puste: nic do pokazania). */
const legacyMessage = ref("");
let alive = true;
onMounted(async () => {
  readLegacyStatus()
    .then((status) => {
      if (alive) legacyMessage.value = status?.message ?? "";
    })
    .catch(() => {});
  try {
    const value = await readAutostart();
    if (alive) autostart.value = value;
  } catch (e) {
    if (alive) error.value = String(e);
  }
});
onUnmounted(() => {
  alive = false;
});
</script>
<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <h2>Uruchamianie</h2>
  <p v-if="legacyMessage" class="hint" role="status" aria-label="Przeniesienie danych">
    {{ legacyMessage }}
  </p>
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
  <label class="inline-check"
    ><input
      :checked="settings.openMorningOnStart"
      type="checkbox"
      :disabled="busy"
      @change="save({ openMorningOnStart: checked($event) })"
    />Otwieraj plan dnia przy pierwszym uruchomieniu</label
  >
  <p class="hint">
    W dzień roboczy, dopóki nie klikniesz „Rozpocznij dzień”, pierwsze
    uruchomienie albo pokazanie okna otwiera Plan i historia → Rano. Raz
    dziennie; nie w święto ani w całodniowy urlop z kalendarza.
  </p>
  <h3>Aktualizacje</h3>
  <label class="inline-check"
    ><input
      :checked="settings.autoUpdateCheck"
      type="checkbox"
      :disabled="busy"
      @change="save({ autoUpdateCheck: checked($event) })"
    />Sprawdzaj aktualizacje automatycznie</label
  >
  <p class="hint">
    Przy starcie i raz dziennie aplikacja pyta serwer wydań o nową wersję.
    Pobranie i instalacja zaczynają się dopiero po kliknięciu w banerze.
  </p>
  <div class="planning-actions">
    <button
      type="button"
      class="secondary-button"
      :disabled="updateBusy"
      @click="checkNow"
    >
      <RefreshCw :size="14" />{{
        updateBusy ? "Sprawdzanie…" : "Sprawdź teraz"
      }}
    </button>
  </div>
  <p v-if="updateMessage" class="hint" role="status">
    {{ updateMessage }}
  </p>
</template>
