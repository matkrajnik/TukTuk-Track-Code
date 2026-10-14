<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { desktop, type Snapshot, type Task } from "../store";

defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const idleNotice = ref<{
  taskId: number;
  idleAt: number;
  returnedAt: number;
} | null>(null);
let unlisten: UnlistenFn | undefined;

async function readIdle() {
  idleNotice.value = await invoke("idle_notice");
}
async function resolveIdle(trim: boolean) {
  try {
    emit("updated", await invoke("idle_resolve", { trim }));
    idleNotice.value = null;
  } catch (e) {
    emit("error", String(e));
  }
}
onMounted(async () => {
  if (!desktop) return;
  try {
    unlisten = await listen("idle-notice", readIdle);
    await readIdle();
  } catch {}
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <section v-if="idleNotice" class="idle-banner" role="alert">
    <h2>Wracasz po przerwie?</h2>
    <p>
      {{ tasks.find((t) => t.id === idleNotice?.taskId)?.title }} · brak
      aktywności od
      {{ new Date(idleNotice.idleAt).toLocaleTimeString("pl-PL") }}. Czy cały
      ten czas był pracą?
    </p>
    <div class="planning-actions">
      <button class="today-button" @click="resolveIdle(false)">
        Zachowaj cały czas</button
      ><button class="today-button" @click="resolveIdle(true)">
        Zatrzymaj na początku przerwy
      </button>
    </div>
  </section>
</template>
