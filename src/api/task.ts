/**
 * Zadania, sesje i dane pomocnicze wspólne dla aplikacji i podglądu: typy
 * odpowiadające strukturom Rust i reguły bez dostępu do backendu.
 */
import type { JiraFieldValue } from "./jira";
import { emptyRule, type RepeatRule } from "../recurrence";
import { issueRef } from "../issue";
import type { Parsed } from "../quickadd";
export interface Task {
  id: number;
  title: string;
  done: boolean;
  createdAt: number;
  jiraKey?: string | null;
  jiraUrl?: string | null;
  /** Ostatni znany status zgłoszenia w Jirze. */
  jiraStatus?: string | null;
  /** Wartości pól z Jiry na kafelku (z ustawień `jiraCardFields`, tylko niepuste). */
  jiraFields?: JiraFieldValue[];
  /**
   * Zgłoszenie Redmine (`redmine_tasks`, PLAN_1.9.md R2); zadanie ma najwyżej
   * jedno zgłoszenie: z Jiry albo z Redmine. Opis do kafelków daje `issueRef`.
   */
  redmineId?: number | null;
  redmineUrl?: string | null;
  /** Ostatni znany status, tracker i termin (`RRRR-MM-DD`) zgłoszenia Redmine. */
  redmineStatus?: string | null;
  redmineTracker?: string | null;
  redmineDue?: string | null;
  details?: TaskDetails;
  archived?: boolean;
  /**
   * Chwila ukończenia (ms), tylko dla ukończonych zadań. Brak dla zadań
   * ukończonych przed 2.0: dzień ukończenia liczy wtedy `doneMoments` (`done.ts`).
   */
  doneAt?: number | null;
}
export interface TaskDetails {
  priority: number;
  plannedDay: string;
  dueDay: string;
  project: string;
  waiting: string;
  notes: Record<string, string>;
  estimateMinutes: number;
  checklist: { text: string; done: boolean }[];
  repeatDays: number[];
  repeatFrom: string;
  /** Ręczna kolejność w paśmie priorytetu (etap 3); brak w starych danych = 0. */
  order: number;
  /** Własna godzina startu bloku planu: pusta albo „HH:MM”. */
  plannedStart: string;
  /** Reguła cyklu (P3); brak albo pusty rodzaj = co tydzień w `repeatDays`. */
  repeatRule?: RepeatRule;
  /** Chwila zapisania blokady „Czekam na” (ms, 0 = brak albo stary zapis). */
  waitingSince?: number;
  /** Status Jiry w chwili blokady (propozycja „odblokować?”). */
  waitingStatus?: string;
  /** Status Jiry, dla którego propozycja została odrzucona albo obsłużona. */
  waitingDismissed?: string;
  /** „Przypomnij jutro” z powiadomienia: dzień kolejnego przypomnienia. */
  waitingRemindDay?: string;
  /** „Nie pytaj dla tego zadania” o przesunięcie zgłoszenia w Jirze (J1). */
  jiraNoAsk?: boolean;
  /** Przypięcie do widgetu („w toku”): chwila przypięcia (ms), 0 = nieprzypięte. */
  pinnedAt?: number;
  /**
   * Jawnie wybrane zadanie Ideo TS (1.8); brak = domyślne zadanie projektu
   * (`ts_project_map`) albo żadne. Rozstrzyga `resolveTsTask` w `timesheet.ts`.
   */
  tsTaskId?: string;
  /** Nazwa zadania TS z chwili wyboru (do pokazania bez połączenia). */
  tsTaskName?: string;
}
export const validTime = (value: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
export const detailsOf = (task: Task): TaskDetails => ({
  priority: 0,
  plannedDay: "",
  dueDay: "",
  project: "",
  waiting: "",
  estimateMinutes: 0,
  repeatFrom: "",
  order: 0,
  plannedStart: "",
  waitingSince: 0,
  waitingStatus: "",
  waitingDismissed: "",
  waitingRemindDay: "",
  ...task.details,
  notes: { ...task.details?.notes },
  checklist: (task.details?.checklist ?? []).map((item) => ({ ...item })),
  repeatDays: [...(task.details?.repeatDays ?? [])],
  repeatRule: { ...emptyRule(), ...task.details?.repeatRule },
});
/** Status zgłoszenia zadania (Jira albo Redmine) jak `planner::issue_status`. */
export const issueStatus = (task: Task) => issueRef(task)?.status ?? "";
/**
 * Szczegóły po zdjęciu blokady przy starcie pomiaru (`planner::clear_waiting`):
 * bieżący status zgłoszenia trafia do `waitingDismissed`.
 */
export function unblocked(task: Task): TaskDetails {
  const d = detailsOf(task);
  if (!d.waiting) return d;
  return {
    ...d,
    waiting: "",
    waitingSince: 0,
    waitingStatus: "",
    waitingRemindDay: "",
    waitingDismissed: issueStatus(task),
  };
}
export interface Session {
  id: number;
  taskId: number;
  startedAt: number;
  endedAt: number | null;
}
/**
 * Akcje przy „Luce w zapisie” (`db::GapMode`): dołącz do poprzedniego pomiaru,
 * do następnego albo podziel po pół.
 */
export type GapMode = "previous" | "next" | "split";
/** Luki krótsze niż sekunda (np. 14:58:39 → 14:58:39) nie są pokazywane. */
export const MIN_GAP_MS = 1000;
/** `db::TRASH_IN_GAP`: lukę blokuje tylko przedział zadania w koszu. */
export const TRASH_IN_GAP =
  "W tej luce leży pomiar zadania z kosza. Przywróć zadanie albo usuń je na stałe.";
export interface Snapshot {
  tasks: Task[];
  sessions: Session[];
  trash?: Task[];
  trashSessions?: Session[];
  closedDays?: string[];
  /** Rozpoczęcie, zakończenie i wznowienia dni (`days::DayMark`, B13). */
  dayMarks?: DayMark[];
  recurrenceState?: Record<string, string>;
}
/** Wiersz `day_marks`: „Rozpocznij / Zakończ / Wznów dzień” (B13). */
export interface DayMark {
  day: string;
  startedAt: number | null;
  endedAt: number | null;
  resumed: number;
}
/** `days::MEASURING`: „Zakończ dzień” przy pomiarze bez potwierdzenia. */
export const DAY_MEASURING =
  "Trwa pomiar. Potwierdź zatrzymanie pomiaru, żeby zakończyć dzień.";
/** Ładunek `quick_add` (`quickadd::NewTask` w Rust). */
export interface NewTask {
  title: string;
  jiraKey?: string | null;
  /** Numer zgłoszenia Redmine (`#1234`); razem z `jiraKey` to błąd. */
  redmineId?: number | null;
  details?: TaskDetails | null;
  /** Utwórz i rozpocznij: poprzedni pomiar staje. */
  start?: boolean;
  /** `/nazwa`: szablon; jawne pola z linii wygrywają z szablonem. */
  template?: string | null;
}
/** Ładunek `quick_add` z wyniku parsera: szczegóły tylko, gdy podano pola planu. */
export function quickTask(parsed: Parsed, start = false): NewTask {
  const f = parsed.fields;
  const planned =
    f.estimateMinutes != null ||
    !!f.plannedDay ||
    !!f.dueDay ||
    f.priority != null ||
    !!f.project;
  return {
    title: parsed.title,
    jiraKey: f.jiraKey ?? null,
    redmineId: f.redmineId ?? null,
    details: planned
      ? {
          ...detailsOf({ id: 0, title: "", done: false, createdAt: 0 }),
          estimateMinutes: f.estimateMinutes ?? 0,
          plannedDay: f.plannedDay ?? "",
          dueDay: f.dueDay ?? "",
          priority: f.priority ?? 0,
          project: f.project ?? "",
        }
      : null,
    start,
    template: f.template ?? null,
  };
}
/** `Zadanie może mieć jedno zgłoszenie` (`quickadd::ONE_ISSUE`). */
export const oneIssueError =
  "Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.";
/** Szablon zadania (`templates::Template` w Rust). */
export interface TaskTemplate {
  id: number;
  /** `/nazwa` w szybkim dodawaniu: litery, cyfry, `-` i `_`, bez spacji. */
  name: string;
  /** Tytuł z polami `{klucz}` i `{tytuł}`. */
  title: string;
  estimateMinutes: number;
  steps: string[];
  project: string;
  priority: number;
}
export type TemplateBody = Omit<TaskTemplate, "id" | "name">;
export const templateEvent = "templates-changed";
/**
 * Pola szablonu pod pola z linii (`templates::merge_details`): niezerowe pola
 * z linii wygrywają, kroki szablonu stają się listą kontrolną.
 */
export function mergeTemplate(
  line: TaskDetails | null,
  template: TemplateBody,
): TaskDetails | null {
  const has =
    template.estimateMinutes > 0 ||
    !!template.project ||
    template.priority !== 0 ||
    template.steps.length > 0;
  if (!line && !has) return null;
  const details =
    line ?? detailsOf({ id: 0, title: "", done: false, createdAt: 0 });
  return {
    ...details,
    estimateMinutes: details.estimateMinutes || template.estimateMinutes,
    project: details.project.trim() || template.project,
    priority: details.priority || template.priority,
    checklist: details.checklist.length
      ? details.checklist
      : template.steps.map((text) => ({ text, done: false })),
  };
}
/** Szablon z zadania („Zapisz jako szablon” w rozwiniętym kafelku). */
export function templateFromTask(task: Task): TemplateBody {
  const d = detailsOf(task);
  return {
    // Klucz w tytule zadania z Jiry zamienia się w pole {klucz}.
    title: task.jiraKey
      ? task.title.replaceAll(task.jiraKey, "{klucz}")
      : task.title,
    estimateMinutes: d.estimateMinutes,
    steps: d.checklist.map((item) => item.text),
    project: d.project,
    priority: d.priority,
  };
}
/** Ostatnie zdarzenie hooka agenta do diagnostyki (`hooks::Recent`, B8). */
export interface HookEventRow {
  at: number;
  agent: string;
  event: string;
  /** Początek identyfikatora sesji. */
  session: string;
  cwd: string;
  /** Subagent, typ powiadomienia albo faza po zdarzeniu. */
  detail: string;
}
/** Rejestracja hooków w pliku jednego agenta (`hooks::AgentStatus`). */
export interface AgentHooksFile {
  /** Folder konfiguracji agenta istnieje (agent jest zainstalowany). */
  found: boolean;
  file: string;
  /** `missing`, `stale` albo `current`. */
  registration: string;
  error: string;
}
/** Stan hooków agentów (`hooks::Status`, B8). */
export interface AgentHooksStatus {
  enabled: boolean;
  /** Program todo-track-hook.exe leży obok aplikacji. */
  exeFound: boolean;
  error: string;
  claude: AgentHooksFile;
  codex: AgentHooksFile;
  /** Zatwierdzenie hooków w Codexie (`/hooks`): `none`, `untrusted`, `partial`, `trusted`. */
  codexTrust: string;
  events: HookEventRow[];
}
/** Folder przypisany do zadania (`agents::Binding`, tabela `agent_bindings`). */
export interface AgentBinding {
  folder: string;
  taskId: number;
  title: string;
}
export function duration(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return [
    Math.floor(seconds / 3600),
    Math.floor(seconds / 60) % 60,
    seconds % 60,
  ]
    .map((v) => String(v).padStart(2, "0"))
    .join(":");
}
/** Zmiana blokady „Czekam na” (`planner::WaitingChange`). */
export interface WaitingChange {
  action: "block" | "unblock" | "dismiss" | "remind";
  text?: string;
}
/** Wpis `plan_tasks`: dzień planu i kolejność jednego zadania. */
export interface PlanEntry {
  id: number;
  day: string;
  order: number;
}
/** Wpis `save_day_notes` (`planner::DayNote`): notatka zadania na jeden dzień. */
export interface DayNote {
  id: number;
  /** Pusta (albo z samych spacji) usuwa notatkę dnia. */
  note: string;
}
/** Wynik Start/Stop, odpowiednik `shortcuts::Toggle` w Rust. */
export type ToggleResult =
  | { action: "stopped"; taskId: number; title: string; elapsedMs: number }
  | { action: "started"; taskId: number; title: string }
  /** Dzień zakończony (B13): pomiar nie ruszył, główne okno pyta „Wznowić dzień?”. */
  | { action: "dayEnded"; taskId: number; title: string }
  | { action: "quick" };

/** Najwięcej przypiętych otwartych zadań (`planner::MAX_PINNED`). */
export const maxPinned = 3;
/** Przypięte otwarte zadania w kolejności przypięcia (przyciski w widgecie). */
export function pinnedTasks(data: Snapshot): Task[] {
  const at = (t: Task) => t.details?.pinnedAt ?? 0;
  return data.tasks
    .filter((t) => !t.done && !t.archived && at(t) > 0)
    .sort((a, b) => at(a) - at(b) || a.id - b.id);
}
/** Zadanie do wznowienia: ostatnio mierzone, nieukończone, niezarchiwizowane, nie w koszu. */
export function resumeCandidate(data: Snapshot): Task | undefined {
  const open = new Map(
    data.tasks.filter((t) => !t.done && !t.archived).map((t) => [t.id, t]),
  );
  const last = data.sessions
    .filter((s) => open.has(s.taskId))
    .sort(
      (a, b) =>
        (b.endedAt ?? b.startedAt) - (a.endedAt ?? a.startedAt) ||
        b.startedAt - a.startedAt ||
        b.id - a.id,
    )[0];
  return last && open.get(last.taskId);
}
/** Uruchamianie z Windows: stan wpisu w rejestrze i opcja „schowane do traya”. */
export interface Autostart {
  enabled: boolean;
  hidden: boolean;
}
/** Stan mini-widgetu (`widget::Status` w Rust). */
export interface WidgetStatus {
  enabled: boolean;
  backgroundOnly: boolean;
  /** Schowany przyciskiem X do końca sesji aplikacji; ustawienie się nie zmienia. */
  dismissed: boolean;
}
export const widgetEvent = "widget-state";
export type ExportFormat = "csv" | "xlsx";
/** Wynik eksportu: ścieżka zapisanego pliku albo (w podglądzie) tekst CSV. */
export type ExportResult = { path: string } | { text: string };
/** Stan rozszerzenia przeglądarki (`browser::Status`, W4). */
export interface BrowserStatus {
  enabled: boolean;
  /** Wpisy hosta native messaging są w rejestrze Chrome i Edge. */
  registered: boolean;
  /** Rozszerzenie odezwało się w ostatnich 45 s. */
  connected: boolean;
  lastMessage: number | null;
  /** Ostatni rozpoznany klucz zgłoszenia i kiedy przyszedł. */
  lastKey: string | null;
  lastKeyAt: number | null;
  /** Błąd rejestracji (np. brak todo-track-bridge.exe). */
  error: string;
  extensionId: string;
  /** Folder do „Załaduj rozpakowane”; `null`, gdy nie znaleziono. */
  extensionFolder: string | null;
}
/** Stały identyfikator rozszerzenia (`browser::EXTENSION_ID`). */
export const browserExtensionId = "cecpcpkmmmkgfdklnkkkigofkcibnhbl";
