import { test, expect, type Page } from "@playwright/test";
const key = "todo-track-preview-settings-v1";
async function openSettings(page: Page) {
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
}
const stored = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), key);

test("shortcut input captures a combination, saves it in preview and survives reload", async ({
  page,
}) => {
  await page.goto("/");
  await openSettings(page);
  const toggle = page.getByLabel("Start/Stop (globalnie)", { exact: true });
  const quick = page.getByLabel("Szybkie przełączanie (globalnie)", {
    exact: true,
  });
  await expect(toggle).toHaveValue("Control+Alt+Space");
  await expect(quick).toHaveValue("Control+Shift+Space");
  const resetToggle = page.getByRole("button", {
    name: "Przywróć domyślny: Start/Stop (globalnie)",
  });
  await expect(resetToggle).toBeDisabled();

  // Sam klawisz bez modyfikatora nie jest zapisywany.
  await toggle.click();
  await expect(toggle).toHaveAttribute(
    "placeholder",
    "Naciśnij kombinację klawiszy…",
  );
  await toggle.press("t");
  await expect(
    page.getByText("Dodaj Ctrl, Alt, Shift lub Win do klawisza."),
  ).toBeVisible();
  expect(await stored(page)).toEqual({});

  // Ctrl+K w polu nie otwiera szybkiego przełączania, tylko staje się skrótem.
  await toggle.press("Control+Alt+k");
  await expect(toggle).toHaveValue("Control+Alt+K");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByText("Control+Alt+K — Start/Stop z dowolnego okna", {
      exact: false,
    }),
  ).toBeVisible();
  expect(await stored(page)).toEqual({ shortcutToggle: "Control+Alt+K" });
  await expect(resetToggle).toBeEnabled();

  // Ten sam skrót co szybkie przełączanie: błąd przy polu, wartość bez zmian.
  await toggle.click();
  await toggle.press("Control+Shift+Space");
  await expect(
    page.getByText(
      "Skróty szybkiego przełączania i Start/Stop muszą być różne.",
    ),
  ).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-invalid", "true");
  await expect(toggle).toHaveValue("Control+Alt+K");
  expect(await stored(page)).toEqual({ shortcutToggle: "Control+Alt+K" });

  // Esc anuluje nagrywanie bez zmian.
  await quick.click();
  await quick.press("Escape");
  await expect(quick).toHaveValue("Control+Shift+Space");

  // Klawisze funkcyjne i cyfry w formacie pluginu.
  await quick.click();
  await quick.press("Control+Shift+F9");
  await expect(quick).toHaveValue("Control+Shift+F9");
  await expect(toggle).not.toHaveAttribute("aria-invalid", "true");

  await page.reload();
  await openSettings(page);
  await expect(toggle).toHaveValue("Control+Alt+K");
  await expect(quick).toHaveValue("Control+Shift+F9");
  expect(await stored(page)).toEqual({
    shortcutToggle: "Control+Alt+K",
    shortcutQuick: "Control+Shift+F9",
  });

  await resetToggle.click();
  await expect(toggle).toHaveValue("Control+Alt+Space");
  await expect(resetToggle).toBeDisabled();
  expect((await stored(page)).shortcutToggle).toBe("Control+Alt+Space");
});
