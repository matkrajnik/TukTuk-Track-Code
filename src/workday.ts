import { dayBounds } from "./time";
import { holidayName } from "./holidays";

/**
 * Norma dnia (cel dzienny z Ustawień → Czas pracy) bez zależności od stanu
 * aplikacji: czyste funkcje używane przez `target.ts` (cele i paski) i
 * `report.ts` (nadgodziny, B4/B5). Osobny moduł, żeby `report.ts` nie ciągnął
 * `settings.ts` i `store.ts` (cykl importów przez `timesheet.ts`).
 */
export interface TargetSettings {
  readonly targetDayMinutes: number;
  readonly workDays: readonly number[];
  /** Święta ustawowe nie są dniami roboczymi (domyślnie tak). */
  readonly holidaysInTarget?: boolean;
}

/** Dzień tygodnia ISO (1 = poniedziałek, 7 = niedziela) dla klucza dnia. */
export function isoWeekday(day: string): number {
  return ((new Date(dayBounds(day)[0]).getDay() + 6) % 7) + 1;
}

/** Święto, które zwalnia dzień z celu (null, gdy święta nie są uwzględniane). */
export const targetHoliday = (settings: TargetSettings, day: string) =>
  settings.holidaysInTarget === false ? null : holidayName(day);

/** Dzień roboczy: dzień tygodnia z ustawień, który nie jest uwzględnianym świętem. */
export const isWorkDay = (settings: TargetSettings, day: string) =>
  settings.workDays.includes(isoWeekday(day)) && !targetHoliday(settings, day);

/** Cel dnia w ms; 0 dla dnia wolnego i święta. */
export const dayTarget = (settings: TargetSettings, day: string) =>
  isWorkDay(settings, day) ? settings.targetDayMinutes * 60_000 : 0;
