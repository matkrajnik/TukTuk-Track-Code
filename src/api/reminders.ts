/** Przypomnienia (C3): typy, reguły i etykiety wspólne z `reminders.rs`. */
import { dayBounds, dayKey, dayText } from "../time";
import { holidayName } from "../holidays";
import { isoWeekday } from "../workday";

export type ReminderRepeat = "none" | "daily" | "workdays" | "weekly";

/** Przypomnienie jak z bazy (`reminder_list`). */
export interface Reminder {
  id: number;
  text: string;
  taskId: number | null;
  /** Tytuł przypiętego zadania (tylko do wyświetlenia). */
  taskTitle: string | null;
  /** Lokalna data i godzina następnego wystąpienia, `RRRR-MM-DDTGG:MM`. */
  at: string;
  repeat: ReminderRepeat;
  /** Dni tygodnia dla `weekly` (1 = poniedziałek … 7 = niedziela). */
  weekdays: number[];
  enabled: boolean;
  /** Odłożone wystąpienie powtarzalnego przypomnienia. */
  snoozeUntil: string | null;
  lastFiredAt: number | null;
  createdAt: number;
  updatedAt: number;
}
/** Dane formularza (`reminder_add` / `reminder_update`). */
export interface ReminderInput {
  text: string;
  at: string;
  repeat: ReminderRepeat;
  weekdays: number[];
  taskId: number | null;
  enabled: boolean;
}
/** Nadpisanie przypomnienia dla jednego spotkania (`meeting_reminder_list`). */
export interface MeetingOverride {
  meetingId: string;
  /** `null` = bez przypomnienia. */
  minutes: number | null;
}
/** Wartość pola „Przypomnienie” spotkania: minuty, `null` (brak) albo `"default"`. */
export type MeetingReminderValue = number | null | "default";

export const remindersEvent = "reminders-changed";
/** Najdłuższy tekst przypomnienia (`reminders::MAX_TEXT`). */
export const REMINDER_TEXT_MAX = 200;
/** „X minut przed” spotkaniem (`reminders::MEETING_MINUTES`). */
export const meetingReminderMinutes = [1, 5, 10, 15, 30] as const;
/** Odłożenie z przycisku toastu. */
export const SNOOZE_MINUTES = 10;

export const reminderRepeats: { value: ReminderRepeat; label: string }[] = [
  { value: "none", label: "Jednorazowo" },
  { value: "daily", label: "Codziennie" },
  { value: "workdays", label: "W dni robocze" },
  { value: "weekly", label: "Co tydzień" },
];
export const repeatLabel = (repeat: ReminderRepeat) =>
  reminderRepeats.find((r) => r.value === repeat)?.label ?? repeat;
/** Dni tygodnia formularza (ISO: 1 = poniedziałek). */
export const reminderWeekdays = [
  { value: 1, short: "Pn", name: "poniedziałek" },
  { value: 2, short: "Wt", name: "wtorek" },
  { value: 3, short: "Śr", name: "środa" },
  { value: 4, short: "Cz", name: "czwartek" },
  { value: 5, short: "Pt", name: "piątek" },
  { value: 6, short: "So", name: "sobota" },
  { value: 7, short: "Nd", name: "niedziela" },
] as const;

const pad2 = (n: number) => String(n).padStart(2, "0");
const AT = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})$/;
/** Sprawdza `RRRR-MM-DDTGG:MM` (istniejący dzień i godzina); `null`, gdy zły. */
export function parseAt(at: string): { day: string; time: string } | null {
  const m = AT.exec(at.trim());
  if (!m) return null;
  const [y, mo, d, h, mi] = m.slice(1).map(Number);
  const date = new Date(y, mo - 1, d);
  if (
    date.getFullYear() !== y ||
    date.getMonth() !== mo - 1 ||
    date.getDate() !== d ||
    h > 23 ||
    mi > 59
  )
    return null;
  return { day: `${m[1]}-${m[2]}-${m[3]}`, time: `${m[4]}:${m[5]}` };
}
/** Lokalny czas (ms) jako `RRRR-MM-DDTGG:MM`. */
export function atFromMs(ms: number): string {
  const d = new Date(ms);
  return `${dayKey(ms)}T${pad2(d.getHours())}:${pad2(d.getMinutes())}`;
}
/** Domyślny czas nowego przypomnienia: za godzinę, zaokrąglone w górę do 5 minut. */
export function defaultReminderAt(now: number): string {
  const d = new Date(now + 3_600_000);
  d.setSeconds(0, 0);
  const up = Math.ceil(d.getMinutes() / 5) * 5;
  d.setMinutes(up);
  return atFromMs(d.getTime());
}

/** Czy dzień (`RRRR-MM-DD`) pasuje do reguły powtarzania (`reminders::day_matches`). */
function dayMatches(repeat: ReminderRepeat, weekdays: number[], day: string) {
  switch (repeat) {
    case "daily":
      return true;
    case "workdays":
      return isoWeekday(day) <= 5 && !holidayName(day);
    case "weekly":
      return weekdays.includes(isoWeekday(day));
    default:
      return false;
  }
}
const nextDay = (day: string) => dayKey(dayBounds(day)[1] + 3_600_000);
/**
 * Pierwsze wystąpienie powtarzalnego przypomnienia ściśle po `after`, o
 * godzinie z `at` (`reminders::next_occurrence`); `null` dla jednorazowego.
 */
export function nextOccurrence(
  repeat: ReminderRepeat,
  weekdays: number[],
  at: string,
  after: string,
): string | null {
  if (repeat === "none") return null;
  const start = parseAt(at);
  const from = parseAt(after);
  if (!start || !from) return null;
  let day = start.day > from.day ? start.day : from.day;
  for (let i = 0; i < 400; i++) {
    if (dayMatches(repeat, weekdays, day)) {
      const candidate = `${day}T${start.time}`;
      if (candidate > after) return candidate;
    }
    day = nextDay(day);
  }
  return null;
}
/** Pierwsze wystąpienie nie wcześniejsze niż `from` (`reminders::first_occurrence`). */
export function firstOccurrence(
  repeat: ReminderRepeat,
  weekdays: number[],
  at: string,
  from: string,
): string | null {
  const f = parseAt(from);
  if (!f) return null;
  const before = atFromMs(+new Date(`${f.day}T${f.time}`) - 60_000);
  return nextOccurrence(repeat, weekdays, at, before);
}

/** Kiedy przypomnienie wystrzeli: odłożenie, jeśli jest, inaczej `at`. */
export const effectiveAt = (r: Reminder) => r.snoozeUntil ?? r.at;

/** „10:00” dziś, „jutro 10:00”, inaczej „pt., 16 paź 10:00”. */
export function whenLabel(at: string, todayKey: string): string {
  const p = parseAt(at);
  if (!p) return at;
  if (p.day === todayKey) return p.time;
  if (p.day === nextDay(todayKey)) return `jutro ${p.time}`;
  return `${dayText(p.day)} ${p.time}`;
}

/** Opis powtarzania na liście: „Codziennie”, „Co tydzień: Pn, Pt”. */
export function repeatText(r: Pick<Reminder, "repeat" | "weekdays">): string {
  if (r.repeat !== "weekly") return repeatLabel(r.repeat);
  const days = reminderWeekdays
    .filter((d) => r.weekdays.includes(d.value))
    .map((d) => d.short)
    .join(", ");
  return `Co tydzień: ${days}`;
}

/** Etykieta opcji pola „Przypomnienie” spotkania. */
export const meetingReminderOptionLabel = (minutes: number) =>
  `${minutes} min przed`;
