<script setup lang="ts">
/** Ustawienia → Szablony: lista szablonów zadań i nowy szablon. */
import { ref } from "vue";
import { Plus } from "lucide-vue-next";
import TemplateEditor from "../TemplateEditor.vue";
import { templates } from "../../store";
/** Formularz nowego szablonu; błędy pokazuje `TemplateEditor` przy szablonie. */
const addingTemplate = ref(false);
</script>
<template>
  <h2>Szablony</h2>
  <p class="hint">
    Wpisz /nazwa w polu „Nowe zadanie” albo w szybkim przełączaniu, np.
    /review WMS-123. W tytule {klucz} to klucz Jiry, a {tytuł} to tytuł z
    linii albo zgłoszenia. Pola wpisane w linii (~, #, !) wygrywają z
    szablonem. Szablon zapiszesz też z rozwiniętego zadania.
  </p>
  <div class="template-list" aria-label="Szablony zadań">
    <TemplateEditor v-for="t in templates" :key="t.id" :template="t" />
    <p v-if="!templates.length && !addingTemplate" class="empty-detail">
      Brak szablonów.
    </p>
    <TemplateEditor
      v-if="addingTemplate"
      :template="null"
      @done="addingTemplate = false"
    />
  </div>
  <button
    v-if="!addingTemplate"
    class="action-button"
    @click="addingTemplate = true"
  >
    <Plus :size="14" />Nowy szablon
  </button>
</template>
