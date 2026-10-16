<script setup lang="ts">
import { nextTick, ref } from "vue";
import { GitBranch, MessageSquarePlus, UserCheck, Users } from "lucide-vue-next";
import {
  jiraAssign,
  jiraAssignable,
  jiraBranchName,
  jiraComment,
  jiraCommentLimit,
  type JiraPerson,
} from "../store";
import { useSettings } from "../target";

/**
 * Akcje na zgłoszeniu Jiry (J2): „Przypisz do mnie”, „Zmień osobę…”,
 * „Dodaj komentarz” i „Kopiuj nazwę gałęzi”. Wspólne dla menu karty na tablicy
 * i rozwiniętego kafelka zadania z kluczem. Zapisy idą przez backend
 * (`jira::write`); przy wyłączonych zmianach w Jirze zostaje tylko nazwa gałęzi.
 */
const props = defineProps<{ issueKey: string; summary: string }>();
const emit = defineEmits<{ assigned: [person: JiraPerson] }>();
const settings = useSettings();

const busy = ref(false);
const message = ref("");
const error = ref("");
const panel = ref<"" | "people" | "comment">("");
const query = ref("");
const people = ref<JiraPerson[] | null>(null);
const comment = ref("");
const commentInput = ref<HTMLTextAreaElement | null>(null);
const queryInput = ref<HTMLInputElement | null>(null);

async function run(action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  error.value = "";
  message.value = "";
  try {
    await action();
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    busy.value = false;
  }
}
const assignMe = () =>
  run(async () => {
    const who = await jiraAssign(props.issueKey, null);
    emit("assigned", who);
    message.value = `Przypisano ${props.issueKey} do Ciebie.`;
  });
async function openPanel(next: "people" | "comment") {
  panel.value = panel.value === next ? "" : next;
  error.value = "";
  message.value = "";
  await nextTick();
  if (panel.value === "people") {
    queryInput.value?.focus();
    if (people.value === null) await search();
  } else if (panel.value === "comment") commentInput.value?.focus();
}
const search = () =>
  run(async () => {
    people.value = await jiraAssignable(props.issueKey, query.value);
  });
const assignTo = (person: JiraPerson) =>
  run(async () => {
    const who = await jiraAssign(props.issueKey, person.accountId);
    emit("assigned", { ...who, displayName: person.displayName });
    panel.value = "";
    message.value = `Przypisano ${props.issueKey}: ${person.displayName}.`;
  });
const sendComment = () =>
  run(async () => {
    await jiraComment(props.issueKey, comment.value);
    comment.value = "";
    panel.value = "";
    message.value = `Dodano komentarz do ${props.issueKey}.`;
  });
const copyBranch = () =>
  run(async () => {
    const name = await jiraBranchName(props.issueKey, props.summary);
    try {
      await navigator.clipboard.writeText(name);
      message.value = `Skopiowano nazwę gałęzi: ${name}`;
    } catch {
      message.value = `Nazwa gałęzi: ${name} (zaznacz i skopiuj Ctrl+C)`;
    }
  });
</script>

<template>
  <div class="jira-issue-actions">
    <div
      class="jira-issue-actions-row"
      role="group"
      :aria-label="'Akcje Jiry: ' + issueKey"
    >
      <template v-if="settings.jiraWrite">
        <button
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-label="'Przypisz do mnie: ' + issueKey"
          @click="assignMe"
        >
          <UserCheck :size="14" />Przypisz do mnie
        </button>
        <button
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-expanded="panel === 'people'"
          :aria-label="'Zmień osobę: ' + issueKey"
          @click="openPanel('people')"
        >
          <Users :size="14" />Zmień osobę…
        </button>
        <button
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-expanded="panel === 'comment'"
          :aria-label="'Dodaj komentarz: ' + issueKey"
          @click="openPanel('comment')"
        >
          <MessageSquarePlus :size="14" />Dodaj komentarz
        </button>
      </template>
      <button
        type="button"
        class="action-button"
        :disabled="busy"
        :aria-label="'Kopiuj nazwę gałęzi: ' + issueKey"
        @click="copyBranch"
      >
        <GitBranch :size="14" />Kopiuj nazwę gałęzi
      </button>
    </div>
    <form
      v-if="panel === 'people'"
      class="jira-people"
      @submit.prevent="search"
      @keydown.esc.stop="panel = ''"
    >
      <input
        ref="queryInput"
        v-model="query"
        maxlength="100"
        :aria-label="'Szukaj osoby: ' + issueKey"
        placeholder="Imię, nazwisko lub e-mail…"
        :disabled="busy"
      /><button class="secondary-button" :disabled="busy">Szukaj</button>
      <div class="jira-people-list">
        <button
          v-for="person in people ?? []"
          :key="person.accountId"
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-label="'Przypisz: ' + person.displayName"
          @click="assignTo(person)"
        >
          {{ person.displayName }}
        </button>
        <span v-if="people && !people.length" class="hint"
          >Nikt nie pasuje do wyszukiwania.</span
        >
      </div>
    </form>
    <form
      v-if="panel === 'comment'"
      class="jira-comment"
      @submit.prevent="sendComment"
      @keydown.esc.stop="panel = ''"
    >
      <textarea
        ref="commentInput"
        v-model="comment"
        rows="3"
        :maxlength="jiraCommentLimit"
        :aria-label="'Komentarz do ' + issueKey"
        placeholder="Treść komentarza (pusta linia zaczyna nowy akapit)"
        :disabled="busy"
      />
      <div class="jira-comment-actions">
        <button type="button" class="action-button" @click="panel = ''">
          Anuluj
        </button>
        <button class="primary-button" :disabled="busy || !comment.trim()">
          Wyślij komentarz
        </button>
      </div>
    </form>
    <p v-if="message" class="hint jira-action-message" role="status">
      {{ message }}
    </p>
    <p v-if="error" class="jira-move-error" role="alert">{{ error }}</p>
    <p v-if="!settings.jiraWrite" class="hint">
      Zmiany w Jirze są wyłączone w Ustawieniach → Jira.
    </p>
  </div>
</template>
