/**
 * Ideo Timesheet (PLAN_1.8.md): typy komend, komunikaty i reguły walidacji
 * z `timesheet.rs`, wspólne dla aplikacji i podglądu. Hasło nigdy nie jest
 * zapisywane po stronie interfejsu.
 */
import { tsProjectKey } from "../timesheet";
export interface TsConfig {
  apiUrl: string;
  login: string;
  deviceId: string;
  deviceName: string;
}
export interface TsSettings {
  config: TsConfig;
  /** Hasło jest w magazynie poświadczeń. */
  connected: boolean;
  /** Ustawienie „Wysyłaj raport TS” (`tsSend`). */
  sendEnabled: boolean;
}
export interface TsTask {
  id: string;
  name: string;
}
export interface TsProject {
  name: string;
  tasks: TsTask[];
}
/** Wpis wypełniony w TS; wartości jako tekst, jak oddał serwer. */
export interface TsFilled {
  id: string;
  hours: string;
  overtime: string;
  description: string;
}
/** Wpis wysyłany do TS (`timesheet::Entry`). */
export interface TsEntry {
  taskId: string;
  hours: string;
  overtime: string;
  description: string;
}
export type TsMode = "replace" | "keep";
export interface TsSendResult {
  filled: TsFilled[];
  sentAt: number;
  /** Zapis przyjęty, ale odczyt dnia po zapisie się nie udał (`filled` puste). */
  warning?: string;
}
export interface TsSent {
  day: string;
  sentAt: number;
  hours: string;
}
export const tsDefaultApi = "https://api.ts.ideo.pl";
export const tsNotConnected = "Połącz konto TS: podaj login i hasło.";
export const tsSendDisabled =
  "Wysyłka do TS jest wyłączona w Ustawieniach → Timesheet.";
export const tsLoginRejected = "TS odrzucił logowanie. Sprawdź login i hasło.";
export const tsReadBackWarning =
  "Zapisano w TS, ale nie udało się odczytać dnia — sprawdź w przeglądarce.";
export const tsNotConfirmed =
  "TS nie potwierdził zapisu — sprawdź dzień w przeglądarce.";
const tsApiError =
  "Podaj główny adres API TS z https, bez ścieżki i parametrów, np. https://api.ts.ideo.pl.";
/** Adres API jak `timesheet::normalize_api`. */
export function tsNormalizeApi(value: string): string {
  const raw = value.trim();
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(tsApiError);
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.pathname !== "/" ||
    /[?#]/.test(raw) ||
    url.username ||
    url.password
  )
    throw new Error(tsApiError);
  return url.origin;
}
const tsValidDay = (value: string) => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return false;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  return d.getUTCMonth() === +m[2] - 1 && d.getUTCDate() === +m[3];
};
export function tsCheckDate(value: string) {
  if (!tsValidDay(value))
    throw new Error("Niepoprawny dzień (oczekiwany format RRRR-MM-DD).");
}
/** Godziny jak `timesheet::valid_hours`. */
export const tsValidHours = (value: string) =>
  /^\d{1,6}([.,]\d{1,2})?$/.test(value);
export const tsControl = /[\u0000-\u001f\u007f-\u009f]/;
/** Walidacja wpisów jak `timesheet::validate_entries`. */
export function tsValidateEntries(entries: TsEntry[]) {
  if (!entries.length) throw new Error("Brak wpisów do zapisania w TS.");
  if (entries.length > 50)
    throw new Error("Jednorazowo można zapisać w TS najwyżej 50 wpisów.");
  for (const e of entries) {
    const id = e.taskId.trim();
    if (!id || [...id].length > 20 || tsControl.test(id))
      throw new Error("Niepoprawny identyfikator zadania TS.");
    if (!tsValidHours(e.hours) || !tsValidHours(e.overtime))
      throw new Error(
        "Godziny w TS podaj jako liczbę z najwyżej dwoma miejscami po przecinku, np. 1,25.",
      );
    if ([...e.description].length > 2000)
      throw new Error("Opis wpisu TS może mieć najwyżej 2000 znaków.");
    if (tsControl.test(e.description))
      throw new Error("Opis wpisu TS nie może zawierać znaków sterujących.");
  }
}
/** `keep` jak `timesheet::merge_keep`: nasze wpisy, potem wpisy TS spoza listy. */
export function tsMergeKeep(entries: TsEntry[], filled: TsFilled[]): TsEntry[] {
  const ours = new Set(entries.map((e) => e.taskId.trim()));
  return [
    ...entries,
    ...filled
      .filter((f) => !ours.has(f.id.trim()))
      .map((f) => ({
        taskId: f.id,
        hours: f.hours,
        overtime: f.overtime,
        description: f.description,
      })),
  ];
}
/** Suma godzin z odczytu TS jak `timesheet::hours_total` („7,50”). */
export function tsHoursTotal(filled: TsFilled[]): string {
  const hundredths = filled.reduce((sum, f) => {
    const raw = f.hours.trim();
    const v = Number(raw.replace(",", "."));
    return raw && Number.isFinite(v) ? sum + Math.round(v * 100) : sum;
  }, 0);
  return `${Math.trunc(hundredths / 100)},${String(hundredths % 100).padStart(2, "0")}`;
}
export const tsEntryRow = (e: TsEntry): TsFilled => ({
  id: e.taskId.trim(),
  hours: e.hours,
  overtime: e.overtime,
  description: e.description,
});
/**
 * Jak `timesheet::sent_outcome`: odczyt po zapisie musi zawierać każde wysłane
 * zadanie z niezerowymi godzinami (inaczej błąd, bez znacznika wysyłki); brak
 * odczytu (`null`) → znacznik z sumą wysłanych godzin i ostrzeżenie.
 */
export function tsSentOutcome(
  sent: TsEntry[],
  read: TsFilled[] | null,
): { filled: TsFilled[]; hours: string; warning?: string } {
  if (read === null)
    return {
      filled: [],
      hours: tsHoursTotal(sent.map(tsEntryRow)),
      warning: tsReadBackWarning,
    };
  const ok = sent
    .filter((e) => Number(e.hours.trim().replace(",", ".")) > 0)
    .every((e) => read.some((f) => f.id.trim() === e.taskId.trim()));
  if (!ok) throw new Error(tsNotConfirmed);
  return { filled: read, hours: tsHoursTotal(read) };
}
/**
 * Zmiana konta, wysyłki albo domyślnych zadań TS: zdarzenie okna po każdej
 * udanej komendzie zmieniającej stan, a w podglądzie także zdarzenie `storage`
 * dla `previewTsKey` (inne okno albo test zmienia symulowany TS), jak przy Jirze.
 */
export const tsChangedEvent = "todo-track-ts-changed";
export function notifyTs() {
  window.dispatchEvent(new Event(tsChangedEvent));
}
// ---------------------------------------------------------------------------
// Zadania TS dla zadań Todo Track (PLAN_1.8.md, T2).

/** Domyślne zadanie TS projektu Todo Track (`timesheet::ProjectMap`). */
export interface TsProjectMapping {
  project: string;
  tsTaskId: string;
  tsTaskName: string;
}
/** Identyfikator zadania TS jak `timesheet::valid_task_id`. */
export const tsValidTaskId = (id: string) =>
  !!id && [...id].length <= 20 && !tsControl.test(id);
/** Nazwa zadania TS jak `timesheet::valid_task_name`. */
export const tsValidTaskName = (name: string) =>
  [...name].length <= 300 && !tsControl.test(name);
export const tsSortMap = (list: TsProjectMapping[]) =>
  [...list].sort((a, b) => {
    const x = tsProjectKey(a.project);
    const y = tsProjectKey(b.project);
    return x < y ? -1 : x > y ? 1 : 0;
  });
