import type { Session, Task } from "./store";
import { dayBounds, dayKey } from "./time";

/**
 * Lista „Ukończone” / „Wszystkie” w Zadaniach (PLAN_BACKLOG.md, B2): domyślnie
 * zadania ukończone w ostatnich 30 dniach (dziś i 29 dni wstecz), pogrupowane
 * po dniu ukończenia, starsze po „Pokaż starsze”. Czyste funkcje na snapshocie.
 */
export const RECENT_DONE_DAYS = 30;

/**
 * Chwila ukończenia: `doneAt` z bazy (od 2.0), a dla zadań ukończonych
 * wcześniej koniec ostatniego pomiaru, bez pomiarów — chwila utworzenia.
 */
export function doneMoments(tasks: Task[], sessions: Session[], now: number) {
  const lastEnd = new Map<number, number>();
  for (const s of sessions) {
    const end = s.endedAt ?? now;
    if (end > (lastEnd.get(s.taskId) ?? -Infinity)) lastEnd.set(s.taskId, end);
  }
  return new Map(
    tasks
      .filter((t) => t.done)
      .map((t) => [t.id, t.doneAt ?? lastEnd.get(t.id) ?? t.createdAt]),
  );
}

/** Pierwszy dzień okresu „ostatnie 30 dni” dla dnia `today`. */
export function recentDoneFrom(today: string) {
  const d = new Date(dayBounds(today)[0]);
  return dayKey(
    new Date(
      d.getFullYear(),
      d.getMonth(),
      d.getDate() - RECENT_DONE_DAYS + 1,
    ).getTime(),
  );
}

/**
 * Ukończone zadania pogrupowane po dniu ukończenia, najnowszy dzień pierwszy.
 * Kolejność w dniu zostaje taka, jak w `tasks` (ręczna kolejność listy).
 */
export function groupByDoneDay(tasks: Task[], moments: Map<number, number>) {
  const days = new Map<string, Task[]>();
  for (const task of tasks) {
    const day = dayKey(moments.get(task.id) ?? task.createdAt);
    days.set(day, [...(days.get(day) ?? []), task]);
  }
  return [...days]
    .sort((a, b) => b[0].localeCompare(a[0]))
    .map(([day, list]) => ({ day, tasks: list }));
}
