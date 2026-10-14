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
test("preview reads a swapped shortcut pair together and resets a colliding pair", async ({
  page,
}) => {
  const open = async () => {
    await page.reload();
    await page
      .getByRole("button", { name: "Historia i kopie", exact: true })
      .click();
    await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  };
  await page.evaluate(
    (k) =>
      localStorage.setItem(
        k,
        JSON.stringify({
          shortcutQuick: "Control+Alt+Space",
          shortcutToggle: "Control+Shift+Space",
        }),
      ),
    key,
  );
  await open();
  await expect(
    page.getByText("Control+Alt+Space — szybkie przełączanie z dowolnego okna."),
  ).toBeVisible();
  await page.evaluate(
    (k) =>
      localStorage.setItem(
        k,
        JSON.stringify({
          shortcutQuick: "Ctrl+Alt+J",
          shortcutToggle: "Alt+Control+J",
          dayStart: "09:00",
        }),
      ),
    key,
  );
  await open();
  await expect(page.getByLabel("Początek dnia")).toHaveValue("09:00");
  await expect(
    page.getByText(
      "Control+Shift+Space — szybkie przełączanie z dowolnego okna.",
    ),
  ).toBeVisible();
});
test("startup toggles save autostart state to localStorage in preview", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  const withWindows = page.getByLabel("Uruchamiaj z Windows", { exact: true });
  const hidden = page.getByLabel("Uruchamiaj schowane do traya", {
    exact: true,
  });
  await expect(withWindows).not.toBeChecked();
  await expect(hidden).toBeDisabled();
  await withWindows.check();
  await expect(hidden).toBeEnabled();
  await hidden.check();
  const read = () =>
    page.evaluate(() => ({
      autostart: JSON.parse(
        localStorage.getItem("todo-track-preview-autostart-v1")!,
      ),
      settings: JSON.parse(
        localStorage.getItem("todo-track-preview-settings-v1")!,
      ),
    }));
  expect(await read()).toEqual({
    autostart: { enabled: true },
    settings: { autostartHidden: true },
  });
  await page.reload();
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await expect(withWindows).toBeChecked();
  await expect(hidden).toBeChecked();
  // Wyłączenie autostartu zachowuje wybór „schowane” na następne włączenie.
  await withWindows.uncheck();
  await expect(hidden).toBeDisabled();
  await expect(hidden).toBeChecked();
  expect(await read()).toEqual({
    autostart: { enabled: false },
    settings: { autostartHidden: true },
  });
});
test("failed autostart save reverts the clicked checkbox and shows the error", async ({
  page,
}) => {
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  const withWindows = page.getByLabel("Uruchamiaj z Windows", { exact: true });
  const hidden = page.getByLabel("Uruchamiaj schowane do traya", {
    exact: true,
  });
  await expect(withWindows).not.toBeChecked();
  // Symulacja błędu zapisu (w aplikacji: brak dostępu do rejestru).
  await page.evaluate(() => {
    const original = Storage.prototype.setItem;
    Storage.prototype.setItem = function (name: string, value: string) {
      if (name === "todo-track-preview-autostart-v1")
        throw new Error("Odmowa dostępu");
      return original.call(this, name, value);
    };
  });
  await withWindows.click();
  await expect(page.getByRole("alert")).toContainText("Odmowa dostępu");
  await expect(withWindows).not.toBeChecked();
  await expect(hidden).toBeDisabled();
});
