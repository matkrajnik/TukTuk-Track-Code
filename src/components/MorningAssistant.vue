<script setup lang="ts">
/**
 * „Komentarz asystenta” w Rano (PLAN_BACKLOG.md, A6): krótka ocena dzisiejszego
 * planu względem pojemności dnia i terminów. Jedyne, co woła model, to kliknięcie
 * przycisku (nigdy samo). Komentarz niczego nie zmienia w planie; propozycje
 * przesunięć są kartami „Zastosuj” / „Odrzuć” jak w czacie (A3), a zapisu
 * dokonuje dopiero „Zastosuj”, istniejącą komendą `plan_task`.
 */
import { computed, reactive, ref, watch } from "vue";
import { Bot } from "lucide-vue-next";
import AiPreview from "./AiPreview.vue";
import {
  aiMorningComment,
  aiMorningPreview,
  correctionTable,
  type MorningComment,
  type MorningMove,
} from "../aiplan";
import { planTask, type Snapshot } from "../store";
import { nextOrder } from "../review";
import { dayText } from "../time";
import { corrections } from "../estimates";
import { useSettings } from "../target";

const props = defineProps<{ data: Snapshot; day: string }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const settings = useSettings();

/**
 * Poprawka szacunków jak w pasku pojemności Rano: przy „Planuj z poprawką
 * szacunków” model dostaje szacunki po poprawce, inaczej surowe.
 */
function correction() {
  if (!settings.value.planWithCorrection) return null;
  return correctionTable(props.data, corrections(props.data, Date.now()));
}

const result = ref<MorningComment | null>(null);
const loading = ref(false);
const error = ref("");
const busy = ref(false);
/** Karty już zastosowane albo odrzucone (po numerze propozycji). */
const applied = reactive<Record<number, boolean>>({});
const dismissed = reactive<Record<number, boolean>>({});
const cardError = reactive<Record<number, string>>({});
let request = 0;

function reset() {
  request++;
  result.value = null;
  loading.value = false;
  error.value = "";
  clearCards();
}
function clearCards() {
  for (const map of [applied, dismissed, cardError])
    for (const key of Object.keys(map)) delete map[Number(key)];
}
watch(() => props.day, reset);

const previewLoad = computed(() => () => aiMorningPreview(props.day, correction()));

async function ask() {
  if (loading.value || busy.value) return;
  const id = ++request;
  loading.value = true;
  error.value = "";
  try {
    const answer = await aiMorningComment(props.day, correction());
    if (id !== request) return;
    clearCards();
    result.value = answer;
  } catch (e) {
    if (id !== request) return;
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    if (id === request) loading.value = false;
  }
}

const visible = computed(() =>
  (result.value?.moves ?? []).filter((m) => !dismissed[m.id]),
);

/** Zadanie nadal otwarte i w planie dnia, z którego propozycja schodzi. */
function stillPlanned(move: MorningMove) {
  const task = props.data.tasks.find((t) => t.id === move.taskId);
  return !!task && !task.done && task.details?.plannedDay === move.fromDay;
}

/** „Zastosuj”: przenosi zadanie na proponowany dzień, na koniec jego planu. */
async function apply(move: MorningMove) {
  if (busy.value || loading.value) return;
  delete cardError[move.id];
  if (!stillPlanned(move)) {
    cardError[move.id] =
      "Plan tego zadania już się zmienił. Poproś o nowy komentarz.";
    return;
  }
  busy.value = true;
  // Karta należy do tego wyniku; nowy komentarz (albo inny dzień) podmienia
  // `result`, a wtedy stan karty o tym samym numerze nie może się zmienić.
  const owner = result.value;
  const current = () => owner !== null && result.value === owner;
  try {
    const data = await planTask(
      move.taskId,
      move.toDay,
      nextOrder(props.data.tasks, move.toDay),
    );
    emit("updated", data);
    if (current()) applied[move.id] = true;
  } catch (e) {
    if (current()) cardError[move.id] = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <section class="morning-assist" aria-label="Asystent planu dnia">
    <h4>Asystent</h4>
    <p class="hint">
      Asystent ocenia plan względem pojemności dnia i terminów i tylko
      proponuje. Plan zmieni się dopiero po kliknięciu „Zastosuj”.
    </p>
    <div class="close-assist-actions">
      <button
        type="button"
        class="secondary-button"
        :disabled="loading || busy"
        @click="ask"
      >
        <Bot :size="14" />{{ loading ? "Oceniam plan…" : "Komentarz asystenta" }}
      </button>
    </div>
    <AiPreview :load="previewLoad" />
    <p v-if="error" class="error" role="alert">{{ error }}</p>
    <p
      v-if="result && !result.comment && !result.moves.length"
      role="status"
    >
      {{ result.message }}
    </p>
    <p v-if="result?.comment" class="assist-comment" aria-label="Komentarz">
      {{ result.comment }}
    </p>
    <ul v-if="visible.length" class="assist-cards" aria-label="Propozycje przesunięć">
      <li v-for="m in visible" :key="m.id" class="assist-card" data-kind="move">
        <header>
          <strong>Przesuń „{{ m.taskTitle }}”</strong>
          <span class="assist-card-kind">
            dziś → {{ dayText(m.toDay) }}
          </span>
        </header>
        <p v-if="m.reason" class="assist-card-reason">{{ m.reason }}</p>
        <p v-if="cardError[m.id]" class="error" role="alert">
          {{ cardError[m.id] }}
        </p>
        <p v-if="applied[m.id]" class="assist-card-done" role="status">
          Zastosowano.
        </p>
        <div v-else class="assist-card-actions">
          <button
            type="button"
            class="secondary-button"
            :disabled="busy || loading"
            :aria-label="'Zastosuj: ' + m.taskTitle + ' na ' + dayText(m.toDay)"
            @click="apply(m)"
          >
            Zastosuj
          </button>
          <button
            type="button"
            class="today-button"
            :disabled="busy || loading"
            :aria-label="'Odrzuć propozycję: ' + m.taskTitle"
            @click="dismissed[m.id] = true"
          >
            Odrzuć
          </button>
        </div>
      </li>
    </ul>
    <p
      v-else-if="result && result.moves.length"
      role="status"
    >
      Wszystkie propozycje odrzucono.
    </p>
  </section>
</template>
