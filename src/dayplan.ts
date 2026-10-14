import type { Session, Task } from "./store";
import { compareTasks } from "./order";
import { dayBounds, inDay } from "./time";

/** Liczba odcieni palety osi dnia (tokeny `--tl-0` … `--tl-7` w base.css). */
export const paletteSize = 8;
/** Długość bloku planu dla zadania bez szacunku. */
export const defaultPlanMinutes = 30;
const hour = 3_600_000;

/** Deterministyczny indeks koloru zadania (hash multiplikatywny Knutha). */
export function colorIndex(id: number): number {
  return (Math.imul(id >>> 0, 2654435761) >>> 0) % paletteSize;
}
export const colorVar = (id: number) => `var(--tl-${colorIndex(id)})`;

/**
 * Lokalny znacznik czasu godziny „HH:MM” danego dnia. Liczony przez `Date`
 * z polami lokalnymi, więc w dniu zmiany czasu wskazuje właściwą chwilę.
 */
export function atTime(day: string, time: string): number | null {
  const match = /^([01]\d|2[0-3]):([0-5]\d)$/.exec(time);
  if (!match) return null;
  const [year, month, date] = day.split("-").map(Number);
  return new Date(
    year,
    month - 1,
    date,
    Number(match[1]),
    Number(match[2]),
  ).getTime();
}

export interface PlanBlock {
  task: Task;
  start: number;
  end: number;
  /** Brak szacunku: blok ma domyślne 30 min i jest rysowany kreskowany. */
  estimated: boolean;
  /** Zadanie ma własną godzinę startu (`plannedStart`). */
  pinned: boolean;
}

/** Zadania planu dnia w ręcznej kolejności (priorytet, `order`, id). */
export function plannedTasks(tasks: Task[], day: string): Task[] {
  return tasks
    .filter((t) => !t.archived && t.details?.plannedDay === day)
    .sort(compareTasks());
}

/**
 * Bloki toru „Plan”: kolejno od `dayStart`, długość z szacunku (bez szacunku
 * 30 min). Zadanie z `plannedStart` jest przypięte do swojej godziny, a kolejne
 * układają się za nim.
 */
export function planBlocks(
  tasks: Task[],
  day: string,
  dayStart: string,
): PlanBlock[] {
  const [, dayEnd] = dayBounds(day);
  let cursor = atTime(day, dayStart) ?? atTime(day, "08:00")!;
  return plannedTasks(tasks, day).map((task) => {
    const pinnedAt = atTime(day, task.details?.plannedStart ?? "");
    const minutes = task.details?.estimateMinutes ?? 0;
    const start = pinnedAt ?? cursor;
    const end = Math.min(
      dayEnd,
      start + (minutes > 0 ? minutes : defaultPlanMinutes) * 60_000,
    );
    cursor = end;
    return {
      task,
      start,
      end,
      estimated: minutes > 0,
      pinned: pinnedAt !== null,
    };
  });
}

export interface WorkBlock {
  session: Session;
  start: number;
  end: number;
  running: boolean;
}

/** Bloki toru „Rzeczywistość”: przedziały przycięte do dnia przez `inDay`. */
export function workBlocks(
  sessions: Session[],
  day: string,
  now: number,
): WorkBlock[] {
  const [dayStartMs, dayEnd] = dayBounds(day);
  return sessions
    .filter((s) => inDay(s, day, now) > 0)
    .map((s) => ({
      session: s,
      start: Math.max(dayStartMs, s.startedAt),
      end: Math.min(dayEnd, s.endedAt ?? now),
      running: s.endedAt === null,
    }))
    .sort((a, b) => a.start - b.start);
}

// Odejmowanie minut i sekund zamiast setMinutes: w dobie zmiany czasu
// setMinutes na niejednoznacznej godzinie 02:00 wraca do pierwszego wystąpienia.
const floorHour = (ms: number) => {
  const d = new Date(ms);
  return (
    ms - d.getMinutes() * 60_000 - d.getSeconds() * 1000 - d.getMilliseconds()
  );
};
const ceilHour = (ms: number) => {
  const floored = floorHour(ms);
  return floored === ms ? ms : floored + hour;
};

/**
 * Zakres osi: od min(`dayStart`, pierwszy pomiar) do max(17:00, ostatni pomiar,
 * koniec ostatniego bloku planu), zaokrąglony do pełnych godzin i przycięty do doby.
 */
export function axisRange(
  day: string,
  dayStart: string,
  plan: PlanBlock[],
  work: WorkBlock[],
): [number, number] {
  const [dayStartMs, dayEnd] = dayBounds(day);
  const start = Math.min(
    atTime(day, dayStart) ?? atTime(day, "08:00")!,
    ...plan.map((b) => b.start),
    ...work.map((b) => b.start),
  );
  const end = Math.max(
    atTime(day, "17:00")!,
    ...plan.map((b) => b.end),
    ...work.map((b) => b.end),
  );
  const from = Math.max(dayStartMs, floorHour(start));
  return [from, Math.max(from + hour, Math.min(dayEnd, ceilHour(end)))];
}

/** Pełne godziny osi (znaczniki i etykiety), także w dobie 23- i 25-godzinnej. */
export function hourTicks(from: number, to: number): number[] {
  const ticks: number[] = [];
  for (let t = floorHour(from); t <= to; t += hour) ticks.push(t);
  return ticks;
}

export type ComparisonStatus = "planned" | "unplanned" | "unstarted";
export interface ComparisonRow {
  task: Task;
  /** Plan w ms z szacunku; 0, gdy zadanie nie ma szacunku albo planu. */
  plan: number;
  actual: number;
  status: ComparisonStatus;
  /** Zadanie z planu dnia, ale bez szacunku: różnicy nie da się policzyć. */
  noEstimate: boolean;
}

/**
 * Tabela różnic: najpierw zadania planu w kolejności bloków, potem praca spoza
 * planu malejąco według czasu. „niezaczęte” = plan bez pracy w tym dniu.
 */
export function comparison(
  tasks: Task[],
  sessions: Session[],
  day: string,
  now: number,
  plan: PlanBlock[],
): ComparisonRow[] {
  const actual = new Map<number, number>();
  sessions.forEach((s) => {
    const part = inDay(s, day, now);
    if (part > 0) actual.set(s.taskId, (actual.get(s.taskId) ?? 0) + part);
  });
  const planned = new Set(plan.map((b) => b.task.id));
  const rows: ComparisonRow[] = plan.map(({ task, estimated }) => {
    const spent = actual.get(task.id) ?? 0;
    return {
      task,
      plan: estimated ? (task.details?.estimateMinutes ?? 0) * 60_000 : 0,
      actual: spent,
      status: spent > 0 ? "planned" : "unstarted",
      noEstimate: !estimated,
    };
  });
  const byId = new Map(tasks.map((t) => [t.id, t]));
  [...actual]
    .filter(([id]) => !planned.has(id) && byId.has(id))
    .sort((a, b) => b[1] - a[1] || a[0] - b[0])
    .forEach(([id, spent]) =>
      rows.push({
        task: byId.get(id)!,
        plan: 0,
        actual: spent,
        status: "unplanned",
        noEstimate: false,
      }),
    );
  return rows;
}
