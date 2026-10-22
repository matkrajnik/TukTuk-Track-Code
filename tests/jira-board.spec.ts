import { test, expect, type Page } from "./fixtures/test";
import {
  branchName,
  columnFor,
  decideMove,
  type JiraColumn,
  type JiraMoveOption,
} from "../src/store";
import { ids, openPreview, seed, stored } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * J2: Jira z poziomu TukTuk Track na symulowanej Jirze podglądu (`store.ts`).
 * Tablica WMS ma kolumny Do zrobienia, W toku, Code review i Gotowe; status
 * „Testy” nie ma kolumny i trafia do „W toku” według kategorii. „Zakończ”
 * (do Gotowe) wymaga pola Rozwiązanie, więc tylko otwiera zgłoszenie w Jirze.
 * Zadania z kluczem z `fixtures/seed.ts` są przypisane do mnie (WMS-12 „W toku”
 * z trwającym pomiarem, WMS-7 w koszu).
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
// Scenariusze tablicy mają wiele kroków; przy obciążonym komputerze 30 s to za mało.
test.describe.configure({ timeout: 60_000 });

const BOARD_KEY = "tuktuk-track-preview-jira-board-v1";
const JIRA_KEY = "tuktuk-track-preview-jira-v1";
const OPENED_KEY = "tuktuk-track-preview-jira-opened-v1";

const column = (page: Page, name: string) =>
  page.getByRole("region", { name: "Kolumna " + name, exact: true });
const card = (page: Page, key: string) =>
  page.locator(`.jira-card[data-key="${key}"]`);
const keys = (page: Page, name: string) =>
  column(page, name).locator(".jira-card").evaluateAll((cards) =>
    cards.map((c) => (c as HTMLElement).dataset.key),
  );
const boardIssue = (page: Page, key: string) =>
  page.evaluate(
    ([k, issue]) =>
      JSON.parse(localStorage.getItem(k) ?? "{}").issues?.find(
        (i: { key: string }) => i.key === issue,
      ) ?? null,
    [BOARD_KEY, key] as const,
  );

async function openBoard(page: Page, settings?: Record<string, unknown>) {
  await openPreview(page, {
    active: { task: ids.analysis, from: "11:00" },
    settings,
  });
  await page.getByRole("button", { name: "Jira", exact: true }).click();
  await page.getByRole("button", { name: "Tablica", exact: true }).click();
  await expect(column(page, "Do zrobienia")).toBeVisible();
}
async function wholeBoard(page: Page) {
  await page.getByRole("button", { name: "Cała tablica", exact: true }).click();
  await expect(card(page, "WMS-41")).toBeVisible();
}
async function menu(page: Page, key: string) {
  await page.getByRole("button", { name: "Akcje: " + key, exact: true }).click();
}

test("kolumny, wybór przejścia i nazwa gałęzi jak w Rust", () => {
  const columns: JiraColumn[] = [
    { name: "Do zrobienia", statusIds: ["1"] },
    { name: "W toku", statusIds: ["3"] },
    { name: "Review", statusIds: ["4"] },
    { name: "Gotowe", statusIds: ["5"] },
  ];
  const categories = {
    "1": "new",
    "3": "indeterminate",
    "4": "indeterminate",
    "5": "done",
    "6": "indeterminate",
  };
  expect(columnFor("4", "indeterminate", columns, categories)).toBe(2);
  // Status spoza kolumn: według kategorii.
  expect(columnFor("6", "indeterminate", columns, categories)).toBe(1);
  expect(columnFor("99", "done", columns, categories)).toBe(3);
  expect(columnFor("1", "new", columns.slice(1), categories)).toBe(0);
  const o = (
    id: string,
    toStatusId: string,
    toCategory: string,
    required: string[] = [],
  ): JiraMoveOption => ({
    id,
    name: id,
    toStatus: id,
    toStatusId,
    toCategory,
    required,
  });
  const options = [
    o("21", "3", "indeterminate"),
    o("22", "6", "indeterminate"),
    o("31", "4", "indeterminate"),
    o("41", "5", "done", ["Resolution"]),
  ];
  expect(decideMove(options, 2, columns, categories, null)).toEqual({
    kind: "perform",
    option: options[2],
  });
  expect(decideMove(options, 1, columns, categories, null)).toEqual({
    kind: "choose",
    options: [options[0], options[1]],
  });
  expect(decideMove(options, 3, columns, categories, null).kind).toBe("screen");
  expect(decideMove(options, 0, columns, categories, null).kind).toBe("none");
  expect(() => decideMove(options, 1, columns, categories, "41")).toThrow();
  expect(branchName("feature/", "WMS-123", "Krótki tytuł")).toBe(
    "feature/WMS-123-krotki-tytul",
  );
  expect(branchName("", "WMS-1", "Zażółć gęślą jaźń — ŁÓDŹ!")).toBe(
    "WMS-1-zazolc-gesla-jazn-lodz",
  );
  const long = branchName("x/", "WMS-42", "Bardzo długi tytuł ".repeat(6));
  expect(long.length).toBeLessThanOrEqual(52);
  expect(long.endsWith("-")).toBe(false);
});

test("tablica z kolumnami: moje i cała tablica, karta z czasem i planem", async ({
  page,
}) => {
  await openBoard(page);
  // „Moje”: WMS-31 i WMS-7 do zrobienia, WMS-12 w toku (status spoza kolumn).
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-31", "WMS-7"]);
  expect(await keys(page, "W toku")).toEqual(["WMS-12"]);
  await expect(
    column(page, "Do zrobienia").getByLabel("Zgłoszeń w kolumnie: 2"),
  ).toBeVisible();
  const running = card(page, "WMS-12");
  await expect(running).toContainText("Analiza importu");
  await expect(running).toContainText("mierzone teraz");
  await expect(running).toContainText("zaplanowane na dziś");
  // Czas lokalny z przedziałów zadania (z trwającym pomiarem od 11:00).
  await expect(running.locator(".jira-card-local .mono")).toHaveText(/^\d\d:\d\d:\d\d$/);
  await expect(
    running.getByRole("button", { name: "Zatrzymaj: WMS-12", exact: true }),
  ).toBeVisible();
  await expect(card(page, "WMS-31")).toContainText("Historia");
  await expect(card(page, "WMS-31")).toContainText("High");
  await expect(card(page, "WMS-31").getByLabel("Przypisane: Jan Kowalski")).toBeVisible();
  // „Cała tablica”: zgłoszenia innych osób i nieprzypisane; Testy → W toku.
  await wholeBoard(page);
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-31", "WMS-41", "WMS-7"]);
  expect(await keys(page, "W toku")).toEqual(["WMS-44", "WMS-12"]);
  expect(await keys(page, "Code review")).toEqual(["WMS-40"]);
  await expect(card(page, "WMS-44")).toContainText("Testy");
  await expect(card(page, "WMS-41").getByLabel("Nieprzypisane")).toBeVisible();
  // Start z karty uruchamia jeden pomiar zadania zgłoszenia.
  await card(page, "WMS-41")
    .getByRole("button", { name: "Rozpocznij: WMS-41", exact: true })
    .click();
  await expect(
    card(page, "WMS-41").getByRole("button", { name: "Zatrzymaj: WMS-41" }),
  ).toBeVisible();
  await expect(card(page, "WMS-41")).toContainText("mierzone teraz");
  const data = await stored(page);
  const task = data.tasks.find((t: { jiraKey?: string }) => t.jiraKey === "WMS-41");
  expect(task.title).toBe("Etykiety dla kurierów");
  expect(data.sessions.filter((s: { endedAt: null }) => s.endedAt === null)).toHaveLength(1);
  // Widok i zakres są zapamiętane.
  await page.reload();
  await page.getByRole("button", { name: "Jira", exact: true }).click();
  await expect(column(page, "Code review")).toBeVisible();
  await expect(card(page, "WMS-40")).toBeVisible();
});

test("przeciągnięcie wykonuje przejście, kilka przejść pyta, pola wymagane otwierają Jirę", async ({
  page,
}) => {
  await openBoard(page);
  // Jedno przejście do kolumny: status zmienia się w Jirze i na kafelku.
  await card(page, "WMS-12").dragTo(column(page, "Code review"));
  await expect(page.getByRole("status").filter({ hasText: "WMS-12 → Code review." })).toBeVisible();
  expect(await keys(page, "Code review")).toEqual(["WMS-12"]);
  expect((await boardIssue(page, "WMS-12")).status).toBe("Code review");
  expect(
    (await stored(page)).tasks.find((t: { id: number }) => t.id === ids.analysis).jiraStatus,
  ).toBe("Code review");
  // Do „W toku” prowadzą dwa przejścia (In Progress i Testy): wybór z listy.
  await card(page, "WMS-31").dragTo(column(page, "W toku"));
  const choice = page.getByRole("group", { name: "Wybierz przejście: WMS-31" });
  await expect(choice).toBeVisible();
  await expect(choice.getByRole("button", { name: /^Przesuń WMS-31 do: / })).toHaveText([
    "Rozpocznij → In Progress",
    "Do testów → Testy",
  ]);
  await choice.getByRole("button", { name: "Przesuń WMS-31 do: In Progress" }).click();
  await expect(choice).toHaveCount(0);
  expect(await keys(page, "W toku")).toEqual(["WMS-31"]);
  expect((await boardIssue(page, "WMS-31")).status).toBe("In Progress");
  // „Anuluj” wyboru cofa kartę.
  await card(page, "WMS-7").dragTo(column(page, "W toku"));
  await page
    .getByRole("group", { name: "Wybierz przejście: WMS-7" })
    .getByRole("button", { name: "Anuluj" })
    .click();
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-7"]);
  // Przejście z wymaganym polem: karta wraca, zgłoszenie otwiera się w Jirze.
  await card(page, "WMS-31").dragTo(column(page, "Gotowe"));
  await expect(page.getByRole("alert")).toContainText(
    "wymaga wypełnienia pól w Jirze (Rozwiązanie)",
  );
  expect(await keys(page, "W toku")).toEqual(["WMS-31"]);
  expect((await boardIssue(page, "WMS-31")).status).toBe("In Progress");
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), OPENED_KEY),
  ).toEqual(["https://asistonwms.atlassian.net/browse/WMS-31"]);
  // Klawiatura: „Przesuń do…” w menu karty.
  await menu(page, "WMS-31");
  await page
    .getByRole("button", { name: "Przesuń WMS-31 do kolumny: Code review" })
    .click();
  await expect.poll(() => keys(page, "Code review")).toEqual(["WMS-31", "WMS-12"]);
});

test("błąd Jiry cofa przesunięcie z komunikatem", async ({ page }) => {
  await openBoard(page);
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ ready: true, fail: 403 })),
    JIRA_KEY,
  );
  await card(page, "WMS-31").dragTo(column(page, "Code review"));
  await expect(page.getByRole("alert")).toContainText(
    "Nie przesunięto WMS-31 do „Code review”. Brak uprawnień do zmiany tego zgłoszenia w Jirze.",
  );
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-31", "WMS-7"]);
  expect(await keys(page, "Code review")).toEqual([]);
  expect((await boardIssue(page, "WMS-31")).status).toBe("Do zrobienia");
});

test("błąd po przeciągnięciu z otwartym wyborem cofa kartę do prawdziwej kolumny", async ({
  page,
}) => {
  await openBoard(page);
  await card(page, "WMS-31").dragTo(column(page, "W toku"));
  await expect(page.getByRole("group", { name: "Wybierz przejście: WMS-31" })).toBeVisible();
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ ready: true, fail: 403 })),
    JIRA_KEY,
  );
  await card(page, "WMS-31").dragTo(column(page, "Code review"));
  await expect(page.getByRole("alert")).toContainText("Nie przesunięto WMS-31 do „Code review”.");
  await expect(page.getByRole("group", { name: "Wybierz przejście: WMS-31" })).toHaveCount(0);
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-31", "WMS-7"]);
  expect(await keys(page, "W toku")).not.toContain("WMS-31");
  expect((await boardIssue(page, "WMS-31")).status).toBe("Do zrobienia");
});

test("przypisz do mnie, zmień osobę i komentarz z menu karty", async ({ page }) => {
  await openBoard(page);
  await wholeBoard(page);
  await menu(page, "WMS-41");
  await page.getByRole("button", { name: "Przypisz do mnie: WMS-41" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Przypisano WMS-41 do Ciebie." })).toBeVisible();
  await expect(card(page, "WMS-41").getByLabel("Przypisane: Jan Kowalski")).toBeVisible();
  expect((await boardIssue(page, "WMS-41")).assignee.displayName).toBe("Jan Kowalski");
  // Zmień osobę: wyszukiwarka osób, które można przypisać.
  await page.getByRole("button", { name: "Zmień osobę: WMS-41" }).click();
  await page.getByLabel("Szukaj osoby: WMS-41").fill("anna");
  await page.getByRole("button", { name: "Szukaj", exact: true }).click();
  await expect(page.getByRole("button", { name: /^Przypisz: / })).toHaveText(["Anna Nowak"]);
  await page.getByRole("button", { name: "Przypisz: Anna Nowak" }).click();
  await expect(card(page, "WMS-41").getByLabel("Przypisane: Anna Nowak")).toBeVisible();
  expect((await boardIssue(page, "WMS-41")).assignee.accountId).toBe("preview-anna");
  // Komentarz zwykłym tekstem.
  await page.getByRole("button", { name: "Dodaj komentarz: WMS-41" }).click();
  await page.getByLabel("Komentarz do WMS-41").fill("Etykiety gotowe.\n\nDo sprawdzenia.");
  await page.getByRole("button", { name: "Wyślij komentarz" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Dodano komentarz do WMS-41." }),
  ).toBeVisible();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!).comments, BOARD_KEY),
  ).toEqual({ "WMS-41": ["Etykiety gotowe.\n\nDo sprawdzenia."] });
  // Nazwa gałęzi z prefiksem z ustawień.
  await page.getByRole("button", { name: "Kopiuj nazwę gałęzi: WMS-41" }).click();
  await expect(page.locator(".jira-action-message")).toContainText(
    "feature/WMS-41-etykiety-dla-kurierow",
  );
});

test("nowe zgłoszenie tworzy zadanie w TukTuk Track; typ z wymaganymi polami odsyła do Jiry", async ({
  page,
}) => {
  await openBoard(page);
  await page.getByRole("button", { name: "Nowe zgłoszenie", exact: true }).click();
  const form = page.getByRole("form", { name: "Nowe zgłoszenie" });
  await expect(form.getByLabel("Projekt")).toHaveAttribute("data-value", "WMS");
  await form.getByLabel("Typ zgłoszenia").selectOption({ label: "Błąd" });
  await expect(form.getByRole("alert")).toContainText(
    "Typ „Błąd” wymaga pól, których TukTuk Track nie wypełnia: Środowisko.",
  );
  await form.getByLabel("Tytuł zgłoszenia").fill("Raport przyjęć");
  await expect(form.getByRole("button", { name: "Utwórz zgłoszenie" })).toBeDisabled();
  await form.getByRole("button", { name: "Utwórz w Jirze" }).click();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), OPENED_KEY),
  ).toEqual(["https://asistonwms.atlassian.net/secure/CreateIssue!default.jspa"]);
  await form.getByLabel("Typ zgłoszenia").selectOption({ label: "Zadanie" });
  await expect(form.getByRole("alert")).toHaveCount(0);
  await expect(form.getByLabel("Przypisz do mnie")).toBeChecked();
  await expect(form.getByLabel("Utwórz też zadanie w TukTuk Track")).toBeChecked();
  await form.getByLabel("Opis zgłoszenia").fill("Zestawienie dzienne.");
  await form.getByLabel("Dzień planu zadania").fill("2026-10-16");
  await form.getByRole("button", { name: "Utwórz zgłoszenie" }).click();
  await expect(
    page.getByRole("status").filter({ hasText: "Utworzono WMS-100 i zadanie w TukTuk Track." }),
  ).toBeVisible();
  // Nowe zgłoszenie na tablicy („Moje”, do zrobienia) i zadanie z planem.
  await expect(card(page, "WMS-100")).toContainText("Raport przyjęć");
  await expect(card(page, "WMS-100")).toContainText("zaplanowane na");
  const task = (await stored(page)).tasks.find(
    (t: { jiraKey?: string }) => t.jiraKey === "WMS-100",
  );
  expect(task.title).toBe("Raport przyjęć");
  expect(task.details.plannedDay).toBe("2026-10-16");
  expect(task.jiraUrl).toBe("https://asistonwms.atlassian.net/browse/WMS-100");
  expect((await stored(page)).sessions.filter((s: { taskId: number }) => s.taskId === task.id)).toEqual([]);
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Przedziały: Raport przyjęć", exact: true }),
  ).toBeVisible();
});

test("wyłączone zmiany w Jirze: tablica tylko do odczytu", async ({ page }) => {
  await openBoard(page, { jiraWrite: false });
  await expect(page.getByText("Tablica tylko do odczytu")).toBeVisible();
  await expect(card(page, "WMS-31")).toHaveAttribute("draggable", "false");
  await expect(page.getByRole("button", { name: "Nowe zgłoszenie", exact: true })).toBeDisabled();
  await card(page, "WMS-31").dragTo(column(page, "Code review"));
  expect(await keys(page, "Do zrobienia")).toEqual(["WMS-31", "WMS-7"]);
  await menu(page, "WMS-31");
  await expect(page.getByRole("group", { name: "Przesuń do: WMS-31" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Przypisz do mnie: WMS-31" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Kopiuj nazwę gałęzi: WMS-31" })).toBeVisible();
  // Interfejs nie wysyła zapisu, a symulowany backend i tak go odrzuca.
  expect((await boardIssue(page, "WMS-31")).status).toBe("Do zrobienia");
});

test("rozwinięty kafelek zadania z kluczem ma akcje Jiry", async ({ page }) => {
  await openPreview(page, { settings: { jiraBranchPrefix: "bugfix/" } });
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await page.getByRole("button", { name: "Kopiuj nazwę gałęzi: WMS-12" }).click();
  await expect(page.locator(".jira-action-message")).toContainText(
    "bugfix/WMS-12-analiza-importu",
  );
  await page.getByRole("button", { name: "Dodaj komentarz: WMS-12" }).click();
  await page.getByLabel("Komentarz do WMS-12").fill("Import działa.");
  await page.getByRole("button", { name: "Wyślij komentarz" }).click();
  await expect(page.locator(".jira-action-message")).toHaveText(
    "Dodano komentarz do WMS-12.",
  );
  await page.getByRole("button", { name: "Przypisz do mnie: WMS-12" }).click();
  await expect(page.locator(".jira-action-message")).toHaveText(
    "Przypisano WMS-12 do Ciebie.",
  );
  // Prefiks gałęzi w Ustawieniach → Jira.
  await openSettings(page, "Jira");
  await expect(page.getByLabel("Prefiks nazwy gałęzi")).toHaveValue("bugfix/");
  await page.getByLabel("Prefiks nazwy gałęzi").fill("feature x");
  await page.getByLabel("Prefiks nazwy gałęzi").press("Tab");
  await expect(page.getByRole("alert")).toContainText("Prefiks gałęzi");
});

test("lista zgłoszeń w podglądzie działa na symulowanej Jirze", async ({ page }) => {
  await openPreview(page, { data: seed() });
  await page.getByRole("button", { name: "Jira", exact: true }).click();
  await expect(page.locator(".jira-issue")).toHaveCount(4);
  await page.getByLabel("Szukaj moich zgłoszeń").fill("przyjęcia");
  await expect(page.locator(".jira-issue")).toHaveCount(1);
  await page.getByRole("button", { name: "Rozpocznij: WMS-31", exact: true }).click();
  await expect(page.getByRole("button", { name: "Zatrzymaj: WMS-31", exact: true })).toBeVisible();
});

/** Wstrzymane wywołania symulowanej Jiry (`previewJiraHoldKey`). */
const HOLD_KEY = "tuktuk-track-preview-jira-hold-v1";
const hold = (page: Page, names: string[]) =>
  page.evaluate(
    ([k, v]) => localStorage.setItem(k, JSON.stringify(v)),
    [HOLD_KEY, names] as const,
  );

test("zapis nowego zgłoszenia w trakcie odświeżania nie przepada", async ({ page }) => {
  await openBoard(page);
  await page.getByRole("button", { name: "Nowe zgłoszenie", exact: true }).click();
  const form = page.getByRole("form", { name: "Nowe zgłoszenie" });
  await form.getByLabel("Typ zgłoszenia").selectOption({ label: "Zadanie" });
  await form.getByLabel("Tytuł zgłoszenia").fill("Raport wydań");
  await form.getByLabel("Dzień planu zadania").fill("2026-10-16");
  await hold(page, ["create", "read"]);
  await form.getByRole("button", { name: "Utwórz zgłoszenie" }).click();
  await page.getByRole("button", { name: "Odśwież", exact: true }).click();
  await expect(page.getByText("Proszę czekać…")).toBeVisible();
  // Jira tworzy zgłoszenie, gdy odświeżanie jeszcze trwa: wynik od razu.
  await hold(page, ["read"]);
  await expect(
    page.getByRole("status").filter({ hasText: "Utworzono WMS-100 i zadanie w TukTuk Track." }),
  ).toBeVisible();
  await expect(form).toHaveCount(0);
  await hold(page, []);
  // Odświeżenie po utworzeniu wykonuje się po bieżącym: karta na tablicy
  // z planem nowego zadania (dane przekazane do App).
  await expect(card(page, "WMS-100")).toContainText("Raport wydań");
  await expect(card(page, "WMS-100")).toContainText("zaplanowane na");
  expect(
    (await stored(page)).tasks.filter((t: { jiraKey?: string }) => t.jiraKey === "WMS-100"),
  ).toHaveLength(1);
});

test("odświeżenie tablicy w trakcie przesuwania nie gubi wyniku", async ({ page }) => {
  await openBoard(page);
  await hold(page, ["move", "read"]);
  await card(page, "WMS-12").dragTo(column(page, "Code review"));
  await expect(card(page, "WMS-12")).toHaveClass(/isPending/);
  await page.getByRole("button", { name: "Odśwież", exact: true }).click();
  await expect(page.getByText("Proszę czekać…")).toBeVisible();
  await hold(page, ["read"]);
  await expect(
    page.getByRole("status").filter({ hasText: "WMS-12 → Code review." }),
  ).toBeVisible();
  await hold(page, []);
  await expect.poll(() => keys(page, "Code review")).toEqual(["WMS-12"]);
});
