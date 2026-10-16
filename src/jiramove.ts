import { ref } from "vue";
import { previewFocusHolds } from "./focus";
import { readSettings } from "./settings";
import {
  desktop,
  jiraTransitions,
  readMovePrompts,
  type JiraMovePrompt,
  type JiraTransition,
  type Task,
} from "./store";

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
 * Po ukończeniu zadania z kluczem Jiry pobiera przejścia i pokazuje propozycję
 * w banerze. Wyłączone zapisy, brak połączenia albo błąd Jiry: bez propozycji.
 */
export async function offerAfterDone(task: Task): Promise<void> {
  if (!task.jiraKey) return;
  try {
    if (!(await readSettings()).jiraWrite) return;
    const transitions = await jiraTransitions(task.id);
    if (!transitions.length) return;
    donePrompts.value = [
      {
        taskId: task.id,
        key: task.jiraKey,
        title: task.title,
        url: task.jiraUrl ?? "",
        transitions,
      },
    ];
  } catch {
    // Jira niedostępna albo konto niepołączone: ukończenie bez propozycji.
  }
}

export function dropDonePrompt(taskId: number) {
  donePrompts.value = donePrompts.value.filter((p) => p.taskId !== taskId);
}
