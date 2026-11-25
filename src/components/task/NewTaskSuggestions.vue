<script setup lang="ts">
import { nextTick, watch } from "vue";
import type { Task } from "../../store";

/**
 * Lista istniejących zadań pod polem „Nowe zadanie” (combobox w TasksView).
 * Klawisze obsługuje `keydown` (wywoływany z pola): strzałki wybierają,
 * Enter na wybranej pozycji wpisuje jej nazwę, Escape zamyka listę.
 */
const props = defineProps<{ tasks: Task[]; exactId: number | null }>();
const emit = defineEmits<{ pick: [task: Task]; dismiss: [] }>();
const active = defineModel<number>("active", { required: true });

const LIST_ID = "new-task-suggestions";
const optionId = (index: number) => `${LIST_ID}-${index}`;

/** Obsłużone klawisze zatrzymują domyślną akcję (Enter nie wysyła formularza). */
function keydown(event: KeyboardEvent) {
  const count = props.tasks.length;
  if (!count) return;
  switch (event.key) {
    case "ArrowDown":
    case "ArrowUp": {
      event.preventDefault();
      // Cykl: brak wyboru (-1) → pozycje → brak wyboru.
      const step = event.key === "ArrowDown" ? 1 : -1;
      active.value = ((active.value + 1 + step + count + 1) % (count + 1)) - 1;
      break;
    }
    case "Enter":
      if (active.value < 0 || event.shiftKey) return;
      event.preventDefault();
      emit("pick", props.tasks[active.value]);
      break;
    case "Escape":
      event.preventDefault();
      emit("dismiss");
      break;
  }
}
watch(active, async (index) => {
  if (index < 0) return;
  await nextTick();
  document
    .getElementById(optionId(index))
    ?.scrollIntoView?.({ block: "nearest" });
});
defineExpose({ keydown, listId: LIST_ID, optionId });
</script>

<template>
  <!-- @mousedown.prevent: klik w pozycję nie zabiera fokusu z pola. -->
  <ul
    :id="LIST_ID"
    class="search-select-list new-task-suggestions"
    role="listbox"
    aria-label="Istniejące zadania"
    @mousedown.prevent
  >
    <li
      v-for="(task, index) in tasks"
      :id="optionId(index)"
      :key="task.id"
      class="search-select-option"
      :class="{ active: index === active }"
      role="option"
      :aria-selected="index === active"
      @mousemove="active = index"
      @click="emit('pick', task)"
    >
      <span class="suggestion-title">{{ task.title }}</span>
      <span v-if="task.id === exactId" class="suggestion-meta">kontynuuj</span>
      <span v-else-if="task.archived" class="suggestion-meta">archiwum</span>
      <span v-else-if="task.details?.project" class="suggestion-meta">{{
        task.details.project
      }}</span>
    </li>
  </ul>
</template>

<style scoped>
.new-task-suggestions {
  max-width: none;
  right: 0;
}
.search-select-option {
  display: flex;
  gap: 12px;
  justify-content: space-between;
}
.suggestion-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.suggestion-meta {
  flex-shrink: 0;
  color: var(--muted);
  font-size: var(--fs-sm);
}
</style>
