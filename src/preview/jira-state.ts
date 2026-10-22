/**
 * Stan symulowanej Jiry podglądu: pytania o przesunięcie (J1), przepływ,
 * tablice, zgłoszenia przykładowe i pola kafelków (J2). Bez odczytu danych
 * zadań, więc korzysta z niego także `snapshot.ts`.
 */
import {
  jiraFieldName,
  jiraMoveEvent,
  type JiraBoardRef,
  type JiraColumn,
  type JiraFieldInfo,
  type JiraFieldValue,
  type JiraIssueType,
  type JiraMoveOption,
  type JiraMovePrompt,
  type JiraPerson,
  type JiraTransition,
  type JiraWatchStatus,
} from "../api/jira";
import { previewJson, previewSettingsKey } from "./storage";
import type { Snapshot } from "../api/task";
/**
 * Symulowana Jira podglądu: ostatnio widziany pomiar i pytania; status zgłoszenia
 * to status z kafelka (pusty = „Do zrobienia”). `fail` (400 albo 403) symuluje
 * odrzucenie przejścia przez Jirę. `watch` to stan obserwacji przypisań (B5),
 * a `watchFail` — komunikat błędu, którym kończy się kolejne sprawdzenie
 * (np. wygasły token).
 */
export const previewJiraKey = "tuktuk-track-preview-jira-v1";
export interface PreviewJira {
  ready?: boolean;
  seen?: [number, number] | null;
  prompts?: JiraMovePrompt[];
  fail?: number;
  watch?: JiraWatchStatus;
  watchFail?: string;
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
export const previewTransitionsFor = (status: string) =>
  previewWorkflow
    .filter((t) => t.toStatus.toLowerCase() !== status.trim().toLowerCase())
    .map((t) => ({ ...t }));
export function previewJiraState(): PreviewJira {
  return previewJson(previewJiraKey) as PreviewJira;
}
export function savePreviewJira(state: PreviewJira) {
  // Bez zmian nie zapisujemy: inne okna podglądu (widget) dostają `storage`.
  const raw = JSON.stringify(state);
  if (localStorage.getItem(previewJiraKey) !== raw)
    localStorage.setItem(previewJiraKey, raw);
}
/** Ustawienia zapisu do Jiry w podglądzie (wartości domyślne jak w Rust). */
export function previewJiraSettings() {
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
export function notifyPreviewJira() {
  window.dispatchEvent(new Event(jiraMoveEvent));
}
/** Symulowana Jira podglądu (J2). */
export const previewJiraBoardKey = "tuktuk-track-preview-jira-board-v1";
/** Linki otwarte „w przeglądarce” w podglądzie (do sprawdzenia w testach). */
export const previewJiraOpenedKey = "tuktuk-track-preview-jira-opened-v1";
export interface PreviewIssue {
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
export interface PreviewBoardState {
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
export const bandName = (band: number) =>
  band > 0 ? "High" : band < 0 ? "Low" : "Medium";
/** Pola z ustawień w podglądzie (jak `jira_fields::card_fields`). */
function previewCardFields(): string[] {
  const raw = previewJson(previewSettingsKey).jiraCardFields;
  return Array.isArray(raw)
    ? raw.filter((id): id is string => typeof id === "string")
    : ["labels", "assignee"];
}
/** Wartości wybranych pól zgłoszenia symulowanej Jiry (tylko niepuste). */
export function previewIssueFields(
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
export function attachPreviewJiraFields(data: Snapshot) {
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
export const previewPeople: JiraPerson[] = [
  { ...previewJiraMe },
  { accountId: "preview-anna", displayName: "Anna Nowak" },
  { accountId: "preview-piotr", displayName: "Piotr Wiśniewski" },
];
export const previewBoards: JiraBoardRef[] = [
  { id: 1, name: "WMS" },
  { id: 2, name: "Foundation" },
];
export const previewBoardProject: Record<number, string> = { 1: "WMS", 2: "FND" };
/** Statusy symulowanej Jiry; „Testy” nie ma kolumny (trafia według kategorii). */
const previewStatuses = [
  { id: "1", name: "Do zrobienia", category: "new" },
  { id: "3", name: "In Progress", category: "indeterminate" },
  { id: "4", name: "Code review", category: "indeterminate" },
  { id: "5", name: "Gotowe", category: "done" },
  { id: "6", name: "Testy", category: "indeterminate" },
];
export const previewColumns: JiraColumn[] = [
  { name: "Do zrobienia", statusIds: ["1"] },
  { name: "W toku", statusIds: ["3"] },
  { name: "Code review", statusIds: ["4"] },
  { name: "Gotowe", statusIds: ["5"] },
];
export const previewCategories: Record<string, string> = Object.fromEntries(
  previewStatuses.map((s) => [s.id, s.category]),
);
export function previewStatusId(name: string): string {
  const n = name.trim().toLowerCase();
  return previewStatuses.find((s) => s.name.toLowerCase() === n)?.id ?? "";
}
/** Przejścia tablicy: przepływ z J1, „Do testów” i „Zakończ” z polem Rozwiązanie. */
export function previewMoveOptions(status: string): JiraMoveOption[] {
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
export const previewTypes: (JiraIssueType & { required: string[] })[] = [
  { id: "10001", name: "Zadanie", required: [] },
  { id: "10002", name: "Błąd", required: ["Środowisko"] },
  { id: "10004", name: "Historia", required: [] },
];
export const previewIssueSeed = (): PreviewIssue[] => [
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
