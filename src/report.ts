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

/** Suma pracy dnia (wszystkie zadania, podział przez lokalną północ). */
export const dayTotal = (data: Snapshot, day: string, now: number) =>
  rowsTotal(dayRows(data, day, now));

/**
 * Układ raportu TS:
 * - `day`: jeden dzień, kolumny Zadanie i czas (raport dzienny, DayClose);
 * - `byDay`: kolumny Dzień, Zadanie, czas, suma po każdym dniu i na końcu;
 * - `summary`: zadania zsumowane ze wszystkich dni, notatki łączone „; ”.
 */
export type TsLayout = "day" | "byDay" | "summary";

export interface TsOptions {
  /** Domyślnie `day` dla jednego dnia i `byDay` dla kilku. */
  layout?: TsLayout;
  /** Dołącza notatkę zadania z danego dnia po tytule. */
  includeNotes?: boolean;
  /** Nagłówek kolumny czasu. */
  hoursHeader?: string;
  /** Usuwa białe znaki z początku i końca opisu wiersza. */
  trim?: boolean;
  /**
   * Pomija wiersze z zerowym czasem (np. pomiar rozpoczęty przed chwilą).
   * W układach wielodniowych zawsze włączone.
   */
  skipEmpty?: boolean;
  /** Kolejność wierszy dnia: najwięcej czasu (domyślnie) albo snapshotu. */
  order?: "total" | "snapshot";
}

/** Dni bez powtórzeń, rosnąco. */
export const sortedDays = (days: Iterable<string>) => [...new Set(days)].sort();

function rowsFor(
  data: Snapshot,
  day: string,
  now: number,
  skipEmpty: boolean,
  order: "total" | "snapshot",
) {
  const rows = dayRows(data, day, now).filter(
    (row) => !skipEmpty || row.total > 0,
  );
  return order === "total" ? rows.sort(byTotal) : rows;
}

const describe = (title: string, notes: string[], trim: boolean) => {
  const label = [title, notes.filter(Boolean).join("; ")]
    .filter(Boolean)
    .join(" — ")
    .replace(/\s+/g, " ");
  return trim ? label.trim() : label;
};

/** Tytuł raportu: jeden dzień albo zakres z liczbą dni. */
function heading(days: string[]) {
  if (days.length === 1) return `Raport TS — ${days[0]}`;
  if (!days.length) return "Raport TS — brak dni";
  return `Raport TS — ${days[0]} – ${days[days.length - 1]} (${days.length} dni)`;
}

/**
 * Raport TS z wybranych dni — jedyne źródło tekstu dla raportu dziennego,
 * zamknięcia dnia i trybu wielodniowego. Godziny dziesiętne z przecinkiem,
 * zaokrąglenie per wiersz, sumy liczone z pełnego czasu.
 */
export function tsText(
  days: string[],
  data: Snapshot,
  now: number,
  options: TsOptions = {},
) {
  const list = sortedDays(days);
  const {
    layout = list.length === 1 ? "day" : "byDay",
    includeNotes = true,
    hoursHeader = "Czas (h)",
    trim = true,
    order = "total",
  } = options;
  const skipEmpty = layout === "day" ? (options.skipEmpty ?? false) : true;
  const perDay = list.map((day) => ({
    day,
    rows: rowsFor(data, day, now, skipEmpty, order),
  }));
  const total = perDay.reduce((sum, d) => sum + rowsTotal(d.rows), 0);
  const note = (task: Task, day: string) =>
    includeNotes ? (task.details?.notes?.[day] ?? "") : "";

  if (layout === "day") {
    const { day, rows } = perDay[0] ?? { day: "", rows: [] };
    return [
      heading(list),
      `Zadanie\t${hoursHeader}`,
      ...rows.map(
        ({ task, total }) =>
          `${describe(task.title, [note(task, day)], trim)}\t${decimalHours(total)}`,
      ),
      `Razem\t${decimalHours(total)}`,
    ].join("\n");
  }

  if (layout === "byDay") {
    return [
      heading(list),
      `Dzień\tZadanie\t${hoursHeader}`,
      ...perDay
        .filter((d) => d.rows.length)
        .flatMap(({ day, rows }) => [
          ...rows.map(
            ({ task, total }) =>
              `${day}\t${describe(task.title, [note(task, day)], trim)}\t${decimalHours(total)}`,
          ),
          `Razem ${day}\t\t${decimalHours(rowsTotal(rows))}`,
        ]),
      `Razem\t\t${decimalHours(total)}`,
    ].join("\n");
  }

  const tasks = new Map<
    number,
    { task: Task; total: number; notes: string[] }
  >();
  for (const { day, rows } of perDay)
    for (const row of rows) {
      const entry = tasks.get(row.task.id) ?? {
        task: row.task,
        total: 0,
        notes: [],
      };
      entry.total += row.total;
      const text = note(row.task, day).trim();
      if (text && !entry.notes.includes(text)) entry.notes.push(text);
      tasks.set(row.task.id, entry);
    }
  return [
    heading(list),
    `Zadanie\t${hoursHeader}`,
    ...[...tasks.values()]
      .sort((a, b) => b.total - a.total || a.task.id - b.task.id)
      .map(
        ({ task, total, notes }) =>
          `${describe(task.title, notes, trim)}\t${decimalHours(total)}`,
      ),
    `Razem\t${decimalHours(total)}`,
  ].join("\n");
}
