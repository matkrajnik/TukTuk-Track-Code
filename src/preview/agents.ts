/** Podgląd przypisań folderów agentów i czasu agentów (`agents.rs`, `agent_time.rs`). */
import type { AgentBinding } from "../api/task";
import { read } from "./snapshot";
import { agentTotals, type AgentPeriod, type AgentTime } from "../agenttime";
const bindingsKey = "todo-track-preview-agent-bindings-v1";
/** Zdarzenie podglądu: przypisania folderów zmieniły się, dopasuj agentów od nowa. */
export const previewAgentsEvent = "todo-track-preview-agents";
/** Przypisania w podglądzie: `{ folder, taskId }` z localStorage. */
export function previewBindings(): { folder: string; taskId: number }[] {
  try {
    const value = JSON.parse(localStorage.getItem(bindingsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/** Postać zapisywana (jak `agents::canon_folder` w Windows): separatory `\`, bez końcowego. */
const trimFolder = (folder: string) =>
  folder.trim().replace(/[\\/]/g, "\\").replace(/\\+$/, "");
const sameFolder = (a: string, b: string) =>
  trimFolder(a).toLowerCase() === trimFolder(b).toLowerCase();
async function previewBindingList(): Promise<AgentBinding[]> {
  const data = await read();
  return previewBindings()
    .flatMap((b) => {
      const task = [...data.tasks, ...(data.trash ?? [])].find(
        (t) => t.id === b.taskId,
      );
      return task
        ? [{ folder: b.folder, taskId: b.taskId, title: task.title }]
        : [];
    })
    .sort((a, b) =>
      a.folder.toLowerCase().localeCompare(b.folder.toLowerCase()),
    );
}
function previewBindingsChanged(list: { folder: string; taskId: number }[]) {
  localStorage.setItem(bindingsKey, JSON.stringify(list));
  // Jak `agents::bind`: przypisanie uzupełnia okresy agentów bez zadania.
  previewAgentBackfill(Date.now());
  window.dispatchEvent(new Event(previewAgentsEvent));
}
export async function agentBindings(): Promise<AgentBinding[]> {
  return previewBindingList();
}
export async function bindAgentFolder(
  folder: string,
  taskId: number,
): Promise<AgentBinding[]> {
  const clean = trimFolder(folder);
  if (!clean || [...clean].length > 1024)
    throw new Error("Niepoprawny folder.");
  if (!(await read()).tasks.some((t) => t.id === taskId))
    throw new Error("Nie znaleziono zadania.");
  previewBindingsChanged([
    ...previewBindings().filter((b) => !sameFolder(b.folder, clean)),
    { folder: clean, taskId },
  ]);
  return previewBindingList();
}
/** Okresy pracy agentów w podglądzie (`agent_sessions`), dane z `seed.ts`. */
export const agentSessionsKey = "todo-track-preview-agent-sessions-v1";
function previewAgentPeriods(): AgentPeriod[] {
  try {
    const value = JSON.parse(localStorage.getItem(agentSessionsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/**
 * Jak `agent_time::backfill`: okresy z ostatnich 7 dni bez zadania dostają
 * zadanie z przypisania folderu (także podfolderu, wygrywa najdłuższe).
 */
function previewAgentBackfill(now: number) {
  const norm = (f: string) => trimFolder(f).toLowerCase();
  const bindings = previewBindings().map((b) => ({ ...b, norm: norm(b.folder) }));
  let changed = false;
  const periods = previewAgentPeriods().map((p) => {
    if (p.taskId !== null || p.end < now - 7 * 86_400_000) return p;
    const folder = norm(p.folder);
    const found = bindings
      .filter((b) => b.norm && (folder === b.norm || folder.startsWith(b.norm + "\\")))
      .sort((a, b) => b.norm.length - a.norm.length)[0];
    if (!found) return p;
    changed = true;
    return { ...p, taskId: found.taskId };
  });
  if (changed) localStorage.setItem(agentSessionsKey, JSON.stringify(periods));
}
export async function agentTime(from: number, to: number): Promise<AgentTime> {
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const periods = previewAgentPeriods()
    .filter((p) => p.start < to && p.end > from)
    .sort((a, b) => a.start - b.start || a.id - b.id);
  const now = Date.now();
  const { sessions, trashSessions } = await read();
  return {
    periods,
    ...agentTotals(periods, [...sessions, ...(trashSessions ?? [])], from, to, now),
  };
}
export async function unbindAgentFolder(
  folder: string,
): Promise<AgentBinding[]> {
  previewBindingsChanged(
    previewBindings().filter((b) => !sameFolder(b.folder, folder)),
  );
  return previewBindingList();
}
