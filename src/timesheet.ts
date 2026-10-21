import type { Snapshot, Task, TsProject, TsProjectMapping } from "./store";
import { byTotal, dayRows, decimalHours } from "./report";
import { plural } from "./time";

/**
 * Wysyłka Raportu TS do Ideo Timesheet (PLAN_1.8.md): czyste funkcje bez
 * wywołań backendu, testowane jednostkowo.
 */

/** Zadanie TS przypisane zadaniu Todo Track i skąd pochodzi. */
export interface ResolvedTsTask {
  id: string;
  name: string;
  /** `task` – jawny wybór w zadaniu, `project` – domyślne zadanie projektu. */
  source: "task" | "project";
}

/** Klucz nazwy projektu jak SQLite `COLLATE NOCASE` (tylko litery ASCII). */
export const tsProjectKey = (project: string) =>
  project.trim().replace(/[A-Z]/g, (c) => c.toLowerCase());

/**
 * Zadanie TS dla zadania Todo Track: jawne `tsTaskId` zadania → domyślne
 * zadanie projektu (`ts_project_map`, nazwa projektu bez wielkości liter) →
 * brak (`null`). Mapowanie projektu nie jest kopiowane do zadania, więc jego
 * zmiana działa od razu na wszystkie zadania bez jawnego wyboru.
 */
export function resolveTsTask(
  task: Pick<Task, "details">,
  map: readonly TsProjectMapping[],
): ResolvedTsTask | null {
  const id = task.details?.tsTaskId?.trim() ?? "";
  if (id)
    return { id, name: task.details?.tsTaskName?.trim() || id, source: "task" };
  const project = tsProjectKey(task.details?.project ?? "");
  if (!project) return null;
  const mapped = map.find((m) => tsProjectKey(m.project) === project);
  const mappedId = mapped?.tsTaskId.trim() ?? "";
  if (!mapped || !mappedId) return null;
  return {
    id: mappedId,
    name: mapped.tsTaskName.trim() || mappedId,
    source: "project",
  };
}

/** Zadanie TS o tym identyfikatorze na liście projektów TS (albo `null`). */
export function findTsTask(
  projects: readonly TsProject[],
  id: string,
): { project: string; id: string; name: string } | null {
  for (const p of projects)
    for (const t of p.tasks)
      if (t.id === id) return { project: p.name, id: t.id, name: t.name };
  return null;
}

/** Wiersz Raportu TS do wysłania: jedno zadanie TS, scalone zadania Todo Track. */
export interface TsSendRow {
  tsTaskId: string;
  tsTaskName: string;
  /** Zadania Todo Track scalone w ten wpis, w kolejności raportu. */
  tasks: Task[];
  /** Pełny czas (ms) przed zaokrągleniem. */
  ms: number;
  /** Godziny jak w raporcie (`decimalHours` po scaleniu, np. „1,50”). */
  hours: string;
  overtime: "0";
  /** Notatki dnia (albo tytuły) połączone „; ” bez powtórzeń. */
  description: string;
}

/** Zadanie z czasem w dniu, którego nie da się wysłać (brak zadania TS). */
export interface TsUnmapped {
  task: Task;
  ms: number;
}

export interface TsEntriesOptions {
  /** Opis z notatki dnia (domyślnie); `false` — zawsze tytuł zadania. */
  includeNotes?: boolean;
}

export interface TsEntriesResult {
  entries: TsSendRow[];
  unmapped: TsUnmapped[];
  /**
   * Wpisy z czasem poniżej 0,01 h (po scaleniu „0,00”): nie idą do TS, bo przy
   * „Zachowaj pozostałe wpisy TS” zastąpiłyby wpis TS o tym zadaniu zerem.
   */
  tooShort: TsSendRow[];
  /** Czas wpisów do wysłania (bez zadań bez zadania TS i wpisów „0,00”). */
  totalMs: number;
  /** Suma godzin wpisów do wysłania (per wpis po zaokrągleniu, „7,50”). */
  totalHours: string;
  /** Czas zadań bez zadania TS. */
  unmappedMs: number;
  /** Trwający pomiar wliczony do `now`. */
  running: boolean;
}

/** Białe znaki zbite do jednej spacji, bez spacji na końcach. */
export const tsCleanText = (text: string) => text.replace(/\s+/g, " ").trim();

/**
 * Wpisy TS dnia z Raportu TS (`dayRows`): wiersze z zerowym czasem pominięte,
 * zadanie TS z `resolveTsTask`, wiersze o tym samym zadaniu TS scalone (suma
 * czasu → `decimalHours` po scaleniu, opisy „; ” bez powtórzeń). Opis to
 * notatka dnia zadania, a bez niej tytuł. Nadgodziny zawsze „0”. Kolejność:
 * najwięcej czasu, potem kolejność pierwszego zadania w raporcie. Wpisy, które
 * po scaleniu dają „0,00” h, trafiają do `tooShort` zamiast do wysyłki.
 */
export function tsEntries(
  day: string,
  data: Snapshot,
  now: number,
  map: readonly TsProjectMapping[],
  options: TsEntriesOptions = {},
): TsEntriesResult {
  const { includeNotes = true } = options;
  const rows = dayRows(data, day, now)
    .filter((row) => row.total > 0)
    .sort(byTotal);
  const merged = new Map<
    string,
    { name: string; tasks: Task[]; ms: number; notes: string[]; order: number }
  >();
  const unmapped: TsUnmapped[] = [];
  let running = false;
  for (const row of rows) {
    if (row.sessions.some((s) => s.endedAt === null)) running = true;
    const resolved = resolveTsTask(row.task, map);
    if (!resolved) {
      unmapped.push({ task: row.task, ms: row.total });
      continue;
    }
    const entry = merged.get(resolved.id) ?? {
      name: resolved.name,
      tasks: [],
      ms: 0,
      notes: [],
      order: merged.size,
    };
    entry.tasks.push(row.task);
    entry.ms += row.total;
    const note = includeNotes
      ? tsCleanText(row.task.details?.notes?.[day] ?? "")
      : "";
    const text = note || tsCleanText(row.task.title);
    if (text && !entry.notes.includes(text)) entry.notes.push(text);
    merged.set(resolved.id, entry);
  }
  const all: TsSendRow[] = [...merged.entries()]
    .sort(([, a], [, b]) => b.ms - a.ms || a.order - b.order)
    .map(([id, e]) => ({
      tsTaskId: id,
      tsTaskName: e.name,
      tasks: e.tasks,
      ms: e.ms,
      hours: decimalHours(e.ms),
      overtime: "0" as const,
      description: e.notes.join("; "),
    }));
  const entries = all.filter((e) => tsHundredths(e.hours) > 0);
  const tooShort = all.filter((e) => tsHundredths(e.hours) <= 0);
  return {
    entries,
    unmapped,
    tooShort,
    totalMs: entries.reduce((sum, e) => sum + e.ms, 0),
    totalHours: tsHoursText(
      entries.reduce((sum, e) => sum + tsHundredths(e.hours), 0),
    ),
    unmappedMs: unmapped.reduce((sum, u) => sum + u.ms, 0),
    running,
  };
}

/** Godziny z TS w setnych („1,5”, „1.25”); nieczytelne liczą się jako 0. */
export function tsHundredths(hours: string): number {
  const raw = hours.trim();
  const value = Number(raw.replace(",", "."));
  return raw && Number.isFinite(value) ? Math.round(value * 100) : 0;
}

/** Setne części godziny jako „7,50”. */
export const tsHoursText = (hundredths: number) =>
  `${Math.trunc(hundredths / 100)},${String(Math.abs(hundredths % 100)).padStart(2, "0")}`;

export interface TsSaveCheck {
  /** Liczba wpisów i suma godzin odczytane z TS po zapisie. */
  count: number;
  hours: string;
  /** Liczba wpisów albo suma (różnica > 0,01 h) inne niż wysłane. */
  differs: boolean;
}

/**
 * Porównanie odczytu z TS po zapisie z tym, co poszło w POST (`sent` — przy
 * „Zachowaj” razem z doklejonymi wpisami TS): inna liczba wpisów albo suma
 * godzin różna o więcej niż 0,01 h oznacza, że TS zapisał inne wartości.
 */
export function tsCheckSaved(
  sent: readonly { hours: string }[],
  read: readonly { hours: string }[],
): TsSaveCheck {
  const sum = (list: readonly { hours: string }[]) =>
    list.reduce((total, e) => total + tsHundredths(e.hours), 0);
  const readSum = sum(read);
  return {
    count: read.length,
    hours: tsHoursText(readSum),
    differs: sent.length !== read.length || Math.abs(sum(sent) - readSum) > 1,
  };
}

/** Liczba ze słowem: `tsPlural(5, "wpis", "wpisy", "wpisów")` → „5 wpisów”. */
export const tsPlural = (n: number, one: string, few: string, many: string) =>
  `${n} ${plural(n, one, few, many)}`;
