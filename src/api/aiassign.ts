/**
 * Asystent przypisań (PLAN_BACKLOG.md, A4): typy komend i wspólne stałe aplikacji
 * i podglądu. Odpowiada `assign.rs`.
 */

/** Zadanie TS z listy formularza dnia w płaskiej postaci (`assign::TsOption`). */
export interface TsOption {
  id: string;
  name: string;
  /** Nazwa projektu TS. */
  project: string;
}

/**
 * Wynik pytania o zadanie TS (`assign::TsPick`): `tsTaskId` z listy albo `null`,
 * gdy asystent nie jest pewny; `reason` to uzasadnienie albo powód braku propozycji.
 */
export interface TsSuggestion {
  tsTaskId: string | null;
  confidence: number;
  reason: string;
}

/** `Match::source` podpowiedzi asystenta w banerze propozycji (`assign::SOURCE`). */
export const ASSISTANT_SOURCE = "assistant";
/** Najmniejsza pewność podpowiedzi (`assign::MIN_CONFIDENCE`). */
export const MIN_CONFIDENCE = 0.6;
/** Ile otwartych zadań trafia do modelu (`assign::OPEN_MAX`). */
export const OPEN_MAX = 80;
/** Najdłuższy fragment promptu sesji i notatki dnia (`assign::TEXT_MAX`). */
export const TEXT_MAX = 500;
