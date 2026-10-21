<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Plug, RefreshCw, Unplug, X } from "lucide-vue-next";
import {
  calendarConnect,
  calendarDisconnect,
  calendarEvents,
  calendarRefresh,
  calendarStatus,
  meetingOccupies,
  onCalendarChanged,
  type CalendarStatus,
} from "../store";
import { dayBounds, dayKey, stamp } from "../time";

/** Ustawienia → Kalendarz: konto Microsoft 365 i pobieranie spotkań. */
const status = ref<CalendarStatus>({
  clientId: "",
  tenant: "organizations",
  email: "",
  connected: false,
  connecting: false,
  lastSync: null,
  lastError: "",
});
const clientId = ref("");
const tenant = ref("organizations");
const busy = ref(false);
const connecting = ref(false);
const error = ref("");
const today = ref(0);
const enabled = computed(() => !!status.value.clientId);

async function load(fields = false) {
  status.value = await calendarStatus();
  if (fields) {
    clientId.value = status.value.clientId;
    tenant.value = status.value.tenant || "organizations";
  }
  const [from, to] = dayBounds(dayKey(Date.now()));
  today.value = status.value.connected
    ? (await calendarEvents(from, to)).filter(meetingOccupies).length
    : 0;
}
async function run(action: () => Promise<CalendarStatus>) {
  busy.value = true;
  error.value = "";
  try {
    status.value = await action();
    await load();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    await load().catch(() => {});
  } finally {
    busy.value = false;
  }
}
async function connect() {
  connecting.value = true;
  try {
    await run(() => calendarConnect(clientId.value, tenant.value));
  } finally {
    connecting.value = false;
  }
}
/** Przerywa logowanie albo rozłącza konto (ta sama komenda w Rust). */
const disconnect = () => run(calendarDisconnect);
const refresh = () => run(calendarRefresh);

let unlisten: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  try {
    const stop = await onCalendarChanged(() => void load().catch(() => {}));
    if (!alive) return stop();
    unlisten = stop;
    await load(true);
  } catch (e) {
    if (alive) error.value = String(e instanceof Error ? e.message : e);
  }
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
});
</script>
<template>
  <p class="hint">
    Spotkania z kalendarza Microsoft 365 (Outlook) pokażą, ile czasu zostaje na
    zadania. Potrzebna jest rejestracja aplikacji w Entra ID (opis w README):
    platforma „Mobile and desktop”, przekierowanie http://localhost, uprawnienia
    User.Read, Calendars.Read i offline_access.
  </p>
  <p v-if="!enabled && !status.connected" class="hint calendar-off">
    Kalendarz jest wyłączony: podaj identyfikator aplikacji (client ID).
  </p>
  <div class="planning-fields settings-fields">
    <label
      >Identyfikator aplikacji (client ID)<input
        v-model="clientId"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="00000000-0000-0000-0000-000000000000"
        :disabled="busy || status.connected"
    /></label>
    <label
      >Dzierżawca<input
        v-model="tenant"
        type="text"
        spellcheck="false"
        autocomplete="off"
        placeholder="organizations"
        :disabled="busy || status.connected"
    /></label>
  </div>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <div class="planning-actions">
    <template v-if="status.connected">
      <button class="action-button" :disabled="busy" @click="refresh">
        <RefreshCw :size="14" />Odśwież
      </button>
      <button class="today-button" :disabled="busy" @click="disconnect">
        <Unplug :size="14" />Rozłącz
      </button>
    </template>
    <template v-else-if="connecting || status.connecting">
      <span class="hint">Czekam na logowanie w przeglądarce (do 5 minut)…</span>
      <button class="today-button" @click="disconnect">
        <X :size="14" />Anuluj
      </button>
    </template>
    <button
      v-else
      class="action-button"
      :disabled="busy"
      @click="connect"
    >
      <Plug :size="14" />Połącz konto Microsoft
    </button>
  </div>
  <template v-if="status.connected">
    <p class="calendar-account">Połączono jako {{ status.email }}</p>
    <p class="hint">
      Ostatnie pobranie:
      {{ status.lastSync ? stamp(status.lastSync) : "jeszcze nie" }} ·
      spotkania dziś: {{ today }}. Spotkania pobierają się co 10 minut, od
      poniedziałku poprzedniego tygodnia do 3 tygodni naprzód.
    </p>
  </template>
  <p v-if="status.lastError" class="error calendar-error">
    Błąd pobierania: {{ status.lastError }}
  </p>
</template>
