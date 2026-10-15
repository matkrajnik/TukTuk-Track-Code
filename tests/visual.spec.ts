import { test, expect, type Page } from "@playwright/test";
import { ids, openPreview } from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";

/**
 * Zrzuty wzorcowe widoków (`npm run test:visual:update` odświeża wzorce).
 * Dane z `fixtures/seed.ts` z trwającym pomiarem, zegar zatrzymany na
 * `NOW` (setFixedTime), więc liczniki i daty nie zmieniają się między
 * przebiegami. Animacje, przejścia i kursor tekstowy wyłącza konfiguracja
 * `expect.toHaveScreenshot` w playwright.config.ts; mysz po nawigacji trafia
 * w róg okna, żeby żaden przycisk nie miał stanu :hover.
 *
 * Testy mają tag @visual (CI uruchamia je osobnym krokiem). Wzorce są
 * zależne od systemu (sufiks `-win32`): generowane na Windows,
 * tak jak CI na windows-latest.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ mode: "parallel" });

const themes = ["dark", "light"] as const;
const widths = [760, 1280] as const;
type Theme = (typeof themes)[number];

async function useTheme(page: Page, theme: Theme) {
  // Uruchamia się przed skryptami strony przy każdym ładowaniu, także po
  // `localStorage.clear()` w openPreview i przeładowaniu.
  await page.addInitScript(
    (t) => localStorage.setItem("todo-track-theme", t),
    theme,
  );
}

async function settle(page: Page) {
  await page.mouse.move(0, 0);
  await page.evaluate(() => document.fonts.ready);
  // Fokus po kliknięciu nie jest częścią wzorca.
  await page.evaluate(() => (document.activeElement as HTMLElement)?.blur());
}

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

const views: {
  name: string;
  open: (page: Page) => Promise<void>;
  ready: (page: Page) => Promise<void>;
}[] = [
  {
    name: "zadania",
    open: async () => {},
    ready: (page) =>
      expect(page.getByText("Analiza importu").first()).toBeVisible(),
  },
  {
    name: "dashboard",
    open: (page) => nav(page, "Dashboard"),
    ready: (page) =>
      expect(
        page.getByRole("button", { name: "Dashboard", exact: true }),
      ).toHaveAttribute("aria-pressed", "true"),
  },
  {
    name: "plan-dzien",
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Dzień");
    },
    ready: (page) =>
      expect(page.getByRole("region", { name: "Plan dnia" })).toBeVisible(),
  },
  {
    name: "plan-tablica-tygodnia",
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Tydzień");
      await nav(page, "Tablica");
    },
    ready: (page) =>
      expect(page.locator(".week-card-title").first()).toBeVisible(),
  },
  {
    name: "zamkniecie-dnia",
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Zamknięcie dnia");
    },
    ready: (page) =>
      expect(
        page.getByRole("heading", { name: /^Zamknięcie dnia/ }),
      ).toBeVisible(),
  },
  {
    name: "ustawienia",
    open: (page) =>
      page.getByRole("button", { name: "Otwórz ustawienia" }).click(),
    ready: (page) =>
      expect(
        page.getByRole("button", { name: "Ustawienia", exact: true }),
      ).toBeVisible(),
  },
];

for (const theme of themes)
  for (const width of widths) {
    test.describe(`${theme} ${width}px`, { tag: "@visual" }, () => {
      test.use({ viewport: { width, height: 800 } });

      for (const view of views)
        test(`widok ${view.name}`, async ({ page }) => {
          await useTheme(page, theme);
          await openPreview(page, {
            active: { task: ids.analysis, from: "11:00" },
          });
          await view.open(page);
          await view.ready(page);
          await settle(page);
          await expect(page).toHaveScreenshot(
            `${view.name}-${theme}-${width}.png`,
            { fullPage: true },
          );
        });

      // W podglądzie Jira jest wyłączona, więc widok idzie przez zastępcze IPC.
      test("widok jira", async ({ page }) => {
        await useTheme(page, theme);
        await openDesktop(page, {
          jira: {
            boards: [
              { id: 1, name: "WMS" },
              { id: 2, name: "Foundation" },
            ],
            issues: [
              {
                key: "WMS-12",
                summary: "Analiza importu",
                status: "W toku",
                boards: ["WMS"],
              },
              {
                key: "WMS-31",
                summary: "Obsługa przyjęcia towaru",
                status: "Do zrobienia",
                boards: ["WMS", "Foundation"],
              },
            ],
          },
        });
        await nav(page, "Jira");
        await expect(page.locator(".jira-issue")).toHaveCount(2);
        await settle(page);
        await expect(page).toHaveScreenshot(`jira-${theme}-${width}.png`, {
          fullPage: true,
        });
      });
    });
  }

for (const theme of themes)
  test(`mini-widget ${theme}`, { tag: "@visual" }, async ({ page }) => {
    await page.setViewportSize({ width: 320, height: 64 });
    await useTheme(page, theme);
    // Najpierw dane, potem widget: openPreview ładuje główne okno.
    await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
    await page.goto("/#widget");
    // Zmiana samego hasha nie ładuje strony ponownie, a okno wybiera widok przy starcie.
    await page.reload();
    const widget = page.locator("#widget");
    await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveText(
      "01:00:00",
    );
    await settle(page);
    await expect(page).toHaveScreenshot(`widget-${theme}.png`);
  });
