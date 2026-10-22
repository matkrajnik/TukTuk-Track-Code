<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Maximize2, Minimize2, Minus, X } from "lucide-vue-next";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

const emit = defineEmits<{ error: [message: string] }>();
const maximized = ref(false);
let unlisten: UnlistenFn | undefined;

async function windowAction(action: "minimize" | "maximize" | "close") {
  try {
    const window = getCurrentWindow();
    if (action === "minimize") await window.minimize();
    else if (action === "close") await window.close();
    else {
      await window.toggleMaximize();
      maximized.value = await window.isMaximized();
    }
  } catch (e) {
    emit("error", String(e));
  }
}
onMounted(async () => {
  try {
    const window = getCurrentWindow();
    maximized.value = await window.isMaximized();
    unlisten = await window.onResized(async () => {
      maximized.value = await getCurrentWindow().isMaximized();
    });
  } catch (e) {
    emit("error", String(e));
  }
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <div class="titlebar">
    <div data-tauri-drag-region class="titlebar-drag">TukTuk Track</div>
    <div class="window-controls">
      <button
        aria-label="Minimalizuj okno"
        title="Minimalizuj na pasek zadań"
        @click="windowAction('minimize')"
      >
        <Minus :size="15" />
      </button>
      <button
        :aria-label="maximized ? 'Przywróć rozmiar okna' : 'Maksymalizuj okno'"
        @click="windowAction('maximize')"
      >
        <Minimize2 v-if="maximized" :size="14" /><Maximize2
          v-else
          :size="14"
        />
      </button>
      <button
        class="window-close"
        aria-label="Zamknij okno do traya"
        title="Schowaj do traya"
        @click="windowAction('close')"
      >
        <X :size="17" />
      </button>
    </div>
  </div>
</template>
