/**
 * Symulowany kalendarz Microsoft podglądu (`calendar.rs`) i ręczne spotkania
 * (`meetings.rs`, B12) w osobnym kluczu, więc „Rozłącz” ich nie kasuje.
 */
import {
  MEETING_SUBJECT_MAX,
  calendarClientIdMessage,
  calendarEvent,
  guid,
  meetingTypes,
  validTenant,
  type CalendarEvent,
  type CalendarStatus,
  type MeetingInput,
} from "../api/calendar";
import { meetingRepeatDays, occurrenceBounds } from "../meetingRepeat";
import { dayBounds, dayKey } from "../time";
import { forgetMeetingReminders } from "./reminders";
import { read as readSnapshot } from "./snapshot";
/** Dane podglądu: ustawienia, pobrane spotkania i spotkania „w chmurze” (`sample`). */
interface PreviewCalendar {
  clientId: string;
  tenant: string;
  email: string;
  lastSync: number | null;
  lastError: string;
  events: CalendarEvent[];
  sample: CalendarEvent[];
}
export const calendarKey = "tuktuk-track-preview-calendar-v1";
/** Konto „zalogowane” w podglądzie. */
export const previewCalendarEmail = "jan.kowalski@firma.example";
function previewCalendar(): PreviewCalendar {
  const empty: PreviewCalendar = {
    clientId: "",
    tenant: "organizations",
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
    sample: [],
  };
  try {
    const value = JSON.parse(localStorage.getItem(calendarKey) ?? "{}");
    return value && typeof value === "object" ? { ...empty, ...value } : empty;
  } catch {
    return empty;
  }
}
function savePreviewCalendar(data: PreviewCalendar) {
  localStorage.setItem(calendarKey, JSON.stringify(data));
  window.dispatchEvent(new Event(calendarEvent));
}
const previewStatus = (c: PreviewCalendar): CalendarStatus => ({
  clientId: c.clientId,
  tenant: c.tenant || "organizations",
  email: c.email,
  connected: !!c.email && !!c.clientId,
  connecting: false,
  lastSync: c.lastSync,
  lastError: c.lastError,
});
export async function calendarStatus(): Promise<CalendarStatus> {
  return previewStatus(previewCalendar());
}
export async function calendarConnect(
  clientId: string,
  tenant: string,
): Promise<CalendarStatus> {
  const id = clientId.trim().toLowerCase();
  const t = tenant.trim().toLowerCase() || "organizations";
  if (!guid.test(id)) throw new Error(calendarClientIdMessage);
  if (!validTenant(t))
    throw new Error(
      "Podaj dzierżawcę: organizations, identyfikator albo domenę firmy.",
    );
  const c = previewCalendar();
  const next: PreviewCalendar = {
    ...c,
    clientId: id,
    tenant: t,
    email: previewCalendarEmail,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
export async function calendarDisconnect(): Promise<CalendarStatus> {
  const next = {
    ...previewCalendar(),
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
export async function calendarRefresh(): Promise<CalendarStatus> {
  const c = previewCalendar();
  if (!c.clientId)
    throw new Error(
      "Kalendarz jest wyłączony: podaj client ID w Ustawieniach → Kalendarz.",
    );
  if (!c.email)
    throw new Error("Połącz konto Microsoft w Ustawieniach → Kalendarz.");
  const next = {
    ...c,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/**
 * Jak `calendar_events` (`calendar::meetings_between`): spotkania z Outlooka
 * tylko przy połączonym koncie, ręczne zawsze, według początku.
 */
export async function calendarEvents(
  from: number,
  to: number,
): Promise<CalendarEvent[]> {
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const c = previewCalendar();
  const outlook = c.email
    ? c.events.map((e) => ({ ...e, source: "outlook" as const, taskId: null }))
    : [];
  const manual = await previewManual();
  return [...outlook, ...manual]
    .filter(
      (e) => e.start < to && (e.end > from || (e.end === e.start && e.start >= from)),
    )
    .sort(
      (a, b) =>
        a.start - b.start ||
        a.end - b.end ||
        (a.id < b.id ? -1 : a.id > b.id ? 1 : 0),
    );
}

// ---------- Ręczne spotkania (`meetings.rs`) ----------

export const meetingsKey = "tuktuk-track-preview-meetings-v1";
interface StoredMeeting {
  id: string;
  start: number;
  end: number;
  subject: string;
  showAs: string;
  allDay: boolean;
  taskId: number | null;
  /** Tylko wiersze cyklu: id pierwszego wystąpienia (`series_id`). */
  seriesId?: string;
}
function storedMeetings(): StoredMeeting[] {
  try {
    const value = JSON.parse(localStorage.getItem(meetingsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
function saveMeetings(list: StoredMeeting[]) {
  localStorage.setItem(meetingsKey, JSON.stringify(list));
  window.dispatchEvent(new Event(calendarEvent));
}
/** Ręczne spotkania jak z bazy; zadanie usunięte na stałe = bez zadania (`ON DELETE SET NULL`). */
async function previewManual(): Promise<CalendarEvent[]> {
  const list = storedMeetings();
  if (!list.length) return [];
  const data = await readSnapshot();
  const ids = new Set(
    [...data.tasks, ...(data.trash ?? [])].map((t) => t.id),
  );
  return list.map((m) => ({
    ...m,
    cancelled: false,
    response: "organizer",
    source: "manual" as const,
    taskId: m.taskId !== null && ids.has(m.taskId) ? m.taskId : null,
  }));
}
const toEvent = async (id: string) =>
  (await previewManual()).find((e) => e.id === id)!;
/** Walidacja jak `meetings::validate`. */
async function validMeeting(input: MeetingInput): Promise<Omit<StoredMeeting, "id">> {
  const subject = input.subject.trim();
  if (!subject) throw new Error("Podaj temat spotkania.");
  if ([...subject].length > MEETING_SUBJECT_MAX)
    throw new Error(
      `Temat spotkania może mieć najwyżej ${MEETING_SUBJECT_MAX} znaków.`,
    );
  if (!meetingTypes.some((t) => t.value === input.showAs))
    throw new Error("Niepoprawny typ spotkania.");
  if (!Number.isSafeInteger(input.start) || input.start < 0)
    throw new Error("Niepoprawny początek spotkania.");
  const day = dayKey(input.start);
  let [start, end] = [input.start, input.end];
  if (input.allDay) [start, end] = dayBounds(day);
  else {
    if (!Number.isSafeInteger(end) || end <= start)
      throw new Error("Koniec spotkania musi być po początku.");
    if (dayKey(end - 1) !== day)
      throw new Error(
        "Początek i koniec spotkania muszą być tego samego dnia. Dla dłuższego zaznacz „Cały dzień”.",
      );
  }
  if (input.taskId !== null) {
    const data = await readSnapshot();
    if (!data.tasks.some((t) => t.id === input.taskId))
      throw new Error("Wybrane zadanie nie istnieje.");
  }
  return {
    start,
    end,
    subject,
    showAs: input.showAs,
    allDay: input.allDay,
    taskId: input.taskId,
  };
}
function readOnly(id: string) {
  if (!id.startsWith("manual:"))
    throw new Error("Spotkania z Outlooka są tylko do odczytu.");
}
export async function meetingAdd(input: MeetingInput): Promise<CalendarEvent> {
  const valid = await validMeeting(input);
  // Walidacja spotkania idzie przed walidacją cyklu (jak w `meetings::add`).
  const first = dayKey(valid.start);
  const days = input.repeat ? meetingRepeatDays(first, input.repeat) : [first];
  const list = storedMeetings();
  const now = Date.now();
  const taken = new Set(list.map((m) => m.id));
  let n = 1;
  // Jak `meetings::fresh_id`: `manual:<now>`, potem `-2`, `-3`…
  const nextId = () => {
    for (;;) {
      const id = n === 1 ? `manual:${now}` : `manual:${now}-${n}`;
      if (!taken.has(id)) {
        taken.add(id);
        return id;
      }
      n++;
    }
  };
  const firstId = nextId();
  const series = input.repeat ? { seriesId: firstId } : {};
  const added = days.map((day, i) => {
    const [start, end] = i === 0 ? [valid.start, valid.end] : occurrenceBounds(valid, day);
    return { id: i === 0 ? firstId : nextId(), ...valid, start, end, ...series };
  });
  saveMeetings([...list, ...added]);
  return toEvent(firstId);
}
export async function meetingUpdate(
  id: string,
  input: MeetingInput,
): Promise<CalendarEvent> {
  readOnly(id);
  if (input.repeat)
    throw new Error("Cykl można ustawić tylko przy dodawaniu spotkania.");
  const valid = await validMeeting(input);
  const list = storedMeetings();
  const index = list.findIndex((m) => m.id === id);
  if (index < 0) throw new Error("Nie ma takiego spotkania.");
  list[index] = { ...list[index], id, ...valid };
  saveMeetings(list);
  return toEvent(id);
}
export async function meetingDelete(id: string): Promise<void> {
  readOnly(id);
  const list = storedMeetings();
  if (!list.some((m) => m.id === id))
    throw new Error("Nie ma takiego spotkania.");
  saveMeetings(list.filter((m) => m.id !== id));
  // Nadpisanie przypomnienia (C3) znika razem ze spotkaniem, jak w `meetings::delete`.
  forgetMeetingReminders([id]);
}
/** Jak `meetings::delete_following`: to spotkanie i późniejsze z tego samego cyklu; bez cyklu jak `meetingDelete`. */
export async function meetingDeleteFollowing(id: string): Promise<void> {
  readOnly(id);
  const list = storedMeetings();
  const target = list.find((m) => m.id === id);
  if (!target) throw new Error("Nie ma takiego spotkania.");
  if (!target.seriesId) return meetingDelete(id);
  const removed = list.filter(
    (m) => m.seriesId === target.seriesId && m.start >= target.start,
  );
  saveMeetings(list.filter((m) => !removed.includes(m)));
  forgetMeetingReminders(removed.map((m) => m.id));
}
