<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Bell, X } from "lucide-vue-next";
const busy = ref(false),
  error = ref("");
function updateTheme() {
  try {
    document.documentElement.dataset.theme =
      localStorage.getItem("tuktuk-track-theme") === "light" ? "light" : "dark";
  } catch {}
}
updateTheme();
let unlisten: UnlistenFn | undefined;
onMounted(async () => {
  window.addEventListener("storage", updateTheme);
  unlisten = await listen("reminder-shown", () => {
    error.value = "";
    updateTheme();
  });
});
onUnmounted(() => {
  window.removeEventListener("storage", updateTheme);
  unlisten?.();
});
async function action(action: string) {
  if (busy.value) return;
  busy.value = true;
  try {
    await invoke("reminder_action", { action });
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <section class="reminder-card" aria-label="Przypomnienie o zadaniu">
    <header>
      <Bell :size="19" /><span>TukTuk Track</span
      ><button
        class="icon-button"
        aria-label="Przypomnij za 5 minut"
        :disabled="busy"
        @click="action('snooze')"
      >
        <X :size="16" />
      </button>
    </header>
    <h1>Nad czym teraz pracujesz?</h1>
    <p>Od 5 minut nie mierzysz czasu. Wpisz zadanie lub wybierz je z listy.</p>
    <p v-if="error" class="error">{{ error }}</p>
    <footer>
      <button
        class="secondary-button"
        :disabled="busy"
        @click="action('snooze')"
      >
        Za 5 minut</button
      ><button class="primary-button" :disabled="busy" @click="action('open')">
        Wpisz zadanie
      </button>
    </footer>
  </section>
</template>
