<script setup lang="ts">
import { computed } from "vue";
import { X } from "lucide-vue-next";
import { blockingError, type Parsed, type Token } from "../quickadd";

/** Rozpoznane zapisy pod polem szybkiego dodawania; kliknięcie usuwa zapis. */
const props = defineProps<{ parsed: Parsed }>();
const emit = defineEmits<{ remove: [token: Token] }>();
const blocking = computed(() => blockingError(props.parsed));
</script>

<template>
  <div
    v-if="parsed.tokens.length"
    class="quick-chips"
    aria-label="Rozpoznane pola"
  >
    <button
      v-for="token in parsed.tokens"
      :key="`${token.start}-${token.text}`"
      type="button"
      class="quick-chip"
      :class="[token.kind, { invalid: token.kind === 'error' }]"
      :title="token.error ?? `Usuń ${token.text}`"
      :aria-label="`Usuń: ${token.label}`"
      @mousedown.prevent
      @click="emit('remove', token)"
    >
      {{ token.label }}<X :size="11" />
    </button>
    <span v-if="blocking" class="quick-chip-error" role="alert">{{
      blocking
    }}</span>
  </div>
</template>
