import { test, expect } from "@playwright/test";
import { at, ids, NOW, openPreview, stored, TODAY } from "./fixtures/seed";
test.use({ timezoneId: "Europe/Warsaw" });

test("quick switch: Ctrl+K, arrows choose a task and Enter stops the previous measurement", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();

  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await expect(dialog).toBeVisible();
  const search = dialog.getByLabel("Znajdź zadanie do uruchomienia");
  await expect(search).toBeFocused();
  // Nieukończone i niezarchiwizowane, w kolejności listy zadań (priorytet, potem nowsze).
  const results = dialog.locator(".quick-result > span");
  await expect(results).toHaveText([
    "Analiza importu",
    "Spotkanie zespołu",
    "Code review",
    "Wdrożenie etykiet",
  ]);
  const selected = dialog.locator(".quick-result.selected > span");
  await expect(selected).toHaveText("Analiza importu");
  // Strzałka w górę nie wychodzi poza listę, w dół nie wychodzi za koniec.
  await page.keyboard.press("ArrowUp");
  await expect(selected).toHaveText("Analiza importu");
  for (let i = 0; i < 6; i++) await page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText("Wdrożenie etykiet");
  await page.keyboard.press("ArrowUp");
  await expect(selected).toHaveText("Code review");

  // Wpisanie tekstu filtruje (tytuł i projekt) i wraca do pierwszego wyniku.
  await search.fill("e");
  await expect(results).toHaveText([
    "Spotkanie zespołu",
    "Code review",
    "Wdrożenie etykiet",
  ]);
  await expect(selected).toHaveText("Spotkanie zespołu");
  await page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText("Code review");

  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Code review", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Start: Analiza importu", { exact: true }),
  ).toBeVisible();

  const data = await stored(page);
  const open = data.sessions.filter((s: any) => s.endedAt === null);
  expect(open).toEqual([
    expect.objectContaining({ taskId: ids.review, startedAt: Date.parse(NOW) }),
  ]);
  expect(
    data.sessions.find(
      (s: any) =>
        s.taskId === ids.analysis && s.startedAt === at(TODAY, "11:00"),
    ),
  ).toMatchObject({ endedAt: Date.parse(NOW) });
});

test("quick switch: Escape closes without touching the timer and finished tasks are not offered", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  const before = await stored(page);
  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await dialog.getByLabel("Znajdź zadanie do uruchomienia").fill("dokumentacja");
  await expect(
    dialog.getByText("Brak pasujących nieukończonych zadań."),
  ).toBeVisible();
  // Bez wyników lista proponuje utworzenie zadania (Enter), ale Esc nic nie zmienia.
  await expect(
    dialog.getByRole("button", { name: "Utwórz i rozpocznij: dokumentacja" }),
  ).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  expect(await stored(page)).toEqual(before);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
});
