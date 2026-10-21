<script setup lang="ts">
/**
 * „Zapisz jako szablon” w rozwiniętym kafelku: nazwa wpisywana tutaj, reszta
 * z zadania (`templateFromTask`). Po zapisie formularz znika, a zostaje
 * informacja z linkiem do Ustawień → Szablony.
 */
import { nextTick, ref, watch } from "vue";
import { X } from "lucide-vue-next";
import { saveTemplate, templateFromTask, type Task } from "../../store";
import { openSettings } from "../../navigation";

const props = defineProps<{ task: Task }>();
/** Formularz otwarty (przycisk w pasku akcji kafelka). */
const open = defineModel<boolean>("open", { required: true });
const emit = defineEmits<{ error: [message: string] }>();
const name = ref("");
const saved = ref("");
const busy = ref(false);
const input = ref<HTMLInputElement | null>(null);
watch(open, async (value) => {
  if (!value) return;
  saved.value = "";
  name.value = "";
  await nextTick();
  input.value?.focus();
});
async function save() {
  const value = name.value.trim();
  if (!value || busy.value) return;
  busy.value = true;
  emit("error", "");
  try {
    await saveTemplate(null, value, templateFromTask(props.task));
    open.value = false;
    saved.value = `Zapisano szablon „${value}”. Użyj go, wpisując /${value} w polu „Nowe zadanie”; zmienisz go w`;
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <form
    v-if="open"
    class="template-save"
    @submit.prevent="save"
    @keydown.esc.stop="open = false"
  >
    <label
      >Nazwa szablonu
      <span class="template-slash">/</span
      ><input
        ref="input"
        v-model="name"
        :aria-label="'Nazwa szablonu: ' + task.title"
        placeholder="np. review"
        maxlength="60"
    /></label>
    <button class="primary-button" :disabled="busy || !name.trim()">
      Zapisz szablon
    </button>
    <button type="button" class="action-button" @click="open = false">
      <X :size="14" />Anuluj
    </button>
  </form>
  <p v-if="saved" class="hint template-saved" role="status">
    {{ saved }}
    <button type="button" class="text-link" @click="openSettings('templates')">
      Ustawieniach → Szablony</button
    >.
  </p>
</template>
