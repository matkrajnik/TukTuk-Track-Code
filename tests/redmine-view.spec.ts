import { test, expect, type Page } from "@playwright/test";
import {
  ids,
  openPreview,
  REDMINE_KEY,
  REDMINE_SITE,
  stored,
} from "./fixtures/seed";

/**
 * Widok Redmine (PLAN_1.9.md, R3) na symulowanym Redmine podglądu: śledzony
 * projekt Magazyn WMS (z podprojektem „WMS – integracje”), zakres „Moje” /
 * „Wszystkie otwarte”, grupy statusów w kolejności z Redmine, filtr trackera,
 * wyszukiwanie, „Rozpocznij”, link, sekcja spoza śledzonych projektów oraz
 * stany bez projektów i bez konta. Żaden test nie łączy się z Redmine.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const openRedmine = (page: Page) =>
  page.getByRole("button", { name: "Redmine", exact: true }).click();
const view = (page: Page) => page.locator(".redmine-view");
const group = (page: Page, status: string) =>
  page.getByRole("region", { name: `Status ${status}`, exact: true });
const numbers = (page: Page, status: string) =>
  group(page, status).locator(".redmine-number");
const groupNames = (page: Page) => view(page).locator(".redmine-group-name");
const outside = (page: Page) =>
  page.getByRole("region", {
    name: "Przypisane do mnie spoza śledzonych projektów",
  });

async function setRedmine(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [REDMINE_KEY, JSON.stringify(state)] as const,
  );
  await page.reload();
}

test("navigation has Redmine right after Jira", async ({ page }) => {
  await openPreview(page);
  const items = page
    .getByRole("navigation", { name: "Nawigacja główna" })
    .locator(".nav-label");
  const labels = await items.allTextContents();
  expect(labels.indexOf("Redmine")).toBe(labels.indexOf("Jira") + 1);
  await openRedmine(page);
  await expect(
    page.getByRole("button", { name: "Redmine", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await expect(
    page.getByRole("heading", { level: 1, name: "Redmine" }),
  ).toBeVisible();
});

test("project issues grouped by status, Moje / Wszystkie otwarte and closed only without „Tylko otwarte”", async ({
  page,
}) => {
  await openPreview(page);
  await openRedmine(page);
  await expect(view(page).getByText("Połączono jako Jan Kowalski")).toBeVisible();
  const projects = page.getByRole("group", { name: "Projekt Redmine" });
  await expect(projects.getByRole("button")).toHaveText(["Magazyn WMS"]);
  await expect(
    projects.getByRole("button", { name: "Magazyn WMS" }),
  ).toHaveAttribute("aria-pressed", "true");
  const scope = page.getByRole("group", { name: "Zakres zgłoszeń" });
  await expect(scope.getByRole("button", { name: "Moje" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );

  // „Moje”: kolejność statusów z Redmine (New, In Progress, Feedback); w grupie
  // priorytet malejąco; podprojekt z nazwą na karcie.
  await expect(groupNames(page)).toHaveText(["New", "In Progress", "Feedback"]);
  await expect(numbers(page, "New")).toHaveText(["#1234"]);
  await expect(numbers(page, "In Progress")).toHaveText(["#1260", "#1250"]);
  await expect(numbers(page, "Feedback")).toHaveText(["#1240"]);
  await expect(group(page, "In Progress").locator(".jira-column-count")).toHaveText("2");
  const edi = page.getByRole("article", { name: "#1260 Mapowanie kodów EDI" });
  await expect(edi.locator(".redmine-project")).toHaveText("WMS – integracje");
  const csv = page.getByRole("article", { name: "#1234 Import CSV zamówień" });
  await expect(csv.locator(".redmine-project")).toHaveCount(0);
  await expect(csv.locator(".redmine-tracker")).toHaveText("Funkcja");
  await expect(csv.locator(".jira-priority")).toHaveText("High");
  await expect(csv.locator(".jira-priority")).toHaveClass(/high/);
  await expect(csv.locator(".redmine-version")).toHaveText("wersja 2.4");
  await expect(csv.locator(".redmine-due")).toHaveText("termin 30.10");
  await expect(csv.locator(".redmine-status")).toHaveText("New");
  // Moje zgłoszenia nie mają inicjałów osoby.
  await expect(csv.locator(".jira-avatar")).toHaveCount(0);
  await expect(view(page).locator(".jira-result-info")).toContainText(
    "Magazyn WMS · moje · 4 zgłoszeń",
  );

  // „Wszystkie otwarte”: także zgłoszenie Anny Nowak, z inicjałami.
  await scope.getByRole("button", { name: "Wszystkie otwarte" }).click();
  await expect(numbers(page, "New")).toHaveText(["#1234", "#1251"]);
  const other = page.getByRole("article", {
    name: "#1251 Raport stanów magazynowych",
  });
  await expect(other.getByLabel("Przypisane: Anna Nowak")).toHaveText("AN");
  // Przy „Wszystkich otwartych” nie ma przełącznika „Tylko otwarte”.
  await expect(view(page).getByLabel("Tylko otwarte")).toHaveCount(0);

  // „Moje” bez „Tylko otwarte”: grupa zamkniętych na końcu.
  await scope.getByRole("button", { name: "Moje" }).click();
  await view(page).getByLabel("Tylko otwarte").uncheck();
  await expect(groupNames(page)).toHaveText([
    "New",
    "In Progress",
    "Feedback",
    "Closed",
  ]);
  await expect(numbers(page, "Closed")).toHaveText(["#1245"]);

  // Grupa zwija się; liczba zostaje w nagłówku.
  const head = group(page, "New").getByRole("button", { name: /New/ });
  await head.click();
  await expect(head).toHaveAttribute("aria-expanded", "false");
  await expect(numbers(page, "New")).toHaveCount(0);
  await head.click();
  await expect(numbers(page, "New")).toHaveText(["#1234"]);

  // Wybór zakresu i „Tylko otwarte” zostaje po przeładowaniu.
  await scope.getByRole("button", { name: "Wszystkie otwarte" }).click();
  await page.reload();
  await openRedmine(page);
  await expect(
    page
      .getByRole("group", { name: "Zakres zgłoszeń" })
      .getByRole("button", { name: "Wszystkie otwarte" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(numbers(page, "New")).toHaveText(["#1234", "#1251"]);
});

test("tracker chips and search filter the cards", async ({ page }) => {
  await openPreview(page);
  await openRedmine(page);
  const chips = page.getByRole("group", { name: "Tracker" });
  await expect(chips.getByRole("button")).toHaveText([
    "Błąd",
    "Funkcja",
    "Wsparcie",
  ]);
  await chips.getByRole("button", { name: "Funkcja" }).click();
  await expect(
    chips.getByRole("button", { name: "Funkcja" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(view(page).locator(".redmine-number")).toHaveText([
    "#1234",
    "#1260",
  ]);
  await expect(groupNames(page)).toHaveText(["New", "In Progress"]);
  await chips.getByRole("button", { name: "Funkcja" }).click();
  await expect(view(page).locator(".redmine-groups .redmine-number")).toHaveCount(4);

  const search = page.getByLabel("Szukaj zgłoszeń Redmine");
  await search.fill("#1250");
  await expect(view(page).locator(".redmine-groups .redmine-number")).toHaveText([
    "#1250",
  ]);
  await search.fill("etykiet");
  await expect(view(page).locator(".redmine-groups .redmine-number")).toHaveText([
    "#1240",
  ]);
  await search.fill("nie ma takiego");
  await expect(
    view(page).getByText("Brak zgłoszeń pasujących do wyszukiwania."),
  ).toBeVisible();
});

test("„Rozpocznij” creates the task, switches the measurement and the link opens the issue", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await openRedmine(page);
  await page.getByRole("button", { name: "Rozpocznij: #1234" }).click();
  const csv = page.getByRole("article", { name: "#1234 Import CSV zamówień" });
  await expect(csv.getByText("mierzone teraz")).toBeVisible();
  await expect(
    csv.getByRole("button", { name: "Zatrzymaj: #1234" }),
  ).toBeVisible();
  const data = await stored(page);
  const task = data.tasks.find((t: any) => t.redmineId === 1234);
  expect(task).toMatchObject({
    title: "Import CSV zamówień",
    redmineUrl: `${REDMINE_SITE}/issues/1234`,
    redmineStatus: "New",
    redmineTracker: "Funkcja",
    redmineDue: "2026-10-30",
    done: false,
  });
  expect(task.details.priority).toBe(1);
  expect(task.jiraKey).toBeUndefined();
  // Jeden aktywny pomiar: poprzedni zatrzymany.
  const active = data.sessions.filter((s: any) => s.endedAt === null);
  expect(active).toHaveLength(1);
  expect(active[0].taskId).toBe(task.id);
  // Drugi „Rozpocznij” po zatrzymaniu nie dubluje zadania.
  await csv.getByRole("button", { name: "Zatrzymaj: #1234" }).click();
  await page.getByRole("button", { name: "Rozpocznij: #1234" }).click();
  await expect(csv.getByText("mierzone teraz")).toBeVisible();
  const again = await stored(page);
  expect(again.tasks.filter((t: any) => t.redmineId === 1234)).toHaveLength(1);

  // „Otwórz w Redmine” otwiera adres zgłoszenia (w podglądzie zapisany ślad).
  await page.getByRole("button", { name: "Otwórz w Redmine: #1240" }).click();
  const opened = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("todo-track-preview-redmine-opened-v1")!),
  );
  expect(opened).toEqual([`${REDMINE_SITE}/issues/1240`]);
});

test("issue from the trash comes back on „Rozpocznij”", async ({ page }) => {
  const data = (await openPreview(page)) as any;
  data.trash.push({
    id: 40,
    title: "Stary eksport",
    done: true,
    createdAt: 0,
    redmineId: 1240,
    redmineUrl: `${REDMINE_SITE}/issues/1240`,
  });
  await page.evaluate(
    (d) => localStorage.setItem("todo-track-preview-v1", JSON.stringify(d)),
    data,
  );
  await page.reload();
  await openRedmine(page);
  await page.getByRole("button", { name: "Rozpocznij: #1240" }).click();
  await expect(
    page
      .getByRole("article", { name: "#1240 Błąd zapisu etykiet" })
      .getByText("mierzone teraz"),
  ).toBeVisible();
  const after = await stored(page);
  expect(after.trash.some((t: any) => t.id === 40)).toBe(false);
  expect(after.tasks.find((t: any) => t.id === 40)).toMatchObject({
    title: "Stary eksport",
    done: false,
  });
});

test("section with my issues outside tracked projects", async ({ page }) => {
  await openPreview(page);
  await openRedmine(page);
  const section = outside(page);
  await expect(section.locator(".redmine-number")).toHaveText(["#1270"]);
  await expect(section.locator(".redmine-project")).toHaveText("Portal klienta");
  // Podprojekt śledzonego projektu nie jest „spoza”.
  await expect(section.getByText("#1260")).toHaveCount(0);
  await section.getByRole("button", { name: "Rozpocznij: #1270" }).click();
  await expect(section.getByText("mierzone teraz")).toBeVisible();
  const task = (await stored(page)).tasks.find((t: any) => t.redmineId === 1270);
  expect(task).toMatchObject({ title: "Formularz reklamacji" });
});

test("many tracked projects use a select and remember the choice", async ({
  page,
}) => {
  await openPreview(page);
  const project = (id: number, name: string) => ({
    id,
    name,
    identifier: `p${id}`,
    parentId: null,
  });
  await setRedmine(page, {
    selectedProjects: [
      project(7, "Magazyn WMS"),
      project(9, "Portal klienta"),
      project(20, "Projekt C"),
      project(21, "Projekt D"),
      project(22, "Projekt E"),
    ],
  });
  await openRedmine(page);
  const select = page.getByLabel("Projekt Redmine");
  await expect(select).toHaveValue("7");
  await select.selectOption({ label: "Portal klienta" });
  await expect(numbers(page, "New")).toHaveText(["#1270"]);
  // Wszystkie moje zgłoszenia są w śledzonych projektach.
  await expect(
    outside(page).getByText(
      "Brak otwartych zgłoszeń przypisanych do Ciebie w innych projektach.",
    ),
  ).toBeVisible();
  await page.reload();
  await openRedmine(page);
  await expect(page.getByLabel("Projekt Redmine")).toHaveValue("9");
});

test("without tracked projects the view lists my issues from all projects", async ({
  page,
}) => {
  await openPreview(page);
  await setRedmine(page, { selectedProjects: [] });
  await openRedmine(page);
  await expect(
    page.getByRole("heading", {
      name: "Moje zgłoszenia ze wszystkich projektów",
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Wybierz projekty w Ustawieniach" }),
  ).toBeVisible();
  await expect(page.getByRole("group", { name: "Projekt Redmine" })).toHaveCount(0);
  // Z terminem przed bez terminu przy tym samym paśmie priorytetu.
  await expect(numbers(page, "New")).toHaveText(["#1234", "#1270"]);
  await expect(numbers(page, "In Progress")).toHaveText(["#1260", "#1250"]);
  await expect(outside(page)).toHaveCount(0);
  await page
    .getByRole("button", { name: "Wybierz projekty w Ustawieniach" })
    .click();
  await expect(
    page.getByRole("button", { name: "Ustawienia", exact: true }),
  ).toHaveAttribute("aria-current", "page");
});

test("without an account the view links to Settings instead of a form", async ({
  page,
}) => {
  await openPreview(page);
  await setRedmine(page, { connected: false });
  await openRedmine(page);
  const link = page.getByRole("button", {
    name: "Połącz konto Redmine w Ustawieniach",
  });
  await expect(link).toBeVisible();
  await expect(view(page).locator("input")).toHaveCount(0);
  await expect(view(page).locator(".redmine-group")).toHaveCount(0);
  await expect(view(page).getByText("Połączono jako")).toHaveCount(0);
  await link.click();
  await expect(
    page.getByRole("button", { name: "Ustawienia", exact: true }),
  ).toHaveAttribute("aria-current", "page");
});
