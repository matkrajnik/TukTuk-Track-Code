import type { Task } from "./store";

/**
 * Jeden komparator kolejności zadań: aktywny timer, priorytet malejąco,
 * ręczna kolejność rosnąco. Przy remisie decyduje kolejność snapshotu
 * (sortowanie jest stabilne; backend zwraca zadania według id malejąco).
 * Bez `activeTaskId` aktywny pomiar nie jest wyróżniany.
 */
export function compareTasks(activeTaskId?: number | null) {
  return (a: Task, b: Task) =>
    Number(activeTaskId != null && b.id === activeTaskId) -
      Number(activeTaskId != null && a.id === activeTaskId) ||
    (b.details?.priority ?? 0) - (a.details?.priority ?? 0) ||
    (a.details?.order ?? 0) - (b.details?.order ?? 0);
}

export function sortTasks(tasks: Task[], activeTaskId?: number | null) {
  return tasks.sort(compareTasks(activeTaskId));
}
