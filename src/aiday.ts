import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { desktop } from "./store";
import type { DayAnalysis, DayProposal } from "./api/aiday";
import * as aiDaySim from "./preview/aiday";
import { useSettings } from "./target";

export * from "./api/aiday";

/**
 * Asystent w Zamknięciu dnia (A5) jest dostępny: wybrany silnik (nie „off” ani
 * „jeszcze nie pytano”) i włączony „Asystent w Zamknięciu dnia” (`aiDayClose`).
 */
export function useDayAssistEnabled() {
  const settings = useSettings();
  return computed(
    () =>
      (settings.value.aiEngine === "claude" ||
        settings.value.aiEngine === "codex") &&
      settings.value.aiDayClose,
  );
}

/**
 * `ai_day_analyze`: jedno wywołanie modelu na kliknięcie „Przeanalizuj dzień”;
 * zwraca propozycje i niczego nie zapisuje. W podglądzie odpowiada symulowany
 * asystent (`preview/aiday.ts`).
 */
export async function aiDayAnalyze(day: string): Promise<DayAnalysis> {
  if (desktop) return invoke("ai_day_analyze", { day });
  return aiDaySim.aiDayAnalyze(day);
}

/**
 * `ai_day_preview`: „Pokaż, co zostanie wysłane” — treść zapytania o dzień po
 * filtrze danych osobowych; nic nie wywołuje i nic nie wysyła.
 */
export async function aiDayPreview(day: string): Promise<string> {
  if (desktop) return invoke("ai_day_preview", { day });
  return aiDaySim.aiDayPreview(day);
}

/** Propozycje danego rodzaju. */
export const ofKind = (list: readonly DayProposal[], kind: DayProposal["kind"]) =>
  list.filter((p) => p.kind === kind);
