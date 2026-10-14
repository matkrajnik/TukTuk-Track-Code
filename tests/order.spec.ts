import { test, expect, type Page } from "@playwright/test";
test.use({ timezoneId: "Europe/Warsaw" });
const key = "todo-track-preview-v1";

async function seed(page: Page, tasks: object[]) {
  await page.clock.setFixedTime(new Date("2026-10-14T17:00:00+02:00"));
  await page.goto("/");
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [key, JSON.stringify({ tasks, sessions: [] })],
  );
  await page.reload();
}
const titles = (page: Page) => page.locator(".task-card .task-title");
const stored = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), key);

test("Alt+arrows on the handle reorder tasks, keep focus and survive reload", async ({
  page,
}) => {
  await seed(page, [
    { id: 3, title: "Trzecie", done: false, createdAt: 0 },
    {
      id: 2,
      title: "Drugie",
      done: false,
      createdAt: 0,
      details: {
        project: "WMS",
        estimateMinutes: 45,
        notes: { "2026-10-14": "Notatka" },
      },
    },
    { id: 1, title: "Pierwsze", done: false, createdAt: 0 },
  ]);
  await expect(titles(page)).toHaveText(["Trzecie", "Drugie", "Pierwsze"]);
  const handle = page.getByRole("button", {
    name: "Przesuń: Trzecie",
    exact: true,
  });
  await handle.focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(titles(page)).toHaveText(["Drugie", "Trzecie", "Pierwsze"]);
  await expect(handle).toBeFocused();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(titles(page)).toHaveText(["Drugie", "Pierwsze", "Trzecie"]);
  // Na końcu listy nic się nie zmienia.
  await page.keyboard.press("Alt+ArrowDown");
  await expect(titles(page)).toHaveText(["Drugie", "Pierwsze", "Trzecie"]);
  await page.reload();
  await expect(titles(page)).toHaveText(["Drugie", "Pierwsze", "Trzecie"]);
  const data = await stored(page);
  const drugie = data.tasks.find((t: any) => t.id === 2).details;
  expect(drugie.order).toBe(1);
  expect(drugie.project).toBe("WMS");
  expect(drugie.estimateMinutes).toBe(45);
  expect(drugie.notes["2026-10-14"]).toBe("Notatka");
  // Zapis otwartego formularza planu nie przywraca starej kolejności.
  await page
    .getByRole("button", { name: "Przedziały: Drugie", exact: true })
    .click();
  await page.getByLabel("Projekt / obszar", { exact: true }).fill("Raporty");
  await page
    .getByRole("button", { name: "Przesuń: Drugie", exact: true })
    .focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(titles(page)).toHaveText(["Pierwsze", "Drugie", "Trzecie"]);
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByRole("status")).toHaveText("Zapisano plan i notatki.");
  await page.reload();
  await expect(titles(page)).toHaveText(["Pierwsze", "Drugie", "Trzecie"]);
  expect(
    (await stored(page)).tasks.find((t: any) => t.id === 2).details.project,
  ).toBe("Raporty");
});

test("moving a task into another priority band changes its priority", async ({
  page,
}) => {
  await seed(page, [
    { id: 3, title: "Inne", done: false, createdAt: 0 },
    { id: 2, title: "Zwykłe", done: false, createdAt: 0 },
    {
      id: 1,
      title: "Pilne",
      done: false,
      createdAt: 0,
      details: { priority: 1 },
    },
  ]);
  await expect(titles(page)).toHaveText(["Pilne", "Inne", "Zwykłe"]);
  await page
    .getByRole("button", { name: "Przesuń: Inne", exact: true })
    .focus();
  await page.keyboard.press("Alt+ArrowUp");
  await expect(titles(page)).toHaveText(["Inne", "Pilne", "Zwykłe"]);
  await expect(page.getByLabel("Priorytet: Inne", { exact: true })).toHaveValue(
    "1",
  );
  await page
    .getByRole("button", { name: "Przesuń: Pilne", exact: true })
    .focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(titles(page)).toHaveText(["Inne", "Zwykłe", "Pilne"]);
  await expect(
    page.getByLabel("Priorytet: Pilne", { exact: true }),
  ).toHaveValue("0");
  await page.reload();
  await expect(titles(page)).toHaveText(["Inne", "Zwykłe", "Pilne"]);
  await expect(page.getByLabel("Priorytet: Inne", { exact: true })).toHaveValue(
    "1",
  );
});

test("drag and drop by the handle reorders tasks within a group", async ({
  page,
}) => {
  await seed(page, [
    { id: 3, title: "Trzecie", done: false, createdAt: 0 },
    { id: 2, title: "Drugie", done: false, createdAt: 0 },
    { id: 1, title: "Pierwsze", done: false, createdAt: 0 },
  ]);
  await page
    .getByRole("button", { name: "Przesuń: Pierwsze", exact: true })
    .dragTo(page.locator(".task-card").filter({ hasText: "Trzecie" }));
  await expect(titles(page)).toHaveText(["Pierwsze", "Trzecie", "Drugie"]);
});

test("the day plan list has the same handles and order", async ({ page }) => {
  const planned = { plannedDay: "2026-10-14" };
  await seed(page, [
    { id: 2, title: "Plan B", done: false, createdAt: 0, details: planned },
    { id: 1, title: "Plan A", done: false, createdAt: 0, details: planned },
  ]);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  const plan = page.getByRole("region", { name: "Plan dnia" });
  const names = plan.locator(".journal-name");
  await expect(names.first()).toContainText("Plan B");
  await plan
    .getByRole("button", { name: "Przesuń: Plan B", exact: true })
    .focus();
  await page.keyboard.press("Alt+ArrowDown");
  await expect(names.first()).toContainText("Plan A");
  await expect(names.nth(1)).toContainText("Plan B");
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(titles(page)).toHaveText(["Plan A", "Plan B"]);
});
