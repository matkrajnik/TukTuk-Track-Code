import { test, expect, type Page } from "./fixtures/test";
import { at as atDay, ids, seed as seedData, TODAY } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw" });

// B3: akcje przy luce w zapisie, luki krótsze niż sekunda, „Przytnij i wznów”
// z banera bezczynności i poprawka startu trwającego pomiaru.
const key = "tuktuk-track-preview-v1";
const at = (time: string) => new Date(`2026-10-14T${time}+02:00`).getTime();
const tasks = [
  { id: 1, title: "Analiza", done: false, createdAt: 0 },
  { id: 2, title: "Wdrożenie", done: false, createdAt: 0 },
];

// Środa 14 października 2026, 12:00 czasu lokalnego.
async function open(page: Page, sessions: object[], extra: object = {}) {
  await page.clock.setFixedTime(new Date("2026-10-14T12:00:00+02:00"));
  await page.goto("/");
  await page.evaluate(
    ([k, value]) => localStorage.setItem(k, value),
    [key, JSON.stringify({ tasks, sessions, ...extra })],
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  return page.getByRole("region", { name: "Oś dnia" });
}
const storedSessions = (page: Page) =>
  page.evaluate(
    (k) =>
      JSON.parse(localStorage.getItem(k)!).sessions as {
        id: number;
        taskId: number;
        startedAt: number;
        endedAt: number | null;
      }[],
    key,
  );
const byId = async (page: Page) =>
  Object.fromEntries((await storedSessions(page)).map((s) => [s.id, s]));

/**
 * Analiza 09:00:00–09:58:42, luka 00:03:49, Wdrożenie 10:02:31–11:00:00,
 * Analiza od 11:00:00.400 (luka 0,4 s, niewidoczna) do 11:30:00.
 */
const gapDay = [
  { id: 1, taskId: 1, startedAt: at("09:00:00"), endedAt: at("09:58:42") },
  { id: 2, taskId: 2, startedAt: at("10:02:31"), endedAt: at("11:00:00") },
  {
    id: 3,
    taskId: 1,
    startedAt: at("11:00:00") + 400,
    endedAt: at("11:30:00"),
  },
];

test("gap in the record can be joined to the previous or next measurement or split in half", async ({
  page,
}) => {
  const expected: Record<string, [number, number]> = {
    "Dołącz do poprzedniego": [at("10:02:31"), at("10:02:31")],
    "Dołącz do następnego": [at("09:58:42"), at("09:58:42")],
    // 3 min 49 s po połowie: 09:58:42 + 1 min 54,5 s.
    "Podziel po pół": [at("09:58:42") + 114_500, at("09:58:42") + 114_500],
  };
  for (const [action, [end1, start2]] of Object.entries(expected)) {
    const card = await open(page, gapDay);
    // Jedna luka: 0,4 s przed trzecim pomiarem nie jest pokazywana.
    await expect(card.locator(".timeline-row")).toHaveCount(4);
    const gap = card.locator(".timeline-row.gap");
    await expect(gap).toHaveCount(1);
    await expect(gap).toContainText("09:58:42 → 10:02:31");
    await expect(gap).toContainText("Luka w zapisie");
    await expect(gap).toContainText("00:03:49");
    await gap
      .getByRole("group", { name: "Luka 09:58:42 → 10:02:31" })
      .getByRole("button", { name: action })
      .click();
    await expect(card.locator(".timeline-row.gap")).toHaveCount(0);
    await expect(card.locator(".timeline-row")).toHaveCount(3);
    const s = await byId(page);
    expect([s[1].endedAt, s[2].startedAt], action).toEqual([end1, start2]);
    // Pozostałe granice bez zmian.
    expect([s[1].startedAt, s[2].endedAt, s[3].startedAt]).toEqual([
      at("09:00:00"),
      at("11:00:00"),
      at("11:00:00") + 400,
    ]);
  }
});

test("gap holding only an interval of a trashed task offers no actions", async ({
  page,
}) => {
  // Przedział zadania w koszu nie jest na Osi dnia, ale wróci przy
  // przywróceniu, więc blokuje lukę (jak db::fill_gap) — bez przycisków.
  const card = await open(page, gapDay.slice(0, 2), {
    trash: [{ id: 3, title: "Usunięte", done: false, createdAt: 0 }],
    trashSessions: [
      { id: 4, taskId: 3, startedAt: at("10:00:00"), endedAt: at("10:01:00") },
    ],
  });
  const gap = card.locator(".timeline-row.gap");
  await expect(gap).toHaveCount(1);
  await expect(gap).toContainText("09:58:42 → 10:02:31");
  await expect(gap).toContainText("pomiar zadania z kosza");
  await expect(gap.getByRole("button")).toHaveCount(0);
  // Podgląd odrzuca zapełnienie tak samo jak Rust.
  const error = await page.evaluate(async () => {
    const { fillGap } = await import("/src/store.ts");
    return fillGap(1, 2, "previous").then(
      () => "",
      (e: Error) => e.message,
    );
  });
  expect(error).toContain("pomiar zadania z kosza");
  expect((await byId(page))[1].endedAt).toBe(at("09:58:42"));
});

test("zero-length gap after editing an interval is not listed", async ({
  page,
}) => {
  const card = await open(page, [
    { id: 1, taskId: 1, startedAt: at("08:00:00"), endedAt: at("09:00:00") },
    { id: 2, taskId: 2, startedAt: at("10:00:00"), endedAt: at("11:00:00") },
  ]);
  await expect(card.locator(".timeline-row.gap")).toHaveCount(1);
  // Edycja końca pierwszego przedziału na początek drugiego: luka 0 s znika.
  await card
    .locator(".timeline-row")
    .filter({ hasText: "08:00:00" })
    .getByRole("button", { name: "Edytuj" })
    .click();
  const dialog = page.getByRole("dialog", { name: "Edytuj przedział" });
  await dialog.getByLabel("Koniec").fill("2026-10-14T10:00");
  await dialog.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(card.locator(".timeline-row")).toHaveCount(2);
  await expect(card.locator(".timeline-row.gap")).toHaveCount(0);
  await expect(card).not.toContainText("Luka w zapisie");
});

test("start of a running measurement can be corrected without stopping it", async ({
  page,
}) => {
  const card = await open(page, [
    { id: 1, taskId: 1, startedAt: at("09:00:00"), endedAt: at("10:00:00") },
    { id: 2, taskId: 2, startedAt: at("11:20:00"), endedAt: null },
  ]);
  await card
    .locator(".timeline-row")
    .filter({ hasText: "Wdrożenie" })
    .getByRole("button", { name: "Edytuj" })
    .click();
  const dialog = page.getByRole("dialog", { name: "Edytuj przedział" });
  await expect(dialog.getByLabel("Koniec")).toHaveCount(0);
  const start = dialog.getByLabel("Początek");
  await expect(start).toHaveValue("2026-10-14T11:20");
  // Nakładanie na poprzedni pomiar i start w przyszłości są odrzucane.
  await start.fill("2026-10-14T09:30");
  await dialog.getByRole("button", { name: "Zapisz początek" }).click();
  await expect(dialog.getByRole("alert")).toContainText("nakłada");
  await start.fill("2026-10-14T12:30");
  await dialog.getByRole("button", { name: "Zapisz początek" }).click();
  await expect(dialog.getByRole("alert")).toContainText("wcześniejszy");
  await start.fill("2026-10-14T10:00");
  await dialog.getByRole("button", { name: "Zapisz początek" }).click();
  await expect(dialog).toHaveCount(0);
  let s = await byId(page);
  expect([s[2].startedAt, s[2].endedAt]).toEqual([at("10:00:00"), null]);
  await expect(
    card.locator(".timeline-row").filter({ hasText: "Wdrożenie" }),
  ).toContainText("10:00:00 → teraz");

  // To samo z kafelka zadania: „Popraw start przedziału”.
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(
    page.getByLabel("Stop: Wdrożenie", { exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Przedziały: Wdrożenie", exact: true })
    .click();
  await expect(
    page.getByLabel("Edytuj przedział 2", { exact: true }),
  ).toBeDisabled();
  await page.getByLabel("Popraw start przedziału 2", { exact: true }).click();
  await dialog.getByLabel("Początek").fill("2026-10-14T10:30");
  await dialog.getByRole("button", { name: "Zapisz początek" }).click();
  await expect(dialog).toHaveCount(0);
  s = await byId(page);
  expect([s[2].startedAt, s[2].endedAt]).toEqual([at("10:30:00"), null]);
  expect([s[1].startedAt, s[1].endedAt]).toEqual([
    at("09:00:00"),
    at("10:00:00"),
  ]);
});

test("idle banner: trim and resume cuts out the break and keeps measuring the same task", async ({
  page,
}) => {
  const MIN = 60_000;
  const banner = page
    .getByRole("alert")
    .filter({ hasText: "Wracasz po przerwie?" });
  const tick = async (time: string, userIdle: number) => {
    await page.clock.setFixedTime(atDay(TODAY, time));
    return backend<boolean>(page, "idleTick", { userIdle });
  };
  // Pomiar „Analiza importu” od 11:00, brak aktywności 11:30–12:00.
  await openDesktop(page, {
    data: seedData({ active: { task: ids.analysis, from: "11:00" } }),
    now: atDay(TODAY, "11:30"),
  });
  expect(await tick("11:30", 0)).toBe(false);
  expect(await tick("11:50", 20 * MIN)).toBe(false);
  expect(await tick("12:00", 0)).toBe(true);
  await banner.getByRole("button", { name: "Przytnij i wznów" }).click();
  await expect(banner).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  const sessions = (await backend<any>(page, "snapshot")).sessions
    .filter(
      (s: any) =>
        s.taskId === ids.analysis && s.startedAt >= atDay(TODAY, "11:00"),
    )
    .map((s: any) => [s.startedAt, s.endedAt])
    .sort((a: number[], b: number[]) => a[0] - b[0]);
  expect(sessions).toEqual([
    [atDay(TODAY, "11:00"), atDay(TODAY, "11:30")],
    [atDay(TODAY, "12:00"), null],
  ]);
  const calls = await backend<any[]>(page, "calls");
  expect(calls.filter((c) => c.cmd === "idle_resolve")).toEqual([
    { cmd: "idle_resolve", args: { trim: true, resume: true } },
  ]);
});
