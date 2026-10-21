/**
 * Podgląd komend `day_start`, `day_end`, `day_resume` (`days.rs`, B13): wiersze
 * `day_marks` w danych zadań podglądu (`dayMarks`). Te same reguły: tylko
 * dzisiejszy dzień, trwający pomiar zatrzymuje wyłącznie jawne potwierdzenie.
 */
import {
  DAY_MEASURING,
  type DayMark,
  type Snapshot,
} from "../api/task";
import { dayBounds, dayKey } from "../time";
import { read, saveSnapshot } from "./snapshot";

function checkToday(day: string) {
  if (
    !/^\d{4}-\d{2}-\d{2}$/.test(day) ||
    dayKey(+new Date(day + "T12:00:00")) !== day
  )
    throw new Error("Niepoprawny dzień.");
  const today = dayKey(Date.now());
  if (day > today)
    throw new Error("Nie można zmienić stanu dnia w przyszłości.");
  if (day < today)
    throw new Error(
      "Rozpocząć, zakończyć i wznowić można tylko dzisiejszy dzień.",
    );
}

const state = (m?: DayMark) =>
  m?.endedAt != null
    ? "ended"
    : m && (m.startedAt != null || m.resumed > 0)
      ? "started"
      : "none";

function mark(data: Snapshot, day: string): DayMark {
  const marks = (data.dayMarks ??= []);
  let m = marks.find((x) => x.day === day);
  if (!m) {
    m = { day, startedAt: null, endedAt: null, resumed: 0 };
    marks.push(m);
    marks.sort((a, b) => a.day.localeCompare(b.day));
  }
  return m;
}

/** Odpowiednik `days::start`: `first` = dzień właśnie się zaczął. */
export async function dayStart(
  day: string,
): Promise<{ data: Snapshot; first: boolean }> {
  checkToday(day);
  const data = await read();
  const current = data.dayMarks?.find((m) => m.day === day);
  const s = state(current);
  if (s === "ended")
    throw new Error("Dzień jest już zakończony. Użyj „Wznów dzień”.");
  if (s === "started") return { data, first: false };
  mark(data, day).startedAt = Date.now();
  saveSnapshot(data);
  return { data, first: true };
}

/** Odpowiednik `days::end`. */
export async function dayEnd(
  day: string,
  stopMeasurement: boolean,
  reviewAt: number | null,
): Promise<Snapshot> {
  checkToday(day);
  const data = await read();
  if (state(data.dayMarks?.find((m) => m.day === day)) === "ended")
    throw new Error("Dzień jest już zakończony.");
  const active = data.sessions.find((s) => s.endedAt === null);
  if (active && !stopMeasurement) throw new Error(DAY_MEASURING);
  const now = Date.now();
  if (active) active.endedAt = Math.max(active.startedAt, now);
  const m = mark(data, day);
  if (m.startedAt == null) {
    const [from, to] = dayBounds(day);
    const trash = new Set((data.trash ?? []).map((t) => t.id));
    const starts = data.sessions
      .filter(
        (s) =>
          !trash.has(s.taskId) &&
          s.startedAt < to &&
          (s.endedAt === null || s.endedAt > from),
      )
      .map((s) => Math.max(s.startedAt, from));
    m.startedAt = reviewAt ?? (starts.length ? Math.min(...starts) : null);
  }
  m.endedAt = now;
  saveSnapshot(data);
  return data;
}

/** Odpowiednik `days::resume`. */
export async function dayResume(day: string): Promise<Snapshot> {
  checkToday(day);
  const data = await read();
  const m = data.dayMarks?.find((x) => x.day === day);
  if (state(m) !== "ended" || !m) throw new Error("Dzień nie jest zakończony.");
  m.endedAt = null;
  m.resumed += 1;
  saveSnapshot(data);
  return data;
}
