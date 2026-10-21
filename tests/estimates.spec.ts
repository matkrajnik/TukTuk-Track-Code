import { test, expect, type Page } from "./fixtures/test";
test.use({ timezoneId: "Europe/Warsaw" });
const key = "todo-track-preview-v1";

// Środa 14 października 2026, 12:00. Okres 30 dni zaczyna się 15.09.
const now = "2026-10-14T12:00:00+02:00";
const task = (
  id: number,
  title: string,
  details: object,
  extra: object = { done: true },
) => ({ id, title, done: false, createdAt: 0, details, ...extra });
const tasks = [
  // Ukończone, w okresie 30 dni.
  task(1, "Import", { project: "WMS", estimateMinutes: 60 }), // 50%
  task(2, "Raport", { project: "WMS", estimateMinutes: 60 }), // 100%
  task(3, "Etykiety", { project: "WMS", estimateMinutes: 60 }), // 120%
  task(4, "Portal klienta", { project: "Portal", estimateMinutes: 60 }), // 150%
  task(5, "Migracja", { estimateMinutes: 60 }, { done: true, archived: true }), // 250%
  // Tylko w 90 dniach i we „Wszystko”.
  task(6, "Logowanie", { project: "Portal", estimateMinutes: 120 }), // 25%
  // Tylko we „Wszystko”.
  task(7, "Stary projekt", { project: "WMS", estimateMinutes: 60 }), // 1000%
  // Poza zbiorem: brak szacunku, brak czasu.
  task(8, "Bez szacunku", {}),
  task(9, "Bez czasu", { estimateMinutes: 60 }),
  // Otwarte: przekroczone (zagrożone), w normie, zarchiwizowane.
  task(10, "Wdrożenie", { estimateMinutes: 60 }, {}),
  task(11, "Przegląd", { estimateMinutes: 60 }, {}),
  task(12, "Wstrzymane", { estimateMinutes: 60 }, { archived: true }),
];
let sid = 0;
const span = (taskId: number, start: string, end: string | null) => ({
  id: ++sid,
  taskId,
  startedAt: Date.parse(start),
  endedAt: end === null ? null : Date.parse(end),
});
const sessions = [
  span(1, "2026-10-13T09:00:00+02:00", "2026-10-13T09:30:00+02:00"),
  span(2, "2026-10-12T09:00:00+02:00", "2026-10-12T10:00:00+02:00"),
  span(3, "2026-10-09T09:00:00+02:00", "2026-10-09T10:00:00+02:00"),
  span(3, "2026-10-10T09:00:00+02:00", "2026-10-10T09:12:00+02:00"),
  span(4, "2026-10-01T09:00:00+02:00", "2026-10-01T10:30:00+02:00"),
  span(5, "2026-10-05T09:00:00+02:00", "2026-10-05T11:30:00+02:00"),
  span(6, "2026-08-01T09:00:00+02:00", "2026-08-01T09:30:00+02:00"),
  span(7, "2026-01-10T08:00:00+01:00", "2026-01-10T18:00:00+01:00"),
  span(8, "2026-10-13T11:00:00+02:00", "2026-10-13T12:00:00+02:00"),
  span(10, "2026-10-13T13:00:00+02:00", "2026-10-13T13:30:00+02:00"),
  span(10, "2026-10-14T11:00:00+02:00", null), // trwa: 30 + 60 = 90 min
  span(11, "2026-10-14T08:00:00+02:00", "2026-10-14T08:30:00+02:00"),
  span(12, "2026-10-14T06:00:00+02:00", "2026-10-14T08:00:00+02:00"),
];

async function seed(page: Page) {
  await page.clock.setFixedTime(new Date(now));
  await page.goto("/");
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [key, JSON.stringify({ tasks, sessions })],
  );
  await page.reload();
}
const card = (page: Page) =>
  page.getByRole("region", { name: "Trafność szacunków", exact: true });
const stats = (page: Page) => card(page).getByRole("definition");
const bucketLabels = (counts: number[]) =>
  ["<50%", "50–80%", "80–120%", "120–150%", "150–200%", ">200%"].map(
    (label, i) => `Koszyk ${label}: ${counts[i]}`,
  );
async function expectBuckets(page: Page, counts: number[]) {
  const items = card(page)
    .getByRole("list", { name: "Histogram trafności" })
    .getByRole("listitem");
  await expect(items).toHaveCount(6);
  for (const [i, label] of bucketLabels(counts).entries())
    await expect(items.nth(i)).toHaveAttribute("aria-label", label);
}

test("estimate accuracy card computes indicators, histogram and misses", async ({
  page,
}) => {
  await seed(page);
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();

  // 30 dni: 50%, 100%, 120%, 150%, 250% (zarchiwizowane ukończone też się liczy).
  await expect(
    card(page).getByRole("button", { name: "30 dni", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(stats(page)).toHaveText(["5", "120%", "40%", "40%"]);
  await expectBuckets(page, [0, 1, 2, 1, 0, 1]);
  await expect(
    card(page)
      .getByRole("table", { name: "Mediana według projektu" })
      .locator("tbody tr"),
  ).toHaveText(["Portal1150%", "WMS3100%", "Bez projektu1250%"]);
  const misses = card(page)
    .getByRole("list", { name: "Największe rozjazdy" })
    .locator(".estimate-miss-title");
  await expect(misses).toHaveText([
    "Migracja",
    "Portal klienta",
    "Import",
    "Etykiety",
    "Raport",
  ]);

  // 90 dni dokłada 25% z sierpnia.
  await card(page).getByRole("button", { name: "90 dni", exact: true }).click();
  await expect(stats(page)).toHaveText(["6", "110%", "33%", "33%"]);
  await expectBuckets(page, [1, 1, 2, 1, 0, 1]);

  // Wszystko dokłada 1000% ze stycznia.
  await card(page)
    .getByRole("button", { name: "Wszystko", exact: true })
    .click();
  await expect(stats(page)).toHaveText(["7", "120%", "29%", "43%"]);
  await expectBuckets(page, [1, 1, 2, 1, 0, 2]);
  await expect(misses.first()).toHaveText("Stary projekt");

  // Zagrożone teraz: tylko otwarte, niezarchiwizowane, po przekroczeniu.
  const risky = page.getByRole("region", { name: "Zagrożone teraz" });
  await expect(risky.locator(".dashboard-count > span")).toHaveText([
    "Wdrożenie",
  ]);
  await expect(risky.locator(".estimate-over")).toHaveText("150%");

  await page.screenshot({
    path: "test-results/estimates-dashboard.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);

  // Link rozjazdu otwiera raport z dnia ostatniej pracy.
  await card(page)
    .getByRole("button", { name: "Raport 2026-10-10: Etykiety", exact: true })
    .click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-10-10");
});

test("empty period shows a note and the task tile warns above 100%", async ({
  page,
}) => {
  await seed(page);
  // Kafelek: 90 z 60 min = 150% w kolorze ostrzegawczym, 30 z 60 = 50% bez.
  const over = page
    .locator(".task-card")
    .filter({ hasText: "Wdrożenie" })
    .locator(".estimate-over");
  await expect(over).toHaveText("150%");
  await expect(
    page
      .locator(".task-card")
      .filter({ hasText: "Przegląd" })
      .locator(".estimate-over"),
  ).toHaveCount(0);
  const color = await over.evaluate((el) => getComputedStyle(el).color);
  const warning = await page.evaluate(() =>
    getComputedStyle(document.documentElement)
      .getPropertyValue("--warning")
      .trim(),
  );
  const probe = await page.evaluate((value) => {
    const el = document.createElement("span");
    el.style.color = value;
    document.body.append(el);
    const rgb = getComputedStyle(el).color;
    el.remove();
    return rgb;
  }, warning);
  expect(color).toBe(probe);

  // Bez danych w okresie: wskaźniki z kreską i komunikat.
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [key, JSON.stringify({ tasks: [tasks[9]], sessions: [] })],
  );
  await page.reload();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(stats(page)).toHaveText(["0", "—", "—", "—"]);
  await expect(card(page)).toContainText(
    "Brak ukończonych zadań z szacunkiem w tym okresie.",
  );
  await expect(
    page.getByRole("region", { name: "Zagrożone teraz" }),
  ).toContainText("Żadne otwarte zadanie nie przekroczyło szacunku.");
});
