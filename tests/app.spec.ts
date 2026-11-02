import { test, expect, addPlanned, type Page } from "./fixtures/test";
import { optionLabels, pick } from "./fixtures/searchSelect";

test.use({ timezoneId: "Europe/Warsaw" });
const add = async (page: Page, title: string) => {
  await addPlanned(page, title);
  await expect(
    page.locator(".task-title").filter({ hasText: title }),
  ).toBeVisible();
};
async function seed(page: Page) {
  await page.clock.setFixedTime(new Date("2026-10-12T12:00:00+02:00"));
  await page.goto("/");
  await page.evaluate(() => {
    const at = (s: string) => new Date(s).getTime();
    localStorage.setItem(
      "tuktuk-track-preview-v1",
      JSON.stringify({
        tasks: [
          {
            id: 1,
            title: "Projekt strony",
            done: false,
            createdAt: at("2026-10-10T08:00:00"),
          },
          {
            id: 2,
            title: "Spotkanie zespołu",
            done: true,
            createdAt: at("2026-10-10T08:00:00"),
          },
        ],
        sessions: [
          {
            id: 1,
            taskId: 1,
            startedAt: at("2026-10-10T23:00:00"),
            endedAt: at("2026-10-11T01:00:00"),
          },
          {
            id: 2,
            taskId: 2,
            startedAt: at("2026-10-11T09:00:00"),
            endedAt: at("2026-10-11T09:30:00"),
          },
          {
            id: 3,
            taskId: 1,
            startedAt: at("2026-10-11T10:00:00"),
            endedAt: at("2026-10-11T11:00:00"),
          },
          {
            id: 4,
            taskId: 1,
            startedAt: at("2026-10-12T11:45:00"),
            endedAt: null,
          },
        ],
      }),
    );
  });
  await page.reload();
}

test("task lifecycle, repeated sessions, persistence and deletion", async ({
  page,
}) => {
  await page.goto("/");
  await add(page, "Pierwsze zadanie");
  await add(page, "Drugie zadanie");
  await page.getByLabel("Start: Pierwsze zadanie", { exact: true }).click();
  await expect(page.locator(".focus-clock")).not.toHaveText("00:00:00", {
    timeout: 5000,
  });
  await page.getByLabel("Start: Drugie zadanie", { exact: true }).click();
  await expect(
    page.getByLabel("Stop: Drugie zadanie", { exact: true }),
  ).toBeVisible();
  await page.getByLabel("Start: Pierwsze zadanie", { exact: true }).click();
  await page.reload();
  await expect(
    page.getByLabel("Stop: Pierwsze zadanie", { exact: true }),
  ).toBeVisible();
  await expect(page.locator(".session-row")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Przedziały: Pierwsze zadanie", exact: true })
    .click();
  await expect(page.locator(".session-row")).toHaveCount(2);
  await expect(
    page.getByLabel("Edytuj przedział 3", { exact: true }),
  ).toBeDisabled();
  await page.getByLabel("Ukończ: Pierwsze zadanie", { exact: true }).click();
  await expect(page.locator(".active-bar")).toHaveCount(0);
  await page.getByRole("button", { name: "Ukończone", exact: true }).click();
  await page.getByLabel("Przywróć: Pierwsze zadanie", { exact: true }).click();
  await page.getByRole("button", { name: "Do zrobienia", exact: true }).click();
  await page.getByLabel("Edytuj: Pierwsze zadanie", { exact: true }).click();
  await page.getByLabel("Nazwa zadania").fill("Zmieniona nazwa");
  await page.getByRole("button", { name: "Zapisz zmiany" }).click();
  await page.getByLabel("Usuń: Zmieniona nazwa", { exact: true }).click();
  await page.getByRole("button", { name: "Anuluj", exact: true }).click();
  await expect(
    page.locator(".task-title").filter({ hasText: "Zmieniona nazwa" }),
  ).toBeVisible();
  await page.getByLabel("Usuń: Zmieniona nazwa", { exact: true }).click();
  await page.getByRole("button", { name: "Usuń zadanie", exact: true }).click();
  const stored = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("tuktuk-track-preview-v1")!),
  );
  expect(stored.tasks).toHaveLength(1);
  expect(stored.sessions).toHaveLength(1);
});

test("started tasks stay in progress after stop, reload and restore", async ({
  page,
}) => {
  await page.goto("/");
  await add(page, "Rozpoczęte zadanie");
  await add(page, "Nowe zadanie");
  const started = page
    .locator(".task-card")
    .filter({
      has: page.locator(".task-title", { hasText: "Rozpoczęte zadanie" }),
    });
  const fresh = page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: "Nowe zadanie" }) });
  await expect(page.locator(".task-status")).toHaveCount(0);
  // Odznaka „W trakcie” jest tylko przy trwającym pomiarze; po Stop zadanie
  // zostaje w grupie „Rozpoczęte”.
  const startedGroup = page.getByRole("region", {
    name: "Rozpoczęte",
    exact: true,
  });
  await page.getByLabel("Start: Rozpoczęte zadanie", { exact: true }).click();
  await expect(started.locator(".task-status")).toHaveText("W trakcie");
  await page.getByLabel("Stop: Rozpoczęte zadanie", { exact: true }).click();
  await expect(started.locator(".task-status")).toHaveCount(0);
  await expect(startedGroup).toContainText("Rozpoczęte zadanie");
  await page.reload();
  await expect(startedGroup).toContainText("Rozpoczęte zadanie");
  await expect(startedGroup).not.toContainText("Nowe zadanie");
  await page.getByLabel("Ukończ: Rozpoczęte zadanie", { exact: true }).click();
  await page.getByRole("button", { name: "Ukończone", exact: true }).click();
  await expect(started.locator(".task-status")).toHaveCount(0);
  await expect(started.locator(".done-label")).toHaveText("Gotowe");
  await page
    .getByLabel("Przywróć: Rozpoczęte zadanie", { exact: true })
    .click();
  await page.getByRole("button", { name: "Do zrobienia", exact: true }).click();
  await expect(startedGroup).toContainText("Rozpoczęte zadanie");
});

test("task groups prioritize started tasks and dashboard links to daily reports", async ({
  page,
}) => {
  await seed(page);
  await add(page, "Do zaplanowania nowe");
  await expect(page.locator(".task-group-heading")).toHaveText([
    "Rozpoczęte 1",
    "Nierozpoczęte 1",
  ]);
  await expect(page.locator(".task-card .task-title")).toHaveText([
    "Projekt strony",
    "Do zaplanowania nowe",
  ]);
  await page.getByLabel("Start: Do zaplanowania nowe", { exact: true }).click();
  await expect(page.locator(".task-group-heading")).toHaveText(["Rozpoczęte 2"]);
  await expect(page.locator(".task-card .task-title")).toHaveText([
    "Do zaplanowania nowe",
    "Projekt strony",
  ]);
  await page.getByRole("button", { name: "Wszystkie", exact: true }).click();
  await expect(page.locator(".task-group-heading")).toHaveText([
    "Rozpoczęte 2",
    "Ukończone 1",
  ]);
  await page.getByLabel("Szukaj zadań").fill("Spotkanie");
  await expect(page.locator(".task-group-heading")).toHaveText(["Ukończone 1"]);
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(page.locator(".dashboard-stats strong")).toHaveText([
    "00:15:00",
    "03:45:00",
    "03:45:00",
  ]);
  await expect(
    page.getByRole("region", { name: "Statusy zadań" }).locator("strong"),
  ).toHaveText(["2", "0", "1", "0"]);
  await page.screenshot({
    path: "test-results/dashboard-dark.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/dashboard-compact.png",
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Raport 2026-10-11: 02:30:00", exact: true })
    .click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-10-11");
  await expect(page.locator(".report-total")).toHaveText("02:30:00");
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await page.reload();
  await expect(
    page.getByRole("button", { name: "Zadania", exact: true }),
  ).toHaveAttribute("aria-current", "page");
});

test("dark theme persists, compact cards show intervals only when expanded", async ({
  page,
}) => {
  await seed(page);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator(".task-duration")).toHaveText("03:15:00");
  await expect(page.locator(".session-row")).toHaveCount(0);
  await expect(page.locator(".daily-summary strong")).toHaveText("00:15:00");
  await page
    .getByRole("button", { name: "Przedziały: Projekt strony", exact: true })
    .click();
  await expect(page.locator(".session-row")).toHaveCount(3);
  await expect(page.locator(".session-row").last()).toContainText("23:00");
  await expect(page.locator(".session-row").last()).toContainText("01:00");
  await page.screenshot({
    path: "test-results/tasks-dark.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Włącz tryb jasny" }).click();
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await page.screenshot({
    path: "test-results/tasks-light.png",
    fullPage: true,
  });
  await page.setViewportSize({ width: 760, height: 600 });
  await page
    .getByRole("button", { name: "Przedziały: Projekt strony", exact: true })
    .click();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/tasks-compact.png",
    fullPage: true,
  });
});

test("daily reports clip midnight, include completed tasks and navigate empty days", async ({
  page,
}) => {
  await seed(page);
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await expect(page.locator(".report-total")).toHaveText("00:15:00");
  await page.getByLabel("Dzień raportu").fill("2026-10-11");
  await expect(page.locator(".report-total")).toHaveText("02:30:00");
  await expect(page.locator(".report-card")).toHaveCount(2);
  await page
    .getByRole("button", {
      name: "Przedziały raportu: Projekt strony",
      exact: true,
    })
    .click();
  await expect(page.locator(".session-row")).toHaveCount(2);
  await expect(page.locator(".session-duration")).toHaveText([
    "01:00:00",
    "01:00:00",
  ]);
  await page.screenshot({
    path: "test-results/report-dark.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Poprzedni dzień" }).click();
  await expect(page.locator(".report-total")).toHaveText("01:00:00");
  await page.getByRole("button", { name: "Poprzedni dzień" }).click();
  await expect(page.locator(".report-total")).toHaveText("00:00:00");
  await expect(
    page.getByText("Brak zarejestrowanej pracy w tym dniu."),
  ).toBeVisible();
  await page.getByRole("button", { name: "Dzisiaj", exact: true }).click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-10-12");
  await page.getByRole("button", { name: "Następny dzień" }).click();
  await expect(page.locator(".report-total")).toHaveText("00:00:00");
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test("session editing validates overlaps and dates, updates totals and survives reload", async ({
  page,
}) => {
  await seed(page);
  await page
    .getByRole("button", { name: "Przedziały: Projekt strony", exact: true })
    .click();
  await page.getByLabel("Edytuj przedział 3", { exact: true }).click();
  await page.getByLabel("Początek", { exact: true }).fill("2026-10-11T09:15");
  await page.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(page.getByRole("alert")).toContainText("nakłada się");
  await page.getByLabel("Początek", { exact: true }).fill("2026-10-11T12:00");
  await page.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(page.getByRole("alert")).toContainText("późniejszy");
  await page.getByLabel("Początek", { exact: true }).fill("2026-10-13T09:00");
  await page.getByLabel("Koniec", { exact: true }).fill("2026-10-13T10:00");
  await page.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(page.getByRole("alert")).toContainText("przyszłości");
  await page.getByLabel("Początek", { exact: true }).fill("2026-10-11T09:30");
  await page.getByLabel("Koniec", { exact: true }).fill("2026-10-11T11:00");
  await page.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".task-duration")).toHaveText("03:45:00");
  await page.reload();
  await expect(page.locator(".task-duration")).toHaveText("03:45:00");
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByLabel("Dzień raportu").fill("2026-10-11");
  await expect(page.locator(".report-total")).toHaveText("03:00:00");
  await page
    .getByRole("button", {
      name: "Przedziały raportu: Projekt strony",
      exact: true,
    })
    .click();
  await page.getByLabel("Edytuj przedział 1", { exact: true }).click();
  await page.getByLabel("Koniec", { exact: true }).fill("2026-10-10T23:30");
  await page.getByRole("button", { name: "Zapisz przedział" }).click();
  await expect(page.locator(".report-total")).toHaveText("02:00:00");
  await page.getByRole("button", { name: "Poprzedni dzień" }).click();
  await expect(page.locator(".report-total")).toHaveText("00:30:00");
});

test("TS report sums daily intervals, copies decimal hours and supports manual copy", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await seed(page);
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByRole("button", { name: "Raport TS", exact: true }).click();
  const content = page.getByRole("textbox", { name: "Treść raportu TS" });
  await expect(content).toHaveValue(
    "Raport TS — 2026-10-12\nZadanie\tCzas (h)\nProjekt strony\t0,25\nRazem\t0,25",
  );
  await page.getByLabel("Dzień raportu").fill("2026-10-11");
  const expected =
    "Raport TS — 2026-10-11\nZadanie\tCzas (h)\nProjekt strony\t2,00\nSpotkanie zespołu\t0,50\nRazem\t2,50";
  await expect(content).toHaveValue(expected);
  await page.getByRole("button", { name: "Kopiuj raport" }).click();
  await expect(page.getByRole("status")).toHaveText("Skopiowano raport.");
  expect(
    (await page.evaluate(() => navigator.clipboard.readText())).replace(
      /\r\n/g,
      "\n",
    ),
  ).toBe(expected);
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({ path: "test-results/report-ts.png", fullPage: true });
  await page.evaluate(() => {
    Object.defineProperty(navigator.clipboard, "writeText", {
      value: () => Promise.reject(new Error("unavailable")),
    });
    document.execCommand = () => false;
  });
  await page.getByRole("button", { name: "Kopiuj raport" }).click();
  await expect(page.getByRole("status")).toContainText("Ctrl+C");
  expect(
    await content.evaluate((element: HTMLTextAreaElement) =>
      element.value.slice(element.selectionStart, element.selectionEnd),
    ),
  ).toBe(expected);
  await page.getByLabel("Dzień raportu").fill("2026-10-09");
  await expect(content).toHaveValue(
    "Raport TS — 2026-10-09\nZadanie\tCzas (h)\nRazem\t0,00",
  );
});

for (const [day, expected] of [
  ["2026-03-29", "23:00:00"],
  ["2026-10-25", "25:00:00"],
]) {
  test(`daily reports respect local DST on ${day}`, async ({ page }) => {
    await page.goto("/");
    await page.evaluate((day) => {
      const start = new Date(`${day}T00:00:00`);
      const end = new Date(start);
      end.setDate(end.getDate() + 1);
      localStorage.setItem(
        "tuktuk-track-preview-v1",
        JSON.stringify({
          tasks: [
            { id: 1, title: "Zmiana czasu", done: true, createdAt: +start },
          ],
          sessions: [{ id: 1, taskId: 1, startedAt: +start, endedAt: +end }],
        }),
      );
    }, day);
    await page.reload();
    await page
      .getByRole("button", { name: "Raporty dzienne", exact: true })
      .click();
    await page.getByLabel("Dzień raportu").fill(day);
    await expect(page.locator(".report-total")).toHaveText(expected);
    await page.getByRole("button", { name: "Następny dzień" }).click();
    await expect(page.locator(".report-total")).toHaveText("00:00:00");
  });
}

test("splitting onto an archived task takes it out of the archive", async ({
  page,
}) => {
  await seed(page);
  await page.evaluate(() => {
    const data = JSON.parse(localStorage.getItem("tuktuk-track-preview-v1")!);
    data.tasks.push({
      id: 3,
      title: "Stary projekt",
      done: false,
      archived: true,
      createdAt: new Date("2026-10-01T08:00:00").getTime(),
    });
    localStorage.setItem("tuktuk-track-preview-v1", JSON.stringify(data));
  });
  await page.reload();
  await page.getByRole("button", { name: "Podziel bieżący pomiar" }).click();
  await page.getByLabel("Moment podziału").fill("2026-10-12T11:50");
  await pick(
    page.getByLabel("Zadanie drugiej części"),
    "Stary projekt (archiwum)",
  );
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "Podziel przedział" })
    .click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".active-title")).toHaveText("Stary projekt");
  const archived = await page.evaluate(
    () =>
      JSON.parse(localStorage.getItem("tuktuk-track-preview-v1")!).tasks.find(
        (t: { id: number }) => t.id === 3,
      ).archived,
  );
  expect(archived).toBe(false);
});

test("splitting the active measurement from the bar keeps the timer on the new task", async ({
  page,
}) => {
  await seed(page);
  await add(page, "Przegląd kodu");
  await page.getByRole("button", { name: "Podziel bieżący pomiar" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByRole("tab", { name: "Podziel" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByLabel("Moment podziału")).toHaveValue(
    "2026-10-12T11:52:30",
  );
  await page.getByLabel("Moment podziału").fill("2026-10-12T12:30");
  await expect(
    dialog.getByRole("button", { name: "Podziel przedział" }),
  ).toBeDisabled();
  await page.getByLabel("Moment podziału").fill("2026-10-12T11:50");
  await pick(page.getByLabel("Zadanie drugiej części"), "Przegląd kodu");
  expect(
    await optionLabels(page.getByLabel("Zadanie drugiej części")),
  ).not.toContain("Spotkanie zespołu");
  const preview = page.getByLabel("Podgląd podziału").locator("li");
  await expect(preview.nth(0)).toContainText("Projekt strony");
  await expect(preview.nth(0)).toContainText("00:05:00");
  await expect(preview.nth(1)).toContainText("Przegląd kodu");
  await expect(preview.nth(1)).toContainText("00:10:00");
  await dialog.getByRole("button", { name: "Podziel przedział" }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.locator(".active-title")).toHaveText("Przegląd kodu");
  await expect(page.locator(".focus-clock")).toHaveText("00:10:00");
  await expect(
    page.getByLabel("Stop: Przegląd kodu", { exact: true }),
  ).toBeVisible();
  await page.reload();
  await expect(page.locator(".active-title")).toHaveText("Przegląd kodu");
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await expect(page.locator(".report-total")).toHaveText("00:15:00");
  await expect(
    page.locator(".report-card").filter({ hasText: "Projekt strony" }),
  ).toContainText("00:05:00");
  await expect(
    page.locator(".report-card").filter({ hasText: "Przegląd kodu" }),
  ).toContainText("00:10:00");
});
