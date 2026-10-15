import type { Session } from "./store";

export function dayKey(time: number): string {
  const d = new Date(time);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
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
  const d = new Date(time);
  return `${dayKey(time)}T${[d.getHours(), d.getMinutes(), d.getSeconds()].map((v) => String(v).padStart(2, "0")).join(":")}`;
}

const timeLabel = (time: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(time);
const dayLabel = (time: number) =>
  new Intl.DateTimeFormat("pl-PL", {
    day: "numeric",
    month: "short",
    year: "numeric",
  }).format(time);
/** Data i godzina przedziału, np. „14 paź 2026, 09:00:00”. */
export const stamp = (time: number) => `${dayLabel(time)}, ${timeLabel(time)}`;

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
