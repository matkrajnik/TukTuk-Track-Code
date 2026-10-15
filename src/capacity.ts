import { meetingOccupies, type CalendarEvent, type Session } from "./store";
import { dayBounds } from "./time";

/**
 * Pojemność dnia ze spotkaniami. Odpowiednik w Rust: `src-tauri/src/capacity.rs`.
 *
 * - Pojemność = cel dnia minus minuty spotkań zajmujących czas (`meetingOccupies`),
 *   policzone bez nakładania i przycięte do lokalnej doby, nie mniej niż 0.
 * - Całodniowe `oof` oznacza dzień wolny: pojemność 0.
 * - Cel mierzonego czasu (`dayGoal`: tray, powiadomienie, pasek celu dnia) to cel
 *   dnia minus spotkania, których nie pokrywa zapisany pomiar, więc spotkanie
 *   dodane jako czas nie zmniejsza celu drugi raz.
 */
export type Span = [start: number, end: number];

/** Posortowane, rozłączne przedziały `[start, end)`; puste są pomijane. */
export function mergeSpans(spans: readonly Span[]): Span[] {
  const sorted = spans
    .filter(([a, b]) => b > a)
    .map(([a, b]): Span => [a, b])
    .sort((x, y) => x[0] - y[0] || x[1] - y[1]);
  const out: Span[] = [];
  for (const [a, b] of sorted) {
    const last = out[out.length - 1];
    if (last && a <= last[1]) last[1] = Math.max(last[1], b);
    else out.push([a, b]);
  }
  return out;
}

const length = (spans: readonly Span[]) =>
  spans.reduce((sum, [a, b]) => sum + b - a, 0);

/** Część wspólna dwóch list rozłącznych, posortowanych przedziałów (ms). */
function intersection(a: readonly Span[], b: readonly Span[]): number {
  let i = 0,
    j = 0,
    sum = 0;
  while (i < a.length && j < b.length) {
    const from = Math.max(a[i][0], b[j][0]);
    const to = Math.min(a[i][1], b[j][1]);
    if (to > from) sum += to - from;
    if (a[i][1] < b[j][1]) i++;
    else j++;
  }
  return sum;
}

/** Spotkanie nakłada się na dzień `day`. */
export const meetingInDay = (e: CalendarEvent, day: string) => {
  const [from, to] = dayBounds(day);
  return e.start < to && e.end > from;
};

/** Całodniowe „poza biurem”, które zwalnia dzień (pierwsze pasujące). */
export const offDayEvent = (events: readonly CalendarEvent[], day: string) =>
  events.find(
    (e) =>
      e.allDay && e.showAs === "oof" && meetingOccupies(e) && meetingInDay(e, day),
  ) ?? null;

/**
 * Zajęte przedziały spotkań dnia: przycięte do doby i złączone. Wydarzenia
 * całodniowe nie są spotkaniami (np. „Praca zdalna”, konferencja); całodniowe
 * „poza biurem” obsługuje `offDayEvent`.
 */
export function meetingSpans(
  events: readonly CalendarEvent[],
  day: string,
): Span[] {
  const [from, to] = dayBounds(day);
  return mergeSpans(
    events
      .filter((e) => !e.allDay && meetingOccupies(e))
      .map((e): Span => [Math.max(from, e.start), Math.min(to, e.end)]),
  );
}

export interface DayCapacity {
  /** Cel dnia (0 dla dnia wolnego i święta). */
  target: number;
  /** Czas spotkań bez nakładania. */
  meetings: number;
  /** Cel minus spotkania, nie mniej niż 0; 0 przy urlopie całodniowym. */
  capacity: number;
  /** Całodniowe „poza biurem”. */
  off: CalendarEvent | null;
}

/** Pojemność dnia z celu (`dayTarget`) i spotkań. */
export function dayCapacity(
  target: number,
  events: readonly CalendarEvent[],
  day: string,
): DayCapacity {
  const off = offDayEvent(events, day);
  const meetings = length(meetingSpans(events, day));
  return {
    target,
    meetings,
    capacity: off ? 0 : Math.max(0, target - meetings),
    off,
  };
}

/** Cel mierzonego czasu: cel dnia minus spotkania niepokryte pomiarem. */
export function dayGoal(
  target: number,
  events: readonly CalendarEvent[],
  sessions: readonly Session[],
  day: string,
  now: number,
): number {
  if (target <= 0 || offDayEvent(events, day)) return 0;
  const [from, to] = dayBounds(day);
  const meetings = meetingSpans(events, day);
  const worked = mergeSpans(
    sessions.map((s): Span => [
      Math.max(from, s.startedAt),
      Math.min(to, s.endedAt ?? now),
    ]),
  );
  return Math.max(
    0,
    target - (length(meetings) - intersection(meetings, worked)),
  );
}
