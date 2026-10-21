<script setup lang="ts">
/** „Przedziały pracy” w rozwiniętym kafelku: lista z czasem i edycją. */
import { Pencil } from "lucide-vue-next";
import { duration, type Session } from "../../store";
import { stamp } from "../../time";

const props = defineProps<{
  /** Przedziały zadania, od najnowszego. */
  sessions: Session[];
  running: boolean;
  busy: boolean;
  now: number;
}>();
const emit = defineEmits<{ session: [session: Session] }>();
const elapsed = (s: Session) =>
  Math.max(0, (s.endedAt ?? props.now) - s.startedAt);
</script>

<template>
  <h3 class="details-heading">Przedziały pracy</h3>
  <div v-for="session in sessions" :key="session.id" class="session-row">
    <div class="interval">
      <time :datetime="new Date(session.startedAt).toISOString()">{{
        stamp(session.startedAt)
      }}</time
      ><span class="interval-arrow">→</span
      ><time
        v-if="session.endedAt !== null"
        :datetime="new Date(session.endedAt).toISOString()"
        >{{ stamp(session.endedAt) }}</time
      ><span v-else class="running-label">W trakcie</span>
    </div>
    <span class="mono session-duration">{{
      duration(elapsed(session))
    }}</span>
    <button
      class="icon-button"
      :aria-label="'Edytuj przedział ' + session.id"
      :disabled="busy || session.endedAt === null"
      :title="
        session.endedAt === null
          ? 'Zatrzymaj pomiar przed edycją'
          : 'Edytuj przedział'
      "
      @click="emit('session', session)"
    >
      <Pencil :size="14" />
    </button>
  </div>
  <p v-if="!sessions.length" class="empty-detail">
    Brak przedziałów. Kliknij Start, aby rozpocząć pomiar.
  </p>
  <p v-if="running" class="hint">
    Zatrzymaj bieżący pomiar, aby edytować jego godziny.
  </p>
</template>
