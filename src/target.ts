import { ref, readonly } from "vue";
import {
  defaultSettings,
  onSettingsChanged,
  readSettings,
  type AppSettings,
} from "./settings";
import { dayBounds, dayKey, hm } from "./time";
import {
  dayTarget,
  isoWeekday,
  type TargetSettings,
} from "./workday";

/** Formaty czasu trwania (z `time.ts`), eksportowane też stąd. */
export { hm, hhmm } from "./time";
/** Norma dnia (`workday.ts`), eksportowana też stąd. */
export {
  dayTarget,
  isoWeekday,
  isWorkDay,
  targetHoliday,
  type TargetSettings,
} from "./workday";

/** Cel kilku dni (tygodnia, miesiąca) = suma celów dni roboczych. */
export const daysTarget = (settings: TargetSettings, days: readonly string[]) =>
  days.reduce((sum, day) => sum + dayTarget(settings, day), 0);

/** Klucze dni tygodnia Pon–Nd zawierającego `day`. */
export function weekDays(day: string): string[] {
  const monday = new Date(dayBounds(day)[0]);
  monday.setDate(monday.getDate() - isoWeekday(day) + 1);
  return Array.from({ length: 7 }, (_, i) =>
    dayKey(
      new Date(
        monday.getFullYear(),
        monday.getMonth(),
        monday.getDate() + i,
      ).getTime(),
    ),
  );
}

/** Klucze wszystkich dni miesiąca „RRRR-MM” (odporne na zmianę czasu). */
export function monthDays(month: string): string[] {
  const [y, m] = month.split("-").map(Number);
  const count = new Date(y, m, 0).getDate();
  return Array.from({ length: count }, (_, i) =>
    dayKey(new Date(y, m - 1, i + 1).getTime()),
  );
}

/** Cel tygodnia zawierającego `day`: dni robocze bez świąt × cel dnia. */
export const weekTarget = (settings: TargetSettings, day: string) =>
  daysTarget(settings, weekDays(day));

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
