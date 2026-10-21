<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ConfirmDialog from "./ConfirmDialog.vue";
import { desktop, read, setStartGuard, type Snapshot } from "../store";
import {
  answerDay,
  askDay,
  confirmStart,
  dayEnd,
  dayQuestion,
  setDayEndHandler,
} from "../days";
import { openView } from "../navigation";
import { dayKey } from "../time";

/**
 * Dialogi dnia (B13) w oknie z interfejsem: „Wznowić dzień?” przed startem
 * pomiaru po zakończeniu dnia (strażnik `setStartGuard`), a w głównym oknie
 * (`main`) też „Zakończ dzień” z pytaniem o zatrzymanie trwającego pomiaru,
 * przejściem do Zamknięcia dnia i obsługą prośby z traya i widgetu.
 */
const props = defineProps<{ main?: boolean; data?: Snapshot }>();
const emit = defineEmits<{ updated: [data: Snapshot]; error: [message: string] }>();

async function endDay() {
  const data = props.data ?? (await read());
  const active = data.sessions.find((s) => s.endedAt === null);
  let stop = false;
  if (active) {
    const title = data.tasks.find((t) => t.id === active.taskId)?.title ?? "";
    if (!(await askDay({ kind: "end", title }))) return;
    stop = true;
  }
  emit("updated", await dayEnd(dayKey(Date.now()), stop));
  openView({ view: "journal", dayClose: true });
}

const unlisten = ref<UnlistenFn | null>(null);
onMounted(async () => {
  setStartGuard(() => confirmStart(props.data ?? null));
  if (!props.main) return;
  setDayEndHandler(endDay);
  if (desktop) {
    try {
      unlisten.value = await listen("day-end-request", () => {
        endDay().catch((e) =>
          emit("error", String(e instanceof Error ? e.message : e)),
        );
      });
    } catch (e) {
      emit("error", String(e));
    }
  }
});
onUnmounted(() => {
  setStartGuard(null);
  if (props.main) setDayEndHandler(null);
  unlisten.value?.();
  answerDay(false);
});
</script>

<template>
  <ConfirmDialog
    v-if="dayQuestion?.kind === 'end'"
    title="Zakończyć dzień?"
    @close="answerDay(false)"
  >
    <p>
      Zakończyć dzień i zatrzymać pomiar »{{ dayQuestion.title }}«? Pomiar
      zatrzyma się teraz, a potem otworzy się Zamknięcie dnia.
    </p>
    <div class="modal-actions">
      <button @click="answerDay(false)">Anuluj</button
      ><button class="primary-button" @click="answerDay(true)">
        Zakończ dzień i zatrzymaj pomiar
      </button>
    </div>
  </ConfirmDialog>
  <ConfirmDialog
    v-else-if="dayQuestion?.kind === 'start'"
    title="Wznowić dzień?"
    @close="answerDay(false)"
  >
    <p>
      Dzień jest zakończony. Wznowić go i rozpocząć pomiar? Przypomnienia
      i propozycje wrócą do końca dnia.
    </p>
    <div class="modal-actions">
      <button @click="answerDay(false)">Anuluj</button
      ><button class="primary-button" @click="answerDay(true)">
        Wznów i start
      </button>
    </div>
  </ConfirmDialog>
</template>
