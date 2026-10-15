<script setup lang="ts">
import { computed } from "vue";
import type { Session, Task } from "../store";
import { hhmm } from "../target";
import { sessionsSignature, useAgentTime } from "../composables/useAgentTime";

/**
 * Czas agentów w rozwiniętym kafelku (W1): Twój czas, czas agentów nad zadaniem
 * i część równoległa (agenci w czasie Twojego pomiaru tego zadania). Cała
 * zapisana historia (okresy starsze niż 400 dni są usuwane). Bez czasu agentów
 * sekcja się nie pokazuje.
 */
const props = defineProps<{ task: Task; sessions: Session[]; own: number }>();
/** Cała historia: od początku epoki do daleko w przyszłość. */
const ALL: [number, number] = [0, 8_640_000_000_000_000];
const agentTime = useAgentTime(
  () => ALL,
  () => sessionsSignature(props.sessions),
);
const total = computed(() =>
  agentTime.time.value.totals.find((t) => t.taskId === props.task.id),
);
</script>

<template>
  <section
    v-if="total"
    class="agent-task-time"
    :aria-label="'Czas agentów: ' + task.title"
  >
    <h3 class="details-heading">Czas agentów</h3>
    <table class="estimate-table">
      <thead>
        <tr>
          <th>Ty</th>
          <th title="Czas pracy agentów AI nad zadaniem">Agent</th>
          <th title="Część czasu agentów nakładająca się na Twój pomiar">
            Równolegle
          </th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td class="mono">{{ hhmm(own) }}</td>
          <td class="mono">{{ hhmm(total.agentMs) }}</td>
          <td class="mono">{{ hhmm(total.parallelMs) }}</td>
        </tr>
      </tbody>
    </table>
    <p class="hint">
      Osobna informacja: czas agentów nie wlicza się do celu, raportu TS ani
      eksportu.
    </p>
  </section>
</template>
