import type { Task } from "./store";

/**
 * Jeden komparator kolejności zadań: aktywny timer, priorytet malejąco,
 * ręczna kolejność rosnąco, id malejąco (nowsze wyżej).
 * Bez `activeTaskId` aktywny pomiar nie jest wyróżniany.
 */
export function compareTasks(activeTaskId?: number | null) {
  return (a: Task, b: Task) =>
    Number(activeTaskId != null && b.id === activeTaskId) -
      Number(activeTaskId != null && a.id === activeTaskId) ||
    (b.details?.priority ?? 0) - (a.details?.priority ?? 0) ||
    (a.details?.order ?? 0) - (b.details?.order ?? 0) ||
    b.id - a.id;
}

export function sortTasks(tasks: Task[], activeTaskId?: number | null) {
  return tasks.sort(compareTasks(activeTaskId));
}

/** Wpis komendy `reorder_tasks`: nowa pozycja i priorytet (pasmo) zadania. */
export interface ReorderEntry {
  id: number;
  order: number;
  priority: number;
}

/**
 * Przeniesienie zadania `id` na miejsce zadania `targetId` z tej samej listy
 * widocznej (`visible`, w kolejności wyświetlania). Przesunięcie w dół stawia
 * zadanie za celem, w górę — przed nim; zadanie przejmuje priorytet celu, więc
 * przejście do innego pasma zmienia priorytet.
 *
 * Kolejność liczona jest na pełnej liście `all` (bez wyróżniania timera), żeby
 * zadania ukryte filtrem zachowały swoje miejsca. Wynik to całe docelowe pasmo
 * ponumerowane od 1 krokiem 1, albo `null`, gdy nie ma czego zmieniać.
 */
export function planMove(
  all: Task[],
  visible: Task[],
  id: number,
  targetId: number,
): ReorderEntry[] | null {
  if (id === targetId) return null;
  const from = visible.findIndex((t) => t.id === id);
  const to = visible.findIndex((t) => t.id === targetId);
  const target = visible[to];
  if (from < 0 || !target) return null;
  const down = to > from;
  const priority = target.details?.priority ?? 0;
  const list = [...all].sort(compareTasks()).filter((t) => t.id !== id);
  const moved = all.find((t) => t.id === id);
  const at = list.findIndex((t) => t.id === targetId);
  if (!moved || at < 0) return null;
  list.splice(down ? at + 1 : at, 0, moved);
  return list
    .filter((t) => t.id === id || (t.details?.priority ?? 0) === priority)
    .map((t, index) => ({ id: t.id, order: index + 1, priority }));
}

/**
 * Sąsiad dla przesunięcia o jedną pozycję (Alt+↑ / Alt+↓). Pomija zadanie
 * z trwającym pomiarem, które zawsze stoi pierwsze i nie zmienia przez to miejsca.
 */
export function neighbour(
  visible: Task[],
  id: number,
  delta: -1 | 1,
  skipId?: number | null,
): Task | undefined {
  let index = visible.findIndex((t) => t.id === id);
  if (index < 0) return undefined;
  do index += delta;
  while (visible[index] && visible[index].id === skipId);
  return visible[index];
}
