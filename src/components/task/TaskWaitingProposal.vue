<script setup lang="ts">
/**
 * Propozycja „Czekam na” ze statusu zgłoszenia: oznaczyć albo odblokować;
 * nic nie zmienia się samo. Zmianę wykonuje kafelek (`change`).
 */
import { Hourglass, LockOpen, X } from "lucide-vue-next";
import type { WaitingChange } from "../../store";
import type { IssueRef } from "../../issue";
import type { WaitingProposal } from "../../recurrence";

defineProps<{
  title: string;
  issue: IssueRef;
  proposal: WaitingProposal;
  disabled: boolean;
}>();
const emit = defineEmits<{ change: [change: WaitingChange] }>();
</script>

<template>
  <div
    class="waiting-proposal"
    role="group"
    :aria-label="`Propozycja z ${issue.system === 'jira' ? 'Jiry' : 'Redmine'}: ${title}`"
  >
    <span
      >{{ issue.label }}: {{ proposal.status }} —
      {{
        proposal.action === "block"
          ? "oznaczyć jako czekam na?"
          : "odblokować?"
      }}</span
    >
    <button
      class="action-button"
      :disabled="disabled"
      @click="
        emit(
          'change',
          proposal.action === 'block'
            ? { action: 'block', text: `${issue.label}: ${proposal.status}` }
            : { action: 'unblock' },
        )
      "
    >
      <Hourglass v-if="proposal.action === 'block'" :size="14" /><LockOpen
        v-else
        :size="14"
      />{{
        proposal.action === "block" ? "Oznacz jako czekam na" : "Odblokuj"
      }}
    </button>
    <button
      class="action-button"
      :disabled="disabled"
      @click="emit('change', { action: 'dismiss' })"
    >
      <X :size="14" />Nie teraz
    </button>
  </div>
</template>
