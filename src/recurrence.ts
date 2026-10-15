/**
 * Reguły cykli (P3) i blokada „Czekam na”. Odpowiednik `src-tauri/src/recurrence.rs`
 * (`occurs`, `last_work_day`) i części `planner.rs` (`normalize_rule`) dla trybu
 * podglądu i interfejsu. Daty to klucze dni RRRR-MM-DD liczone w UTC, żeby zmiana
 * czasu nie przesuwała dni.
 */
import { holidayName } from "./holidays";

/**
 * Reguła cyklu w `task_details` (`repeatRule`):
 * - `""`: co tydzień w dni z `repeatDays` (jak przed 1.7),
 * - `weeks`: dni z `repeatDays` co `interval` tygodni (1–8) od tygodnia `repeatFrom`,
 * - `monthDay`: miesięcznie w dniu `day` (1–31; w krótszym miesiącu ostatni dzień),
 * - `lastWorkday`: ostatni dzień roboczy miesiąca (dni robocze i święta z ustawień),
 * - `nthWeekday`: `nth` (1–4 albo -1 = ostatni) dzień tygodnia `weekday` (1–7).
 */
export type RepeatKind = "" | "weeks" | "monthDay" | "lastWorkday" | "nthWeekday";
export interface RepeatRule {
  kind: RepeatKind;
  interval: number;
  day: number;
  nth: number;
  weekday: number;
}
export const emptyRule = (): RepeatRule => ({
  kind: "",
  interval: 0,
  day: 0,
  nth: 0,
  weekday: 0,
});

interface RuleDetails {
  repeatDays?: number[];
  repeatRule?: Partial<RepeatRule>;
}
interface WorkSettings {
  readonly workDays: readonly number[];
  readonly holidaysInTarget?: boolean;
}

const monthly = (kind: string | undefined) =>
  kind === "monthDay" || kind === "lastWorkday" || kind === "nthWeekday";

/** Wzorzec cyklu: reguła miesięczna albo dni tygodnia (także „co N tygodni”). */
export const isRecurring = (d: RuleDetails | undefined) =>
  !!d && (monthly(d.repeatRule?.kind) || (d.repeatDays?.length ?? 0) > 0);

/**
 * Porządkuje regułę jak `planner::normalize_rule`: zeruje pola nieużywane przez
 * rodzaj; null dla nieznanego rodzaju albo wartości spoza zakresu.
 */
export function normalizeRule(
  rule: Partial<RepeatRule> | undefined,
): RepeatRule | null {
  const r = { ...emptyRule(), ...rule };
  const base = { ...emptyRule(), kind: r.kind };
  const int = (v: number, min: number, max: number) =>
    Number.isInteger(v) && v >= min && v <= max;
  switch (r.kind) {
    case "":
      return emptyRule();
    case "weeks":
      return int(r.interval, 1, 8) ? { ...base, interval: r.interval } : null;
    case "monthDay":
      return int(r.day, 1, 31) ? { ...base, day: r.day } : null;
    case "lastWorkday":
      return base;
    case "nthWeekday":
      return (int(r.nth, 1, 4) || r.nth === -1) && int(r.weekday, 1, 7)
        ? { ...base, nth: r.nth, weekday: r.weekday }
        : null;
    default:
      return null;
  }
}
/** Reguła miesięczna nie ma dni tygodnia (jak w Rust). */
export const ruleUsesWeekdays = (kind: string | undefined) => !monthly(kind);

const parts = (day: string) => day.split("-").map(Number) as [number, number, number];
const utc = (day: string) => {
  const [y, m, d] = parts(day);
  return Date.UTC(y, m - 1, d);
};
const keyOf = (ms: number) => new Date(ms).toISOString().slice(0, 10);
/** Dzień tygodnia ISO (1 = poniedziałek). */
const weekdayOf = (day: string) => ((new Date(utc(day)).getUTCDay() + 6) % 7) + 1;
const monthLength = (y: number, m: number) => new Date(Date.UTC(y, m, 0)).getUTCDate();

/** Dzień roboczy jak `holidays::is_work_day`. */
export const workDay = (s: WorkSettings, day: string) =>
  s.workDays.includes(weekdayOf(day)) &&
  !(s.holidaysInTarget !== false && holidayName(day));

/** Ostatni dzień roboczy miesiąca albo null (brak dni roboczych). */
export function lastWorkDay(s: WorkSettings, y: number, m: number): string | null {
  for (let d = monthLength(y, m); d >= 1; d--) {
    const day = keyOf(Date.UTC(y, m - 1, d));
    if (workDay(s, day)) return day;
  }
  return null;
}

/** Czy reguła wzorca wypada w dniu `day` przy początku cyklu `from`. */
export function occurs(
  details: RuleDetails,
  from: string,
  day: string,
  s: WorkSettings,
): boolean {
  const rule = { ...emptyRule(), ...details.repeatRule };
  const days = details.repeatDays ?? [];
  const weekday = weekdayOf(day);
  const [y, m, d] = parts(day);
  const last = monthLength(y, m);
  switch (rule.kind) {
    case "weeks": {
      const monday = (k: string) => utc(k) - (weekdayOf(k) - 1) * 86_400_000;
      const weeks = Math.round((monday(day) - monday(from)) / (7 * 86_400_000));
      const n = Math.max(1, rule.interval);
      return days.includes(weekday) && ((weeks % n) + n) % n === 0;
    }
    case "monthDay":
      return rule.day > 0 && d === Math.min(rule.day, last);
    case "lastWorkday":
      return lastWorkDay(s, y, m) === day;
    case "nthWeekday":
      return (
        weekday === rule.weekday &&
        (rule.nth === -1
          ? d + 7 > last
          : rule.nth >= 1 && rule.nth <= 4 && Math.floor((d - 1) / 7) + 1 === rule.nth)
      );
    default:
      return days.includes(weekday);
  }
}

const nthNames: Record<number, string> = {
  1: "1.",
  2: "2.",
  3: "3.",
  4: "4.",
  [-1]: "ostatni",
};
const weekdayNames = [
  "poniedziałek",
  "wtorek",
  "środa",
  "czwartek",
  "piątek",
  "sobota",
  "niedziela",
];
/** Opis reguły po polsku, np. „ostatni piątek miesiąca”. */
export function ruleText(details: RuleDetails): string {
  const rule = { ...emptyRule(), ...details.repeatRule };
  switch (rule.kind) {
    case "weeks":
      return rule.interval > 1
        ? `co ${rule.interval} ${rule.interval < 5 ? "tygodnie" : "tygodni"}`
        : "co tydzień";
    case "monthDay":
      return `${rule.day}. dnia miesiąca`;
    case "lastWorkday":
      return "ostatni dzień roboczy miesiąca";
    case "nthWeekday":
      return `${nthNames[rule.nth] ?? ""} ${weekdayNames[rule.weekday - 1] ?? ""} miesiąca`;
    default:
      return "co tydzień";
  }
}

/* ---------- „Czekam na” ---------- */

/** Pełne dni kalendarzowe od blokady: „od dziś”, „od 1 dnia”, „od 3 dni”. */
export function waitingAge(since: number | undefined, now: number): string {
  if (!since || since <= 0) return "";
  const start = new Date(since);
  const today = new Date(now);
  const days = Math.round(
    (Date.UTC(today.getFullYear(), today.getMonth(), today.getDate()) -
      Date.UTC(start.getFullYear(), start.getMonth(), start.getDate())) /
      86_400_000,
  );
  if (days <= 0) return "od dziś";
  return days === 1 ? "od 1 dnia" : `od ${days} dni`;
}

interface WaitingTask {
  done: boolean;
  jiraKey?: string | null;
  jiraStatus?: string | null;
  details?: {
    waiting?: string;
    waitingStatus?: string;
    waitingDismissed?: string;
  };
}
export interface WaitingProposal {
  /** `block`: oznaczyć jako czekam na; `unblock`: odblokować. */
  action: "block" | "unblock";
  status: string;
}
const fold = (v: string) => v.trim().toLocaleLowerCase("pl");
/** Czy status Jiry jest na liście statusów oznaczających czekanie. */
export const waitingStatus = (statuses: readonly string[], status: string) =>
  statuses.some((s) => fold(s) === fold(status));

/**
 * Propozycja na kafelku zadania z Jiry. Nic nie zmienia się samo:
 * - zadanie bez blokady, a zgłoszenie w statusie czekania → „oznaczyć?”,
 * - blokada zapisana przy statusie czekania, a zgłoszenie już w innym → „odblokować?”.
 * Propozycja odrzucona („Nie teraz”, odblokowanie, start) nie wraca dla tego statusu.
 */
export function waitingProposal(
  task: WaitingTask,
  statuses: readonly string[],
): WaitingProposal | null {
  const status = task.jiraStatus?.trim() ?? "";
  if (task.done || !task.jiraKey || !status) return null;
  const d = task.details ?? {};
  if (d.waitingDismissed && fold(d.waitingDismissed) === fold(status)) return null;
  const inWaiting = waitingStatus(statuses, status);
  if (!d.waiting?.trim()) return inWaiting ? { action: "block", status } : null;
  const from = d.waitingStatus ?? "";
  if (from && waitingStatus(statuses, from) && !inWaiting)
    return { action: "unblock", status };
  return null;
}
