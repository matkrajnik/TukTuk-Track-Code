/**
 * Wspólny opis zgłoszenia na zadaniu (PLAN_1.9.md, R2). Zadanie ma najwyżej
 * jedno zgłoszenie: z Jiry (`jiraKey`) albo z Redmine (`redmineId`). Komponenty
 * pokazujące klucz, status i link (kafelek, karta tygodnia, szybkie
 * przełączanie, widget, poranny przegląd, raporty) używają `issueRef` zamiast
 * powielać `v-if="task.jiraKey"`; `JiraFieldChips` i `JiraIssueActions`
 * zostają tylko dla Jiry.
 */

export type IssueSystem = "jira" | "redmine";

export interface IssueRef {
  system: IssueSystem;
  /** `WMS-12` dla Jiry, `#1234` dla Redmine. */
  key: string;
  /** Adres zgłoszenia (pusty, gdy nieznany). */
  url: string;
  /** Ostatni znany status (pusty, gdy nieznany). */
  status: string;
  /** „Jira” / „Redmine” do etykiet i tytułów. */
  label: string;
}

/** Pola zadania potrzebne do opisu zgłoszenia (podzbiór `Task` ze `store.ts`). */
export interface IssueFields {
  jiraKey?: string | null;
  jiraUrl?: string | null;
  jiraStatus?: string | null;
  redmineId?: number | null;
  redmineUrl?: string | null;
  redmineStatus?: string | null;
}

export const issueLabel = (system: IssueSystem): string =>
  system === "jira" ? "Jira" : "Redmine";

/** Czy wartość to poprawny numer zgłoszenia Redmine (1 ≤ id, do 10 cyfr). */
export const validRedmineId = (id: unknown): id is number =>
  typeof id === "number" && Number.isInteger(id) && id >= 1 && id <= 9_999_999_999;

/** `#1234` z numeru zgłoszenia Redmine. */
export const redmineKey = (id: number): string => `#${id}`;

/**
 * Zgłoszenie zadania albo `null`. Gdy zadanie ma (błędnie) oba pola, wygrywa
 * Jira — z ostrzeżeniem w konsoli, bo backend nie dopuszcza takiego stanu.
 */
export function issueRef(task: IssueFields | null | undefined): IssueRef | null {
  if (!task) return null;
  const jira = task.jiraKey?.trim() ?? "";
  const redmine = validRedmineId(task.redmineId) ? task.redmineId : null;
  if (jira) {
    if (redmine !== null)
      console.warn(
        `Zadanie ma dwa zgłoszenia (${jira} i ${redmineKey(redmine)}); pokazuję Jirę.`,
      );
    return {
      system: "jira",
      key: jira,
      url: task.jiraUrl ?? "",
      status: task.jiraStatus?.trim() ?? "",
      label: issueLabel("jira"),
    };
  }
  if (redmine !== null)
    return {
      system: "redmine",
      key: redmineKey(redmine),
      url: task.redmineUrl ?? "",
      status: task.redmineStatus?.trim() ?? "",
      label: issueLabel("redmine"),
    };
  return null;
}

/** „Status w Jirze: …” / „Status w Redmine: …” do podpowiedzi chipa statusu. */
export const statusTitle = (issue: IssueRef): string =>
  `Status w ${issue.system === "jira" ? "Jirze" : "Redmine"}: ${issue.status}`;

/** Termin zgłoszenia Redmine (`RRRR-MM-DD`) jako „DD.MM” do chipa; pusty, gdy brak. */
export function dueChip(due: string | null | undefined): string {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(due?.trim() ?? "");
  return match ? `${match[3]}.${match[2]}` : "";
}
