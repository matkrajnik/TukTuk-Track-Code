import type { Task, TsProject, TsProjectMapping } from "./store";

/**
 * Wysyłka Raportu TS do Ideo Timesheet (PLAN_1.8.md): czyste funkcje bez
 * wywołań backendu, testowane jednostkowo.
 */

/** Zadanie TS przypisane zadaniu Todo Track i skąd pochodzi. */
export interface ResolvedTsTask {
  id: string;
  name: string;
  /** `task` – jawny wybór w zadaniu, `project` – domyślne zadanie projektu. */
  source: "task" | "project";
}

/** Klucz nazwy projektu jak SQLite `COLLATE NOCASE` (tylko litery ASCII). */
export const tsProjectKey = (project: string) =>
  project.trim().replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * Zadanie TS dla zadania Todo Track: jawne `tsTaskId` zadania → domyślne
 * zadanie projektu (`ts_project_map`, nazwa projektu bez wielkości liter) →
 * brak (`null`). Mapowanie projektu nie jest kopiowane do zadania, więc jego
 * zmiana działa od razu na wszystkie zadania bez jawnego wyboru.
 */
export function resolveTsTask(
  task: Pick<Task, "details">,
  map: readonly TsProjectMapping[],
): ResolvedTsTask | null {
  const id = task.details?.tsTaskId?.trim() ?? "";
  if (id)
    return { id, name: task.details?.tsTaskName?.trim() || id, source: "task" };
  const project = tsProjectKey(task.details?.project ?? "");
  if (!project) return null;
  const mapped = map.find((m) => tsProjectKey(m.project) === project);
  const mappedId = mapped?.tsTaskId.trim() ?? "";
  if (!mapped || !mappedId) return null;
  return {
    id: mappedId,
    name: mapped.tsTaskName.trim() || mappedId,
    source: "project",
  };
}

/** Zadanie TS o tym identyfikatorze na liście projektów TS (albo `null`). */
export function findTsTask(
  projects: readonly TsProject[],
  id: string,
): { project: string; id: string; name: string } | null {
  for (const p of projects)
    for (const t of p.tasks)
      if (t.id === id) return { project: p.name, id: t.id, name: t.name };
  return null;
}
