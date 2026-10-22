import { test, expect, type Page } from "./fixtures/test";
import { ids, openPreview, stored } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * B11: skróty w oknie aplikacji — Ctrl+Shift+S = Start/Stop (logika
 * `toggle_measurement`), N = fokus w „Nowe zadanie”, lista w Ustawienia → Skróty.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

type Session = { taskId: number; endedAt: number | null };
const running = async (page: Page) =>
  ((await stored(page)).sessions as Session[]).filter((s) => s.endedAt === null);
const newTask = (page: Page) => page.getByLabel("Nowe zadanie", { exact: true });

test("Ctrl+Shift+S zatrzymuje i wznawia ostatnio mierzone zadanie", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  expect((await running(page)).map((s) => s.taskId)).toEqual([ids.analysis]);

  await page.keyboard.press("Control+Shift+S");
  await expect.poll(async () => (await running(page)).length).toBe(0);

  await page.keyboard.press("Control+Shift+S");
  await expect
    .poll(async () => (await running(page)).map((s) => s.taskId))
    .toEqual([ids.analysis]);

  // Działa także na innym widoku, a w otwartym oknie dialogowym nie.
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await page.keyboard.press("Control+Shift+S");
  await expect.poll(async () => (await running(page)).length).toBe(0);
  await page.keyboard.press("Control+k");
  await expect(page.getByRole("dialog")).toBeVisible();
  await page.keyboard.press("Control+Shift+S");
  await page.waitForTimeout(200);
  expect(await running(page)).toEqual([]);
});

test("Ctrl+Shift+S bez wcześniejszego pomiaru otwiera szybkie przełączanie", async ({
  page,
}) => {
  const data = await openPreview(page);
  await page.evaluate(
    ([key, value]) => {
      localStorage.setItem(key, value);
    },
    [
      "tuktuk-track-preview-v1",
      JSON.stringify({ ...(data as object), sessions: [] }),
    ] as const,
  );
  await page.reload();
  await expect(newTask(page)).toBeVisible();
  await page.keyboard.press("Control+Shift+S");
  await expect(page.getByRole("dialog")).toBeVisible();
  expect(await running(page)).toEqual([]);
});

test("N przenosi fokus do „Nowe zadanie”, ale nie w polu tekstowym", async ({
  page,
}) => {
  await openPreview(page);
  await expect(newTask(page)).toBeVisible();
  await page.locator("body").click({ position: { x: 5, y: 400 } });
  await page.keyboard.press("n");
  await expect(newTask(page)).toBeFocused();
  // Litera trafia do pola jak zwykłe pisanie, bez ponownego skoku.
  await page.keyboard.type("nowe");
  await expect(newTask(page)).toHaveValue("nowe");

  // Z innego widoku: przejście do Zadań i fokus w polu.
  await openSettings(page, "Skróty");
  const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
  await search.fill("n");
  await expect(search).toHaveValue("n");
  await expect(newTask(page)).toHaveCount(0);
  await search.fill("");
  await page.getByRole("heading", { name: "Skróty", exact: true }).click();
  await page.keyboard.press("n");
  await expect(newTask(page)).toBeFocused();

  // Shift+N i Ctrl+N nie są skrótem.
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await page.getByRole("heading", { name: "Skróty", exact: true }).click();
  await page.keyboard.press("Shift+N");
  await expect(newTask(page)).toHaveCount(0);
});

test("Ustawienia → Skróty pokazują skróty w oknie aplikacji", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Skróty");
  const list = page.getByRole("list", { name: "Skróty w oknie aplikacji" });
  await expect(list.getByRole("listitem")).toHaveText([
    /Ctrl\+K — szybkie przełączanie/,
    /Ctrl\+Shift\+S — Start\/Stop/,
    /N — przejście do pola „Nowe zadanie”/,
    /Ctrl\+, — Ustawienia/,
  ]);
  // Wyszukiwarka ustawień znajduje sekcję po skrócie.
  const search = page.getByRole("searchbox", { name: "Szukaj ustawienia" });
  await search.fill("Ctrl+Shift+S");
  await expect(
    page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab"),
  ).toHaveText(["Skróty"]);
});

test("aplikacja: Ctrl+Shift+S ustępuje globalnemu skrótowi o tej samej kombinacji", async ({
  page,
}) => {
  const toggles = async () =>
    (await backend<{ cmd: string }[]>(page, "calls")).filter(
      (c) => c.cmd === "toggle_measurement",
    ).length;
  // Domyślne skróty globalne: lokalny Ctrl+Shift+S woła toggle_measurement.
  await openDesktop(page);
  await expect(newTask(page)).toBeVisible();
  await page.keyboard.press("Control+Shift+S");
  await expect.poll(toggles).toBe(1);

  // Start/Stop ustawiony globalnie na Ctrl+Shift+S: obsługuje go tylko Rust.
  await openDesktop(page, { settings: { shortcutToggle: "Control+Shift+S" } });
  await expect(newTask(page)).toBeVisible();
  await page.waitForTimeout(100);
  await page.keyboard.press("Control+Shift+S");
  await page.waitForTimeout(200);
  expect(await toggles()).toBe(0);

  // Szybkie przełączanie na tej kombinacji też wyłącza lokalną obsługę.
  await openDesktop(page, { settings: { shortcutQuick: "Shift+Control+S" } });
  await expect(newTask(page)).toBeVisible();
  await page.waitForTimeout(100);
  await page.keyboard.press("Control+Shift+S");
  await page.waitForTimeout(200);
  expect(await toggles()).toBe(0);
});
