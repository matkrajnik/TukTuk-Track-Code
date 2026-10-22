import { test, expect, type Page } from "./fixtures/test";
import {
  SETTINGS_KEY,
  TODAY,
  agentSessions,
  calendarFixture,
  at,
  openPreview,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * W2: rozproszenie i przegląd tygodnia. Dane z `fixtures/seed.ts`: dziś czwartek
 * 15.10.2026 12:00; dziś „Spotkanie zespołu” 08:00–08:30, potem „Analiza
 * importu” 08:30–10:30 (jedno przełączenie), w środę tak samo 09:00–09:30
 * i 09:30–15:30, w poniedziałek i wtorek zmiany zadań po 30 min przerwy.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const FOCUS_KEY = "tuktuk-track-preview-focus-v1";
const WEEK_NOTES_KEY = "tuktuk-track-preview-week-notes-v1";

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

/** Bloki skupienia w podglądzie: dziś jeden ukończony i jeden przerwany, w środę ukończony. */
async function seedFocus(page: Page) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [
      FOCUS_KEY,
      JSON.stringify({
        blocks: [
          {
            id: 1,
            taskId: 1,
            start: at("2026-10-14", "10:00"),
            end: at("2026-10-14", "10:25"),
            completed: true,
          },
          {
            id: 2,
            taskId: 1,
            start: at(TODAY, "08:35"),
            end: at(TODAY, "09:00"),
            completed: true,
          },
          {
            id: 3,
            taskId: 1,
            start: at(TODAY, "09:10"),
            end: at(TODAY, "09:20"),
            completed: false,
          },
        ],
        ended: null,
        pause: null,
        resume: null,
      }),
    ] as const,
  );
  await page.reload();
}

test("wskaźniki rozproszenia: przerwa, północ, powrót do tego samego zadania (moduł)", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(
    async ({ day, next, t }) => {
      const m = await import("/src/report.ts");
      const s = (id: number, taskId: number, from: number, to: number | null) => ({
        id,
        taskId,
        startedAt: from,
        endedAt: to,
      });
      const breaks = [
        s(1, 1, t.a0900, t.a1000),
        // Inne zadanie 10 min po pomiarze: przełączenie.
        s(2, 2, t.a1010, t.a1040),
        // Powrót po 20 min przerwy: nie jest przełączeniem.
        s(3, 1, t.a1100, t.a1130),
        // To samo zadanie po 10 min przerwy: nie przełączenie, osobny pomiar.
        s(4, 1, t.a1140, t.a1200),
        // 30 s przerwy w tym samym zadaniu: ten sam nieprzerwany pomiar.
        s(5, 1, t.a1200b, t.a1230),
        // Inne zadanie dokładnie 15 min po końcu: przełączenie; 15 min 1 s już nie.
        s(6, 3, t.a1245, t.a1300),
        s(7, 2, t.a1316, t.a1330),
      ];
      const midnight = [
        s(10, 1, t.n2300, t.n2355),
        // Start innego zadania 10 min po końcu, ale już następnego dnia.
        s(11, 2, t.n0005, t.n0100),
        // Trwający pomiar przez północ (do `now`).
        s(12, 3, t.n2330, null),
      ];
      const blocks = [
        { start: t.a0900, end: t.a1000, completed: true },
        { start: t.a1100, end: t.a1130, completed: false },
        { start: t.n2300, end: t.n2355, completed: null },
      ];
      return {
        breaks: m.dayDistraction(breaks, day, t.a1400, blocks.slice(0, 2)),
        // Przełączenie po północy należy do nowego dnia.
        before: m.dayDistraction(midnight, day, t.now, blocks.slice(2)),
        after: m.dayDistraction(midnight, next, t.now, blocks.slice(2)),
        runs: m.continuousRuns(breaks, t.a1400).length,
        average: m.averageDistraction(
          [...breaks, ...midnight],
          [day, next, "2026-10-12"],
          t.now,
          blocks,
        ),
        decimal: [m.oneDecimal(1 / 3), m.oneDecimal(2), m.oneDecimal(1.25)],
      };
    },
    {
      day: "2026-10-14",
      next: TODAY,
      t: {
        a0900: at("2026-10-14", "09:00"),
        a1000: at("2026-10-14", "10:00"),
        a1010: at("2026-10-14", "10:10"),
        a1040: at("2026-10-14", "10:40"),
        a1100: at("2026-10-14", "11:00"),
        a1130: at("2026-10-14", "11:30"),
        a1140: at("2026-10-14", "11:40"),
        a1200: at("2026-10-14", "12:00"),
        a1200b: at("2026-10-14", "12:00") + 30_000,
        a1230: at("2026-10-14", "12:30"),
        a1245: at("2026-10-14", "12:45"),
        a1300: at("2026-10-14", "13:00"),
        a1316: at("2026-10-14", "13:15") + 1000,
        a1330: at("2026-10-14", "13:30"),
        a1400: at("2026-10-14", "14:00"),
        n2300: at("2026-10-14", "23:00"),
        n2355: at("2026-10-14", "23:55"),
        n0005: at(TODAY, "00:05"),
        n0100: at(TODAY, "01:00"),
        n2330: at(TODAY, "23:30"),
        now: at("2026-10-16", "00:30"),
      },
    },
  );
  const min = 60_000;
  expect(result.breaks).toEqual({
    switches: 2,
    // 60, 30, 30, 50 (11:40–12:30 mimo przerwy 30 s), 15 i niecałe 15 min.
    measurements: 6,
    medianMs: 30 * min,
    longestMs: 60 * min,
    focusStarted: 2,
    focusDone: 1,
  });
  expect(result.runs).toBe(6);
  expect(result.before.switches).toBe(0);
  expect(result.before.measurements).toBe(1);
  expect(result.before.longestMs).toBe(55 * min);
  expect(result.before.focusStarted).toBe(1);
  expect(result.before.focusDone).toBe(0);
  expect(result.after.switches).toBe(1);
  // 00:05–01:00 i część trwającego pomiaru do północy (23:30–24:00).
  expect(result.after.measurements).toBe(2);
  expect(result.after.longestMs).toBe(55 * min);
  expect(result.after.medianMs).toBe(42.5 * min);
  // Średnia tylko z dni z pomiarem (poniedziałek bez pracy pominięty).
  expect(result.average.days).toBe(2);
  expect(result.average.switches).toBe(1.5);
  expect(result.decimal).toEqual(["0,3", "2", "1,3"]);
});

test("karta Rozproszenie na Dashboardzie: dziś i średnia z 7 dni roboczych", async ({
  page,
}) => {
  await openPreview(page);
  await seedFocus(page);
  await nav(page, "Dashboard");
  const card = page.getByRole("region", { name: "Rozproszenie" });
  await expect(card).toBeVisible();
  const row = (name: string) =>
    card.getByRole("row", { name: new RegExp("^" + name) });
  // Dziś: Spotkanie zespołu → Analiza importu bez przerwy; pomiary 0:30 i 2:00.
  // Średnio z pon.–śr. (wcześniejsze dni robocze bez pracy są pomijane).
  await expect(row("Przełączenia")).toHaveText(/Przełączenia\s*1\s*0,3/);
  await expect(row("Mediana pomiaru")).toHaveText(/01:15\s*03:45/);
  await expect(row("Najdłuższy pomiar")).toHaveText(/02:00\s*04:40/);
  await expect(row("Bloki skupienia")).toHaveText(/1 \/ 2\s*0,3 \/ 0,3/);
});

test("Plan i historia → Tydzień → Przegląd: czas, plan, szacunki, agenci, rozproszenie i wnioski", async ({
  page,
}) => {
  await openPreview(page, { agentSessions: agentSessions() });
  await seedFocus(page);
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Przegląd");
  await expect(
    page.getByRole("button", { name: "Przegląd", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");

  const time = page.getByRole("region", { name: "Czas kontra cel i spotkania" });
  await expect(time).toContainText(
    "Zmierzone 23:30 · cel 40:00 · spotkania 00:00 · zostawało na pracę 40:00",
  );
  await expect(time.getByRole("row")).toHaveCount(6);

  const plan = page.getByRole("region", { name: "Plan kontra wykonanie" });
  await expect(plan.locator("dd")).toHaveText(["3", "1", "0", "2"]);
  await expect(plan).toContainText("Wdrożenie etykiet");

  const misses = page.getByRole("region", { name: "Rozjazdy szacunków" });
  // Otwarte zadania tylko po przekroczeniu szacunku: „Analiza importu” (110%)
  // zostaje, „Wdrożenie etykiet” (otwarte, poniżej szacunku) nie.
  await expect(misses.locator("tbody tr")).toHaveCount(3);
  await expect(misses.locator("tbody tr").first()).toHaveText(
    /Analiza importu\s*10:00\s*11:00\s*110%/,
  );
  await expect(misses).not.toContainText("Wdrożenie etykiet");

  const agents = page.getByRole("region", { name: "Czas agentów w tygodniu" });
  await expect(agents).toContainText("Razem 03:55");
  await expect(agents).toContainText("Bez zadania");

  const spread = page.getByRole("region", { name: "Rozproszenie dzień po dniu" });
  // Pon–Pt (dni robocze); weekend bez pracy pominięty.
  await expect(spread.locator("tbody tr")).toHaveCount(5);
  await expect(spread.locator("tbody tr").nth(2)).toHaveText(
    /1\s*03:15\s*06:00\s*1 \/ 1/,
  );
  await expect(spread.locator("tbody tr").nth(3)).toHaveText(
    /1\s*01:15\s*02:00\s*1 \/ 2/,
  );

  // Wnioski: zapis do week_notes (w podglądzie localStorage), po przeładowaniu zostają.
  const notes = page.getByLabel("Wnioski z tygodnia");
  const save = page.getByRole("button", { name: "Zapisz wnioski" });
  await expect(save).toBeDisabled();
  await notes.fill("  Za dużo przełączeń w czwartek.  ");
  await save.click();
  await expect(
    page.getByRole("region", { name: "Wnioski" }).getByRole("status"),
  ).toHaveText("Zapisano");
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), WEEK_NOTES_KEY),
  ).toEqual({ "2026-10-12": "Za dużo przełączeń w czwartek." });
  await page.reload();
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Przegląd");
  await expect(notes).toHaveValue("Za dużo przełączeń w czwartek.");
  // Następny tydzień ma własne (puste) wnioski.
  await page.getByRole("button", { name: "Następny okres" }).click();
  await expect(notes).toHaveValue("");
  await page.getByRole("button", { name: "Poprzedni okres" }).click();
  await expect(notes).toHaveValue("Za dużo przełączeń w czwartek.");
});

test("powiadomienie „Przegląd tygodnia” otwiera zakładkę, a ustawienia mają wyłącznik i godzinę", async ({
  page,
}) => {
  await openPreview(page);
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("tuktuk-track-navigate", {
        detail: { view: "journal", section: "week-review" },
      }),
    ),
  );
  await expect(page.getByRole("heading", { name: "Przegląd tygodnia" })).toBeVisible();
  await expect(page.getByRole("region", { name: "Wnioski" })).toBeVisible();

  await openSettings(page, "Powiadomienia");
  const toggle = page.getByLabel("Przypominaj o przeglądzie tygodnia");
  const hour = page.getByLabel("Przegląd tygodnia o godzinie");
  await expect(toggle).toBeChecked();
  await expect(hour).toHaveValue("15:00");
  await hour.fill("16:30");
  await hour.blur();
  await expect(hour).toHaveValue("16:30");
  await toggle.uncheck();
  await expect(hour).toBeDisabled();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
  ).toEqual({ weekReviewTime: "16:30", notifyWeekReview: false });
});

test("przegląd tygodnia odejmuje spotkania z kalendarza i pokazuje dzień poza biurem", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Przegląd");
  const time = page.getByRole("region", { name: "Czas kontra cel i spotkania" });
  // Daily w poniedziałek i dziś (po 15 min) i „wstępnie” przyjęty przegląd (1 h);
  // anulowane i odrzucone nie zajmują czasu, piątek to całodniowy urlop.
  await expect(time).toContainText(
    "Zmierzone 23:30 · cel 40:00 · spotkania 01:30 · zostawało na pracę 30:30",
  );
  await expect(time.locator("tbody tr").nth(4)).toContainText("poza biurem");
});
