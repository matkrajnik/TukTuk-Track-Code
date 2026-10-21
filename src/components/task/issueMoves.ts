import { reactive, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  desktop,
  jiraTransition,
  jiraTransitions,
  redmineOpen,
  redmineTransition,
  redmineTransitions,
  setJiraNoAsk,
  type JiraTransition,
  type RedmineStatus,
  type Snapshot,
  type Task,
} from "../../store";
import type { IssueRef } from "../../issue";

/**
 * Zmiany zgłoszenia z kafelka zadania: „Przesuń w Jirze…” (J1), „Przesuń
 * w Redmine…” (PLAN_1.9.md, R4) i „Pytaj znowu”, z własnym `busy` i błędem
 * pokazywanym przy kafelku. Nic nie zmienia się bez kliknięcia.
 */
export function useIssueMoves(
  task: () => Task,
  updated: (data: Snapshot) => void,
  failed: (message: string) => void,
) {
  /** Przejścia zgłoszenia Jiry; `null` = lista zwinięta. */
  const jiraList = ref<JiraTransition[] | null>(null);
  /** Dozwolone statusy zgłoszenia Redmine bez bieżącego; `null` = zwinięte. */
  const redmineList = ref<RedmineStatus[] | null>(null);
  const busy = ref(false);
  /** Błąd zgłoszenia przy kafelku: komunikat z linkiem do zgłoszenia. */
  const error = ref("");
  async function run(action: () => Promise<void>) {
    if (busy.value) return;
    busy.value = true;
    error.value = "";
    try {
      await action();
    } catch (e) {
      error.value = String(e instanceof Error ? e.message : e);
    } finally {
      busy.value = false;
    }
  }
  const openJiraList = () =>
    run(async () => {
      jiraList.value =
        jiraList.value === null ? await jiraTransitions(task().id) : null;
    });
  const moveJira = (t: JiraTransition) =>
    run(async () => {
      updated(await jiraTransition(task().id, t.id));
      jiraList.value = null;
    });
  const askAgain = () =>
    run(async () => updated(await setJiraNoAsk(task().id, false)));
  const openRedmineList = () =>
    run(async () => {
      redmineList.value =
        redmineList.value === null ? await redmineTransitions(task().id) : null;
    });
  const moveRedmine = (s: RedmineStatus) =>
    run(async () => {
      updated(await redmineTransition(task().id, s.id));
      redmineList.value = null;
    });
  async function openJira(url: string) {
    try {
      if (desktop) await invoke("jira_open", { url });
    } catch (e) {
      failed(String(e));
    }
  }
  /** „Otwórz w Jirze” / „Otwórz w Redmine”: link tylko pod adresem skonfigurowanego konta. */
  async function openIssue(issue: IssueRef) {
    if (issue.system === "jira") return openJira(issue.url);
    try {
      await redmineOpen(issue.url);
    } catch (e) {
      failed(String(e));
    }
  }
  return reactive({
    jiraList,
    redmineList,
    busy,
    error,
    openJiraList,
    moveJira,
    closeJira: () => (jiraList.value = null),
    askAgain,
    openRedmineList,
    moveRedmine,
    closeRedmine: () => (redmineList.value = null),
    openJira,
    openIssue,
  });
}
export type IssueMoves = ReturnType<typeof useIssueMoves>;
