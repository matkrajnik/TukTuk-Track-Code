import type { Task } from "./store";
import { compareTasks } from "./order";
import { isRecurring } from "./recurrence";
import { hm } from "./time";

/**
 * „Rozłóż” na tablicy tygodnia (PLAN_1.7.md, P1). Funkcje czyste: dostają
 * zadania, dni z pojemnością i czas zadania, więc testy jednostkowe nie
 * zależą od strefy czasowej ani od Vue.
 *
 * - Kandydaci: zadania z „Do zaplanowania” i „Zaległych”; zablokowane
 *   („Czekam na”) i cykliczne są pomijane.
 * - Kolejność: termin rosnąco (bez terminu na końcu), potem priorytet i ręczna
 *   kolejność jak na listach (`compareTasks`).
 * - Zachłannie: każde zadanie trafia na pierwszy dzień z pojemnością > 0, na
 *   którym zostało dość miejsca, i nigdy na dzień po swoim terminie.
 */

/** Czas zadania bez szacunku. */
export const SPREAD_DEFAULT_MINUTES = 30;

export interface SpreadDay {
  day: string;
  /** Pojemność dnia (ms); 0 = dzień wolny. */
  capacity: number;
  /** Czas już zaplanowany na ten dzień (ms). */
  load: number;
}

export interface SpreadPlacement {
  task: Task;
  day: string;
  /** Czas zadania w planie (ms). */
  duration: number;
}

export interface SpreadMiss {
  task: Task;
  duration: number;
  reason: string;
}

export interface SpreadDayResult extends SpreadDay {
  /** Zadania dołożone przez „Rozłóż”, w kolejności układania. */
  tasks: SpreadPlacement[];
  /** Obciążenie po rozłożeniu (ms). */
  after: number;
}

export interface SpreadResult {
  days: SpreadDayResult[];
  placed: SpreadPlacement[];
  missed: SpreadMiss[];
  /** Pominięte: zablokowane („Czekam na”) i cykliczne. */
  skipped: Task[];
}

/** Zadanie, którego „Rozłóż” nie rusza: zablokowane albo cykliczne. */
export const spreadSkips = (task: Task) =>
  !!task.details?.waiting?.trim() || isRecurring(task.details);

/** Kolejność układania: termin, priorytet, ręczna kolejność. */
export function spreadOrder(a: Task, b: Task): number {
  const due = (t: Task) => t.details?.dueDay || "￿";
  return due(a).localeCompare(due(b)) || compareTasks()(a, b);
}

const dayLabel = (day: string) => `${day.slice(8, 10)}.${day.slice(5, 7)}`;

/**
 * Układa `tasks` (kandydatów; zakończone i zarchiwizowane też są pomijane) na
 * `days` (kolejne dni od dziś do końca widocznego tygodnia). `duration` zwraca
 * czas zadania w ms; 0 oznacza brak szacunku (wtedy 30 min). `today` (klucz
 * dnia) rozstrzyga, czy termin już minął; bez niego liczy się pierwszy dzień
 * z `days`.
 */
export function spread(
  tasks: readonly Task[],
  days: readonly SpreadDay[],
  duration: (task: Task) => number,
  today?: string,
): SpreadResult {
  const result: SpreadDayResult[] = days.map((d) => ({
    ...d,
    tasks: [],
    after: d.load,
  }));
  const placed: SpreadPlacement[] = [];
  const missed: SpreadMiss[] = [];
  const skipped: Task[] = [];
  const open = tasks.filter((t) => !t.done && !t.archived);
  for (const task of open) if (spreadSkips(task)) skipped.push(task);
  const queue = open.filter((t) => !spreadSkips(t)).sort(spreadOrder);
  for (const task of queue) {
    const own = duration(task);
    const length = own > 0 ? own : SPREAD_DEFAULT_MINUTES * 60_000;
    const due = task.details?.dueDay ?? "";
    const eligible = result.filter(
      (d) => d.capacity > 0 && (!due || d.day <= due),
    );
    const day = eligible.find((d) => d.capacity - d.after >= length);
    if (day) {
      const placement = { task, day: day.day, duration: length };
      day.tasks.push(placement);
      day.after += length;
      placed.push(placement);
      continue;
    }
    let reason: string;
    if (!eligible.length) {
      const first = today ?? result[0]?.day;
      if (due && (!first || due < first))
        reason = `Termin ${dayLabel(due)} już minął`;
      else if (due) reason = `Brak dnia roboczego do terminu ${dayLabel(due)}`;
      else reason = "Brak dni roboczych do końca tygodnia";
    } else {
      const largest = Math.max(...eligible.map((d) => d.capacity));
      if (length > largest)
        reason = `Dłuższe niż pojemność dnia (${hm(largest)})`;
      else if (due) reason = `Brak miejsca do terminu ${dayLabel(due)}`;
      else reason = "Brak miejsca do końca tygodnia";
    }
    missed.push({ task, duration: length, reason });
  }
  return { days: result, placed, missed, skipped };
}
