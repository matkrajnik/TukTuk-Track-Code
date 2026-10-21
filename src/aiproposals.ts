/**
 * „Zastosuj” propozycji agenta (serwer MCP, A2): zmiana idzie przez istniejące
 * komendy (`quick_add`, `save_day_notes`, `agent_bind`, `mutate start`,
 * `plan_task`), tak jak po kliknięciu użytkownika w zwykłym interfejsie. Sam
 * agent niczego nie zapisuje; bez „Zastosuj” baza zostaje bez zmian.
 */
import type { McpProposal } from "./api/mcp";
import { nextOrder } from "./review";
import {
  bindAgentFolder,
  change,
  detailsOf,
  planTask,
  quickAdd,
  read,
  saveDayNotes,
  type NewTask,
  type Snapshot,
  type Task,
} from "./store";

const REDMINE_KEY = /^#(\d+)$/;
const JIRA_KEY = /^[A-Za-z][A-Za-z0-9]+-\d+$/;

/** Ładunek `quick_add` dla `task_create`: klucz Jiry albo `#numer` Redmine, pola planu. */
export function newTaskOf(p: McpProposal): NewTask {
  const key = p.key.trim();
  const redmine = REDMINE_KEY.exec(key);
  const planned = !!(p.project || p.day || p.estimate);
  return {
    title: p.title,
    jiraKey: JIRA_KEY.test(key) ? key.toUpperCase() : null,
    redmineId: redmine ? Number(redmine[1]) : null,
    details: planned
      ? {
          ...detailsOf({ id: 0, title: "", done: false, createdAt: 0 }),
          project: p.project,
          plannedDay: p.day,
          estimateMinutes: p.estimate,
        }
      : null,
    start: false,
    template: null,
  };
}

/** Notatka dnia po dopisaniu zdania do istniejącej (nowy wiersz). */
export function appendedNote(existing: string | undefined, text: string): string {
  const old = (existing ?? "").trimEnd();
  return old ? `${old}\n${text}` : text;
}

/**
 * Stosuje propozycję i zwraca nową migawkę. Błąd (zadanie usunięte w międzyczasie,
 * klucz Jiry bez połączonego konta) przechodzi do wywołującego, a propozycja
 * zostaje w kolejce.
 */
export async function applyProposal(
  p: McpProposal,
  tasks: readonly Task[],
): Promise<Snapshot> {
  const taskId = p.taskId;
  switch (p.kind) {
    case "task_create":
      return quickAdd(newTaskOf(p));
    case "note_append": {
      if (taskId == null) throw new Error("Propozycja nie wskazuje zadania.");
      const task = tasks.find((t) => t.id === taskId);
      if (!task) throw new Error("Nie znaleziono zadania.");
      return saveDayNotes(p.day, [
        { id: taskId, note: appendedNote(task.details?.notes?.[p.day], p.text) },
      ]);
    }
    case "link_folder":
      if (taskId == null) throw new Error("Propozycja nie wskazuje zadania.");
      await bindAgentFolder(p.folder, taskId);
      return read();
    case "switch":
      if (taskId == null) throw new Error("Propozycja nie wskazuje zadania.");
      return change("start", taskId);
    case "plan":
      if (taskId == null) throw new Error("Propozycja nie wskazuje zadania.");
      return planTask(taskId, p.day, nextOrder(tasks, p.day));
  }
}
