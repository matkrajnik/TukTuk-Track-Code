<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import ConfirmDialog from "./ConfirmDialog.vue";
import { desktop, read, setStartGuard, type Snapshot } from "../store";
import {
  answerDay,
  answerStartRequest,
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
const props = defineProps<{
  main?: boolean;
  data?: Snapshot;
  /** Strażnika startu rejestruje rodzic (widget w podglądzie), tu tylko dialogi. */
  dialogsOnly?: boolean;
}>();
const emit = defineEmits<{ updated: [data: Snapshot]; error: [message: string] }>();
const fail = (e: unknown) =>
  emit("error", String(e instanceof Error ? e.message : e));

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

const unlisten: UnlistenFn[] = [];
onMounted(async () => {
  if (!props.dialogsOnly)
    setStartGuard(() =>
      confirmStart(props.data ?? null, (data) => emit("updated", data)),
    );
  if (!props.main) return;
  setDayEndHandler(endDay);
  if (desktop) {
    try {
      unlisten.push(
        await listen("day-end-request", () => {
          endDay().catch(fail);
        }),
      );
      // Start z traya, skrótu, widgetu, trybu skupienia albo powiadomienia
      // po zakończeniu dnia: pytanie tutaj, start w backendzie.
      unlisten.push(
        await listen<number>("day-start-request", (e) => {
          answerStartRequest(e.payload)
            .then((data) => emit("updated", data))
            .catch(fail);
        }),
      );
    } catch (e) {
      emit("error", String(e));
    }
  }
});
onUnmounted(() => {
  if (!props.dialogsOnly) setStartGuard(null);
  if (props.main) setDayEndHandler(null);
  unlisten.forEach((fn) => fn());
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
