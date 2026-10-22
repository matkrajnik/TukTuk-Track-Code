import { computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { desktop, type Snapshot } from "./store";
import type { EstimateCorrection, MorningComment, WeekDraft } from "./api/aiplan";
import type { CorrectionOf } from "./estimates";
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
export async function aiMorningComment(
  day: string,
  correction: EstimateCorrection | null = null,
): Promise<MorningComment> {
  if (desktop) return invoke("ai_morning_comment", { day, correction });
  return aiPlanSim.aiMorningComment(day, correction);
}

/** `ai_morning_preview`: „Pokaż, co zostanie wysłane” po filtrze osobowym. */
export async function aiMorningPreview(
  day: string,
  correction: EstimateCorrection | null = null,
): Promise<string> {
  if (desktop) return invoke("ai_morning_preview", { day, correction });
  return aiPlanSim.aiMorningPreview(day, correction);
}

/**
 * Mnożniki poprawki szacunków (`corrections` z `estimates.ts`) jako tabela dla
 * komendy: mnożnik ogólny i wpisy dla projektów zadań, których mnożnik jest inny.
 * Klucz to nazwa projektu dokładnie jak w zadaniu.
 */
export function correctionTable(data: Snapshot, of: CorrectionOf): EstimateCorrection {
  const all = of("").factor;
  const projects: Record<string, number> = {};
  const seen = new Set<string>();
  for (const task of data.tasks) {
    const project = task.details?.project ?? "";
    if (!project || seen.has(project)) continue;
    seen.add(project);
    const factor = of(project).factor;
    if (factor !== all) projects[project] = factor;
  }
  return { all, projects };
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
