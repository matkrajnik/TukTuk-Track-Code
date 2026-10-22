import { test, expect, type Page } from "./fixtures/test";
import {
  ids,
  NOW,
  PREVIEW_KEY,
  SETTINGS_KEY,
  seed,
  stored,
  type SeedOptions,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Tryb skupienia na podglądzie (PLAN_1.3.md, etap 6) ze sterowanym zegarem.
 * Reguły kolejki powiadomień i końca bloku po uśpieniu sprawdzają testy Rust
 * (`focus.rs`); tu: start, odliczanie, przerwa i przerwanie przez Stop.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
const FOCUS_KEY = "tuktuk-track-preview-focus-v1";
const MIN = 60_000;

async function open(
  page: Page,
  path: string,
  options: SeedOptions & { settings?: Record<string, unknown> } = {},
) {
  await page.clock.install({ time: new Date(Date.parse(NOW) - 10_000) });
  await page.goto(path);
  await page.evaluate(
    ([keys, values]) => {
      localStorage.clear();
      keys.forEach((k, i) => values[i] && localStorage.setItem(k, values[i]));
    },
    [
      [PREVIEW_KEY, SETTINGS_KEY],
      [
        JSON.stringify(seed(options)),
        options.settings ? JSON.stringify(options.settings) : "",
      ],
    ] as const,
  );
  await page.reload();
  await page.clock.pauseAt(new Date(NOW));
}
const running = async (page: Page) =>
  (await stored(page)).sessions
    .filter((s: { endedAt: number | null }) => s.endedAt === null)
    .map((s: { taskId: number }) => s.taskId);
const blocks = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").blocks ?? [],
    FOCUS_KEY,
  );

test("widget starts a block with measurement, counts down and the break stops measuring", async ({
  page,
}) => {
  await page.setViewportSize({ width: 320, height: 64 });
  // Bez pomiaru: blok startuje na ostatnio mierzonym zadaniu.
  await open(page, "/#widget");
  const widget = page.locator("#widget");
  expect(await running(page)).toEqual([]);
  await widget
    .getByRole("button", { name: "Rozpocznij blok skupienia" })
    .click();
  const left = widget.getByLabel("Pozostało w bloku skupienia");
  await expect(left).toHaveText("25:00");
  expect(await running(page)).toHaveLength(1);
  // Odliczanie zamiast zegara pomiaru; pomiar liczy się dalej normalnie.
  await expect(widget.getByLabel("Czas bieżącego pomiaru")).toHaveCount(0);
  await expect(widget.getByRole("status")).toHaveText("Skupienie do 12:25");
  await page.clock.runFor(MIN);
  await expect(left).toHaveText("24:00");
  await page.clock.fastForward(4 * MIN + 30_000);
  await expect(left).toHaveText("19:30");
  const scroll = await page.evaluate(() => ({
    width: document.documentElement.scrollWidth,
    height: document.documentElement.scrollHeight,
  }));
  expect(scroll).toEqual({ width: 320, height: 64 });

  // Koniec bloku: pytanie o przerwę, pomiar nadal trwa (nic nie przełącza się samo).
  await page.clock.fastForward(20 * MIN);
  await expect(widget.getByRole("button", { name: "Przerwa" })).toBeVisible();
  await expect(
    widget.getByRole("button", { name: "Kolejny blok" }),
  ).toBeVisible();
  await expect(left).toHaveCount(0);
  await expect(widget.getByRole("status")).toHaveText(
    "Koniec bloku: przerwa czy kolejny?",
  );
  expect(await running(page)).toHaveLength(1);
  expect((await blocks(page))[0].completed).toBe(true);

  // Przerwa (domyślnie) zatrzymuje pomiar i odlicza 5 minut.
  await widget.getByRole("button", { name: "Przerwa" }).click();
  expect(await running(page)).toEqual([]);
  await expect(widget.getByLabel("Pozostało z przerwy")).toHaveText("05:00");
  await page.clock.fastForward(5 * MIN);
  await expect(widget.getByRole("status")).toContainText("Wracasz do:");
  // „Start” odpowiada na „Wracasz do X?”.
  await widget.getByRole("button", { name: "Wznów ostatnie zadanie" }).click();
  expect(await running(page)).toHaveLength(1);
});

test("Stop ends the block as not completed and the bar shows the countdown", async ({
  page,
}) => {
  await open(page, "/", { active: { task: ids.analysis, from: "11:00" } });
  const bar = page.getByLabel("Bieżący pomiar", { exact: true });
  await bar.getByRole("button", { name: "Rozpocznij blok skupienia" }).click();
  const left = bar.getByLabel("Pozostało w bloku skupienia");
  await expect(left).toHaveText("25:00");
  // Pomiar trwał przed blokiem i trwa dalej na tym samym przedziale.
  expect(await running(page)).toEqual([ids.analysis]);
  await page.clock.fastForward(7 * MIN);
  await expect(left).toHaveText("18:00");

  await bar.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByLabel("Bieżący pomiar", { exact: true })).toHaveCount(0);
  expect(await running(page)).toEqual([]);
  await expect
    .poll(async () => (await blocks(page))[0]?.completed)
    .toBe(false);
  const [block] = await blocks(page);
  expect(block.end - block.start).toBe(7 * MIN);
  // Przerwany blok bez pytania o przerwę i bez licznika na Dashboardzie.
  await expect(page.getByRole("region", { name: "Koniec bloku skupienia" })).toHaveCount(0);
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Bloki skupienia" }),
  ).toContainText(/Ukończone bloki dziś\s*0/);

  // Zmiana zadania w trakcie bloku też go przerywa.
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await page.evaluate(
    ([k, task]) => {
      const data = JSON.parse(localStorage.getItem(k)!);
      data.sessions.unshift({
        id: 999,
        taskId: task,
        startedAt: Date.now(),
        endedAt: null,
      });
      localStorage.setItem(k, JSON.stringify(data));
    },
    [PREVIEW_KEY, ids.analysis] as const,
  );
  await page.reload();
  await page
    .getByLabel("Bieżący pomiar", { exact: true })
    .getByRole("button", { name: "Rozpocznij blok skupienia" })
    .click();
  await page.clock.fastForward(2 * MIN);
  await page.evaluate(
    ([k, task]) => {
      const data = JSON.parse(localStorage.getItem(k)!);
      const now = Date.now();
      data.sessions.forEach((s: { endedAt: number | null }) => {
        if (s.endedAt === null) s.endedAt = now;
      });
      data.sessions.unshift({ id: 1000, taskId: task, startedAt: now, endedAt: null });
      localStorage.setItem(k, JSON.stringify(data));
    },
    [PREVIEW_KEY, ids.labels] as const,
  );
  await page.reload();
  await expect
    .poll(async () => (await blocks(page))[1]?.completed)
    .toBe(false);
  await expect(
    page.getByLabel("Pozostało w bloku skupienia"),
  ).toHaveCount(0);
});

test("block end banner: break stops measuring, then Wracasz do X? starts it again", async ({
  page,
}) => {
  await open(page, "/", {
    active: { task: ids.analysis, from: "11:00" },
    settings: { focusBlockMinutes: 50, focusBreakMinutes: 10 },
  });
  await page
    .getByLabel("Bieżący pomiar", { exact: true })
    .getByRole("button", { name: "Rozpocznij blok skupienia" })
    .click();
  await expect(page.getByLabel("Pozostało w bloku skupienia")).toHaveText(
    "50:00",
  );
  await page.clock.fastForward(50 * MIN);
  const ended = page.getByRole("region", { name: "Koniec bloku skupienia" });
  await expect(ended).toContainText("„Analiza importu” · 50 min");
  expect(await running(page)).toEqual([ids.analysis]);

  await ended.getByRole("button", { name: "Przerwa" }).click();
  await expect(page.getByLabel("Bieżący pomiar", { exact: true })).toHaveCount(0);
  expect(await running(page)).toEqual([]);
  const pause = page.getByRole("region", { name: "Przerwa" });
  await expect(pause.getByLabel("Pozostało z przerwy")).toHaveText("10:00");
  await expect(pause).toContainText("Pomiar zatrzymany.");
  await page.clock.fastForward(4 * MIN);
  await expect(pause.getByLabel("Pozostało z przerwy")).toHaveText("06:00");

  await page.clock.fastForward(6 * MIN);
  const back = page.getByRole("region", { name: "Powrót z przerwy" });
  await expect(back).toContainText("Wracasz do „Analiza importu”?");
  // Bez kliknięcia timer stoi.
  await page.clock.runFor(MIN);
  expect(await running(page)).toEqual([]);
  await back.getByRole("button", { name: "Rozpocznij" }).click();
  await expect(back).toHaveCount(0);
  expect(await running(page)).toEqual([ids.analysis]);
  await expect(page.getByLabel("Bieżący pomiar", { exact: true })).toContainText(
    "Analiza importu",
  );

  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  const card = page.getByRole("region", { name: "Bloki skupienia" });
  await expect(card).toContainText(/Ukończone bloki dziś\s*1/);
  await expect(card).toContainText(/W tym tygodniu\s*1/);
});

test("settings set block and break length; break can keep measuring", async ({
  page,
}) => {
  await open(page, "/", { active: { task: ids.analysis, from: "11:00" } });
  await openSettings(page, "Skupienie");
  await page.getByRole("button", { name: "Blok 50 min" }).click();
  await expect(page.getByLabel("Blok skupienia (minuty)")).toHaveValue("50");
  await page.getByLabel("Przerwa (minuty)").fill("7");
  await page.getByLabel("Przerwa (minuty)").blur();
  await page.getByLabel("Przerwa zatrzymuje pomiar").uncheck();
  await expect
    .poll(() =>
      page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
    )
    .toEqual({
      focusBlockMinutes: 50,
      focusBreakMinutes: 7,
      focusBreakStops: false,
    });
  await page.getByLabel("Blok skupienia (minuty)").fill("0");
  await page.getByLabel("Blok skupienia (minuty)").blur();
  await expect(page.getByRole("alert")).toContainText(
    "Blok skupienia musi mieć od 1 do 240 minut.",
  );

  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await page
    .getByLabel("Bieżący pomiar", { exact: true })
    .getByRole("button", { name: "Rozpocznij blok skupienia" })
    .click();
  await page.clock.fastForward(50 * MIN);
  await page
    .getByRole("region", { name: "Koniec bloku skupienia" })
    .getByRole("button", { name: "Przerwa" })
    .click();
  // Ustawienie „pomiar trwa przez przerwę”.
  expect(await running(page)).toEqual([ids.analysis]);
  await expect(page.getByRole("region", { name: "Przerwa" })).toContainText(
    "trwa dalej",
  );
  await expect(page.getByLabel("Pozostało z przerwy")).toHaveText("07:00");
});
