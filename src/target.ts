import { ref, readonly } from "vue";
import {
  defaultSettings,
  onSettingsChanged,
  readSettings,
  type AppSettings,
} from "./settings";
import { dayBounds } from "./time";

/** Czas jako G:MM, np. „8:00”, „12:05”. Minuty w dół, jak w trayu. */
export function hm(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000);
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}`;
}
/** Czas jako GG:MM, np. „03:12”. */
export const hhmm = (ms: number) => hm(ms).padStart(5, "0");

/** Dzień tygodnia ISO (1 = poniedziałek, 7 = niedziela) dla klucza dnia. */
export function isoWeekday(day: string): number {
  return ((new Date(dayBounds(day)[0]).getDay() + 6) % 7) + 1;
}

interface TargetSettings {
  readonly targetDayMinutes: number;
  readonly workDays: readonly number[];
}

export const isWorkDay = (settings: TargetSettings, day: string) =>
  settings.workDays.includes(isoWeekday(day));

/** Cel dnia w ms; 0 dla dnia wolnego. */
export const dayTarget = (settings: TargetSettings, day: string) =>
  isWorkDay(settings, day) ? settings.targetDayMinutes * 60_000 : 0;

/** Cel tygodnia = cel dnia × liczba dni roboczych. */
export const weekTarget = (settings: TargetSettings) =>
  settings.targetDayMinutes * settings.workDays.length * 60_000;

/** Postęp 0–100 do paska; po przekroczeniu pasek jest pełny. */
export const progress = (total: number, target: number) =>
  target > 0 ? Math.min(100, (total / target) * 100) : 0;

/** Różnica względem celu z pełnych minut: ujemna = brakuje. */
export function targetDiffMinutes(total: number, target: number): number {
  return Math.floor(Math.max(0, total) / 60_000) - Math.round(target / 60_000);
}

/** Różnica ze znakiem, np. „+0:40”, „−1:20”, „0:00”. */
export function signedDiff(total: number, target: number): string {
  const diff = targetDiffMinutes(total, target);
  const text = hm(Math.abs(diff) * 60_000);
  return diff > 0 ? `+${text}` : diff < 0 ? `−${text}` : text;
}

/** Komunikat zamknięcia dnia; pusty, gdy dzień nie ma celu. */
export function targetMessage(total: number, target: number): string {
  if (target <= 0) return "";
  const diff = targetDiffMinutes(total, target);
  if (diff < 0) return `Do celu brakuje ${hm(-diff * 60_000)}`;
  if (diff > 0) return `Cel przekroczony o ${hm(diff * 60_000)}`;
  return "Cel osiągnięty";
}

const shared = ref<AppSettings>({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
});
let started = false;

/**
 * Wspólne, reaktywne ustawienia dla widoków. Wczytywane raz, potem
 * aktualizowane po zdarzeniu `settings-changed` (także w podglądzie).
 */
export function useSettings() {
  if (!started) {
    started = true;
    void onSettingsChanged((s) => (shared.value = s)).catch(() => undefined);
    void readSettings()
      .then((s) => (shared.value = s))
      .catch(() => undefined);
  }
  return readonly(shared);
}
