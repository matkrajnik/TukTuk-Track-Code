/**
 * Komendy przypomnień (`reminder_*`, `meeting_reminder_*`, C3) z odpowiednikami
 * w podglądzie (`preview/reminders.ts`); osobno, żeby `store.ts` został cienką
 * warstwą.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  remindersEvent,
  type MeetingOverride,
  type MeetingReminderValue,
  type Reminder,
  type ReminderInput,
} from "./api/reminders";
import * as remindersSim from "./preview/reminders";

export * from "./api/reminders";

const desktop = isTauri();

/** `reminder_list`: „Moje przypomnienia” (włączone według terminu, potem wyłączone). */
export async function reminderList(): Promise<Reminder[]> {
  if (desktop) return invoke("reminder_list");
  return remindersSim.reminderList();
}
/** `reminder_add`: nowe przypomnienie (walidacja jak w `reminders.rs`). */
export async function reminderAdd(reminder: ReminderInput): Promise<Reminder> {
  if (desktop) return invoke("reminder_add", { reminder });
  return remindersSim.reminderAdd(reminder);
}
/** `reminder_update`: zmiana, włączenie i wyłączenie przypomnienia. */
export async function reminderUpdate(
  id: number,
  reminder: ReminderInput,
): Promise<Reminder> {
  if (desktop) return invoke("reminder_update", { id, reminder });
  return remindersSim.reminderUpdate(id, reminder);
}
/** `reminder_delete`: usuwa przypomnienie. */
export async function reminderDelete(id: number): Promise<void> {
  if (desktop) return invoke("reminder_delete", { id });
  return remindersSim.reminderDelete(id);
}
/** `reminder_snooze`: odkłada przypomnienie o `minutes` minut. */
export async function reminderSnooze(
  id: number,
  minutes: number,
): Promise<Reminder> {
  if (desktop) return invoke("reminder_snooze", { id, minutes });
  return remindersSim.reminderSnooze(id, minutes);
}
/** `meeting_reminder_list`: nadpisania przypomnień spotkań (brak = domyślne). */
export async function meetingReminderList(): Promise<MeetingOverride[]> {
  if (desktop) return invoke("meeting_reminder_list");
  return remindersSim.meetingReminderList();
}
/** `meeting_reminder_set`: minuty, `null` (bez przypomnienia) albo `"default"`. */
export async function meetingReminderSet(
  meetingId: string,
  minutes: MeetingReminderValue,
): Promise<void> {
  if (desktop) return invoke("meeting_reminder_set", { meetingId, minutes });
  return remindersSim.meetingReminderSet(meetingId, minutes);
}
/** Nasłuch zmian przypomnień (komendy, „Odłóż” z toastu, inne okno). */
export async function onRemindersChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(remindersEvent, () => handler());
  window.addEventListener(remindersEvent, handler);
  return () => window.removeEventListener(remindersEvent, handler);
}
