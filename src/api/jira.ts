/**
 * Jira: typy komend, stałe i reguły z `jira.rs`, `jira_move.rs`,
 * `jira_board.rs` i `jira_fields.rs`, wspólne dla aplikacji i podglądu.
 */
import type { Snapshot } from "./task";
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
export function checkKey(issueKey: string) {
  if (!jiraKeyPattern.test(issueKey) || issueKey.length > 100)
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
}
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
export const projectPattern = /^[A-Z][A-Z0-9_]{0,19}$/;
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
