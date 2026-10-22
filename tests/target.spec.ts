import { test, expect, type Page } from "./fixtures/test";
import { openSettings } from "./fixtures/settings";
const key = "tuktuk-track-preview-v1";
const settingsKey = "tuktuk-track-preview-settings-v1";

// Środa 14 października 2026, 12:00 czasu lokalnego.
test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-14T12:00:00+02:00"));
  await page.goto("/");
});

async function seed(
  page: Page,
  sessions: [string, string][],
  settings?: Record<string, unknown>,
) {
  await page.evaluate(
    ([k, sk, sessions, settings]) => {
      localStorage.setItem(
        k as string,
        JSON.stringify({
          tasks: [
            {
              id: 1,
              title: "Import danych",
              done: false,
              createdAt: 0,
              details: { plannedDay: "2026-10-14" },
            },
          ],
          sessions: (sessions as [string, string][]).map(([a, b], i) => ({
            id: i + 1,
            taskId: 1,
            startedAt: +new Date(a),
            endedAt: +new Date(b),
          })),
        }),
      );
      if (settings) localStorage.setItem(sk as string, JSON.stringify(settings));
    },
    [key, settingsKey, sessions, settings ?? null] as const,
  );
  await page.reload();
}

test("tasks show progress to the daily target and turn amber after exceeding it", async ({
  page,
}) => {
  await seed(page, [["2026-10-14T08:00:00", "2026-10-14T11:12:00"]]);
  await expect(page.locator(".daily-summary strong")).toHaveText("03:12:00");
  await expect(page.locator(".target-text")).toHaveText("03:12 / 08:00");
  const bar = page.getByRole("progressbar", { name: "Postęp do celu dnia" });
  await expect(bar).toHaveAttribute("aria-valuenow", "192");
  await expect(bar).toHaveAttribute("aria-valuemax", "480");
  await expect(bar).not.toHaveClass(/over/);
  await expect(bar.locator("span")).toHaveAttribute("style", /width: 40%/);
  const color = () =>
    bar.locator("span").evaluate((el) => getComputedStyle(el).backgroundColor);
  const accent = await color();

  // Dashboard: dzisiaj i bieżący tydzień względem celu (5 × 8 h).
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(
    page.getByRole("progressbar", { name: "Ten tydzień względem celu" }),
  ).toHaveAttribute("aria-valuetext", "03:12 / 40:00");
  await expect(page.locator(".dashboard-day .target-line")).toHaveCount(5);

  // Tydzień w Plan i historia: różnica względem celu przy każdym dniu.
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await page.getByRole("button", { name: "Podsumowanie", exact: true }).click();
  const diffs = page.locator(".weekly-day .target-diff");
  await expect(diffs).toHaveText([
    "−8:00",
    "−8:00",
    "−4:48",
    "−8:00",
    "−8:00",
    "wolne",
    "wolne",
  ]);
  await expect(page.getByText("Cel tygodnia 40:00 · −36:48")).toBeVisible();

  // Niższy cel w ustawieniach od razu zmienia pasek na bursztynowy.
  await openSettings(page, "Czas pracy");
  const target = page.getByLabel("Cel dzienny (minuty)");
  await target.fill("120");
  await target.press("Tab");
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(page.locator(".target-text")).toHaveText("03:12 / 02:00");
  await expect(bar).toHaveClass(/over/);
  await expect(bar.locator("span")).toHaveAttribute("style", /width: 100%/);
  expect(await color()).not.toBe(accent);

  // Dzień wolny: bez paska.
  await page.evaluate(
    (sk) =>
      localStorage.setItem(sk, JSON.stringify({ workDays: [1, 2, 4, 5] })),
    settingsKey,
  );
  await page.reload();
  await expect(page.locator(".daily-summary strong")).toHaveText("03:12:00");
  await expect(page.locator(".target-text")).toHaveCount(0);
});

test("day closing tells how much is missing or over the target without blocking", async ({
  page,
}) => {
  await seed(page, [
    ["2026-10-14T08:00:00", "2026-10-14T09:00:00"],
    ["2026-10-14T10:00:00", "2026-10-14T10:30:00"],
  ]);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknięcie dnia", exact: true })
    .click();
  await expect(page.getByText("Do celu brakuje 6:30")).toBeVisible();
  const close = page.getByRole("button", {
    name: "Oznacz dzień jako sprawdzony",
    exact: true,
  });
  await expect(close).toBeEnabled();

  await seed(
    page,
    [
      ["2026-10-14T08:00:00", "2026-10-14T09:00:00"],
      ["2026-10-14T10:00:00", "2026-10-14T10:40:00"],
    ],
    { targetDayMinutes: 60 },
  );
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknięcie dnia", exact: true })
    .click();
  const message = page.getByText("Cel przekroczony o 0:40");
  await expect(message).toBeVisible();
  await expect(message).toHaveClass(/over/);
  await close.click();
  await expect(
    page.getByRole("button", { name: "Otwórz dzień ponownie" }),
  ).toBeVisible();

  // Sobota nie jest dniem roboczym: brak komunikatu.
  await page.getByLabel("Dzień planu").fill("2026-10-17");
  await page.getByLabel("Dzień planu").dispatchEvent("change");
  await expect(page.getByRole("heading", { name: /sob., 17 paź/ })).toBeVisible();
  await expect(page.getByText(/Do celu brakuje|Cel przekroczony/)).toHaveCount(
    0,
  );
});
