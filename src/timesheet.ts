import type {
  Session,
  Snapshot,
  Task,
  TsProject,
  TsProjectMapping,
} from "./store";
import { byTotal, dayRows, decimalHours, overtime } from "./report";
import { dayBounds, inDay, plural } from "./time";
import { dayTarget, type TargetSettings } from "./workday";

/**
 * Wysyłka Raportu TS do Ideo Timesheet (PLAN_1.8.md): czyste funkcje bez
 * wywołań backendu, testowane jednostkowo.
 */

/** Zadanie TS przypisane zadaniu TukTuk Track i skąd pochodzi. */
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
 * Zadanie TS dla zadania TukTuk Track: jawne `tsTaskId` zadania → domyślne
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

/** Wiersz Raportu TS do wysłania: jedno zadanie TS, scalone zadania TukTuk Track. */
export interface TsSendRow {
  tsTaskId: string;
  tsTaskName: string;
  /** Zadania TukTuk Track scalone w ten wpis, w kolejności raportu. */
  tasks: Task[];
  /** Pełny czas (ms) przed zaokrągleniem. */
  ms: number;
  /**
   * Godziny w normie (proponowany podział). Bez ustawień normy: cały czas jak
   * w raporcie (`decimalHours` po scaleniu, np. „1,50”).
   */
  hours: string;
  /** Nadgodziny (proponowany podział, „1,25”); „0”, gdy brak. */
  overtime: string;
  /** Godziny i nadgodziny razem jak w raporcie („1,50”). */
  total: string;
  /** Notatki dnia (albo tytuły) połączone „; ” bez powtórzeń. */
  description: string;
  /** Koniec ostatniej pracy wpisu w tym dniu (ms): kolejność podziału nadgodzin. */
  lastEnd: number;
}

/** Zadanie z czasem w dniu, którego nie da się wysłać (brak zadania TS). */
export interface TsUnmapped {
  task: Task;
  ms: number;
}

export interface TsEntriesOptions {
  /** Opis z notatki dnia (domyślnie); `false` — zawsze tytuł zadania. */
  includeNotes?: boolean;
  /**
   * Norma dnia (Ustawienia → Czas pracy): z nią wpisy dostają podział na
   * godziny i nadgodziny (`tsSplitOvertime`); bez niej nadgodziny „0”.
   */
  settings?: TargetSettings;
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
  /**
   * Suma godzin wpisów do wysłania razem z nadgodzinami (per wpis po
   * zaokrągleniu, „7,50”) — podstawa znacznika wysyłki.
   */
  totalHours: string;
  /** Norma dnia w setnych godziny (0 w dzień wolny); `null` bez `settings`. */
  norm: number | null;
  /** Proponowane nadgodziny razem (setne godziny). */
  overtime: number;
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
 * notatka dnia zadania, a bez niej tytuł. Z `settings` czas ponad normę dnia
 * trafia do nadgodzin (`tsSplitOvertime`), bez nich nadgodziny „0”. Kolejność:
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
  const { includeNotes = true, settings } = options;
  const rows = dayRows(data, day, now)
    .filter((row) => row.total > 0)
    .sort(byTotal);
  const dayEnd = dayBounds(day)[1];
  const merged = new Map<
    string,
    {
      name: string;
      tasks: Task[];
      sessions: Session[];
      ms: number;
      notes: string[];
      order: number;
      lastEnd: number;
    }
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
      sessions: [],
      ms: 0,
      notes: [],
      order: merged.size,
      lastEnd: 0,
    };
    entry.tasks.push(row.task);
    entry.sessions.push(...row.sessions);
    entry.ms += row.total;
    for (const s of row.sessions)
      if (inDay(s, day, now) > 0)
        entry.lastEnd = Math.max(entry.lastEnd, Math.min(s.endedAt ?? now, dayEnd));
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
      overtime: "0",
      total: decimalHours(e.ms),
      description: e.notes.join("; "),
      lastEnd: e.lastEnd,
    }));
  const sendable = all.filter((e) => tsHundredths(e.total) > 0);
  const tooShort = all.filter((e) => tsHundredths(e.total) <= 0);
  const total = sendable.reduce((sum, e) => sum + tsHundredths(e.total), 0);
  let norm: number | null = null;
  let extra = 0;
  if (settings) {
    // Norma w setnych godziny; nadgodziny z tej samej funkcji co raporty (B5),
    // liczone z pracy wysyłanych zadań. Nie mniej niż nadwyżka zaokrąglonych
    // wpisów ponad normę (suma godzin nigdy jej nie przekracza) i nie więcej
    // niż wszystkie wpisy.
    norm = Math.round(dayTarget(settings, day) / 36_000);
    const sessions = sendable.flatMap((e) => merged.get(e.tsTaskId)!.sessions);
    const measured = Math.round(overtime(day, sessions, settings, now) / 36_000);
    extra = Math.min(total, Math.max(measured, total - norm));
  }
  const entries = tsSplitOvertime(sendable, extra);
  return {
    entries,
    unmapped,
    tooShort,
    totalMs: entries.reduce((sum, e) => sum + e.ms, 0),
    totalHours: tsHoursText(total),
    norm,
    overtime: extra,
    unmappedMs: unmapped.reduce((sum, u) => sum + u.ms, 0),
    running,
  };
}

/** Nadgodziny jako tekst do TS: „0”, gdy brak, inaczej „1,25”. */
export const tsOvertimeText = (hundredths: number) =>
  hundredths > 0 ? tsHoursText(hundredths) : "0";

/**
 * Podział wpisów na godziny i nadgodziny (B4): `overtime` setnych godziny
 * odejmowane od wpisów chronologicznie ostatnich (`lastEnd` malejąco — zadanie
 * kończące dzień ma nadgodziny), każdy wpis najwyżej do swojego czasu.
 * Kolejność wpisów bez zmian.
 */
export function tsSplitOvertime<
  T extends { total: string; lastEnd: number; hours: string; overtime: string },
>(entries: readonly T[], overtime: number): T[] {
  let left = Math.max(0, overtime);
  const extra = new Map<T, number>();
  const latest = [...entries].sort((a, b) => b.lastEnd - a.lastEnd);
  for (const e of latest) {
    const take = Math.min(left, tsHundredths(e.total));
    extra.set(e, take);
    left -= take;
  }
  return entries.map((e) => {
    const over = extra.get(e) ?? 0;
    return {
      ...e,
      hours: tsHoursText(tsHundredths(e.total) - over),
      overtime: tsOvertimeText(over),
    };
  });
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
  /** Liczba wpisów odczytanych z TS po zapisie. */
  count: number;
  /** Suma godzin razem z nadgodzinami odczytana z TS („4,50”). */
  hours: string;
  /** Liczba wpisów albo suma godzin lub nadgodzin (różnica > 0,01 h) inne niż wysłane. */
  differs: boolean;
}

type TsAmounts = { hours: string; overtime?: string };

/**
 * Porównanie odczytu z TS po zapisie z listą faktycznie wysłaną w POST
 * (`sent` z wyniku `ts_send` — przy „Zachowaj” razem z wpisami TS doklejonymi
 * tuż przed zapisem, jak `timesheet::saved_differs`): inna liczba wpisów albo
 * suma godzin lub nadgodzin różna o więcej niż 0,01 h oznacza, że TS zapisał
 * inne wartości.
 */
export function tsCheckSaved(
  sent: readonly TsAmounts[],
  read: readonly TsAmounts[],
): TsSaveCheck {
  const sum = (list: readonly TsAmounts[], key: "hours" | "overtime") =>
    list.reduce((total, e) => total + tsHundredths(e[key] ?? ""), 0);
  const readHours = sum(read, "hours");
  const readOvertime = sum(read, "overtime");
  return {
    count: read.length,
    hours: tsHoursText(readHours + readOvertime),
    differs:
      sent.length !== read.length ||
      Math.abs(sum(sent, "hours") - readHours) > 1 ||
      Math.abs(sum(sent, "overtime") - readOvertime) > 1,
  };
}

/** Suma nadgodzin wpisów (setne godziny). */
export const tsOvertimeSum = (list: readonly TsAmounts[]) =>
  list.reduce((total, e) => total + tsHundredths(e.overtime ?? ""), 0);

/** Liczba ze słowem: `tsPlural(5, "wpis", "wpisy", "wpisów")` → „5 wpisów”. */
export const tsPlural = (n: number, one: string, few: string, many: string) =>
  `${n} ${plural(n, one, few, many)}`;
