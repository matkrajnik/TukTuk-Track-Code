<script setup lang="ts">
import { nextTick, ref } from "vue";
import { GitBranch, MessageSquarePlus, UserCheck } from "lucide-vue-next";
import {
  redmineAssignMe,
  redmineBranchName,
  redmineComment,
  redmineNotesLimit,
  type RedmineIssue,
} from "../store";
import { redmineKey } from "../issue";
import { useSettings } from "../target";

/**
 * Akcje na zgłoszeniu Redmine (PLAN_1.9.md, R4): „Przypisz do mnie”, „Dodaj
 * komentarz” i „Kopiuj nazwę gałęzi”. Wspólne dla menu karty w widoku Redmine
 * i rozwiniętego kafelka zadania z `#1234`. Zapisy idą przez backend
 * (`redmine::write`) i tylko po kliknięciu; przy wyłączonych zmianach w
 * Redmine zostaje tylko nazwa gałęzi. Bez „Zmień osobę…” (wyszukiwarka
 * użytkowników Redmine wymaga uprawnień administratora).
 */
const props = defineProps<{ issueId: number; me?: boolean }>();
const emit = defineEmits<{ assigned: [issue: RedmineIssue | null] }>();
const settings = useSettings();
const key = () => redmineKey(props.issueId);

const busy = ref(false);
const message = ref("");
const error = ref("");
const commenting = ref(false);
const comment = ref("");
const commentInput = ref<HTMLTextAreaElement | null>(null);

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
    emit("assigned", await redmineAssignMe(props.issueId));
    message.value = `Przypisano ${key()} do Ciebie.`;
  });
async function toggleComment() {
  commenting.value = !commenting.value;
  error.value = "";
  message.value = "";
  await nextTick();
  if (commenting.value) commentInput.value?.focus();
}
const sendComment = () =>
  run(async () => {
    await redmineComment(props.issueId, comment.value);
    comment.value = "";
    commenting.value = false;
    message.value = `Dodano komentarz do ${key()}.`;
  });
const copyBranch = () =>
  run(async () => {
    const name = await redmineBranchName(props.issueId);
    try {
      await navigator.clipboard.writeText(name);
      message.value = `Skopiowano nazwę gałęzi: ${name}`;
    } catch {
      message.value = `Nazwa gałęzi: ${name} (zaznacz i skopiuj Ctrl+C)`;
    }
  });
</script>

<template>
  <div class="jira-issue-actions redmine-issue-actions">
    <div
      class="jira-issue-actions-row"
      role="group"
      :aria-label="'Akcje Redmine: ' + key()"
    >
      <template v-if="settings.redmineWrite">
        <button
          v-if="!me"
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-label="'Przypisz do mnie: ' + key()"
          @click="assignMe"
        >
          <UserCheck :size="14" />Przypisz do mnie
        </button>
        <button
          type="button"
          class="action-button"
          :disabled="busy"
          :aria-expanded="commenting"
          :aria-label="'Dodaj komentarz: ' + key()"
          @click="toggleComment"
        >
          <MessageSquarePlus :size="14" />Dodaj komentarz
        </button>
      </template>
      <button
        type="button"
        class="action-button"
        :disabled="busy"
        :aria-label="'Kopiuj nazwę gałęzi: ' + key()"
        @click="copyBranch"
      >
        <GitBranch :size="14" />Kopiuj nazwę gałęzi
      </button>
    </div>
    <form
      v-if="commenting"
      class="jira-comment"
      @submit.prevent="sendComment"
      @keydown.esc.stop="commenting = false"
    >
      <textarea
        ref="commentInput"
        v-model="comment"
        rows="3"
        :maxlength="redmineNotesLimit"
        :aria-label="'Komentarz do ' + key()"
        placeholder="Treść komentarza (zwykły tekst)"
        :disabled="busy"
      />
      <div class="jira-comment-actions">
        <button type="button" class="action-button" @click="commenting = false">
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
    <p v-if="!settings.redmineWrite" class="hint">
      Zmiany w Redmine są wyłączone w Ustawieniach → Redmine.
    </p>
  </div>
</template>
