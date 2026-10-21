import { test, expect, type Page } from "./fixtures/test";
import { at, ids, seed, TODAY } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw" });

const MIN = 60_000;
const banner = (page: Page) =>
  page.getByRole("alert").filter({ hasText: "Wracasz po przerwie?" });

/** Ustawia zegar strony na podaną godzinę dzisiaj i zgłasza bezczynność. */
async function tick(
  page: Page,
  time: string,
  userIdle: number,
  agentIdle?: number,
) {
  await page.clock.setFixedTime(at(TODAY, time));
  return backend<boolean>(page, "idleTick", { userIdle, agentIdle });
}
/** Pomiar „Analiza importu” od 11:00, brak aktywności 11:30–12:00, powrót o 12:00. */
async function breakAndReturn(page: Page) {
  await openDesktop(page, {
    data: seed({ active: { task: ids.analysis, from: "11:00" } }),
    now: at(TODAY, "11:30"),
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  expect(await tick(page, "11:30", 0)).toBe(false);
  expect(await tick(page, "11:50", 20 * MIN)).toBe(false);
  await expect(banner(page)).toHaveCount(0);
  expect(await tick(page, "12:00", 0)).toBe(true);
  await expect(banner(page)).toBeVisible();
  await expect(banner(page)).toContainText(
    "Analiza importu · brak aktywności od 11:30:00",
  );
}
const activeSession = async (page: Page) =>
  (await backend<any>(page, "snapshot")).sessions.find(
    (s: any) => s.startedAt === at(TODAY, "11:00"),
  );

test("idle banner: keeping the whole break leaves the measurement running", async ({
  page,
}) => {
  await breakAndReturn(page);
  await banner(page)
    .getByRole("button", { name: "Zachowaj cały czas" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  expect(await activeSession(page)).toMatchObject({ endedAt: null });
  const calls = await backend<any[]>(page, "calls");
  expect(calls.filter((c) => c.cmd === "idle_resolve")).toEqual([
    { cmd: "idle_resolve", args: { trim: false } },
  ]);
});

test("idle banner: stopping at the start of the break ends the measurement then", async ({
  page,
}) => {
  await breakAndReturn(page);
  await banner(page)
    .getByRole("button", { name: "Zatrzymaj na początku przerwy" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Start: Analiza importu", { exact: true }),
  ).toBeVisible();
  expect(await activeSession(page)).toMatchObject({
    taskId: ids.analysis,
    endedAt: at(TODAY, "11:30"),
  });
});

test("idle banner: a break ended by a working AI agent is forgotten without asking", async ({
  page,
}) => {
  await openDesktop(page, {
    data: seed({ active: { task: ids.analysis, from: "11:00" } }),
    now: at(TODAY, "11:30"),
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  // Użytkownik i agent bez aktywności od 11:35: przerwa się zaczyna…
  expect(await tick(page, "11:50", 20 * MIN, 15 * MIN)).toBe(false);
  // …ale agent wraca do pracy, gdy użytkownika nadal nie ma: przerwa przepada.
  expect(await tick(page, "11:55", 25 * MIN, 0)).toBe(false);
  // Powrót użytkownika już o nic nie pyta.
  expect(await tick(page, "12:00", 0, 0)).toBe(false);
  // Agent pracował przez całą nieobecność: to w ogóle nie jest przerwa.
  expect(await tick(page, "12:20", 15 * MIN, MIN)).toBe(false);
  expect(await tick(page, "12:25", 0, MIN)).toBe(false);
  await page.waitForTimeout(200);
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  const calls = await backend<any[]>(page, "calls");
  expect(calls.some((c) => c.cmd === "idle_resolve")).toBe(false);
  expect(await activeSession(page)).toMatchObject({ endedAt: null });
});
