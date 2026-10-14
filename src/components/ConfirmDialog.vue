<script setup lang="ts">
import { useModal } from "../composables/useModal";
withDefaults(
  defineProps<{
    title: string;
    label?: string;
    error?: string;
    busy?: boolean;
    /** Gdy podany, okno ma przyciski Anuluj i potwierdzenia. */
    confirmLabel?: string;
  }>(),
  { label: undefined, error: "", busy: false, confirmLabel: undefined },
);
const emit = defineEmits<{ close: []; confirm: [] }>();
const { root, trapFocus } = useModal();
</script>

<template>
  <div
    class="modal-overlay"
    @click.self="emit('close')"
    @keydown.esc="emit('close')"
  >
    <section
      ref="root"
      class="modal"
      role="dialog"
      aria-modal="true"
      :aria-label="label ?? title"
      @keydown="trapFocus"
    >
      <h2>{{ title }}</h2>
      <p v-if="error" class="error" role="alert">{{ error }}</p>
      <slot />
      <div v-if="confirmLabel" class="modal-actions">
        <button :disabled="busy" @click="emit('close')">Anuluj</button
        ><button class="delete-button" :disabled="busy" @click="emit('confirm')">
          {{ confirmLabel }}
        </button>
      </div>
    </section>
  </div>
</template>
