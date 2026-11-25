import type { Parsed } from "./quickadd";
import { fold } from "./searchSelect";
import type { Snapshot, Task } from "./store";

/** Najwięcej podpowiedzi pod polem „Nowe zadanie”. */
export const SUGGESTION_LIMIT = 8;

/** Tytuł do porównań: bez wielkości liter, ogonków i nadmiarowych spacji. */
function key(title: string): string {
  return fold(title).replace(/\s+/g, " ").trim();
}

/** Zadania do kontynuacji: nieukończone (zarchiwizowane też, start je przywraca). */
function open(data: Snapshot): Task[] {
  return data.tasks.filter((t) => !t.done);
}

/** Ostatni start pomiaru zadania (ms); trwający pomiar wygrywa. */
function lastWork(data: Snapshot, id: number): number {
  return data.sessions.reduce(
    (latest, s) =>
      s.taskId !== id
        ? latest
        : Math.max(latest, s.endedAt === null ? Infinity : s.startedAt),
    -Infinity,
  );
}

/** Kolejność: ostatnio mierzone wyżej, potem nowsze zadania. */
function byRecentWork(data: Snapshot) {
  return (a: Task, b: Task) => {
    const la = lastWork(data, a.id);
    const lb = lastWork(data, b.id);
    return la === lb ? b.id - a.id : lb > la ? 1 : -1;
  };
}

/**
 * Tytuł, który identyfikuje istniejące zadanie: tylko zwykły tytuł bez
 * zgłoszenia Jiry, Redmine ani szablonu (te tworzą własne powiązania).
 */
function plainTitle(parsed: Parsed): string {
  const f = parsed.fields;
  if (f.jiraKey || f.redmineId != null || f.template) return "";
  return parsed.title.trim();
}

/**
 * Nieukończone zadanie o tej samej nazwie co wpisany tytuł (wielkość liter
 * i ogonki bez znaczenia). Przy kilku: ostatnio mierzone.
 */
export function existingTask(data: Snapshot, parsed: Parsed): Task | null {
  const title = key(plainTitle(parsed));
  if (!title) return null;
  const same = open(data).filter((t) => key(t.title) === title);
  return same.sort(byRecentWork(data))[0] ?? null;
}

/**
 * Podpowiedzi istniejących zadań do wpisanego tytułu: każde słowo musi
 * wystąpić w nazwie. Zadanie o tej samej nazwie na początku.
 */
export function suggestTasks(data: Snapshot, parsed: Parsed): Task[] {
  const title = key(plainTitle(parsed));
  if (!title) return [];
  const words = title.split(" ");
  const exact = (t: Task) => Number(key(t.title) === title);
  const recent = byRecentWork(data);
  return open(data)
    .filter((t) => {
      const name = key(t.title);
      return words.every((w) => name.includes(w));
    })
    .sort((a, b) => exact(b) - exact(a) || recent(a, b))
    .slice(0, SUGGESTION_LIMIT);
}
