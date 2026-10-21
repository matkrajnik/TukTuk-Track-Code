<script setup lang="ts">
/**
 * Wybór tablic Jiry (najwyżej 20) z wyszukiwaniem i stronicowaniem, z własnym
 * `busy` i błędem. Lista tablic ładuje się po otwarciu.
 */
import { onMounted, ref } from "vue";
import { jiraBoards, jiraSelectBoards, type JiraBoardRef } from "../../store";

/** Tablice wybrane do tej pory (początek wyboru). */
const props = defineProps<{ selected: JiraBoardRef[] }>();
const emit = defineEmits<{ saved: [boards: JiraBoardRef[]]; cancel: [] }>();
const boardName = ref(""),
  boards = ref<JiraBoardRef[]>([]),
  chosen = ref<JiraBoardRef[]>([...props.selected]),
  nextStart = ref<number | null>(null);
const busy = ref(false),
  error = ref("");
async function run(fn: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  try {
    await fn();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
async function fetchBoards(more: boolean) {
  const page = await jiraBoards(
    boardName.value.trim(),
    more ? (nextStart.value ?? 0) : 0,
  );
  const combined = more ? [...boards.value, ...page.values] : page.values;
  boards.value = [...new Map(combined.map((b) => [b.id, b])).values()];
  nextStart.value = page.nextStart;
}
function select(b: JiraBoardRef) {
  if (chosen.value.some((x) => x.id === b.id))
    chosen.value = chosen.value.filter((x) => x.id !== b.id);
  else if (chosen.value.length < 20) chosen.value.push(b);
}
const save = () =>
  run(async () => {
    await jiraSelectBoards(chosen.value);
    emit("saved", [...chosen.value]);
  });
onMounted(() => run(() => fetchBoards(false)));
</script>

<template>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <section class="board-picker" aria-label="Wybór tablic">
    <form class="board-search" @submit.prevent="run(() => fetchBoards(false))">
      <input
        v-model="boardName"
        aria-label="Szukaj tablicy"
        placeholder="Nazwa tablicy…"
        :disabled="busy"
      /><button class="secondary-button" :disabled="busy">
        Szukaj tablic
      </button>
    </form>
    <div class="chosen-boards">
      <button
        v-for="b in chosen"
        :key="b.id"
        :disabled="busy"
        :aria-label="'Odznacz tablicę: ' + b.name"
        @click="select(b)"
      >
        {{ b.name }} ×
      </button>
    </div>
    <div class="board-options">
      <label v-for="b in boards" :key="b.id"
        ><input
          type="checkbox"
          :checked="chosen.some((x) => x.id === b.id)"
          :disabled="
            busy ||
            (chosen.length >= 20 && !chosen.some((x) => x.id === b.id))
          "
          @change="select(b)"
        />{{ b.name }} <small>#{{ b.id }}</small></label
      >
      <p v-if="!boards.length && !busy" class="hint">
        Brak tablic pasujących do wyszukiwania.
      </p>
    </div>
    <div class="connection-actions">
      <button
        v-if="nextStart !== null"
        class="secondary-button"
        :disabled="busy"
        @click="run(() => fetchBoards(true))"
      >
        Pokaż więcej tablic</button
      ><span class="hint">Wybrano {{ chosen.length }} / 20</span
      ><button class="secondary-button" :disabled="busy" @click="emit('cancel')">
        Anuluj</button
      ><button class="primary-button" :disabled="busy" @click="save">
        Zapisz tablice
      </button>
    </div>
  </section>
</template>
