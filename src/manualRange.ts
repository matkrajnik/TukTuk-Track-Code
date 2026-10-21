import type { Session } from "./store";
import { dayBounds, dayKey } from "./time";

const HOUR = 3_600_000;
const floorMinute = (time: number) => time - (time % 60_000);

/**
 * Ostatni wolny fragment doby przed `upTo` (bez zakończonych przedziałów i
 * trwającego pomiaru), najwyżej godzina; `null`, gdy przed `upTo` nie ma luki.
 */
function latestGap(
  busy: readonly [number, number][],
  from: number,
  upTo: number,
): [number, number] | null {
  let end = upTo;
  // Od końca: przedział zachodzący na `end` przesuwa koniec luki na swój początek.
  for (;;) {
    const cover = busy.find(([a, b]) => a < end && b >= end);
    if (!cover) break;
    end = cover[0];
  }
  if (end <= from) return null;
  const before = busy.filter(([, b]) => b <= end).map(([, b]) => b);
  const start = Math.max(from, end - HOUR, ...before);
  return start < end ? [start, end] : null;
}

/**
 * Domyślne godziny formularza „Dodaj czas ręcznie” (B11): początek to koniec
 * ostatniego zakończonego przedziału dnia, a bez przedziałów początek dnia z
 * ustawień (`dayStart`, „GG:MM”). Koniec dla dzisiaj to teraz (albo start
 * trwającego pomiaru, żeby się nie nakładać), dla innych dni godzina po początku.
 * Gdy wyliczony koniec nie wypada po początku, koniec = początek + 1 h (w obrębie
 * doby), a dla dzisiaj początek nie leży w przyszłości. Gdy tak wyliczony
 * przedział nachodzi na zapisany albo trwający pomiar (np. pomiar trwa od
 * początku dnia albo od końca ostatniego przedziału), formularz proponuje
 * ostatnią wolną godzinę przed nim.
 */
export function manualRange(
  day: string,
  sessions: readonly Session[],
  now: number,
  dayStart: string,
): [start: number, end: number] {
  const [from, to] = dayBounds(day);
  const [y, m, d] = day.split("-").map(Number);
  const [hour, minute] = dayStart.split(":").map(Number);
  const workStart = new Date(
    y,
    m - 1,
    d,
    Number.isFinite(hour) ? hour : 8,
    Number.isFinite(minute) ? minute : 0,
  ).getTime();
  const today = dayKey(now) === day;
  const limit = today ? now : to;
  const ends = sessions
    .filter((s) => s.endedAt !== null && s.endedAt > from && s.startedAt < to)
    .map((s) => Math.min(s.endedAt!, to))
    .filter((end) => end <= limit);
  let start = ends.length ? Math.max(...ends) : workStart;
  if (today && start >= now) start = Math.max(from, floorMinute(now) - HOUR);
  // Przedział kończący się o północy: ostatnia godzina doby zamiast pustego.
  if (start >= to - 60_000) start = to - HOUR;
  const running = today
    ? sessions.find((s) => s.endedAt === null && s.startedAt < now)
    : undefined;
  let end = start + HOUR;
  if (today) {
    end = floorMinute(now);
    if (running && running.startedAt > start)
      end = Math.min(end, running.startedAt);
  }
  if (end <= start) end = start + HOUR;
  end = Math.min(end, to);
  // Zajęte fragmenty doby: zapisane przedziały i trwający pomiar (do teraz).
  const busy = sessions
    .map((s): [number, number] => [
      s.startedAt,
      s.endedAt ?? Math.max(now, s.startedAt),
    ])
    .filter(([a, b]) => b > from && a < to)
    .map(([a, b]): [number, number] => [Math.max(a, from), Math.min(b, to)]);
  if (busy.some(([a, b]) => a < end && b > start)) {
    const upTo = running ? Math.min(running.startedAt, end) : end;
    const gap = latestGap(busy, from, Math.max(upTo, from));
    if (gap) return gap;
  }
  return [start, end];
}
