/**
 * Rano i przegląd tygodnia z asystentem (PLAN_BACKLOG.md, A6): typy komend i
 * stałe wspólne dla aplikacji i podglądu. Odpowiada `plan_assist.rs`.
 */

/** Propozycja przesunięcia zadania z dzisiejszego planu (`plan_assist::MorningMove`). */
export interface MorningMove {
  /** Numer w odpowiedzi: stabilny klucz karty. */
  id: number;
  taskId: number;
  taskTitle: string;
  /** Dzień planu, z którego zadanie schodzi (dziś). */
  fromDay: string;
  /** Jeden z najbliższych dni roboczych, nie późniejszy niż termin zadania. */
  toDay: string;
  reason: string;
}

/** Wynik „Komentarz asystenta” (`plan_assist::MorningComment`). */
export interface MorningComment {
  day: string;
  comment: string;
  moves: MorningMove[];
  /** Powód braku komentarza (pusty, gdy jest). */
  message: string;
}

/** Wynik „Szkic wniosków” (`plan_assist::WeekDraft`). */
export interface WeekDraft {
  /** Poniedziałek tygodnia. */
  week: string;
  text: string;
  /** Powód braku szkicu (pusty, gdy jest). */
  message: string;
}

/** Wiadomości jak w `plan_assist.rs`. */
export const planAssistMessages = {
  off: "Asystent w Rano i przeglądzie tygodnia jest wyłączony w Ustawieniach → Asystent AI.",
  emptyMorning: "Brak zadań w planie dnia ani terminów do oceny.",
  nothingMorning: "Asystent nie ma uwag do planu.",
  emptyWeek: "Brak danych z tego tygodnia do szkicu.",
  nothingWeek: "Asystent nie napisał szkicu wniosków.",
} as const;

/** Ile dni roboczych po dzisiejszym jest oferowanych (`plan_assist::HORIZON_DAYS`). */
export const HORIZON_DAYS = 5;
/** Terminy z tego okna (dni od dziś) trafiają do modelu. */
export const DEADLINE_WINDOW = 14;
/** Limity jak w `plan_assist.rs`. */
export const PLAN_MAX = 40;
export const DEADLINES_MAX = 20;
export const MOVES_MAX = 10;
export const COMMENT_MAX = 1500;
export const REASON_MAX = 300;
export const SUBJECT_MAX = 160;
export const WEEK_TASKS_MAX = 15;
export const NOTE_MAX = 300;
export const DRAFT_MAX = 3000;
