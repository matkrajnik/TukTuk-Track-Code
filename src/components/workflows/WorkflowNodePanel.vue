<script setup lang="ts">
/**
 * Konfiguracja wybranego kroku procesu (PLAN_WDROZENIA.md, 4.3): nazwa,
 * ustawienia typu i połączenia wyjść. Każda zmiana pola (zdarzenie
 * `change`, nie każde naciśnięcie klawisza) to jeden krok cofania.
 */
import { computed } from "vue";
import {
  MAX_REPAIR_ROUNDS,
  nodeTypeNames,
  outputPorts,
  portNames,
  type AgentRole,
  type CompareOp,
  type EndOutcome,
  type NodeKind,
  type Port,
  type Provider,
  type WorkflowGraph,
  type WorkflowIssue,
  type WorkflowNode,
} from "../../api/workflows";
import {
  conditionFields,
  connect,
  connectableTargets,
  connectionTarget,
  defaultValue,
  opNames,
  opsFor,
  removeNode,
  roleNames,
  updateNode,
  verdictNames,
  withRole,
} from "../../workflowEdit";

const props = defineProps<{
  graph: WorkflowGraph;
  node: WorkflowNode;
  issues: WorkflowIssue[];
}>();
const emit = defineEmits<{ change: [graph: WorkflowGraph] }>();

const SECONDS_PER_MINUTE = 60;
const roles: AgentRole[] = ["analysis", "implementation", "fix", "review"];
const providers: { id: Provider; name: string }[] = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
];
const outcomes: { id: EndOutcome; name: string }[] = [
  { id: "succeeded", name: "Sukces" },
  { id: "rejected", name: "Odrzucenie" },
  { id: "failed", name: "Błąd" },
];

const value = (e: Event) =>
  (e.target as HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement)
    .value;

function setNode(change: (node: WorkflowNode) => WorkflowNode) {
  emit("change", updateNode(props.graph, props.node.id, change));
}
function setKind(kind: NodeKind) {
  setNode((n) => ({ ...n, kind }));
}

const kind = computed(() => props.node.kind);
const fields = computed(() => conditionFields(props.graph));
const fieldKey = (nodeId: string, field: string) => `${nodeId}|${field}`;
const selectedField = computed(() => {
  const k = kind.value;
  if (k.type !== "condition") return undefined;
  return fields.value.find(
    (f) => f.nodeId === k.field.nodeId && f.field === k.field.field,
  );
});

function setField(e: Event) {
  const k = kind.value;
  if (k.type !== "condition") return;
  const option = fields.value.find(
    (f) => fieldKey(f.nodeId, f.field) === value(e),
  );
  if (!option) return;
  const ops = opsFor(option.type);
  setKind({
    ...k,
    field: { nodeId: option.nodeId, field: option.field },
    op: ops.includes(k.op) ? k.op : "equals",
    value: defaultValue(option.type),
  });
}

function setConditionValue(e: Event) {
  const k = kind.value;
  if (k.type !== "condition") return;
  const raw = value(e);
  const type = selectedField.value?.type;
  const parsed =
    type?.type === "number"
      ? raw === ""
        ? null
        : Number(raw)
      : type?.type === "boolean"
        ? raw === "true"
        : raw;
  setKind({ ...k, value: parsed });
}

function setRepairLimit(e: Event) {
  const k = kind.value;
  if (k.type !== "condition") return;
  const raw = value(e);
  setKind({ ...k, repairLimit: raw === "" ? null : Number(raw) });
}

function setTimeoutMinutes(e: Event) {
  const k = kind.value;
  if (k.type !== "agent") return;
  const minutes = Number(value(e));
  setKind({ ...k, timeoutSecs: Math.round(minutes * SECONDS_PER_MINUTE) });
}

function setTarget(port: Port, e: Event) {
  const target = value(e);
  emit("change", connect(props.graph, props.node.id, port, target || null));
}

const targets = computed(() => connectableTargets(props.graph, props.node.id));
const ports = computed(() => outputPorts(props.node.kind));

function remove() {
  emit("change", removeNode(props.graph, props.node.id));
}
</script>

<template>
  <section
    class="wf-node-panel"
    :aria-label="`Ustawienia kroku ${node.label}`"
  >
    <h3>
      {{ nodeTypeNames[node.kind.type] }}
      <span class="wf-node-id">{{ node.id }}</span>
    </h3>
    <ul v-if="issues.length" class="wf-node-issues">
      <li v-for="(issue, i) in issues" :key="i">
        <span aria-hidden="true">⚠</span> {{ issue.message }}
      </li>
    </ul>
    <div class="settings-fields">
      <label>
        Nazwa kroku
        <input
          :value="node.label"
          @change="setNode((n) => ({ ...n, label: value($event) }))"
        />
      </label>

      <template v-if="kind.type === 'agent'">
        <label>
          Rola
          <select
            :value="kind.role"
            @change="setKind(withRole(kind, value($event) as AgentRole))"
          >
            <option v-for="r in roles" :key="r" :value="r">
              {{ roleNames[r] }}
            </option>
          </select>
        </label>
        <p class="hint">
          Dostęp:
          {{
            kind.access === "readOnly"
              ? "tylko odczyt repozytorium (kontrola CLI i wykrywanie zmian, nie izolacja systemu plików)"
              : "zapis w katalogu roboczym przebiegu"
          }}
          (wynika z roli).
        </p>
        <label>
          Dostawca
          <select
            :value="kind.provider"
            @change="setKind({ ...kind, provider: value($event) as Provider })"
          >
            <option v-for="p in providers" :key="p.id" :value="p.id">
              {{ p.name }}
            </option>
          </select>
        </label>
        <label>
          Model
          <input
            :value="kind.model ?? ''"
            placeholder="wybór przy uruchomieniu"
            @change="setKind({ ...kind, model: value($event).trim() || null })"
          />
        </label>
        <label>
          Instrukcja
          <textarea
            :value="kind.instruction"
            rows="5"
            @change="setKind({ ...kind, instruction: value($event) })"
          />
        </label>
        <label>
          Limit czasu (min)
          <input
            type="number"
            min="1"
            max="240"
            :value="kind.timeoutSecs / SECONDS_PER_MINUTE"
            @change="setTimeoutMinutes"
          />
        </label>
      </template>

      <template v-else-if="kind.type === 'condition'">
        <label>
          Pole wyniku
          <select
            :value="fieldKey(kind.field.nodeId, kind.field.field)"
            @change="setField"
          >
            <option
              v-if="!selectedField"
              :value="fieldKey(kind.field.nodeId, kind.field.field)"
              disabled
            >
              Wybierz pole
            </option>
            <option
              v-for="f in fields"
              :key="fieldKey(f.nodeId, f.field)"
              :value="fieldKey(f.nodeId, f.field)"
            >
              {{ f.label }}
            </option>
          </select>
        </label>
        <label>
          Porównanie
          <select
            :value="kind.op"
            @change="setKind({ ...kind, op: value($event) as CompareOp })"
          >
            <option
              v-for="op in opsFor(selectedField?.type)"
              :key="op"
              :value="op"
            >
              {{ opNames[op] }}
            </option>
          </select>
        </label>
        <label>
          Wartość
          <select
            v-if="selectedField?.type.type === 'choice'"
            :value="String(kind.value)"
            @change="setConditionValue"
          >
            <option
              v-for="option in selectedField.type.options"
              :key="option"
              :value="option"
            >
              {{ verdictNames[option] ?? option }}
            </option>
          </select>
          <select
            v-else-if="selectedField?.type.type === 'boolean'"
            :value="String(kind.value)"
            @change="setConditionValue"
          >
            <option value="true">tak</option>
            <option value="false">nie</option>
          </select>
          <input
            v-else
            :type="selectedField?.type.type === 'number' ? 'number' : 'text'"
            :value="kind.value ?? ''"
            @change="setConditionValue"
          />
        </label>
        <label>
          Limit rund poprawek
          <input
            type="number"
            min="1"
            :max="MAX_REPAIR_ROUNDS"
            :value="kind.repairLimit ?? ''"
            placeholder="brak (to nie jest pętla poprawek)"
            @change="setRepairLimit"
          />
        </label>
      </template>

      <label v-else-if="kind.type === 'approval'">
        Opis decyzji
        <textarea
          :value="kind.description"
          rows="3"
          @change="setKind({ ...kind, description: value($event) })"
        />
      </label>

      <label v-else-if="kind.type === 'end'">
        Wynik przebiegu
        <select
          :value="kind.outcome"
          @change="setKind({ ...kind, outcome: value($event) as EndOutcome })"
        >
          <option v-for="o in outcomes" :key="o.id" :value="o.id">
            {{ o.name }}
          </option>
        </select>
      </label>

      <template v-if="ports.length">
        <h4>Połączenia</h4>
        <label v-for="port in ports" :key="port">
          Wyjście „{{ portNames[port] }}” prowadzi do
          <select
            :value="connectionTarget(graph, node.id, port) ?? ''"
            @change="setTarget(port, $event)"
          >
            <option value="">— niepołączone —</option>
            <option v-for="t in targets" :key="t.id" :value="t.id">
              {{ t.label }} ({{ nodeTypeNames[t.kind.type] }})
            </option>
          </select>
        </label>
      </template>

      <button
        v-if="node.kind.type !== 'trigger'"
        type="button"
        class="delete-button"
        @click="remove"
      >
        Usuń krok
      </button>
    </div>
  </section>
</template>

<style scoped>
.wf-node-panel h3 {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
  margin: 0 0 var(--space-2);
}
.wf-node-id {
  color: var(--muted);
  font-size: var(--fs-xs);
  font-weight: 400;
}
.wf-node-panel h4 {
  margin: var(--space-3) 0 0;
  font-size: var(--fs-sm);
}
.wf-node-panel textarea {
  width: 100%;
  resize: vertical;
}
.wf-node-issues {
  margin: 0 0 var(--space-2);
  padding-left: var(--space-4);
  color: var(--warning);
  font-size: var(--fs-sm);
}
</style>
