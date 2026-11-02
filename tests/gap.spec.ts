import { test, expect, type Page } from "./fixtures/test";
import { at, ids, seed, TODAY } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw" });

const banner = (page: Page) =>
  page.getByRole("alert").filter({ hasText: "Pomiar zatrzymany po przerwie" });
const YESTERDAY = "2026-10-14";
/** Pomiar „Analiza importu” od wczoraj 16:00, aplikacja działała do 17:30, start dziś o 12:00 (NOW). */
function overnight() {
  const data = seed();
  // Przywrócenie odmawia, gdy po zatrzymaniu zapisano inny pomiar (seed ma dzisiejsze).
  data.sessions = data.sessions.filter(
    (s: any) => s.endedAt !== null && s.endedAt <= at(YESTERDAY, "16:00"),
  );
  data.sessions.unshift({
    id: 100,
    taskId: ids.analysis,
    startedAt: at(YESTERDAY, "16:00"),
    endedAt: null,
  });
  return data;
}
const session = async (page: Page, id: number) =>
  (await backend<any>(page, "snapshot")).sessions.find((s: any) => s.id === id);
const gapCalls = async (page: Page) =>
  (await backend<any[]>(page, "calls")).filter((c) => c.cmd === "gap_resolve");
const openOvernight = (page: Page) =>
  openDesktop(page, {
    data: overnight(),
    heartbeat: at(YESTERDAY, "17:30"),
  });

test("gap banner: shows the task and the downtime after the computer was off overnight", async ({
  page,
}) => {
  await openOvernight(page);
  await expect(banner(page)).toBeVisible();
  await expect(banner(page)).toContainText("Aplikacja nie działała od");
  await expect(banner(page)).toContainText("14 paź, 17:30");
  await expect(banner(page)).toContainText("15 paź, 12:00");
  await expect(banner(page)).toContainText(
    "Pomiar „Analiza importu” zatrzymano o 17:30.",
  );
  expect((await session(page, 100)).endedAt).toBe(at(YESTERDAY, "17:30"));
  await expect(
    page.getByLabel("Start: Analiza importu", { exact: true }),
  ).toBeVisible();
});

test("gap banner: OK keeps the stop and hides the banner", async ({ page }) => {
  await openOvernight(page);
  await banner(page).getByRole("button", { name: "OK" }).click();
  await expect(banner(page)).toHaveCount(0);
  await expect(page.locator("#main")).toBeFocused();
  expect(await gapCalls(page)).toEqual([
    { cmd: "gap_resolve", args: { action: "keep" } },
  ]);
  expect((await session(page, 100)).endedAt).toBe(at(YESTERDAY, "17:30"));
});

test("gap banner: restoring the whole time keeps the original measurement running", async ({
  page,
}) => {
  await openOvernight(page);
  await banner(page).getByRole("button", { name: "Przywróć cały czas" }).click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  expect((await session(page, 100)).endedAt).toBeNull();
  const data = await backend<any>(page, "snapshot");
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toHaveLength(1);
});

function sameDay() {
  return seed({ active: { task: ids.analysis, from: "11:00" } });
}

test("gap banner: resuming starts the same task from now", async ({ page }) => {
  await openDesktop(page, {
    data: sameDay(),
    heartbeat: at(TODAY, "11:20"),
  });
  await expect(banner(page)).toContainText(
    "Aplikacja nie działała od 11:20 do 12:00.",
  );
  await banner(page).getByRole("button", { name: "Wznów pomiar" }).click();
  await expect(banner(page)).toHaveCount(0);
  const { sessions } = await backend<any>(page, "snapshot");
  expect(
    sessions.find((s: any) => s.startedAt === at(TODAY, "11:00")).endedAt,
  ).toBe(at(TODAY, "11:20"));
  expect(sessions).toContainEqual(
    expect.objectContaining({
      taskId: ids.analysis,
      startedAt: at(TODAY, "12:00"),
      endedAt: null,
    }),
  );
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
});

test("gap banner: no banner after a short restart or without a heartbeat", async ({
  page,
}) => {
  await openDesktop(page, {
    data: sameDay(),
    heartbeat: at(TODAY, "11:56"),
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  await page.waitForTimeout(200);
  await expect(banner(page)).toHaveCount(0);
  expect(await gapCalls(page)).toEqual([]);
});

test("gap banner: no banner without a heartbeat", async ({ page }) => {
  await openDesktop(page, { data: sameDay() });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  await page.waitForTimeout(200);
  await expect(banner(page)).toHaveCount(0);
});

test("gap banner: restoring is refused while another measurement runs", async ({
  page,
}) => {
  await openOvernight(page);
  await page.getByLabel("Start: Code review", { exact: true }).click();
  await banner(page).getByRole("button", { name: "Przywróć cały czas" }).click();
  await expect(
    page.getByText("Trwa inny pomiar. Zatrzymaj go, aby przywrócić cały czas."),
  ).toBeVisible();
  await expect(banner(page)).toBeVisible();
  expect((await session(page, 100)).endedAt).toBe(at(YESTERDAY, "17:30"));
});
