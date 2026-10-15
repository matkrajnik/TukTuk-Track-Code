import type { PlanEntry, Task } from "./store";
import { compareTasks } from "./order";
import { isRecurring } from "./recurrence";
import { dayBounds, dayKey } from "./time";
import { isWorkDay } from "./target";

/**
 * Poranny przegląd (PLAN_1.7.md, P2): listy kroków widoku „Przegląd dnia”.
 * Czyste funkcje na zadaniach i ustawieniach; nic tu nie zapisuje ani nie
 * przełącza timera, zmiany planu robią dopiero przyciski widoku.
 */
interface WorkDaySettings {
  readonly targetDayMinutes: number;
  readonly workDays: readonly number[];
  readonly holidaysInTarget?: boolean;
}

/** Dzień przesunięty o `amount` dni (odporne na zmianę czasu). */
export function addDays(day: string, amount: number): string {
  const d = new Date(dayBounds(day)[0]);
  return dayKey(
    new Date(d.getFullYear(), d.getMonth(), d.getDate() + amount).getTime(),
  );
}

/**
 * Najbliższy dzień roboczy przed (`step` -1) albo po (`step` 1) dniu `day`.
 * Bez dni roboczych w ustawieniach (albo w ciągu 3 tygodni): sąsiedni dzień.
 */
function workDayFrom(settings: WorkDaySettings, day: string, step: 1 | -1) {
  for (let i = 1; i <= 21; i++) {
    const candidate = addDays(day, i * step);
    if (isWorkDay(settings, candidate)) return candidate;
  }
  return addDays(day, step);
}
export const previousWorkDay = (settings: WorkDaySettings, day: string) =>
  workDayFrom(settings, day, -1);
export const nextWorkDay = (settings: WorkDaySettings, day: string) =>
  workDayFrom(settings, day, 1);

const open = (t: Task) => !t.done && !t.archived;
/** Wzorzec cyklu: `plannedDay` to tylko początek cyklu, nie plan. */
const recurringSource = (t: Task) => isRecurring(t.details);

/**
 * Krok 1: niedokończone zadania z planów przed dniem `today` (poprzedni dzień
 * roboczy i wcześniejsze zaległości), najnowszy plan pierwszy, potem ręczna
 * kolejność.
 */
export function unfinishedTasks(tasks: readonly Task[], today: string): Task[] {
  const byOrder = compareTasks();
  return tasks
    .filter(
      (t) =>
        open(t) &&
        !recurringSource(t) &&
        !!t.details?.plannedDay &&
        t.details.plannedDay < today,
    )
    .sort(
      (a, b) =>
        b.details!.plannedDay.localeCompare(a.details!.plannedDay) ||
        byOrder(a, b),
    );
}

/** Krok 2: terminy dziś i zaległe (bez zadań z kroku 1), najstarszy pierwszy. */
export function dueTasks(
  tasks: readonly Task[],
  today: string,
  skip: ReadonlySet<number> = new Set(),
): Task[] {
  const byOrder = compareTasks();
  return tasks
    .filter(
      (t) =>
        open(t) &&
        !skip.has(t.id) &&
        !!t.details?.dueDay &&
        t.details.dueDay <= today,
    )
    .sort(
      (a, b) =>
        a.details!.dueDay.localeCompare(b.details!.dueDay) || byOrder(a, b),
    );
}

/**
 * Od kiedy zadanie z Jiry jest „nowe”: chwila ostatniego „Zaczynam dzień”, a bez
 * przeglądu początek poprzedniego dnia roboczego.
 */
export function reviewSince(
  settings: WorkDaySettings & { readonly morningReviewAt: number },
  today: string,
): number {
  return settings.morningReviewAt > 0
    ? settings.morningReviewAt
    : dayBounds(previousWorkDay(settings, today))[0];
}

/**
 * Krok 3: zadania z Jiry (import przypisanych z 1.1) utworzone od `since`,
 * jeszcze niezaplanowane na dziś (bez zadań z kroków 1 i 2), najnowsze pierwsze.
 */
export function newJiraTasks(
  tasks: readonly Task[],
  since: number,
  today: string,
  skip: ReadonlySet<number> = new Set(),
): Task[] {
  return tasks
    .filter(
      (t) =>
        open(t) &&
        !skip.has(t.id) &&
        !!t.jiraKey &&
        t.createdAt > since &&
        t.details?.plannedDay !== today,
    )
    .sort((a, b) => b.createdAt - a.createdAt || a.id - b.id);
}

/** Następna wolna pozycja `order` na końcu planu dnia `day`. */
export function nextOrder(tasks: readonly Task[], day: string): number {
  const orders = tasks
    .filter((t) => t.details?.plannedDay === day)
    .map((t) => t.details?.order ?? 0);
  return orders.length ? Math.min(1_000_000, Math.max(...orders) + 1) : 0;
}

/** „Wszystkie na dziś”: wpisy `planTasks` w kolejności listy, na końcu planu. */
export function planAll(
  tasks: readonly Task[],
  moved: readonly Task[],
  day: string,
): PlanEntry[] {
  const start = nextOrder(tasks, day);
  return moved.map((t, i) => ({
    id: t.id,
    day,
    order: Math.min(1_000_000, start + i),
  }));
}
