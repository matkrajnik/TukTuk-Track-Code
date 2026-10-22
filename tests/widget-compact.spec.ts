import { test, expect, type Page } from "./fixtures/test";
import { openSettings } from "./fixtures/settings";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * C2: malutki mini-widget. Okno 96×28 pokazuje tylko czas (pomiar albo
 * wyszarzona suma dnia), po najechaniu pełny widok, po zjechaniu wraca po
 * 600 ms. Zegar Playwright stoi, więc opóźnienie zwijania przesuwa `runFor`.
 * Prostokąt powiększonego okna liczy Rust (`widget::expanded_rect`, testy w
 * `widget.rs`); tu sprawdzamy widok i wywołania `widget_set_layout`.
 */
test.use({ timezoneId: "Europe/Warsaw" });
const dataKey = "tuktuk-track-preview-v1";
const settingsKey = "tuktuk-track-preview-settings-v1";

async function seed(
  page: Page,
  options: { running?: boolean; compact?: boolean } = {},
) {
  const { running = true, compact = true } = options;
  await page.clock.install({ time: new Date("2026-10-14T09:50:00+02:00") });
  await page.goto("/#widget");
  await page.evaluate(
    ([key, setKey, isRunning, isCompact]) => {
      const t = (s: string) => new Date(s).getTime();
      localStorage.setItem(
        key as string,
        JSON.stringify({
          tasks: [
            {
              id: 1,
              title: "Przegląd kodu",
              done: false,
              createdAt: t("2026-10-14T07:00:00+02:00"),
            },
            {
              id: 2,
              title: "Raport miesięczny",
              done: false,
              createdAt: t("2026-10-14T07:00:00+02:00"),
              details: { project: "Biuro" },
            },
          ],
          sessions: [
            {
              id: 1,
              taskId: 1,
              startedAt: t("2026-10-14T08:00:00+02:00"),
              endedAt: t("2026-10-14T09:00:00+02:00"),
            },
            {
              id: 2,
              taskId: 2,
              startedAt: t("2026-10-14T09:00:00+02:00"),
              endedAt: isRunning ? null : t("2026-10-14T09:30:00+02:00"),
            },
          ],
        }),
      );
      localStorage.setItem(
        setKey as string,
        JSON.stringify({ widgetEnabled: true, widgetCompact: isCompact }),
      );
    },
    [dataKey, settingsKey, running, compact] as const,
  );
  await page.reload();
  await page.clock.pauseAt(new Date("2026-10-14T10:00:00+02:00"));
}
const storedSettings = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
    settingsKey,
  );

test("malutki widget pokazuje tylko czas bieżącego pomiaru", async ({
  page,
}) => {
  await page.setViewportSize({ width: 96, height: 28 });
  await seed(page);
  const widget = page.locator("#widget");
  await expect(widget).toHaveClass(/compact/);
  const time = widget.getByLabel("Czas bieżącego pomiaru");
  await expect(time).toHaveText("01:00:00");
  await page.clock.runFor(2000);
  await expect(time).toHaveText("01:00:02");
  // Bez przycisków i nazwy zadania; kolor obramowania sygnalizuje pomiar.
  await expect(widget.getByRole("button")).toHaveCount(0);
  await expect(widget.locator("strong")).toHaveCount(0);
  await expect(widget).toHaveClass(/running/);
  const scroll = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  }));
  expect(scroll).toEqual({ width: 96, height: 28 });
  await expect(time).toHaveAttribute("title", "Raport miesięczny · pomiar trwa");
});

test("malutki widget bez pomiaru pokazuje wyszarzoną sumę dnia", async ({
  page,
}) => {
  await page.setViewportSize({ width: 96, height: 28 });
  await seed(page, { running: false });
  const widget = page.locator("#widget");
  // 8:00–9:00 i 9:00–9:30.
  const total = widget.getByLabel("Suma dnia");
  await expect(total).toHaveText("1:30");
  await expect(total).toHaveClass(/idle/);
  await expect(widget).not.toHaveClass(/running/);
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveCount(0);
  await expect(widget.getByRole("button")).toHaveCount(0);
});

test("najechanie pokazuje pełny widok, zjechanie po 600 ms wraca do malutkiego", async ({
  page,
}) => {
  await page.setViewportSize({ width: 96, height: 28 });
  await seed(page);
  const widget = page.locator("#widget");
  await expect(widget).toHaveClass(/compact/);
  await widget.hover();
  await expect(widget).not.toHaveClass(/compact/);
  // Okno rośnie w aplikacji; w podglądzie rozmiar strony ustawia test.
  await page.setViewportSize({ width: 320, height: 64 });
  await expect(
    widget.getByRole("button", { name: "Zatrzymaj pomiar" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("button", { name: "Szybkie przełączanie" }),
  ).toBeVisible();
  await expect(
    widget.getByRole("button", { name: "Schowaj widget" }),
  ).toBeVisible();
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toBeVisible();
  await expect(
    widget.getByRole("button", { name: "Pokaż główne okno" }),
  ).toContainText("Raport miesięczny");

  // Zjechanie: pełny widok trwa 600 ms, potem wraca malutki.
  await page.mouse.move(600, 400);
  await page.clock.runFor(500);
  await expect(widget).not.toHaveClass(/compact/);
  // Powrót kursora przed upływem opóźnienia anuluje zwijanie.
  await widget.hover();
  await page.clock.runFor(1000);
  await expect(widget).not.toHaveClass(/compact/);
  await page.mouse.move(600, 400);
  await page.clock.runFor(700);
  await expect(widget).toHaveClass(/compact/);
  await page.setViewportSize({ width: 96, height: 28 });
  await expect(widget.getByRole("button")).toHaveCount(0);
});

test("otwarte okno szybkiego przełączania (⇄) trzyma widget powiększony", async ({
  page,
}) => {
  await page.setViewportSize({ width: 96, height: 28 });
  await seed(page);
  const widget = page.locator("#widget");
  await widget.hover();
  await page.setViewportSize({ width: 320, height: 64 });
  await widget.getByRole("button", { name: "Szybkie przełączanie" }).click();
  await expect(
    page.getByRole("dialog", { name: "Szybkie przełączanie" }),
  ).toBeVisible();
  await page.mouse.move(600, 400);
  await page.clock.runFor(3000);
  await expect(widget).not.toHaveClass(/compact/);
  // Po zamknięciu dialogu widget się zwija.
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.clock.runFor(1500);
  await expect(widget).toHaveClass(/compact/);
});

test("przełącznik w widgecie włącza i wyłącza malutki widok, ustawienie się zapisuje", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 64 });
  await seed(page, { compact: false });
  const widget = page.locator("#widget");
  // Domyślnie pełny widok (jak dotąd), przełącznik w rogu.
  await expect(widget).not.toHaveClass(/compact/);
  const toggle = widget.getByRole("button", { name: "Malutki widget" });
  await expect(toggle).toHaveAttribute("aria-pressed", "false");
  await toggle.click();
  // Okno w aplikacji kurczy się do 96×28: kursor zostaje poza nim.
  await page.setViewportSize({ width: 96, height: 28 });
  await page.mouse.move(600, 400);
  await page.clock.runFor(1000);
  await expect(widget).toHaveClass(/compact/);
  expect((await storedSettings(page)).widgetCompact).toBe(true);
  // Po przeładowaniu widget startuje jako malutki.
  await page.reload();
  await expect(page.locator("#widget")).toHaveClass(/compact/);
  // W pełnym widoku (po najechaniu) przełącznik jest wciśnięty i wyłącza tryb.
  await page.locator("#widget").hover();
  const pressed = page
    .locator("#widget")
    .getByRole("button", { name: "Malutki widget" });
  await expect(pressed).toHaveAttribute("aria-pressed", "true");
  await page.setViewportSize({ width: 320, height: 64 });
  await pressed.click();
  await expect(page.locator("#widget")).not.toHaveClass(/compact/);
  expect((await storedSettings(page)).widgetCompact).toBe(false);
  await page.mouse.move(600, 400);
  await page.clock.runFor(2000);
  await expect(page.locator("#widget")).not.toHaveClass(/compact/);
});

test("Ustawienia → Okna: przełącznik malutkiego widgetu, wyłączony bez widgetu", async ({
  page,
}) => {
  await page.goto("/");
  await openSettings(page, "Okna");
  const compact = page.getByLabel("Malutki widget: tylko czas, pełny po najechaniu");
  await expect(compact).not.toBeChecked();
  await expect(compact).toBeDisabled();
  await page.getByLabel("Pokazuj mini-widget zawsze na wierzchu").check();
  await expect(compact).toBeEnabled();
  await compact.check();
  await expect(compact).toBeChecked();
  expect(await storedSettings(page)).toEqual({
    widgetEnabled: true,
    widgetCompact: true,
  });
  await compact.uncheck();
  expect((await storedSettings(page)).widgetCompact).toBe(false);
  // Wyszukiwarka ustawień znajduje przełącznik w sekcji Okna.
  await page.getByRole("searchbox", { name: "Szukaj ustawienia" }).fill("malutki");
  await expect(
    page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab"),
  ).toHaveText(["Okna"]);
});

test.describe("aplikacja: układ okna widgetu liczy Rust", () => {
  const layouts = async (page: Page) =>
    (
      await backend<{ cmd: string; args: any }[]>(page, "calls")
    )
      .filter((c) => c.cmd === "widget_set_layout")
      .map((c) => c.args);

  test("pełny widok bez malutkiego widgetu zostaje przy rozmiarze 64", async ({
    page,
  }) => {
    await openDesktop(page, { window: "widget" });
    await expect(page.locator("#widget")).not.toHaveClass(/compact/);
    await expect.poll(() => layouts(page)).toContainEqual({
      compact: false,
      expanded: false,
      fullHeight: 64,
    });
  });

  test("najechanie powiększa, zjechanie po 600 ms zwija", async ({ page }) => {
    await page.setViewportSize({ width: 96, height: 28 });
    await openDesktop(page, {
      window: "widget",
      settings: { widgetEnabled: true, widgetCompact: true },
    });
    const widget = page.locator("#widget");
    await expect(widget).toHaveClass(/compact/);
    await expect.poll(() => layouts(page)).toContainEqual({
      compact: true,
      expanded: false,
      fullHeight: 64,
    });
    await widget.hover();
    await expect(widget).not.toHaveClass(/compact/);
    await expect.poll(() => layouts(page)).toContainEqual({
      compact: true,
      expanded: true,
      fullHeight: 64,
    });
    await page.mouse.move(700, 500);
    await expect(widget).toHaveClass(/compact/);
    const all = await layouts(page);
    expect(all.at(-1)).toEqual({
      compact: true,
      expanded: false,
      fullHeight: 64,
    });
  });
});
