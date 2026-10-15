import type { Session } from "./store";

/**
 * Czas agentów jako osobny tor (PLAN_1.7.md, W1): okres pracy agenta AI z tabeli
 * `agent_sessions` (`agent_time::Period`). Nie wchodzi do celu dnia, raportu TS
 * ani eksportu.
 */
export interface AgentPeriod {
  id: number;
  /** `claude` albo `codex`. */
  agent: string;
  folder: string;
  /** Dopasowane zadanie; `null` = agent bez zadania (szary blok). */
  taskId: number | null;
  start: number;
  end: number;
}
/** Czas agentów nad zadaniem i część nakładająca się na Twój pomiar tego zadania. */
export interface AgentTotal {
  taskId: number;
  agentMs: number;
  parallelMs: number;
}
/** Wynik `agent_time(from, to)`. */
export interface AgentTime {
  periods: AgentPeriod[];
  totals: AgentTotal[];
  unmatchedMs: number;
}
export const emptyAgentTime = (): AgentTime => ({
  periods: [],
  totals: [],
  unmatchedMs: 0,
});

type Span = [number, number];

/** Suma przedziałów: posortowane, bez nakładania (`agent_time::union`). */
export function union(spans: Span[]): Span[] {
  const out: Span[] = [];
  for (const [a, b] of spans.filter(([a, b]) => b > a).sort((x, y) => x[0] - y[0] || x[1] - y[1])) {
    const last = out[out.length - 1];
    if (last && a <= last[1]) last[1] = Math.max(last[1], b);
    else out.push([a, b]);
  }
  return out;
}
const length = (spans: Span[]) => spans.reduce((sum, [a, b]) => sum + b - a, 0);
/** Długość części wspólnej dwóch sum przedziałów (`agent_time::overlap`). */
export function overlap(a: Span[], b: Span[]): number {
  let i = 0;
  let j = 0;
  let sum = 0;
  while (i < a.length && j < b.length) {
    const from = Math.max(a[i][0], b[j][0]);
    const to = Math.min(a[i][1], b[j][1]);
    if (to > from) sum += to - from;
    if (a[i][1] < b[j][1]) i++;
    else j++;
  }
  return sum;
}

/**
 * Sumy według zadania w `[from, to)` jak `agent_time::totals`: czas agentów
 * (równoczesna praca dwóch agentów liczona raz) i część równoległa z pomiarem
 * tego samego zadania; trwający pomiar kończy się w `now`.
 */
export function agentTotals(
  periods: AgentPeriod[],
  sessions: Session[],
  from: number,
  to: number,
  now: number,
): { totals: AgentTotal[]; unmatchedMs: number } {
  const clip = (a: number, b: number): Span => [Math.max(a, from), Math.min(b, to)];
  const byTask = new Map<number, Span[]>();
  const unmatched: Span[] = [];
  for (const p of periods) {
    if (p.taskId === null) unmatched.push(clip(p.start, p.end));
    else {
      const list = byTask.get(p.taskId) ?? [];
      list.push(clip(p.start, p.end));
      byTask.set(p.taskId, list);
    }
  }
  const totals: AgentTotal[] = [];
  for (const [taskId, spans] of [...byTask].sort((a, b) => a[0] - b[0])) {
    const agent = union(spans);
    const mine = union(
      sessions
        .filter((s) => s.taskId === taskId)
        .map((s) => clip(s.startedAt, Math.max(s.startedAt, s.endedAt ?? now))),
    );
    const agentMs = length(agent);
    if (agentMs > 0)
      totals.push({ taskId, agentMs, parallelMs: overlap(agent, mine) });
  }
  return { totals, unmatchedMs: length(union(unmatched)) };
}

/** Nazwa agenta w interfejsie. */
export const agentName = (agent: string) =>
  agent === "claude" ? "Claude Code" : agent === "codex" ? "Codex" : agent;
