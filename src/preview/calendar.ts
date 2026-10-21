/** Symulowany kalendarz Microsoft podglądu (`calendar.rs`). */
import {
  calendarClientIdMessage,
  calendarEvent,
  guid,
  validTenant,
  type CalendarEvent,
  type CalendarStatus,
} from "../api/calendar";
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
export const calendarKey = "todo-track-preview-calendar-v1";
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
export async function calendarEvents(
  from: number,
  to: number,
): Promise<CalendarEvent[]> {
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const c = previewCalendar();
  // Jak `calendar_events`: bez połączonego konta żadne spotkania nie obowiązują.
  if (!c.email) return [];
  return c.events
    .filter(
      (e) => e.start < to && (e.end > from || (e.end === e.start && e.start >= from)),
    )
    .sort((a, b) => a.start - b.start || a.end - b.end || a.id.localeCompare(b.id));
}
