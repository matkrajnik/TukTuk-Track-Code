<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { addSession, type Task, type Snapshot } from "./store";
import { localInput } from "./time";

/** Wstępne godziny z zewnątrz, np. „Dodaj jako czas” przy spotkaniu. */
interface ManualPreset {
  start: number;
  end: number;
  /** Nowy obiekt = nowe żądanie, nawet z tymi samymi godzinami. */
  key: string;
}
const props = defineProps<{
  tasks: Task[];
  day: string;
  preset?: ManualPreset | null;
  /** Klucz localStorage zapamiętujący ostatnio wybrane zadanie. */
  rememberKey?: string;
}>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const opened = ref(false),
  busy = ref(false),
  error = ref("");
const taskId = ref(0),
  start = ref(props.day + "T09:00"),
  end = ref(props.day + "T10:00");
const form = ref<HTMLFormElement | null>(null);
watch(
  () => props.day,
  (day) => {
    start.value = day + "T09:00";
    end.value = day + "T10:00";
  },
);
function remembered(): number {
  if (!props.rememberKey) return 0;
  try {
    const id = Number(localStorage.getItem(props.rememberKey));
    return props.tasks.some((t) => t.id === id) ? id : 0;
  } catch {
    return 0;
  }
}
function remember(id: number) {
  if (!props.rememberKey) return;
  try {
    localStorage.setItem(props.rememberKey, String(id));
  } catch {
    // Bez pamięci wyboru formularz działa dalej.
  }
}
watch(
  () => props.preset,
  async (preset) => {
    if (!preset) return;
    start.value = localInput(preset.start);
    end.value = localInput(preset.end);
    taskId.value = remembered() || taskId.value;
    error.value = "";
    opened.value = true;
    await nextTick();
    form.value?.scrollIntoView?.({ block: "nearest" });
  },
  { immediate: true },
);
async function save() {
  busy.value = true;
  error.value = "";
  try {
    emit(
      "updated",
      await addSession(
        taskId.value,
        +new Date(start.value),
        +new Date(end.value),
      ),
    );
    remember(taskId.value);
    opened.value = false;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <div class="manual-time">
    <button
      class="today-button"
      :aria-expanded="opened"
      @click="opened = !opened"
    >
      Dodaj czas ręcznie
    </button>
    <form
      v-if="opened"
      ref="form"
      class="planning-form"
      @submit.prevent="save"
    >
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <div class="planning-fields">
        <label
          >Zadanie do uzupełnienia<select
            v-model.number="taskId"
            aria-label="Zadanie do uzupełnienia"
            required
          >
            <option :value="0" disabled>Wybierz zadanie</option>
            <option v-for="task in tasks" :key="task.id" :value="task.id">
              {{ task.title }}
            </option>
          </select></label
        >
        <label
          >Początek ręcznego czasu<input
            v-model="start"
            type="datetime-local"
            step="1"
            required
        /></label>
        <label
          >Koniec ręcznego czasu<input
            v-model="end"
            type="datetime-local"
            step="1"
            required
        /></label>
      </div>
      <button class="primary-button" :disabled="busy || !taskId">
        Zapisz ręczny czas
      </button>
    </form>
  </div>
</template>
