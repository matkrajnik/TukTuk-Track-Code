<script setup lang="ts">
import { computed, ref } from "vue";
import { shortcutFromEvent } from "../settings";
const props = defineProps<{
  modelValue: string;
  label: string;
  defaultValue: string;
  error?: string;
  disabled?: boolean;
}>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
const recording = ref(false),
  hint = ref("");
const id = `shortcut-${Math.random().toString(36).slice(2, 9)}`;
const message = computed(() => hint.value || props.error || "");
function start() {
  if (props.disabled) return;
  recording.value = true;
  hint.value = "";
}
function stop() {
  recording.value = false;
}
function onKey(e: KeyboardEvent) {
  // Tab i Shift+Tab bez innych modyfikatorów przenoszą fokus, jak zwykle.
  if (e.code === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) return;
  e.preventDefault();
  // Nie przekazuj dalej, np. Ctrl+K nie otwiera szybkiego przełączania.
  e.stopPropagation();
  if (!recording.value) {
    if (e.key === "Enter" || e.key === " ") start();
    return;
  }
  if (e.code === "Escape" && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    hint.value = "";
    stop();
    return;
  }
  const result = shortcutFromEvent(e);
  if (!result) return;
  if ("message" in result) {
    hint.value = result.message;
    return;
  }
  hint.value = "";
  stop();
  if (result.value !== props.modelValue)
    emit("update:modelValue", result.value);
}
function reset() {
  hint.value = "";
  stop();
  if (props.defaultValue !== props.modelValue)
    emit("update:modelValue", props.defaultValue);
}
</script>
<template>
  <div class="shortcut-field">
    <label :for="id">{{ label }}</label>
    <div class="shortcut-row">
      <input
        :id="id"
        class="shortcut-input"
        :class="{ recording }"
        type="text"
        readonly
        :value="recording ? '' : modelValue"
        :placeholder="recording ? 'Naciśnij kombinację klawiszy…' : ''"
        :disabled="disabled"
        :aria-invalid="message ? 'true' : undefined"
        :aria-describedby="message ? id + '-error' : undefined"
        @focus="start"
        @click="start"
        @blur="stop"
        @keydown="onKey"
      />
      <button
        type="button"
        class="secondary-button"
        :disabled="disabled || modelValue === defaultValue"
        :aria-label="'Przywróć domyślny: ' + label"
        @click="reset"
      >
        Przywróć domyślny
      </button>
    </div>
    <p v-if="message" :id="id + '-error'" class="error shortcut-error">
      {{ message }}
    </p>
  </div>
</template>
