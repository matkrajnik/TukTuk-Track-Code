import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { desktop } from "./store";
import type { MorningComment, WeekDraft } from "./api/aiplan";
import * as aiPlanSim from "./preview/aiplan";
import { useSettings } from "./target";

export * from "./api/aiplan";

/**
 * Asystent w Rano i przeglądzie tygodnia (A6) jest dostępny: wybrany silnik (nie
 * „off” ani „jeszcze nie pytano”) i włączony „Asystent w Rano i przeglądzie
 * tygodnia” (`aiPlanAssist`).
 */
export function usePlanAssistEnabled() {
  const settings = useSettings();
  return computed(
    () =>
      (settings.value.aiEngine === "claude" ||
        settings.value.aiEngine === "codex") &&
      settings.value.aiPlanAssist,
  );
}

/**
 * `ai_morning_comment`: jedno wywołanie modelu na kliknięcie „Komentarz
 * asystenta”; zwraca komentarz i propozycje przesunięć, niczego nie zapisuje.
 * W podglądzie odpowiada symulowany asystent (`preview/aiplan.ts`).
 */
export async function aiMorningComment(day: string): Promise<MorningComment> {
  if (desktop) return invoke("ai_morning_comment", { day });
  return aiPlanSim.aiMorningComment(day);
}

/** `ai_morning_preview`: „Pokaż, co zostanie wysłane” po filtrze osobowym. */
export async function aiMorningPreview(day: string): Promise<string> {
  if (desktop) return invoke("ai_morning_preview", { day });
  return aiPlanSim.aiMorningPreview(day);
}

/**
 * `ai_week_draft`: jedno wywołanie modelu na kliknięcie „Szkic wniosków”;
 * zwraca tekst do pola „Wnioski”, niczego nie zapisuje.
 */
export async function aiWeekDraft(week: string): Promise<WeekDraft> {
  if (desktop) return invoke("ai_week_draft", { week });
  return aiPlanSim.aiWeekDraft(week);
}

/** `ai_week_preview`: „Pokaż, co zostanie wysłane” po filtrze osobowym. */
export async function aiWeekPreview(week: string): Promise<string> {
  if (desktop) return invoke("ai_week_preview", { week });
  return aiPlanSim.aiWeekPreview(week);
}

/**
 * Szkic wniosków wstawiony do pola: puste pole dostaje sam szkic, a pole z
 * tekstem — szkic pod nim, po pustym wierszu (nic, co użytkownik napisał,
 * nie ginie).
 */
export function withDraft(current: string, draft: string): string {
  const text = draft.trim();
  const base = current.trim();
  if (!text) return current;
  return base ? `${base}\n\n${text}` : text;
}
