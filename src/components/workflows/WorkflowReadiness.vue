<script setup lang="ts">
/**
 * Panel „Gotowość agentów” opublikowanej wersji procesu (E4): dostawca, wersja
 * CLI i tryb egzekwowania dostępu per węzeł agenta. Nie uruchamia agentów.
 */
import { computed, onMounted, ref, watch } from "vue";
import {
  workflowPreflight,
  type NodeReadiness,
  type WorkflowEnforcement,
  type WorkflowGraph,
} from "../../workflows";

const props = defineProps<{ versionId: number; graph: WorkflowGraph }>();

const items = ref<NodeReadiness[]>([]);
const loading = ref(false);
const error = ref("");
const status = ref("");

const providerNames = { claude: "Claude Code", codex: "Codex" } as const;

const enforcementText: Record<WorkflowEnforcement, string> = {
  cliControl:
    "kontrola CLI (narzędzia tylko do odczytu + wykrywanie zmian), nie izolacja systemu plików",
  codexSandbox: "sandbox Codexa",
  none: "brak egzekwowania dostępu",
};

const labels = computed(
  () => new Map(props.graph.nodes.map((n) => [n.id, n.label])),
);

/** Numer ostatniego żądania: odpowiedź starszego (np. sprzed publikacji) jest odrzucana. */
let latestRequest = 0;

async function check() {
  const request = ++latestRequest;
  const versionId = props.versionId;
  loading.value = true;
  error.value = "";
  status.value = "Sprawdzam gotowość agentów…";
  try {
    const result = await workflowPreflight(versionId);
    if (request !== latestRequest || versionId !== props.versionId) return;
    items.value = result;
    const blocked = result.filter((i) => i.blocked).length;
    status.value = blocked
      ? `Gotowość sprawdzona: zablokowane węzły: ${blocked}.`
      : "Gotowość sprawdzona: wszystkie węzły gotowe.";
  } catch (e) {
    if (request !== latestRequest) return;
    items.value = [];
    error.value = e instanceof Error ? e.message : String(e);
    status.value = "";
  } finally {
    if (request === latestRequest) loading.value = false;
  }
}

onMounted(check);
watch(() => props.versionId, check);
</script>

<template>
  <section class="wf-readiness" aria-labelledby="wf-readiness-title">
    <div class="wf-readiness-head">
      <h3 id="wf-readiness-title">Gotowość agentów</h3>
      <button type="button" :disabled="loading" @click="check">
        Sprawdź ponownie
      </button>
    </div>
    <p class="hint" role="status" aria-live="polite">{{ status }}</p>
    <p v-if="error" class="wf-readiness-error" role="alert">{{ error }}</p>
    <ul v-if="items.length" class="wf-readiness-list">
      <li v-for="item in items" :key="item.nodeId">
        <strong>{{ labels.get(item.nodeId) ?? item.nodeId }}</strong>
        <span>
          {{ providerNames[item.provider] }}
          {{ item.version ? `(wersja ${item.version})` : "(nie znaleziono)" }},
          {{
            item.access === "readOnly"
              ? "tylko odczyt"
              : "zapis w katalogu roboczym"
          }}
        </span>
        <span>Egzekwowanie: {{ enforcementText[item.enforcement] }}</span>
        <span v-if="item.warning" class="hint">{{ item.warning }}</span>
        <span v-if="item.blocked" class="wf-readiness-blocked">
          Zablokowane: {{ item.blocked.message }}
        </span>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.wf-readiness {
  margin-top: 16px;
  padding: 12px 16px;
  border: 1px solid var(--border, #8884);
  border-radius: 8px;
}
.wf-readiness-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.wf-readiness-list {
  list-style: none;
  padding: 0;
  margin: 8px 0 0;
  display: grid;
  gap: 10px;
}
.wf-readiness-list li {
  display: grid;
  gap: 2px;
}
.wf-readiness-blocked,
.wf-readiness-error {
  color: var(--danger, #b3261e);
}
</style>
