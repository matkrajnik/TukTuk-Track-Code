import { test, expect, type Page } from "./fixtures/test";
import {
  calendarFixture,
  ids,
  jiraFixture,
  manualMeetings,
  openPreview,
  seed,
} from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";

/**
 * Zrzuty wzorcowe widoków (`npm run test:visual:update` odświeża wzorce).
 * Dane z `fixtures/seed.ts` z trwającym pomiarem, zegar zatrzymany na
 * `NOW` (setFixedTime), więc liczniki i daty nie zmieniają się między
 * przebiegami. Widoki planu i Zamknięcia dnia mają połączony kalendarz
 * z przykładowymi spotkaniami (`calendarFixture`). Animacje, przejścia i kursor tekstowy wyłącza konfiguracja
 * `expect.toHaveScreenshot` w playwright.config.ts; mysz po nawigacji trafia
 * w róg okna, żeby żaden przycisk nie miał stanu :hover.
 *
 * Testy mają tag @visual (CI uruchamia je osobnym krokiem). Wzorce są
 * zależne od systemu (sufiks `-win32`). Obecne wzorce powstały lokalnie na
 * Windows 11; renderowanie czcionek na windows-latest może się różnić. Wzorce
 * z CI daje ręczne uruchomienie workflow Check z `update_snapshots`
 * (artefakt `visual-snapshots` do skopiowania do `tests/visual.spec.ts-snapshots`).
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
  /** Połączony kalendarz z przykładowymi spotkaniami (tor, pojemność, sekcja). */
  calendar?: boolean;
  /** Ręczne spotkania (B12) bez połączonego kalendarza. */
  meetings?: boolean;
}[] = [
  {
    // B1: rozwinięty kafelek — przyciski drugorzędne z tłem i ikoną, przedziały
    // bez sekund, pole „Cykl” z „Bez cyklu”.
    name: "zadania-kafelek",
    open: (page) =>
      page
        .getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
        .click(),
    ready: async (page) => {
      await expect(
        page.getByRole("group", { name: "Akcje zadania" }),
      ).toBeVisible();
      await expect(page.getByLabel("Rodzaj cyklu")).toHaveValue("none");
    },
  },
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
      ).toHaveAttribute("aria-current", "page"),
  },
  {
    // B5: lista Raportu dnia jak kafelki (projekt, klucz i status, plan
    // z procentem, termin, notatka dnia) bez akcji Start / Ukończ.
    name: "raport-dnia",
    open: async (page) => {
      await nav(page, "Raporty dzienne");
      await page.getByLabel("Dzień raportu").fill("2026-10-14");
    },
    ready: (page) =>
      expect(page.locator(".report-card .task-day-note")).toHaveText(
        "Notatka: Mapowanie kolumn",
      ),
  },
  {
    // B5: Miesiąc z kolumną i sumą nadgodzin pod wykresem dni.
    name: "miesiac",
    open: async (page) => {
      await nav(page, "Raporty dzienne");
      await nav(page, "Miesiąc");
    },
    ready: (page) =>
      expect(page.locator(".monthly-day-foot")).toBeVisible(),
  },
  {
    name: "plan-dzien",
    calendar: true,
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Dzień");
    },
    ready: (page) =>
      expect(page.getByRole("region", { name: "Plan dnia" })).toBeVisible(),
  },
  {
    // B12: oś dnia z ręcznymi spotkaniami bez połączonego kalendarza
    // (tor „Spotkania”, kropkowany obrys, plan omijający spotkania).
    name: "plan-dzien-reczne",
    meetings: true,
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Dzień");
    },
    ready: (page) =>
      expect(page.locator(".dtl-meeting.manual")).toHaveCount(2),
  },
  {
    name: "plan-tablica-tygodnia",
    calendar: true,
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Tydzień");
      await nav(page, "Tablica");
    },
    ready: (page) =>
      expect(page.locator(".week-card-title").first()).toBeVisible(),
  },
  {
    // B13: Rano z „Rozpocznij dzień” i paskiem z „Zakończ dzień” obok Stop.
    name: "rano",
    calendar: true,
    open: async (page) => {
      await nav(page, "Plan i historia");
      await nav(page, "Rano");
    },
    ready: async (page) => {
      await expect(
        page.getByRole("region", { name: "Przegląd dnia" }),
      ).toBeVisible();
      await expect(
        page.getByRole("button", { name: "Rozpocznij dzień" }),
      ).toBeVisible();
    },
  },
  {
    name: "zamkniecie-dnia",
    calendar: true,
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
    // Pierwsza sekcja (Czas pracy); openPreview czyści zapamiętaną sekcję.
    open: (page) => nav(page, "Ustawienia"),
    ready: (page) =>
      expect(
        page.getByRole("heading", { name: "Czas pracy", exact: true }),
      ).toBeVisible(),
  },
  {
    // R3: symulowany Redmine podglądu — śledzony projekt, grupy statusów,
    // karty z trackerem, priorytetem, terminem i podprojektem oraz sekcja
    // spoza śledzonych projektów.
    name: "redmine",
    open: (page) => nav(page, "Redmine"),
    ready: async (page) => {
      await expect(page.locator(".redmine-group")).toHaveCount(3);
      await expect(
        page.getByRole("region", {
          name: "Przypisane do mnie spoza śledzonych projektów",
        }),
      ).toBeVisible();
    },
  },
  {
    // R5: Ustawienia → Redmine z połączonym kontem symulowanego Redmine
    // (adres przykładowy, jeden śledzony projekt, wartości domyślne statusów).
    name: "ustawienia-redmine",
    open: async (page) => {
      await page.evaluate(() =>
        localStorage.setItem("todo-track-settings-section", "redmine"),
      );
      await page.reload();
      await nav(page, "Ustawienia");
    },
    ready: async (page) => {
      await expect(
        page.getByRole("heading", { name: "Redmine", exact: true }),
      ).toBeVisible();
      await expect(
        page.getByText("Połączono jako Jan Kowalski (jkowalski)"),
      ).toBeVisible();
      await expect(page.locator(".redmine-tracked li")).toHaveCount(1);
    },
  },
  {
    // A1: Ustawienia → Asystent AI z wykrytymi Claude Code i Codexem, wybranym
    // Claude Code i jedną próbą w „Ostatnich wywołaniach”.
    name: "ustawienia-asystent-ai",
    open: async (page) => {
      await page.evaluate(() => {
        localStorage.setItem(
          "todo-track-preview-ai-v1",
          JSON.stringify({
            claude: {
              path: "C:\\Users\\jan\\.local\\bin\\claude.exe",
              version: "2.1.304",
            },
            codex: {
              path: "C:\\Users\\jan\\AppData\\Roaming\\npm\\codex.cmd",
              version: "0.162.1",
            },
            calls: [
              {
                kind: "test",
                at: 1792058400000,
                length: 96,
                ok: true,
                error: "",
                cached: false,
              },
            ],
          }),
        );
        localStorage.setItem(
          "todo-track-preview-settings-v1",
          JSON.stringify({ aiEngine: "claude", aiHiddenNames: ["Jan Kowalski"] }),
        );
        localStorage.setItem("todo-track-settings-section", "ai");
      });
      await page.reload();
      await nav(page, "Ustawienia");
    },
    ready: async (page) => {
      await expect(
        page.getByRole("heading", { name: "Asystent AI", exact: true }),
      ).toBeVisible();
      await expect(page.getByText("Claude Code: wersja 2.1.304")).toBeVisible();
      await expect(page.getByRole("button", { name: "Przetestuj silnik" })).toBeEnabled();
      await expect(page.getByText("test · 96 znaków · ok")).toBeVisible();
    },
  },
  {
    // A3: panel czatu po prawej z pytaniem, odpowiedzią symulatora, listą użytych
    // narzędzi i kartą akcji „Utwórz zadanie”; pozycja „Asystent” w nawigacji.
    name: "asystent-czat",
    open: async (page) => {
      await page.evaluate(() => {
        localStorage.setItem(
          "todo-track-preview-ai-v1",
          JSON.stringify({
            claude: {
              path: "C:\\Users\\jan\\.local\\bin\\claude.exe",
              version: "2.1.304",
            },
          }),
        );
        localStorage.setItem(
          "todo-track-preview-settings-v1",
          JSON.stringify({ aiEngine: "claude" }),
        );
      });
      await page.reload();
      await page.keyboard.press("Control+j");
      const input = page.getByRole("textbox", {
        name: "Wiadomość do asystenta",
      });
      await input.fill("Utwórz zadanie „Poprawić raport błędów”");
      await input.press("Enter");
    },
    ready: async (page) => {
      await expect(
        page.getByRole("region", { name: "Propozycja: Poprawić raport błędów" }),
      ).toBeVisible();
      await expect(page.getByRole("button", { name: "Wyślij" })).toBeVisible();
    },
  },
  {
    name: "ustawienia-timesheet",
    // Połączone konto symulowanego TS z jednym domyślnym zadaniem projektu.
    open: async (page) => {
      await page.evaluate(() => {
        localStorage.setItem(
          "todo-track-preview-ts-v1",
          JSON.stringify({
            connected: true,
            login: "jan.kowalski",
            deviceId: "0123456789abcdef0123456789abcdef",
            projectMap: [
              { project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" },
            ],
          }),
        );
        // Zapamiętana sekcja: przy 760 px lista sekcji jest polem wyboru.
        localStorage.setItem("todo-track-settings-section", "timesheet");
      });
      await page.reload();
      await nav(page, "Ustawienia");
    },
    ready: async (page) => {
      await expect(
        page.getByRole("heading", { name: "Timesheet (TS)", exact: true }),
      ).toBeVisible();
      await expect(page.getByText("Wewnętrzne → Spotkania")).toBeVisible();
    },
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
            calendar: view.calendar ? calendarFixture() : undefined,
            meetings: view.meetings ? manualMeetings() : undefined,
          });
          await view.open(page);
          await view.ready(page);
          // Spotkania wczytują się osobno od danych zadań.
          if (view.calendar)
            await expect(
              page
                .locator(".dtl-meeting, .week-capacity-meetings, .close-meetings")
                .first(),
            ).toBeVisible();
          await settle(page);
          await expect(page).toHaveScreenshot(
            `${view.name}-${theme}-${width}.png`,
            { fullPage: true },
          );
        });

      // W podglądzie Jira jest wyłączona, więc widok idzie przez zastępcze IPC
      // z tymi samymi danymi i tym samym trwającym pomiarem co inne widoki.
      test("widok jira", async ({ page }) => {
        await useTheme(page, theme);
        await openDesktop(page, {
          data: seed({ active: { task: ids.analysis, from: "11:00" } }),
          jira: jiraFixture(),
        });
        await nav(page, "Jira");
        await expect(page.locator(".jira-issue")).toHaveCount(3);
        await settle(page);
        await expect(page).toHaveScreenshot(`jira-${theme}-${width}.png`, {
          fullPage: true,
        });
      });

      // J2: ta sama Jira jako tablica z kolumnami (karty z typem, priorytetem,
      // osobą, czasem lokalnym i znacznikiem „mierzone teraz”).
      test("widok jira tablica", async ({ page }) => {
        await useTheme(page, theme);
        await openDesktop(page, {
          data: seed({ active: { task: ids.analysis, from: "11:00" } }),
          jira: jiraFixture(),
        });
        await nav(page, "Jira");
        await page.getByRole("button", { name: "Tablica", exact: true }).click();
        await expect(page.locator(".jira-card")).toHaveCount(3);
        await settle(page);
        await expect(page).toHaveScreenshot(
          `jira-tablica-${theme}-${width}.png`,
          { fullPage: true },
        );
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

// B13: pasek po „Zakończ dzień” — „Dzień zakończony 16:40 · 3:30” z „Wznów dzień”.
for (const theme of themes)
  for (const width of widths)
    test(`pasek dzień zakończony ${theme} ${width}px`, { tag: "@visual" }, async ({
      page,
    }) => {
      await page.setViewportSize({ width, height: 800 });
      await useTheme(page, theme);
      await openPreview(page, {
        now: "2026-10-15T16:45:00+02:00",
        data: {
          ...seed(),
          dayMarks: [
            {
              day: "2026-10-15",
              startedAt: Date.parse("2026-10-15T08:00:00+02:00"),
              endedAt: Date.parse("2026-10-15T16:40:00+02:00"),
              resumed: 0,
            },
          ],
        },
      });
      await nav(page, "Dashboard");
      const bar = page.locator("header.topbar");
      await expect(
        bar.getByRole("group", { name: "Dzień zakończony w pasku" }),
      ).toBeVisible();
      await settle(page);
      await expect(bar).toHaveScreenshot(
        `pasek-zakonczony-${theme}-${width}.png`,
      );
    });

// B2: Dashboard w średnim oknie (1000 px): karta „Zagrożone teraz” z tytułem
// w jednej linii i liczbami pod nim, gdy brakuje miejsca.
for (const theme of themes)
  test(`dashboard 1000px ${theme}`, { tag: "@visual" }, async ({ page }) => {
    await page.setViewportSize({ width: 1000, height: 800 });
    await useTheme(page, theme);
    await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
    await nav(page, "Dashboard");
    await expect(
      page.getByRole("region", { name: "Zagrożone teraz" }),
    ).toContainText("Analiza importu");
    await settle(page);
    await expect(page).toHaveScreenshot(`dashboard-${theme}-1000.png`, {
      fullPage: true,
    });
  });
