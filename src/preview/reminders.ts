/**
 * Symulowane przypomnienia podglądu (`reminders.rs`, C3): lista przypomnień
 * i nadpisania przypomnień spotkań w localStorage, z tą samą walidacją co w
 * Rust. Podgląd nie wysyła powiadomień, więc przypomnienia tylko się zapisują.
 *
 * Klucz `tuktuk-track-preview-reminders-v1`: `{ reminders?, nextId?, meetings? }`.
 * - `reminders`: przypomnienia (`Reminder` bez `taskTitle`, tytuł liczy lista),
 * - `nextId`: następny identyfikator,
 * - `meetings`: `{ [id spotkania]: minuty | null }` (brak klucza = domyślne).
 */
import {
  REMINDER_TEXT_MAX,
  atFromMs,
  firstOccurrence,
  meetingReminderMinutes,
  parseAt,
  reminderRepeats,
  remindersEvent,
  type MeetingOverride,
  type MeetingReminderValue,
  type Reminder,
  type ReminderInput,
} from "../api/reminders";
import { previewJson } from "./storage";
import { read as readSnapshot } from "./snapshot";

export const remindersKey = "tuktuk-track-preview-reminders-v1";

type Stored = Omit<Reminder, "taskTitle">;
interface State {
  reminders: Stored[];
  nextId: number;
  meetings: Record<string, number | null>;
}

function state(): State {
  const raw = previewJson(remindersKey);
  return {
    reminders: Array.isArray(raw.reminders) ? (raw.reminders as Stored[]) : [],
    nextId: typeof raw.nextId === "number" ? raw.nextId : 1,
    meetings:
      raw.meetings && typeof raw.meetings === "object"
        ? (raw.meetings as Record<string, number | null>)
        : {},
  };
}
function save(next: State) {
  try {
    localStorage.setItem(remindersKey, JSON.stringify(next));
  } catch {
    // Bez localStorage zmiana nie przetrwa przeładowania.
  }
  window.dispatchEvent(new Event(remindersEvent));
}

/** Przypomnienia z tytułem zadania; zadanie usunięte na stałe = bez zadania. */
async function withTitles(list: Stored[]): Promise<Reminder[]> {
  const data = await readSnapshot();
  const titles = new Map(
    [...data.tasks, ...(data.trash ?? [])].map((t) => [t.id, t.title]),
  );
  return list.map((r) => ({
    ...r,
    taskId: r.taskId !== null && titles.has(r.taskId) ? r.taskId : null,
    taskTitle: r.taskId !== null ? (titles.get(r.taskId) ?? null) : null,
  }));
}

/** Kolejność jak w Rust: włączone według terminu, potem wyłączone od najnowszych. */
function ordered(list: Reminder[]): Reminder[] {
  const when = (r: Reminder) => r.snoozeUntil ?? r.at;
  return [...list].sort((a, b) => {
    if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
    if (a.enabled) {
      const byWhen = when(a).localeCompare(when(b));
      if (byWhen) return byWhen;
    }
    return b.at.localeCompare(a.at) || a.id - b.id;
  });
}

export async function reminderList(): Promise<Reminder[]> {
  return ordered(await withTitles(state().reminders));
}

/** Walidacja jak `reminders::validate`. */
async function valid(
  input: ReminderInput,
): Promise<Omit<Stored, "id" | "snoozeUntil" | "lastFiredAt" | "createdAt" | "updatedAt">> {
  const text = input.text.trim();
  if (!text) throw new Error("Podaj treść przypomnienia.");
  if ([...text].length > REMINDER_TEXT_MAX)
    throw new Error(
      `Treść przypomnienia może mieć najwyżej ${REMINDER_TEXT_MAX} znaków.`,
    );
  const at = input.at.trim();
  if (!parseAt(at))
    throw new Error("Podaj poprawny dzień i godzinę przypomnienia.");
  if (!reminderRepeats.some((r) => r.value === input.repeat))
    throw new Error("Niepoprawny sposób powtarzania przypomnienia.");
  let weekdays = [...input.weekdays];
  if (input.repeat === "weekly") {
    if (!weekdays.length)
      throw new Error("Zaznacz co najmniej jeden dzień tygodnia.");
    if (weekdays.some((d) => !Number.isInteger(d) || d < 1 || d > 7))
      throw new Error("Dni tygodnia muszą być liczbami od 1 do 7.");
    weekdays = [...new Set(weekdays)].sort((a, b) => a - b);
  } else weekdays = [];
  if (input.taskId !== null) {
    const data = await readSnapshot();
    if (!data.tasks.some((t) => t.id === input.taskId))
      throw new Error("Wybrane zadanie nie istnieje.");
  }
  const current = atFromMs(Date.now());
  let when = at;
  if (input.repeat === "none") {
    if (input.enabled && at < current)
      throw new Error("Przypomnienie jednorazowe musi być w przyszłości.");
  } else {
    const first = firstOccurrence(
      input.repeat,
      weekdays,
      at,
      at > current ? at : current,
    );
    if (!first)
      throw new Error("Nie udało się wyznaczyć wystąpienia przypomnienia.");
    when = first;
  }
  return {
    text,
    taskId: input.taskId,
    at: when,
    repeat: input.repeat,
    weekdays,
    enabled: input.enabled,
  };
}

async function one(id: number): Promise<Reminder> {
  const found = (await withTitles(state().reminders)).find((r) => r.id === id);
  if (!found) throw new Error("Nie ma takiego przypomnienia.");
  return found;
}

export async function reminderAdd(input: ReminderInput): Promise<Reminder> {
  const v = await valid(input);
  const s = state();
  const now = Date.now();
  const id = s.nextId;
  s.reminders.push({
    id,
    ...v,
    snoozeUntil: null,
    lastFiredAt: null,
    createdAt: now,
    updatedAt: now,
  });
  s.nextId = id + 1;
  save(s);
  return one(id);
}

export async function reminderUpdate(
  id: number,
  input: ReminderInput,
): Promise<Reminder> {
  const v = await valid(input);
  const s = state();
  const index = s.reminders.findIndex((r) => r.id === id);
  if (index < 0) throw new Error("Nie ma takiego przypomnienia.");
  s.reminders[index] = {
    ...s.reminders[index],
    ...v,
    snoozeUntil: null,
    updatedAt: Date.now(),
  };
  save(s);
  return one(id);
}

export async function reminderDelete(id: number): Promise<void> {
  const s = state();
  if (!s.reminders.some((r) => r.id === id))
    throw new Error("Nie ma takiego przypomnienia.");
  s.reminders = s.reminders.filter((r) => r.id !== id);
  save(s);
}

/** Jak `reminders::snooze`: jednorazowe dostaje nowy `at`, powtarzalne `snoozeUntil`. */
export async function reminderSnooze(
  id: number,
  minutes: number,
): Promise<Reminder> {
  if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440)
    throw new Error("Odłożenie musi mieć od 1 do 1440 minut.");
  const s = state();
  const index = s.reminders.findIndex((r) => r.id === id);
  if (index < 0) throw new Error("Nie ma takiego przypomnienia.");
  const until = atFromMs(Date.now() + minutes * 60_000);
  const r = s.reminders[index];
  s.reminders[index] =
    r.repeat === "none"
      ? { ...r, at: until, snoozeUntil: null, enabled: true, updatedAt: Date.now() }
      : { ...r, snoozeUntil: until, updatedAt: Date.now() };
  save(s);
  return one(id);
}

export async function meetingReminderList(): Promise<MeetingOverride[]> {
  return Object.entries(state().meetings).map(([meetingId, minutes]) => ({
    meetingId,
    minutes,
  }));
}

/** Jak `reminders::set_meeting_reminder` (bez sprawdzania, czy spotkanie istnieje). */
export async function meetingReminderSet(
  meetingId: string,
  value: MeetingReminderValue,
): Promise<void> {
  const id = meetingId.trim();
  if (!id || [...id].length > 512) throw new Error("Niepoprawne spotkanie.");
  const s = state();
  if (value === "default") delete s.meetings[id];
  else if (value === null) s.meetings[id] = null;
  else if (
    typeof value === "number" &&
    (meetingReminderMinutes as readonly number[]).includes(value)
  )
    s.meetings[id] = value;
  else
    throw new Error(
      "Przypomnienie przed spotkaniem: 1, 5, 10, 15 albo 30 minut.",
    );
  save(s);
}
