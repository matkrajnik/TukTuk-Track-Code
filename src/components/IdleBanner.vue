<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Clock } from "lucide-vue-next";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { desktop, type Snapshot, type Task } from "../store";
import type { AgentTask } from "../agents";

defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const idleNotice = ref<{
  taskId: number;
  idleAt: number;
  /** Ostatnie wejście użytkownika: od tej chwili liczy się „Przypisz przerwę do X”. */
  awaySince?: number;
  returnedAt: number;
  /** Zadania, nad którymi agenci pracowali podczas nieobecności (`idle::Notice`). */
  agentTasks?: AgentTask[];
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
/** „Przypisz przerwę do X”: podział pomiaru w chwili początku przerwy. */
async function assignBreak(task: AgentTask) {
  try {
    emit(
      "updated",
      await invoke("idle_resolve", { trim: false, assign: task.taskId }),
    );
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
  <section v-if="idleNotice" class="idle-banner infobar-warning" role="alert">
    <Clock class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>Wracasz po przerwie?</h2>
    <p>
      {{ tasks.find((t) => t.id === idleNotice?.taskId)?.title }} · brak
      aktywności od
      {{ new Date(idleNotice.idleAt).toLocaleTimeString("pl-PL") }}. Czy cały
      ten czas był pracą?
    </p>
    <p v-if="idleNotice.agentTasks?.length" class="hint">
      W tym czasie agent AI pracował nad innym zadaniem. Przerwę możesz
      przypisać do niego: pomiar zostanie podzielony o
      {{
        new Date(idleNotice.awaySince ?? idleNotice.idleAt).toLocaleTimeString(
          "pl-PL",
        )
      }}
      (ostatnia aktywność przy komputerze).
    </p>
    <div class="planning-actions">
      <button class="secondary-button" @click="resolveIdle(false)">
        Zachowaj cały czas</button
      ><button class="secondary-button" @click="resolveIdle(true)">
        Zatrzymaj na początku przerwy</button
      ><button
        v-for="task in idleNotice.agentTasks ?? []"
        :key="task.taskId ?? ''"
        class="secondary-button"
        @click="assignBreak(task)"
      >
        Przypisz przerwę do „{{ task.title }}”
      </button>
    </div>
  </section>
</template>
