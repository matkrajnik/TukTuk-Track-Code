import { test, expect, type Page } from "./fixtures/test";
import { applySettings, defaultSettings } from "../src/settings";
import { jiraFieldName, priorityBand } from "../src/store";
import { openPreview, stored } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Pola Jiry na kafelku (Ustawienia → Jira → „Pola Jiry na kafelku”) na
 * symulowanej Jirze podglądu (`store.ts`): chipy etykiet i przypisanego na
 * kartach widoku Jira i kafelku zadania, wybór i kolejność pól (także
 * niestandardowych) oraz priorytet z Jiry jako pasmo priorytetu zadania.
 * Reguły zapisu (`jira_fields::save_fields`, `priority_band`) są pokryte
 * testami Rust; tu sprawdzamy, że interfejs je pokazuje i zapisuje ustawienie.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const BOARD_KEY = "todo-track-preview-jira-board-v1";
const taskCard = (page: Page, title: string) =>
  page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: title }) });
const chips = (scope: ReturnType<Page["locator"]>) => scope.locator(".jira-field");
const tasksView = (page: Page) =>
  page.getByRole("button", { name: "Zadania", exact: true }).click();
const jiraView = (page: Page) =>
  page.getByRole("button", { name: "Jira", exact: true }).click();

test("pasmo priorytetu, nazwy pól i walidacja listy pól jak w Rust (moduł)", () => {
  expect(priorityBand("Highest")).toBe(1);
  expect(priorityBand("high")).toBe(1);
  expect(priorityBand("Medium")).toBe(0);
  expect(priorityBand("Low")).toBe(-1);
  expect(priorityBand("Najniższy")).toBe(-1);
  expect(priorityBand("")).toBeNull();
  expect(priorityBand("P3")).toBeNull();
  expect(jiraFieldName("labels")).toBe("Etykiety");
  expect(jiraFieldName("customfield_9")).toBe("customfield_9");
  expect(
    jiraFieldName("customfield_9", [
      { id: "customfield_9", name: "Sprint", custom: true, kind: "array" },
    ]),
  ).toBe("Sprint");
  expect(defaultSettings.jiraCardFields).toEqual(["labels", "assignee"]);
  expect(
    applySettings(defaultSettings, {
      jiraCardFields: [" labels ", "labels", "customfield_10020"],
    }).jiraCardFields,
  ).toEqual(["labels", "customfield_10020"]);
  for (const bad of [["summary"], ["status"], ["custom field"], ["1abc"], ["a", "b", "c", "d", "e", "f", "g"]])
    expect(() => applySettings(defaultSettings, { jiraCardFields: bad })).toThrow(
      "Pola Jiry na kafelku",
    );
});

test("chipy pól na kartach Jiry i kafelku zadania; wybór, kolejność i usuwanie pól w Ustawieniach", async ({
  page,
}) => {
  await openPreview(page);
  await jiraView(page);
  // Domyślnie etykiety (osobne chipy) i przypisany.
  const issue = page.locator(".jira-issue", { hasText: "WMS-31" });
  await expect(chips(issue)).toHaveText(["magazyn", "pilne", "Przypisany: Jan Kowalski"]);
  await expect(chips(issue).first()).toHaveAttribute("title", "Etykiety: magazyn, pilne");
  await expect(chips(page.locator(".jira-issue", { hasText: "FND-3" }))).toHaveText([
    "Przypisany: Jan Kowalski",
  ]);
  // Tablica pokazuje te same pola na karcie.
  await page.getByRole("button", { name: "Tablica", exact: true }).click();
  await expect(chips(page.locator('.jira-card[data-key="WMS-31"]'))).toHaveText([
    "magazyn",
    "pilne",
    "Przypisany: Jan Kowalski",
  ]);
  await page.getByRole("button", { name: "Lista", exact: true }).click();
  // „Rozpocznij” tworzy zadanie z chipami i pasmem priorytetu z Jiry (High → wysoki).
  await page.getByRole("button", { name: "Rozpocznij: WMS-31", exact: true }).click();
  await tasksView(page);
  const card = taskCard(page, "Obsługa przyjęcia towaru");
  await expect(chips(card)).toHaveText(["magazyn", "pilne", "Przypisany: Jan Kowalski"]);
  await expect(page.getByLabel("Priorytet: Obsługa przyjęcia towaru")).toHaveValue("1");
  // Zgłoszenie zadania bez etykiet: tylko przypisany (po pobraniu listy).
  await expect(chips(taskCard(page, "Analiza importu"))).toHaveText([
    "Przypisany: Jan Kowalski",
  ]);

  // Ustawienia → Jira: dodanie pola niestandardowego, kolejność, usunięcie.
  await openSettings(page, "Jira");
  const chosen = page.getByRole("list", { name: "Pola Jiry na kafelku" });
  await expect(chosen.getByRole("listitem")).toHaveText([/^Etykiety/, /^Przypisany/]);
  await page.getByRole("button", { name: "Dodaj pole…" }).click();
  await page.getByLabel("Szukaj pola").fill("sprint");
  const picker = page.locator(".jira-field-picker");
  const sprint = picker.getByRole("checkbox", { name: /^Sprint/ });
  await expect(picker.getByRole("checkbox")).toHaveCount(1);
  await sprint.check();
  await expect(page.getByText(/^Odświeżono pola \d+ zgłoszeń\.$/)).toBeVisible();
  await expect(chosen.getByRole("listitem")).toHaveText([
    /^Etykiety/,
    /^Przypisany/,
    /^Sprint/,
  ]);
  await page.getByRole("button", { name: "Przesuń wyżej: Sprint" }).click();
  await expect(chosen.getByRole("listitem")).toHaveText([/^Etykiety/, /^Sprint/, /^Przypisany/]);
  await expect(page.getByRole("button", { name: "Przesuń wyżej: Etykiety" })).toBeDisabled();
  await page.getByRole("button", { name: "Usuń pole: Przypisany" }).click();
  await expect(chosen.getByRole("listitem")).toHaveText([/^Etykiety/, /^Sprint/]);
  await expect(page.getByText("Wybrano 2 / 6")).toBeVisible();
  // Pola pokazywane zawsze nie są na liście do wyboru.
  await page.getByLabel("Szukaj pola").fill("");
  await expect(picker.getByRole("checkbox")).toHaveCount(9);
  await expect(picker.getByRole("checkbox", { name: /summary|status/ })).toHaveCount(0);
  // Ustawienie przeżywa przeładowanie; kafelek pokazuje nową listę w nowej kolejności.
  await page.reload();
  await tasksView(page);
  await expect(chips(taskCard(page, "Obsługa przyjęcia towaru"))).toHaveText([
    "magazyn",
    "pilne",
    "Sprint: Sprint 42",
  ]);
  await jiraView(page);
  await expect(chips(page.locator(".jira-issue", { hasText: "WMS-31" }))).toHaveText([
    "magazyn",
    "pilne",
    "Sprint: Sprint 42",
  ]);
  // Pusta lista: tylko klucz i status.
  await openSettings(page, "Jira");
  await page.getByRole("button", { name: "Usuń pole: Etykiety" }).click();
  await page.getByRole("button", { name: "Usuń pole: Sprint" }).click();
  await expect(chosen).toContainText("Brak pól");
  await tasksView(page);
  await expect(chips(taskCard(page, "Obsługa przyjęcia towaru"))).toHaveCount(0);
  await expect(taskCard(page, "Obsługa przyjęcia towaru").locator(".task-jira-key")).toHaveText(
    "WMS-31",
  );
});

test("priorytet z Jiry ustawia pasmo zadania; ręczna zmiana zostaje do kolejnej zmiany w Jirze", async ({
  page,
}) => {
  await openPreview(page);
  // WMS-12 („Analiza importu”, pasmo wysoki w danych) ma w Jirze priorytet Low.
  const setPriority = (priority: string) =>
    page.evaluate(
      ([k, p]) => {
        const state = JSON.parse(localStorage.getItem(k) ?? "{}");
        state.issues = [
          {
            key: "WMS-12",
            summary: "Analiza importu",
            status: "W toku",
            issueType: "Zadanie",
            priority: p,
            assignee: { accountId: "preview-me", displayName: "Jan Kowalski" },
            boards: [1],
            fields: { labels: "import" },
          },
        ];
        localStorage.setItem(k, JSON.stringify(state));
      },
      [BOARD_KEY, priority] as const,
    );
  await setPriority("Low");
  const priority = page.getByLabel("Priorytet: Analiza importu");
  await expect(priority).toHaveValue("1");
  await jiraView(page);
  await expect(page.locator(".jira-issue", { hasText: "WMS-12" })).toBeVisible();
  await tasksView(page);
  await expect(priority).toHaveValue("-1");
  await expect(chips(taskCard(page, "Analiza importu"))).toHaveText([
    "import",
    "Przypisany: Jan Kowalski",
  ]);
  // Ręczne pasmo zostaje, dopóki priorytet w Jirze się nie zmieni.
  await priority.selectOption("1");
  await jiraView(page);
  await expect(page.locator(".jira-issue", { hasText: "WMS-12" })).toBeVisible();
  await tasksView(page);
  await expect(priority).toHaveValue("1");
  // Zmiana w Jirze na Medium → pasmo normalne.
  await setPriority("Medium");
  await jiraView(page);
  await expect(page.locator(".jira-issue", { hasText: "WMS-12" })).toBeVisible();
  await tasksView(page);
  await expect(priority).toHaveValue("0");
  const data = await stored(page);
  expect(data.tasks.find((t: { jiraKey?: string }) => t.jiraKey === "WMS-12").details.priority).toBe(0);
});
