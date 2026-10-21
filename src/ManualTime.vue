<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import {
  addSession,
  type Session,
  type Task,
  type Snapshot,
} from "./store";
import { localInput } from "./time";
import { manualRange } from "./manualRange";
import { useSettings } from "./target";
import { Plus } from "lucide-vue-next";
import SearchSelect from "./components/SearchSelect.vue";

/** Wstępne godziny z zewnątrz, np. „Dodaj jako czas” przy spotkaniu. */
interface ManualPreset {
  start: number;
  end: number;
  /** Nowy obiekt = nowe żądanie, nawet z tymi samymi godzinami. */
  key: string;
  /** Zadanie ręcznego spotkania (B12): ma pierwszeństwo przed zapamiętanym. */
  taskId?: number | null;
}
const props = defineProps<{
  tasks: Task[];
  day: string;
  preset?: ManualPreset | null;
  /** Klucz localStorage zapamiętujący ostatnio wybrane zadanie. */
  rememberKey?: string;
  /** Przedziały do domyślnych godzin (koniec ostatniego przedziału dnia, B11). */
  sessions?: readonly Session[];
}>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const opened = ref(false),
  busy = ref(false),
  error = ref("");
const settings = useSettings();
const taskId = ref(0),
  start = ref(""),
  end = ref("");
const form = ref<HTMLFormElement | null>(null);
/**
 * Domyślne godziny (B11): od początku dnia z ustawień albo od końca ostatniego
 * przedziału dnia, do teraz dla dzisiaj (`manualRange`).
 */
function suggest() {
  const [from, to] = manualRange(
    props.day,
    props.sessions ?? [],
    Date.now(),
    settings.value.dayStart,
  );
  start.value = localInput(from);
  end.value = localInput(to);
}
suggest();
watch(() => props.day, suggest);
/** Otwarcie przyciskiem podstawia świeże godziny; zamknięcie zostawia wpisane. */
function toggleForm() {
  if (!opened.value) {
    suggest();
    error.value = "";
  }
  opened.value = !opened.value;
}
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
    taskId.value =
      preset.taskId && props.tasks.some((t) => t.id === preset.taskId)
        ? preset.taskId
        : remembered() || taskId.value;
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
      @click="toggleForm"
    >
      <Plus :size="14" />Dodaj czas ręcznie
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
          >Zadanie do uzupełnienia<SearchSelect
            v-model="taskId"
            label="Zadanie do uzupełnienia"
            placeholder="Wybierz zadanie"
            required
            :options="tasks.map((t) => ({ value: t.id, label: t.title }))"
        /></label
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
