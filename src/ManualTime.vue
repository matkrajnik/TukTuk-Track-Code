<script setup lang="ts">
import { ref, watch } from "vue";
import { addSession, type Task, type Snapshot } from "./store";
const props = defineProps<{ tasks: Task[]; day: string }>();
const emit = defineEmits<{ updated: [data: Snapshot] }>();
const opened = ref(false),
  busy = ref(false),
  error = ref("");
const taskId = ref(0),
  start = ref(props.day + "T09:00"),
  end = ref(props.day + "T10:00");
watch(
  () => props.day,
  (day) => {
    start.value = day + "T09:00";
    end.value = day + "T10:00";
  },
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
    <form v-if="opened" class="planning-form" @submit.prevent="save">
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
