/**
 * Komendy kalendarza Microsoft (`calendar.rs`) i ręcznych spotkań
 * (`meetings.rs`, B12) z odpowiednikami w podglądzie (`preview/calendar.ts`);
 * osobno, żeby `store.ts` został cienką warstwą. `store.ts` eksportuje je dalej
 * (`export *`), więc komponenty importują je jak dotąd z `store`.
 * Nie importuje `store.ts` (bez cyklu importów).
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  calendarEvent,
  type CalendarEvent,
  type CalendarStatus,
  type MeetingInput,
} from "./api/calendar";
import * as calendarSim from "./preview/calendar";

const desktop = isTauri();

export async function calendarStatus(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_status");
  return calendarSim.calendarStatus();
}
/**
 * „Połącz konto Microsoft”: logowanie w przeglądarce i pierwsze pobranie.
 * Podgląd symuluje udane logowanie i „pobiera” spotkania z `sample`.
 */
export async function calendarConnect(
  clientId: string,
  tenant: string,
): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_connect", { clientId, tenant });
  return calendarSim.calendarConnect(clientId, tenant);
}
/** „Rozłącz”: usuwa token i spotkania; client ID i dzierżawca zostają. */
export async function calendarDisconnect(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_disconnect");
  return calendarSim.calendarDisconnect();
}
/** „Odśwież”: pobiera spotkania teraz. */
export async function calendarRefresh(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_refresh");
  return calendarSim.calendarRefresh();
}
/** Spotkania nakładające się na `[from, to)`, według początku. */
export async function calendarEvents(
  from: number,
  to: number,
): Promise<CalendarEvent[]> {
  if (desktop) return invoke("calendar_events", { from, to });
  return calendarSim.calendarEvents(from, to);
}
/** „Zapisz” nowego ręcznego spotkania (B12). */
export async function meetingAdd(meeting: MeetingInput): Promise<CalendarEvent> {
  if (desktop) return invoke("meeting_add", { meeting });
  return calendarSim.meetingAdd(meeting);
}
/** „Zapisz” istniejącego ręcznego spotkania; spotkania z Outlooka są tylko do odczytu. */
export async function meetingUpdate(
  id: string,
  meeting: MeetingInput,
): Promise<CalendarEvent> {
  if (desktop) return invoke("meeting_update", { id, meeting });
  return calendarSim.meetingUpdate(id, meeting);
}
/** „Usuń” ręcznego spotkania. */
export async function meetingDelete(id: string): Promise<void> {
  if (desktop) return invoke("meeting_delete", { id });
  return calendarSim.meetingDelete(id);
}
/** „Usuń to i kolejne” w `MeetingEditor`: spotkanie i późniejsze z tego samego cyklu (`meetings::delete_following`). */
export async function meetingDeleteFollowing(id: string): Promise<void> {
  if (desktop) return invoke("meeting_delete_following", { id });
  return calendarSim.meetingDeleteFollowing(id);
}
/** Nasłuch zmian kalendarza (pobranie, połączenie, rozłączenie, ręczne spotkania). */
export async function onCalendarChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(calendarEvent, () => handler());
  window.addEventListener(calendarEvent, handler);
  return () => window.removeEventListener(calendarEvent, handler);
}
