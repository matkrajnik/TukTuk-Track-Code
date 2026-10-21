<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { Bot } from "lucide-vue-next";
import { applyProposal } from "../aiproposals";
import {
  mcpProposalResolve,
  mcpProposals,
  onMcpProposals,
  proposalName,
  proposalText,
  type McpProposal,
} from "../mcpapi";
import type { Snapshot, Task } from "../store";

/**
 * Baner propozycji agentów (serwer MCP, A2): agent w Claude Code albo Codexie
 * wywołał `propose_*`, a aplikacja niczego nie zmieniła. „Zastosuj” wykonuje
 * zmianę istniejącymi komendami, „Odrzuć” zdejmuje propozycję bez żadnej zmiany.
 */
const props = defineProps<{ tasks: Task[] }>();
const emit = defineEmits<{
  updated: [data: Snapshot];
  error: [message: string];
}>();
const proposals = ref<McpProposal[]>([]);
const busy = ref(false);

async function reload() {
  try {
    proposals.value = await mcpProposals();
  } catch {
    proposals.value = [];
  }
}
async function run(action: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true;
  try {
    await action();
  } catch (e) {
    emit("error", String(e instanceof Error ? e.message : e));
  } finally {
    busy.value = false;
    await reload();
  }
}
const apply = (p: McpProposal) =>
  run(async () => {
    const data = await applyProposal(p, props.tasks);
    // Zmiana zapisana; dopiero potem propozycja znika z kolejki.
    await mcpProposalResolve(p.id);
    emit("updated", data);
  });
const dismiss = (p: McpProposal) =>
  run(async () => {
    await mcpProposalResolve(p.id);
  });

let stop: (() => void) | undefined;
let alive = true;
onMounted(async () => {
  try {
    const unlisten = await onMcpProposals(() => void reload());
    if (!alive) return unlisten();
    stop = unlisten;
  } catch {}
  await reload();
});
onUnmounted(() => {
  alive = false;
  stop?.();
});
</script>

<template>
  <section
    v-if="proposals.length"
    class="idle-banner agent-banner"
    aria-label="Propozycje agentów"
  >
    <Bot class="infobar-icon" :size="20" aria-hidden="true" />
    <h2>
      {{
        proposals.length === 1
          ? "Agent proponuje zmianę"
          : `Agenci proponują zmiany (${proposals.length})`
      }}
    </h2>
    <p class="hint">Nic się nie zmieni, dopóki nie klikniesz „Zastosuj”.</p>
    <ul class="agent-suggestions">
      <li
        v-for="p in proposals"
        :key="p.id"
        :aria-label="`Propozycja: ${proposalName(p)}`"
      >
        <span>{{ proposalText(p) }}</span>
        <span class="planning-actions">
          <button
            class="secondary-button"
            :disabled="busy"
            :aria-label="`Zastosuj propozycję: ${proposalName(p)}`"
            @click="apply(p)"
          >
            Zastosuj</button
          ><button
            class="today-button"
            :disabled="busy"
            :aria-label="`Odrzuć propozycję: ${proposalName(p)}`"
            @click="dismiss(p)"
          >
            Odrzuć
          </button>
        </span>
      </li>
    </ul>
  </section>
</template>
