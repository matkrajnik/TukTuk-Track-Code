<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { Search } from "lucide-vue-next";
import SettingsPanel from "../components/SettingsPanel.vue";
import {
  matchingSections,
  saveSettingsSection,
  type SettingsSection,
} from "../navigation";

/** Wybrana sekcja; App ustawia ją z `navigate` i skrótu Ctrl+,. */
const section = defineModel<SettingsSection>("section", { required: true });
const query = ref("");
const matches = computed(() => matchingSections(query.value));
/**
 * Sekcja w treści: wybrana, a gdy wyszukiwanie ją odfiltrowało, pierwsza
 * pasująca. Wybór z wyszukiwania zapamiętuje się dopiero po kliknięciu.
 */
const shown = computed<SettingsSection | null>(() =>
  matches.value.some((s) => s.id === section.value)
    ? section.value
    : (matches.value[0]?.id ?? null),
);
watch(section, saveSettingsSection, { immediate: true });
function choose(id: SettingsSection) {
  section.value = id;
}
/** Strzałki, Home i End przenoszą wybór po liście sekcji. */
function moveFocus(e: KeyboardEvent) {
  const list = matches.value;
  const at = list.findIndex((s) => s.id === shown.value);
  const next =
    e.key === "ArrowDown"
      ? at + 1
      : e.key === "ArrowUp"
        ? at - 1
        : e.key === "Home"
          ? 0
          : e.key === "End"
            ? list.length - 1
            : null;
  if (next === null || !list.length) return;
  e.preventDefault();
  const target = list[(next + list.length) % list.length];
  choose(target.id);
  document.getElementById(`settings-tab-${target.id}`)?.focus();
}
function chooseFromSelect(e: Event) {
  choose((e.target as HTMLSelectElement).value as SettingsSection);
}
</script>

<template>
  <div class="page-heading"><h1>Ustawienia</h1></div>
  <div class="settings-layout">
    <aside class="settings-nav">
      <label class="settings-search"
        ><Search :size="15" aria-hidden="true" /><input
          v-model="query"
          type="search"
          aria-label="Szukaj ustawienia"
          placeholder="Szukaj ustawienia"
      /></label>
      <div
        class="settings-sections"
        role="tablist"
        aria-label="Sekcje ustawień"
        aria-orientation="vertical"
        @keydown="moveFocus"
      >
        <button
          v-for="s in matches"
          :id="`settings-tab-${s.id}`"
          :key="s.id"
          role="tab"
          :aria-selected="shown === s.id"
          :tabindex="shown === s.id ? 0 : -1"
          :class="{ selected: shown === s.id }"
          aria-controls="settings-content"
          @click="choose(s.id)"
        >
          {{ s.name }}
        </button>
      </div>
      <select
        v-if="matches.length"
        class="settings-select"
        aria-label="Sekcja ustawień"
        :value="shown ?? undefined"
        @change="chooseFromSelect"
      >
        <option v-for="s in matches" :key="s.id" :value="s.id">
          {{ s.name }}
        </option>
      </select>
    </aside>
    <div
      id="settings-content"
      class="settings-content"
      :role="shown ? 'tabpanel' : undefined"
      :aria-labelledby="shown ? `settings-tab-${shown}` : undefined"
    >
      <SettingsPanel v-if="shown" :section="shown" />
      <p v-else class="empty-state" role="status">
        Brak ustawień pasujących do „{{ query.trim() }}”.
      </p>
    </div>
  </div>
</template>
