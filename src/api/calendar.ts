/** Kalendarz Microsoft: typy, walidacja i reguły spotkań (`calendar.rs`). */

/** Stan kalendarza Microsoft (`calendar_status`). */
export interface CalendarStatus {
  clientId: string;
  tenant: string;
  /** Zalogowane konto; pusty, gdy niepołączone. */
  email: string;
  connected: boolean;
  /** Trwa logowanie w przeglądarce. */
  connecting: boolean;
  lastSync: number | null;
  lastError: string;
}
/** Spotkanie (`calendar_events`): czasy w ms, całodniowe od lokalnej północy. */
export interface CalendarEvent {
  id: string;
  start: number;
  end: number;
  subject: string;
  /** `free`, `tentative`, `busy`, `oof`, `workingElsewhere` albo `unknown`. */
  showAs: string;
  allDay: boolean;
  cancelled: boolean;
  /** Odpowiedź na zaproszenie, np. `accepted`, `declined`, `organizer`. */
  response: string;
  /**
   * Źródło (B12): `outlook` (Microsoft 365, tylko do odczytu; domyślne, gdy
   * brak pola) albo `manual` (ręczne spotkanie z `manual_meetings`).
   */
  source?: "outlook" | "manual";
  /** Zadanie ręcznego spotkania („Dodaj jako czas” w Zamknięciu dnia). */
  taskId?: number | null;
  /** Cykl ręcznego spotkania: identyfikator pierwszego wystąpienia (`series_id`); brak = pojedyncze. */
  seriesId?: string | null;
}
/** Cykl nowego ręcznego spotkania (`meetings::MeetingRepeat`): dni ISO 1–7, co `interval` (1–8) tygodni, do `until` (RRRR-MM-DD, włącznie). */
export interface MeetingRepeat {
  days: number[];
  interval: number;
  until: string;
}
/** Dane formularza ręcznego spotkania (`meeting_add` / `meeting_update`). */
export interface MeetingInput {
  start: number;
  end: number;
  subject: string;
  showAs: string;
  allDay: boolean;
  taskId: number | null;
  /** Cykl, tylko przy dodawaniu (`meeting_add`); `meeting_update` z cyklem zwraca błąd. */
  repeat?: MeetingRepeat | null;
}
/** Czy spotkanie jest ręczne (edycja i „Usuń”), a nie z Outlooka. */
export const isManualMeeting = (e: CalendarEvent) => e.source === "manual";
/** Typy ręcznego spotkania (`meetings::SHOW_AS`) z etykietami formularza. */
export const meetingTypes = [
  { value: "busy", label: "Zajęte" },
  { value: "tentative", label: "Wstępnie" },
  { value: "oof", label: "Poza biurem" },
  { value: "workingElsewhere", label: "Praca gdzie indziej" },
] as const;
/** Najdłuższy temat ręcznego spotkania (`meetings::MAX_SUBJECT`). */
export const MEETING_SUBJECT_MAX = 200;
export const calendarEvent = "calendar-changed";
export const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** Jak `calendar::valid_tenant`: organizations/common/consumers, GUID albo domena. */
export const validTenant = (t: string) =>
  ["organizations", "common", "consumers"].includes(t) ||
  guid.test(t) ||
  (t.length <= 253 &&
    t.includes(".") &&
    t.split(".").every((l) => /^[a-z0-9-]+$/i.test(l)));
export const calendarClientIdMessage =
  "Podaj identyfikator aplikacji (client ID) z rejestracji w Entra ID, np. 00000000-0000-0000-0000-000000000000.";
/**
 * Czy spotkanie zajmuje czas (`calendar::occupies`): busy, oof, workingElsewhere
 * albo tentative, nieanulowane i nieodrzucone.
 */
export const meetingOccupies = (e: CalendarEvent) =>
  ["busy", "oof", "workingElsewhere", "tentative"].includes(e.showAs) &&
  !e.cancelled &&
  e.response !== "declined";
/**
 * Tytuł spotkania w interfejsie: „Spotkanie”, gdy tytuły są wyłączone albo
 * puste. „Pokazuj tytuły spotkań” dotyczy tylko Outlooka: temat ręcznego
 * spotkania wpisał użytkownik, więc jest widoczny zawsze.
 */
export const meetingTitle = (e: CalendarEvent, showTitles: boolean) =>
  ((showTitles || isManualMeeting(e)) && e.subject.trim()) || "Spotkanie";
