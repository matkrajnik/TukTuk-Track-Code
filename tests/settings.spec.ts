import { test, expect } from "@playwright/test";
const key = "todo-track-preview-settings-v1";
test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
});
test("settings tab saves through the preview store, validates ranges and survives reload", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Kopie", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Eksportuj kopię", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  for (const name of [
    "Czas pracy",
    "Okna",
    "Powiadomienia",
    "Skróty",
    "Uruchamianie",
  ])
    await expect(page.getByRole("heading", { name, exact: true })).toBeVisible();
  const idle = page.getByLabel(
    "Zapytaj o czas po co najmniej 10 minutach bezczynności",
  );
  await expect(idle).toBeChecked();
  await idle.uncheck();
  const target = page.getByLabel("Cel dzienny (minuty)");
  await target.fill("450");
  await target.press("Tab");
  await page.getByLabel("Dzień roboczy: Pt", { exact: true }).uncheck();
  await expect(page.getByText("Cel dnia 7:30, cel tygodnia 30:00.")).toBeVisible();
  await target.fill("2000");
  await target.press("Tab");
  await expect(page.getByRole("alert")).toHaveText(
    "Cel dzienny musi mieć od 0 do 1440 minut.",
  );
  await expect(target).toHaveValue("450");
  await page.getByLabel("Początek dnia").fill("07:45");
  await page.getByLabel("Początek dnia").press("Tab");
  const stored = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    key,
  );
  expect(stored).toEqual({
    idleEnabled: false,
    targetDayMinutes: 450,
    workDays: [1, 2, 3, 4],
    dayStart: "07:45",
  });
  await page.reload();
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await expect(
    page.getByLabel("Zapytaj o czas po co najmniej 10 minutach bezczynności"),
  ).not.toBeChecked();
  await expect(page.getByLabel("Cel dzienny (minuty)")).toHaveValue("450");
  await expect(page.getByLabel("Początek dnia")).toHaveValue("07:45");
  await expect(
    page.getByLabel("Dzień roboczy: Pt", { exact: true }),
  ).not.toBeChecked();
  await expect(page.getByText("Control+Shift+Space")).toBeVisible();
});
