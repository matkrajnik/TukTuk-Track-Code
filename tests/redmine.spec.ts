import { test, expect, type Page } from "@playwright/test";
import { at, openPreview, seed, stored, TODAY } from "./fixtures/seed";

/**
 * Zgłoszenie Redmine na zadaniu (PLAN_1.9.md, R2): chipy `#1234` ze statusem,
 * trackerem i terminem na kafelku, karcie tygodnia i w widgecie, przycisk
 * „Otwórz w Redmine”, nazwa w szybkim przełączaniu oraz szybkie dodawanie
 * `#1234` z tematem z symulowanego Redmine. R6 rozbudowuje ten plik o widok
 * Redmine, zapisy i Ustawienia.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const REDMINE_SITE = "https://redmine.example.pl";

/** Dane fixture plus zadanie z Redmine `#1234` (status, tracker, termin) i `#1240` bez terminu. */
function redmineSeed() {
  const data = seed();
  data.tasks.unshift(
    {
      id: 20,
      title: "Import CSV zamówień",
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1234,
      redmineUrl: `${REDMINE_SITE}/issues/1234`,
      redmineStatus: "In Progress",
      redmineTracker: "Funkcja",
      redmineDue: "2026-10-30",
      details: {
        ...data.tasks[0].details,
        priority: 0,
        project: "Sklep",
        plannedDay: TODAY,
        dueDay: "",
        estimateMinutes: 0,
        notes: {},
        checklist: [],
        pinnedAt: at("2026-10-14", "09:30"),
      },
    } as any,
    {
      id: 21,
      title: "Błąd zapisu etykiet",
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1240,
      redmineUrl: `${REDMINE_SITE}/issues/1240`,
      redmineStatus: "Feedback",
      redmineTracker: "Błąd",
      redmineDue: "",
      details: { ...data.tasks[0].details, priority: 0, project: "", plannedDay: "", dueDay: "", estimateMinutes: 0, notes: {}, checklist: [] },
    } as any,
  );
  return data;
}

const card = (page: Page, title: string) =>
  page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: title }) });

test("task card shows #number, status, tracker and due chips and „Otwórz w Redmine”", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  const csv = card(page, "Import CSV zamówień");
  await expect(csv.locator(".task-issue-key")).toHaveText("#1234");
  await expect(csv.locator(".task-redmine-key")).toHaveText("#1234");
  await expect(csv.locator(".task-issue-status")).toHaveText("In Progress");
  await expect(csv.locator(".task-redmine-status")).toHaveAttribute(
    "title",
    "Status w Redmine: In Progress",
  );
  await expect(csv.locator(".task-redmine-tracker")).toHaveText("Funkcja");
  await expect(csv.locator(".task-redmine-due")).toHaveText("termin 30.10");
  // Pola Jiry na kafelku zostają tylko dla Jiry.
  await expect(csv.locator(".jira-field-chip")).toHaveCount(0);
  // Bez terminu nie ma chipa terminu; klasy Jiry nie pojawiają się na zadaniu Redmine.
  const labels = card(page, "Błąd zapisu etykiet");
  await expect(labels.locator(".task-redmine-key")).toHaveText("#1240");
  await expect(labels.locator(".task-redmine-due")).toHaveCount(0);
  await expect(labels.locator(".task-jira-key")).toHaveCount(0);
  // Zadanie z Jiry bez zmian: klasy ogólne i klasy Jiry na tym samym elemencie.
  const jira = card(page, "Analiza importu");
  await expect(jira.locator(".task-issue-key.task-jira-key")).toHaveText("WMS-12");
  await expect(jira.locator(".task-issue-status.task-jira-status")).toHaveText("W toku");
  await expect(jira.locator(".task-redmine-key")).toHaveCount(0);

  // Rozwinięty kafelek: „Otwórz w Redmine”, bez akcji Jiry.
  await page
    .getByRole("button", { name: "Przedziały: Import CSV zamówień", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Otwórz w Redmine: #1234", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /Przesuń w Jirze/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Otwórz w Jirze/ })).toHaveCount(0);
});

test("„Czekam na” proposal uses Redmine statuses for a Redmine task", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  const labels = card(page, "Błąd zapisu etykiet");
  const proposal = page.getByRole("group", {
    name: "Propozycja z Redmine: Błąd zapisu etykiet",
  });
  // Domyślna lista Redmine to „Feedback”.
  await expect(proposal).toContainText("Redmine: Feedback — oznaczyć jako czekam na?");
  await expect(
    page.getByRole("group", { name: "Propozycja z Jiry: Błąd zapisu etykiet" }),
  ).toHaveCount(0);
  await proposal.getByRole("button", { name: "Oznacz jako czekam na" }).click();
  await expect(labels.getByText("Czekam na: Redmine: Feedback")).toBeVisible();
  const task = (await stored(page)).tasks.find((t: any) => t.id === 21);
  expect(task.details.waiting).toBe("Redmine: Feedback");
  expect(task.details.waitingStatus).toBe("Feedback");
  // Status Jiry „In Progress” nie jest na liście Redmine: brak propozycji.
  await expect(
    page.getByRole("group", { name: "Propozycja z Redmine: Import CSV zamówień" }),
  ).toHaveCount(0);
});

test("week card and week board search show the Redmine number", async ({ page }) => {
  await openPreview(page, { data: redmineSeed() });
  await page.getByRole("button", { name: "Plan i historia", exact: true }).click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  const column = page.getByRole("region", { name: "Czwartek 2026-10-15", exact: true });
  const week = column.locator(".week-card", { hasText: "Import CSV zamówień" });
  await expect(week.locator(".task-issue-key.task-redmine-key")).toHaveText("#1234");
  await expect(week.locator(".task-issue-status.task-redmine-status")).toHaveText(
    "In Progress",
  );
  // Wyszukiwanie w „Do zaplanowania” po numerze zgłoszenia.
  const backlog = page.getByRole("region", { name: "Do zaplanowania", exact: true });
  await expect(
    backlog.locator(".week-card", { hasText: "Błąd zapisu etykiet" }).locator(".task-redmine-key"),
  ).toHaveText("#1240");
  await page.getByLabel("Szukaj w Do zaplanowania").fill("#1240");
  await expect(backlog.locator(".week-card-title")).toHaveText(["Błąd zapisu etykiet"]);
});

test("quick switch creates a task from #number with the subject from Redmine", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await dialog.getByLabel("Znajdź zadanie do uruchomienia").fill("#1250 ~1h");
  await expect(dialog.locator(".quick-chip")).toHaveText([
    "Redmine: #1250",
    "Szacunek: 1 h",
  ]);
  await expect(dialog.locator(".quick-create .selected > span")).toHaveText(
    "Utwórz i rozpocznij: #1250",
  );
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  const created = card(page, "Aktualizacja dokumentacji API");
  await expect(created.locator(".task-redmine-key")).toHaveText("#1250");
  await expect(created.locator(".task-redmine-status")).toHaveText("In Progress");
  await expect(
    page.getByLabel("Stop: Aktualizacja dokumentacji API", { exact: true }),
  ).toBeVisible();
  const task = (await stored(page)).tasks.find((t: any) => t.redmineId === 1250);
  expect(task).toMatchObject({
    title: "Aktualizacja dokumentacji API",
    redmineUrl: `${REDMINE_SITE}/issues/1250`,
    redmineTracker: "Wsparcie",
    details: { estimateMinutes: 60 },
  });
  expect(task.jiraKey).toBeUndefined();
});

test("widget pins show the Redmine number instead of the title", async ({ page }) => {
  await openPreview(page, { data: redmineSeed() });
  await page.setViewportSize({ width: 320, height: 96 });
  await page.goto("/#widget");
  await page.reload();
  const pins = page
    .locator("#widget")
    .getByRole("group", { name: "Przypięte zadania" })
    .getByRole("button");
  await expect(pins).toHaveText(["#1234"]);
});

test("quick add: #1234 links the task, takes the subject and rejects duplicates and two issues", async ({
  page,
}) => {
  await openPreview(page);
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  const chips = page.getByLabel("Rozpoznane pola");
  const addButton = page.getByRole("button", { name: "Dodaj zadanie", exact: true });

  // Sam numer: tytuł z tematu symulowanego Redmine, chipy na kafelku.
  await input.fill("#1234");
  await expect(chips.getByRole("button")).toHaveText(["Redmine: #1234"]);
  await expect(addButton).toBeEnabled();
  await input.press("Enter");
  const created = card(page, "Import CSV zamówień");
  await expect(created.locator(".task-redmine-key")).toHaveText("#1234");
  await expect(created.locator(".task-redmine-status")).toHaveText("New");
  await expect(created.locator(".task-redmine-tracker")).toHaveText("Funkcja");
  await expect(created.locator(".task-redmine-due")).toHaveText("termin 30.10");
  const data = await stored(page);
  const task = data.tasks.find((t: any) => t.redmineId === 1234);
  expect(task).toMatchObject({
    title: "Import CSV zamówień",
    redmineUrl: `${REDMINE_SITE}/issues/1234`,
    redmineStatus: "New",
    redmineTracker: "Funkcja",
    redmineDue: "2026-10-30",
  });
  expect(task.jiraKey).toBeUndefined();
  // Dodanie nie uruchamia pomiaru.
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toEqual([]);

  // Ten sam numer drugi raz: błąd, bez nowego zadania.
  await input.fill("Inny tytuł #1234");
  await input.press("Enter");
  await expect(page.getByRole("alert").first()).toContainText(
    "#1234 ma już zadanie „Import CSV zamówień”.",
  );
  expect((await stored(page)).tasks.filter((t: any) => t.redmineId === 1234)).toHaveLength(1);

  // Nieznany temat bez tytułu: czytelny błąd.
  await input.fill("#9999");
  await input.press("Enter");
  await expect(page.getByRole("alert").first()).toContainText(
    "Nie znam tematu #9999. Wpisz tytuł albo odśwież listę w widoku Redmine.",
  );
  // Z własnym tytułem numer spoza listy też się wiąże (bez statusu).
  await input.fill("Własny tytuł #9999");
  await input.press("Enter");
  await expect(card(page, "Własny tytuł").locator(".task-redmine-key")).toHaveText("#9999");

  // Klucz Jiry i numer Redmine w jednej linii blokują dodanie.
  await input.fill("WMS-300 Raport #1240");
  await expect(chips).toContainText("Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.");
  await expect(addButton).toBeDisabled();
  await input.press("Enter");
  expect((await stored(page)).tasks.some((t: any) => t.redmineId === 1240)).toBe(false);

  // `\#1234` to zwykły tekst w tytule, `#wms` to projekt.
  await input.fill("Notatka \\#1234 #wms");
  await expect(chips.getByRole("button")).toHaveText(["Projekt: WMS"]);
  await input.press("Enter");
  await expect(card(page, "Notatka #1234")).toBeVisible();
});

test("quick add: with disconnected Redmine #1234 is a project name", async ({ page }) => {
  await openPreview(page);
  await page.evaluate(
    (key) => localStorage.setItem(key, JSON.stringify({ connected: false })),
    "todo-track-preview-redmine-v1",
  );
  await page.reload();
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  await input.fill("Raport #1234");
  await expect(page.getByLabel("Rozpoznane pola").getByRole("button")).toHaveText([
    "Projekt: 1234",
  ]);
  await input.press("Enter");
  const task = (await stored(page)).tasks.find((t: any) => t.title === "Raport");
  expect(task.details.project).toBe("1234");
  expect(task.redmineId).toBeUndefined();
});

test("export CSV has the Redmine column at the end", async ({ page }) => {
  const data = redmineSeed();
  data.sessions.push({
    id: 50,
    taskId: 20,
    startedAt: at(TODAY, "10:30"),
    endedAt: at(TODAY, "11:00"),
  });
  await openPreview(page, { data });
  await page.getByRole("button", { name: "Raporty dzienne", exact: true }).click();
  await page.getByRole("button", { name: "Miesiąc", exact: true }).click();
  const group = page.getByRole("group", { name: "Eksport miesiąca" });
  await group.getByRole("button", { name: "Eksport CSV" }).click();
  const text = await group
    .getByRole("textbox", { name: "Treść eksportu CSV" })
    .inputValue();
  const lines = text.slice(1).split("\n");
  expect(lines[0]).toBe(
    "Data;Start;Koniec;Minuty;Godziny;Zadanie;Klucz Jiry;Projekt;Notatka dnia;Uwagi;Redmine",
  );
  expect(lines).toContain("2026-10-15;10:30;11:00;30;0,50;Import CSV zamówień;;Sklep;;;#1234");
  expect(lines).toContain("2026-10-15;08:30;10:30;120;2,00;Analiza importu;WMS-12;WMS;;;");
});
