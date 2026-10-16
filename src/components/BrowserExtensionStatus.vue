<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  browserExtensionId,
  browserOpenFolder,
  browserStatus,
  type BrowserStatus,
} from "../store";
import { stamp } from "../time";

/**
 * Ustawienia → Czas pracy → Rozszerzenie przeglądarki (W4): stan połączenia,
 * ostatni sygnał i instrukcja instalacji. Wyłącznik jest w SettingsPanel.
 */
const props = defineProps<{ enabled: boolean }>();
const status = ref<BrowserStatus>({
  enabled: false,
  registered: false,
  connected: false,
  lastMessage: null,
  lastKey: null,
  lastKeyAt: null,
  error: "",
  extensionId: browserExtensionId,
  extensionFolder: null,
});
const error = ref("");
const guide = ref(false);

async function load() {
  try {
    status.value = await browserStatus();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  }
}
async function openFolder() {
  error.value = "";
  try {
    await browserOpenFolder();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  }
}
/** „połączone / ostatni sygnał” albo powód braku połączenia. */
const line = computed(() => {
  const s = status.value;
  if (!props.enabled) return "";
  const signal =
    s.lastKey && s.lastKeyAt
      ? `ostatni sygnał: ${s.lastKey}, ${stamp(s.lastKeyAt)}`
      : "jeszcze bez sygnału ze zgłoszenia";
  if (s.connected) return `Połączone · ${signal}`;
  const last = s.lastMessage
    ? ` Ostatnia wiadomość: ${stamp(s.lastMessage)}.`
    : "";
  return `Niepołączone · ${signal}.${last}`;
});

let timer: number | undefined;
onMounted(() => {
  void load();
  // Stan zmienia się co kilka sekund (rozszerzenie wysyła stan co 10 s).
  timer = window.setInterval(() => void load(), 5_000);
});
onUnmounted(() => window.clearInterval(timer));
watch(
  () => props.enabled,
  () => void load(),
);
</script>
<template>
  <p class="hint">
    Rozszerzenie dla Chrome i Edge wysyła adres i tytuł aktywnej karty, ale
    tylko dla domen z listy w opcjach rozszerzenia (domyślnie *.atlassian.net).
    Rozpoznaje zgłoszenie z adresu (także tablicy z otwartym zgłoszeniem) i ma
    pierwszeństwo przed tytułem okna, gdy przeglądarka jest na pierwszym planie.
    Klucz zostaje tylko w pamięci przez 30 sekund. Włączenie zapisuje host
    native messaging w rejestrze użytkownika (bez uprawnień administratora),
    wyłączenie go usuwa.
  </p>
  <template v-if="enabled">
    <p
      class="browser-extension-status"
      :class="{ connected: status.connected }"
      role="status"
      aria-label="Stan rozszerzenia przeglądarki"
    >
      {{ line }}
    </p>
    <p v-if="status.error" class="error">{{ status.error }}</p>
  </template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <button
    class="text-link"
    :aria-expanded="guide"
    aria-controls="browser-extension-guide"
    @click="guide = !guide"
  >
    Jak zainstalować rozszerzenie?
  </button>
  <ol v-if="guide" id="browser-extension-guide" class="browser-extension-guide">
    <li>Włącz „Rozszerzenie przeglądarki” powyżej.</li>
    <li>
      W Chrome otwórz chrome://extensions, w Edge edge://extensions i włącz
      „Tryb dewelopera”.
    </li>
    <li>
      Kliknij „Załaduj rozpakowane” i wskaż folder
      <code class="mono">{{
        status.extensionFolder ?? "browser-extension"
      }}</code
      >.
      <button class="text-link" @click="openFolder">
        Otwórz folder rozszerzenia
      </button>
    </li>
    <li>
      W firmie administrator może zainstalować je zasadami grupy (identyfikator
      <code class="mono">{{ status.extensionId }}</code
      >).
    </li>
  </ol>
</template>
