<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { PowerOff } from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { desktop, type Snapshot, type Task } from "../store";
import { clock, dayKey, dayText } from "../time";

/** Pomiar zatrzymany przy starcie po przerwie w pracy aplikacji (`gap::GapNotice`). */
interface GapNotice {
  sessionId: number;
  taskId: number;
  stoppedAt: number;
  restartedAt: number;
}
type GapAction = "keep" | "restore" | "resume";

const props = defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const notice = ref<GapNotice | null>(null);
const busy = ref(false);
let unlisten: UnlistenFn | undefined;

/** Godzina; gdy przerwa obejmuje różne dni, z dniem („śr., 14 paź, 17:30”). */
function moment(time: number): string {
  const n = notice.value;
  if (!n || dayKey(n.stoppedAt) === dayKey(n.restartedAt)) return clock(time);
  return `${dayText(dayKey(time))}, ${clock(time)}`;
}
const sentence = computed(() => {
  const n = notice.value;
  if (!n) return "";
  const title = props.tasks.find((t) => t.id === n.taskId)?.title;
  const measurement = title ? `Pomiar „${title}”` : "Pomiar";
  return `Aplikacja nie działała od ${moment(n.stoppedAt)} do ${moment(n.restartedAt)}. ${measurement} zatrzymano o ${clock(n.stoppedAt)}.`;
});
async function read() {
  notice.value = await invoke<GapNotice | null>("gap_notice");
}
/** Odpowiedź z banera (`gap::resolve`); błąd zostawia baner. */
async function resolve(action: GapAction) {
  busy.value = true;
  try {
    emit("updated", await invoke<Snapshot>("gap_resolve", { action }));
    notice.value = null;
    // Baner znika razem z klikniętym przyciskiem: fokus na treść (WCAG 2.4.3).
    await nextTick();
    document.getElementById("main")?.focus();
  } catch (e) {
    emit("error", String(e));
  } finally {
    busy.value = false;
  }
}
onMounted(async () => {
  if (!desktop) return;
  try {
    unlisten = await listen("gap-notice", read);
    await read();
  } catch {}
});
onUnmounted(() => unlisten?.());
</script>

<template>
  <section
    v-if="notice"
    class="idle-banner gap-banner infobar-warning"
    role="alert"
  >
    <PowerOff class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Pomiar zatrzymany po przerwie</h2>
    <p>{{ sentence }}</p>
    <div class="planning-actions">
      <button class="primary-button" :disabled="busy" @click="resolve('keep')">
        OK
      </button>
      <button
        class="secondary-button"
        :disabled="busy"
        @click="resolve('resume')"
      >
        Wznów pomiar
      </button>
      <button
        class="secondary-button"
        :disabled="busy"
        @click="resolve('restore')"
      >
        Przywróć cały czas
      </button>
    </div>
  </section>
</template>
