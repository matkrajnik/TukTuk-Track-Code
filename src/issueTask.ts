/**
 * Lokalne zadanie zgłoszenia na listach Jiry i Redmine (B5): czy „Rozpocznij”
 * uruchomi istniejące zadanie, przywróci je z archiwum lub kosza, czy utworzy
 * nowe. Backend (`jira::start_issue`, `redmine::start_issue`) zawsze bierze
 * zadanie już powiązane ze zgłoszeniem — także zarchiwizowane i z kosza — więc
 * duplikat nie powstaje; lista mówi to wprost.
 */
import type { Session, Snapshot, Task } from "./api/task";
import { dayKey, hhmm, inDay } from "./time";

export type IssueTaskState =
  | { kind: "task"; task: Task; today: number }
  | { kind: "archived"; task: Task }
  | { kind: "trash"; task: Task };

/**
 * Stan lokalnego zadania dla każdego zgłoszenia: `key(task)` daje klucz
 * zgłoszenia zadania (np. `jiraKey`) albo nic. Pierwszeństwo: zadanie na
 * liście, potem zarchiwizowane, potem w koszu. `today` to czas zadania dziś
 * (podział przez lokalną północ, trwający pomiar do `now`).
 */
export function issueTasks<K>(
  data: Snapshot | undefined,
  key: (task: Task) => K | null | undefined,
  now: number,
): Map<K, IssueTaskState> {
  const out = new Map<K, IssueTaskState>();
  if (!data) return out;
  const today = dayKey(now);
  const byTask = new Map<number, Session[]>();
  for (const s of data.sessions) {
    const list = byTask.get(s.taskId);
    if (list) list.push(s);
    else byTask.set(s.taskId, [s]);
  }
  const rank = { task: 0, archived: 1, trash: 2 } as const;
  const put = (k: K, state: IssueTaskState) => {
    const current = out.get(k);
    if (!current || rank[state.kind] < rank[current.kind]) out.set(k, state);
  };
  for (const task of data.tasks) {
    const k = key(task);
    if (k === null || k === undefined) continue;
    if (task.archived) put(k, { kind: "archived", task });
    else
      put(k, {
        kind: "task",
        task,
        today: (byTask.get(task.id) ?? []).reduce(
          (sum, s) => sum + inDay(s, today, now),
          0,
        ),
      });
  }
  for (const task of data.trash ?? []) {
    const k = key(task);
    if (k !== null && k !== undefined) put(k, { kind: "trash", task });
  }
  return out;
}

/** Znacznik zgłoszenia z lokalnym zadaniem: „W zadaniach · dziś 01:20”. */
export const issueTaskMarker = (state: IssueTaskState & { kind: "task" }) =>
  `W zadaniach${state.task.done ? " (ukończone)" : ""} · dziś ${hhmm(state.today)}`;

/** Ostrzeżenie przy zadaniu w archiwum albo w koszu. */
export const issueTaskWarning = (state: IssueTaskState) =>
  state.kind === "archived"
    ? "Zadanie jest w archiwum — „Rozpocznij” przywróci je zamiast tworzyć duplikat."
    : state.kind === "trash"
      ? "Zadanie jest w koszu — „Rozpocznij” przywróci je z historią zamiast tworzyć duplikat."
      : "";

/** Podpowiedź przycisku „Rozpocznij”: co się stanie po kliknięciu. */
export function issueStartTitle(state: IssueTaskState | undefined): string {
  if (!state) return "Utwórz nowe zadanie i rozpocznij pomiar";
  if (state.kind === "task")
    return `Rozpocznij pomiar istniejącego zadania „${state.task.title}”`;
  return state.kind === "archived"
    ? "Przywróć zadanie z archiwum i rozpocznij pomiar"
    : "Przywróć zadanie z kosza i rozpocznij pomiar";
}
