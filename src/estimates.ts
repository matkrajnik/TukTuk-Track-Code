import type { Snapshot, Task } from "./store";
import { dayBounds, dayKey } from "./time";

/**
 * Trafność szacunków (PLAN_1.0.md, H). Funkcje czyste: liczą wyłącznie ze
 * snapshotu i zegara, bez backendu. Stosunek = czas rzeczywisty / szacunek.
 */

export type EstimatePeriod = "30" | "90" | "all";
export const periods: { value: EstimatePeriod; label: string }[] = [
  { value: "30", label: "30 dni" },
  { value: "90", label: "90 dni" },
  { value: "all", label: "Wszystko" },
];

export interface EstimateRow {
  task: Task;
  estimate: number;
  actual: number;
  ratio: number;
  /** Klucz dnia ostatniej pracy (koniec ostatniego przedziału). */
  lastDay: string;
}

/** Widełki trafności: 80–120% włącznie. */
export const LOW = 0.8;
export const HIGH = 1.2;

/** Sześć koszyków histogramu; górne granice. Środkowy obejmuje 80–120% włącznie. */
export const buckets = [
  { label: "<50%", test: (r: number) => r < 0.5 },
  { label: "50–80%", test: (r: number) => r >= 0.5 && r < LOW },
  { label: "80–120%", test: (r: number) => r >= LOW && r <= HIGH },
  { label: "120–150%", test: (r: number) => r > HIGH && r <= 1.5 },
  { label: "150–200%", test: (r: number) => r > 1.5 && r <= 2 },
  { label: ">200%", test: (r: number) => r > 2 },
];

export const estimateMs = (task: Task) =>
  Math.max(0, task.details?.estimateMinutes ?? 0) * 60_000;

/** Suma czasu i moment końca ostatniego przedziału dla każdego zadania. */
export function taskTotals(
  data: Snapshot,
  now: number,
): Map<number, { total: number; last: number }> {
  const totals = new Map<number, { total: number; last: number }>();
  for (const s of data.sessions) {
    const end = s.endedAt ?? now;
    const row = totals.get(s.taskId) ?? { total: 0, last: 0 };
    row.total += Math.max(0, end - s.startedAt);
    row.last = Math.max(row.last, end);
    totals.set(s.taskId, row);
  }
  return totals;
}

/** Pierwszy dzień okresu: dziś i poprzednie N−1 dni lokalnych; „all” = brak granicy. */
export function periodStart(period: EstimatePeriod, now: number): string {
  if (period === "all") return "";
  const date = new Date(dayBounds(dayKey(now))[0]);
  date.setDate(date.getDate() - Number(period) + 1);
  return dayKey(+date);
}

/**
 * Zbiór do statystyki: ukończone zadania (także zarchiwizowane) z szacunkiem
 * i czasem większym od zera, których ostatni przedział zakończył się w okresie.
 */
export function estimateRows(
  data: Snapshot,
  now: number,
  period: EstimatePeriod,
): EstimateRow[] {
  const totals = taskTotals(data, now);
  const from = periodStart(period, now);
  const rows: EstimateRow[] = [];
  for (const task of data.tasks) {
    const estimate = estimateMs(task);
    const time = totals.get(task.id);
    if (!task.done || !estimate || !time || time.total <= 0) continue;
    const lastDay = dayKey(time.last);
    if (lastDay < from) continue;
    rows.push({
      task,
      estimate,
      actual: time.total,
      ratio: time.total / estimate,
      lastDay,
    });
  }
  return rows;
}

export function median(values: number[]): number {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

export interface EstimateSummary {
  count: number;
  median: number;
  /** Odsetek (0–1) w widełkach ±20%. */
  within: number;
  /** Odsetek (0–1) przekroczeń ponad widełki, czyli powyżej 120%. */
  over: number;
}

export function summarize(rows: EstimateRow[]): EstimateSummary {
  const count = rows.length;
  const share = (test: (r: number) => boolean) =>
    count ? rows.filter((row) => test(row.ratio)).length / count : 0;
  return {
    count,
    median: median(rows.map((row) => row.ratio)),
    within: share((r) => r >= LOW && r <= HIGH),
    over: share((r) => r > HIGH),
  };
}

/** Liczności sześciu koszyków histogramu. */
export const histogram = (rows: EstimateRow[]) =>
  buckets.map((bucket) => rows.filter((row) => bucket.test(row.ratio)).length);

export const NO_PROJECT = "Bez projektu";

/** Mediana stosunku według projektu, alfabetycznie; „Bez projektu” na końcu. */
export function projectMedians(
  rows: EstimateRow[],
): { project: string; count: number; median: number }[] {
  const groups = new Map<string, number[]>();
  for (const row of rows) {
    const project = row.task.details?.project?.trim() || NO_PROJECT;
    groups.set(project, [...(groups.get(project) ?? []), row.ratio]);
  }
  return [...groups]
    .map(([project, ratios]) => ({
      project,
      count: ratios.length,
      median: median(ratios),
    }))
    .sort(
      (a, b) =>
        Number(a.project === NO_PROJECT) - Number(b.project === NO_PROJECT) ||
        a.project.localeCompare(b.project, "pl"),
    );
}

/** Największe rozjazdy według bezwzględnej różnicy czasu, malejąco. */
export const biggestMisses = (rows: EstimateRow[], limit = 10) =>
  [...rows]
    .sort(
      (a, b) =>
        Math.abs(b.actual - b.estimate) - Math.abs(a.actual - a.estimate) ||
        b.task.id - a.task.id,
    )
    .slice(0, limit);

/** Procent wykonania szacunku zaokrąglony do całości, jak na kafelku. */
export const percent = (actual: number, estimate: number) =>
  estimate > 0 ? Math.round((actual / estimate) * 100) : 0;

/** Kafelek ostrzega, gdy wyświetlany procent przekracza 100. */
export const overEstimate = (actual: number, estimate: number) =>
  percent(actual, estimate) > 100;

/**
 * „Zagrożone teraz”: otwarte zadania (nieukończone, niezarchiwizowane), których
 * czas przekroczył szacunek; największy stosunek pierwszy.
 */
export function atRisk(
  data: Snapshot,
  now: number,
): { task: Task; estimate: number; actual: number; ratio: number }[] {
  const totals = taskTotals(data, now);
  return data.tasks
    .filter((task) => !task.done && !task.archived)
    .map((task) => {
      const estimate = estimateMs(task);
      const actual = totals.get(task.id)?.total ?? 0;
      return {
        task,
        estimate,
        actual,
        ratio: estimate ? actual / estimate : 0,
      };
    })
    .filter((row) => row.estimate > 0 && overEstimate(row.actual, row.estimate))
    .sort((a, b) => b.ratio - a.ratio || b.task.id - a.task.id);
}

/** Stosunek jako procent, np. „125%”. */
export const ratioText = (ratio: number) => `${Math.round(ratio * 100)}%`;
/** Odsetek jako procent, np. „40%”. */
export const shareText = (share: number) => `${Math.round(share * 100)}%`;

/**
 * Poprawka szacunków (PLAN_1.7.md, P1): mnożnik z mediany stosunku
 * rzeczywistość/plan ukończonych zadań z szacunkiem z ostatnich 90 dni.
 * Najpierw zadania projektu (co najmniej `CORRECTION_MIN`), inaczej wszystkie
 * (co najmniej `CORRECTION_MIN`), inaczej 1. Mnożnik obcięty do 0,5–3.
 */
export const CORRECTION_MIN = 5;
export const CORRECTION_PERIOD: EstimatePeriod = "90";
export const CORRECTION_LOW = 0.5;
export const CORRECTION_HIGH = 3;

export interface Correction {
  /** Mnożnik szacunku (1, gdy brak danych). */
  factor: number;
  /** Liczba ukończonych zadań, z których pochodzi mnożnik (0 bez danych). */
  count: number;
  /** Źródło: projekt zadania, wszystkie projekty albo brak danych. */
  scope: "project" | "all" | "none";
  /** Nazwa projektu, gdy `scope === "project"`. */
  project: string;
}
export type CorrectionOf = (project?: string | null) => Correction;

const NO_CORRECTION: Correction = {
  factor: 1,
  count: 0,
  scope: "none",
  project: "",
};
const clampFactor = (value: number) =>
  Math.min(CORRECTION_HIGH, Math.max(CORRECTION_LOW, value));
const projectKey = (project?: string | null) =>
  (project ?? "").trim().toLocaleLowerCase("pl");

/**
 * Mnożniki dla wszystkich projektów z jednego przejścia po danych; wynik to
 * funkcja `correction(project)`. Puste `project` = od razu „wszystkie”.
 */
export function corrections(data: Snapshot, now: number): CorrectionOf {
  const rows = estimateRows(data, now, CORRECTION_PERIOD);
  const all: Correction =
    rows.length >= CORRECTION_MIN
      ? {
          factor: clampFactor(median(rows.map((r) => r.ratio))),
          count: rows.length,
          scope: "all",
          project: "",
        }
      : NO_CORRECTION;
  const groups = new Map<string, { name: string; ratios: number[] }>();
  for (const row of rows) {
    const key = projectKey(row.task.details?.project);
    if (!key) continue;
    const group = groups.get(key) ?? {
      name: row.task.details!.project.trim(),
      ratios: [],
    };
    group.ratios.push(row.ratio);
    groups.set(key, group);
  }
  const byProject = new Map<string, Correction>();
  for (const [key, group] of groups)
    if (group.ratios.length >= CORRECTION_MIN)
      byProject.set(key, {
        factor: clampFactor(median(group.ratios)),
        count: group.ratios.length,
        scope: "project",
        project: group.name,
      });
  return (project) => byProject.get(projectKey(project)) ?? all;
}

/** Mnożnik dla projektu (jedno miejsce reguły; `corrections` dla wielu zadań). */
export const correction = (
  project: string | null | undefined,
  data: Snapshot,
  now: number,
): Correction => corrections(data, now)(project);

/** Brak poprawki: każdy projekt ma mnożnik 1. */
export const noCorrection: CorrectionOf = () => NO_CORRECTION;

/** Szacunek z poprawką w minutach, zaokrąglony do 5 min (co najmniej 5). */
export function correctedMinutes(minutes: number, c: Correction): number {
  if (minutes <= 0) return 0;
  return Math.max(5, Math.round((minutes * c.factor) / 5) * 5);
}

/** Czas zadania w planie (ms): szacunek, z poprawką gdy podano `of`; 0 bez szacunku. */
export function plannedMs(task: Task, of?: CorrectionOf): number {
  const minutes = Math.max(0, task.details?.estimateMinutes ?? 0);
  if (!minutes) return 0;
  const factor = of ? of(task.details?.project).factor : 1;
  return Math.round(minutes * factor) * 60_000;
}

const durationLabel = (minutes: number) => {
  const h = Math.floor(minutes / 60),
    m = minutes % 60;
  return [h ? `${h} h` : "", m ? `${m} min` : ""].filter(Boolean).join(" ");
};

/** Podpowiedź „zwykle ~2 h 40 min”; pusta bez szacunku albo bez danych. */
export function usuallyText(minutes: number, c: Correction): string {
  if (minutes <= 0 || c.scope === "none") return "";
  return `zwykle ~${durationLabel(correctedMinutes(minutes, c))}`;
}

/** Opis źródła poprawki (podpowiedź po najechaniu). */
export function correctionTitle(c: Correction): string {
  if (c.scope === "none") return "";
  const factor = c.factor.toLocaleString("pl-PL", {
    maximumFractionDigits: 2,
  });
  const source =
    c.scope === "project"
      ? `projektu ${c.project}`
      : "wszystkich projektów";
  return `Poprawka ×${factor}: mediana rzeczywistość/plan z ${c.count} ukończonych zadań ${source} z 90 dni.`;
}
