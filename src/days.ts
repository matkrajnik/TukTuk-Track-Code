/**
 * Rozpocznij, zakończ i wznów dzień (PLAN_BACKLOG.md, B13): stan dnia z
 * `dayMarks` migawki, komendy `day_start` / `day_end` / `day_resume`
 * (w podglądzie `src/preview/days.ts`) i wspólny przebieg dla przycisków
 * w Rano, Zamknięciu dnia, pasku, widgecie i trayu.
 *
 * Timer zmienia się tylko po kliknięciu: „Zakończ dzień” przy trwającym
 * pomiarze pyta „Zakończyć dzień i zatrzymać pomiar »X«?”, a Start po
 * zakończeniu dnia pyta „Wznowić dzień?” (`DayDialogs.vue`).
 */
import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import {
  change,
  desktop,
  read,
  type DayMark,
  type Snapshot,
} from "./store";
import { readSettings, writeSettings } from "./settings";
import * as daysSim from "./preview/days";
import { clock, dayKey } from "./time";

export type DayState = "none" | "started" | "ended";

export function dayMarkOf(
  data: Snapshot,
  day: string,
): DayMark | undefined {
  return data.dayMarks?.find((m) => m.day === day);
}

/**
 * Stan dnia jak `days::state_of`. Przegląd dnia zapisany przed B13
 * (`morningReviewDay`, bez wiersza `day_marks`) też oznacza rozpoczęty dzień.
 */
export function dayStateOf(
  mark: DayMark | undefined,
  reviewedToday = false,
): DayState {
  if (mark?.endedAt != null) return "ended";
  if (mark && (mark.startedAt != null || mark.resumed > 0)) return "started";
  return reviewedToday ? "started" : "none";
}

/** Godziny dnia: „08:05–16:40”, „od 08:05”, „do 16:40” albo pusto. */
export function dayHours(mark: DayMark | undefined): string {
  if (!mark) return "";
  const from = mark.startedAt != null ? clock(mark.startedAt) : "";
  const to = mark.endedAt != null ? clock(mark.endedAt) : "";
  if (from && to) return `${from}–${to}`;
  if (from) return `od ${from}`;
  if (to) return `do ${to}`;
  return "";
}

/** „Dzień: 08:05–16:40 (wznawiany)” do raportów; pusto bez godzin. */
export function dayHoursLine(mark: DayMark | undefined): string {
  const hours = dayHours(mark);
  if (!hours) return "";
  return `Dzień: ${hours}${mark!.resumed > 0 ? " (wznawiany)" : ""}`;
}

/** `day_start`: dzień rozpoczęty teraz, z przeglądem dnia w ustawieniach. */
export async function dayStart(day: string): Promise<Snapshot> {
  if (desktop) return invoke("day_start", { day });
  const { data, first } = await daysSim.dayStart(day);
  if (first)
    await writeSettings({ morningReviewDay: day, morningReviewAt: Date.now() });
  return data;
}

/** `day_end`: `stopMeasurement` tylko z potwierdzenia w dialogu. */
export async function dayEnd(
  day: string,
  stopMeasurement: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("day_end", { day, stopMeasurement });
  const s = await readSettings();
  const reviewAt =
    s.morningReviewDay === day && s.morningReviewAt > 0
      ? s.morningReviewAt
      : null;
  return daysSim.dayEnd(day, stopMeasurement, reviewAt);
}

/** `day_resume`: tylko zakończony dzisiejszy dzień. */
export async function dayResume(day: string): Promise<Snapshot> {
  if (desktop) return invoke("day_resume", { day });
  return daysSim.dayResume(day);
}

/** „Wznów dzień”, a z `taskId` też „Wznów pomiar »X«” (osobne kliknięcie). */
let resuming = false;
export async function resumeDay(taskId: number | null = null): Promise<Snapshot> {
  const data = await dayResume(dayKey(Date.now()));
  if (taskId == null) return data;
  // Dzień jest już wznowiony: strażnik startu nie pyta drugi raz (dane okna
  // mogą jeszcze nie mieć wznowienia).
  resuming = true;
  try {
    return await change("start", taskId);
  } finally {
    resuming = false;
  }
}

/** Pytanie dialogu dnia w bieżącym oknie. */
export type DayQuestion = { kind: "end"; title: string } | { kind: "start" };
export const dayQuestion = ref<DayQuestion | null>(null);
let answer: ((ok: boolean) => void) | null = null;
export function askDay(question: DayQuestion): Promise<boolean> {
  answer?.(false);
  dayQuestion.value = question;
  return new Promise((resolve) => (answer = resolve));
}
export function answerDay(ok: boolean) {
  const resolve = answer;
  answer = null;
  dayQuestion.value = null;
  resolve?.(ok);
}

/**
 * Strażnik startu (`setStartGuard`): po zakończeniu dzisiejszego dnia pyta
 * „Wznowić dzień?”; „Wznów i start” wznawia dzień, „Anuluj” blokuje start.
 */
export async function confirmStart(
  current: Snapshot | null = null,
): Promise<boolean> {
  if (resuming) return true;
  const today = dayKey(Date.now());
  // Dane okna, gdy są (bez dodatkowego `get_snapshot` przy każdym starcie, B6).
  const data = current ?? (await read());
  if (dayStateOf(dayMarkOf(data, today)) !== "ended") return true;
  if (!(await askDay({ kind: "start" }))) return false;
  await dayResume(today);
  return true;
}

/**
 * „Zakończ dzień” w głównym oknie (`DayDialogs.vue`): dialog przy trwającym
 * pomiarze, zakończenie i Zamknięcie dnia. Okna bez obsługi (widget) proszą
 * główne okno (`day_end_request`).
 */
let endHandler: (() => Promise<void>) | null = null;
export function setDayEndHandler(handler: (() => Promise<void>) | null) {
  endHandler = handler;
}
export async function requestDayEnd(): Promise<void> {
  if (endHandler) return endHandler();
  if (desktop) return invoke("day_end_request");
  await dayEnd(dayKey(Date.now()), false);
}
