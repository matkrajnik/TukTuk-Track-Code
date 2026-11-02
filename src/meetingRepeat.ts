/**
 * Cykle ręcznych spotkań: odpowiednik `meetings::repeat_dates`, `occurrences`
 * i `occurrence_bounds` (`src-tauri/src/meetings.rs`) dla podglądu
 * (`preview/calendar.ts`) i formularza `MeetingEditor`. Dni to klucze
 * RRRR-MM-DD liczone w UTC jak w `recurrence.ts`.
 */
import type { MeetingRepeat } from "./api/calendar";
import { keyOf, utc, weekdayOf } from "./recurrence";
import { dayBounds, dayKey } from "./time";

/** Najwięcej wystąpień jednego cyklu (`meetings::MAX_OCCURRENCES`). */
export const MEETING_REPEAT_MAX = 200;
/** Najdłuższy cykl w dniach (`meetings::MAX_REPEAT_SPAN_DAYS`). */
export const MEETING_REPEAT_MAX_DAYS = 366;
/** Najdłuższy odstęp w tygodniach (`meetings::MAX_REPEAT_INTERVAL`). */
export const MEETING_REPEAT_MAX_INTERVAL = 8;
const DAY_MS = 86_400_000;

/** Dzień przesunięty o `n` dni (klucze w UTC). */
export const shiftDay = (day: string, n: number): string =>
  keyOf(utc(day) + n * DAY_MS);

/** Poprawny klucz dnia RRRR-MM-DD (istniejąca data). */
export const isDayKey = (day: string): boolean =>
  /^\d{4}-\d{2}-\d{2}$/.test(day) && keyOf(utc(day)) === day;

/** Domyślny ostatni dzień cyklu: dzień + 3 miesiące (w krótszym miesiącu jego ostatni dzień). */
export function defaultRepeatUntil(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  const target = m - 1 + 3;
  const year = y + Math.floor(target / 12);
  const month = target % 12;
  const last = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  return keyOf(Date.UTC(year, month, Math.min(d, last)));
}

/**
 * Jak `meetings::occurrences`: dzień początku zawsze, potem dni z `(first, until]`.
 * Woła ją tylko `meetingRepeatDays` po walidacji (poprawne klucze, `until` ≥
 * `first`, najwyżej `MEETING_REPEAT_MAX_DAYS` dni), więc pętla się kończy.
 */
function meetingOccurrences(
  first: string,
  until: string,
  days: readonly number[],
  interval: number,
): string[] {
  const monday = (k: string) => utc(k) - (weekdayOf(k) - 1) * DAY_MS;
  const startWeek = monday(first);
  const every = Math.max(1, interval);
  const result = [first];
  for (let i = 1; ; i++) {
    const day = shiftDay(first, i);
    if (day > until) break;
    const weeks = Math.round((monday(day) - startWeek) / (7 * DAY_MS));
    if (days.includes(weekdayOf(day)) && weeks % every === 0) result.push(day);
  }
  return result;
}

/** Walidacja i dni cyklu jak `meetings::repeat_dates`; rzuca Error z tym samym komunikatem. */
export function meetingRepeatDays(first: string, repeat: MeetingRepeat): string[] {
  if (!isDayKey(first)) throw new Error("Wprowadź poprawny dzień i godziny.");
  const { days, interval, until } = repeat;
  if (days.length === 0)
    throw new Error("Wybierz co najmniej jeden dzień tygodnia.");
  if (
    !days.every((d) => Number.isInteger(d) && d >= 1 && d <= 7) ||
    new Set(days).size !== days.length
  )
    throw new Error("Niepoprawne dni tygodnia cyklu.");
  if (
    !Number.isInteger(interval) ||
    interval < 1 ||
    interval > MEETING_REPEAT_MAX_INTERVAL
  )
    throw new Error(
      `Odstęp cyklu musi wynosić od 1 do ${MEETING_REPEAT_MAX_INTERVAL} tygodni.`,
    );
  if (!isDayKey(until)) throw new Error("Podaj poprawny ostatni dzień cyklu.");
  if (until < first)
    throw new Error("Ostatni dzień cyklu nie może być przed dniem spotkania.");
  if ((utc(until) - utc(first)) / DAY_MS > MEETING_REPEAT_MAX_DAYS)
    throw new Error(
      `Cykl może trwać najwyżej ${MEETING_REPEAT_MAX_DAYS} dni.`,
    );
  const dates = meetingOccurrences(first, until, days, interval);
  if (dates.length > MEETING_REPEAT_MAX)
    throw new Error(
      `Cykl może mieć najwyżej ${MEETING_REPEAT_MAX} spotkań. Skróć okres albo wybierz mniej dni.`,
    );
  return dates;
}

/**
 * Granice wystąpienia w dniu `day` jak `meetings::occurrence_bounds`: ta sama
 * lokalna godzina od–do co pierwsze spotkanie (koniec o północy kończy dzień),
 * całodniowe = cały lokalny dzień. `new Date` w luce zmiany czasu przesuwa
 * godzinę do przodu, a przy podwójnej godzinie bierze wcześniejszą.
 */
export function occurrenceBounds(
  first: { start: number; end: number; allDay: boolean },
  day: string,
): [number, number] {
  if (first.allDay) return dayBounds(day);
  const s = new Date(first.start);
  const e = new Date(first.end);
  const endDays = Math.round(
    (utc(dayKey(first.end)) - utc(dayKey(first.start))) / DAY_MS,
  );
  const at = (key: string, t: Date) => {
    const [y, m, d] = key.split("-").map(Number);
    return new Date(
      y,
      m - 1,
      d,
      t.getHours(),
      t.getMinutes(),
      t.getSeconds(),
      t.getMilliseconds(),
    ).getTime();
  };
  const start = at(day, s);
  const end = at(shiftDay(day, endDays), e);
  return [start, end > start ? end : start + (first.end - first.start)];
}
