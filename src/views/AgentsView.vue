<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { Check, CheckCheck } from "lucide-vue-next";
import { agentTaskLabel } from "../agents";
import {
  boardCounts,
  markAgentSeen,
  markAllAgentsSeen,
  onAgentBoardChanged,
  readAgentBoard,
  type BoardEntry,
  type BoardPhase,
} from "../agentboard";
import { duration } from "../store";

/**
 * Tablica sesji agentów: co czeka na użytkownika, co pracuje, co się skończyło
 * i nie zostało przejrzane. Dane z hooków Claude Code i Codexa (`agent_board.rs`).
 */
const props = defineProps<{ now: number }>();

const list = ref<BoardEntry[]>([]);
const loaded = ref(false);
const error = ref("");
const kinds: Record<BoardEntry["kind"], string> = {
  claude: "Claude",
  codex: "Codex",
};
const sections: { phase: BoardPhase; title: string; empty: string }[] = [
  {
    phase: "waiting",
    title: "Czeka na Ciebie",
    empty: "Żadna sesja nie czeka na odpowiedź.",
  },
  { phase: "working", title: "Pracuje", empty: "Żaden agent teraz nie pracuje." },
  {
    phase: "done",
    title: "Skończone, nieprzejrzane",
    empty: "Nie ma zakończonych sesji do przejrzenia.",
  },
];
const byPhase = computed(() => {
  const groups: Record<BoardPhase, BoardEntry[]> = {
    waiting: [],
    working: [],
    done: [],
  };
  for (const entry of list.value) groups[entry.phase].push(entry);
  return groups;
});
const counts = computed(() => boardCounts(list.value));
const unseen = computed(() => counts.value.waiting + counts.value.done);

const clock = (ms: number) =>
  new Date(ms).toLocaleTimeString("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
  });
/** Ile trwa bieżąca faza: „od 12 min” dla czekania i pracy, godzina dla końca. */
function timing(entry: BoardEntry): string {
  if (entry.phase === "done") return clock(entry.phaseAt);
  return duration(Math.max(0, props.now - entry.phaseAt));
}
function detail(entry: BoardEntry): string {
  if (entry.phase === "done" && entry.timedOut)
    return "bez sygnału końca (brak zdarzeń hooków)";
  if (entry.phase === "working" && entry.subagents > 0)
    return entry.subagents === 1 ? "1 subagent" : `subagenci: ${entry.subagents}`;
  return entry.reason ?? "";
}
const entryKey = (e: BoardEntry) => `${e.kind}|${e.id}`;

/** Komunikat dla czytników ekranu po „Przejrzane”. */
const status = ref("");
/**
 * Licznik zdarzeń `agent-board-changed`: wynik odczytu albo „Przejrzane”, który
 * przyszedł po nowszym zdarzeniu, jest starszy od listy i zostaje pominięty.
 */
let version = 0;
async function run(action: () => Promise<BoardEntry[]>): Promise<boolean> {
  error.value = "";
  const started = version;
  try {
    const next = await action();
    if (version === started) list.value = next;
    return true;
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
    return false;
  }
}
/** Po zniknięciu klikniętego przycisku fokus wraca na nagłówek sekcji (WCAG 2.4.3). */
async function focusSection(phase: BoardPhase) {
  await nextTick();
  document.getElementById(`board-${phase}-title`)?.focus();
}
async function seen(entry: BoardEntry) {
  if (await run(() => markAgentSeen(entry))) {
    status.value = `Oznaczono jako przejrzane: ${kinds[entry.kind]} · ${entry.name}`;
    await focusSection(entry.phase);
  }
}
async function seenAll() {
  if (await run(markAllAgentsSeen)) {
    status.value = "Oznaczono wszystkie sesje jako przejrzane";
    await focusSection("waiting");
  }
}

let alive = true;
let unlisten: (() => void) | undefined;
onMounted(async () => {
  try {
    const stop = await onAgentBoardChanged((next) => {
      version++;
      list.value = next;
    });
    if (!alive) return stop();
    unlisten = stop;
    await run(readAgentBoard);
  } catch (e) {
    error.value = String(e instanceof Error ? e.message : e);
  } finally {
    loaded.value = true;
  }
});
onUnmounted(() => {
  alive = false;
  unlisten?.();
});
</script>

<template>
  <div class="page-heading">
    <h1>Sesje agentów</h1>
    <div class="planning-actions">
      <button
        v-if="unseen > 0"
        class="today-button board-all"
        @click="seenAll"
      >
        <CheckCheck :size="16" aria-hidden="true" />Oznacz wszystkie jako
        przejrzane
      </button>
    </div>
  </div>
  <p v-if="error" class="error" role="alert">{{ error }}</p>
  <p class="sr-only" role="status">{{ status }}</p>
  <p v-if="loaded && !list.length" class="report-note board-empty">
    Brak sesji do pokazania. Tablica zapełnia się zdarzeniami hooków Claude
    Code i Codexa (Ustawienia → Agenci) i jest pusta po uruchomieniu aplikacji.
  </p>
  <div class="board">
    <section
      v-for="section in sections"
      :key="section.phase"
      class="dashboard-card board-section"
      :class="`board-${section.phase}`"
      :aria-labelledby="`board-${section.phase}-title`"
    >
      <h2 :id="`board-${section.phase}-title`" tabindex="-1">
        {{ section.title }}
        <span class="board-count">{{ byPhase[section.phase].length }}</span>
      </h2>
      <ul v-if="byPhase[section.phase].length" class="board-list">
        <li
          v-for="entry in byPhase[section.phase]"
          :key="entryKey(entry)"
          class="board-item"
          :class="{ 'board-seen': entry.seen && entry.phase !== 'working' }"
        >
          <div class="board-line">
            <span class="board-name">
              <span
                v-if="entry.phase === 'working'"
                class="agent-dot"
                aria-hidden="true"
              />
              <span
                v-else
                class="board-mark"
                :class="`board-mark-${entry.phase}`"
                aria-hidden="true"
              />{{ kinds[entry.kind] }} · {{ entry.name }}
            </span>
            <span class="board-time mono">{{ timing(entry) }}</span>
          </div>
          <p v-if="detail(entry)" class="board-reason">{{ detail(entry) }}</p>
          <p
            v-if="entry.branch || agentTaskLabel(entry)"
            class="agent-work"
          >
            <span v-if="entry.branch" class="agent-branch"
              >{{ entry.detached ? "odłączony HEAD" : "gałąź" }}
              <code class="mono">{{ entry.branch }}</code></span
            >
            <span v-if="agentTaskLabel(entry)" class="agent-task">{{
              agentTaskLabel(entry)
            }}</span>
          </p>
          <p class="board-folder mono" :title="entry.cwd">{{ entry.cwd }}</p>
          <button
            v-if="entry.phase !== 'working' && !entry.seen"
            class="text-link board-seen-button"
            :aria-label="`Oznacz jako przejrzane: ${kinds[entry.kind]} · ${entry.name}`"
            @click="seen(entry)"
          >
            <Check :size="14" aria-hidden="true" />Przejrzane
          </button>
          <span v-else-if="entry.phase === 'waiting'" class="board-note"
            >przejrzane, nadal czeka</span
          >
        </li>
      </ul>
      <p v-else class="report-note">{{ section.empty }}</p>
    </section>
  </div>
</template>

<style scoped>
.board {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 16px;
  align-items: start;
}
@media (max-width: 1007px) {
  .board {
    grid-template-columns: minmax(0, 1fr);
  }
}
.board-section h2 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 0 0 12px;
}
.board-count {
  min-width: 22px;
  padding: 0 6px;
  border-radius: 11px;
  background: var(--raised);
  color: var(--muted);
  font-size: var(--fs-sm);
  font-weight: 600;
  text-align: center;
  font-variant-numeric: tabular-nums;
}
.board-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
}
.board-item {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid var(--line);
  border-radius: var(--radius);
  min-width: 0;
}
.board-waiting .board-item:not(.board-seen) {
  border-left: 3px solid var(--warning);
}
/* Przejrzane czekanie: przerywana ramka zamiast przezroczystości (kontrast tekstu). */
.board-seen {
  border-style: dashed;
}
.board-section h2:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.board-line {
  display: flex;
  justify-content: space-between;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
/* Nazwa folderu łamie się zamiast ucinać: w trzech kolumnach bywa wąsko. */
.board-name {
  min-width: 0;
  overflow-wrap: anywhere;
  font-weight: 600;
}
.board-time {
  flex: none;
  color: var(--muted);
  font-variant-numeric: tabular-nums;
}
.board-mark {
  display: inline-block;
  width: 8px;
  height: 8px;
  margin-right: 8px;
  border-radius: 50%;
}
.board-mark-waiting {
  background: var(--warning);
}
.board-mark-done {
  border: 2px solid var(--muted);
  width: 6px;
  height: 6px;
}
.board-reason {
  margin: 0;
  color: var(--text);
  font-size: var(--fs-sm);
  overflow-wrap: anywhere;
}
.board-folder {
  margin: 0;
  color: var(--muted);
  font-size: var(--fs-xs);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.board-item .agent-work {
  margin: 0;
}
.board-seen-button {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  align-self: flex-start;
}
.board-note {
  color: var(--muted);
  font-size: var(--fs-xs);
}
.board-all {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.board-empty {
  margin-bottom: 16px;
}
</style>
