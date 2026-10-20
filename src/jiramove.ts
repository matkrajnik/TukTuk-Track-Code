import { ref } from "vue";
import { previewFocusHolds } from "./focus";
import { readSettings } from "./settings";
import {
  desktop,
  jiraTransition,
  jiraTransitions,
  readMovePrompts,
  redmineOpen,
  redmineTransition,
  redmineTransitionOf,
  redmineTransitions,
  type JiraMovePrompt,
  type JiraTransition,
  type Snapshot,
  type Task,
} from "./store";
import { issueRef, type IssueSystem } from "./issue";
import { invoke } from "@tauri-apps/api/core";

/**
 * Pytania o przesunięcie zgłoszenia przy starcie pracy (J1) widoczne w banerze
 * i widgecie. W czasie bloku skupienia czekają (jak `jira_move::publish`); w
 * podglądzie obserwacja startu i tak się odbywa, żeby po bloku pytanie wróciło.
 */
export async function readVisibleMovePrompts(): Promise<JiraMovePrompt[]> {
  const prompts = await readMovePrompts();
  return !desktop && previewFocusHolds() ? [] : prompts;
}

/** Propozycja po ukończeniu zadania: „Ukończone. Przesunąć KEY dalej?”. */
export interface DonePrompt {
  /** Jira albo Redmine (`issueRef(task).system`). */
  system: IssueSystem;
  taskId: number;
  key: string;
  title: string;
  url: string;
  /** Wszystkie dostępne przejścia, bez domyślnego wyboru. */
  transitions: JiraTransition[];
}
/** Propozycje po ukończeniu (tylko w oknie, które ukończyło zadanie). */
export const donePrompts = ref<DonePrompt[]>([]);

/**
 * Po ukończeniu zadania ze zgłoszeniem pobiera przejścia (Jira) albo dozwolone
 * statusy (Redmine, PLAN_1.9.md R4) i pokazuje propozycję w banerze. System
 * rozpoznaje `issueRef`. Wyłączone zapisy danego systemu, brak połączenia albo
 * błąd serwera: bez propozycji.
 */
export async function offerAfterDone(task: Task): Promise<void> {
  const issue = issueRef(task);
  if (!issue) return;
  try {
    const settings = await readSettings();
    let transitions: JiraTransition[];
    if (issue.system === "redmine") {
      if (!settings.redmineWrite) return;
      transitions = (await redmineTransitions(task.id)).map(redmineTransitionOf);
    } else {
      if (!settings.jiraWrite) return;
      transitions = await jiraTransitions(task.id);
    }
    if (!transitions.length) return;
    donePrompts.value = [
      {
        system: issue.system,
        taskId: task.id,
        key: issue.key,
        title: task.title,
        url: issue.url,
        transitions,
      },
    ];
  } catch {
    // Serwer niedostępny albo konto niepołączone: ukończenie bez propozycji.
  }
}

/** System pytania (starszy zapis podglądu bez pola to Jira). */
export const promptSystem = (p: { system?: IssueSystem }): IssueSystem =>
  p.system === "redmine" ? "redmine" : "jira";
/** „w Jirze” / „w Redmine” do tekstów banera i widgetu. */
export const inSystem = (system: IssueSystem) =>
  system === "redmine" ? "w Redmine" : "w Jirze";

/**
 * Wykonuje wybrane przejście pytania: Jira — `jira_transition`, Redmine —
 * `redmine_transition` (`id` przejścia to `status_id`). Zawsze po kliknięciu.
 */
export function performMove(
  system: IssueSystem,
  taskId: number,
  transitionId: string,
): Promise<Snapshot> {
  return system === "redmine"
    ? redmineTransition(taskId, Number(transitionId))
    : jiraTransition(taskId, transitionId);
}

/** „Otwórz … w Jirze / w Redmine” z banera (link tylko pod adresem konta). */
export async function openIssueUrl(system: IssueSystem, url: string) {
  if (system === "redmine") return redmineOpen(url);
  if (desktop) await invoke("jira_open", { url });
}

export function dropDonePrompt(taskId: number) {
  donePrompts.value = donePrompts.value.filter((p) => p.taskId !== taskId);
}
