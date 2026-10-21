import type { Session } from "./store";
import { dayBounds, dayKey } from "./time";

const HOUR = 3_600_000;
const floorMinute = (time: number) => time - (time % 60_000);

/**
 * Domyślne godziny formularza „Dodaj czas ręcznie” (B11): początek to koniec
 * ostatniego zakończonego przedziału dnia, a bez przedziałów początek dnia z
 * ustawień (`dayStart`, „GG:MM”). Koniec dla dzisiaj to teraz (albo start
 * trwającego pomiaru, żeby się nie nakładać), dla innych dni godzina po początku.
 * Gdy wyliczony koniec nie wypada po początku, koniec = początek + 1 h (w obrębie
 * doby), a dla dzisiaj początek nie leży w przyszłości.
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
  let end = start + HOUR;
  if (today) {
    end = floorMinute(now);
    const running = sessions.find(
      (s) => s.endedAt === null && s.startedAt >= start && s.startedAt < end,
    );
    if (running) end = running.startedAt;
  }
  if (end <= start) end = start + HOUR;
  return [start, Math.min(end, to)];
}
