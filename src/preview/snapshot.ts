/**
 * Dane zadań podglądu w localStorage: odczyt (z zadaniami cyklicznymi i polami
 * Jiry, jak `db::snapshot`) i zapis.
 */
import { detailsOf, type Snapshot } from "../api/task";
import { attachPreviewJiraFields } from "./jira-state";
import { dayKey } from "../time";
import { emptyRule, isRecurring, occurs } from "../recurrence";
export const snapshotKey = "tuktuk-track-preview-v1";
export async function read(): Promise<Snapshot> {
  const raw = localStorage.getItem(snapshotKey);
  const data: Snapshot = raw ? JSON.parse(raw) : { tasks: [], sessions: [] };
  if (generateRecurring(data)) saveSnapshot(data);
  attachPreviewJiraFields(data);
  return data;
}
/**
 * Dni robocze i święta z ustawień podglądu (klucz jak w `settings.ts`, bez
 * importu, który zamknąłby cykl modułów) dla reguły „ostatni dzień roboczy”.
 */
function previewWorkSettings(): {
  workDays: number[];
  holidaysInTarget: boolean;
} {
  let stored: Record<string, unknown> = {};
  try {
    stored =
      JSON.parse(
        localStorage.getItem("tuktuk-track-preview-settings-v1") ?? "{}",
      ) ?? {};
  } catch {
    stored = {};
  }
  const days = stored.workDays;
  return {
    workDays:
      Array.isArray(days) && days.every((d) => Number.isInteger(d))
        ? (days as number[])
        : [1, 2, 3, 4, 5],
    holidaysInTarget: stored.holidaysInTarget !== false,
  };
}
function generateRecurring(data: Snapshot): boolean {
  let changed = false;
  const today = dayKey(Date.now());
  const work = previewWorkSettings();
  const earliest = new Date();
  earliest.setDate(earliest.getDate() - 365);
  for (const source of [...data.tasks]) {
    const rule = source.details;
    if (!rule || !isRecurring(rule) || !rule.repeatFrom || source.archived)
      continue;
    const last = data.recurrenceState?.[source.id];
    const cursor = new Date((last || rule.repeatFrom) + "T12:00:00");
    if (last) cursor.setDate(cursor.getDate() + 1);
    if (dayKey(+cursor) < rule.repeatFrom)
      cursor.setTime(+new Date(rule.repeatFrom + "T12:00:00"));
    if (+cursor < +earliest) cursor.setTime(+earliest);
    while (Number.isFinite(+cursor) && dayKey(+cursor) <= today) {
      const day = dayKey(+cursor);
      if (
        occurs(rule, rule.repeatFrom, day, work) &&
        rule.plannedDay !== day
      ) {
        data.tasks.unshift({
          id:
            Math.max(
              0,
              ...data.tasks.map((t) => t.id),
              ...(data.trash ?? []).map((t) => t.id),
            ) + 1,
          title: source.title,
          done: false,
          createdAt: Date.now(),
          details: {
            ...detailsOf(source),
            notes: {},
            waiting: "",
            plannedDay: day,
            dueDay: "",
            repeatDays: [],
            repeatFrom: "",
            repeatRule: emptyRule(),
            waitingSince: 0,
            waitingStatus: "",
            waitingDismissed: "",
            waitingRemindDay: "",
            pinnedAt: 0,
            checklist: (rule.checklist ?? []).map((i) => ({
              ...i,
              done: false,
            })),
          },
        });
      }
      (data.recurrenceState ??= {})[source.id] = day;
      changed = true;
      cursor.setDate(cursor.getDate() + 1);
    }
  }
  return changed;
}
/** Zapis danych zadań podglądu (odpowiednik transakcji w bazie aplikacji). */
export function saveSnapshot(data: Snapshot) {
  localStorage.setItem(snapshotKey, JSON.stringify(data));
}
