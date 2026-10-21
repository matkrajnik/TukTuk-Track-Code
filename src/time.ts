import type { Session } from "./store";

const pad2 = (n: number) => String(n).padStart(2, "0");

export function dayKey(time: number): string {
  const d = new Date(time);
  return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}`;
}

export function dayBounds(day: string): [number, number] {
  const [year, month, date] = day.split("-").map(Number);
  return [
    new Date(year, month - 1, date).getTime(),
    new Date(year, month - 1, date + 1).getTime(),
  ];
}

export function inDay(session: Session, day: string, now: number): number {
  const [start, end] = dayBounds(day);
  return Math.max(
    0,
    Math.min(session.endedAt ?? now, end) - Math.max(session.startedAt, start),
  );
}

export function localInput(time: number): string {
  return `${dayKey(time)}T${clock(time, true)}`;
}

/**
 * Godzina zegarowa w czasie lokalnym: „09:05”, z `seconds` „09:05:03” (to samo
 * co `toLocaleTimeString("pl-PL")` z godziną i minutami dwucyfrowymi).
 */
export function clock(time: number, seconds = false): string {
  const d = new Date(time);
  const text = `${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
  return seconds ? `${text}:${pad2(d.getSeconds())}` : text;
}
/** Czas trwania jako G:MM, np. „8:00”, „12:05”. Minuty w dół, jak w trayu. */
export function hm(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  return `${Math.floor(minutes / 60)}:${pad2(minutes % 60)}`;
}
/** Czas trwania jako GG:MM, np. „03:12”. */
export const hhmm = (ms: number) => hm(ms).padStart(5, "0");
/**
 * Polska odmiana rzeczownika po liczbie (samo słowo): 1 → `one`, 2–4 poza
 * 12–14 (także 22–24 …) → `few`, reszta → `many`; np.
 * `plural(5, "zadanie", "zadania", "zadań")` → „zadań”.
 */
export function plural(
  n: number,
  one: string,
  few: string,
  many: string,
): string {
  const ones = n % 10;
  const tens = n % 100;
  if (n === 1) return one;
  return ones >= 2 && ones <= 4 && (tens < 12 || tens > 14) ? few : many;
}

const dayLabel = (time: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(time);
/** Data i godzina przedziału, np. „14 paź 2026, 09:00:00”. */
export const stamp = (time: number) => `${dayLabel(time)}, ${clock(time, true)}`;

const shortDay = new Intl.DateTimeFormat("pl-PL", {
  weekday: "short",
  day: "numeric",
  month: "short",
});
const shortDayYear = new Intl.DateTimeFormat("pl-PL", {
  weekday: "short",
  day: "numeric",
  month: "short",
  year: "numeric",
});
/**
 * Etykieta dnia w interfejsie, np. „czw., 15 paź”; rok tylko spoza bieżącego.
 * Klucze ISO (RRRR-MM-DD) zostają w danych i w tekście raportu TS.
 */
export function dayText(key: string): string {
  const [start] = dayBounds(key);
  if (!Number.isFinite(start)) return key;
  const sameYear = new Date(start).getFullYear() === new Date().getFullYear();
  return (sameYear ? shortDay : shortDayYear).format(start);
}

const dayMonthFormat = new Intl.DateTimeFormat("pl-PL", {
  day: "numeric",
  month: "short",
});
/** Sam dzień i miesiąc, np. „14 paź” (gdy dzień tygodnia stoi obok). */
export const dayMonth = (key: string) => dayMonthFormat.format(dayBounds(key)[0]);
