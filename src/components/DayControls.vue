<script setup lang="ts">
import { computed, ref } from "vue";
import { Play, Sunrise, Sunset } from "lucide-vue-next";
import { resumeCandidate, type Snapshot } from "../store";
import {
  dayMarkOf,
  dayStart,
  dayStateOf,
  requestDayEnd,
  resumeDay,
} from "../days";
import { clock, dayKey, hm, inDay } from "../time";
import { useSettings } from "../target";

/**
 * Przyciski stanu dnia (B13) w Rano i w Zamknięciu dnia: nie rozpoczęty →
 * „Rozpocznij dzień” (tylko z `start`, w Rano), rozpoczęty → „Zakończ dzień”,
 * zakończony dziś → „Wznów dzień” i opcjonalnie „Wznów pomiar »X«”.
 * Dotyczą wyłącznie dzisiejszego dnia.
 */
const props = defineProps<{ data: Snapshot; now: number; start?: boolean }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const settings = useSettings();
const busy = ref(false),
  error = ref(""),
  message = ref("");

const today = computed(() => dayKey(props.now));
const mark = computed(() => dayMarkOf(props.data, today.value));
const state = computed(() =>
  dayStateOf(mark.value, settings.value.morningReviewDay === today.value),
);
const last = computed(() =>
  props.data.sessions.some((s) => s.endedAt === null)
    ? undefined
    : resumeCandidate(props.data),
);
/** „Dzień zakończony 16:40 · 7:55” jak w trayu i widgecie. */
const endedText = computed(() => {
  const ended = mark.value?.endedAt;
  if (ended == null) return "";
  const total = props.data.sessions.reduce(
    (sum, s) => sum + inDay(s, today.value, props.now),
    0,
  );
  return `Dzień zakończony ${clock(ended)} · ${hm(total)}`;
});

async function run(action: () => Promise<Snapshot | void>, done = "") {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    const result = await action();
    if (result) emit("updated", result);
    message.value = done;
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const begin = () =>
  run(() => dayStart(today.value), "Dzień rozpoczęty. Miłej pracy!");
const end = () => run(() => requestDayEnd());
const resume = (taskId: number | null = null) =>
  run(
    () => resumeDay(taskId),
    taskId == null ? "Dzień wznowiony." : "Dzień wznowiony, pomiar trwa.",
  );
</script>

<template>
  <div class="day-controls" role="group" aria-label="Stan dnia">
    <span v-if="state === 'ended'" class="hint day-ended" role="status">{{
      endedText
    }}</span>
    <button
      v-if="state === 'none' && start"
      class="primary-button"
      :disabled="busy"
      @click="begin"
    >
      <Sunrise :size="16" aria-hidden="true" />Rozpocznij dzień
    </button>
    <button
      v-else-if="state !== 'ended'"
      class="secondary-button"
      :disabled="busy"
      @click="end"
    >
      <Sunset :size="16" aria-hidden="true" />Zakończ dzień
    </button>
    <template v-else>
      <button class="secondary-button" :disabled="busy" @click="resume()">
        <Sunrise :size="16" aria-hidden="true" />Wznów dzień</button
      ><button
        v-if="last"
        class="secondary-button"
        :disabled="busy"
        :aria-label="`Wznów pomiar »${last.title}«`"
        @click="resume(last.id)"
      >
        <Play :size="14" aria-hidden="true" />Wznów pomiar »{{ last.title }}«
      </button>
    </template>
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p v-if="message" class="hint" role="status">{{ message }}</p>
  </div>
</template>
