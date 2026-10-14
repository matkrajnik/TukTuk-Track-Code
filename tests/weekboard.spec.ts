import { test, expect, type Page } from "@playwright/test";
test.use({ timezoneId: "Europe/Warsaw" });
const key = "todo-track-preview-v1";

// Środa 14 października 2026; tydzień Pon 12.10 – Nd 18.10.
async function seed(page: Page, tasks: object[], sessions: object[] = []) {
  await page.clock.setFixedTime(new Date("2026-10-14T12:00:00+02:00"));
  await page.goto("/");
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [key, JSON.stringify({ tasks, sessions })],
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
}
const stored = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), key);
const column = (page: Page, name: string) =>
  page.getByRole("region", { name, exact: true });
const capacity = (page: Page, day: string) =>
  page.getByRole("progressbar", { name: "Pojemność: " + day, exact: true });
const cards = (page: Page, name: string) =>
  column(page, name).locator(".week-card-title");

const tasks = [
  {
    id: 1,
    title: "Analiza importu",
    done: false,
    createdAt: 0,
    jiraKey: "WMS-12",
    details: {
      project: "WMS",
      estimateMinutes: 300,
      notes: { "2026-10-13": "Notatka" },
      checklist: [{ text: "Krok", done: true }],
      priority: 1,
    },
  },
  {
    id: 2,
    title: "Wdrożenie",
    done: false,
    createdAt: 0,
    details: { plannedDay: "2026-10-14", estimateMinutes: 240 },
  },
  {
    id: 3,
    title: "Spotkanie",
    done: false,
    createdAt: 0,
    details: { plannedDay: "2026-10-14", estimateMinutes: 60 },
  },
  {
    id: 4,
    title: "Przegląd",
    done: false,
    createdAt: 0,
    details: { plannedDay: "2026-10-15", estimateMinutes: 450 },
  },
  {
    id: 5,
    title: "Stary plan",
    done: false,
    createdAt: 0,
    details: { plannedDay: "2026-10-05", estimateMinutes: 30 },
  },
  {
    id: 6,
    title: "Zrobione",
    done: true,
    createdAt: 0,
    details: { plannedDay: "2026-10-12", estimateMinutes: 60 },
  },
];

test("week board: columns, estimate sums against capacity and the summary tab", async ({
  page,
}) => {
  await seed(page, tasks, [
    {
      id: 1,
      taskId: 2,
      startedAt: Date.parse("2026-10-14T08:00:00+02:00"),
      endedAt: Date.parse("2026-10-14T10:00:00+02:00"),
    },
  ]);
  // Tablica jest domyślna.
  await expect(
    page.getByRole("region", { name: "Tablica tygodnia" }),
  ).toBeVisible();
  await expect(cards(page, "Do zaplanowania")).toHaveText([
    "Analiza importu",
    "Stary plan",
  ]);
  await expect(
    column(page, "Do zaplanowania").getByText("Zaległe (1)"),
  ).toBeVisible();
  await expect(
    column(page, "Do zaplanowania").getByText("WMS-12"),
  ).toBeVisible();
  await expect(cards(page, "Środa 2026-10-14")).toHaveText([
    "Spotkanie",
    "Wdrożenie",
  ]);
  await expect(
    column(page, "Środa 2026-10-14").locator(".week-estimate"),
  ).toHaveText("5:00 / 8:00");
  await expect(
    column(page, "Środa 2026-10-14").locator(".week-actual"),
  ).toHaveText("Praca 2:00");
  // Wdrożenie: 2 h z 4 h szacunku.
  await expect(
    column(page, "Środa 2026-10-14").getByText("4:00 · 50%"),
  ).toBeVisible();
  await expect(capacity(page, "Środa 2026-10-14")).toHaveClass(/\bok\b/);
  // 7:30 z 8:00 = 94% → bursztynowy.
  await expect(capacity(page, "Czwartek 2026-10-15")).toHaveClass(/\bnear\b/);
  await expect(
    column(page, "Sobota 2026-10-17").locator(".week-estimate"),
  ).toHaveText("0:00 / wolne");
  await expect(
    column(page, "Poniedziałek 2026-10-12").getByText("Ukończone"),
  ).toBeVisible();

  // Wyszukiwanie w kolumnie Do zaplanowania.
  await page.getByLabel("Szukaj w Do zaplanowania").fill("wms");
  await expect(cards(page, "Do zaplanowania")).toHaveText(["Analiza importu"]);
  await page.getByLabel("Szukaj w Do zaplanowania").fill("");

  await page.getByRole("button", { name: "Podsumowanie", exact: true }).click();
  await expect(page.getByText("Podsumowanie tygodnia")).toBeVisible();
  await expect(page.locator(".report-total")).toHaveText("02:00:00");
  await page.getByRole("button", { name: "Tablica", exact: true }).click();
  await expect(cards(page, "Środa 2026-10-14")).toHaveCount(2);
});

test("moving a task to Wednesday through the menu updates the sum and flags overload", async ({
  page,
}) => {
  await seed(page, tasks);
  await page
    .getByRole("button", { name: "Przenieś na…: Analiza importu", exact: true })
    .click();
  const menu = page.getByRole("menu", { name: "Przenieś na: Analiza importu" });
  await expect(menu.getByRole("menuitem").first()).toBeFocused();
  await menu
    .getByRole("menuitem", { name: "Środa 2026-10-14", exact: true })
    .click();
  await expect(menu).toHaveCount(0);
  await expect(cards(page, "Środa 2026-10-14")).toHaveText([
    "Analiza importu",
    "Spotkanie",
    "Wdrożenie",
  ]);
  await expect(cards(page, "Do zaplanowania")).toHaveText(["Stary plan"]);
  // 5 h + 4 h + 1 h = 10 h przy pojemności 8 h → czerwony pasek.
  await expect(
    column(page, "Środa 2026-10-14").locator(".week-estimate"),
  ).toHaveText("10:00 / 8:00");
  await expect(capacity(page, "Środa 2026-10-14")).toHaveClass(/\bover\b/);
  await expect(capacity(page, "Środa 2026-10-14")).toHaveAttribute(
    "aria-valuetext",
    "10:00 / 8:00",
  );

  // Zmieniły się tylko plannedDay i order, reszta planu została.
  const moved = (await stored(page)).tasks.find((t: any) => t.id === 1).details;
  expect(moved.plannedDay).toBe("2026-10-14");
  expect(moved.project).toBe("WMS");
  expect(moved.estimateMinutes).toBe(300);
  expect(moved.priority).toBe(1);
  expect(moved.notes["2026-10-13"]).toBe("Notatka");
  expect(moved.checklist[0].done).toBe(true);

  // Plan dnia w zakładce Dzień widzi przeniesienie.
  await page.getByRole("button", { name: "Dzień", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Plan dnia" }).locator(".journal-name"),
  ).toHaveCount(3);
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();

  // Zaległy plan wraca do tego tygodnia, a Wdrożenie idzie na następny poniedziałek.
  await page
    .getByRole("button", { name: "Przenieś na…: Stary plan", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "Piątek 2026-10-16", exact: true })
    .click();
  await expect(cards(page, "Piątek 2026-10-16")).toHaveText(["Stary plan"]);
  await expect(
    column(page, "Do zaplanowania").getByText(/Zaległe/),
  ).toHaveCount(0);
  await page
    .getByRole("button", { name: "Przenieś na…: Wdrożenie", exact: true })
    .click();
  await page
    .getByRole("menuitem", {
      name: "Następny poniedziałek 2026-10-19",
      exact: true,
    })
    .click();
  await expect(cards(page, "Środa 2026-10-14")).toHaveText([
    "Analiza importu",
    "Spotkanie",
  ]);
  await page.getByRole("button", { name: "Następny okres" }).click();
  await expect(cards(page, "Poniedziałek 2026-10-19")).toHaveText([
    "Wdrożenie",
  ]);
  await page.getByRole("button", { name: "Dzisiaj", exact: true }).click();

  // Z powrotem do kolumny Do zaplanowania; po przeładowaniu stan zostaje.
  await page
    .getByRole("button", { name: "Przenieś na…: Spotkanie", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "Do zaplanowania", exact: true })
    .click();
  await expect(cards(page, "Do zaplanowania")).toHaveText(["Spotkanie"]);
  await page.reload();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await expect(cards(page, "Środa 2026-10-14")).toHaveText(["Analiza importu"]);
  await page.setViewportSize({ width: 760, height: 800 });
  await page.screenshot({
    path: "test-results/week-board.png",
    fullPage: true,
  });
});

test("dragging a card by its handle moves it to another day", async ({
  page,
}) => {
  await seed(page, tasks);
  await page
    .getByRole("button", { name: "Przesuń: Spotkanie", exact: true })
    .dragTo(column(page, "Wtorek 2026-10-13"));
  await expect(cards(page, "Wtorek 2026-10-13")).toHaveText(["Spotkanie"]);
  await expect(
    column(page, "Środa 2026-10-14").locator(".week-estimate"),
  ).toHaveText("4:00 / 8:00");
  // Alt+↑ na uchwycie zmienia kolejność w obrębie kolumny.
  await page
    .getByRole("button", { name: "Przesuń: Przegląd", exact: true })
    .dragTo(column(page, "Środa 2026-10-14"));
  await expect(cards(page, "Środa 2026-10-14")).toHaveText([
    "Wdrożenie",
    "Przegląd",
  ]);
  await page
    .getByRole("button", { name: "Przesuń: Przegląd", exact: true })
    .focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(cards(page, "Środa 2026-10-14")).toHaveText([
    "Przegląd",
    "Wdrożenie",
  ]);
});
