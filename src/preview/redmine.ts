/** Symulowany Redmine podglądu (PLAN_1.9.md, R6) z regułami `redmine.rs`. */
import { read, saveSnapshot } from "./snapshot";
import {
  chooseRedmineStatus,
  notifyRedmine,
  redmineCertError,
  redmineGroups,
  redmineIssueLimit,
  redmineKeyError,
  redmineKeyRejected,
  redmineNormalizeSite,
  redmineNotConnected,
  redmineNotesLimit,
  redmineTransitionOf,
  redmineValidApiKey,
  redmineWriteDisabled,
  trackedProjectIds,
  type RedmineAccount,
  type RedmineIssue,
  type RedmineIssues,
  type RedmineProject,
  type RedmineStatus,
} from "../api/redmine";
import { previewJson, previewSettingsKey } from "./storage";
import { detailsOf, unblocked, type Snapshot, type Task } from "../api/task";
import { branchName, priorityBand, type JiraMovePrompt } from "../api/jira";
import { redmineKey, validRedmineId } from "../issue";
import {
  notifyPreviewJira,
  previewJiraState,
  savePreviewJira,
} from "./jira-state";
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
export const previewRedmineKey = "tuktuk-track-preview-redmine-v1";
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
export function previewRedmineState(): PreviewRedmine {
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
export function previewRedmineIssue(id: number): PreviewRedmineIssue | undefined {
  const state = previewRedmineState();
  const list = Array.isArray(state.issues) ? state.issues : previewRedmineIssues;
  return list.find((i) => i.id === id);
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
export function previewRedmineStartPrompt(
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
    saveSnapshot(data);
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
  if (changed) saveSnapshot(data);
}
export async function redmineIssueTransitions(
  issueId: number,
): Promise<RedmineStatus[]> {
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
export async function redmineIssueTransition(
  issueId: number,
  statusId: number,
): Promise<RedmineIssue | null> {
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
export async function redmineTransitions(
  taskId: number,
): Promise<RedmineStatus[]> {
  return redmineIssueTransitions(await previewRedmineTask(taskId));
}
export async function redmineTransition(
  taskId: number,
  statusId: number,
): Promise<Snapshot> {
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
export async function redmineAssignMe(
  issueId: number,
): Promise<RedmineIssue | null> {
  if (!validRedmineId(issueId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  const state = previewRedmineWrite();
  const issue = previewRedmineEditable(state, issueId);
  delete issue.assignedToId;
  delete issue.assignedTo;
  savePreviewRedmine(state);
  return previewRedmineFull(issue, state);
}
export async function redmineComment(
  issueId: number,
  text: string,
): Promise<void> {
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
export async function redmineBranchName(issueId: number): Promise<string> {
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
/** Ślad linków „Otwórz w Redmine” w podglądzie (dla testów). */
export const previewRedmineOpenedKey = "tuktuk-track-preview-redmine-opened-v1";
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
export async function redmineAccount(): Promise<RedmineAccount> {
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
export async function redmineConnect(
  site: string,
  apiKey: string,
  acceptInvalidCert: boolean,
): Promise<void> {
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
}
export async function redmineTest(): Promise<string> {
  const state = previewRedmineConnected();
  if (state.certError && !state.acceptInvalidCert)
    throw new Error(redmineCertError);
  if (state.failTest) throw new Error(state.failTest);
  const user = previewRedmineUser(state);
  return `Połączono jako ${user.name} (${user.login}), Redmine odpowiada.`;
}
export async function redmineDisconnect(): Promise<void> {
  const state = previewRedmineState();
  state.connected = false;
  savePreviewRedmine(state);
}
export async function redmineIssues(
  projectId: number | null,
  mine: boolean,
  openOnly: boolean,
): Promise<RedmineIssues> {
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
export async function redmineProjects(): Promise<RedmineProject[]> {
  return previewRedmineProjectList(previewRedmineConnected()).map((p) => ({
    ...p,
  }));
}
export async function redmineSelectProjects(
  projects: RedmineProject[],
): Promise<void> {
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
export async function redmineStatuses(
  refresh: boolean,
): Promise<RedmineStatus[]> {
  return previewRedmineStatusList(previewRedmineConnected()).map((s) => ({
    ...s,
  }));
}
export async function redmineStart(id: number): Promise<Snapshot> {
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
  saveSnapshot(data);
  return data;
}
export async function redmineOpen(url: string): Promise<void> {
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
