import type { Session, Snapshot, Task } from "./store";
import { dayKey, inDay } from "./time";

export interface DayRow {
  task: Task;
  /** Przedziały zadania obejmujące dzień, od najnowszego. */
  sessions: Session[];
  /** Czas przypadający na wybrany dzień (podział przez lokalną północ). */
  total: number;
}

export const decimalHours = (milliseconds: number) =>
  (milliseconds / 3_600_000).toFixed(2).replace(".", ",");

export const taskSessions = (data: Snapshot, taskId: number) =>
  data.sessions
    .filter((s) => s.taskId === taskId)
    .sort((a, b) => b.startedAt - a.startedAt || b.id - a.id);

/**
 * Zadania z pracą w danym dniu, w kolejności snapshotu. Trwający pomiar
 * rozpoczęty tego dnia jest uwzględniony nawet przy zerowym czasie.
 */
export function dayRows(data: Snapshot, day: string, now: number): DayRow[] {
  return data.tasks
    .map((task) => {
      const sessions = taskSessions(data, task.id).filter(
        (s) =>
          inDay(s, day, now) > 0 ||
          (s.endedAt === null && dayKey(s.startedAt) === day),
      );
      return {
        task,
        sessions,
        total: sessions.reduce((sum, s) => sum + inDay(s, day, now), 0),
      };
    })
    .filter((row) => row.sessions.length);
}

/** Kolejność raportu dziennego: najwięcej czasu, potem id. */
export const byTotal = (a: DayRow, b: DayRow) =>
  b.total - a.total || a.task.id - b.task.id;

export const rowsTotal = (rows: DayRow[]) =>
  rows.reduce((sum, row) => sum + row.total, 0);

export interface TsOptions {
  /** Dołącza notatkę zadania z danego dnia po tytule. */
  includeNotes?: boolean;
  /** Nagłówek kolumny czasu. */
  hoursHeader?: string;
  /** Usuwa białe znaki z początku i końca opisu wiersza. */
  trim?: boolean;
}

/**
 * Raport TS: tytuł z datą, wiersz na zadanie w godzinach dziesiętnych
 * (zaokrąglenie per wiersz) i suma liczona z pełnego czasu.
 */
export function tsText(day: string, rows: DayRow[], options: TsOptions = {}) {
  const { includeNotes = true, hoursHeader = "Czas (h)", trim = true } =
    options;
  return [
    `Raport TS — ${day}`,
    `Zadanie\t${hoursHeader}`,
    ...rows.map(({ task, total }) => {
      const label = [task.title, includeNotes ? task.details?.notes?.[day] : ""]
        .filter(Boolean)
        .join(" — ")
        .replace(/\s+/g, " ");
      return `${trim ? label.trim() : label}\t${decimalHours(total)}`;
    }),
    `Razem\t${decimalHours(rowsTotal(rows))}`,
  ].join("\n");
}
