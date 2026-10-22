import { offDayEvent } from "./capacity";
import type { AppSettings } from "./settings";
import type { CalendarEvent } from "./store";
import { isWorkDay } from "./workday";

/**
 * Plan dnia przy pierwszym uruchomieniu w danym dniu (B11): główne okno
 * otwiera się na Plan i historia → Rano, gdy w dniu roboczym przegląd dnia nie
 * został jeszcze zapisany. Stan „pokazano dziś” trzyma okno w `localStorage`
 * (także w aplikacji: WebView2 zachowuje go między uruchomieniami), nie baza.
 */
export const morningShownKey = "tuktuk-track-morning-shown-v1";

/**
 * Czy Rano pokazano już w dniu `day`. Zapisany dzień późniejszy niż `day`
 * (zegar cofnięty) też się liczy, żeby nie otwierać Rano ponownie.
 */
export function morningShown(day: string): boolean {
  try {
    const stored = localStorage.getItem(morningShownKey) ?? "";
    return stored !== "" && stored >= day;
  } catch {
    // Bez pamięci okna nie otwieramy Rano za każdym pokazaniem.
    return true;
  }
}

export function markMorningShown(day: string) {
  try {
    localStorage.setItem(morningShownKey, day);
  } catch {
    /* bez pamięci okna aplikacja działa dalej */
  }
}

/**
 * Te same reguły co powiadomienie „Przegląd dnia” (`notify::morning_review_due`)
 * bez godziny początku dnia: przełącznik włączony, dzień roboczy (dni tygodnia
 * i święta z ustawień), bez całodniowego „poza biurem” z kalendarza i bez
 * przeglądu zapisanego na ten dzień.
 */
export function morningStartDue(
  settings: Pick<
    AppSettings,
    | "openMorningOnStart"
    | "morningReviewDay"
    | "workDays"
    | "targetDayMinutes"
    | "holidaysInTarget"
  >,
  day: string,
  events: readonly CalendarEvent[],
): boolean {
  return (
    settings.openMorningOnStart &&
    settings.morningReviewDay !== day &&
    isWorkDay(settings, day) &&
    !offDayEvent(events, day)
  );
}
