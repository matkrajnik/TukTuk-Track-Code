import type { Session, Snapshot, Task } from "./store";
import { dayBounds, dayKey, inDay } from "./time";
import { dayTarget, type TargetSettings } from "./workday";

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

/** Nadgodziny dnia (ms) przy znanej sumie pracy: nadwyżka ponad normę dnia. */
export const overtimeOf = (
  worked: number,
  day: string,
  settings: TargetSettings,
) => Math.max(0, worked - dayTarget(settings, day));

/**
 * Nadgodziny dnia (ms) z pomiarów (B4, B5): praca dnia ponad normę, czyli cel
 * dzienny z Ustawień → Czas pracy; w dzień wolny lub święto norma to 0, więc
 * cała praca to nadgodziny. `sessions` mogą obejmować inne dni — liczy się
 * część w `day` (podział przez lokalną północ, trwający pomiar do `now`).
 */
export function overtime(
  day: string,
  sessions: readonly Session[],
  settings: TargetSettings,
  now: number = Date.now(),
): number {
  const worked = sessions.reduce((sum, s) => sum + inDay(s, day, now), 0);
  return overtimeOf(worked, day, settings);
}

/**
 * Dodaje przedział `[start, end)` do sum dni, dzieląc go przez lokalne
 * północe (doba zmiany czasu ma 23 albo 25 godzin).
 */
export function addToDays(
  totals: Map<string, number>,
  start: number,
  end: number,
) {
  let cursor = start;
  while (cursor < end) {
    const day = dayKey(cursor);
    const next = Math.min(end, dayBounds(day)[1]);
    totals.set(day, (totals.get(day) ?? 0) + next - cursor);
    cursor = next;
  }
}

/**
 * Sumy dni z zakończonych przedziałów zadań ze snapshotu, jednym przejściem
 * po sesjach. Nie zależy od bieżącej chwili; trwający pomiar dolicza się
 * osobno przez `addToDays(totals, start, now)`. Dla każdego dnia wynik jest
 * równy `dayTotal` bez trwającego pomiaru.
 */
export function closedDayTotals(data: Snapshot): Map<string, number> {
  const ids = new Set(data.tasks.map((t) => t.id));
  const totals = new Map<string, number>();
  for (const s of data.sessions)
    if (s.endedAt !== null && ids.has(s.taskId))
      addToDays(totals, s.startedAt, s.endedAt);
  return totals;
}

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

/**
 * Rozproszenie (PLAN_1.7.md, W2): wskaźniki dnia liczone z przedziałów pomiaru
 * i bloków skupienia. Wspólne dla Dashboardu i przeglądu tygodnia.
 *
 * - **Przełączenie**: start pomiaru innego zadania niż poprzedni pomiar, nie
 *   później niż 15 min po jego końcu. Liczy się w dniu startu (lokalnie), więc
 *   przełączenie tuż po północy należy do nowego dnia. Powrót do tego samego
 *   zadania i start po dłuższej przerwie nie są przełączeniami.
 * - **Nieprzerwany pomiar**: kolejne przedziały tego samego zadania z przerwą
 *   krótszą niż minuta (np. po „Podziel”) liczą się jako jeden. Pomiar przez
 *   północ dzieli się lokalną północą; trwający liczy się do `now`.
 * - **Bloki skupienia**: rozpoczęte w danym dniu i ukończone z nich.
 */
export const SWITCH_WINDOW_MS = 15 * 60_000;
/** Krótsza przerwa w pomiarze tego samego zadania nie przerywa pomiaru. */
export const CONTINUE_GAP_MS = 60_000;

/** Blok skupienia (`focus_blocks`): `completed` null = trwa. */
export interface FocusBlockRow {
  start: number;
  end: number;
  completed: boolean | null;
}

export interface Distraction {
  /** Przełączenia na inne zadanie w ciągu 15 min po poprzednim pomiarze. */
  switches: number;
  /** Liczba nieprzerwanych pomiarów w dniu. */
  measurements: number;
  /** Mediana i najdłuższy nieprzerwany pomiar (część w dniu, ms). */
  medianMs: number;
  longestMs: number;
  /** Bloki skupienia rozpoczęte w dniu i ukończone z nich. */
  focusStarted: number;
  focusDone: number;
}

const byStart = (sessions: readonly Session[]) =>
  [...sessions].sort((a, b) => a.startedAt - b.startedAt || a.id - b.id);

/** Mediana liczb (0 dla pustej listy). */
function middle(values: number[]): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

/** Przełączenia ze startem w `[from, to)`. */
export function switchCount(
  sessions: readonly Session[],
  from: number,
  to: number,
  now: number,
): number {
  const sorted = byStart(sessions);
  let count = 0;
  for (let i = 1; i < sorted.length; i++) {
    const s = sorted[i];
    if (s.startedAt < from || s.startedAt >= to) continue;
    const previous = sorted[i - 1];
    const end = previous.endedAt ?? now;
    if (
      previous.taskId !== s.taskId &&
      s.startedAt - end <= SWITCH_WINDOW_MS
    )
      count++;
  }
  return count;
}

/** Nieprzerwane pomiary `[start, end)`: przedziały tego samego zadania złączone. */
export function continuousRuns(
  sessions: readonly Session[],
  now: number,
): [number, number][] {
  const runs: { taskId: number; start: number; end: number }[] = [];
  for (const s of byStart(sessions)) {
    const end = Math.max(s.startedAt, s.endedAt ?? now);
    const last = runs[runs.length - 1];
    if (
      last &&
      last.taskId === s.taskId &&
      s.startedAt - last.end < CONTINUE_GAP_MS
    )
      last.end = Math.max(last.end, end);
    else runs.push({ taskId: s.taskId, start: s.startedAt, end });
  }
  return runs.map((r) => [r.start, r.end]);
}

/** Wskaźniki rozproszenia dnia `day` (podział przez lokalną północ). */
export function dayDistraction(
  sessions: readonly Session[],
  day: string,
  now: number,
  blocks: readonly FocusBlockRow[] = [],
): Distraction {
  const [from, to] = dayBounds(day);
  const parts = continuousRuns(sessions, now)
    .map(([a, b]) => Math.min(b, to) - Math.max(a, from))
    .filter((ms) => ms > 0);
  const started = blocks.filter((b) => b.start >= from && b.start < to);
  return {
    switches: switchCount(sessions, from, to, now),
    measurements: parts.length,
    medianMs: middle(parts),
    longestMs: parts.length ? Math.max(...parts) : 0,
    focusStarted: started.length,
    focusDone: started.filter((b) => b.completed === true).length,
  };
}

export interface DistractionAverage {
  /** Dni z co najmniej jednym pomiarem, z których liczona jest średnia. */
  days: number;
  switches: number;
  /** Mediana wszystkich nieprzerwanych pomiarów z tych dni. */
  medianMs: number;
  /** Średni najdłuższy pomiar dnia. */
  longestMs: number;
  focusDone: number;
  focusStarted: number;
}

/**
 * Średnia z dni `days` (np. 7 ostatnich dni roboczych). Dni bez żadnego pomiaru
 * (urlop, choroba) nie zaniżają średniej.
 */
export function averageDistraction(
  sessions: readonly Session[],
  days: readonly string[],
  now: number,
  blocks: readonly FocusBlockRow[] = [],
): DistractionAverage {
  const runs = continuousRuns(sessions, now);
  const all: number[] = [];
  const worked = days
    .map((day) => {
      const [from, to] = dayBounds(day);
      all.push(
        ...runs
          .map(([a, b]) => Math.min(b, to) - Math.max(a, from))
          .filter((ms) => ms > 0),
      );
      return dayDistraction(sessions, day, now, blocks);
    })
    .filter((d) => d.measurements > 0);
  const avg = (pick: (d: Distraction) => number) =>
    worked.length
      ? worked.reduce((sum, d) => sum + pick(d), 0) / worked.length
      : 0;
  return {
    days: worked.length,
    switches: avg((d) => d.switches),
    medianMs: middle(all),
    longestMs: avg((d) => d.longestMs),
    focusDone: avg((d) => d.focusDone),
    focusStarted: avg((d) => d.focusStarted),
  };
}

/** Liczba z jednym miejscem po przecinku bez zbędnego „,0”, np. „1,5”, „2”. */
export const oneDecimal = (value: number) =>
  (Math.round(value * 10) / 10).toLocaleString("pl-PL", {
    maximumFractionDigits: 1,
  });
