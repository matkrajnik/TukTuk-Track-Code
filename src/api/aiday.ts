/**
 * Zamknięcie dnia z asystentem (PLAN_BACKLOG.md, A5): typy komend i stałe wspólne
 * dla aplikacji i podglądu. Odpowiada `day_assist.rs`.
 */

/** Rodzaje propozycji asystenta (`day_assist::schema`). */
export type DayProposalKind = "gap" | "unassigned" | "note" | "ts_description";

/**
 * `fill_gap` (B3): luka obejmuje cały przedział między pomiarami i jest
 * przypisywana pomiarem przed nią (`previous`) albo po niej (`next`).
 */
export interface GapFill {
  beforeId: number;
  afterId: number;
  mode: "previous" | "next";
}

/** Propozycja po zmapowaniu na dane dnia (`day_assist::DayProposal`). */
export interface DayProposal {
  /** Numer w odpowiedzi: stabilny klucz karty. */
  id: number;
  kind: DayProposalKind;
  /** Godziny HH:MM; puste przy `note` i `ts_description`. */
  from: string;
  to: string;
  /** Chwile przedziału dla `gap` i `unassigned` (ms). */
  fromMs: number | null;
  toMs: number | null;
  taskId: number;
  taskTitle: string;
  text: string;
  reason: string;
  /** `unassigned`: pełny folder agenta do przypisania (`agent_bind`). */
  folder: string | null;
  /** `gap` obejmująca całą lukę między pomiarami sąsiedniego zadania. */
  fill: GapFill | null;
}

/** Wynik „Przeanalizuj dzień” (`day_assist::DayAnalysis`). */
export interface DayAnalysis {
  day: string;
  proposals: DayProposal[];
  /** Powód braku propozycji (pusty, gdy są). */
  message: string;
}

/** Wiadomości jak w `day_assist.rs`. */
export const dayAssistMessages = {
  off: "Asystent w Zamknięciu dnia jest wyłączony w Ustawieniach → Asystent AI.",
  empty: "Brak danych z tego dnia do analizy.",
  nothing: "Asystent nie znalazł nic do zaproponowania.",
} as const;

/** Najkrótsza luka, o której pyta się model (`day_assist::GAP_MIN_MS`). */
export const GAP_MIN_MS = 5 * 60_000;
/** Limity jak w `day_assist.rs`. */
export const PROPOSALS_MAX = 30;
export const TEXT_MAX = 1000;
export const REASON_MAX = 300;
export const SUBJECT_MAX = 160;
export const NOTE_MAX = 1000;

/** Proponowany opis wpisu TS dla zadania (`ts_description`) w podglądzie „Wyślij do TS”. */
export interface TsDescriptionHint {
  taskId: number;
  text: string;
  reason: string;
}
