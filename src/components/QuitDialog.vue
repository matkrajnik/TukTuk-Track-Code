<script setup lang="ts">
/**
 * „Zakończ aplikację” z traya w trakcie kroku przebiegu Workflows (decyzja po
 * E7): tray (`quit::request_quit`) zapamiętuje pytanie, pokazuje główne okno i
 * wysyła `app-quit-request` z nazwą procesu. Po rejestracji nasłuchu okno odpytuje
 * też oczekujące pytanie (`app_quit_pending`), bo zdarzenie wysłane przed nią
 * ginie. „Zakończ” woła `app_quit_confirmed` (błąd zatrzymania pomiaru pokazuje
 * się w oknie i odblokowuje przyciski), „Anuluj” i Esc czyszczą pytanie
 * (`app_quit_cancel`), a fokus wraca tam, gdzie był. Jedno wywołanie naraz:
 * podwójne kliknięcie nie wysyła drugiego.
 */
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ConfirmDialog from "./ConfirmDialog.vue";
import {
  QUIT_REQUEST_EVENT,
  desktop,
  quitAppCancel,
  quitAppConfirmed,
  quitAppPending,
} from "../store";

const workflow = ref<string | null>(null);
const busy = ref(false);
const error = ref("");
let unlisten: UnlistenFn | null = null;
let alive = true;

function ask(name: string | null) {
  if (name === null || busy.value) return;
  error.value = "";
  workflow.value = name || "bez nazwy";
}

function close() {
  if (busy.value) return;
  workflow.value = null;
  error.value = "";
  quitAppCancel().catch((e) =>
    console.warn("Nie udało się anulować pytania o zakończenie aplikacji:", e),
  );
}

async function confirm() {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    // Po sukcesie aplikacja się kończy: okno zostaje zajęte, bez drugiego wywołania.
    await quitAppConfirmed();
  } catch (e) {
    error.value = `Nie udało się zakończyć aplikacji: ${e instanceof Error ? e.message : String(e)}`;
    busy.value = false;
  }
}

onMounted(async () => {
  if (!desktop) return;
  try {
    const stop = await listen<string>(QUIT_REQUEST_EVENT, (e) => ask(e.payload));
    if (alive) unlisten = stop;
    else stop();
  } catch (e) {
    console.warn("Nie udało się nasłuchiwać pytania o zakończenie aplikacji:", e);
  }
  try {
    const pending = await quitAppPending();
    if (alive) ask(pending);
  } catch (e) {
    console.warn("Nie udało się odczytać pytania o zakończenie aplikacji:", e);
  }
});

onUnmounted(() => {
  alive = false;
  unlisten?.();
  unlisten = null;
});
</script>

<template>
  <ConfirmDialog
    v-if="workflow !== null"
    title="Zakończyć aplikację?"
    confirm-label="Zakończ"
    :error="error"
    :busy="busy"
    @close="close"
    @confirm="confirm"
  >
    <p>
      Trwa krok przebiegu „{{ workflow }}”. Zakończenie aplikacji przerwie go, a
      przebieg zostanie oznaczony jako przerwany. Zakończyć?
    </p>
  </ConfirmDialog>
</template>
