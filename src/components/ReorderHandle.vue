<script setup lang="ts">
/**
 * Uchwyt ręcznej kolejności: przeciąganie (natywne HTML5 DnD) i Alt+↑ / Alt+↓.
 * Upuszczenie obsługuje element listy przez `useReorder().dropProps`.
 */
defineProps<{
  title: string;
  taskId: number;
  scope: string;
  disabled: boolean;
}>();
const emit = defineEmits<{
  move: [delta: -1 | 1];
  dragstart: [event: DragEvent];
  dragend: [];
}>();
</script>

<template>
  <button
    class="reorder-handle"
    type="button"
    :draggable="!disabled"
    :aria-label="'Przesuń: ' + title"
    :aria-disabled="disabled"
    :data-reorder-handle="scope + '-' + taskId"
    title="Przeciągnij albo użyj Alt+↑ / Alt+↓, aby zmienić kolejność"
    @keydown.alt.up.prevent="!disabled && emit('move', -1)"
    @keydown.alt.down.prevent="!disabled && emit('move', 1)"
    @dragstart="emit('dragstart', $event)"
    @dragend="emit('dragend')"
  >
    ⋮⋮
  </button>
</template>
