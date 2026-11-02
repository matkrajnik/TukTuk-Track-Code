import { test, expect, type Page } from "./fixtures/test";
import { matchingSections, sectionInMessage } from "../src/navigation";
import {
  chooseRedmineStatus,
  previewRedmineStatuses,
  redmineCertError,
  redmineKeyRejected,
  redmineNormalizeSite,
  redmineTransitionOf,
  redmineWriteDisabled,
} from "../src/store";
import {
  at,
  ids,
  openPreview,
  REDMINE_KEY,
  REDMINE_SITE,
  redmineFixture,
  redmineIds,
  redmineSeed,
  seed,
  SETTINGS_KEY,
  stored,
  TODAY,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import { backend, openDesktop } from "./fixtures/tauri";
import { pick } from "./fixtures/searchSelect";

/**
 * Redmine obok Jiry (PLAN_1.9.md): wszystkie scenariusze e2e w jednym pliku,
 * po jednym bloku na zadanie — R2 (zgłoszenie na zadaniu, szybkie dodawanie
 * `#1234`), R3 (widok Redmine i obserwacja przypisań), R4 (zapisy: pytanie
 * przy starcie, „Przesuń w Redmine…”, po ukończeniu, „Przypisz do mnie”,
 * komentarz), R5 (Ustawienia → Redmine) i R6 (pełna ścieżka od Ustawień do
 * kafelka oraz zastępcze IPC z `redmineFixture()`). Testy działają na
 * symulowanym Redmine podglądu (`localStorage`) albo na zastępczym IPC; żaden
 * nie łączy się z Redmine, a adresy są przykładowe.
 */
test.use({ timezoneId: "Europe/Warsaw" });

test.describe("R2 — zgłoszenie Redmine na zadaniu", () => {
  /**
   * Zgłoszenie Redmine na zadaniu (PLAN_1.9.md, R2): chipy `#1234` ze statusem,
   * trackerem i terminem na kafelku, karcie tygodnia i w widgecie, przycisk
   * „Otwórz w Redmine”, nazwa w szybkim przełączaniu oraz szybkie dodawanie
   * `#1234` z tematem z symulowanego Redmine. R6 rozbudowuje ten plik o widok
   * Redmine, zapisy i Ustawienia.
   */

  /**
   * `redmineSeed()` pod chipy: `#1234` już „In Progress” i przypięte w
   * widgecie, `#1240` bez planu na dziś.
   */
  function chipSeed() {
    const data = redmineSeed();
    const task = (id: number) => (data.tasks as any[]).find((t) => t.id === id);
    task(redmineIds.importCsv).redmineStatus = "In Progress";
    task(redmineIds.importCsv).details.pinnedAt = at("2026-10-14", "09:30");
    task(redmineIds.labels).details.plannedDay = "";
    return data;
  }

  const card = (page: Page, title: string) =>
    page
      .locator(".task-card")
      .filter({ has: page.locator(".task-title", { hasText: title }) });

  test("task card shows #number, status, tracker and due chips and „Otwórz w Redmine”", async ({
    page,
  }) => {
    await openPreview(page, { data: chipSeed() });
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
    await openPreview(page, { data: chipSeed() });
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
    await openPreview(page, { data: chipSeed() });
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
    await openPreview(page, { data: chipSeed() });
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
    await openPreview(page, { data: chipSeed() });
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
    const addButton = page.getByRole("button", { name: "Dodaj i rozpocznij", exact: true });

    // Sam numer: tytuł z tematu symulowanego Redmine, chipy na kafelku.
    await input.fill("#1234");
    await expect(chips.getByRole("button")).toHaveText(["Redmine: #1234"]);
    await expect(addButton).toBeEnabled();
    await input.press("Shift+Enter");
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
    // „Zaplanuj” (Shift+Enter) nie uruchamia pomiaru.
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
      "tuktuk-track-preview-redmine-v1",
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
    const data = chipSeed();
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
});

test.describe("R3 — widok Redmine", () => {
  /**
   * Widok Redmine (PLAN_1.9.md, R3) na symulowanym Redmine podglądu: śledzony
   * projekt Magazyn WMS (z podprojektem „WMS – integracje”), zakres „Moje” /
   * „Wszystkie otwarte”, grupy statusów w kolejności z Redmine, filtr trackera,
   * wyszukiwanie, „Rozpocznij”, link, sekcja spoza śledzonych projektów oraz
   * stany bez projektów i bez konta. Żaden test nie łączy się z Redmine.
   */

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
      JSON.parse(localStorage.getItem("tuktuk-track-preview-redmine-opened-v1")!),
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
      (d) => localStorage.setItem("tuktuk-track-preview-v1", JSON.stringify(d)),
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
    await expect(select).toHaveAttribute("data-value", "7");
    await pick(select, "Portal klienta");
    await expect(numbers(page, "New")).toHaveText(["#1270"]);
    // Wszystkie moje zgłoszenia są w śledzonych projektach.
    await expect(
      outside(page).getByText(
        "Brak otwartych zgłoszeń przypisanych do Ciebie w innych projektach.",
      ),
    ).toBeVisible();
    await page.reload();
    await openRedmine(page);
    await expect(page.getByLabel("Projekt Redmine")).toHaveAttribute(
      "data-value",
      "9",
    );
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

  test("without tracked projects unchecking „Tylko otwarte” shows my closed issues", async ({
    page,
  }) => {
    await openPreview(page);
    await setRedmine(page, { selectedProjects: [] });
    await openRedmine(page);
    const openOnly = view(page).getByLabel("Tylko otwarte");
    await expect(openOnly).toBeChecked();
    await expect(groupNames(page)).toHaveText(["New", "In Progress", "Feedback"]);
    const info = view(page).locator(".jira-result-info");
    await expect(info).toContainText("Przypisane do mnie · 5 zgłoszeń");
    await openOnly.uncheck();
    await expect(groupNames(page)).toHaveText([
      "New",
      "In Progress",
      "Feedback",
      "Closed",
    ]);
    await expect(numbers(page, "Closed")).toHaveText(["#1245"]);
    await expect(info).toContainText("Przypisane do mnie · 6 zgłoszeń");
    await openOnly.check();
    await expect(groupNames(page)).toHaveText(["New", "In Progress", "Feedback"]);
  });

  test("a failing query for issues outside tracked projects keeps the project list", async ({
    page,
  }) => {
    await openPreview(page);
    await setRedmine(page, { failMine: "Brak uprawnień w Redmine do tej operacji." });
    await openRedmine(page);
    await expect(numbers(page, "New")).toHaveText(["#1234"]);
    await expect(groupNames(page)).toHaveText(["New", "In Progress", "Feedback"]);
    await expect(
      view(page).locator(".page-heading ~ p.error, .redmine-view > p.error"),
    ).toHaveCount(0);
    const section = outside(page);
    await expect(section.getByRole("alert")).toContainText(
      "Nie udało się pobrać zgłoszeń spoza śledzonych projektów: Brak uprawnień w Redmine do tej operacji.",
    );
    await expect(
      section.getByText(
        "Brak otwartych zgłoszeń przypisanych do Ciebie w innych projektach.",
      ),
    ).toHaveCount(0);
  });
});

test.describe("R3 — obserwacja przypisań (zastępcze IPC)", () => {
  /**
   * Obserwacja przypisań w Redmine (PLAN_1.9.md, R3) przez zastępcze IPC
   * (`redmineCheck` = `redmine::sync_assigned` z `watch_once`): pierwsze
   * sprawdzenie tylko zapamiętuje, nowe otwarte przypisanie trafia do „Do
   * zaplanowania” bez dnia i bez pomiaru, zgłoszenie z zadaniem w koszu się nie
   * dubluje, zdjęte i przypisane ponownie wraca, status odświeża się na kafelku.
   * Obserwacja Jiry działa niezależnie.
   */

  const column = (page: Page, name: string) =>
    page.getByRole("region", { name, exact: true });
  const unplanned = (page: Page) =>
    column(page, "Do zaplanowania").locator(".week-card-title");
  const redmineStatusOn = (page: Page, title: string) =>
    page
      .getByRole("article", { name: title, exact: true })
      .locator(".task-redmine-status");

  function redmineData() {
    const data = seed();
    const base = data.tasks[0].details;
    (data.tasks as any[]).unshift({
      id: 30,
      title: "Import CSV zamówień",
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1234,
      redmineUrl: `${REDMINE_SITE}/issues/1234`,
      redmineStatus: "New",
      redmineTracker: "Funkcja",
      redmineDue: "",
      details: { ...base, priority: 0, plannedDay: TODAY, dueDay: "", notes: {}, checklist: [], estimateMinutes: 0 },
    });
    (data.trash as any[]).push({
      id: 31,
      title: "Porzucony eksport",
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1299,
      redmineUrl: `${REDMINE_SITE}/issues/1299`,
      redmineStatus: "New",
      details: { ...base, priority: 0, plannedDay: "", dueDay: "", notes: {}, checklist: [], estimateMinutes: 0 },
    });
    return data;
  }

  test("Redmine watch: first check only remembers, then new assignments land in Do zaplanowania without duplicates", async ({
    page,
  }) => {
    await openDesktop(page, { data: redmineData() });
    await page
      .getByRole("button", { name: "Plan i historia", exact: true })
      .click();
    await page.getByRole("button", { name: "Tydzień", exact: true }).click();
    await expect(unplanned(page)).toHaveText(["Code review"]);
    await expect(redmineStatusOn(page, "Import CSV zamówień")).toHaveText("New");

    // Pierwsze sprawdzenie: backlog zapamiętany, status odświeżony.
    const first = await backend<number[]>(page, "redmineCheck", [
      { id: 1234, subject: "Import CSV zamówień", status: "In Progress" },
      { id: 1300, subject: "Raport zwrotów", status: "New" },
    ]);
    expect(first).toEqual([]);
    await expect(unplanned(page)).toHaveText(["Code review"]);
    await expect(redmineStatusOn(page, "Import CSV zamówień")).toHaveText(
      "In Progress",
    );

    // Drugie: nowe #1301 do „Do zaplanowania”; #1300 było już przypisane,
    // #1299 ma zadanie w koszu, #1302 jest zamknięte.
    const second = await backend<number[]>(page, "redmineCheck", [
      { id: 1234, subject: "Import CSV zamówień", status: "In Progress" },
      { id: 1300, subject: "Raport zwrotów", status: "New" },
      {
        id: 1301,
        subject: "Etykiety palet",
        status: "New",
        tracker: "Błąd",
        priority: "High",
      },
      { id: 1299, subject: "Porzucony eksport", status: "New" },
      { id: 1302, subject: "Zamknięte", status: "Closed", closed: true },
    ]);
    expect(second).toEqual([1301]);
    await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
    await expect(
      column(page, "Do zaplanowania")
        .locator(".week-card", { hasText: "Etykiety palet" })
        .locator(".task-redmine-key"),
    ).toHaveText("#1301");

    // Kolejne sprawdzenie z tym samym zestawem niczego nie dubluje.
    expect(
      await backend<number[]>(page, "redmineCheck", [
        { id: 1300, subject: "Raport zwrotów", status: "New" },
        { id: 1301, subject: "Etykiety palet", status: "Feedback", tracker: "Błąd" },
      ]),
    ).toEqual([]);
    await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
    await expect(redmineStatusOn(page, "Etykiety palet")).toHaveText("Feedback");

    // #1300 zdjęte z listy, potem przypisane ponownie: wraca jako nowe zadanie.
    expect(
      await backend<number[]>(page, "redmineCheck", [
        { id: 1301, subject: "Etykiety palet", status: "Feedback" },
      ]),
    ).toEqual([]);
    expect(
      await backend<number[]>(page, "redmineCheck", [
        { id: 1300, subject: "Raport zwrotów", status: "New" },
        { id: 1301, subject: "Etykiety palet", status: "Feedback" },
      ]),
    ).toEqual([1300]);

    const snapshot = await backend<any>(page, "snapshot");
    const labels = snapshot.tasks.find((t: any) => t.redmineId === 1301);
    expect(labels).toMatchObject({
      title: "Etykiety palet",
      redmineUrl: `${REDMINE_SITE}/issues/1301`,
      redmineStatus: "Feedback",
      done: false,
    });
    expect(labels.jiraKey).toBeUndefined();
    expect(labels.details.priority).toBe(1);
    expect(labels.details.plannedDay).toBe("");
    // Bez pomiaru: nowe zadania nie startują timera.
    expect(
      snapshot.sessions.filter((s: any) => s.endedAt === null),
    ).toEqual([]);
    expect(snapshot.tasks.some((t: any) => t.redmineId === 1299)).toBe(false);
    expect(snapshot.trash.map((t: any) => t.id)).toEqual([ids.trashed, 31]);
  });

  test("Redmine auto-import off: nothing is added and Jira watch is independent", async ({
    page,
  }) => {
    await openDesktop(page, {
      data: redmineData(),
      settings: { redmineAutoImport: false },
    });
    await backend(page, "redmineCheck", []);
    expect(
      await backend<number[]>(page, "redmineCheck", [
        { id: 1400, subject: "Nowe", status: "New" },
      ]),
    ).toEqual([]);
    // Jira importuje dalej (własny wyłącznik `jiraAutoImport`).
    await backend(page, "jiraCheck", []);
    expect(
      await backend<string[]>(page, "jiraCheck", [
        { key: "WMS-50", summary: "Nowe z Jiry", status: "Do zrobienia" },
      ]),
    ).toEqual(["WMS-50"]);
    const snapshot = await backend<any>(page, "snapshot");
    expect(snapshot.tasks.some((t: any) => t.redmineId === 1400)).toBe(false);
    expect(snapshot.tasks.some((t: any) => t.jiraKey === "WMS-50")).toBe(true);
  });
});

test.describe("R4 — zapisy do Redmine", () => {
  test.use({ locale: "pl-PL" });

  /**
   * Zapisy do Redmine (PLAN_1.9.md, R4) na symulowanym Redmine podglądu: pytanie
   * o przesunięcie przy starcie pracy (ten sam baner co dla Jiry), „Nie pytaj”,
   * propozycja po ukończeniu, „Przesuń w Redmine…” na kafelku, „Przesuń do…” w
   * widoku Redmine, „Przypisz do mnie”, komentarz, nazwa gałęzi i główny
   * przełącznik `redmineWrite`. Żaden test nie łączy się z Redmine.
   */

  const JIRA_KEY = "tuktuk-track-preview-jira-v1";
  const IMPORT = "Import CSV zamówień";

  const banner = (page: Page) =>
    page.getByRole("region", { name: "Przesunięcie zgłoszenia w Redmine" });
  const jiraBanner = (page: Page) =>
    page.getByRole("region", { name: "Przesunięcie zgłoszenia w Jirze" });
  const doneBanner = (page: Page) =>
    page.getByRole("region", { name: "Przesunięcie zgłoszenia po ukończeniu" });
  const card = (page: Page, title: string) =>
    page.locator(".task-card", {
      has: page.getByRole("button", {
        name: "Przedziały: " + title,
        exact: true,
      }),
    });
  const expand = (page: Page, title: string) =>
    page.getByLabel("Przedziały: " + title, { exact: true }).click();
  const taskStatus = async (page: Page, id = 20) =>
    (await stored(page)).tasks.find((t: { id: number }) => t.id === id)
      .redmineStatus;
  /** Stan symulowanego Redmine (zgłoszenia po zapisie, komentarze). */
  const redmineState = (page: Page) =>
    page.evaluate(
      (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
      REDMINE_KEY,
    );
  const simStatus = async (page: Page, id: number) =>
    ((await redmineState(page)).issues ?? []).find(
      (i: { id: number }) => i.id === id,
    )?.status;
  const seenTask = (page: Page) =>
    page.evaluate(
      (k) => JSON.parse(localStorage.getItem(k) ?? "{}").seen?.[1] ?? null,
      JIRA_KEY,
    );
  async function setRedmine(page: Page, state: Record<string, unknown>) {
    await page.evaluate(([key, value]) => localStorage.setItem(key, value), [
      REDMINE_KEY,
      JSON.stringify(state),
    ] as const);
    await page.reload();
  }
  async function start(page: Page, title: string) {
    await page
      .getByRole("button", { name: "Start: " + title, exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Stop: " + title, exact: true }),
    ).toBeVisible();
  }
  async function stop(page: Page, title: string) {
    await page
      .getByRole("button", { name: "Stop: " + title, exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Start: " + title, exact: true }),
    ).toBeVisible();
  }
  const openRedmine = (page: Page) =>
    page.getByRole("button", { name: "Redmine", exact: true }).click();
  const statusGroup = (page: Page, status: string) =>
    page.getByRole("region", { name: `Status ${status}`, exact: true });
  const issueCard = (page: Page, id: number, subject: string) =>
    page.getByRole("article", { name: `#${id} ${subject}`, exact: true });

  test("cel pytania jak w Rust: dopasowanie po nazwie, lista bez zamkniętych i bieżącego", () => {
    const all = [...previewRedmineStatuses];
    const one = chooseRedmineStatus(all, 1, " in progress ");
    expect(one.statusId).toBe(2);
    expect(one.statuses.map((s) => s.name)).toEqual(["In Progress", "Feedback"]);
    const none = chooseRedmineStatus(all, 1, "W toku");
    expect(none.statusId).toBeNull();
    expect(none.statuses).toHaveLength(2);
    expect(chooseRedmineStatus(all, 1, "Closed").statusId).toBeNull();
    expect(chooseRedmineStatus(all.slice(3), 1, "Closed").statuses).toEqual([]);
    expect(redmineTransitionOf(all[3])).toEqual({
      id: "4",
      name: "Resolved",
      toStatus: "Resolved",
      toCategory: "done",
    });
  });

  test("start → baner → „Przesuń” zmienia status #1234 na kafelku; pomiar trwa od razu", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await expect(banner(page)).toHaveCount(0);
    await start(page, IMPORT);
    await expect(banner(page)).toBeVisible();
    await expect(jiraBanner(page)).toHaveCount(0);
    await expect(banner(page).getByRole("heading")).toHaveText(
      "Przesunąć #1234 do „In Progress”?",
    );
    await expect(banner(page)).toContainText("ma w Redmine status New");
    // Nic nie zmieniło się samo, a pomiar już trwa.
    expect(await taskStatus(page)).toBe("New");
    expect(await simStatus(page, 1234)).toBeUndefined();
    const tile = card(page, IMPORT);
    await expect(tile.locator(".task-issue-status")).toHaveText("New");
    await banner(page)
      .getByRole("button", { name: "Przesuń #1234 do In Progress" })
      .click();
    await expect(banner(page)).toHaveCount(0);
    await expect(tile.locator(".task-issue-status")).toHaveText("In Progress");
    expect(await taskStatus(page)).toBe("In Progress");
    expect(await simStatus(page, 1234)).toBe("In Progress");
    await expect(
      page.getByRole("button", { name: "Stop: " + IMPORT, exact: true }),
    ).toBeVisible();
    // Zgłoszenie w toku: kolejny start nie pyta.
    await stop(page, IMPORT);
    await start(page, IMPORT);
    await expect.poll(() => seenTask(page)).toBe(20);
    await expect(banner(page)).toHaveCount(0);
    // Status spoza „Statusów początkowych” (Feedback) też nie pyta.
    await start(page, "Błąd zapisu etykiet");
    await expect.poll(() => seenTask(page)).toBe(21);
    await expect(banner(page)).toHaveCount(0);
  });

  test("bez dopasowania nazwy baner pokazuje dozwolone statusy niezamknięte", async ({
    page,
  }) => {
    await openPreview(page, {
      data: redmineSeed(),
      settings: { redmineMoveStatus: "W toku", jiraMoveStatus: "In Progress" },
    });
    await start(page, IMPORT);
    await expect(banner(page).getByRole("heading")).toHaveText(
      "Przesunąć #1234 do „W toku”?",
    );
    await expect(banner(page)).toContainText("Wybierz status:");
    await expect(
      banner(page).getByRole("button", { name: /^Przesuń do: / }),
    ).toHaveText(["In Progress", "Feedback"]);
    await banner(page)
      .getByRole("button", { name: "Przesuń do: Feedback" })
      .click();
    await expect(banner(page)).toHaveCount(0);
    await expect(card(page, IMPORT).locator(".task-issue-status")).toHaveText(
      "Feedback",
    );
  });

  test("„Nie pytaj dla tego zadania” wycisza #1234, kafelek odwraca", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await start(page, IMPORT);
    await banner(page).getByRole("button", { name: "Nie teraz" }).click();
    await expect(banner(page)).toHaveCount(0);
    await stop(page, IMPORT);
    await start(page, IMPORT);
    await expect(banner(page)).toBeVisible();
    await banner(page)
      .getByRole("button", { name: "Nie pytaj dla tego zadania" })
      .click();
    await expect(banner(page)).toHaveCount(0);
    const details = (await stored(page)).tasks.find(
      (t: { id: number }) => t.id === 20,
    ).details;
    // To samo pole co dla Jiry (zadanie ma jedno zgłoszenie).
    expect(details.jiraNoAsk).toBe(true);
    expect(details.project).toBe("Sklep");
    expect(await taskStatus(page)).toBe("New");
    await stop(page, IMPORT);
    await start(page, IMPORT);
    await expect.poll(() => seenTask(page)).toBe(20);
    await expect(banner(page)).toHaveCount(0);
    await expand(page, IMPORT);
    await expect(card(page, IMPORT).locator(".jira-no-ask")).toContainText(
      "Bez pytania o przesunięcie #1234 przy starcie pracy.",
    );
    await page
      .getByRole("button", { name: "Pytaj o przesunięcie: " + IMPORT })
      .click();
    await expect(
      page.getByRole("button", { name: "Pytaj o przesunięcie: " + IMPORT }),
    ).toHaveCount(0);
    await stop(page, IMPORT);
    await start(page, IMPORT);
    await expect(banner(page)).toBeVisible();
  });

  test("błąd Redmine zostawia pytanie z komunikatem i linkiem do #1234", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await setRedmine(page, { fail: 422 });
    await start(page, IMPORT);
    await banner(page)
      .getByRole("button", { name: "Przesuń #1234 do In Progress" })
      .click();
    await expect(banner(page).getByRole("alert")).toContainText(
      "Redmine odrzucił dane: Status is invalid",
    );
    await banner(page)
      .getByRole("button", { name: "Otwórz #1234 w Redmine" })
      .click();
    await expect
      .poll(() =>
        page.evaluate(() =>
          JSON.parse(
            localStorage.getItem("tuktuk-track-preview-redmine-opened-v1") ?? "[]",
          ),
        ),
      )
      .toEqual([`${REDMINE_SITE}/issues/1234`]);
    expect(await taskStatus(page)).toBe("New");
    // Redmine przyjął zapis bez zmiany statusu.
    await setRedmine(page, { unchanged: true });
    await expand(page, IMPORT);
    await page.getByRole("button", { name: "Przesuń w Redmine: #1234" }).click();
    await page
      .getByRole("group", { name: "Statusy w Redmine: #1234" })
      .getByRole("button", { name: "Przesuń #1234 do: In Progress" })
      .click();
    const tile = card(page, IMPORT);
    await expect(tile.getByRole("alert")).toContainText(
      "Redmine nie zmienił statusu #1234 (nadal „New”)",
    );
    await expect(
      tile.getByRole("button", { name: "Otwórz #1234 w Redmine" }),
    ).toBeVisible();
    expect(await taskStatus(page)).toBe("New");
  });

  test("„Przesuń w Redmine…” w kafelku i propozycja po ukończeniu", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await expand(page, IMPORT);
    await page.getByRole("button", { name: "Przesuń w Redmine: #1234" }).click();
    const list = page.getByRole("group", { name: "Statusy w Redmine: #1234" });
    // Dozwolone statusy bez bieżącego, także zamknięte.
    await expect(
      list.getByRole("button", { name: /^Przesuń #1234 do: / }),
    ).toHaveText(["In Progress", "Feedback", "Resolved", "Closed"]);
    await list
      .getByRole("button", { name: "Przesuń #1234 do: Feedback" })
      .click();
    await expect(list).toHaveCount(0);
    expect(await taskStatus(page)).toBe("Feedback");
    await expect(card(page, IMPORT).locator(".task-issue-status")).toHaveText(
      "Feedback",
    );
    // Ukończenie: „Ukończone. Przesunąć #1234 dalej?” z listą statusów.
    await page
      .getByRole("button", { name: "Ukończ: " + IMPORT, exact: true })
      .click();
    await expect(doneBanner(page).getByRole("heading")).toHaveText(
      "Ukończone. Przesunąć #1234 dalej?",
    );
    await expect(
      doneBanner(page).getByRole("button", { name: /^Przesuń do: / }),
    ).toHaveText(["New", "In Progress", "Resolved", "Closed"]);
    expect(await taskStatus(page)).toBe("Feedback");
    await doneBanner(page)
      .getByRole("button", { name: "Przesuń do: Resolved" })
      .click();
    await expect(doneBanner(page)).toHaveCount(0);
    expect(await taskStatus(page)).toBe("Resolved");
    expect(await simStatus(page, 1234)).toBe("Resolved");
  });

  test("„Przesuń do…” w widoku Redmine przenosi kartę do grupy, błąd ją cofa", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await openRedmine(page);
    const issue = issueCard(statusGroup(page, "New"), 1234, IMPORT);
    await expect(issue).toBeVisible();
    await issue.getByRole("button", { name: "Akcje: #1234" }).click();
    await issue.getByRole("button", { name: "Przesuń do: #1234" }).click();
    await expect(
      issue.getByRole("button", { name: /^Przesuń #1234 do: / }),
    ).toHaveText(["In Progress", "Feedback", "Resolved", "Closed"]);
    await issue
      .getByRole("button", { name: "Przesuń #1234 do: In Progress" })
      .click();
    const moved = issueCard(statusGroup(page, "In Progress"), 1234, IMPORT);
    await expect(moved).toBeVisible();
    await expect(moved.locator(".redmine-status")).toHaveText("In Progress");
    await expect(issueCard(statusGroup(page, "New"), 1234, IMPORT)).toHaveCount(
      0,
    );
    expect(await simStatus(page, 1234)).toBe("In Progress");
    // Kafelek powiązanego zadania dostaje nowy status.
    await expect.poll(() => taskStatus(page)).toBe("In Progress");
    // Odrzucenie przez Redmine: karta wraca do swojej grupy z komunikatem.
    await setRedmine(page, { ...(await redmineState(page)), fail: 403 });
    await openRedmine(page);
    const back = issueCard(statusGroup(page, "In Progress"), 1234, IMPORT);
    await back.getByRole("button", { name: "Akcje: #1234" }).click();
    await back.getByRole("button", { name: "Przesuń do: #1234" }).click();
    await back
      .getByRole("button", { name: "Przesuń #1234 do: Feedback" })
      .click();
    await expect(page.locator(".redmine-view").getByRole("alert")).toContainText(
      "Nie udało się przesunąć #1234: Brak uprawnień w Redmine do tej operacji.",
    );
    await expect(
      issueCard(statusGroup(page, "In Progress"), 1234, IMPORT),
    ).toBeVisible();
    await expect(
      statusGroup(page, "Feedback").locator(".redmine-number"),
    ).not.toContainText(["#1234"]);
    expect(await simStatus(page, 1234)).toBe("In Progress");
  });

  test("„Przypisz do mnie” w widoku i na kafelku, komentarz i nazwa gałęzi", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await openRedmine(page);
    await page
      .getByRole("group", { name: "Zakres zgłoszeń" })
      .getByRole("button", { name: "Wszystkie otwarte" })
      .click();
    const other = issueCard(page, 1251, "Raport stanów magazynowych");
    await expect(other.getByLabel("Przypisane: Anna Nowak")).toBeVisible();
    await other.getByRole("button", { name: "Akcje: #1251" }).click();
    await other.getByRole("button", { name: "Przypisz do mnie: #1251" }).click();
    await expect(other.getByRole("status")).toHaveText(
      "Przypisano #1251 do Ciebie.",
    );
    await expect(other.getByLabel("Przypisane: Anna Nowak")).toHaveCount(0);
    const saved = ((await redmineState(page)).issues ?? []).find(
      (i: { id: number }) => i.id === 1251,
    );
    expect(saved.assignedToId).toBeUndefined();
    // Moje zgłoszenie: bez „Przypisz do mnie”.
    const mine = issueCard(page, 1234, IMPORT);
    await mine.getByRole("button", { name: "Akcje: #1234" }).click();
    await expect(
      mine.getByRole("button", { name: "Przypisz do mnie: #1234" }),
    ).toHaveCount(0);
    // Kafelek zadania: komentarz i nazwa gałęzi.
    await page.getByRole("button", { name: "Zadania", exact: true }).click();
    await expand(page, IMPORT);
    const actions = page.getByRole("group", { name: "Akcje Redmine: #1234" });
    await expect(actions).toBeVisible();
    await actions.getByRole("button", { name: "Dodaj komentarz: #1234" }).click();
    const tile = card(page, IMPORT);
    await tile
      .getByLabel("Komentarz do #1234")
      .fill("Zaczynam.\nPoprawka po przeglądzie — ąę");
    await tile.getByRole("button", { name: "Wyślij komentarz" }).click();
    await expect(
      tile.locator(".redmine-issue-actions").getByRole("status"),
    ).toHaveText("Dodano komentarz do #1234.");
    expect((await redmineState(page)).comments).toEqual({
      "1234": ["Zaczynam.\nPoprawka po przeglądzie — ąę"],
    });
    await actions
      .getByRole("button", { name: "Kopiuj nazwę gałęzi: #1234" })
      .click();
    await expect(
      tile.locator(".redmine-issue-actions").getByRole("status"),
    ).toContainText("feature/1234-import-csv-zamowien");
    // „Przypisz do mnie” na kafelku.
    await actions
      .getByRole("button", { name: "Przypisz do mnie: #1234" })
      .click();
    await expect(
      tile.locator(".redmine-issue-actions").getByRole("status"),
    ).toHaveText("Przypisano #1234 do Ciebie.");
  });

  test("wyłączone zapisy do Redmine: bez pytania, bez „Przesuń…”, przypisania i komentarza", async ({
    page,
  }) => {
    await openPreview(page, {
      data: redmineSeed(),
      settings: { redmineWrite: false },
    });
    await start(page, IMPORT);
    await expect.poll(() => seenTask(page)).toBe(20);
    await expect(banner(page)).toHaveCount(0);
    await expand(page, IMPORT);
    const tile = card(page, IMPORT);
    await expect(
      tile.getByRole("button", { name: "Przesuń w Redmine: #1234" }),
    ).toHaveCount(0);
    await expect(
      tile.getByRole("button", { name: "Przypisz do mnie: #1234" }),
    ).toHaveCount(0);
    await expect(
      tile.getByRole("button", { name: "Dodaj komentarz: #1234" }),
    ).toHaveCount(0);
    await expect(tile).toContainText(
      "Zmiany w Redmine są wyłączone w Ustawieniach → Redmine.",
    );
    // Nazwa gałęzi nie jest zapisem.
    await tile
      .getByRole("button", { name: "Kopiuj nazwę gałęzi: #1234" })
      .click();
    await expect(
      tile.locator(".redmine-issue-actions").getByRole("status"),
    ).toContainText("feature/1234-import-csv-zamowien");
    // Ukończenie nie proponuje przesunięcia.
    await page
      .getByRole("button", { name: "Ukończ: " + IMPORT, exact: true })
      .click();
    await expect
      .poll(
        async () =>
          (await stored(page)).tasks.find((t: { id: number }) => t.id === 20)
            .done,
      )
      .toBe(true);
    await expect(doneBanner(page)).toHaveCount(0);
    // Widok Redmine: menu karty bez „Przesuń do…” i „Przypisz do mnie”.
    await openRedmine(page);
    await page
      .getByRole("group", { name: "Zakres zgłoszeń" })
      .getByRole("button", { name: "Wszystkie otwarte" })
      .click();
    const other = issueCard(page, 1251, "Raport stanów magazynowych");
    await other.getByRole("button", { name: "Akcje: #1251" }).click();
    await expect(
      other.getByRole("button", { name: "Przesuń do: #1251" }),
    ).toHaveCount(0);
    await expect(
      other.getByRole("button", { name: "Przypisz do mnie: #1251" }),
    ).toHaveCount(0);
    await expect(
      other.getByRole("button", { name: "Kopiuj nazwę gałęzi: #1251" }),
    ).toBeVisible();
    expect((await redmineState(page)).issues).toBeUndefined();
    // Jira bez zmian: przełącznik Redmine jej nie dotyczy.
    expect(
      (
        await page.evaluate(() =>
          JSON.parse(localStorage.getItem("tuktuk-track-preview-settings-v1")!),
        )
      ).jiraWrite,
    ).toBeUndefined();
  });

  test("widget pokazuje pytanie Redmine i przesuwa jednym kliknięciem", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    await start(page, IMPORT);
    await expect(banner(page)).toBeVisible();
    await page.goto("/#widget");
    await page.reload();
    const widget = page.locator("#widget");
    const button = widget.getByRole("button", {
      name: "Przesuń #1234 do In Progress",
    });
    await expect(button).toHaveAttribute(
      "title",
      "Redmine: przesunąć #1234 do „In Progress”?",
    );
    await button.click();
    await expect(widget.getByRole("status")).toContainText(
      "Redmine: #1234 → In Progress",
    );
    expect(await taskStatus(page)).toBe("In Progress");
  });
});

test.describe("R5 — Ustawienia → Redmine", () => {
  test.use({ locale: "pl-PL" });

  /**
   * Ustawienia → Redmine (PLAN_1.9.md, R5) na symulowanym Redmine podglądu:
   * połączenie kluczem API (błąd złego klucza, certyfikat, `http`), test
   * połączenia, rozłączenie, śledzone projekty, przełączniki, listy statusów,
   * „Wstaw statusy z Redmine” i wyszukiwarka ustawień. Żaden test nie łączy się
   * z Redmine; adresy są przykładowe.
   */

  const OPENED_KEY = "tuktuk-track-preview-redmine-opened-v1";

  const panel = (page: Page) => page.getByRole("tabpanel");
  const siteInput = (page: Page) => page.getByLabel("Adres Redmine", { exact: true });
  const keyInput = (page: Page) => page.getByLabel("Klucz API", { exact: true });
  const connectButton = (page: Page) =>
    panel(page).getByRole("button", { name: "Połącz", exact: true });
  const account = (page: Page) => panel(page).locator(".redmine-account");
  const tracked = (page: Page) =>
    panel(page).getByRole("list", { name: "Śledzone projekty" }).getByRole("listitem");

  async function setRedmine(page: Page, state: Record<string, unknown>) {
    await page.evaluate(
      ([key, value]) => localStorage.setItem(key, value),
      [REDMINE_KEY, JSON.stringify(state)] as const,
    );
    await page.reload();
  }
  const redmineState = (page: Page) =>
    page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), REDMINE_KEY);
  const storedSettings = (page: Page) =>
    page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), SETTINGS_KEY);

  async function openRedmineSettings(page: Page) {
    await openSettings(page, "Redmine");
    await expect(
      panel(page).getByRole("heading", { name: "Redmine", exact: true }),
    ).toBeVisible();
  }

  test("sekcja Redmine: miejsce na liście, wyszukiwarka i komunikaty (moduł)", () => {
    const ids = matchingSections("").map((s) => s.id);
    expect(ids.indexOf("redmine")).toBe(ids.indexOf("jira") + 1);
    expect(ids.indexOf("notifications")).toBe(ids.indexOf("redmine") + 1);
    for (const q of ["Redmine", "klucz API", "sledzone projekty", "certyfikat"])
      expect(matchingSections(q).map((s) => s.id)).toEqual(["redmine"]);
    // Wspólne słowa z Jirą znajdują obie sekcje.
    expect(matchingSections("przypisane do mnie").map((s) => s.id)).toEqual([
      "jira",
      "redmine",
    ]);
    expect(sectionInMessage(redmineWriteDisabled)).toBe("redmine");
    expect(
      sectionInMessage("Połącz konto w Ustawieniach → Redmine albo poprzedź # znakiem \\."),
    ).toBe("redmine");
    // Normalizacja adresu jak `redmine::normalize_site`.
    expect(redmineNormalizeSite(" https://redmine.example.pl/ ")).toBe(REDMINE_SITE);
    expect(redmineNormalizeSite("https://redmine.example.pl/redmine/")).toBe(
      "https://redmine.example.pl/redmine",
    );
    expect(redmineNormalizeSite("http://127.0.0.1:3000")).toBe("http://127.0.0.1:3000");
    for (const bad of [
      "",
      "redmine.example.pl",
      "ftp://redmine.example.pl",
      "https://redmine.example.pl/?key=1",
      "https://redmine.example.pl/#x",
      "https://user:pass@redmine.example.pl",
    ])
      expect(() => redmineNormalizeSite(bad), bad).toThrow();
  });

  test("wyszukiwarka ustawień znajduje „Redmine” i „klucz API”", async ({ page }) => {
    await openPreview(page);
    await openSettings(page, "Czas pracy");
    const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
    const tabs = page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab");
    for (const q of ["Redmine", "klucz API", "Akceptuj niezaufany certyfikat"]) {
      await search.fill(q);
      await expect(tabs).toHaveText(["Redmine"]);
      await expect(
        panel(page).getByRole("heading", { name: "Redmine", exact: true }),
      ).toBeVisible();
    }
    await expect(keyInput(page)).toBeVisible();
    await search.fill("Czekam na");
    await expect(tabs).toHaveText(["Jira", "Redmine", "Powiadomienia"]);
  });

  test("połączone konto: stan, pusty klucz, test połączenia i jego błędy", async ({
    page,
  }) => {
    await openPreview(page);
    await openRedmineSettings(page);
    await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
    await expect(siteInput(page)).toHaveValue(REDMINE_SITE);
    // Klucz API nigdy nie wraca: pole puste, z podpisem o magazynie poświadczeń.
    await expect(keyInput(page)).toHaveValue("");
    await expect(keyInput(page)).toHaveAttribute("type", "password");
    await expect(keyInput(page)).toHaveAttribute("placeholder", "zapisany");
    await expect(panel(page)).toContainText(
      "Klucz API jest zapisany w magazynie poświadczeń Windows.",
    );
    await expect(connectButton(page)).toBeDisabled();

    await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
    await expect(panel(page).getByRole("status")).toHaveText(
      "Połączono jako Jan Kowalski (jkowalski), Redmine odpowiada.",
    );

    await setRedmine(page, { failTest: "Redmine zwrócił błąd HTTP 502. Spróbuj ponownie." });
    await openRedmineSettings(page);
    await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
    await expect(panel(page).getByRole("alert")).toHaveText(
      "Redmine zwrócił błąd HTTP 502. Spróbuj ponownie.",
    );

    // Osobny komunikat dla certyfikatu spoza magazynu Windows.
    await setRedmine(page, { certError: true });
    await openRedmineSettings(page);
    await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
    await expect(panel(page).getByRole("alert")).toHaveText(redmineCertError);
  });

  test("Połącz: zły klucz daje błąd, poprawny łączy z adresem z prefiksem ścieżki", async ({
    page,
  }) => {
    await openPreview(page);
    await setRedmine(page, { connected: false, site: "" });
    await openRedmineSettings(page);
    await expect(account(page)).toHaveText("Nie połączono");
    await expect(
      panel(page).getByRole("button", { name: "Testuj połączenie" }),
    ).toHaveCount(0);
    await expect(siteInput(page)).toHaveValue("");
    await expect(siteInput(page)).toHaveAttribute(
      "placeholder",
      "https://redmine.firma.pl",
    );
    await expect(
      panel(page).getByText("Lista projektów pojawi się po połączeniu konta."),
    ).toBeVisible();
    await expect(panel(page).getByRole("button", { name: "Dodaj projekt…" })).toBeDisabled();
    // Bez połączenia nie ma „Wstaw statusy z Redmine”.
    await expect(
      panel(page).getByRole("button", { name: "Wstaw statusy z Redmine" }),
    ).toHaveCount(0);

    await siteInput(page).fill("https://redmine.example.pl/redmine/");
    await expect(connectButton(page)).toBeDisabled();
    await keyInput(page).fill("zly");
    await connectButton(page).click();
    await expect(panel(page).getByRole("alert")).toHaveText(redmineKeyRejected);
    await expect(keyInput(page)).toHaveValue("");
    await expect(account(page)).toHaveText("Nie połączono");

    await keyInput(page).fill("klucz z spacja");
    await connectButton(page).click();
    await expect(panel(page).getByRole("alert")).toHaveText(
      "Podaj klucz API Redmine (1–128 znaków, bez spacji).",
    );

    await keyInput(page).fill("0123456789abcdef");
    await keyInput(page).press("Enter");
    await expect(panel(page).getByRole("status")).toHaveText(
      "Połączono jako Jan Kowalski (jkowalski).",
    );
    await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
    await expect(keyInput(page)).toHaveValue("");
    await expect(siteInput(page)).toHaveValue("https://redmine.example.pl/redmine");
    const state = await redmineState(page);
    expect(state.connected).toBe(true);
    expect(state.site).toBe("https://redmine.example.pl/redmine");
    // Klucz API nie trafia do localStorage.
    expect(
      await page.evaluate(() => JSON.stringify({ ...localStorage })),
    ).not.toContain("0123456789abcdef");
    // Nowy adres: śledzone projekty wybiera się od nowa.
    await expect(tracked(page)).toHaveCount(0);
  });

  test("ostrzeżenie przy http i link „Gdzie znaleźć klucz API”", async ({ page }) => {
    await openPreview(page);
    await setRedmine(page, { connected: false, site: "" });
    await openRedmineSettings(page);
    const warning = panel(page).getByText(
      "klucz API jest wysyłany bez szyfrowania",
    );
    const where = panel(page).getByRole("button", { name: "Gdzie znaleźć klucz API" });
    await expect(where).toBeDisabled();
    await expect(warning).toHaveCount(0);

    await siteInput(page).fill("http://redmine.example.pl/redmine");
    await expect(warning).toBeVisible();
    await expect(where).toBeEnabled();
    await where.click();
    expect(
      await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), OPENED_KEY),
    ).toEqual(["http://redmine.example.pl/redmine/my/account"]);

    await siteInput(page).fill("https://redmine.example.pl");
    await expect(warning).toHaveCount(0);
    await siteInput(page).fill("redmine.example.pl");
    await expect(where).toBeDisabled();
  });

  test("opcja niezaufanego certyfikatu pozwala połączyć się i jest zapamiętana", async ({
    page,
  }) => {
    await openPreview(page);
    await setRedmine(page, { connected: false, site: "", certError: true });
    await openRedmineSettings(page);
    const option = page.getByLabel(
      "Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)",
    );
    // Zwinięte „Zaawansowane”, opcja domyślnie wyłączona.
    await expect(option).toBeHidden();
    await siteInput(page).fill("https://redmine.example.pl");
    await keyInput(page).fill("0123456789abcdef");
    await connectButton(page).click();
    await expect(panel(page).getByRole("alert")).toHaveText(redmineCertError);
    await expect(account(page)).toHaveText("Nie połączono");

    await panel(page).getByText("Zaawansowane", { exact: true }).click();
    await expect(option).not.toBeChecked();
    await expect(panel(page)).toContainText("ufa certyfikatom z magazynu Windows");
    await option.check();
    await expect(
      panel(page).getByText("Połączenie nie sprawdza tożsamości serwera."),
    ).toBeVisible();
    await keyInput(page).fill("0123456789abcdef");
    await connectButton(page).click();
    await expect(account(page)).toHaveText("Połączono jako Jan Kowalski (jkowalski)");
    expect((await redmineState(page)).acceptInvalidCert).toBe(true);

    // Po przeładowaniu opcja zostaje włączona (z konfiguracji), sekcja rozwinięta.
    await page.reload();
    await openRedmineSettings(page);
    await expect(option).toBeVisible();
    await expect(option).toBeChecked();
    await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
    await expect(panel(page).getByRole("status")).toContainText("Redmine odpowiada.");
  });

  test("Rozłącz pyta o potwierdzenie i zostawia adres", async ({ page }) => {
    await openPreview(page);
    await openRedmineSettings(page);
    await panel(page).getByRole("button", { name: "Rozłącz", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Rozłączyć konto Redmine?" });
    await expect(dialog).toContainText(
      "Klucz API zostanie usunięty z magazynu poświadczeń Windows.",
    );
    await dialog.getByRole("button", { name: "Rozłącz", exact: true }).click();
    await expect(dialog).toHaveCount(0);
    await expect(account(page)).toHaveText("Nie połączono");
    await expect(siteInput(page)).toHaveValue(REDMINE_SITE);
    await expect(
      panel(page).getByRole("button", { name: "Testuj połączenie" }),
    ).toHaveCount(0);
    expect((await redmineState(page)).connected).toBe(false);
    // Widok Redmine prowadzi z powrotem do Ustawień → Redmine.
    await page.getByRole("button", { name: "Redmine", exact: true }).click();
    await page
      .getByRole("button", { name: "Połącz konto Redmine w Ustawieniach" })
      .click();
    await expect(page.getByRole("tab", { name: "Redmine", exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(account(page)).toHaveText("Nie połączono");
  });

  test("śledzone projekty: dodanie z listy z podprojektami i usunięcie", async ({
    page,
  }) => {
    await openPreview(page);
    await openRedmineSettings(page);
    await expect(panel(page)).toContainText(
      "Widok Redmine pokazuje zgłoszenia tych projektów; nowe przypisania są sprawdzane we wszystkich projektach.",
    );
    await expect(tracked(page)).toHaveCount(1);
    await expect(tracked(page).first()).toContainText("Magazyn WMS");

    await panel(page).getByRole("button", { name: "Dodaj projekt…" }).click();
    const picker = page.getByRole("region", { name: "Wybór projektów Redmine" });
    const options = picker.locator(".redmine-project-options label");
    // Podprojekt z wcięciem pod rodzicem.
    await expect(options).toHaveText([
      /Magazyn WMS/,
      /WMS – integracje/,
      /Portal klienta/,
    ]);
    const indent = (i: number) =>
      options.nth(i).evaluate((el) => parseFloat(getComputedStyle(el).paddingLeft));
    expect(await indent(1)).toBeGreaterThan(await indent(0));
    expect(await indent(2)).toBe(await indent(0));
    await expect(picker.getByLabel("Magazyn WMS")).toBeChecked();
    await expect(picker.getByText("Wybrano 1 / 50")).toBeVisible();

    await picker.getByLabel("Szukaj projektu").fill("portal");
    await expect(options).toHaveText([/Portal klienta/]);
    await picker.getByLabel("Portal klienta").check();
    await picker.getByLabel("Szukaj projektu").fill("integr");
    await picker.getByLabel("WMS – integracje").check();
    await expect(picker.getByText("Wybrano 3 / 50")).toBeVisible();
    await picker.getByRole("button", { name: "Zapisz projekty" }).click();
    await expect(picker).toHaveCount(0);
    await expect(tracked(page)).toHaveCount(3);
    expect(
      (await redmineState(page)).selectedProjects.map((p: { id: number }) => p.id),
    ).toEqual([7, 9, 8]);

    await panel(page).getByRole("button", { name: "Usuń projekt Magazyn WMS" }).click();
    await expect(tracked(page)).toHaveCount(2);
    await panel(page).getByRole("button", { name: "Usuń projekt WMS – integracje" }).click();
    await expect(tracked(page)).toHaveText([/Portal klienta/]);
    expect(
      (await redmineState(page)).selectedProjects.map((p: { id: number }) => p.id),
    ).toEqual([9]);

    // Widok Redmine pokazuje teraz tylko Portal klienta.
    await page.getByRole("button", { name: "Redmine", exact: true }).click();
    await expect(
      page.getByRole("group", { name: "Projekt Redmine" }).getByRole("button"),
    ).toHaveText(["Portal klienta"]);
    // „Ustawienia → Redmine” w nagłówku widoku otwiera tę sekcję.
    await page.getByRole("button", { name: "Ustawienia → Redmine" }).click();
    await expect(tracked(page)).toHaveText([/Portal klienta/]);
  });

  test("przełączniki, statusy i prefiks gałęzi zapisują się z walidacją", async ({
    page,
  }) => {
    await openPreview(page);
    await openRedmineSettings(page);
    const write = page.getByLabel("Pozwól aplikacji zmieniać zgłoszenia w Redmine");
    const ask = page.getByLabel("Pytaj o przesunięcie zgłoszenia przy starcie pracy");
    const move = page.getByLabel("Docelowy status przy starcie pracy");
    const initial = page.getByLabel("Statusy początkowe (jeden w wierszu)");
    const waiting = page.getByLabel("Statusy oznaczające „Czekam na” (jeden w wierszu)");
    const autoImport = page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie");
    const prefix = page.getByLabel("Prefiks nazwy gałęzi");

    // Wartości domyślne.
    await expect(write).toBeChecked();
    await expect(ask).toBeChecked();
    await expect(move).toHaveValue("In Progress");
    await expect(initial).toHaveValue("New\nNowy");
    await expect(waiting).toHaveValue("Feedback");
    await expect(autoImport).toBeChecked();
    await expect(prefix).toHaveValue("feature/");

    await write.uncheck();
    await expect(ask).toBeDisabled();
    await expect(move).toBeDisabled();
    expect((await storedSettings(page)).redmineWrite).toBe(false);
    await write.check();
    await expect(ask).toBeEnabled();
    await ask.uncheck();
    await expect(ask).not.toBeChecked();
    await autoImport.uncheck();
    await expect(autoImport).not.toBeChecked();

    await move.fill("  W toku ");
    await move.blur();
    await expect(move).toHaveValue("W toku");
    await initial.fill("Nowy\n\nnowy\nOtwarte");
    await initial.blur();
    await expect(initial).toHaveValue("Nowy\nOtwarte");
    await waiting.fill("Feedback\nOczekuje na klienta");
    await waiting.blur();
    await prefix.fill("bugfix/");
    await prefix.blur();
    await expect(panel(page)).toContainText("bugfix/1234-krotki-tytul");

    const saved = await storedSettings(page);
    expect(saved).toMatchObject({
      redmineWrite: true,
      redmineAskMove: false,
      redmineAutoImport: false,
      redmineMoveStatus: "W toku",
      redmineNewStatuses: ["Nowy", "Otwarte"],
      waitingRedmineStatuses: ["Feedback", "Oczekuje na klienta"],
      redmineBranchPrefix: "bugfix/",
    });
    // Ustawienia Jiry bez zmian.
    expect(saved.jiraMoveStatus ?? "In Progress").toBe("In Progress");
    expect(saved.waitingJiraStatuses ?? ["Waiting", "Blocked"]).not.toContain(
      "Oczekuje na klienta",
    );

    // Pusty status docelowy jest odrzucany, pole wraca do zapisanej wartości.
    await move.fill(" ");
    await move.blur();
    await expect(page.getByRole("alert")).toHaveText(
      "Nazwa statusu w Redmine musi mieć od 1 do 80 znaków.",
    );
    await expect(move).toHaveValue("W toku");
    await prefix.fill("../x");
    await prefix.blur();
    await expect(prefix).toHaveValue("bugfix/");

    // Po przeładowaniu wartości zostają.
    await page.reload();
    await openRedmineSettings(page);
    await expect(ask).not.toBeChecked();
    await expect(autoImport).not.toBeChecked();
    await expect(move).toHaveValue("W toku");
    await expect(waiting).toHaveValue("Feedback\nOczekuje na klienta");
  });

  test("„Wstaw statusy z Redmine” dopisuje kliknięte statusy do wybranego pola", async ({
    page,
  }) => {
    await openPreview(page);
    await openRedmineSettings(page);
    await panel(page).getByRole("button", { name: "Wstaw statusy z Redmine" }).click();
    const picker = page.getByRole("group", { name: "Statusy z Redmine" });
    await expect(picker.locator(".redmine-status-chips button")).toHaveText([
      "New",
      "In Progress",
      "Feedback",
      "Resolved (zamknięty)",
      "Closed (zamknięty)",
    ]);
    const initial = page.getByLabel("Statusy początkowe (jeden w wierszu)");
    const waiting = page.getByLabel("Statusy oznaczające „Czekam na” (jeden w wierszu)");
    const move = page.getByLabel("Docelowy status przy starcie pracy");

    // Domyślnie do statusów początkowych; powtórka nie dubluje.
    await picker.getByRole("button", { name: "Wstaw status Feedback" }).click();
    await expect(initial).toHaveValue("New\nNowy\nFeedback");
    await expect(panel(page).getByRole("status")).toHaveText(
      "Dodano „Feedback” do statusów początkowych.",
    );
    await picker.getByRole("button", { name: "Wstaw status New" }).click();
    await expect(panel(page).getByRole("status")).toHaveText(
      "„New” już jest na liście statusów początkowych.",
    );
    await expect(initial).toHaveValue("New\nNowy\nFeedback");

    await picker.getByLabel("Wstaw do").selectOption({ label: "statusów „Czekam na”" });
    await picker.getByRole("button", { name: "Wstaw status Resolved" }).click();
    await expect(waiting).toHaveValue("Feedback\nResolved");

    await picker
      .getByLabel("Wstaw do")
      .selectOption({ label: "statusu docelowego" });
    await picker.getByRole("button", { name: "Wstaw status In Progress" }).click();
    await expect(move).toHaveValue("In Progress");
    await move.fill("W toku");
    await move.blur();
    await picker.getByRole("button", { name: "Wstaw status In Progress" }).click();
    await expect(move).toHaveValue("In Progress");

    expect(await storedSettings(page)).toMatchObject({
      redmineNewStatuses: ["New", "Nowy", "Feedback"],
      waitingRedmineStatuses: ["Feedback", "Resolved"],
      redmineMoveStatus: "In Progress",
    });
    await panel(page).getByRole("button", { name: "Zwiń statusy z Redmine" }).click();
    await expect(picker).toHaveCount(0);
  });
});

test.describe("R6 — pełna ścieżka i zastępcze IPC", () => {
  test.use({ locale: "pl-PL" });

  const IMPORT = "Import CSV zamówień";
  const panel = (page: Page) => page.getByRole("tabpanel");
  const banner = (page: Page) =>
    page.getByRole("region", { name: "Przesunięcie zgłoszenia w Redmine" });
  const nav = (page: Page, name: string) =>
    page.getByRole("button", { name, exact: true }).click();
  const tile = (page: Page, title: string) =>
    page.locator(".task-card", {
      has: page.getByRole("button", {
        name: "Przedziały: " + title,
        exact: true,
      }),
    });

  test("Ustawienia → projekty → widok → „Rozpocznij” → baner przesunięcia → kafelek", async ({
    page,
  }) => {
    await openPreview(page, { redmine: { connected: false, site: "" } });
    // Bez konta widok odsyła do Ustawień.
    await nav(page, "Redmine");
    await expect(
      page.getByText("Połącz konto Redmine w Ustawieniach"),
    ).toBeVisible();

    // Ustawienia → Redmine: połączenie kluczem API (symulowane).
    await openSettings(page, "Redmine");
    await page.getByLabel("Adres Redmine", { exact: true }).fill(REDMINE_SITE);
    await page.getByLabel("Klucz API", { exact: true }).fill("0123456789abcdef");
    await panel(page).getByRole("button", { name: "Połącz", exact: true }).click();
    await expect(panel(page).locator(".redmine-account")).toHaveText(
      "Połączono jako Jan Kowalski (jkowalski)",
    );

    // Śledzony projekt: Magazyn WMS (z podprojektem).
    const tracked = panel(page)
      .getByRole("list", { name: "Śledzone projekty" })
      .getByRole("listitem");
    await expect(tracked).toHaveCount(0);
    await panel(page).getByRole("button", { name: "Dodaj projekt…" }).click();
    const picker = page.getByRole("region", { name: "Wybór projektów Redmine" });
    await picker.getByLabel("Magazyn WMS").check();
    await picker.getByRole("button", { name: "Zapisz projekty" }).click();
    await expect(tracked).toHaveText([/Magazyn WMS/]);

    // Widok Redmine: moje otwarte zgłoszenia projektu w grupach statusów.
    await nav(page, "Redmine");
    await expect(page.getByText("Połączono jako Jan Kowalski")).toBeVisible();
    await expect(
      page.locator(".redmine-view .redmine-group-name"),
    ).toHaveText(["New", "In Progress", "Feedback"]);
    await expect(
      page
        .getByRole("region", {
          name: "Przypisane do mnie spoza śledzonych projektów",
        })
        .locator(".redmine-number"),
    ).toHaveText(["#1270"]);

    // „Rozpocznij”: zadanie, pomiar i pytanie o przesunięcie (status „New”).
    await page.getByRole("button", { name: "Rozpocznij: #1234" }).click();
    await expect(
      page
        .getByRole("article", { name: "#1234 " + IMPORT })
        .getByText("mierzone teraz"),
    ).toBeVisible();
    await expect(banner(page).getByRole("heading")).toHaveText(
      "Przesunąć #1234 do „In Progress”?",
    );
    // Nic nie zmienia się samo.
    let task = (await stored(page)).tasks.find((t: any) => t.redmineId === 1234);
    expect(task.redmineStatus).toBe("New");
    await banner(page)
      .getByRole("button", { name: "Przesuń #1234 do In Progress" })
      .click();
    await expect(banner(page)).toHaveCount(0);

    // Kafelek zadania: numer, nowy status i trwający pomiar.
    await nav(page, "Zadania");
    const card = tile(page, IMPORT);
    await expect(card.locator(".task-issue-key")).toHaveText("#1234");
    await expect(card.locator(".task-issue-status")).toHaveText("In Progress");
    await expect(
      page.getByRole("button", { name: "Stop: " + IMPORT, exact: true }),
    ).toBeVisible();
    const data = await stored(page);
    task = data.tasks.find((t: any) => t.redmineId === 1234);
    expect(task).toMatchObject({
      title: IMPORT,
      redmineStatus: "In Progress",
      redmineUrl: `${REDMINE_SITE}/issues/1234`,
    });
    expect(task.jiraKey).toBeUndefined();
    expect(
      data.sessions.filter((s: any) => s.endedAt === null).map((s: any) => s.taskId),
    ).toEqual([task.id]);
  });

  test("seed: start #1234 („New”) pyta o przesunięcie, #1240 („Feedback”) proponuje „Czekam na”", async ({
    page,
  }) => {
    await openPreview(page, { data: redmineSeed() });
    // #1240 w „Feedback” (domyślna lista „Czekam na” dla Redmine).
    await expect(
      page.getByRole("group", {
        name: "Propozycja z Redmine: Błąd zapisu etykiet",
      }),
    ).toContainText("Redmine: Feedback — oznaczyć jako czekam na?");
    await expect(
      page.getByRole("group", { name: "Propozycja z Redmine: " + IMPORT }),
    ).toHaveCount(0);
    // Start #1240 nie pyta (status spoza „Statusów początkowych”), #1234 pyta.
    await page
      .getByRole("button", { name: "Start: Błąd zapisu etykiet", exact: true })
      .click();
    await expect(
      page.getByRole("button", { name: "Stop: Błąd zapisu etykiet", exact: true }),
    ).toBeVisible();
    await expect(banner(page)).toHaveCount(0);
    await page
      .getByRole("button", { name: "Start: " + IMPORT, exact: true })
      .click();
    await expect(banner(page).getByRole("heading")).toHaveText(
      "Przesunąć #1234 do „In Progress”?",
    );
  });

  test("zastępcze IPC z redmineFixture(): widok, „Rozpocznij” i szybkie dodawanie #1270", async ({
    page,
  }) => {
    await openDesktop(page, { data: redmineSeed(), redmine: redmineFixture() });
    await nav(page, "Redmine");
    await expect(page.getByText("Połączono jako Jan Kowalski")).toBeVisible();
    await expect(
      page.locator(".redmine-view .redmine-group-name"),
    ).toHaveText(["New", "In Progress", "Feedback"]);
    await expect(
      page.getByRole("region", { name: "Status New", exact: true }).locator(".redmine-number"),
    ).toHaveText(["#1234"]);
    await page.getByRole("button", { name: "Rozpocznij: #1250" }).click();
    await expect(
      page
        .getByRole("article", { name: "#1250 Aktualizacja dokumentacji API" })
        .getByText("mierzone teraz"),
    ).toBeVisible();
    let snapshot = await backend<any>(page, "snapshot");
    const started = snapshot.tasks.find((t: any) => t.redmineId === 1250);
    expect(started).toMatchObject({
      title: "Aktualizacja dokumentacji API",
      redmineStatus: "In Progress",
      redmineTracker: "Wsparcie",
    });
    expect(
      snapshot.sessions
        .filter((s: any) => s.endedAt === null)
        .map((s: any) => s.taskId),
    ).toEqual([started.id]);

    // Szybkie dodawanie: przy połączonym Redmine `#1270` to zgłoszenie.
    await nav(page, "Zadania");
    const input = page.getByLabel("Nowe zadanie", { exact: true });
    await input.fill("#1270");
    await input.press("Enter");
    await expect(tile(page, "Formularz reklamacji")).toHaveCount(1);
    snapshot = await backend<any>(page, "snapshot");
    expect(snapshot.tasks.find((t: any) => t.redmineId === 1270)).toMatchObject({
      title: "Formularz reklamacji",
      redmineUrl: `${REDMINE_SITE}/issues/1270`,
      redmineStatus: "New",
    });
    const calls = (await backend<any[]>(page, "calls")).map((c) => c.cmd);
    expect(calls).toContain("redmine_start");
    expect(calls).toContain("quick_add");
  });
});
