<script setup lang="ts" generic="T extends string | number">
/**
 * Pole wyboru z wyszukiwaniem (B1): pole tekstowe z rolą `combobox` i lista
 * `listbox` filtrowana wpisanym tekstem. Klawiatura: strzałki w dół i w górę
 * (także otwierają listę), Home / End, Enter wybiera, Escape zamyka bez zmiany.
 * Aktywna pozycja przez `aria-activedescendant`, fokus zostaje w polu.
 * Zamiast `<select>` tam, gdzie opcji jest więcej niż kilka.
 */
import { computed, nextTick, ref, useId, watch } from "vue";
import { ChevronDown } from "lucide-vue-next";
import { filterOptions, type SearchOption } from "../searchSelect";

const props = withDefaults(
  defineProps<{
    modelValue: T | null | undefined;
    options: readonly SearchOption<T>[];
    /** Nazwa dostępna pola (aria-label), np. „Zadanie do planu”. */
    label: string;
    placeholder?: string;
    disabled?: boolean;
    required?: boolean;
    /** Identyfikator pola tekstowego (dla `<label for>`). */
    inputId?: string;
    title?: string;
    /** Tekst, gdy żadna pozycja nie pasuje. */
    emptyText?: string;
  }>(),
  { placeholder: "", emptyText: "Brak pasujących pozycji" },
);
const emit = defineEmits<{
  "update:modelValue": [value: T];
  change: [value: T];
}>();

const uid = useId();
const listId = `search-select-${uid}`;
const optionId = (index: number) => `${listId}-${index}`;
const input = ref<HTMLInputElement | null>(null);
const open = ref(false);
/** Użytkownik coś wpisał od otwarcia listy: pole pokazuje zapytanie. */
const typing = ref(false);
const query = ref("");
const active = ref(-1);

const selected = computed(() =>
  props.options.find((o) => o.value === props.modelValue),
);
const shown = computed(() =>
  typing.value ? filterOptions(props.options, query.value) : [...props.options],
);
const text = computed(() =>
  typing.value ? query.value : (selected.value?.label ?? ""),
);
/** Pozycja listy z nagłówkiem grupy, gdy grupa zmienia się względem poprzedniej. */
const rows = computed(() =>
  shown.value.map((option, index) => ({
    option,
    index,
    heading:
      option.group &&
      (index === 0 || shown.value[index - 1].group !== option.group)
        ? option.group
        : "",
  })),
);

const enabled = (index: number) =>
  index >= 0 && index < shown.value.length && !shown.value[index].disabled;
function step(from: number, delta: 1 | -1): number {
  const n = shown.value.length;
  for (let i = from + delta; i >= 0 && i < n; i += delta)
    if (enabled(i)) return i;
  return from;
}
function first(): number {
  return step(-1, 1);
}
function last(): number {
  return step(shown.value.length, -1);
}

function openList() {
  if (props.disabled || open.value) return;
  open.value = true;
  typing.value = false;
  query.value = "";
  const index = shown.value.findIndex((o) => o.value === props.modelValue);
  active.value = enabled(index) ? index : first();
}
function close() {
  open.value = false;
  typing.value = false;
  query.value = "";
  active.value = -1;
}
function choose(index: number) {
  if (!enabled(index)) return;
  const value = shown.value[index].value;
  close();
  if (value !== props.modelValue) {
    emit("update:modelValue", value);
    emit("change", value);
  }
}
function onInput(event: Event) {
  if (!open.value) openList();
  typing.value = true;
  query.value = (event.target as HTMLInputElement).value;
  active.value = first();
}
function onKeydown(event: KeyboardEvent) {
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      if (!open.value) openList();
      else active.value = step(active.value, 1);
      break;
    case "ArrowUp":
      event.preventDefault();
      if (!open.value) openList();
      else active.value = step(active.value, -1);
      break;
    case "Home":
    case "End":
      if (!open.value) return;
      event.preventDefault();
      active.value = event.key === "Home" ? first() : last();
      break;
    case "Enter":
      // Zamknięta lista: Enter zostaje dla formularza (np. „Dodaj do planu”).
      if (!open.value) return;
      event.preventDefault();
      choose(active.value);
      break;
    case "Escape":
      // Escape zamyka tylko listę, nie okno dialogowe ani formularz wokół.
      if (!open.value) return;
      event.preventDefault();
      event.stopPropagation();
      close();
      break;
    case "Tab":
      close();
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
watch(
  () => props.disabled,
  (disabled) => disabled && close(),
);
defineExpose({ focus: () => input.value?.focus() });
</script>

<template>
  <span class="search-select" :class="{ open }">
    <input
      :id="inputId"
      ref="input"
      type="text"
      role="combobox"
      autocomplete="off"
      spellcheck="false"
      aria-autocomplete="list"
      :aria-label="label"
      :aria-expanded="open"
      :aria-controls="open ? listId : undefined"
      :aria-activedescendant="
        open && active >= 0 ? optionId(active) : undefined
      "
      :aria-required="required || undefined"
      :data-value="modelValue ?? ''"
      :value="text"
      :placeholder="selected ? '' : placeholder"
      :disabled="disabled"
      :title="title"
      @click="open ? undefined : openList()"
      @input="onInput"
      @keydown="onKeydown"
      @blur="close"
    /><ChevronDown class="search-select-chevron" :size="14" aria-hidden="true" />
    <!-- Lista tylko przy otwartym polu: zamknięte pola nie dokładają do strony
         ukrytych pozycji (tytuły zadań), a nazwa listy nie dubluje etykiety pola.
         @mousedown.prevent na całej liście: pasek przewijania, nagłówek grupy
         ani wiersz „Brak pasujących pozycji” nie zabierają fokusu z pola. -->
    <ul
      v-if="open"
      :id="listId"
      class="search-select-list"
      role="listbox"
      @mousedown.prevent
    >
      <template v-for="row in rows" :key="String(row.option.value)">
        <li
          v-if="row.heading"
          class="search-select-group"
          role="presentation"
        >
          {{ row.heading }}
        </li>
        <li
          :id="optionId(row.index)"
          class="search-select-option"
          :class="{
            active: row.index === active,
            grouped: !!row.option.group,
          }"
          role="option"
          :aria-selected="row.option.value === modelValue"
          :aria-disabled="row.option.disabled || undefined"
          @mousemove="enabled(row.index) && (active = row.index)"
          @click.prevent="choose(row.index)"
        >
          {{ row.option.label }}
        </li>
      </template>
      <li v-if="!rows.length" class="search-select-empty" role="presentation">
        {{ emptyText }}
      </li>
    </ul>
  </span>
</template>
