import { test, expect, type Page } from "@playwright/test";
test.use({ timezoneId: "Europe/Warsaw" });
const key = "todo-track-preview-v1";
const at = (time: string) => new Date(`2026-10-14T${time}:00+02:00`).getTime();
const planned = (extra: object = {}) => ({
  plannedDay: "2026-10-14",
  ...extra,
});

// Środa 14 października 2026, 12:00 czasu lokalnego.
async function seed(page: Page, tasks: object[], sessions: object[]) {
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
  return page.getByRole("region", { name: "Plan kontra rzeczywistość" });
}
const rowTexts = (table: ReturnType<Page["locator"]>) =>
  table
    .locator("tbody tr, tfoot tr")
    .evaluateAll((rows) =>
      rows.map((r) =>
        [...r.children].map((c) => c.textContent!.trim()).join(" | "),
      ),
    );

test("day timeline compares plan blocks with work and lists differences", async ({
  page,
}) => {
  const card = await seed(
    page,
    [
      {
        id: 2,
        title: "Wdrożenie",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 90, order: 2 }),
      },
      {
        id: 1,
        title: "Analiza",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 60, order: 1 }),
      },
    ],
    [{ id: 1, taskId: 1, startedAt: at("08:15"), endedAt: at("09:30") }],
  );
  await expect(card.locator(".dtl-plan")).toHaveCount(2);
  await expect(card.locator(".dtl-work")).toHaveCount(1);
  await expect(card.locator(".dtl-live")).toHaveCount(0);
  // Bloki planu kolejno od 08:00 w ręcznej kolejności.
  await expect(
    card.getByRole("button", { name: "Plan: Analiza · 08:00–09:00" }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", { name: "Plan: Wdrożenie · 09:00–10:30" }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", { name: "Praca: Analiza · 08:15–09:30" }),
  ).toBeVisible();
  // Oś od 08:00 do 17:00.
  const labels = card.locator(".dtl-grid text");
  await expect(labels.first()).toHaveText("08:00");
  await expect(labels.last()).toHaveText("17:00");
  await expect(card.locator(".dtl-legend li")).toHaveText([
    "Analiza",
    "Wdrożenie",
  ]);
  const table = card.getByRole("table", { name: "Plan i rzeczywistość" });
  expect(await rowTexts(table)).toEqual([
    "Analiza | 01:00 | 01:15 | +0:15",
    "Wdrożenie | 01:30 | niezaczęte | −1:30",
    "Razem | 02:30 | 01:15 | −1:15",
  ]);
  await expect(
    table.locator("tbody tr").first().locator("td").last(),
  ).toHaveClass(/dtl-over/);
  await expect(
    table.locator("tbody tr").nth(1).locator("td").last(),
  ).not.toHaveClass(/dtl-over/);
  // Kolory z palety tokenów, ten sam dla bloku planu i pracy zadania.
  const fill = (name: string) =>
    card
      .getByRole("button", { name })
      .evaluate((el) => getComputedStyle(el).fill);
  expect(await fill("Praca: Analiza · 08:15–09:30")).toBe(
    await fill("Plan: Analiza · 08:00–09:00"),
  );

  // Hover pokazuje tytuł i godziny.
  await card
    .getByRole("button", { name: "Plan: Wdrożenie · 09:00–10:30" })
    .hover();
  await expect(card.getByRole("tooltip")).toHaveText(
    "Plan: Wdrożenie · 09:00–10:30",
  );
  await page.setViewportSize({ width: 760, height: 900 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await card.scrollIntoViewIfNeeded();
  await page.screenshot({
    path: "test-results/day-timeline.png",
    fullPage: true,
  });

  // Klik w blok pracy otwiera edycję przedziału.
  await card
    .getByRole("button", { name: "Praca: Analiza · 08:15–09:30" })
    .click();
  const dialog = page.getByRole("dialog", { name: "Edytuj przedział" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Anuluj", exact: true }).click();
  await expect(dialog).toHaveCount(0);

  // Klik w blok planu rozwija plan zadania; własna godzina startu przypina blok.
  await card
    .getByRole("button", { name: "Plan: Wdrożenie · 09:00–10:30" })
    .click();
  const start = page.getByLabel("Godzina startu w planie", { exact: true });
  await expect(start).toBeVisible();
  await start.fill("13:00");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByRole("status")).toHaveText("Zapisano plan i notatki.");
  await expect(
    card.getByRole("button", { name: "Plan: Wdrożenie · 13:00–14:30" }),
  ).toBeVisible();
  await page.reload();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await expect(
    card.getByRole("button", { name: "Plan: Wdrożenie · 13:00–14:30" }),
  ).toBeVisible();
  const stored = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    key,
  );
  expect(
    stored.tasks.find((t: { id: number }) => t.id === 2).details.plannedStart,
  ).toBe("13:00");
});

test("pinned start, missing estimate, unplanned and running work", async ({
  page,
}) => {
  const card = await seed(
    page,
    [
      {
        id: 1,
        title: "Przegląd",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 45, order: 1 }),
      },
      {
        id: 2,
        title: "Stand-up",
        done: false,
        createdAt: 0,
        details: planned({ order: 2, plannedStart: "09:30" }),
      },
      {
        id: 3,
        title: "Raport",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 30, order: 3 }),
      },
      { id: 4, title: "Telefon", done: false, createdAt: 0 },
    ],
    [
      { id: 1, taskId: 1, startedAt: at("07:30"), endedAt: at("08:30") },
      { id: 2, taskId: 4, startedAt: at("11:15"), endedAt: null },
    ],
  );
  // Przypięty blok bez szacunku: 30 min od 09:30, kolejny układa się za nim.
  await expect(
    card.getByRole("button", { name: "Plan: Przegląd · 08:00–08:45" }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", {
      name: "Plan: Stand-up · 09:30–10:00 · bez szacunku",
    }),
  ).toHaveClass(/dashed/);
  await expect(
    card.getByRole("button", { name: "Plan: Raport · 10:00–10:30" }),
  ).toBeVisible();
  await expect(card.locator(".dtl-plan.dashed")).toHaveCount(1);
  // Trwający pomiar ma animowaną krawędź.
  await expect(card.locator(".dtl-work")).toHaveCount(2);
  await expect(card.locator(".dtl-live")).toHaveCount(1);
  await expect(
    card.getByRole("button", { name: "Praca: Telefon · 11:15–trwa" }),
  ).toBeVisible();
  // Oś zaczyna się od pierwszego pomiaru (07:30 → 07:00).
  await expect(card.locator(".dtl-grid text").first()).toHaveText("07:00");
  expect(
    await rowTexts(card.getByRole("table", { name: "Plan i rzeczywistość" })),
  ).toEqual([
    "Przegląd | 00:45 | 01:00 | +0:15",
    "Stand-up | bez szacunku | niezaczęte | —",
    "Raport | 00:30 | niezaczęte | −0:30",
    "Telefon | nieplanowane | 00:45 | —",
    // Różnica „Razem” tylko z wierszy z szacunkiem; cała praca osobno.
    "Razem | 01:15 | 01:00 · łącznie 01:45 | −0:15",
  ]);
  await expect(card.locator("tfoot td").last()).not.toHaveClass(/dtl-over/);
  await page.setViewportSize({ width: 760, height: 900 });
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await card.screenshot({ path: "test-results/day-timeline-light.png" });
  // Klik w trwający blok otwiera edycję aktywnego przedziału.
  await card
    .getByRole("button", { name: "Praca: Telefon · 11:15–trwa" })
    .click();
  await expect(
    page.getByRole("dialog", { name: "Edytuj przedział" }),
  ).toBeVisible();
});

test("plan blocks follow the plan list order with done tasks last", async ({
  page,
}) => {
  const card = await seed(
    page,
    [
      {
        id: 1,
        title: "Analiza",
        done: true,
        createdAt: 0,
        details: planned({ estimateMinutes: 60, order: 1 }),
      },
      {
        id: 2,
        title: "Wdrożenie",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 30, order: 2 }),
      },
      {
        id: 3,
        title: "Raport",
        done: false,
        createdAt: 0,
        details: planned({ estimateMinutes: 30, order: 3 }),
      },
    ],
    [
      { id: 1, taskId: 1, startedAt: at("08:00"), endedAt: at("08:30") },
      { id: 2, taskId: 2, startedAt: at("09:00"), endedAt: at("11:00") },
    ],
  );
  // Zakończona Analiza stoi na liście planu pod niezakończonymi — tak samo na osi.
  await expect(
    card.getByRole("button", { name: "Plan: Wdrożenie · 08:00–08:30" }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", { name: "Plan: Raport · 08:30–09:00" }),
  ).toBeVisible();
  await expect(
    card.getByRole("button", { name: "Plan: Analiza · 09:00–10:00" }),
  ).toHaveClass(/done/);
  const blocks = await card
    .locator(".dtl-plan")
    .evaluateAll((els) => els.map((e) => e.getAttribute("aria-label")));
  expect(blocks.map((l) => l!.split(" · ")[0])).toEqual([
    "Plan: Wdrożenie",
    "Plan: Raport",
    "Plan: Analiza",
  ]);
  const listOrder = await page
    .locator("[id^='plan-task-']")
    .evaluateAll((els) =>
      els.map((e) => Number(e.id.slice("plan-task-".length))),
    );
  expect(listOrder).toEqual([2, 3, 1]);
});

test("day timeline helpers: palette and DST hours", async ({ page }) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const m = await import("/src/dayplan.ts");
    const [from] = [new Date(2026, 9, 25).getTime()];
    const ticks = m
      .hourTicks(from, new Date(2026, 9, 25, 4).getTime())
      .map((t: number) => new Date(t).getHours());
    const colors = Array.from({ length: 64 }, (_, i) => m.colorIndex(i + 1));
    const styles = getComputedStyle(document.documentElement);
    const tokens = Array.from({ length: 8 }, (_, i) =>
      styles.getPropertyValue(`--tl-${i}`).trim(),
    );
    return { ticks, colors, tokens, again: m.colorIndex(17) };
  });
  // 25 października 2026: godzina 02:00 występuje dwa razy.
  expect(result.ticks).toEqual([0, 1, 2, 2, 3, 4]);
  expect(new Set(result.colors).size).toBe(8);
  expect(result.colors[16]).toBe(result.again);
  expect(result.tokens.every((c) => /^#[0-9a-f]{6}$/.test(c))).toBe(true);
  expect(new Set(result.tokens).size).toBe(8);
});
