import { test, expect, type Page } from "@playwright/test";
import { ids, jiraFixture, openPreview, seed } from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
import { matchingSections, sectionInMessage } from "../src/navigation";

/** Zakładka Ustawienia (S1 w PLAN_1.7.md). */
test.use({ timezoneId: "Europe/Warsaw" });

const sections = [
  "Czas pracy",
  "Kalendarz",
  "Timesheet (TS)",
  "Jira",
  "Redmine",
  "Powiadomienia",
  "Skupienie",
  "Szablony",
  "Okna",
  "Skróty",
  "Uruchamianie",
];
const navButton = (page: Page, name: string) =>
  page
    .getByRole("navigation", { name: "Nawigacja główna" })
    .getByRole("button", { name, exact: true });
const tab = (page: Page, name: string) =>
  page.getByRole("tab", { name, exact: true });
/** Sekcja widoczna w treści: zaznaczona na liście i z nagłówkiem. */
async function expectSection(page: Page, name: string) {
  await expect(navButton(page, "Ustawienia")).toHaveAttribute(
    "aria-current",
    "page",
  );
  await expect(tab(page, name)).toHaveAttribute("aria-selected", "true");
  await expect(
    page
      .getByRole("tabpanel")
      .getByRole("heading", { name, exact: true }),
  ).toBeVisible();
}

test("Ustawienia to ostatnia pozycja nawigacji z listą sekcji; Historia i kopie bez podzakładki", async ({
  page,
}) => {
  await openPreview(page);
  const nav = page.getByRole("navigation", { name: "Nawigacja główna" });
  await expect(nav.getByRole("button").last()).toHaveText("Ustawienia");
  await navButton(page, "Ustawienia").click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Ustawienia" }),
  ).toBeVisible();
  await expect(
    page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab"),
  ).toHaveText(sections);
  // Domyślnie pierwsza sekcja, z polami jak dotąd.
  await expectSection(page, "Czas pracy");
  await expect(page.getByLabel("Cel dzienny (minuty)")).toBeVisible();
  await expect(page.getByLabel("Pokazuj mini-widget zawsze na wierzchu")).toHaveCount(0);

  for (const name of sections) {
    await tab(page, name).click();
    await expectSection(page, name);
  }

  // Strzałki przenoszą wybór po liście.
  await tab(page, "Okna").click();
  await page.keyboard.press("ArrowDown");
  await expectSection(page, "Skróty");
  await expect(tab(page, "Skróty")).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expectSection(page, "Okna");

  await navButton(page, "Historia i kopie").click();
  await expect(
    page.getByRole("button", { name: "Kopie", exact: true }),
  ).toBeVisible();
  // Jedyny przycisk „Ustawienia” to pozycja nawigacji; podzakładki już nie ma.
  await expect(
    page.getByRole("button", { name: "Ustawienia", exact: true }),
  ).toHaveCount(1);
});

test("Ctrl+, otwiera Ustawienia z każdego widoku na ostatniej sekcji", async ({
  page,
}) => {
  await openPreview(page);
  await navButton(page, "Dashboard").click();
  await page.keyboard.press("Control+,");
  await expectSection(page, "Czas pracy");
  await tab(page, "Powiadomienia").click();
  await navButton(page, "Plan i historia").click();
  await page.keyboard.press("Control+,");
  await expectSection(page, "Powiadomienia");
});

test("Szukaj ustawienia filtruje sekcje po etykietach, także bez polskich znaków", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
  const tabs = page
    .getByRole("tablist", { name: "Sekcje ustawień" })
    .getByRole("tab");

  await search.fill("mini-widget");
  await expect(tabs).toHaveText(["Okna"]);
  await expectSection(page, "Okna");
  await expect(
    page.getByLabel("Pokazuj mini-widget zawsze na wierzchu"),
  ).toBeVisible();

  await search.fill("skrot");
  await expect(tabs).toHaveText(["Skróty"]);
  await expectSection(page, "Skróty");

  // Kilka pasujących sekcji: treść pokazuje pierwszą, klik wybiera inną.
  await search.fill("przerwa");
  await expect(tabs).toHaveText(["Czas pracy", "Skupienie"]);
  await expectSection(page, "Czas pracy");
  await tab(page, "Skupienie").click();
  await expectSection(page, "Skupienie");

  await search.fill("client id");
  await expect(tabs).toHaveText(["Kalendarz"]);

  await search.fill("zzz");
  await expect(tabs).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveText(
    "Brak ustawień pasujących do „zzz”.",
  );

  await search.fill("");
  await expect(tabs).toHaveText(sections);
  await expectSection(page, "Skupienie");
});

test("zapamiętana sekcja przeżywa zmianę widoku i przeładowanie", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Okna");
  await navButton(page, "Zadania").click();
  await navButton(page, "Ustawienia").click();
  await expectSection(page, "Okna");
  await page.reload();
  await navButton(page, "Ustawienia").click();
  await expectSection(page, "Okna");
});

test("linki z innych widoków i navigate otwierają właściwą sekcję", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });

  // Dashboard → karta Skupienie.
  await navButton(page, "Dashboard").click();
  await page
    .getByRole("region", { name: "Bloki skupienia" })
    .getByRole("button", { name: "Długość bloku i przerwy" })
    .click();
  await expectSection(page, "Skupienie");
  await expect(page.getByLabel("Blok skupienia (minuty)")).toBeVisible();

  // Kafelek zadania → zapisany szablon → Ustawienia → Szablony.
  await navButton(page, "Zadania").click();
  await page.getByLabel("Przedziały: Code review", { exact: true }).click();
  await page
    .getByRole("button", { name: "Zapisz jako szablon: Code review" })
    .click();
  await page
    .getByLabel("Nazwa szablonu: Code review", { exact: true })
    .fill("review");
  await page.getByRole("button", { name: "Zapisz szablon", exact: true }).click();
  await page
    .getByRole("status")
    .filter({ hasText: "Zapisano szablon „review”" })
    .getByRole("button", { name: "Ustawieniach → Szablony" })
    .click();
  await expectSection(page, "Szablony");
  await expect(page.getByRole("form", { name: "Szablon review" })).toBeVisible();

  // Zdarzenie `navigate` (kliknięcie powiadomienia) z polem `section`.
  await navButton(page, "Zadania").click();
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("todo-track-navigate", {
        detail: { view: "settings", section: "notifications" },
      }),
    ),
  );
  await expectSection(page, "Powiadomienia");
  // Nieznana sekcja zostawia ostatnią.
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("todo-track-navigate", {
        detail: { view: "settings", section: "nie-ma" },
      }),
    ),
  );
  await expectSection(page, "Powiadomienia");

  // Ustawienia → Jira prowadzi z powrotem do widoku Jira (połączenie konta).
  await tab(page, "Jira").click();
  await page.getByRole("button", { name: "Połączenie z Jirą" }).click();
  await expect(navButton(page, "Jira")).toHaveAttribute("aria-current", "page");
});

test("widok Jira ma link do Ustawienia → Jira", async ({ page }) => {
  await openDesktop(page, {
    data: seed(),
    jira: jiraFixture(),
  });
  await navButton(page, "Jira").click();
  await page.getByRole("button", { name: "Ustawienia → Jira" }).click();
  await expectSection(page, "Jira");
  await expect(
    page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie"),
  ).toBeVisible();
});

test.describe("przy 760 px", () => {
  test.use({ viewport: { width: 760, height: 800 } });
  test("lista sekcji jest rozwijana nad treścią", async ({ page }) => {
    await openPreview(page);
    await navButton(page, "Ustawienia").click();
    const select = page.getByRole("combobox", { name: "Sekcja ustawień" });
    await expect(select).toBeVisible();
    await expect(page.getByRole("tablist")).toBeHidden();
    await expect(select).toHaveValue("work");
    const content = page.locator("#settings-content");
    expect((await select.boundingBox())!.y).toBeLessThan(
      (await content.boundingBox())!.y,
    );
    await select.selectOption({ label: "Uruchamianie" });
    await expect(
      page.getByRole("heading", { name: "Uruchamianie", exact: true }),
    ).toBeVisible();
    await expect(page.getByLabel("Uruchamiaj z Windows")).toBeVisible();
  });
});

test("wyszukiwanie sekcji i rozpoznanie sekcji w komunikacie (moduł)", () => {
  expect(matchingSections("").map((s) => s.name)).toEqual(sections);
  expect(matchingSections("ŚWIĘTA").map((s) => s.id)).toEqual(["work"]);
  expect(matchingSections("autostart").map((s) => s.id)).toEqual(["startup"]);
  expect(
    sectionInMessage("Połącz konto Microsoft w Ustawieniach → Kalendarz."),
  ).toBe("calendar");
  expect(
    sectionInMessage(
      "Nie ma szablonu „x”. Dodaj go w Ustawieniach → Szablony albo poprzedź / znakiem \\.",
    ),
  ).toBe("templates");
  expect(sectionInMessage("Zwykły błąd zapisu.")).toBeNull();
});

test("wklejona etykieta pola znajduje jej sekcję", async ({ page }) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
  const tabs = page
    .getByRole("tablist", { name: "Sekcje ustawień" })
    .getByRole("tab");
  for (const [label, name] of [
    ["Pokazuj tytuły spotkań", "Kalendarz"],
    ["Pokazuj mini-widget zawsze na wierzchu", "Okna"],
    ["Uwzględniaj święta w celu", "Czas pracy"],
    ["Powiadom o osiągnięciu celu dnia", "Powiadomienia"],
    ["Wieczorne „pomiar nadal trwa”", "Powiadomienia"],
    ["Uruchamiaj schowane do traya", "Uruchamianie"],
  ]) {
    await search.fill(label);
    await expect(tabs).toHaveText([name]);
    await expect(page.getByLabel(label, { exact: true })).toBeVisible();
  }
  // Słowa w innej kolejności i bez polskich znaków też pasują.
  await search.fill("spotkan pokazuj");
  await expect(tabs).toHaveText(["Kalendarz"]);
});

test("wyszukiwanie po słowach (moduł)", () => {
  expect(matchingSections("Pokazuj tytuły spotkań").map((s) => s.id)).toEqual([
    "calendar",
  ]);
  expect(
    matchingSections("Przerwa zatrzymuje pomiar").map((s) => s.id),
  ).toEqual(["focus"]);
  expect(matchingSections("pokazuj zzz")).toEqual([]);
});

test("błąd nieznanego szablonu w szybkim dodawaniu i przełączaniu prowadzi do Ustawienia → Szablony", async ({
  page,
}) => {
  await openPreview(page);
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  await input.fill("/nieznany Popraw raport");
  const chips = page.getByLabel("Rozpoznane pola");
  await expect(chips.getByRole("alert")).toContainText(
    "Nie ma szablonu „nieznany”.",
  );
  await chips.getByRole("button", { name: "Otwórz ustawienia" }).click();
  await expectSection(page, "Szablony");

  // Szybkie przełączanie: przycisk zamyka okno i otwiera tę samą sekcję.
  await openSettings(page, "Czas pracy");
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await dialog
    .getByLabel("Znajdź zadanie do uruchomienia")
    .fill("/nieznany Popraw raport");
  await dialog.getByRole("button", { name: "Otwórz ustawienia" }).click();
  await expect(dialog).toBeHidden();
  await expectSection(page, "Szablony");
});
