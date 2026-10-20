import { invoke, isTauri } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { ref } from "vue";
import { renderTitle, unknownTitle, type Parsed } from "./quickadd";
import { exportCsv, exportRows } from "./export";
import { agentTotals, type AgentPeriod, type AgentTime } from "./agenttime";
import { tsProjectKey } from "./timesheet";
import { issueRef, redmineKey, validRedmineId } from "./issue";
import {
  emptyRule,
  isRecurring,
  normalizeRule,
  occurs,
  ruleUsesWeekdays,
  type RepeatRule,
} from "./recurrence";
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
}
/** Wartość pola Jiry na kafelku (`jira_fields::FieldValue`). */
export interface JiraFieldValue {
  id: string;
  name: string;
  value: string;
}
/** Pole z listy pól Jiry (`jira_fields::FieldInfo`). */
export interface JiraFieldInfo {
  id: string;
  name: string;
  custom: boolean;
  kind: string;
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
const validTime = (value: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
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
const issueStatus = (task: Task) => issueRef(task)?.status ?? "";
/**
 * Szczegóły po zdjęciu blokady przy starcie pomiaru (`planner::clear_waiting`):
 * bieżący status zgłoszenia trafia do `waitingDismissed`.
 */
function unblocked(task: Task): TaskDetails {
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
export interface Snapshot {
  tasks: Task[];
  sessions: Session[];
  trash?: Task[];
  trashSessions?: Session[];
  closedDays?: string[];
  recurrenceState?: Record<string, string>;
}
export const desktop = isTauri();
/**
 * Czy Jira jest połączona: tylko wtedy szybkie dodawanie rozpoznaje klucz
 * zgłoszenia. W podglądzie Jira udaje połączoną (dane przykładowe mają zgłoszenia,
 * a `quickAdd` wiąże klucz z adresem domyślnym).
 */
export const jiraConnected = ref(!desktop);
/** Odświeża `jiraConnected` z `jira_settings` (w podglądzie nic nie robi). */
export async function refreshJiraConnected(): Promise<void> {
  if (!desktop) return;
  try {
    jiraConnected.value = (
      await invoke<{ connected: boolean }>("jira_settings")
    ).connected;
  } catch {
    jiraConnected.value = false;
  }
}
/**
 * Czy Redmine jest połączone: tylko wtedy szybkie dodawanie rozpoznaje `#1234`
 * jako zgłoszenie (bez połączenia to projekt „1234”). W podglądzie symulowane
 * Redmine jest połączone, chyba że stan w `previewRedmineKey` mówi inaczej.
 */
export const redmineConnected = ref(!desktop);
/** Odświeża `redmineConnected` z `redmine_settings` (w podglądzie ze stanu symulacji). */
export async function refreshRedmineConnected(): Promise<void> {
  if (!desktop) {
    redmineConnected.value = previewRedmineState().connected !== false;
    return;
  }
  try {
    redmineConnected.value = (
      await invoke<{ connected: boolean }>("redmine_settings")
    ).connected;
  } catch {
    redmineConnected.value = false;
  }
}
const key = "todo-track-preview-v1";
export async function read(): Promise<Snapshot> {
  if (desktop) return invoke("get_snapshot");
  const raw = localStorage.getItem(key);
  const data: Snapshot = raw ? JSON.parse(raw) : { tasks: [], sessions: [] };
  if (generateRecurring(data)) localStorage.setItem(key, JSON.stringify(data));
  attachPreviewJiraFields(data);
  return data;
}
export async function change(
  action: string,
  id: number | null = null,
  title: string | null = null,
): Promise<Snapshot> {
  if (desktop) return invoke("mutate", { action, id, title });
  const data = await read();
  const now = Date.now();
  const task = data.tasks.find((t) => t.id === id);
  const stop = () =>
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
  if (["rename", "toggle", "delete", "start"].includes(action) && !task)
    throw new Error("Nie znaleziono zadania.");
  if (action === "add" || action === "rename") {
    title = title?.trim() ?? "";
    if (!title || [...title].length > 240)
      throw new Error("Tytuł musi mieć od 1 do 240 znaków.");
    if (task) task.title = title;
    else
      data.tasks.unshift({
        id:
          Math.max(
            0,
            ...data.tasks.map((t) => t.id),
            ...(data.trash ?? []).map((t) => t.id),
          ) + 1,
        title,
        done: false,
        createdAt: now,
      });
  } else if (action === "start" && task) {
    if (task.done) throw new Error("Najpierw przywróć zadanie do zrobienia.");
    task.details = unblocked(task);
    task.archived = false;
    if (!data.sessions.some((s) => s.taskId === id && s.endedAt === null)) {
      stop();
      data.sessions.unshift({
        id:
          Math.max(
            0,
            ...data.sessions.map((s) => s.id),
            ...(data.trashSessions ?? []).map((s) => s.id),
          ) + 1,
        taskId: task.id,
        startedAt: now,
        endedAt: null,
      });
    }
  } else if (action === "stop") stop();
  else if (action === "toggle" && task) {
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(now, s.startedAt);
    });
    task.done = !task.done;
  } else if (action === "delete") {
    if (task) {
      data.sessions.forEach((s) => {
        if (s.taskId === id && s.endedAt === null)
          s.endedAt = Math.max(s.startedAt, now);
      });
      (data.trash ??= []).push(task);
      (data.trashSessions ??= []).push(
        ...data.sessions.filter((s) => s.taskId === id),
      );
    }
    data.tasks = data.tasks.filter((t) => t.id !== id);
    data.sessions = data.sessions.filter((s) => s.taskId !== id);
  } else if (action === "restore") {
    const restored = data.trash?.find((t) => t.id === id);
    if (!restored) throw Error("Nie znaleziono zadania w koszu.");
    data.tasks.unshift(restored);
    data.trash = data.trash?.filter((t) => t.id !== id);
    data.sessions.push(
      ...(data.trashSessions ?? []).filter((s) => s.taskId === id),
    );
    data.trashSessions = data.trashSessions?.filter((s) => s.taskId !== id);
  } else if ((action === "archive" || action === "unarchive") && task) {
    task.archived = action === "archive";
    if (task.archived)
      data.sessions.forEach((s) => {
        if (s.taskId === id && s.endedAt === null)
          s.endedAt = Math.max(now, s.startedAt);
      });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
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
/** Adres Jiry w podglądzie (domyślny z `jira::Config`). */
const previewJiraSite = "https://asistonwms.atlassian.net";
/** Adres symulowanego Redmine w podglądzie (przykładowy, PLAN_1.9.md). */
export const previewRedmineSite = "https://redmine.example.pl";
/**
 * Symulowane Redmine podglądu (PLAN_1.9.md, R6): stan `{ connected, site,
 * login, displayName, acceptInvalidCert, fail, write, statuses, projects,
 * selectedProjects, issues }` w `localStorage`; każde brakujące pole ma wartość
 * z przykładu (`previewRedmine*`). Brak klucza = połączone z przykładowymi
 * zgłoszeniami. Klucz API „zly” w „Połącz” daje błąd 401. Inne okno albo test
 * zmieniające ten klucz wywołują `onRedmineChanged` i `onMovePromptsChanged`.
 */
export const previewRedmineKey = "todo-track-preview-redmine-v1";
/**
 * Zgłoszenie symulowanego Redmine (podzbiór `redmine::Issue`). Pola widoku
 * Redmine (R3) są opcjonalne: brak projektu = projekt śledzony z przykładu,
 * brak osoby = przypisane do mnie.
 */
export interface PreviewRedmineIssue {
  id: number;
  subject: string;
  status: string;
  tracker: string;
  dueDate: string;
  projectId?: number;
  priority?: string;
  /** `null` = nieprzypisane; brak pola = przypisane do mnie. */
  assignedToId?: number | null;
  assignedTo?: string;
  version?: string;
  category?: string;
  updatedOn?: string;
}
interface PreviewRedmine {
  connected?: boolean;
  /** Adres z „Połącz” (R5); brak = `previewRedmineSite`. */
  site?: string;
  /** Login i imię i nazwisko z `/users/current.json`; brak = `previewRedmineMe`. */
  login?: string;
  displayName?: string;
  /**
   * `false` = symulowany backend odrzuca zapisy jak przy wyłączonym
   * `redmine_write` (`redmine::ensure_write`), niezależnie od przełącznika w
   * Ustawieniach podglądu; brak = decyduje ustawienie `redmineWrite`.
   */
  write?: boolean;
  /** „Akceptuj niezaufany certyfikat” zapamiętane w konfiguracji (R5). */
  acceptInvalidCert?: boolean;
  /** Serwer z certyfikatem spoza magazynu Windows (testy R5). */
  certError?: boolean;
  /** Błąd „Testuj połączenie” (testy R5). */
  failTest?: string;
  issues?: PreviewRedmineIssue[];
  /** Wszystkie projekty (`redmine_projects`). */
  projects?: RedmineProject[];
  /** Śledzone projekty (`redmine_select_projects`); brak = pierwszy z przykładu. */
  selectedProjects?: RedmineProject[];
  statuses?: RedmineStatus[];
  /** Błąd zapytania o moje zgłoszenia ze wszystkich projektów (testy). */
  failMine?: string;
  /** Odrzucenie zapisu przez symulowany Redmine: 403 albo 422 (R4, testy). */
  fail?: number;
  /** Redmine przyjmuje zmianę statusu, ale go nie zmienia (R4, testy). */
  unchanged?: boolean;
  /** Komentarze wysłane do zgłoszeń (`notes`), po numerze. */
  comments?: Record<string, string[]>;
}
/** Użytkownik symulowanego Redmine (`/users/current.json`). */
export const previewRedmineMe = Object.freeze({
  id: 12,
  login: "jkowalski",
  name: "Jan Kowalski",
});
/** Statusy symulowanego Redmine w kolejności z `/issue_statuses.json`. */
export const previewRedmineStatuses: readonly RedmineStatus[] = Object.freeze([
  { id: 1, name: "New", isClosed: false },
  { id: 2, name: "In Progress", isClosed: false },
  { id: 3, name: "Feedback", isClosed: false },
  { id: 4, name: "Resolved", isClosed: true },
  { id: 5, name: "Closed", isClosed: true },
]);
/** Projekty symulowanego Redmine: Magazyn WMS z podprojektem i Portal klienta. */
export const previewRedmineProjects: readonly RedmineProject[] = Object.freeze([
  { id: 7, name: "Magazyn WMS", identifier: "wms", parentId: null },
  { id: 8, name: "WMS – integracje", identifier: "wms-integracje", parentId: 7 },
  { id: 9, name: "Portal klienta", identifier: "portal", parentId: null },
]);
/**
 * Przykładowe zgłoszenia (tematy do `#1234` w szybkim dodawaniu, lista widoku
 * Redmine): w śledzonym Magazynie WMS cztery moje w trzech statusach (jedno z
 * terminem), jedno Anny Nowak i jedno zamknięte; jedno w podprojekcie; jedno
 * moje w nieśledzonym Portalu klienta.
 */
export const previewRedmineIssues: readonly PreviewRedmineIssue[] = Object.freeze([
  {
    id: 1234,
    subject: "Import CSV zamówień",
    status: "New",
    tracker: "Funkcja",
    dueDate: "2026-10-30",
    projectId: 7,
    priority: "High",
    version: "2.4",
    updatedOn: "2026-10-14T09:00:00Z",
  },
  {
    id: 1240,
    subject: "Błąd zapisu etykiet",
    status: "Feedback",
    tracker: "Błąd",
    dueDate: "",
    projectId: 7,
    priority: "Normal",
    updatedOn: "2026-10-13T09:00:00Z",
  },
  {
    id: 1250,
    subject: "Aktualizacja dokumentacji API",
    status: "In Progress",
    tracker: "Wsparcie",
    dueDate: "",
    projectId: 7,
    priority: "Low",
    updatedOn: "2026-10-12T09:00:00Z",
  },
  {
    id: 1251,
    subject: "Raport stanów magazynowych",
    status: "New",
    tracker: "Funkcja",
    dueDate: "",
    projectId: 7,
    priority: "Normal",
    assignedToId: 31,
    assignedTo: "Anna Nowak",
    updatedOn: "2026-10-11T09:00:00Z",
  },
  {
    id: 1245,
    subject: "Eksport faktur do ERP",
    status: "Closed",
    tracker: "Błąd",
    dueDate: "",
    projectId: 7,
    priority: "Normal",
    updatedOn: "2026-10-10T09:00:00Z",
  },
  {
    id: 1260,
    subject: "Mapowanie kodów EDI",
    status: "In Progress",
    tracker: "Funkcja",
    dueDate: "",
    projectId: 8,
    priority: "Normal",
    updatedOn: "2026-10-09T09:00:00Z",
  },
  {
    id: 1270,
    subject: "Formularz reklamacji",
    status: "New",
    tracker: "Błąd",
    dueDate: "",
    projectId: 9,
    priority: "Urgent",
    updatedOn: "2026-10-08T09:00:00Z",
  },
]);
function previewRedmineState(): PreviewRedmine {
  return previewJson(previewRedmineKey) as PreviewRedmine;
}
function savePreviewRedmine(state: PreviewRedmine) {
  localStorage.setItem(previewRedmineKey, JSON.stringify(state));
}
/** Użytkownik symulowanego Redmine: login i nazwa ze stanu albo z przykładu. */
function previewRedmineUser(state: PreviewRedmine) {
  const login =
    typeof state.login === "string" && state.login.trim()
      ? state.login.trim()
      : previewRedmineMe.login;
  const name =
    typeof state.displayName === "string" && state.displayName.trim()
      ? state.displayName.trim()
      : login === previewRedmineMe.login
        ? previewRedmineMe.name
        : login;
  return { id: previewRedmineMe.id, login, name };
}
/** Zgłoszenie symulowanego Redmine po numerze (ze stanu albo z listy przykładowej). */
function previewRedmineIssue(id: number): PreviewRedmineIssue | undefined {
  const state = previewRedmineState();
  const list = Array.isArray(state.issues) ? state.issues : previewRedmineIssues;
  return list.find((i) => i.id === id);
}
/** `Zadanie może mieć jedno zgłoszenie` (`quickadd::ONE_ISSUE`). */
export const oneIssueError =
  "Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.";
/**
 * Szybkie dodawanie ze składnią: zadanie z polami, kluczem Jiry albo numerem
 * Redmine i opcjonalnym startem w jednym kroku. Odpowiednik
 * `quickadd::create_linked`; w podglądzie nie ma listy zgłoszeń Jiry, więc pusty
 * tytuł z kluczem bierze tytuł z danych `jira_issues` tylko w aplikacji; temat
 * `#1234` pochodzi z symulowanego Redmine (`previewRedmineIssues`).
 */
export async function quickAdd(task: NewTask): Promise<Snapshot> {
  if (desktop) return invoke("quick_add", { task });
  const jiraKey = task.jiraKey?.trim() || null;
  if (jiraKey && !/^[A-Z0-9_]+-\d+$/.test(jiraKey))
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
  const redmineId = task.redmineId ?? null;
  if (redmineId !== null && !validRedmineId(redmineId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  if (jiraKey && redmineId !== null) throw new Error(oneIssueError);
  if (redmineId !== null && previewRedmineState().connected === false)
    throw new Error(
      `Redmine nie jest połączone, więc #${redmineId} nie zostanie powiązane. Połącz konto w Ustawieniach → Redmine albo poprzedź # znakiem \\.`,
    );
  const redmineIssue =
    redmineId !== null ? previewRedmineIssue(redmineId) : undefined;
  const name = task.template?.trim();
  const template = name ? findTemplate(previewTemplates(), name) : undefined;
  if (name && !template)
    throw new Error(
      `Nie ma szablonu „${name}”. Dodaj go w Ustawieniach → Szablony.`,
    );
  // Etykieta zgłoszenia do szablonu: klucz Jiry albo `#numer`; temat tylko z
  // symulowanego Redmine (listy Jiry w podglądzie nie ma).
  const label = jiraKey ?? (redmineId !== null ? redmineKey(redmineId) : null);
  const subject = () => redmineIssue?.subject ?? null;
  const title = template
    ? [...renderTitle(template.title, label, task.title, subject)]
        .slice(0, 240)
        .join("")
    : task.title.trim() || [...(subject() ?? "").trim()].slice(0, 240).join("");
  if (!title && label) throw new Error(unknownTitle(label));
  if (!title || [...title].length > 240)
    throw new Error("Tytuł musi mieć od 1 do 240 znaków.");
  const lineDetails = template
    ? mergeTemplate(task.details ?? null, template)
    : task.details;
  const details = lineDetails
    ? {
        ...JSON.parse(JSON.stringify(lineDetails)),
        project: lineDetails.project.trim(),
        waiting: lineDetails.waiting.trim(),
      }
    : undefined;
  if (
    details &&
    ([details.plannedDay, details.dueDay].some(
      (d: string) =>
        d !== "" &&
        (!/^\d{4}-\d{2}-\d{2}$/.test(d) ||
          localDay(+new Date(d + "T12:00:00")) !== d),
    ) ||
      ![-1, 0, 1].includes(details.priority) ||
      details.estimateMinutes < 0 ||
      details.estimateMinutes > 600_000 ||
      [...details.project].length > 80)
  )
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const data = await read();
  if (jiraKey) {
    const existing = [...data.tasks, ...(data.trash ?? [])].find(
      (t) => t.jiraKey === jiraKey,
    );
    if (existing)
      throw new Error(
        `${jiraKey} ma już zadanie „${existing.title}”${data.trash?.includes(existing) ? " (w koszu)" : ""}.`,
      );
  }
  if (redmineId !== null) {
    const existing = [...data.tasks, ...(data.trash ?? [])].find(
      (t) => t.redmineId === redmineId,
    );
    if (existing)
      throw new Error(
        `#${redmineId} ma już zadanie „${existing.title}”${data.trash?.includes(existing) ? " (w koszu)" : ""}.`,
      );
  }
  const now = Date.now();
  const id =
    Math.max(
      0,
      ...data.tasks.map((t) => t.id),
      ...(data.trash ?? []).map((t) => t.id),
    ) + 1;
  data.tasks.unshift({
    id,
    title,
    done: false,
    createdAt: now,
    ...(jiraKey
      ? { jiraKey: jiraKey, jiraUrl: `${previewJiraSite}/browse/${jiraKey}` }
      : {}),
    ...(redmineId !== null
      ? {
          redmineId,
          redmineUrl: `${previewRedmineSite}/issues/${redmineId}`,
          redmineStatus: redmineIssue?.status ?? "",
          redmineTracker: redmineIssue?.tracker ?? "",
          redmineDue: redmineIssue?.dueDate ?? "",
        }
      : {}),
    ...(details ? { details } : {}),
  });
  if (task.start) {
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
    data.sessions.unshift({
      id:
        Math.max(
          0,
          ...data.sessions.map((s) => s.id),
          ...(data.trashSessions ?? []).map((s) => s.id),
        ) + 1,
      taskId: id,
      startedAt: now,
      endedAt: null,
    });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
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
/** Szablony do podpowiedzi i parsera; odświeżane po każdej zmianie. */
export const templates = ref<TaskTemplate[]>([]);
const templatesKey = "todo-track-preview-templates-v1";
const templateEvent = "templates-changed";
const TEMPLATE_NAME = /^[\p{L}\p{N}_-]{1,60}$/u;
const sortTemplates = (list: TaskTemplate[]) =>
  [...list].sort((a, b) =>
    a.name
      .toLocaleLowerCase("pl")
      .localeCompare(b.name.toLocaleLowerCase("pl"), "pl"),
  );
function previewTemplates(): TaskTemplate[] {
  try {
    return JSON.parse(localStorage.getItem(templatesKey) ?? "[]");
  } catch {
    return [];
  }
}
const findTemplate = (list: TaskTemplate[], name: string) =>
  list.find(
    (t) =>
      t.name.toLocaleLowerCase("pl") === name.trim().toLocaleLowerCase("pl"),
  );
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
/** Walidacja jak `templates::validate`; zwraca oczyszczoną treść. */
function validTemplate(name: string, body: TemplateBody): TemplateBody {
  if (!TEMPLATE_NAME.test(name))
    throw new Error(
      "Nazwa szablonu może mieć od 1 do 60 liter, cyfr, „-” lub „_” (bez spacji).",
    );
  const clean = {
    title: body.title.trim(),
    estimateMinutes: body.estimateMinutes,
    steps: body.steps.map((s) => s.trim()).filter(Boolean),
    project: body.project.trim(),
    priority: body.priority,
  };
  if ([...clean.title].length > 240)
    throw new Error("Tytuł szablonu może mieć najwyżej 240 znaków.");
  if ([...clean.project].length > 80)
    throw new Error("Nazwa projektu może mieć najwyżej 80 znaków.");
  if (
    ![-1, 0, 1].includes(clean.priority) ||
    !Number.isInteger(clean.estimateMinutes) ||
    clean.estimateMinutes < 0 ||
    clean.estimateMinutes > 600_000
  )
    throw new Error("Niepoprawny priorytet lub szacunek szablonu.");
  if (clean.steps.length > 100 || clean.steps.some((s) => [...s].length > 240))
    throw new Error("Szablon może mieć najwyżej 100 kroków po 240 znaków.");
  return clean;
}
function previewTemplatesChanged(list: TaskTemplate[]) {
  localStorage.setItem(templatesKey, JSON.stringify(list));
  window.dispatchEvent(new CustomEvent(templateEvent));
}
/** Lista szablonów (`templates_list`); odświeża też `templates`. */
export async function loadTemplates(): Promise<TaskTemplate[]> {
  const list = desktop
    ? await invoke<TaskTemplate[]>("templates_list")
    : sortTemplates(previewTemplates());
  templates.value = list;
  return list;
}
/** Tworzy (`id` = null) albo zmienia szablon, także nazwę (`template_save`). */
export async function saveTemplate(
  id: number | null,
  name: string,
  body: TemplateBody,
): Promise<TaskTemplate[]> {
  name = name.trim();
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_save", {
      id,
      name,
      template: body,
    });
    return templates.value;
  }
  const clean = validTemplate(name, body);
  const list = previewTemplates();
  const other = findTemplate(list, name);
  if (other && other.id !== id)
    throw new Error(`Szablon „${other.name}” już istnieje.`);
  if (id !== null) {
    const current = list.find((t) => t.id === id);
    if (!current) throw new Error("Nie znaleziono szablonu.");
    Object.assign(current, clean, { name });
  } else
    list.push({
      id: Math.max(0, ...list.map((t) => t.id)) + 1,
      name,
      ...clean,
    });
  previewTemplatesChanged(list);
  return loadTemplates();
}
/** Usuwa szablon (`template_delete`). */
export async function deleteTemplate(id: number): Promise<TaskTemplate[]> {
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_delete", { id });
    return templates.value;
  }
  const list = previewTemplates();
  if (!list.some((t) => t.id === id))
    throw new Error("Nie znaleziono szablonu.");
  previewTemplatesChanged(list.filter((t) => t.id !== id));
  return loadTemplates();
}
/** Folder przypisany do zadania (`agents::Binding`, tabela `agent_bindings`). */
export interface AgentBinding {
  folder: string;
  taskId: number;
  title: string;
}
const bindingsKey = "todo-track-preview-agent-bindings-v1";
/** Zdarzenie podglądu: przypisania folderów zmieniły się, dopasuj agentów od nowa. */
export const previewAgentsEvent = "todo-track-preview-agents";
/** Przypisania w podglądzie: `{ folder, taskId }` z localStorage. */
export function previewBindings(): { folder: string; taskId: number }[] {
  try {
    const value = JSON.parse(localStorage.getItem(bindingsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/** Postać zapisywana (jak `agents::canon_folder` w Windows): separatory `\`, bez końcowego. */
const trimFolder = (folder: string) =>
  folder.trim().replace(/[\\/]/g, "\\").replace(/\\+$/, "");
const sameFolder = (a: string, b: string) =>
  trimFolder(a).toLowerCase() === trimFolder(b).toLowerCase();
async function previewBindingList(): Promise<AgentBinding[]> {
  const data = await read();
  return previewBindings()
    .flatMap((b) => {
      const task = [...data.tasks, ...(data.trash ?? [])].find(
        (t) => t.id === b.taskId,
      );
      return task
        ? [{ folder: b.folder, taskId: b.taskId, title: task.title }]
        : [];
    })
    .sort((a, b) =>
      a.folder.toLowerCase().localeCompare(b.folder.toLowerCase()),
    );
}
function previewBindingsChanged(list: { folder: string; taskId: number }[]) {
  localStorage.setItem(bindingsKey, JSON.stringify(list));
  // Jak `agents::bind`: przypisanie uzupełnia okresy agentów bez zadania.
  previewAgentBackfill(Date.now());
  window.dispatchEvent(new Event(previewAgentsEvent));
}
/** Przypisania folderów do zadań (`agent_bindings`). */
export async function agentBindings(): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bindings");
  return previewBindingList();
}
/** Przypisuje folder agenta do zadania (`agent_bind`); zastępuje wcześniejsze. */
export async function bindAgentFolder(
  folder: string,
  taskId: number,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bind", { folder, taskId });
  const clean = trimFolder(folder);
  if (!clean || [...clean].length > 1024)
    throw new Error("Niepoprawny folder.");
  if (!(await read()).tasks.some((t) => t.id === taskId))
    throw new Error("Nie znaleziono zadania.");
  previewBindingsChanged([
    ...previewBindings().filter((b) => !sameFolder(b.folder, clean)),
    { folder: clean, taskId },
  ]);
  return previewBindingList();
}
/** Okresy pracy agentów w podglądzie (`agent_sessions`), dane z `seed.ts`. */
export const agentSessionsKey = "todo-track-preview-agent-sessions-v1";
function previewAgentPeriods(): AgentPeriod[] {
  try {
    const value = JSON.parse(localStorage.getItem(agentSessionsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/**
 * Jak `agent_time::backfill`: okresy z ostatnich 7 dni bez zadania dostają
 * zadanie z przypisania folderu (także podfolderu, wygrywa najdłuższe).
 */
function previewAgentBackfill(now: number) {
  const norm = (f: string) => trimFolder(f).toLowerCase();
  const bindings = previewBindings().map((b) => ({ ...b, norm: norm(b.folder) }));
  let changed = false;
  const periods = previewAgentPeriods().map((p) => {
    if (p.taskId !== null || p.end < now - 7 * 86_400_000) return p;
    const folder = norm(p.folder);
    const found = bindings
      .filter((b) => b.norm && (folder === b.norm || folder.startsWith(b.norm + "\\")))
      .sort((a, b) => b.norm.length - a.norm.length)[0];
    if (!found) return p;
    changed = true;
    return { ...p, taskId: found.taskId };
  });
  if (changed) localStorage.setItem(agentSessionsKey, JSON.stringify(periods));
}
/**
 * Czas agentów w `[from, to)` (`agent_time`): okresy nakładające się na zakres
 * i sumy według zadania z częścią równoległą do Twojego pomiaru.
 */
export async function agentTime(from: number, to: number): Promise<AgentTime> {
  if (desktop) return invoke("agent_time", { from, to });
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const periods = previewAgentPeriods()
    .filter((p) => p.start < to && p.end > from)
    .sort((a, b) => a.start - b.start || a.id - b.id);
  const now = Date.now();
  const { sessions, trashSessions } = await read();
  return {
    periods,
    ...agentTotals(periods, [...sessions, ...(trashSessions ?? [])], from, to, now),
  };
}
/** Usuwa przypisanie folderu (`agent_unbind`). */
export async function unbindAgentFolder(
  folder: string,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_unbind", { folder });
  previewBindingsChanged(
    previewBindings().filter((b) => !sameFolder(b.folder, folder)),
  );
  return previewBindingList();
}
/** Odświeża `templates` po zmianie w innym oknie (np. szybkie przełączanie). */
export async function onTemplatesChanged(): Promise<() => void> {
  const reload = () => void loadTemplates().catch(() => {});
  if (desktop) return listen(templateEvent, reload);
  window.addEventListener(templateEvent, reload);
  return () => window.removeEventListener(templateEvent, reload);
}
export async function editSession(
  id: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
  if (desktop) return invoke("edit_session", { id, startedAt, endedAt });
  if (
    !Number.isSafeInteger(startedAt) ||
    !Number.isSafeInteger(endedAt) ||
    startedAt < 0 ||
    endedAt <= startedAt ||
    endedAt > Date.now()
  )
    throw new Error(
      "Koniec musi być późniejszy niż początek i nie może być w przyszłości.",
    );
  const data = await read();
  const session = data.sessions.find((s) => s.id === id);
  if (!session) throw new Error("Nie znaleziono przedziału.");
  if (session.endedAt === null)
    throw new Error("Zatrzymaj pomiar przed edycją przedziału.");
  if (
    data.sessions.some(
      (s) =>
        s.id !== id &&
        s.startedAt < endedAt &&
        (s.endedAt === null || s.endedAt > startedAt) &&
        (s.endedAt === null || s.endedAt > s.startedAt),
    )
  )
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  session.startedAt = startedAt;
  session.endedAt = endedAt;
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Odpowiednik db::split_session dla trybu podglądu. */
export async function splitSession(
  id: number,
  at: number,
  targetTask: number | null = null,
): Promise<Snapshot> {
  if (desktop) return invoke("split_session", { id, at, targetTask });
  const data = await read();
  const now = Date.now();
  if (data.trashSessions?.some((s) => s.id === id))
    throw new Error("Przedział należy do zadania w koszu.");
  const session = data.sessions.find((s) => s.id === id);
  if (!session) throw new Error("Nie znaleziono przedziału.");
  const end = session.endedAt ?? now;
  if (!Number.isSafeInteger(at) || !(session.startedAt < at && at < end))
    throw new Error("Moment podziału musi leżeć wewnątrz przedziału.");
  const targetId = targetTask ?? session.taskId;
  if (data.trash?.some((t) => t.id === targetId))
    throw new Error("Zadanie docelowe jest w koszu. Najpierw je przywróć.");
  const target = data.tasks.find((t) => t.id === targetId);
  if (!target) throw new Error("Nie znaleziono zadania docelowego.");
  if (session.endedAt === null) {
    if (target.done)
      throw new Error("Najpierw przywróć zadanie docelowe do zrobienia.");
    target.archived = false;
    target.details = unblocked(target);
  }
  const second: Session = {
    id:
      Math.max(
        0,
        ...data.sessions.map((s) => s.id),
        ...(data.trashSessions ?? []).map((s) => s.id),
      ) + 1,
    taskId: targetId,
    startedAt: at,
    endedAt: session.endedAt,
  };
  session.endedAt = at;
  data.sessions.unshift(second);
  localStorage.setItem(key, JSON.stringify(data));
  return data;
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

export async function saveDetails(
  id: number,
  details: TaskDetails,
): Promise<Snapshot> {
  if (desktop) return invoke("save_details", { id, details });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  if (details.plannedStart && !validTime(details.plannedStart))
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const saved: TaskDetails = JSON.parse(JSON.stringify(details));
  saved.project = details.project.trim();
  saved.waiting = details.waiting.trim();
  // Zadanie TS jak planner::validate: puste id czyści nazwę, puste pola znikają.
  const tsId = (details.tsTaskId ?? "").trim();
  const tsName = (details.tsTaskName ?? "").trim();
  delete saved.tsTaskId;
  delete saved.tsTaskName;
  if (tsId) {
    if (!tsValidTaskId(tsId) || !tsValidTaskName(tsName))
      throw new Error("Niepoprawne zadanie TS.");
    saved.tsTaskId = tsId;
    if (tsName) saved.tsTaskName = tsName;
  }
  // Jak planner::validate: reguła cyklu i dane blokady.
  const rule = normalizeRule(saved.repeatRule);
  if (!rule) throw new Error("Niepoprawna reguła cyklu.");
  saved.repeatRule = rule;
  if (!ruleUsesWeekdays(rule.kind)) saved.repeatDays = [];
  if (
    saved.waitingRemindDay &&
    !/^\d{4}-\d{2}-\d{2}$/.test(saved.waitingRemindDay)
  )
    throw new Error("Niepoprawne dane blokady „Czekam na”.");
  const wasWaiting = !!task.details?.waiting;
  if (!saved.waiting) {
    saved.waitingSince = 0;
    saved.waitingStatus = "";
    saved.waitingRemindDay = "";
    if (wasWaiting) saved.waitingDismissed = issueStatus(task);
  } else {
    // Nowa blokada zaczyna bez przypomnienia (jak planner::save).
    if (!wasWaiting) saved.waitingRemindDay = "";
    if (!saved.waitingSince) saved.waitingSince = Date.now();
    if (!saved.waitingStatus) saved.waitingStatus = issueStatus(task);
  }
  if (isRecurring(saved) && !saved.repeatFrom)
    saved.repeatFrom = localDay(Date.now());
  if (isRecurring(saved) && !saved.plannedDay)
    saved.plannedDay = saved.repeatFrom;
  task.details = saved;
  if (saved.waiting)
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(s.startedAt, Date.now());
    });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

/** Zmiana blokady „Czekam na” (`planner::WaitingChange`). */
export interface WaitingChange {
  action: "block" | "unblock" | "dismiss" | "remind";
  text?: string;
}
/**
 * Blokada z kafelka: „Odblokuj”, propozycje z Jiry („Oznacz jako czekam na”,
 * „Nie teraz”). Zmienia tylko pola blokady (`set_waiting`); odpowiednik w podglądzie.
 */
export async function setWaiting(
  id: number,
  change: WaitingChange,
): Promise<Snapshot> {
  if (desktop) return invoke("set_waiting", { id, change });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  const text = (change.text ?? "").trim();
  const status = issueStatus(task);
  const now = Date.now();
  const d = detailsOf(task);
  if (change.action === "block") {
    if (!text || [...text].length > 300)
      throw new Error("Opis blokady musi mieć od 1 do 300 znaków.");
    Object.assign(d, {
      waiting: text,
      waitingSince: now,
      waitingStatus: status,
      waitingRemindDay: "",
      waitingDismissed: "",
    });
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(s.startedAt, now);
    });
  } else if (change.action === "unblock")
    Object.assign(d, {
      waiting: "",
      waitingSince: 0,
      waitingStatus: "",
      waitingRemindDay: "",
      waitingDismissed: status,
    });
  else if (change.action === "dismiss") d.waitingDismissed = status;
  else if (change.action === "remind") {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(text))
      throw new Error("Niepoprawny dzień przypomnienia.");
    // Stary toast po odblokowaniu: bez blokady nic nie zapisujemy.
    if (d.waiting) d.waitingRemindDay = text;
  } else throw new Error("Nieznana zmiana blokady.");
  task.details = d;
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

export async function addSession(
  taskId: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
  if (desktop) return invoke("add_session", { taskId, startedAt, endedAt });
  if (
    !Number.isSafeInteger(startedAt) ||
    !Number.isSafeInteger(endedAt) ||
    startedAt < 0 ||
    endedAt <= startedAt ||
    endedAt > Date.now()
  )
    throw new Error(
      "Koniec musi być późniejszy niż początek i nie może być w przyszłości.",
    );
  const data = await read();
  if (!data.tasks.some((t) => t.id === taskId))
    throw new Error("Nie znaleziono zadania.");
  if (
    data.sessions.some(
      (s) =>
        s.startedAt < endedAt &&
        (s.endedAt === null || s.endedAt > startedAt) &&
        (s.endedAt === null || s.endedAt > s.startedAt),
    )
  )
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  data.sessions.unshift({
    id:
      Math.max(
        0,
        ...data.sessions.map((s) => s.id),
        ...(data.trashSessions ?? []).map((s) => s.id),
      ) + 1,
    taskId,
    startedAt,
    endedAt,
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

export async function setPriority(
  id: number,
  priority: number,
): Promise<Snapshot> {
  if (desktop) return invoke("set_priority", { id, priority });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), priority };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/**
 * Odpowiednik `reorder_tasks` (`planner::reorder`): zmienia wyłącznie `order`
 * i `priority` podanych zadań, reszta szczegółów zostaje bez zmian.
 */
export async function reorderTasks(
  entries: { id: number; order: number; priority: number }[],
): Promise<Snapshot> {
  if (desktop) return invoke("reorder_tasks", { entries });
  const data = await read();
  if (
    entries.length > 10000 ||
    entries.some(
      (e) =>
        !Number.isSafeInteger(e.order) ||
        e.order < 0 ||
        e.order > 1_000_000 ||
        ![-1, 0, 1].includes(e.priority),
    )
  )
    throw new Error("Niepoprawna kolejność zadań.");
  const tasks = entries.map((e) => data.tasks.find((t) => t.id === e.id));
  if (tasks.some((t) => !t)) throw new Error("Nie znaleziono zadania.");
  entries.forEach((e, i) => {
    const task = tasks[i]!;
    task.details = { ...detailsOf(task), order: e.order, priority: e.priority };
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/**
 * Odpowiednik `plan_task` (`planner::plan_task`): przeniesienie na tablicy
 * tygodnia zmienia wyłącznie `plannedDay` (pusty = „Do zaplanowania”) i `order`.
 */
export async function planTask(
  id: number,
  day: string,
  order: number,
): Promise<Snapshot> {
  if (desktop) return invoke("plan_task", { id, day, order });
  if (
    (day !== "" &&
      (!/^\d{4}-\d{2}-\d{2}$/.test(day) ||
        localDay(+new Date(day + "T12:00:00")) !== day)) ||
    !Number.isSafeInteger(order) ||
    order < 0 ||
    order > 1_000_000
  )
    throw new Error("Niepoprawny dzień planu.");
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), plannedDay: day, order };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Wpis `plan_tasks`: dzień planu i kolejność jednego zadania. */
export interface PlanEntry {
  id: number;
  day: string;
  order: number;
}
/**
 * Odpowiednik `plan_tasks` (`planner::plan_tasks`): „Zastosuj” z „Rozłóż”
 * zapisuje dni wielu zadań naraz; jak `planTask` zmienia tylko `plannedDay`
 * i `order`, a przy błędzie nie zmienia niczego.
 */
export async function planTasks(entries: PlanEntry[]): Promise<Snapshot> {
  if (desktop) return invoke("plan_tasks", { entries });
  if (
    entries.length > 10000 ||
    entries.some(
      (e) =>
        (e.day !== "" &&
          (!/^\d{4}-\d{2}-\d{2}$/.test(e.day) ||
            localDay(+new Date(e.day + "T12:00:00")) !== e.day)) ||
        !Number.isSafeInteger(e.order) ||
        e.order < 0 ||
        e.order > 1_000_000,
    )
  )
    throw new Error("Niepoprawny dzień planu.");
  const data = await read();
  const tasks = entries.map((e) => data.tasks.find((t) => t.id === e.id));
  if (tasks.some((t) => !t)) throw new Error("Nie znaleziono zadania.");
  entries.forEach((e, i) => {
    const task = tasks[i]!;
    task.details = { ...detailsOf(task), plannedDay: e.day, order: e.order };
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
export async function closeDay(
  day: string,
  closed: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("close_day", { day, closed });
  const data = await read();
  data.closedDays = (data.closedDays ?? []).filter((d) => d !== day);
  if (closed) data.closedDays.push(day);
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
function localDay(at: number) {
  const d = new Date(at);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
/**
 * Dni robocze i święta z ustawień podglądu (klucz jak w `settings.ts`, bez
 * importu, który zamknąłby cykl modułów) dla reguły „ostatni dzień roboczy”.
 */
function previewWorkSettings(): {
  workDays: number[];
  holidaysInTarget: boolean;
} {
  let stored: Record<string, unknown> = {};
  try {
    stored =
      JSON.parse(
        localStorage.getItem("todo-track-preview-settings-v1") ?? "{}",
      ) ?? {};
  } catch {
    stored = {};
  }
  const days = stored.workDays;
  return {
    workDays:
      Array.isArray(days) && days.every((d) => Number.isInteger(d))
        ? (days as number[])
        : [1, 2, 3, 4, 5],
    holidaysInTarget: stored.holidaysInTarget !== false,
  };
}
function generateRecurring(data: Snapshot): boolean {
  let changed = false;
  const today = localDay(Date.now());
  const work = previewWorkSettings();
  const earliest = new Date();
  earliest.setDate(earliest.getDate() - 365);
  for (const source of [...data.tasks]) {
    const rule = source.details;
    if (!rule || !isRecurring(rule) || !rule.repeatFrom || source.archived)
      continue;
    const last = data.recurrenceState?.[source.id];
    const cursor = new Date((last || rule.repeatFrom) + "T12:00:00");
    if (last) cursor.setDate(cursor.getDate() + 1);
    if (localDay(+cursor) < rule.repeatFrom)
      cursor.setTime(+new Date(rule.repeatFrom + "T12:00:00"));
    if (+cursor < +earliest) cursor.setTime(+earliest);
    while (Number.isFinite(+cursor) && localDay(+cursor) <= today) {
      const day = localDay(+cursor);
      if (
        occurs(rule, rule.repeatFrom, day, work) &&
        rule.plannedDay !== day
      ) {
        data.tasks.unshift({
          id:
            Math.max(
              0,
              ...data.tasks.map((t) => t.id),
              ...(data.trash ?? []).map((t) => t.id),
            ) + 1,
          title: source.title,
          done: false,
          createdAt: Date.now(),
          details: {
            ...detailsOf(source),
            notes: {},
            waiting: "",
            plannedDay: day,
            dueDay: "",
            repeatDays: [],
            repeatFrom: "",
            repeatRule: emptyRule(),
            waitingSince: 0,
            waitingStatus: "",
            waitingDismissed: "",
            waitingRemindDay: "",
            pinnedAt: 0,
            checklist: (rule.checklist ?? []).map((i) => ({
              ...i,
              done: false,
            })),
          },
        });
      }
      (data.recurrenceState ??= {})[source.id] = day;
      changed = true;
      cursor.setDate(cursor.getDate() + 1);
    }
  }
  return changed;
}

/** Wynik Start/Stop, odpowiednik `shortcuts::Toggle` w Rust. */
export type ToggleResult =
  | { action: "stopped"; taskId: number; title: string; elapsedMs: number }
  | { action: "started"; taskId: number; title: string }
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
/** Przypięcie do widgetu i odpięcie (`set_pinned`); odpowiednik w podglądzie. */
export async function setPinned(
  id: number,
  pinned: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("set_pinned", { id, pinned });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  if (pinned) {
    if (task.done || task.archived)
      throw new Error("Przypiąć można tylko otwarte zadanie.");
    if (pinnedTasks(data).filter((t) => t.id !== id).length >= maxPinned)
      throw new Error(
        `Przypiąć można najwyżej ${maxPinned} zadania. Odepnij jedno z nich.`,
      );
  }
  task.details = { ...detailsOf(task), pinnedAt: pinned ? Date.now() : 0 };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
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

/**
 * Start/Stop jak globalny skrót: trwający pomiar → Stop, brak pomiaru → Start
 * ostatniego zadania, brak takiego → „quick”. Ani komenda `toggle_measurement`, ani
 * podgląd nie otwierają wtedy szybkiego przełączania: robi to wywołujący (`quick_open`).
 * Potwierdzenie w aplikacji przychodzi zdarzeniem `toggle-feedback` (tekst).
 */
export async function toggleMeasurement(): Promise<ToggleResult> {
  if (desktop) return invoke("toggle_measurement");
  const data = await read();
  const active = data.sessions.find((s) => s.endedAt === null);
  if (active) {
    const task = data.tasks.find((t) => t.id === active.taskId);
    const now = Date.now();
    await change("stop");
    return {
      action: "stopped",
      taskId: active.taskId,
      title: task?.title ?? "",
      elapsedMs: Math.max(0, now - active.startedAt),
    };
  }
  const task = resumeCandidate(data);
  if (!task) return { action: "quick" };
  await change("start", task.id);
  return { action: "started", taskId: task.id, title: task.title };
}

/** Uruchamianie z Windows: stan wpisu w rejestrze i opcja „schowane do traya”. */
export interface Autostart {
  enabled: boolean;
  hidden: boolean;
}
const autostartKey = "todo-track-preview-autostart-v1";
const previewSettingsKey = "todo-track-preview-settings-v1";
function previewJson(name: string): Record<string, unknown> {
  try {
    const value = JSON.parse(localStorage.getItem(name) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value)
      ? value
      : {};
  } catch {
    return {};
  }
}

/**
 * `autostart_get`: „włączone” pochodzi z rejestru (w podglądzie z osobnego klucza
 * localStorage), „schowane” z ustawienia `autostartHidden`.
 */
export async function readAutostart(): Promise<Autostart> {
  if (desktop) return invoke("autostart_get");
  return {
    enabled: previewJson(autostartKey).enabled === true,
    hidden: previewJson(previewSettingsKey).autostartHidden === true,
  };
}

/** `autostart_set`: zapisuje wpis Run (z `--hidden`, gdy `hidden`) i ustawienie. */
export async function writeAutostart(
  enabled: boolean,
  hidden: boolean,
): Promise<Autostart> {
  if (desktop) return invoke("autostart_set", { enabled, hidden });
  localStorage.setItem(autostartKey, JSON.stringify({ enabled }));
  const settings = {
    ...previewJson(previewSettingsKey),
    autostartHidden: hidden,
  };
  localStorage.setItem(previewSettingsKey, JSON.stringify(settings));
  return readAutostart();
}

/** Stan mini-widgetu (`widget::Status` w Rust). */
export interface WidgetStatus {
  enabled: boolean;
  backgroundOnly: boolean;
  /** Schowany przyciskiem X do końca sesji aplikacji; ustawienie się nie zmienia. */
  dismissed: boolean;
}
const widgetDismissedKey = "todo-track-preview-widget-dismissed";
const widgetEvent = "widget-state";
function previewWidgetDismissed(): boolean {
  try {
    return sessionStorage.getItem(widgetDismissedKey) === "1";
  } catch {
    return false;
  }
}

/**
 * Podgląd `Status::after_settings_change`: schowanie X kasujemy już przy
 * wyłączeniu widgetu (wyłączony i tak jest niewidoczny), więc ponowne włączenie
 * w ustawieniach go pokazuje.
 */
export function previewWidgetSettingsChanged(widgetEnabled: boolean) {
  if (widgetEnabled) return;
  try {
    sessionStorage.removeItem(widgetDismissedKey);
  } catch {}
}

/** `widget_status`: ustawienia widgetu i schowanie do końca sesji. */
export async function readWidgetStatus(): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_status");
  const settings = previewJson(previewSettingsKey);
  return {
    enabled: settings.widgetEnabled === true,
    backgroundOnly: settings.widgetBackgroundOnly === true,
    dismissed: previewWidgetDismissed(),
  };
}

/**
 * `widget_dismiss`: X na widgecie chowa go do końca sesji aplikacji (w podglądzie
 * do końca sesji karty), przełącznik w nagłówku przywraca. Ustawienie bez zmian.
 */
export async function dismissWidget(dismissed: boolean): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_dismiss", { dismissed });
  try {
    if (dismissed) sessionStorage.setItem(widgetDismissedKey, "1");
    else sessionStorage.removeItem(widgetDismissedKey);
  } catch {}
  const status = await readWidgetStatus();
  window.dispatchEvent(new CustomEvent(widgetEvent, { detail: status }));
  return status;
}

/** Nasłuch zmian stanu widgetu (zdarzenie Tauri albo zdarzenie okna w podglądzie). */
export async function onWidgetStatus(
  handler: (status: WidgetStatus) => void,
): Promise<() => void> {
  if (desktop) {
    return listen<WidgetStatus>(widgetEvent, (e) => handler(e.payload));
  }
  const listener = (e: Event) =>
    handler((e as CustomEvent<WidgetStatus>).detail);
  window.addEventListener(widgetEvent, listener);
  return () => window.removeEventListener(widgetEvent, listener);
}

/**
 * Ładunek zdarzenia `navigate` (`notify::View` w Rust): kliknięcie powiadomienia
 * Windows lub przycisku „Podziel…”. Brak `view` = tylko główne okno.
 */
export interface NavigateRequest {
  view?: string | null;
  /** `today` = filtr „Dzisiaj: plan i praca”; `due` = ten filtr z terminami dziś i zaległymi. */
  filter?: string | null;
  /** Zadanie do rozwinięcia na liście zadań. */
  task?: number | null;
  /** Przedział do otwarcia w edytorze na zakładce Podziel. */
  split?: number | null;
  /**
   * Grupa listy zadań, do której przewinąć (np. „Nierozpoczęte”), przy
   * `view: "journal"` zakładka `morning` (Plan i historia → Rano) albo
   * `week-review` (Plan i historia → Tydzień → Przegląd), albo przy
   * `view: "settings"` sekcja Ustawień (`work`, `calendar`, `timesheet`,
   * `jira`, `redmine`, `notifications`, `focus`, `templates`, `windows`, `shortcuts`,
   * `startup`; nieznana sekcja otwiera ostatnią).
   */
  section?: string | null;
  /** Plan i historia → Zamknięcie dnia. */
  dayClose?: boolean;
}
/** Nazwa zdarzenia okna, którym podgląd (i testy) symuluje `navigate` z backendu. */
export const previewNavigateEvent = "todo-track-navigate";

/** Nasłuch `navigate`: zdarzenie Tauri albo zdarzenie okna w podglądzie. */
export async function onNavigate(
  handler: (request: NavigateRequest) => void,
): Promise<() => void> {
  if (desktop)
    return listen<NavigateRequest>("navigate", (e) => handler(e.payload));
  const listener = (e: Event) =>
    handler((e as CustomEvent<NavigateRequest>).detail ?? {});
  window.addEventListener(previewNavigateEvent, listener);
  return () => window.removeEventListener(previewNavigateEvent, listener);
}

/** `widget_open_main`: klik w nazwę zadania na widgecie. W podglądzie nic nie robi. */
export async function openMainWindow(): Promise<void> {
  if (desktop) await invoke("widget_open_main");
}

export type ExportFormat = "csv" | "xlsx";
/** Wynik eksportu: ścieżka zapisanego pliku albo (w podglądzie) tekst CSV. */
export type ExportResult = { path: string } | { text: string };
/**
 * `export_report`: okno zapisu i plik CSV albo XLSX z przedziałami wybranych
 * dni; `null`, gdy użytkownik anulował. W podglądzie CSV wraca jako tekst,
 * a XLSX jest niedostępny.
 */
export async function exportReport(
  format: ExportFormat,
  days: string[],
  name: string,
): Promise<ExportResult | null> {
  if (!days.length) throw new Error("Wybierz co najmniej jeden dzień do eksportu.");
  if (desktop) {
    const path = await invoke<string | null>("export_report", {
      format,
      days,
      name,
    });
    return path ? { path } : null;
  }
  if (format === "xlsx")
    throw new Error("Eksport XLSX jest dostępny tylko w aplikacji Windows.");
  return { text: exportCsv(exportRows(await read(), days, Date.now())) };
}

/** Stan kalendarza Microsoft (`calendar_status`). */
export interface CalendarStatus {
  clientId: string;
  tenant: string;
  /** Zalogowane konto; pusty, gdy niepołączone. */
  email: string;
  connected: boolean;
  /** Trwa logowanie w przeglądarce. */
  connecting: boolean;
  lastSync: number | null;
  lastError: string;
}
/** Spotkanie (`calendar_events`): czasy w ms, całodniowe od lokalnej północy. */
export interface CalendarEvent {
  id: string;
  start: number;
  end: number;
  subject: string;
  /** `free`, `tentative`, `busy`, `oof`, `workingElsewhere` albo `unknown`. */
  showAs: string;
  allDay: boolean;
  cancelled: boolean;
  /** Odpowiedź na zaproszenie, np. `accepted`, `declined`, `organizer`. */
  response: string;
}
/** Dane podglądu: ustawienia, pobrane spotkania i spotkania „w chmurze” (`sample`). */
interface PreviewCalendar {
  clientId: string;
  tenant: string;
  email: string;
  lastSync: number | null;
  lastError: string;
  events: CalendarEvent[];
  sample: CalendarEvent[];
}
export const calendarKey = "todo-track-preview-calendar-v1";
const calendarEvent = "calendar-changed";
/** Konto „zalogowane” w podglądzie. */
export const previewCalendarEmail = "jan.kowalski@firma.example";
const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** Jak `calendar::valid_tenant`: organizations/common/consumers, GUID albo domena. */
const validTenant = (t: string) =>
  ["organizations", "common", "consumers"].includes(t) ||
  guid.test(t) ||
  (t.length <= 253 &&
    t.includes(".") &&
    t.split(".").every((l) => /^[a-z0-9-]+$/i.test(l)));
export const calendarClientIdMessage =
  "Podaj identyfikator aplikacji (client ID) z rejestracji w Entra ID, np. 00000000-0000-0000-0000-000000000000.";
function previewCalendar(): PreviewCalendar {
  const empty: PreviewCalendar = {
    clientId: "",
    tenant: "organizations",
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
    sample: [],
  };
  try {
    const value = JSON.parse(localStorage.getItem(calendarKey) ?? "{}");
    return value && typeof value === "object" ? { ...empty, ...value } : empty;
  } catch {
    return empty;
  }
}
function savePreviewCalendar(data: PreviewCalendar) {
  localStorage.setItem(calendarKey, JSON.stringify(data));
  window.dispatchEvent(new Event(calendarEvent));
}
const previewStatus = (c: PreviewCalendar): CalendarStatus => ({
  clientId: c.clientId,
  tenant: c.tenant || "organizations",
  email: c.email,
  connected: !!c.email && !!c.clientId,
  connecting: false,
  lastSync: c.lastSync,
  lastError: c.lastError,
});
export async function calendarStatus(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_status");
  return previewStatus(previewCalendar());
}
/**
 * „Połącz konto Microsoft”: logowanie w przeglądarce i pierwsze pobranie.
 * Podgląd symuluje udane logowanie i „pobiera” spotkania z `sample`.
 */
export async function calendarConnect(
  clientId: string,
  tenant: string,
): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_connect", { clientId, tenant });
  const id = clientId.trim().toLowerCase();
  const t = tenant.trim().toLowerCase() || "organizations";
  if (!guid.test(id)) throw new Error(calendarClientIdMessage);
  if (!validTenant(t))
    throw new Error(
      "Podaj dzierżawcę: organizations, identyfikator albo domenę firmy.",
    );
  const c = previewCalendar();
  const next: PreviewCalendar = {
    ...c,
    clientId: id,
    tenant: t,
    email: previewCalendarEmail,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** „Rozłącz”: usuwa token i spotkania; client ID i dzierżawca zostają. */
export async function calendarDisconnect(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_disconnect");
  const next = {
    ...previewCalendar(),
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** „Odśwież”: pobiera spotkania teraz. */
export async function calendarRefresh(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_refresh");
  const c = previewCalendar();
  if (!c.clientId)
    throw new Error(
      "Kalendarz jest wyłączony: podaj client ID w Ustawieniach → Kalendarz.",
    );
  if (!c.email)
    throw new Error("Połącz konto Microsoft w Ustawieniach → Kalendarz.");
  const next = {
    ...c,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** Spotkania nakładające się na `[from, to)`, według początku. */
export async function calendarEvents(
  from: number,
  to: number,
): Promise<CalendarEvent[]> {
  if (desktop) return invoke("calendar_events", { from, to });
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const c = previewCalendar();
  // Jak `calendar_events`: bez połączonego konta żadne spotkania nie obowiązują.
  if (!c.email) return [];
  return c.events
    .filter(
      (e) => e.start < to && (e.end > from || (e.end === e.start && e.start >= from)),
    )
    .sort((a, b) => a.start - b.start || a.end - b.end || a.id.localeCompare(b.id));
}
/** Nasłuch zmian kalendarza (pobranie, połączenie, rozłączenie). */
export async function onCalendarChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(calendarEvent, () => handler());
  window.addEventListener(calendarEvent, handler);
  return () => window.removeEventListener(calendarEvent, handler);
}
/**
 * Czy spotkanie zajmuje czas (`calendar::occupies`): busy, oof, workingElsewhere
 * albo tentative, nieanulowane i nieodrzucone.
 */
export const meetingOccupies = (e: CalendarEvent) =>
  ["busy", "oof", "workingElsewhere", "tentative"].includes(e.showAs) &&
  !e.cancelled &&
  e.response !== "declined";
/** Tytuł spotkania w interfejsie: „Spotkanie”, gdy tytuły są wyłączone albo puste. */
export const meetingTitle = (e: CalendarEvent, showTitles: boolean) =>
  (showTitles && e.subject.trim()) || "Spotkanie";

/**
 * Przejście w głównym oknie z innego okna (np. szybkiego przełączania):
 * pokazuje główne okno i wysyła mu `navigate`. W podglądzie nic nie robi,
 * bo wszystkie widoki są w jednym oknie z zarejestrowaną obsługą.
 */
export async function navigateMain(request: NavigateRequest): Promise<void> {
  if (!desktop) return;
  await invoke("widget_open_main");
  await emitTo("main", "navigate", request);
}

/** „Wnioski” z przeglądu tygodnia w podglądzie (`week_notes`): tydzień → tekst. */
export const weekNotesKey = "todo-track-preview-week-notes-v1";
const WEEK_NOTE_MAX = 10_000;
function previewWeekNotes(): Record<string, string> {
  const value = previewJson(weekNotesKey);
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, string] => typeof entry[1] === "string",
    ),
  );
}
/** Jak `week_notes::valid_week`: istniejący poniedziałek RRRR-MM-DD. */
function checkWeek(week: string) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(week);
  const d = m ? new Date(Date.UTC(+m[1], +m[2] - 1, +m[3])) : null;
  if (
    !m ||
    !d ||
    d.getUTCFullYear() !== +m[1] ||
    d.getUTCMonth() !== +m[2] - 1 ||
    d.getUTCDate() !== +m[3] ||
    d.getUTCDay() !== 1
  )
    throw new Error("Tydzień musi być poniedziałkiem w formacie RRRR-MM-DD.");
}
/** Wnioski tygodnia zaczynającego się w poniedziałek `week` (`week_note`). */
export async function weekNote(week: string): Promise<string> {
  if (desktop) return invoke("week_note", { week });
  checkWeek(week);
  return previewWeekNotes()[week] ?? "";
}
/**
 * Zapis wniosków (`week_note_save`): tekst bez białych znaków na końcach, pusty
 * usuwa wpis. Zwraca zapisany tekst.
 */
export async function saveWeekNote(week: string, text: string): Promise<string> {
  if (desktop) return invoke("week_note_save", { week, text });
  checkWeek(week);
  const value = text.trim();
  if ([...value].length > WEEK_NOTE_MAX)
    throw new Error(`Wnioski mogą mieć najwyżej ${WEEK_NOTE_MAX} znaków.`);
  const notes = previewWeekNotes();
  if (value) notes[week] = value;
  else delete notes[week];
  localStorage.setItem(weekNotesKey, JSON.stringify(notes));
  return value;
}

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
/**
 * Podgląd: ostatnia wiadomość rozszerzenia (`lastMessage`, `lastKey`,
 * `lastKeyAt`) w localStorage; testy ustawiają ją, żeby pokazać „połączone”.
 */
export const browserKey = "todo-track-preview-browser-v1";
/** `browser_status`: stan wyłącznika, rejestracji i ostatniego sygnału. */
export async function browserStatus(): Promise<BrowserStatus> {
  if (desktop) return invoke("browser_status");
  const enabled = previewJson(previewSettingsKey).browserExtension === true;
  const data = previewJson(browserKey);
  const num = (v: unknown) => (typeof v === "number" ? v : null);
  const lastMessage = num(data.lastMessage);
  return {
    enabled,
    registered: enabled,
    connected:
      enabled && lastMessage != null && Date.now() - lastMessage <= 45_000,
    lastMessage,
    lastKey: typeof data.lastKey === "string" ? data.lastKey : null,
    lastKeyAt: num(data.lastKeyAt),
    error: "",
    extensionId: browserExtensionId,
    extensionFolder: "browser-extension",
  };
}
/** `browser_open_folder`: folder rozszerzenia w Eksploratorze (tylko aplikacja). */
export async function browserOpenFolder(): Promise<void> {
  if (desktop) return invoke("browser_open_folder");
  throw new Error(
    "Folder rozszerzenia otwiera się w aplikacji Windows. W repozytorium to katalog browser-extension.",
  );
}

/** Przejście zgłoszenia Jiry (`jira::Transition`). */
export interface JiraTransition {
  id: string;
  name: string;
  toStatus: string;
  /** Kategoria statusu docelowego: `new`, `indeterminate`, `done`. */
  toCategory: string;
}
/** Pytanie o przesunięcie zgłoszenia przy starcie pracy (`jira_move::Prompt`). */
export interface JiraMovePrompt {
  /**
   * System zgłoszenia (PLAN_1.9.md, R4): Jira albo Redmine. Dla Redmine `key`
   * to `#1234`, przejścia to statusy docelowe, a `id` przejścia to `status_id`.
   * Brak pola = Jira (zgodność ze starszym zapisem podglądu).
   */
  system?: "jira" | "redmine";
  taskId: number;
  key: string;
  title: string;
  status: string;
  url: string;
  /** Status z ustawień (gdy przejście nie jest wybrane). */
  target: string;
  /** Przejścia do statusu „w toku”. */
  transitions: JiraTransition[];
  /** Wybrane przejście; `null` = lista do wyboru. */
  transitionId: string | null;
  /** Błąd ostatniej próby przesunięcia (baner z linkiem do zgłoszenia). */
  failure?: string;
}
/** Nazwa statusu docelowego w pytaniu (`Prompt::target_name`). */
export const moveTargetName = (p: JiraMovePrompt) =>
  p.transitions.find((t) => t.id === p.transitionId)?.toStatus || p.target;
/**
 * Wybór przejścia jak `jira_move::choose`: przejścia do kategorii
 * `indeterminate`; jedno wygrywa samo, przy kilku to z nazwą z ustawień (status
 * docelowy, potem nazwa przejścia), bez dopasowania `null` (lista do wyboru).
 */
export function chooseTransition(
  all: JiraTransition[],
  preferred: string,
): { transitions: JiraTransition[]; transitionId: string | null } {
  const transitions = all.filter((t) => t.toCategory === "indeterminate");
  const wanted = preferred.trim().toLowerCase();
  const by = (pick: (t: JiraTransition) => string) =>
    (wanted &&
      transitions.find((t) => pick(t).trim().toLowerCase() === wanted)?.id) ||
    null;
  const transitionId =
    transitions.length === 1
      ? transitions[0].id
      : transitions.length
        ? (by((t) => t.toStatus) ?? by((t) => t.name))
        : null;
  return { transitions, transitionId };
}
/** Komunikat przy wyłączonych zapisach (`jira::WRITE_DISABLED`). */
export const jiraWriteDisabled =
  "Zmiany w Jirze są wyłączone. Włączysz je w Ustawienia → Jira („Pozwól aplikacji zmieniać zgłoszenia w Jirze”).";
export const jiraMoveEvent = "jira-move-prompts";
/**
 * Symulowana Jira podglądu: ostatnio widziany pomiar i pytania; status zgłoszenia
 * to status z kafelka (pusty = „Do zrobienia”). `fail` (400 albo 403) symuluje
 * odrzucenie przejścia przez Jirę.
 */
export const previewJiraKey = "todo-track-preview-jira-v1";
interface PreviewJira {
  ready?: boolean;
  seen?: [number, number] | null;
  prompts?: JiraMovePrompt[];
  fail?: number;
}
/** Przepływ symulowanej Jiry: z każdego statusu do pozostałych. */
export const previewWorkflow: readonly JiraTransition[] = Object.freeze([
  {
    id: "11",
    name: "Do zrobienia",
    toStatus: "Do zrobienia",
    toCategory: "new",
  },
  {
    id: "21",
    name: "Rozpocznij",
    toStatus: "In Progress",
    toCategory: "indeterminate",
  },
  {
    id: "31",
    name: "Do code review",
    toStatus: "Code review",
    toCategory: "indeterminate",
  },
  { id: "41", name: "Zakończ", toStatus: "Gotowe", toCategory: "done" },
]);
/** Kategoria statusu w symulowanej Jirze. */
export function previewStatusCategory(status: string): string {
  const s = status.trim().toLowerCase();
  if (["", "do zrobienia", "to do", "open", "backlog", "nowe"].includes(s))
    return "new";
  if (["gotowe", "done", "zamknięte", "closed", "resolved"].includes(s))
    return "done";
  return "indeterminate";
}
const previewTransitionsFor = (status: string) =>
  previewWorkflow
    .filter((t) => t.toStatus.toLowerCase() !== status.trim().toLowerCase())
    .map((t) => ({ ...t }));
function previewJiraState(): PreviewJira {
  return previewJson(previewJiraKey) as PreviewJira;
}
function savePreviewJira(state: PreviewJira) {
  // Bez zmian nie zapisujemy: inne okna podglądu (widget) dostają `storage`.
  const raw = JSON.stringify(state);
  if (localStorage.getItem(previewJiraKey) !== raw)
    localStorage.setItem(previewJiraKey, raw);
}
/** Ustawienia zapisu do Jiry w podglądzie (wartości domyślne jak w Rust). */
function previewJiraSettings() {
  const s = previewJson(previewSettingsKey);
  return {
    write: s.jiraWrite !== false,
    ask: s.jiraAskMove !== false,
    status:
      typeof s.jiraMoveStatus === "string" && s.jiraMoveStatus.trim()
        ? s.jiraMoveStatus.trim()
        : "In Progress",
  };
}
/**
 * Odpowiednik wątku `jira_move::tick` i `check`: nowy pomiar innego zadania niż
 * poprzednio widziany, zadanie z kluczem w kategorii „Do zrobienia” → pytanie.
 * Pomiar trwający przy pierwszym odczycie nie jest nowym startem.
 */
async function previewObserveStart(): Promise<PreviewJira> {
  const state = previewJiraState();
  const data = await read();
  const active = data.sessions.find((s) => s.endedAt === null);
  const current: [number, number] | null = active
    ? [active.id, active.taskId]
    : null;
  const previous = state.seen ?? null;
  let fresh: number | null = null;
  if (!state.ready) state.ready = true;
  else if (
    current &&
    !(previous && (previous[0] === current[0] || previous[1] === current[1]))
  )
    fresh = current[1];
  state.seen = current;
  // Pytania tylko dla zadania aktywnego pomiaru (`jira_move::keep_current`).
  state.prompts = (state.prompts ?? []).filter(
    (p) => current !== null && p.taskId === current[1],
  );
  const task = data.tasks.find((t) => t.id === fresh);
  const settings = previewJiraSettings();
  if (task && !task.jiraKey && validRedmineId(task.redmineId)) {
    const prompt = previewRedmineStartPrompt(task, data);
    if (prompt) state.prompts = [prompt];
  }
  if (
    task?.jiraKey &&
    settings.write &&
    settings.ask &&
    !task.details?.jiraNoAsk
  ) {
    // Status odczytany z Jiry trafia na kafelek (`store_status`).
    const status = task.jiraStatus || "Do zrobienia";
    if (task.jiraStatus !== status) {
      task.jiraStatus = status;
      localStorage.setItem(key, JSON.stringify(data));
    }
    if (previewStatusCategory(status) === "new") {
      const choice = chooseTransition(
        previewTransitionsFor(status),
        settings.status,
      );
      if (choice.transitions.length)
        state.prompts = [
          {
            taskId: task.id,
            system: "jira",
            key: task.jiraKey,
            title: task.title,
            status,
            url: task.jiraUrl ?? `${previewJiraSite}/browse/${task.jiraKey}`,
            target: settings.status,
            ...choice,
          },
        ];
    }
  }
  savePreviewJira(state);
  return state;
}
function notifyPreviewJira() {
  window.dispatchEvent(new Event(jiraMoveEvent));
}
/** Pytania o przesunięcie zgłoszenia (`jira_move_prompts`). */
export async function readMovePrompts(): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_prompts");
  return (await previewObserveStart()).prompts ?? [];
}
/** „Nie teraz” (`jira_move_dismiss`): pytanie znika do kolejnego startu zadania. */
export async function dismissMovePrompt(
  taskId: number,
): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_dismiss", { taskId });
  const state = previewJiraState();
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return state.prompts;
}
/** „Nie pytaj dla tego zadania” i „Pytaj znowu” (`jira_no_ask`). */
export async function setJiraNoAsk(
  taskId: number,
  noAsk: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_no_ask", { taskId, noAsk });
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), jiraNoAsk: noAsk };
  localStorage.setItem(key, JSON.stringify(data));
  if (noAsk) await dismissMovePrompt(taskId);
  return data;
}
/** Zadanie z kluczem dla zapisu w podglądzie (`jira_move::target`). */
async function previewJiraTask(taskId: number) {
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  if (!task?.jiraKey)
    throw new Error("To zadanie nie jest powiązane ze zgłoszeniem Jiry.");
  return { data, task };
}
/** Dostępne przejścia zgłoszenia zadania (`jira_transitions`). */
export async function jiraTransitions(
  taskId: number,
): Promise<JiraTransition[]> {
  if (desktop) return invoke("jira_transitions", { taskId });
  const { task } = await previewJiraTask(taskId);
  return previewTransitionsFor(task.jiraStatus || "Do zrobienia");
}
/**
 * Przejście zgłoszenia (`jira_transition`): status na kafelku po odświeżeniu,
 * pytanie zadania znika. Błąd symulowanej Jiry jak `jira::write_error`.
 */
export async function jiraTransition(
  taskId: number,
  transitionId: string,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_transition", { taskId, transitionId });
  const { data, task } = await previewJiraTask(taskId);
  const state = previewJiraState();
  // Błąd zostaje przy pytaniu zadania (`jira_move::mark_failure`).
  const fail = (message: string) => {
    if ((state.prompts ?? []).some((p) => p.taskId === taskId)) {
      state.prompts = (state.prompts ?? []).map((p) =>
        p.taskId === taskId ? { ...p, failure: message } : p,
      );
      savePreviewJira(state);
      notifyPreviewJira();
    }
    return new Error(message);
  };
  if (state.fail === 403)
    throw fail("Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
  if (state.fail === 400)
    throw fail(
      "Jira odrzuciła zmianę. Przejście może wymagać wypełnienia pól na ekranie w Jirze — zrób je w przeglądarce.",
    );
  const transition = previewTransitionsFor(
    task.jiraStatus || "Do zrobienia",
  ).find((t) => t.id === transitionId);
  if (!transition)
    throw fail("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  task.jiraStatus = transition.toStatus;
  localStorage.setItem(key, JSON.stringify(data));
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return data;
}
/** Nasłuch zmian pytań (zdarzenie Rust, w podglądzie zdarzenie okna i `storage`). */
export async function onMovePromptsChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(jiraMoveEvent, handler);
  const storage = (e: StorageEvent) => {
    if (e.key === previewJiraKey || e.key === previewRedmineKey || e.key === key)
      handler();
  };
  window.addEventListener(jiraMoveEvent, handler);
  window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(jiraMoveEvent, handler);
    window.removeEventListener("storage", storage);
  };
}

// ---------------------------------------------------------------------------
// J2: Jira z poziomu Todo Track (tablica, przejścia, przypisanie, komentarze,
// nazwa gałęzi, nowe zgłoszenia). W aplikacji komendy z `jira.rs` i
// `jira_board.rs`; w podglądzie symulowana Jira w localStorage
// (`previewJiraBoardKey`) z tymi samymi regułami (`columnFor`, `decideMove`,
// `branchName`). Błąd zapisu symuluje `fail` (400 albo 403) ze stanu J1.
// ---------------------------------------------------------------------------

export interface JiraBoardRef {
  id: number;
  name: string;
}
/** `jira::Settings`: konto i wybrane tablice. */
export interface JiraAccount {
  config: {
    site: string;
    email: string;
    displayName: string;
    boards: JiraBoardRef[];
  };
  connected: boolean;
}
/** Zgłoszenie listy „Moje” (`jira::Issue`). */
export interface JiraListIssue {
  key: string;
  summary: string;
  status: string;
  boards: string[];
  url: string;
  /** Wartości pól z ustawień `jiraCardFields`. */
  fields: JiraFieldValue[];
}
export interface JiraColumn {
  name: string;
  statusIds: string[];
}
export interface JiraPerson {
  accountId: string;
  displayName: string;
}
/** Karta tablicy (`jira_board::BoardIssue`). */
export interface JiraBoardIssue {
  key: string;
  summary: string;
  status: string;
  statusId: string;
  category: string;
  issueType: string;
  priority: string;
  assignee: JiraPerson | null;
  url: string;
  column: number;
  /** Wartości pól z ustawień `jiraCardFields`. */
  fields: JiraFieldValue[];
}
export interface JiraBoardView {
  boardId: number;
  project: string;
  columns: JiraColumn[];
  issues: JiraBoardIssue[];
  truncated: boolean;
  limit: number;
  layoutAt: number;
}
/** Przejście z polami ekranu (`jira_board::MoveOption`). */
export interface JiraMoveOption {
  id: string;
  name: string;
  toStatus: string;
  toStatusId: string;
  toCategory: string;
  required: string[];
}
export type JiraMoveOutcome =
  | {
      result: "moved";
      status: string;
      statusId: string;
      category: string;
      column: number;
    }
  | { result: "choose"; options: JiraMoveOption[] }
  | { result: "screen"; url: string; fields: string[]; message: string };
export interface JiraIssueType {
  id: string;
  name: string;
}
export interface JiraProject {
  key: string;
  boards: string[];
}
export interface JiraTypeFields {
  extraRequired: string[];
  createUrl: string;
}
export interface JiraNewIssue {
  project: string;
  issueTypeId: string;
  summary: string;
  description: string;
  assignMe: boolean;
  createTask: boolean;
  plannedDay: string;
}
export interface JiraCreated {
  key: string;
  url: string;
  snapshot: Snapshot | null;
  /** Zgłoszenie utworzone, ale zadanie w Todo Track nie powstało. */
  warning?: string | null;
}
export type JiraScope = "mine" | "all";

/** Najwięcej kart na tablicy (`jira_board::ISSUE_LIMIT`). */
export const jiraIssueLimit = 300;
/** Najdłuższy komentarz (`jira_board::COMMENT_LIMIT`). */
export const jiraCommentLimit = 5000;

/**
 * Kolumna statusu jak `jira_board::column_for`: kolumna ze statusem; inaczej
 * pierwsza kolumna ze statusem tej samej kategorii; inaczej „Do zrobienia” →
 * pierwsza, „Gotowe” → ostatnia, „W toku” → druga (przy 3+ kolumnach).
 */
export function columnFor(
  statusId: string,
  category: string,
  columns: JiraColumn[],
  categories: Record<string, string>,
): number {
  if (!columns.length) return 0;
  const direct = columns.findIndex((c) => c.statusIds.includes(statusId));
  if (direct >= 0) return direct;
  if (category) {
    const same = columns.findIndex((c) =>
      c.statusIds.some((s) => categories[s] === category),
    );
    if (same >= 0) return same;
  }
  if (category === "new") return 0;
  if (category === "done") return columns.length - 1;
  return columns.length >= 3 ? 1 : 0;
}
export type JiraMoveDecision =
  | { kind: "perform" | "screen"; option: JiraMoveOption }
  | { kind: "choose"; options: JiraMoveOption[] }
  | { kind: "none" };
/** Wybór przejścia do kolumny jak `jira_board::decide`. */
export function decideMove(
  options: JiraMoveOption[],
  target: number,
  columns: JiraColumn[],
  categories: Record<string, string>,
  chosen: string | null,
): JiraMoveDecision {
  const candidates = options.filter(
    (o) => columnFor(o.toStatusId, o.toCategory, columns, categories) === target,
  );
  let pick: JiraMoveOption | undefined;
  if (chosen !== null) {
    pick = candidates.find((o) => o.id === chosen);
    if (!pick) throw new Error("To przejście nie prowadzi do wybranej kolumny.");
  } else if (!candidates.length) return { kind: "none" };
  else if (candidates.length > 1) return { kind: "choose", options: candidates };
  else pick = candidates[0];
  return { kind: pick.required.length ? "screen" : "perform", option: pick };
}
const plainLetters: Record<string, string> = {
  ą: "a",
  á: "a",
  ä: "a",
  à: "a",
  â: "a",
  ć: "c",
  č: "c",
  ç: "c",
  ę: "e",
  é: "e",
  ě: "e",
  è: "e",
  ë: "e",
  ê: "e",
  ł: "l",
  ń: "n",
  ň: "n",
  ó: "o",
  ö: "o",
  ô: "o",
  ò: "o",
  ś: "s",
  š: "s",
  ź: "z",
  ż: "z",
  ž: "z",
  í: "i",
  ì: "i",
  î: "i",
  ï: "i",
  ú: "u",
  ü: "u",
  ů: "u",
  ù: "u",
  û: "u",
  ř: "r",
  ý: "y",
  ť: "t",
  ď: "d",
  ß: "ss",
};
/** Nazwa gałęzi jak `jira_board::branch_name` (część bez prefiksu do 50 znaków). */
export function branchName(prefix: string, key: string, title: string): string {
  let slug = "";
  for (const c of title.toLowerCase()) {
    if (/^[a-z0-9]$/.test(c)) slug += c;
    else if (plainLetters[c]) slug += plainLetters[c];
    else if (!slug.endsWith("-")) slug += "-";
  }
  slug = slug.replace(/^-+|-+$/g, "");
  const full = slug ? `${key}-${slug}` : key;
  return prefix + full.slice(0, 50).replace(/-+$/, "");
}
/** Inicjały osoby na karcie. */
export const initials = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toLocaleUpperCase("pl") ?? "")
    .join("") || "?";

const jiraKeyPattern = /^[A-Z0-9_]+-\d+$/;
function checkKey(issueKey: string) {
  if (!jiraKeyPattern.test(issueKey) || issueKey.length > 100)
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
}

/** Symulowana Jira podglądu (J2). */
export const previewJiraBoardKey = "todo-track-preview-jira-board-v1";
/** Linki otwarte „w przeglądarce” w podglądzie (do sprawdzenia w testach). */
export const previewJiraOpenedKey = "todo-track-preview-jira-opened-v1";
interface PreviewIssue {
  key: string;
  summary: string;
  status: string;
  issueType: string;
  priority: string;
  assignee: JiraPerson | null;
  boards: number[];
  /** Inne pola (identyfikator → tekst), np. etykiety, sprint, pola niestandardowe. */
  fields?: Record<string, string>;
}
interface PreviewBoardState {
  issues?: PreviewIssue[];
  comments?: Record<string, string[]>;
  next?: number;
  boards?: JiraBoardRef[];
  /** Ostatni priorytet z Jiry przeniesiony na zadanie (klucz → nazwa). */
  priorities?: Record<string, string>;
}
/** Lista pól symulowanej Jiry (`/rest/api/3/field`), jak `jira_fields::FieldInfo`. */
export const previewFieldCatalog: readonly JiraFieldInfo[] = Object.freeze([
  { id: "assignee", name: "Przypisany", custom: false, kind: "user" },
  { id: "components", name: "Komponenty", custom: false, kind: "array:component" },
  { id: "duedate", name: "Termin", custom: false, kind: "date" },
  { id: "labels", name: "Etykiety", custom: false, kind: "array:string" },
  { id: "priority", name: "Priorytet", custom: false, kind: "priority" },
  { id: "reporter", name: "Zgłaszający", custom: false, kind: "user" },
  { id: "customfield_10016", name: "Story points", custom: true, kind: "number" },
  { id: "customfield_10020", name: "Sprint", custom: true, kind: "array:json" },
  { id: "customfield_10031", name: "Środowisko", custom: true, kind: "option" },
]);
/** Polskie nazwy pól systemowych bez pobranej listy (`jira_fields::system_name`). */
const systemFieldNames: Record<string, string> = {
  labels: "Etykiety",
  assignee: "Przypisany",
  priority: "Priorytet",
  reporter: "Zgłaszający",
  creator: "Autor",
  duedate: "Termin",
  components: "Komponenty",
  fixVersions: "Wersje naprawy",
  versions: "Wersje",
  issuetype: "Typ",
  resolution: "Rozwiązanie",
  created: "Utworzono",
  updated: "Zaktualizowano",
  parent: "Nadrzędne",
  project: "Projekt",
  environment: "Środowisko",
};
/** Nazwa pola: z listy pól, inaczej polska nazwa systemowa, inaczej identyfikator. */
export function jiraFieldName(id: string, catalog?: readonly JiraFieldInfo[]): string {
  return catalog?.find((f) => f.id === id)?.name ?? systemFieldNames[id] ?? id;
}
/** Pasmo priorytetu zadania dla nazwy priorytetu Jiry (`jira_fields::priority_band`). */
export function priorityBand(name: string): number | null {
  const n = name.trim().toLowerCase();
  if (!n) return null;
  if (
    // „immediate” i „natychmiastowy” to priorytety Redmine (PLAN_1.9.md, R2).
    ["highest", "high", "critical", "blocker", "major", "urgent", "immediate", "najwyższy", "wysoki", "krytyczny", "pilny", "natychmiastowy"].includes(n)
  )
    return 1;
  if (["lowest", "low", "minor", "trivial", "najniższy", "niski", "drobny"].includes(n))
    return -1;
  if (["medium", "normal", "średni", "normalny", "zwykły"].includes(n)) return 0;
  return null;
}
const bandName = (band: number) =>
  band > 0 ? "High" : band < 0 ? "Low" : "Medium";
/** Pola z ustawień w podglądzie (jak `jira_fields::card_fields`). */
function previewCardFields(): string[] {
  const raw = previewJson(previewSettingsKey).jiraCardFields;
  return Array.isArray(raw)
    ? raw.filter((id): id is string => typeof id === "string")
    : ["labels", "assignee"];
}
/** Wartości wybranych pól zgłoszenia symulowanej Jiry (tylko niepuste). */
function previewIssueFields(
  issue: PreviewIssue,
  ids: string[] = previewCardFields(),
): JiraFieldValue[] {
  const all: Record<string, string> = {
    ...(issue.fields ?? {}),
    assignee: issue.assignee?.displayName ?? "",
    priority: issue.priority,
  };
  return ids
    .filter((id) => all[id])
    .map((id) => ({
      id,
      name: jiraFieldName(id, previewFieldCatalog),
      value: all[id],
    }));
}
/**
 * Pola Jiry na zadaniach z kluczem (jak `db::snapshot` z `jira_issue_fields`):
 * tylko ze zgłoszeń zapamiętanych w symulowanej tablicy.
 */
function attachPreviewJiraFields(data: Snapshot) {
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  const issues = Array.isArray(state.issues) ? state.issues : [];
  const ids = previewCardFields();
  for (const task of [...data.tasks, ...(data.trash ?? [])]) {
    if (!task.jiraKey) continue;
    const issue = issues.find((i) => i.key === task.jiraKey);
    task.jiraFields = issue ? previewIssueFields(issue, ids) : [];
  }
}
export const previewJiraMe: Readonly<JiraPerson> = Object.freeze({
  accountId: "preview-me",
  displayName: "Jan Kowalski",
});
const previewPeople: JiraPerson[] = [
  { ...previewJiraMe },
  { accountId: "preview-anna", displayName: "Anna Nowak" },
  { accountId: "preview-piotr", displayName: "Piotr Wiśniewski" },
];
const previewBoards: JiraBoardRef[] = [
  { id: 1, name: "WMS" },
  { id: 2, name: "Foundation" },
];
const previewBoardProject: Record<number, string> = { 1: "WMS", 2: "FND" };
/** Statusy symulowanej Jiry; „Testy” nie ma kolumny (trafia według kategorii). */
const previewStatuses = [
  { id: "1", name: "Do zrobienia", category: "new" },
  { id: "3", name: "In Progress", category: "indeterminate" },
  { id: "4", name: "Code review", category: "indeterminate" },
  { id: "5", name: "Gotowe", category: "done" },
  { id: "6", name: "Testy", category: "indeterminate" },
];
const previewColumns: JiraColumn[] = [
  { name: "Do zrobienia", statusIds: ["1"] },
  { name: "W toku", statusIds: ["3"] },
  { name: "Code review", statusIds: ["4"] },
  { name: "Gotowe", statusIds: ["5"] },
];
const previewCategories: Record<string, string> = Object.fromEntries(
  previewStatuses.map((s) => [s.id, s.category]),
);
function previewStatusId(name: string): string {
  const n = name.trim().toLowerCase();
  return previewStatuses.find((s) => s.name.toLowerCase() === n)?.id ?? "";
}
/** Przejścia tablicy: przepływ z J1, „Do testów” i „Zakończ” z polem Rozwiązanie. */
function previewMoveOptions(status: string): JiraMoveOption[] {
  const all: JiraMoveOption[] = [
    ...previewWorkflow.map((t) => ({
      ...t,
      toStatusId: previewStatusId(t.toStatus),
      required: t.id === "41" ? ["Rozwiązanie"] : [],
    })),
    {
      id: "51",
      name: "Do testów",
      toStatus: "Testy",
      toStatusId: "6",
      toCategory: "indeterminate",
      required: [],
    },
  ];
  return all.filter(
    (t) => t.toStatus.toLowerCase() !== status.trim().toLowerCase(),
  );
}
const previewTypes: (JiraIssueType & { required: string[] })[] = [
  { id: "10001", name: "Zadanie", required: [] },
  { id: "10002", name: "Błąd", required: ["Środowisko"] },
  { id: "10004", name: "Historia", required: [] },
];
const previewIssueSeed = (): PreviewIssue[] => [
  {
    key: "WMS-31",
    summary: "Obsługa przyjęcia towaru",
    status: "Do zrobienia",
    issueType: "Historia",
    priority: "High",
    assignee: { ...previewJiraMe },
    boards: [1, 2],
    fields: {
      labels: "magazyn, pilne",
      duedate: "23.10.2026",
      customfield_10020: "Sprint 42",
      customfield_10031: "Produkcja",
    },
  },
  {
    key: "WMS-40",
    summary: "Raport rozbieżności magazynowych",
    status: "Code review",
    issueType: "Błąd",
    priority: "High",
    assignee: { ...previewPeople[1] },
    boards: [1],
    fields: { labels: "raporty", customfield_10020: "Sprint 42" },
  },
  {
    key: "WMS-41",
    summary: "Etykiety dla kurierów",
    status: "Do zrobienia",
    issueType: "Zadanie",
    priority: "Low",
    assignee: null,
    boards: [1],
  },
  {
    key: "WMS-44",
    summary: "Inwentaryzacja cząstkowa",
    status: "Testy",
    issueType: "Zadanie",
    priority: "Medium",
    assignee: { ...previewPeople[2] },
    boards: [1],
  },
  {
    key: "FND-3",
    summary: "Aktualizacja bibliotek",
    status: "In Progress",
    issueType: "Zadanie",
    priority: "Medium",
    assignee: { ...previewJiraMe },
    boards: [2],
  },
];
/**
 * Stan symulowanej tablicy: zgłoszenia z przykładu i z zadań z kluczem
 * (status z kafelka wygrywa, bo zmienia go też J1).
 */
async function previewBoardState(): Promise<{
  state: PreviewBoardState;
  data: Snapshot;
  issues: PreviewIssue[];
}> {
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  const data = await read();
  const issues = Array.isArray(state.issues) ? state.issues : previewIssueSeed();
  for (const task of [...data.tasks, ...(data.trash ?? [])]) {
    if (!task.jiraKey) continue;
    const issue = issues.find((i) => i.key === task.jiraKey);
    if (!issue)
      issues.push({
        key: task.jiraKey,
        summary: task.title,
        status: task.jiraStatus || "Do zrobienia",
        issueType: "Zadanie",
        priority: bandName(task.details?.priority ?? 0),
        assignee: { ...previewJiraMe },
        boards: [1],
      });
    else if (task.jiraStatus) issue.status = task.jiraStatus;
  }
  state.issues = issues;
  // Priorytet z Jiry na zadanie przy pierwszym pobraniu i każdej zmianie
  // (jak `jira_fields::save_fields`); ręczne pasmo zostaje, dopóki Jira go nie zmieni.
  const applied = (state.priorities ??= {});
  let changed = false;
  for (const task of data.tasks) {
    const issue = task.jiraKey
      ? issues.find((i) => i.key === task.jiraKey)
      : undefined;
    if (!issue || applied[issue.key] === issue.priority) continue;
    applied[issue.key] = issue.priority;
    const band = priorityBand(issue.priority);
    if (band !== null && (task.details?.priority ?? 0) !== band) {
      task.details = { ...detailsOf(task), priority: band };
      changed = true;
    }
  }
  if (changed) {
    localStorage.setItem(key, JSON.stringify(data));
    notifyPreviewData();
  }
  return { state, data, issues };
}
/**
 * Odpowiednik zdarzenia Rust `data-changed` w podglądzie (to samo okno):
 * App odczytuje dane na nowo po zmianach z symulowanej Jiry.
 */
export const previewDataEvent = "preview-data-changed";
function notifyPreviewData() {
  window.dispatchEvent(new Event(previewDataEvent));
}
function savePreviewBoard(state: PreviewBoardState) {
  localStorage.setItem(previewJiraBoardKey, JSON.stringify(state));
}
const previewSelectedBoards = (state: PreviewBoardState) =>
  Array.isArray(state.boards)
    ? state.boards
    : previewBoards.map((b) => ({ ...b }));
/** Zapis do symulowanej Jiry: główny przełącznik i błąd `fail` (jak `jira::write`). */
function previewWrite() {
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const fail = previewJiraState().fail;
  if (fail === 403)
    throw new Error("Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
  if (fail === 400)
    throw new Error(
      "Jira odrzuciła zmianę. Przejście może wymagać wypełnienia pól na ekranie w Jirze — zrób je w przeglądarce.",
    );
}
const previewIssueUrl = (issueKey: string) =>
  `${previewJiraSite}/browse/${issueKey}`;
function previewCategory(status: string) {
  const id = previewStatusId(status);
  return id ? previewCategories[id] : previewStatusCategory(status);
}

/** `jira_settings` (w podglądzie połączone konto przykładowe). */
export async function jiraAccount(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_settings");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  return {
    config: {
      site: previewJiraSite,
      email: "jan.kowalski@firma.example",
      displayName: previewJiraMe.displayName,
      boards: previewSelectedBoards(state),
    },
    connected: true,
  };
}
const previewOnlyAccount =
  "W podglądzie Jira jest symulowana. Konto łączysz w aplikacji Windows.";
export async function jiraConnect(
  site: string,
  email: string,
  apiToken: string,
): Promise<JiraAccount> {
  if (desktop) return invoke("jira_connect", { site, email, apiToken });
  throw new Error(previewOnlyAccount);
}
export async function jiraDisconnect(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_disconnect");
  throw new Error(previewOnlyAccount);
}
/** `jira_boards`: wyszukiwanie tablic. */
export async function jiraBoards(
  name: string,
  startAt: number,
): Promise<{ values: JiraBoardRef[]; nextStart: number | null }> {
  if (desktop) return invoke("jira_boards", { name, startAt });
  const q = name.trim().toLocaleLowerCase("pl");
  return {
    values: previewBoards.filter((b) =>
      b.name.toLocaleLowerCase("pl").includes(q),
    ),
    nextStart: null,
  };
}
/** `jira_fields`: lista pól Jiry do wyboru (w podglądzie stała lista). */
export async function jiraFields(refresh: boolean): Promise<JiraFieldInfo[]> {
  if (desktop) return invoke("jira_fields", { refresh });
  return previewFieldCatalog.map((f) => ({ ...f }));
}
/**
 * `jira_refresh_fields`: status i pola zgłoszeń otwartych zadań od razu po
 * zmianie listy pól; zwraca liczbę odświeżonych zgłoszeń. W podglądzie
 * wartości biorą się z symulowanej tablicy przy odczycie.
 */
export async function jiraRefreshFields(): Promise<number> {
  if (desktop) return invoke("jira_refresh_fields");
  const { state, data } = await previewBoardState();
  savePreviewBoard(state);
  notifyPreviewData();
  return data.tasks.filter((t) => t.jiraKey && !t.done).length;
}
/** `jira_select_boards`: wybrane tablice (najwyżej 20). */
export async function jiraSelectBoards(boards: JiraBoardRef[]): Promise<void> {
  if (desktop) return invoke("jira_select_boards", { boards });
  if (boards.length > 20) throw new Error("Wybierz maksymalnie 20 tablic.");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  state.boards = boards.map((b) => ({ id: b.id, name: b.name }));
  savePreviewBoard(state);
}
/** `jira_issues`: lista „Przypisane do mnie”. */
export async function jiraIssues(
  openOnly: boolean,
): Promise<{ issues: JiraListIssue[]; warnings: string[] }> {
  if (desktop) return invoke("jira_issues", { openOnly });
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  const boards = previewSelectedBoards(state);
  if (!boards.length) throw new Error("Najpierw wybierz tablice.");
  return {
    issues: issues
      .filter((i) => i.assignee?.accountId === previewJiraMe.accountId)
      .filter((i) => !openOnly || previewCategory(i.status) !== "done")
      .map((i) => ({
        key: i.key,
        summary: i.summary,
        status: i.status,
        boards: boards
          .filter((b) => i.boards.includes(b.id))
          .map((b) => b.name),
        url: previewIssueUrl(i.key),
        fields: previewIssueFields(i),
      }))
      .filter((i) => i.boards.length)
      .sort((a, b) => a.key.localeCompare(b.key, "pl", { numeric: true })),
    warnings: [],
  };
}
/** `jira_start`: zadanie zgłoszenia (nowe albo z kosza/archiwum) i jeden pomiar. */
export async function jiraStart(issueKey: string): Promise<Snapshot> {
  if (desktop) return invoke("jira_start", { key: issueKey });
  checkKey(issueKey);
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue) throw new Error("Odśwież listę zgłoszeń przed rozpoczęciem.");
  const data = await read();
  const now = Date.now();
  let task = data.tasks.find((t) => t.jiraKey === issueKey);
  const trashed = data.trash?.find((t) => t.jiraKey === issueKey);
  if (!task && trashed) {
    data.trash = data.trash?.filter((t) => t !== trashed);
    data.sessions.push(
      ...(data.trashSessions ?? []).filter((s) => s.taskId === trashed.id),
    );
    data.trashSessions = data.trashSessions?.filter(
      (s) => s.taskId !== trashed.id,
    );
    data.tasks.unshift(trashed);
    task = trashed;
  }
  if (!task) {
    task = {
      id:
        Math.max(
          0,
          ...data.tasks.map((t) => t.id),
          ...(data.trash ?? []).map((t) => t.id),
        ) + 1,
      title: [...issue.summary].slice(0, 240).join(""),
      done: false,
      createdAt: now,
      jiraKey: issueKey,
      jiraUrl: previewIssueUrl(issueKey),
      jiraStatus: issue.status,
    };
    // Jak `jira::start_issue`: nowe zadanie z pasmem priorytetu zgłoszenia
    // i polami z ustawień (w aplikacji dokłada je `db::snapshot`).
    const band = priorityBand(issue.priority);
    if (band !== null) task.details = { ...detailsOf(task), priority: band };
    task.jiraFields = previewIssueFields(issue);
    data.tasks.unshift(task);
  }
  const started = task;
  started.done = false;
  started.archived = false;
  started.details = unblocked(started);
  if (
    !data.sessions.some((s) => s.taskId === started.id && s.endedAt === null)
  ) {
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
    data.sessions.unshift({
      id:
        Math.max(
          0,
          ...data.sessions.map((s) => s.id),
          ...(data.trashSessions ?? []).map((s) => s.id),
        ) + 1,
      taskId: started.id,
      startedAt: now,
      endedAt: null,
    });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** `jira_open`: zgłoszenie w przeglądarce (w podglądzie tylko zapisany link). */
export async function jiraOpen(url: string): Promise<void> {
  if (desktop) return invoke("jira_open", { url });
  try {
    const opened = JSON.parse(
      localStorage.getItem(previewJiraOpenedKey) ?? "[]",
    );
    localStorage.setItem(
      previewJiraOpenedKey,
      JSON.stringify(
        [...(Array.isArray(opened) ? opened : []), url].slice(-20),
      ),
    );
  } catch {
    /* tylko ślad dla testów */
  }
}
/** `jira_board`: kolumny i karty tablicy. */
export async function jiraBoard(
  boardId: number,
  scope: JiraScope,
  openOnly: boolean,
  refresh: boolean,
): Promise<JiraBoardView> {
  if (desktop)
    return invoke("jira_board", { boardId, scope, openOnly, refresh });
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  if (!previewSelectedBoards(state).some((b) => b.id === boardId))
    throw new Error(
      "Tej tablicy nie ma wśród wybranych. Wybierz ją w widoku Jira.",
    );
  const cards = issues
    .filter((i) => i.boards.includes(boardId))
    .filter((i) =>
      scope === "mine"
        ? i.assignee?.accountId === previewJiraMe.accountId &&
          (!openOnly || previewCategory(i.status) !== "done")
        : previewCategory(i.status) !== "done",
    )
    .map((i): JiraBoardIssue => {
      const statusId = previewStatusId(i.status);
      const category = previewCategory(i.status);
      return {
        key: i.key,
        summary: i.summary,
        status: i.status,
        statusId,
        category,
        issueType: i.issueType,
        priority: i.priority,
        assignee: i.assignee ? { ...i.assignee } : null,
        url: previewIssueUrl(i.key),
        column: columnFor(
          statusId,
          category,
          previewColumns,
          previewCategories,
        ),
        fields: previewIssueFields(i),
      };
    });
  return {
    boardId,
    project: previewBoardProject[boardId] ?? "",
    columns: previewColumns.map((c) => ({
      ...c,
      statusIds: [...c.statusIds],
    })),
    issues: cards.slice(0, jiraIssueLimit),
    truncated: cards.length > jiraIssueLimit,
    limit: jiraIssueLimit,
    layoutAt: Date.now(),
  };
}
/** Status zgłoszenia w symulowanej Jirze i na kafelkach zadań z tym kluczem. */
async function previewSetStatus(issueKey: string, status: string) {
  const { state, data, issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (issue) issue.status = status;
  for (const task of [...data.tasks, ...(data.trash ?? [])])
    if (task.jiraKey === issueKey) task.jiraStatus = status;
  savePreviewBoard(state);
  localStorage.setItem(key, JSON.stringify(data));
}
/** `jira_board_move`: przeciągnięcie karty i „Przesuń do…”. */
export async function jiraBoardMove(
  boardId: number,
  issueKey: string,
  column: number,
  transitionId: string | null,
): Promise<JiraMoveOutcome> {
  if (desktop)
    return invoke("jira_board_move", {
      boardId,
      key: issueKey,
      column,
      transitionId,
    });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const { issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue)
    throw new Error("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  const target = previewColumns[column];
  if (!target) throw new Error("Nie ma takiej kolumny na tablicy.");
  const decision = decideMove(
    previewMoveOptions(issue.status),
    column,
    previewColumns,
    previewCategories,
    transitionId,
  );
  if (decision.kind === "none")
    throw new Error(
      `Jira nie ma przejścia ${issueKey} do kolumny „${target.name}”. Zgłoszenie zostaje w poprzedniej kolumnie.`,
    );
  if (decision.kind === "choose")
    return { result: "choose", options: decision.options };
  const o = decision.option;
  if (decision.kind === "screen")
    return {
      result: "screen",
      url: previewIssueUrl(issueKey),
      fields: o.required,
      message: `Przejście „${o.name}” wymaga wypełnienia pól w Jirze (${o.required.join(", ")}). Otwieram ${issueKey} w przeglądarce — dokończ zmianę tam.`,
    };
  previewWrite();
  await previewSetStatus(issueKey, o.toStatus);
  return {
    result: "moved",
    status: o.toStatus,
    statusId: o.toStatusId,
    category: o.toCategory,
    column: columnFor(
      o.toStatusId,
      o.toCategory,
      previewColumns,
      previewCategories,
    ),
  };
}
/** `jira_assignable`: osoby do przypisania. */
export async function jiraAssignable(
  issueKey: string,
  query: string,
): Promise<JiraPerson[]> {
  if (desktop) return invoke("jira_assignable", { key: issueKey, query });
  checkKey(issueKey);
  const q = query.trim().toLocaleLowerCase("pl");
  if ([...q].length > 100) throw new Error("Wpisz najwyżej 100 znaków.");
  return previewPeople
    .filter((p) => p.displayName.toLocaleLowerCase("pl").includes(q))
    .map((p) => ({ ...p }));
}
/** `jira_assign`: „Przypisz do mnie” (`accountId` = null) i „Zmień osobę…”. */
export async function jiraAssign(
  issueKey: string,
  accountId: string | null,
): Promise<JiraPerson> {
  if (desktop) return invoke("jira_assign", { key: issueKey, accountId });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const who =
    accountId === null
      ? previewJiraMe
      : previewPeople.find((p) => p.accountId === accountId);
  if (!who) throw new Error("Niepoprawna osoba w Jirze.");
  previewWrite();
  const { state, issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue)
    throw new Error("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  issue.assignee = { ...who };
  savePreviewBoard(state);
  return { ...who };
}
/** `jira_comment`: komentarz zwykłym tekstem (w Jirze jako ADF). */
export async function jiraComment(
  issueKey: string,
  text: string,
): Promise<void> {
  if (desktop) return invoke("jira_comment", { key: issueKey, text });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const body = text.replace(/\r\n?/g, "\n").trim();
  if (!body) throw new Error("Wpisz treść komentarza.");
  if ([...body].length > jiraCommentLimit)
    throw new Error(
      `Komentarz może mieć najwyżej ${jiraCommentLimit} znaków.`,
    );
  previewWrite();
  const { state } = await previewBoardState();
  const comments = (state.comments ??= {});
  comments[issueKey] = [...(comments[issueKey] ?? []), body];
  savePreviewBoard(state);
}
/** `jira_branch_name`: nazwa gałęzi z prefiksem z ustawień. */
export async function jiraBranchName(
  issueKey: string,
  title: string,
): Promise<string> {
  if (desktop) return invoke("jira_branch_name", { key: issueKey, title });
  checkKey(issueKey);
  const s = previewJson(previewSettingsKey);
  const prefix =
    typeof s.jiraBranchPrefix === "string" ? s.jiraBranchPrefix : "feature/";
  return branchName(prefix, issueKey, title);
}
/** `jira_projects`: projekty wybranych tablic. */
export async function jiraProjects(): Promise<JiraProject[]> {
  if (desktop) return invoke("jira_projects");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  const out: JiraProject[] = [];
  for (const b of previewSelectedBoards(state)) {
    const project = previewBoardProject[b.id];
    if (!project) continue;
    const known = out.find((p) => p.key === project);
    if (known) known.boards.push(b.name);
    else out.push({ key: project, boards: [b.name] });
  }
  if (!out.length)
    throw new Error("Wybrane tablice nie należą do projektu Jiry.");
  return out;
}
const projectPattern = /^[A-Z][A-Z0-9_]{0,19}$/;
/** `jira_issue_types`: typy zgłoszeń projektu. */
export async function jiraIssueTypes(
  project: string,
): Promise<JiraIssueType[]> {
  if (desktop) return invoke("jira_issue_types", { project });
  if (!projectPattern.test(project)) throw new Error("Wybierz projekt w Jirze.");
  return previewTypes.map(({ id, name }) => ({ id, name }));
}
/** `jira_issue_type_fields`: wymagane pola spoza formularza. */
export async function jiraIssueTypeFields(
  project: string,
  issueTypeId: string,
): Promise<JiraTypeFields> {
  if (desktop)
    return invoke("jira_issue_type_fields", { project, issueTypeId });
  const type = previewTypes.find((t) => t.id === issueTypeId);
  if (!type || !projectPattern.test(project))
    throw new Error("Wybierz projekt i typ zgłoszenia.");
  return {
    extraRequired: [...type.required],
    createUrl: `${previewJiraSite}/secure/CreateIssue!default.jspa`,
  };
}
/** Walidacja formularza jak `jira_board::validate_new_issue`; zwraca tytuł. */
export function validateNewIssue(n: JiraNewIssue): string {
  if (!projectPattern.test(n.project))
    throw new Error("Wybierz projekt w Jirze.");
  if (!/^\d{1,20}$/.test(n.issueTypeId))
    throw new Error("Wybierz typ zgłoszenia.");
  const summary = n.summary.trim();
  if (!summary || [...summary].length > 255 || /[\r\n]/.test(summary))
    throw new Error(
      "Tytuł zgłoszenia musi mieć od 1 do 255 znaków w jednej linii.",
    );
  if ([...n.description].length > jiraCommentLimit)
    throw new Error(`Opis może mieć najwyżej ${jiraCommentLimit} znaków.`);
  if (n.plannedDay && !/^\d{4}-\d{2}-\d{2}$/.test(n.plannedDay))
    throw new Error("Dzień planu musi mieć format RRRR-MM-DD.");
  return summary;
}
/** `jira_create`: nowe zgłoszenie i opcjonalnie zadanie w Todo Track. */
export async function jiraCreate(issue: JiraNewIssue): Promise<JiraCreated> {
  if (desktop) return invoke("jira_create", { issue });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const summary = validateNewIssue(issue);
  const type = previewTypes.find((t) => t.id === issue.issueTypeId);
  if (!type) throw new Error("Wybierz typ zgłoszenia.");
  if (type.required.length)
    throw new Error(
      `Ten typ zgłoszenia wymaga pól, których Todo Track nie wypełnia (${type.required.join(", ")}). Utwórz zgłoszenie w Jirze.`,
    );
  previewWrite();
  const { state, issues } = await previewBoardState();
  const number = state.next ?? 100;
  state.next = number + 1;
  const issueKey = `${issue.project}-${number}`;
  issues.push({
    key: issueKey,
    summary,
    status: "Do zrobienia",
    issueType: type.name,
    priority: "Medium",
    assignee: issue.assignMe ? { ...previewJiraMe } : null,
    boards: Object.entries(previewBoardProject)
      .filter(([, p]) => p === issue.project)
      .map(([id]) => Number(id)),
  });
  savePreviewBoard(state);
  let snapshot: Snapshot | null = null;
  if (issue.createTask) {
    const data = await read();
    const linked = data.tasks.find((t) => t.jiraKey === issueKey);
    if (linked) {
      // Jak w Ruście: zadanie zaimportowane wcześniej, bez drugiej kopii.
      if (issue.plannedDay)
        linked.details = { ...detailsOf(linked), plannedDay: issue.plannedDay };
      localStorage.setItem(key, JSON.stringify(data));
      return { key: issueKey, url: previewIssueUrl(issueKey), snapshot: data, warning: null };
    }
    const created: Task = {
      id:
        Math.max(
          0,
          ...data.tasks.map((t) => t.id),
          ...(data.trash ?? []).map((t) => t.id),
        ) + 1,
      title: [...summary].slice(0, 240).join(""),
      done: false,
      createdAt: Date.now(),
      jiraKey: issueKey,
      jiraUrl: previewIssueUrl(issueKey),
    };
    if (issue.plannedDay)
      created.details = { ...detailsOf(created), plannedDay: issue.plannedDay };
    data.tasks.unshift(created);
    localStorage.setItem(key, JSON.stringify(data));
    snapshot = data;
  }
  return { key: issueKey, url: previewIssueUrl(issueKey), snapshot, warning: null };
}

// ---------------------------------------------------------------------------
// Ideo Timesheet (PLAN_1.8.md, T1). W aplikacji komendy z `timesheet.rs`; w
// podglądzie symulowany TS w localStorage (`previewTsKey`) z tymi samymi
// regułami walidacji i zapisu (zapis zastępuje cały dzień, `keep` dokleja
// wpisy TS o zadaniach spoza wysyłanej listy). Hasło nigdy nie jest zapisywane.

export interface TsConfig {
  apiUrl: string;
  login: string;
  deviceId: string;
  deviceName: string;
}
export interface TsSettings {
  config: TsConfig;
  /** Hasło jest w magazynie poświadczeń. */
  connected: boolean;
  /** Ustawienie „Wysyłaj raport TS” (`tsSend`). */
  sendEnabled: boolean;
}
export interface TsTask {
  id: string;
  name: string;
}
export interface TsProject {
  name: string;
  tasks: TsTask[];
}
/** Wpis wypełniony w TS; wartości jako tekst, jak oddał serwer. */
export interface TsFilled {
  id: string;
  hours: string;
  overtime: string;
  description: string;
}
/** Wpis wysyłany do TS (`timesheet::Entry`). */
export interface TsEntry {
  taskId: string;
  hours: string;
  overtime: string;
  description: string;
}
export type TsMode = "replace" | "keep";
export interface TsSendResult {
  filled: TsFilled[];
  sentAt: number;
  /** Zapis przyjęty, ale odczyt dnia po zapisie się nie udał (`filled` puste). */
  warning?: string;
}
export interface TsSent {
  day: string;
  sentAt: number;
  hours: string;
}
export const tsDefaultApi = "https://api.ts.ideo.pl";
export const tsNotConnected = "Połącz konto TS: podaj login i hasło.";
export const tsSendDisabled =
  "Wysyłka do TS jest wyłączona w Ustawieniach → Timesheet.";
export const tsLoginRejected = "TS odrzucił logowanie. Sprawdź login i hasło.";
export const tsReadBackWarning =
  "Zapisano w TS, ale nie udało się odczytać dnia — sprawdź w przeglądarce.";
export const tsNotConfirmed =
  "TS nie potwierdził zapisu — sprawdź dzień w przeglądarce.";
const tsApiError =
  "Podaj główny adres API TS z https, bez ścieżki i parametrów, np. https://api.ts.ideo.pl.";
/**
 * Symulowany TS podglądu. `failLogin` wymusza odrzucenie logowania; hasło „zle”
 * odrzuca zawsze. `tasks` nadpisuje przykładową listę zadań. `failReadBack`:
 * zapis przyjęty, odczyt po zapisie nieudany. `ignoreSave`: TS przyjmuje zapis,
 * ale dzień się nie zmienia (odczyt bez wysłanych wpisów).
 */
export const previewTsKey = "todo-track-preview-ts-v1";
interface PreviewTs {
  connected?: boolean;
  login?: string;
  apiUrl?: string;
  deviceId?: string;
  failLogin?: boolean;
  failReadBack?: boolean;
  ignoreSave?: boolean;
  tasks?: TsProject[];
  filled?: Record<string, TsFilled[]>;
  sent?: Record<string, { sentAt: number; hours: string }>;
  /** Domyślne zadania TS projektów (`ts_project_map`). */
  projectMap?: TsProjectMapping[];
}
export const previewTsTasks: readonly TsProject[] = Object.freeze([
  {
    name: "WMS",
    tasks: [
      { id: "1101", name: "Rozwój" },
      { id: "1102", name: "Błędy i wsparcie" },
      { id: "1103", name: "Code review" },
    ],
  },
  {
    name: "Wewnętrzne",
    tasks: [
      { id: "2201", name: "Spotkania" },
      { id: "2202", name: "Administracja" },
    ],
  },
]);
function previewTs(): PreviewTs {
  return previewJson(previewTsKey) as PreviewTs;
}
function savePreviewTs(state: PreviewTs) {
  localStorage.setItem(previewTsKey, JSON.stringify(state));
}
/** Adres API jak `timesheet::normalize_api`. */
export function tsNormalizeApi(value: string): string {
  const raw = value.trim();
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(tsApiError);
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.pathname !== "/" ||
    /[?#]/.test(raw) ||
    url.username ||
    url.password
  )
    throw new Error(tsApiError);
  return url.origin;
}
const tsValidDay = (value: string) => {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return false;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  return d.getUTCMonth() === +m[2] - 1 && d.getUTCDate() === +m[3];
};
function tsCheckDate(value: string) {
  if (!tsValidDay(value))
    throw new Error("Niepoprawny dzień (oczekiwany format RRRR-MM-DD).");
}
/** Godziny jak `timesheet::valid_hours`. */
export const tsValidHours = (value: string) =>
  /^\d{1,6}([.,]\d{1,2})?$/.test(value);
const tsControl = /[\u0000-\u001f\u007f-\u009f]/;
/** Walidacja wpisów jak `timesheet::validate_entries`. */
export function tsValidateEntries(entries: TsEntry[]) {
  if (!entries.length) throw new Error("Brak wpisów do zapisania w TS.");
  if (entries.length > 50)
    throw new Error("Jednorazowo można zapisać w TS najwyżej 50 wpisów.");
  for (const e of entries) {
    const id = e.taskId.trim();
    if (!id || [...id].length > 20 || tsControl.test(id))
      throw new Error("Niepoprawny identyfikator zadania TS.");
    if (!tsValidHours(e.hours) || !tsValidHours(e.overtime))
      throw new Error(
        "Godziny w TS podaj jako liczbę z najwyżej dwoma miejscami po przecinku, np. 1,25.",
      );
    if ([...e.description].length > 2000)
      throw new Error("Opis wpisu TS może mieć najwyżej 2000 znaków.");
    if (tsControl.test(e.description))
      throw new Error("Opis wpisu TS nie może zawierać znaków sterujących.");
  }
}
/** `keep` jak `timesheet::merge_keep`: nasze wpisy, potem wpisy TS spoza listy. */
export function tsMergeKeep(entries: TsEntry[], filled: TsFilled[]): TsEntry[] {
  const ours = new Set(entries.map((e) => e.taskId.trim()));
  return [
    ...entries,
    ...filled
      .filter((f) => !ours.has(f.id.trim()))
      .map((f) => ({
        taskId: f.id,
        hours: f.hours,
        overtime: f.overtime,
        description: f.description,
      })),
  ];
}
/** Suma godzin z odczytu TS jak `timesheet::hours_total` („7,50”). */
export function tsHoursTotal(filled: TsFilled[]): string {
  const hundredths = filled.reduce((sum, f) => {
    const raw = f.hours.trim();
    const v = Number(raw.replace(",", "."));
    return raw && Number.isFinite(v) ? sum + Math.round(v * 100) : sum;
  }, 0);
  return `${Math.trunc(hundredths / 100)},${String(hundredths % 100).padStart(2, "0")}`;
}
const tsEntryRow = (e: TsEntry): TsFilled => ({
  id: e.taskId.trim(),
  hours: e.hours,
  overtime: e.overtime,
  description: e.description,
});
/**
 * Jak `timesheet::sent_outcome`: odczyt po zapisie musi zawierać każde wysłane
 * zadanie z niezerowymi godzinami (inaczej błąd, bez znacznika wysyłki); brak
 * odczytu (`null`) → znacznik z sumą wysłanych godzin i ostrzeżenie.
 */
export function tsSentOutcome(
  sent: TsEntry[],
  read: TsFilled[] | null,
): { filled: TsFilled[]; hours: string; warning?: string } {
  if (read === null)
    return {
      filled: [],
      hours: tsHoursTotal(sent.map(tsEntryRow)),
      warning: tsReadBackWarning,
    };
  const ok = sent
    .filter((e) => Number(e.hours.trim().replace(",", ".")) > 0)
    .every((e) => read.some((f) => f.id.trim() === e.taskId.trim()));
  if (!ok) throw new Error(tsNotConfirmed);
  return { filled: read, hours: tsHoursTotal(read) };
}
function previewTsSettings(): TsSettings {
  const state = previewTs();
  return {
    config: {
      apiUrl: state.apiUrl ?? tsDefaultApi,
      login: state.login ?? "",
      deviceId: state.deviceId ?? "",
      deviceName: "Todo Track",
    },
    connected: state.connected === true && !!state.login,
    sendEnabled: previewJson(previewSettingsKey).tsSend !== false,
  };
}
function previewTsConnected(): PreviewTs {
  const state = previewTs();
  if (!(state.connected === true && state.login)) throw new Error(tsNotConnected);
  return state;
}
/** `ts_settings`: konfiguracja bez hasła, stan połączenia i przełącznik wysyłki. */
export async function tsSettings(): Promise<TsSettings> {
  if (desktop) return invoke("ts_settings");
  return previewTsSettings();
}
/** `ts_connect`: logowanie, zapis hasła w magazynie i konfiguracji. */
export async function tsConnect(
  apiUrl: string,
  login: string,
  password: string,
): Promise<TsSettings> {
  tsForgetTasks();
  if (desktop)
    return notifyTsAfter(invoke("ts_connect", { apiUrl, login, password }));
  const name = login.trim();
  if (!name || [...name].length > 100 || tsControl.test(name))
    throw new Error("Podaj login TS (do 100 znaków).");
  if (!password || [...password].length > 1024)
    throw new Error("Podaj hasło TS.");
  const api = tsNormalizeApi(apiUrl);
  const state = previewTs();
  if (state.failLogin || password === "zle") throw new Error(tsLoginRejected);
  state.apiUrl = api;
  state.login = name;
  state.connected = true;
  if (!/^[0-9a-f]{32}$/.test(state.deviceId ?? ""))
    state.deviceId = Array.from(
      crypto.getRandomValues(new Uint8Array(16)),
      (b) => b.toString(16).padStart(2, "0"),
    ).join("");
  savePreviewTs(state);
  notifyTs();
  return previewTsSettings();
}
/** `ts_disconnect`: usuwa hasło, zostawia login i identyfikator urządzenia. */
export async function tsDisconnect(): Promise<TsSettings> {
  tsForgetTasks();
  if (desktop) return notifyTsAfter(invoke("ts_disconnect"));
  const state = previewTs();
  state.connected = false;
  savePreviewTs(state);
  notifyTs();
  return previewTsSettings();
}
/** `ts_tasks`: projekty i zadania TS z formularza dnia. */
export async function tsTasks(date: string): Promise<TsProject[]> {
  if (desktop) return invoke("ts_tasks", { date });
  tsCheckDate(date);
  const state = previewTsConnected();
  return structuredClone(state.tasks ?? (previewTsTasks as TsProject[]));
}
/** `ts_filled`: wpisy już zapisane w TS na ten dzień. */
export async function tsFilled(date: string): Promise<TsFilled[]> {
  if (desktop) return invoke("ts_filled", { date });
  tsCheckDate(date);
  const state = previewTsConnected();
  return structuredClone(state.filled?.[date] ?? []);
}
/**
 * `ts_send`: zapis dnia w TS (zastępuje cały dzień; `keep` dokleja wpisy TS
 * o zadaniach spoza listy), potem odczyt dnia i znacznik wysyłki.
 */
export async function tsSend(
  date: string,
  entries: TsEntry[],
  mode: TsMode,
): Promise<TsSendResult> {
  if (desktop)
    return notifyTsAfter(invoke("ts_send", { date, entries, mode }));
  if (previewJson(previewSettingsKey).tsSend === false)
    throw new Error(tsSendDisabled);
  tsCheckDate(date);
  if (mode !== "replace" && mode !== "keep")
    throw new Error("Niepoprawny tryb zapisu w TS.");
  tsValidateEntries(entries);
  const state = previewTsConnected();
  const current = state.filled?.[date] ?? [];
  const saved = mode === "keep" ? tsMergeKeep(entries, current) : entries;
  if (!state.ignoreSave)
    state.filled = { ...state.filled, [date]: saved.map(tsEntryRow) };
  savePreviewTs(state);
  const read = state.failReadBack ? null : (state.filled?.[date] ?? []);
  const { filled, hours, warning } = tsSentOutcome(saved, read);
  const sentAt = Date.now();
  state.sent = { ...state.sent, [date]: { sentAt, hours } };
  savePreviewTs(state);
  const result: TsSendResult = { filled: structuredClone(filled), sentAt };
  if (warning) result.warning = warning;
  notifyTs();
  return result;
}
/**
 * Zmiana konta, wysyłki albo domyślnych zadań TS: zdarzenie okna po każdej
 * udanej komendzie zmieniającej stan, a w podglądzie także zdarzenie `storage`
 * dla `previewTsKey` (inne okno albo test zmienia symulowany TS), jak przy Jirze.
 */
export const tsChangedEvent = "todo-track-ts-changed";
function notifyTs() {
  window.dispatchEvent(new Event(tsChangedEvent));
}
async function notifyTsAfter<T>(result: Promise<T>): Promise<T> {
  const value = await result;
  notifyTs();
  return value;
}
/** Nasłuch zmian TS; zwraca funkcję odłączającą. */
export function onTsChanged(handler: () => void): () => void {
  const storage = (e: StorageEvent) => {
    if (e.key !== previewTsKey) return;
    // Lista zadań TS mogła się zmienić w symulowanym TS.
    tsForgetTasks();
    handler();
  };
  window.addEventListener(tsChangedEvent, handler);
  if (!desktop) window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(tsChangedEvent, handler);
    window.removeEventListener("storage", storage);
  };
}
/** `ts_sent`: ostatnia wysyłka dnia albo null. */
export async function tsSent(day: string): Promise<TsSent | null> {
  if (desktop) return invoke("ts_sent", { day });
  tsCheckDate(day);
  const sent = previewTs().sent?.[day];
  return sent ? { day, ...sent } : null;
}

// ---------------------------------------------------------------------------
// Zadania TS dla zadań Todo Track (PLAN_1.8.md, T2).

/** Domyślne zadanie TS projektu Todo Track (`timesheet::ProjectMap`). */
export interface TsProjectMapping {
  project: string;
  tsTaskId: string;
  tsTaskName: string;
}
/** Identyfikator zadania TS jak `timesheet::valid_task_id`. */
export const tsValidTaskId = (id: string) =>
  !!id && [...id].length <= 20 && !tsControl.test(id);
/** Nazwa zadania TS jak `timesheet::valid_task_name`. */
export const tsValidTaskName = (name: string) =>
  [...name].length <= 300 && !tsControl.test(name);
const tsSortMap = (list: TsProjectMapping[]) =>
  [...list].sort((a, b) => {
    const x = tsProjectKey(a.project);
    const y = tsProjectKey(b.project);
    return x < y ? -1 : x > y ? 1 : 0;
  });
/** `ts_project_map`: domyślne zadania TS projektów. */
export async function tsProjectMap(): Promise<TsProjectMapping[]> {
  if (desktop) return invoke("ts_project_map");
  return tsSortMap(structuredClone(previewTs().projectMap ?? []));
}
/**
 * `ts_set_project_map`: domyślne zadanie TS projektu (puste `tsTaskId` usuwa
 * mapowanie); zwraca całą listę.
 */
export async function tsSetProjectMap(
  project: string,
  tsTaskId: string,
  tsTaskName: string,
): Promise<TsProjectMapping[]> {
  if (desktop)
    return notifyTsAfter(
      invoke("ts_set_project_map", { project, tsTaskId, tsTaskName }),
    );
  const name = project.trim();
  const id = tsTaskId.trim();
  const label = tsTaskName.trim();
  if (!name || [...name].length > 80 || tsControl.test(name))
    throw new Error("Niepoprawna nazwa projektu.");
  const state = previewTs();
  const key = tsProjectKey(name);
  const rest = (state.projectMap ?? []).filter(
    (m) => tsProjectKey(m.project) !== key,
  );
  if (id) {
    if (!tsValidTaskId(id))
      throw new Error("Niepoprawny identyfikator zadania TS.");
    if (!tsValidTaskName(label))
      throw new Error("Niepoprawna nazwa zadania TS.");
    rest.push({ project: name, tsTaskId: id, tsTaskName: label });
  }
  state.projectMap = rest;
  savePreviewTs(state);
  notifyTs();
  return tsSortMap(structuredClone(rest));
}
/**
 * Lista zadań TS do wyboru w edycji zadania: `ts_tasks(dzisiaj)` pobrane raz na
 * czas działania aplikacji (pamięć podręczna w pamięci); `refresh` pobiera
 * ponownie. Nieudane pobranie nie zostaje w pamięci.
 */
let tsTaskCache: { day: string; list: Promise<TsProject[]> } | null = null;
export function tsTodayTasks(refresh = false): Promise<TsProject[]> {
  const day = localDay(Date.now());
  if (refresh || !tsTaskCache || tsTaskCache.day !== day) {
    const entry: { day: string; list: Promise<TsProject[]> } = {
      day,
      list: Promise.resolve([]),
    };
    entry.list = tsTasks(day).catch((e) => {
      if (tsTaskCache === entry) tsTaskCache = null;
      throw e;
    });
    tsTaskCache = entry;
  }
  return tsTaskCache.list;
}
/** Czyści pamięć podręczną listy zadań TS (połączenie, rozłączenie). */
export function tsForgetTasks() {
  tsTaskCache = null;
}

// --- Redmine: widok „projekt → zgłoszenia” (PLAN_1.9.md, R3) ---------------

/** Śledzony albo dostępny projekt Redmine (`redmine::Project`). */
export interface RedmineProject {
  id: number;
  name: string;
  identifier: string;
  parentId: number | null;
}
/** Konfiguracja połączenia bez klucza API (`redmine::Config`). */
export interface RedmineConfig {
  site: string;
  login: string;
  userId: number;
  displayName: string;
  acceptInvalidCert: boolean;
  projects: RedmineProject[];
}
/** `redmine_settings`: konfiguracja i czy klucz API jest w magazynie. */
export interface RedmineAccount {
  config: RedmineConfig;
  connected: boolean;
}
/** Status z `/issue_statuses.json` (`redmine::Status`). */
export interface RedmineStatus {
  id: number;
  name: string;
  isClosed: boolean;
}
/** Zgłoszenie z `/issues.json` (`redmine::Issue`). */
export interface RedmineIssue {
  id: number;
  subject: string;
  projectId: number;
  project: string;
  tracker: string;
  status: string;
  statusId: number;
  closed: boolean;
  priority: string;
  assignedToId: number | null;
  assignedTo: string;
  /** Wersja docelowa (`fixed_version.name`). */
  version: string;
  category: string;
  dueDate: string;
  updatedOn: string;
  url: string;
}
/** Grupa statusu: numery zgłoszeń w kolejności wyświetlania (`redmine::StatusGroup`). */
export interface RedmineStatusGroup {
  status: string;
  closed: boolean;
  ids: number[];
}
/** `redmine_issues`: lista, ostrzeżenia, grupy statusów i śledzone projekty z podprojektami. */
export interface RedmineIssues {
  issues: RedmineIssue[];
  warnings: string[];
  groups: RedmineStatusGroup[];
  tracked: number[];
}
// --- Redmine: zapisy (PLAN_1.9.md, R4) -------------------------------------

/** Komunikat przy wyłączonych zapisach (`redmine::WRITE_DISABLED`). */
export const redmineWriteDisabled =
  "Zmiany w Redmine są wyłączone. Włączysz je w Ustawienia → Redmine („Pozwól aplikacji zmieniać zgłoszenia w Redmine”).";
/** Najdłuższy komentarz (`redmine::NOTES_LIMIT`). */
export const redmineNotesLimit = 4000;
/** Status Redmine jako przejście pytania (`jira_move::redmine_transitions_of`). */
export const redmineTransitionOf = (s: RedmineStatus): JiraTransition => ({
  id: String(s.id),
  name: s.name,
  toStatus: s.name,
  toCategory: s.isClosed ? "done" : "indeterminate",
});
/**
 * Cel pytania przy starcie pracy jak `redmine::choose_status`: dozwolone
 * statusy niezamknięte bez bieżącego; wybrany status o nazwie z ustawień (bez
 * względu na wielkość liter), bez dopasowania `null` — lista do wyboru.
 */
export function chooseRedmineStatus(
  allowed: readonly RedmineStatus[],
  current: number,
  preferred: string,
): { statuses: RedmineStatus[]; statusId: number | null } {
  const statuses = allowed.filter((s) => s.id !== current && !s.isClosed);
  const wanted = preferred.trim().toLowerCase();
  const statusId =
    (wanted &&
      statuses.find((s) => s.name.trim().toLowerCase() === wanted)?.id) ||
    null;
  return { statuses, statusId };
}
/** Ustawienia zapisów do Redmine w podglądzie (wartości domyślne jak w Rust). */
function previewRedmineSettings() {
  const s = previewJson(previewSettingsKey);
  return {
    write: s.redmineWrite !== false && previewRedmineState().write !== false,
    ask: s.redmineAskMove !== false,
    status:
      typeof s.redmineMoveStatus === "string" && s.redmineMoveStatus.trim()
        ? s.redmineMoveStatus.trim()
        : "In Progress",
    newStatuses: Array.isArray(s.redmineNewStatuses)
      ? s.redmineNewStatuses.filter((n): n is string => typeof n === "string")
      : ["New", "Nowy"],
    prefix:
      typeof s.redmineBranchPrefix === "string"
        ? s.redmineBranchPrefix
        : "feature/",
  };
}
/** Status początkowy niezamkniętego zgłoszenia jak `redmine::should_ask`. */
const previewRedmineInitial = (
  newStatuses: readonly string[],
  status: string,
  closed: boolean,
) =>
  !closed &&
  !!status.trim() &&
  newStatuses.some(
    (n) => n.trim().toLowerCase() === status.trim().toLowerCase(),
  );
/**
 * Odpowiednik `jira_move::check_redmine` w podglądzie: status bieżący z
 * symulowanego Redmine trafia na kafelek, a przy statusie początkowym powstaje
 * pytanie (w symulacji wszystkie statusy są dozwolone).
 */
function previewRedmineStartPrompt(
  task: Task,
  data: Snapshot,
): JiraMovePrompt | null {
  const settings = previewRedmineSettings();
  const id = task.redmineId;
  if (!validRedmineId(id) || !settings.write || !settings.ask) return null;
  if (task.details?.jiraNoAsk) return null;
  const state = previewRedmineState();
  if (state.connected === false) return null;
  const source = previewRedmineList(state).find((i) => i.id === id);
  const status = source?.status ?? task.redmineStatus ?? "";
  if (task.redmineStatus !== status) {
    task.redmineStatus = status;
    localStorage.setItem(key, JSON.stringify(data));
  }
  const statuses = previewRedmineStatusList(state);
  const current = statuses.find((s) => s.name === status);
  if (!previewRedmineInitial(settings.newStatuses, status, !!current?.isClosed))
    return null;
  const choice = chooseRedmineStatus(statuses, current?.id ?? 0, settings.status);
  if (!choice.statuses.length) return null;
  return {
    system: "redmine",
    taskId: task.id,
    key: redmineKey(id),
    title: task.title,
    status,
    url: task.redmineUrl ?? `${previewRedmineSite}/issues/${id}`,
    target: settings.status,
    transitions: choice.statuses.map(redmineTransitionOf),
    transitionId: choice.statusId === null ? null : String(choice.statusId),
  };
}
/** Zapis do symulowanego Redmine: przełącznik, połączenie i symulowany błąd. */
function previewRedmineWrite(): PreviewRedmine {
  if (!previewRedmineSettings().write) throw new Error(redmineWriteDisabled);
  const state = previewRedmineConnected();
  if (state.fail === 403)
    throw new Error("Brak uprawnień w Redmine do tej operacji.");
  if (state.fail === 422)
    throw new Error("Redmine odrzucił dane: Status is invalid");
  return state;
}
const redmineMissing =
  "Nie znaleziono zgłoszenia albo adres Redmine jest błędny (REST API może być wyłączone).";
/** Zgłoszenie do zmiany (lista przykładowa kopiowana do stanu przy pierwszym zapisie). */
function previewRedmineEditable(
  state: PreviewRedmine,
  id: number,
): PreviewRedmineIssue {
  state.issues = previewRedmineList(state).map((i) => ({ ...i }));
  const issue = state.issues.find((i) => i.id === id);
  if (!issue) throw new Error(redmineMissing);
  return issue;
}
/** Status zgłoszenia w zadaniach podglądu (`redmine_issues` → kafelek). */
async function previewRedmineTaskStatus(id: number, status: string) {
  const data = await read();
  let changed = false;
  for (const t of [...data.tasks, ...(data.trash ?? [])])
    if (t.redmineId === id && t.redmineStatus !== status) {
      t.redmineStatus = status;
      changed = true;
    }
  if (changed) localStorage.setItem(key, JSON.stringify(data));
}
/** `redmine_issue_transitions`: dozwolone statusy zgłoszenia bez bieżącego. */
export async function redmineIssueTransitions(
  issueId: number,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_issue_transitions", { issueId });
  if (!previewRedmineSettings().write) throw new Error(redmineWriteDisabled);
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  const state = previewRedmineConnected();
  const source = previewRedmineList(state).find((i) => i.id === issueId);
  if (!source) throw new Error(redmineMissing);
  const issue = previewRedmineFull(source, state);
  await previewRedmineTaskStatus(issueId, issue.status);
  return previewRedmineStatusList(state)
    .filter((s) => s.id !== issue.statusId)
    .map((s) => ({ ...s }));
}
/**
 * `redmine_issue_transition`: zmiana statusu (`PUT` ze `status_id`), odczyt po
 * zapisie i status na kafelkach powiązanych zadań. Symulowany Redmine może
 * odrzucić zapis (`fail`) albo przyjąć go bez zmiany (`unchanged`).
 */
export async function redmineIssueTransition(
  issueId: number,
  statusId: number,
): Promise<RedmineIssue | null> {
  if (desktop)
    return invoke("redmine_issue_transition", { issueId, statusId });
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  const state = previewRedmineWrite();
  const status = previewRedmineStatusList(state).find((s) => s.id === statusId);
  if (!status) throw new Error("Redmine odrzucił dane: Status is invalid");
  const issue = previewRedmineEditable(state, issueId);
  if (!state.unchanged) issue.status = status.name;
  savePreviewRedmine(state);
  const full = previewRedmineFull(issue, state);
  await previewRedmineTaskStatus(issueId, full.status);
  if (full.statusId !== statusId)
    throw new Error(
      `Redmine nie zmienił statusu #${issueId} (nadal „${full.status}”). Zmień status w przeglądarce — przejście może wymagać dodatkowych pól.`,
    );
  return full;
}
/** Zadanie ze zgłoszeniem Redmine dla zapisu (`jira_move::redmine_target`). */
async function previewRedmineTask(taskId: number): Promise<number> {
  if (!previewRedmineSettings().write) throw new Error(redmineWriteDisabled);
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  const id = task?.redmineId;
  if (!task || task.jiraKey || !validRedmineId(id))
    throw new Error("To zadanie nie jest powiązane ze zgłoszeniem Redmine.");
  return id;
}
/** `redmine_transitions`: dozwolone statusy zgłoszenia zadania bez bieżącego. */
export async function redmineTransitions(
  taskId: number,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_transitions", { taskId });
  return redmineIssueTransitions(await previewRedmineTask(taskId));
}
/**
 * `redmine_transition`: zmiana statusu zgłoszenia zadania (baner, widget,
 * kafelek); pytanie zadania znika, a błąd zostaje przy pytaniu jak dla Jiry.
 */
export async function redmineTransition(
  taskId: number,
  statusId: number,
): Promise<Snapshot> {
  if (desktop) return invoke("redmine_transition", { taskId, statusId });
  try {
    await redmineIssueTransition(await previewRedmineTask(taskId), statusId);
  } catch (e) {
    const message = String(e instanceof Error ? e.message : e);
    const prompts = previewJiraState();
    if ((prompts.prompts ?? []).some((p) => p.taskId === taskId)) {
      prompts.prompts = (prompts.prompts ?? []).map((p) =>
        p.taskId === taskId ? { ...p, failure: message } : p,
      );
      savePreviewJira(prompts);
      notifyPreviewJira();
    }
    throw new Error(message);
  }
  const prompts = previewJiraState();
  prompts.prompts = (prompts.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(prompts);
  notifyPreviewJira();
  return read();
}
/** `redmine_assign_me`: `PUT` z `assigned_to_id` = moje konto. */
export async function redmineAssignMe(
  issueId: number,
): Promise<RedmineIssue | null> {
  if (desktop) return invoke("redmine_assign_me", { issueId });
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  const state = previewRedmineWrite();
  const issue = previewRedmineEditable(state, issueId);
  delete issue.assignedToId;
  delete issue.assignedTo;
  savePreviewRedmine(state);
  return previewRedmineFull(issue, state);
}
/** `redmine_comment`: komentarz zwykłym tekstem w `notes` (1–4000 znaków). */
export async function redmineComment(
  issueId: number,
  text: string,
): Promise<void> {
  if (desktop) return invoke("redmine_comment", { issueId, text });
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  if (!previewRedmineSettings().write) throw new Error(redmineWriteDisabled);
  const body = text
    .replace(/\r\n?/g, "\n")
    .replace(/[^\P{Cc}\n\t]/gu, "")
    .trim();
  if (!body) throw new Error("Wpisz treść komentarza.");
  if ([...body].length > redmineNotesLimit)
    throw new Error(`Komentarz może mieć najwyżej ${redmineNotesLimit} znaków.`);
  const state = previewRedmineWrite();
  previewRedmineEditable(state, issueId);
  const comments = (state.comments ??= {});
  comments[issueId] = [...(comments[issueId] ?? []), body];
  savePreviewRedmine(state);
}
/** `redmine_branch_name`: `feature/1234-krotki-tytul` z prefiksem z ustawień. */
export async function redmineBranchName(issueId: number): Promise<string> {
  if (desktop) return invoke("redmine_branch_name", { issueId });
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  const subject = previewRedmineList(previewRedmineState()).find(
    (i) => i.id === issueId,
  )?.subject;
  const task = subject
    ? undefined
    : (await read()).tasks.find((t) => t.redmineId === issueId);
  return branchName(
    previewRedmineSettings().prefix,
    String(issueId),
    subject ?? task?.title ?? "",
  );
}

/** Komunikat bez połączenia (`redmine::NOT_CONNECTED`). */
export const redmineNotConnected =
  "Połącz konto Redmine: podaj adres i klucz API.";
/** Najwięcej zgłoszeń na liście (5 stron po 100, `redmine::MAX_ISSUE_PAGES`). */
export const redmineIssueLimit = 500;
/** Ślad linków „Otwórz w Redmine” w podglądzie (dla testów). */
export const previewRedmineOpenedKey = "todo-track-preview-redmine-opened-v1";

const previewRedmineList = (state: PreviewRedmine) =>
  Array.isArray(state.issues) ? state.issues : previewRedmineIssues;
const previewRedmineStatusList = (state: PreviewRedmine) =>
  Array.isArray(state.statuses) ? state.statuses : previewRedmineStatuses;
const previewRedmineProjectList = (state: PreviewRedmine) =>
  Array.isArray(state.projects) ? state.projects : previewRedmineProjects;
const previewRedmineTracked = (state: PreviewRedmine) =>
  Array.isArray(state.selectedProjects)
    ? state.selectedProjects
    : previewRedmineProjectList(state).slice(0, 1);
function previewRedmineConnected(): PreviewRedmine {
  const state = previewRedmineState();
  if (state.connected === false) throw new Error(redmineNotConnected);
  return state;
}
/** Pełne zgłoszenie z przykładu (jak `redmine::parse_issue`). */
function previewRedmineFull(
  issue: PreviewRedmineIssue,
  state: PreviewRedmine,
): RedmineIssue {
  const status = previewRedmineStatusList(state).find(
    (s) => s.name === issue.status,
  );
  const projectId = issue.projectId ?? previewRedmineTracked(state)[0]?.id ?? 0;
  const mine = issue.assignedToId === undefined;
  return {
    id: issue.id,
    subject: issue.subject,
    projectId,
    project:
      previewRedmineProjectList(state).find((p) => p.id === projectId)?.name ??
      "",
    tracker: issue.tracker,
    status: issue.status,
    statusId: status?.id ?? 0,
    closed: status?.isClosed ?? false,
    priority: issue.priority ?? "",
    assignedToId: mine ? previewRedmineMe.id : (issue.assignedToId ?? null),
    assignedTo: mine
      ? previewRedmineUser(state).name
      : (issue.assignedTo ?? ""),
    version: issue.version ?? "",
    category: issue.category ?? "",
    dueDate: issue.dueDate,
    updatedOn: issue.updatedOn ?? "",
    url: `${previewRedmineSite}/issues/${issue.id}`,
  };
}

/**
 * Grupy statusów jak `redmine::group_by_status`: kolejność statusów z listy
 * (`/issue_statuses.json`), status spoza listy na końcu, puste grupy pominięte;
 * w grupie pasmo priorytetu malejąco, potem termin (bez terminu na końcu),
 * potem kolejność z serwera.
 */
export function redmineGroups(
  issues: readonly RedmineIssue[],
  statuses: readonly RedmineStatus[],
): RedmineStatusGroup[] {
  const groups = statuses.map((s) => ({
    status: s.name,
    closed: s.isClosed,
    id: s.id as number | null,
    members: [] as number[],
  }));
  issues.forEach((issue, index) => {
    let slot = groups.findIndex(
      (g) => issue.statusId !== 0 && g.id === issue.statusId,
    );
    if (slot < 0)
      slot = groups.findIndex(
        (g) =>
          g.status === issue.status && (g.id === null || issue.statusId === 0),
      );
    if (slot < 0)
      groups.push({
        status: issue.status,
        closed: issue.closed,
        id: null,
        members: [index],
      });
    else groups[slot].members.push(index);
  });
  return groups
    .filter((g) => g.members.length)
    .map((g) => {
      const sorted = [...g.members].sort((a, b) => {
        const x = issues[a],
          y = issues[b];
        const band =
          (priorityBand(y.priority) ?? 0) - (priorityBand(x.priority) ?? 0);
        if (band) return band;
        if (!x.dueDate !== !y.dueDate) return x.dueDate ? -1 : 1;
        if (x.dueDate !== y.dueDate) return x.dueDate < y.dueDate ? -1 : 1;
        return a - b;
      });
      return {
        status: g.status,
        closed: g.closed,
        ids: sorted.map((i) => issues[i].id),
      };
    });
}
/** Śledzone projekty z podprojektami jak `redmine::tracked_project_ids`. */
export function trackedProjectIds(
  tracked: readonly RedmineProject[],
  known: readonly RedmineProject[],
): number[] {
  const ids: number[] = [];
  for (const p of tracked) if (!ids.includes(p.id)) ids.push(p.id);
  for (let before = -1; before !== ids.length; ) {
    before = ids.length;
    for (const p of known)
      if (
        !ids.includes(p.id) &&
        p.parentId !== null &&
        ids.includes(p.parentId)
      )
        ids.push(p.id);
  }
  return ids;
}

/**
 * Zmiana konta albo śledzonych projektów Redmine: zdarzenie okna po „Połącz”,
 * „Rozłącz” i wyborze projektów, a w podglądzie także zdarzenie `storage` dla
 * `previewRedmineKey` (inne okno albo test zmienia symulowany Redmine), jak
 * przy Jirze i TS.
 */
export const redmineChangedEvent = "todo-track-redmine-changed";
function notifyRedmine() {
  window.dispatchEvent(new Event(redmineChangedEvent));
}
async function notifyRedmineAfter<T>(result: Promise<T>): Promise<T> {
  const value = await result;
  notifyRedmine();
  return value;
}
/** Nasłuch zmian Redmine; zwraca funkcję odłączającą. */
export function onRedmineChanged(handler: () => void): () => void {
  const storage = (e: StorageEvent) => {
    if (e.key === previewRedmineKey || e.key === null) handler();
  };
  window.addEventListener(redmineChangedEvent, handler);
  if (!desktop) window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(redmineChangedEvent, handler);
    window.removeEventListener("storage", storage);
  };
}

/** `redmine_settings` (w podglądzie konto przykładowe, chyba że rozłączone). */
export async function redmineAccount(): Promise<RedmineAccount> {
  if (desktop) return invoke("redmine_settings");
  const state = previewRedmineState();
  const connected = state.connected !== false;
  const user = previewRedmineUser(state);
  return {
    config: {
      site: typeof state.site === "string" ? state.site : previewRedmineSite,
      login: connected ? user.login : "",
      userId: connected ? previewRedmineMe.id : 0,
      displayName: connected ? user.name : "",
      acceptInvalidCert: state.acceptInvalidCert === true,
      projects: previewRedmineTracked(state).map((p) => ({ ...p })),
    },
    connected,
  };
}

// --- Redmine: połączenie w Ustawieniach (PLAN_1.9.md, R5) ------------------

/** `redmine::SITE_ERROR`. */
export const redmineSiteError =
  "Podaj adres Redmine, np. https://redmine.firma.pl albo https://firma.pl/redmine (bez parametrów).";
/** `redmine::KEY_ERROR`. */
export const redmineKeyError =
  "Podaj klucz API Redmine (1–128 znaków, bez spacji).";
/** `redmine::status_error` dla 401. */
export const redmineKeyRejected =
  "Redmine odrzucił klucz API. Sprawdź klucz w „Moje konto” w Redmine.";
/** `redmine::CERT_ERROR`: certyfikat spoza magazynu Windows. */
export const redmineCertError =
  "Certyfikat serwera nie jest zaufany. Zainstaluj certyfikat firmowego urzędu w Windows albo włącz opcję w „Zaawansowane”.";
/**
 * Adres Redmine jak `redmine::normalize_site`: `http` albo `https`, host
 * wymagany, prefiks ścieżki dozwolony (bez końcowego `/`), bez parametrów,
 * fragmentu i danych logowania.
 */
export function redmineNormalizeSite(site: string): string {
  const raw = site.trim();
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(redmineSiteError);
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    !url.hostname ||
    url.search ||
    url.hash ||
    raw.includes("?") ||
    raw.includes("#") ||
    url.username ||
    url.password
  )
    throw new Error(redmineSiteError);
  return url.href.replace(/\/+$/, "");
}
/** Klucz API jak `redmine::valid_api_key`: 1–128 znaków ASCII bez spacji. */
export const redmineValidApiKey = (key: string) =>
  key.length >= 1 && key.length <= 128 && /^[\x21-\x7e]+$/.test(key);
/** Link „Gdzie znaleźć klucz API”: `<adres>/my/account` albo null. */
export function redmineAccountUrl(site: string): string | null {
  try {
    return `${redmineNormalizeSite(site)}/my/account`;
  } catch {
    return null;
  }
}
/**
 * `redmine_connect`: adres i klucz sprawdzone przez `/users/current.json`,
 * klucz w magazynie poświadczeń (nigdy nie wraca), reszta w konfiguracji.
 * W podglądzie klucz „zly” daje 401, a `certError` błąd certyfikatu bez
 * opcji „Akceptuj niezaufany certyfikat”. Zmiana adresu czyści śledzone
 * projekty jak w Rust.
 */
export async function redmineConnect(
  site: string,
  apiKey: string,
  acceptInvalidCert: boolean,
): Promise<RedmineAccount> {
  if (desktop)
    return notifyRedmineAfter(
      invoke("redmine_connect", { site, apiKey, acceptInvalidCert }),
    );
  const normalized = redmineNormalizeSite(site);
  const key = apiKey.trim();
  if (!redmineValidApiKey(key)) throw new Error(redmineKeyError);
  const state = previewRedmineState();
  if (state.certError && !acceptInvalidCert) throw new Error(redmineCertError);
  if (key === "zly") throw new Error(redmineKeyRejected);
  const oldSite =
    typeof state.site === "string" ? state.site : previewRedmineSite;
  if (oldSite !== normalized) state.selectedProjects = [];
  state.site = normalized;
  state.acceptInvalidCert = acceptInvalidCert;
  state.connected = true;
  savePreviewRedmine(state);
  redmineConnected.value = true;
  return notifyRedmineAfter(redmineAccount());
}
/** `redmine_test`: „Połączono jako Imię Nazwisko (login), Redmine odpowiada.” */
export async function redmineTest(): Promise<string> {
  if (desktop) return invoke("redmine_test");
  const state = previewRedmineConnected();
  if (state.certError && !state.acceptInvalidCert)
    throw new Error(redmineCertError);
  if (state.failTest) throw new Error(state.failTest);
  const user = previewRedmineUser(state);
  return `Połączono jako ${user.name} (${user.login}), Redmine odpowiada.`;
}
/** `redmine_disconnect`: usuwa klucz; adres, projekty i powiązania zostają. */
export async function redmineDisconnect(): Promise<RedmineAccount> {
  if (desktop) return notifyRedmineAfter(invoke("redmine_disconnect"));
  const state = previewRedmineState();
  state.connected = false;
  savePreviewRedmine(state);
  redmineConnected.value = false;
  return notifyRedmineAfter(redmineAccount());
}
/**
 * `redmine_issues`: zgłoszenia projektu (z podprojektami, jak Redmine) albo —
 * bez `projectId` — moje ze wszystkich projektów; „Moje” = przypisane do mnie,
 * `openOnly` = bez zamkniętych. Najwyżej 500 z ostrzeżeniem.
 */
export async function redmineIssues(
  projectId: number | null,
  mine: boolean,
  openOnly: boolean,
): Promise<RedmineIssues> {
  if (desktop) return invoke("redmine_issues", { projectId, mine, openOnly });
  if (projectId === 0) throw new Error("Niepoprawny projekt Redmine.");
  const state = previewRedmineConnected();
  if (projectId === null && state.failMine) throw new Error(state.failMine);
  const statuses = previewRedmineStatusList(state);
  const projects = previewRedmineProjectList(state);
  const scope =
    projectId === null
      ? null
      : trackedProjectIds(
          [{ id: projectId, name: "", identifier: "", parentId: null }],
          projects,
        );
  const all = previewRedmineList(state)
    .map((i) => previewRedmineFull(i, state))
    .filter((i) => scope === null || scope.includes(i.projectId))
    .filter(
      (i) =>
        !(mine || projectId === null) || i.assignedToId === previewRedmineMe.id,
    )
    .filter((i) => !openOnly || !i.closed)
    .sort((a, b) => b.updatedOn.localeCompare(a.updatedOn));
  const issues = all.slice(0, redmineIssueLimit);
  return {
    issues,
    warnings:
      all.length > issues.length
        ? [
            `Pokazano pierwsze ${issues.length} z ${all.length} zgłoszeń. Zawęź zakres (np. „Moje” albo tylko otwarte).`,
          ]
        : [],
    groups: redmineGroups(issues, statuses),
    tracked: trackedProjectIds(previewRedmineTracked(state), projects),
  };
}
/** `redmine_projects`: wszystkie projekty (wybór śledzonych w Ustawieniach, R5). */
export async function redmineProjects(): Promise<RedmineProject[]> {
  if (desktop) return invoke("redmine_projects");
  return previewRedmineProjectList(previewRedmineConnected()).map((p) => ({
    ...p,
  }));
}
/** `redmine_select_projects`: śledzone projekty (najwyżej 50). */
export async function redmineSelectProjects(
  projects: RedmineProject[],
): Promise<void> {
  if (desktop)
    return notifyRedmineAfter(invoke("redmine_select_projects", { projects }));
  if (
    projects.length > 50 ||
    projects.some((p) => p.id <= 0 || !p.name.trim())
  )
    throw new Error("Wybierz maksymalnie 50 projektów.");
  const state = previewRedmineState();
  const seen = new Set<number>();
  state.selectedProjects = projects
    .filter((p) => !seen.has(p.id) && Boolean(seen.add(p.id)))
    .map((p) => ({ ...p, name: p.name.trim() }));
  savePreviewRedmine(state);
  notifyRedmine();
}
/** `redmine_statuses`: statusy w kolejności z Redmine. */
export async function redmineStatuses(
  refresh: boolean,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_statuses", { refresh });
  return previewRedmineStatusList(previewRedmineConnected()).map((s) => ({
    ...s,
  }));
}
/**
 * `redmine_start`: zadanie zgłoszenia (nowe albo z kosza/archiwum, z powrotem
 * do „do zrobienia”) i jeden pomiar, jak `redmine::start_issue`.
 */
export async function redmineStart(id: number): Promise<Snapshot> {
  if (desktop) return invoke("redmine_start", { id });
  const state = previewRedmineConnected();
  const source = previewRedmineList(state).find((i) => i.id === id);
  if (!source) throw new Error("Odśwież listę zgłoszeń przed rozpoczęciem.");
  const issue = previewRedmineFull(source, state);
  const data = await read();
  const now = Date.now();
  let task = data.tasks.find((t) => t.redmineId === id);
  const trashed = data.trash?.find((t) => t.redmineId === id);
  if (!task && trashed) {
    data.trash = data.trash?.filter((t) => t !== trashed);
    data.sessions.push(
      ...(data.trashSessions ?? []).filter((s) => s.taskId === trashed.id),
    );
    data.trashSessions = data.trashSessions?.filter(
      (s) => s.taskId !== trashed.id,
    );
    data.tasks.unshift(trashed);
    task = trashed;
  }
  if (!task) {
    task = {
      id:
        Math.max(
          0,
          ...data.tasks.map((t) => t.id),
          ...(data.trash ?? []).map((t) => t.id),
        ) + 1,
      title: [...issue.subject].slice(0, 240).join(""),
      done: false,
      createdAt: now,
      redmineId: id,
      redmineUrl: issue.url,
      redmineStatus: issue.status,
      redmineTracker: issue.tracker,
      redmineDue: issue.dueDate,
    };
    const band = priorityBand(issue.priority);
    if (band !== null) task.details = { ...detailsOf(task), priority: band };
    data.tasks.unshift(task);
  }
  const started = task;
  started.done = false;
  started.archived = false;
  started.details = unblocked(started);
  if (
    !data.sessions.some((s) => s.taskId === started.id && s.endedAt === null)
  ) {
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
    data.sessions.unshift({
      id:
        Math.max(
          0,
          ...data.sessions.map((s) => s.id),
          ...(data.trashSessions ?? []).map((s) => s.id),
        ) + 1,
      taskId: started.id,
      startedAt: now,
      endedAt: null,
    });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** `redmine_open`: zgłoszenie w przeglądarce (w podglądzie tylko zapisany link). */
export async function redmineOpen(url: string): Promise<void> {
  if (desktop) return invoke("redmine_open", { url });
  try {
    const opened = JSON.parse(
      localStorage.getItem(previewRedmineOpenedKey) ?? "[]",
    );
    localStorage.setItem(
      previewRedmineOpenedKey,
      JSON.stringify(
        [...(Array.isArray(opened) ? opened : []), url].slice(-20),
      ),
    );
  } catch {
    /* tylko ślad dla testów */
  }
}
