import { test, expect, type Page } from "./fixtures/test";
import {
  NOW,
  SETTINGS_KEY,
  TODAY,
  at,
  ids,
  openPreview,
  seed,
  stored,
} from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * B13: Rozpocznij, zakończ i wznów dzień. Dane z `fixtures/seed.ts`: dziś
 * czwartek 15.10.2026 12:00. Zegar strony stoi (`setFixedTime`); kolejne
 * godziny ustawia `page.clock.setFixedTime`.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const review = (page: Page) =>
  page.getByRole("region", { name: "Przegląd dnia" });
const controls = (page: Page) =>
  page.getByRole("group", { name: "Stan dnia" });
const time = (page: Page, hhmm: string, day = TODAY) =>
  page.clock.setFixedTime(new Date(`${day}T${hhmm}:00+02:00`));
const closeHeading = (page: Page) =>
  page.getByRole("heading", { name: /^Zamknięcie dnia/ });
const marks = async (page: Page) => (await stored(page)).dayMarks ?? [];
const activeSessions = async (page: Page) =>
  (await stored(page)).sessions.filter(
    (s: { endedAt: number | null }) => s.endedAt === null,
  );

async function openMorning(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
}

test("Rano: Rozpocznij → Zakończ bez pomiaru → Wznów", async ({ page }) => {
  await openPreview(page, { now: `${TODAY}T08:05:00+02:00` });
  await openMorning(page);
  await controls(page).getByRole("button", { name: "Rozpocznij dzień" }).click();
  await expect(review(page).getByText("Dzień rozpoczęty o 08:05")).toBeVisible();
  // Rozpoczęcie zapisuje też przegląd dnia (powiadomienie, „nowe” z Jiry).
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
  ).toEqual({
    morningReviewDay: TODAY,
    morningReviewAt: at(TODAY, "08:05"),
  });
  await expect(
    controls(page).getByRole("button", { name: "Rozpocznij dzień" }),
  ).toHaveCount(0);

  await time(page, "16:40");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  // Bez pomiaru: bez pytania, od razu Zamknięcie dnia z godzinami dnia.
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(closeHeading(page)).toBeVisible();
  await expect(page.getByText("Dzień: 08:05–16:40", { exact: true })).toBeVisible();
  await expect(controls(page).getByRole("status").first()).toHaveText(
    /^Dzień zakończony 16:40 · \d+:\d\d$/,
  );
  expect(await marks(page)).toEqual([
    {
      day: TODAY,
      startedAt: at(TODAY, "08:05"),
      endedAt: at(TODAY, "16:40"),
      resumed: 0,
    },
  ]);
  // „Oznacz dzień jako sprawdzony” zostaje bez zmian obok.
  await expect(
    page.getByRole("button", { name: "Oznacz dzień jako sprawdzony" }),
  ).toBeVisible();

  await openMorning(page);
  await controls(page).getByRole("button", { name: "Wznów dzień" }).click();
  await expect(
    controls(page).getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  await nav(page, "Zamknięcie dnia");
  await expect(page.getByText("Dzień: od 08:05 (wznawiany)")).toBeVisible();
});

test("Rano: Zakończ dzień z trwającym pomiarem pyta i zatrzymuje tylko po potwierdzeniu", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await openMorning(page);
  await controls(page).getByRole("button", { name: "Rozpocznij dzień" }).click();
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  const dialog = page.getByRole("dialog", { name: "Zakończyć dzień?" });
  await expect(dialog).toContainText(
    "Zakończyć dzień i zatrzymać pomiar »Analiza importu«?",
  );
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await activeSessions(page)).toHaveLength(1);
  expect((await marks(page))[0].endedAt).toBeNull();
  await expect(review(page)).toBeVisible();

  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await dialog
    .getByRole("button", { name: "Zakończ dzień i zatrzymaj pomiar" })
    .click();
  await expect(closeHeading(page)).toBeVisible();
  expect(await activeSessions(page)).toHaveLength(0);
  const data = await stored(page);
  expect(
    data.sessions.find((s: { taskId: number }) => s.taskId === ids.analysis)
      .endedAt,
  ).toBe(Date.parse(NOW));
  expect(data.dayMarks[0].endedAt).toBe(Date.parse(NOW));
  // „Wznów pomiar »X«” wznawia dzień i ostatnio mierzone zadanie.
  await controls(page)
    .getByRole("button", { name: "Wznów pomiar »Analiza importu«" })
    .click();
  await expect(
    controls(page).getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  const active = await activeSessions(page);
  expect(active).toHaveLength(1);
  expect(active[0].taskId).toBe(ids.analysis);
});

test("pasek: Zakończ dzień przy pomiarze, „Dzień zakończony” i Wznów dzień", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await nav(page, "Dashboard");
  const bar = page.getByRole("group", { name: "Bieżący pomiar w pasku" });
  await bar.getByRole("button", { name: "Zakończ dzień" }).click();
  const dialog = page.getByRole("dialog", { name: "Zakończyć dzień?" });
  await dialog
    .getByRole("button", { name: "Zakończ dzień i zatrzymaj pomiar" })
    .click();
  await expect(closeHeading(page)).toBeVisible();
  await expect(bar).toHaveCount(0);
  const ended = page.getByRole("group", { name: "Dzień zakończony w pasku" });
  await expect(ended).toHaveText("Dzień zakończony 12:00 · 3:30");
  // Dzień nie był rozpoczęty: początek z pierwszego pomiaru dnia.
  expect((await marks(page))[0].startedAt).toBe(at(TODAY, "08:00"));
  // Widoczne też na liście zadań.
  await nav(page, "Zadania");
  await expect(ended).toBeVisible();
  await ended.getByRole("button", { name: "Wznów dzień" }).click();
  await expect(ended).toHaveCount(0);
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  // Pomiar nie wrócił sam.
  expect(await activeSessions(page)).toHaveLength(0);
});

test("Zamknięcie dnia: Zakończ bez rozpoczęcia i Wznów dzień", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await expect(
    controls(page).getByRole("button", { name: "Wznów dzień" }),
  ).toBeVisible();
  await expect(page.getByText(/^Dzień: .*–12:00$/)).toBeVisible();
  await controls(page).getByRole("button", { name: "Wznów dzień" }).click();
  await expect(
    controls(page).getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  await expect(page.getByText(/\(wznawiany\)$/)).toBeVisible();
  // Inny dzień niż dziś: bez przycisków dnia.
  await page.getByRole("button", { name: "Poprzedni okres" }).click();
  await expect(controls(page)).toHaveCount(0);
});

test("Start po zakończeniu dnia pyta „Wznowić dzień?”", async ({ page }) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await nav(page, "Zadania");
  const start = page.getByRole("button", {
    name: "Start: Code review",
    exact: true,
  });
  await start.click();
  const dialog = page.getByRole("dialog", { name: "Wznowić dzień?" });
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await activeSessions(page)).toHaveLength(0);
  expect((await marks(page))[0].endedAt).not.toBeNull();

  await start.click();
  await dialog.getByRole("button", { name: "Wznów i start" }).click();
  await expect(
    page.getByRole("button", { name: "Stop: Code review", exact: true }),
  ).toBeVisible();
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  // Wznowiony dzień nie pyta drugi raz.
  await page.getByRole("button", { name: "Stop: Code review", exact: true }).click();
  await start.click();
  await expect(dialog).toHaveCount(0);
  expect(await activeSessions(page)).toHaveLength(1);
});

test("po północy nie ma „Wznów dzień”, a nowy dzień zaczyna się od Rozpocznij", async ({
  page,
}) => {
  await openPreview(page, { now: `${TODAY}T23:50:00+02:00` });
  await openMorning(page);
  await controls(page).getByRole("button", { name: "Rozpocznij dzień" }).click();
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  const ended = page.getByRole("group", { name: "Dzień zakończony w pasku" });
  await expect(ended).toBeVisible();
  await time(page, "00:10", "2026-10-16");
  await expect(ended).toHaveCount(0);
  await openMorning(page);
  await expect(
    controls(page).getByRole("button", { name: "Rozpocznij dzień" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Wznów dzień" })).toHaveCount(0);
  // Wczorajszy dzień zostaje zakończony, a jego wznowienie jest odrzucane.
  const error = await page.evaluate(async (day) => {
    const m = await import("/src/days.ts");
    return m.dayResume(day).then(
      () => "",
      (e: Error) => e.message,
    );
  }, TODAY);
  expect(error).toBe(
    "Rozpocząć, zakończyć i wznowić można tylko dzisiejszy dzień.",
  );
  const future = await page.evaluate(async () => {
    const m = await import("/src/days.ts");
    return m.dayStart("2026-10-17").then(
      () => "",
      (e: Error) => e.message,
    );
  });
  expect(future).toBe("Nie można zmienić stanu dnia w przyszłości.");
});

test("godziny dnia w Raporcie dnia i w Miesiącu", async ({ page }) => {
  const data = {
    ...seed(),
    dayMarks: [
      {
        day: "2026-10-14",
        startedAt: at("2026-10-14", "08:05"),
        endedAt: at("2026-10-14", "16:40"),
        resumed: 1,
      },
      {
        day: "2026-10-13",
        startedAt: at("2026-10-13", "09:00"),
        endedAt: at("2026-10-13", "17:15"),
        resumed: 0,
      },
    ],
  };
  await openPreview(page, { data });
  await nav(page, "Raporty dzienne");
  await page.getByLabel("Dzień raportu").fill("2026-10-14");
  await expect(
    page
      .getByRole("region", { name: "Podsumowanie dnia" })
      .getByText("Dzień: 08:05–16:40 (wznawiany)"),
  ).toBeVisible();
  await page.getByLabel("Dzień raportu").fill("2026-10-13");
  await expect(page.getByText("Dzień: 09:00–17:15", { exact: true })).toBeVisible();
  await page.getByLabel("Dzień raportu").fill("2026-10-12");
  await expect(page.locator(".day-hours")).toHaveCount(0);

  await nav(page, "Miesiąc");
  const row = (day: string) =>
    page.getByRole("button", { name: new RegExp(`^Raport ${day}:`) });
  await expect(row("2026-10-14").locator(".monthly-hours")).toHaveText(
    "08:05–16:40",
  );
  await expect(row("2026-10-13").locator(".monthly-hours")).toHaveText(
    "09:00–17:15",
  );
  await expect(row("2026-10-12").locator(".monthly-hours")).toHaveText("–");
  await expect(page.locator(".monthly-day-head")).toContainText("Godziny dnia");
});

test("widget: „Dzień zakończony” i Wznów dzień", async ({ page }) => {
  await openPreview(page, {
    data: {
      ...seed(),
      dayMarks: [
        {
          day: TODAY,
          startedAt: at(TODAY, "08:00"),
          endedAt: at(TODAY, "11:30"),
          resumed: 0,
        },
      ],
    },
  });
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  await expect(widget.locator("strong")).toHaveText(
    /^Dzień zakończony 11:30 · \d+:\d\d$/,
  );
  await widget.getByRole("button", { name: "Wznów dzień" }).click();
  await expect(widget.locator("strong")).toHaveText("Brak pomiaru");
  await expect(
    widget.getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  // Bez pomiaru „Zakończ dzień” z widgetu kończy dzień (w aplikacji przez
  // główne okno z dialogiem i Zamknięciem dnia).
  await widget.getByRole("button", { name: "Zakończ dzień" }).click();
  await expect(widget.getByRole("button", { name: "Wznów dzień" })).toBeVisible();
});

test("Zamknięcie dnia: Zakończ dzień z trwającym pomiarem pyta, potem Wznów", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  const dialog = page.getByRole("dialog", { name: "Zakończyć dzień?" });
  await expect(dialog).toContainText(
    "Zakończyć dzień i zatrzymać pomiar »Analiza importu«?",
  );
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await activeSessions(page)).toHaveLength(1);
  expect(await marks(page)).toEqual([]);

  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await dialog
    .getByRole("button", { name: "Zakończ dzień i zatrzymaj pomiar" })
    .click();
  await expect(dialog).toHaveCount(0);
  await expect(closeHeading(page)).toBeVisible();
  expect(await activeSessions(page)).toHaveLength(0);
  expect((await marks(page))[0].endedAt).toBe(Date.parse(NOW));

  await controls(page).getByRole("button", { name: "Wznów dzień" }).click();
  await expect(
    controls(page).getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  // Wznowienie dnia nie wznawia pomiaru.
  expect(await activeSessions(page)).toHaveLength(0);
});

test("propozycje przełączenia milczą po Zakończ dzień i wracają po Wznów", async ({
  page,
}) => {
  const now = Date.parse(NOW);
  await openPreview(page, {
    agents: [
      {
        kind: "claude",
        name: "Import",
        cwd: "C:\Projekty\wms",
        since: now - 5 * 60_000,
        branch: "feature/WMS-12-import",
      },
    ],
    active: { task: ids.review, from: "11:00" },
  });
  const banner = page.getByRole("region", { name: "Propozycja przełączenia" });
  await expect(banner).toBeVisible();
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await page
    .getByRole("dialog", { name: "Zakończyć dzień?" })
    .getByRole("button", { name: "Zakończ dzień i zatrzymaj pomiar" })
    .click();
  await expect(closeHeading(page)).toBeVisible();
  await expect(banner).toHaveCount(0);
  // Po odświeżeniu też (stan z danych, nie z pamięci okna).
  await page.reload();
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await expect(banner).toHaveCount(0);
  await controls(page).getByRole("button", { name: "Wznów dzień" }).click();
  await expect(banner).toBeVisible();
});

test("nieudany start po „Wznów i start” zostawia okno ze wznowionym dniem", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await controls(page).getByRole("button", { name: "Zakończ dzień" }).click();
  await expect(
    controls(page).getByRole("button", { name: "Wznów dzień" }),
  ).toBeVisible();
  // Start zrobionego zadania: dzień się wznawia, a start zwraca błąd.
  const result = page.evaluate(async (id) => {
    const m = await import("/src/store.ts");
    return m.change("start", id).then(
      () => "",
      (e: Error) => e.message,
    );
  }, ids.docs);
  await page
    .getByRole("dialog", { name: "Wznowić dzień?" })
    .getByRole("button", { name: "Wznów i start" })
    .click();
  expect(await result).toBe("Najpierw przywróć zadanie do zrobienia.");
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  await expect(
    controls(page).getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
  await expect(
    page.getByRole("group", { name: "Dzień zakończony w pasku" }),
  ).toHaveCount(0);
});

test("widget: Start po zakończeniu dnia pyta „Wznowić dzień?”", async ({
  page,
}) => {
  await openPreview(page, {
    data: {
      ...seed(),
      dayMarks: [
        {
          day: TODAY,
          startedAt: at(TODAY, "08:00"),
          endedAt: at(TODAY, "11:30"),
          resumed: 0,
        },
      ],
    },
  });
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  const start = widget.getByRole("button", { name: "Wznów ostatnie zadanie" });
  const dialog = page.getByRole("dialog", { name: "Wznowić dzień?" });
  await start.click();
  await dialog.getByRole("button", { name: "Anuluj" }).click();
  await expect(dialog).toHaveCount(0);
  expect(await activeSessions(page)).toHaveLength(0);
  expect((await marks(page))[0].endedAt).not.toBeNull();
  await expect(widget.locator("strong")).toHaveText(/^Dzień zakończony 11:30/);

  await start.click();
  await dialog.getByRole("button", { name: "Wznów i start" }).click();
  await expect(
    widget.getByRole("button", { name: "Zatrzymaj pomiar" }),
  ).toBeVisible();
  expect(await activeSessions(page)).toHaveLength(1);
  expect((await marks(page))[0]).toMatchObject({ endedAt: null, resumed: 1 });
  await expect(
    widget.getByRole("button", { name: "Zakończ dzień" }),
  ).toBeVisible();
});

test.describe("aplikacja: start spoza głównego okna po zakończeniu dnia", () => {
  const ended = () => ({
    ...seed(),
    dayMarks: [
      {
        day: TODAY,
        startedAt: at(TODAY, "08:00"),
        endedAt: at(TODAY, "11:30"),
        resumed: 0,
      },
    ],
  });
  const commands = async (page: Page) =>
    (await backend<{ cmd: string; args: any }[]>(page, "calls")).filter((c) =>
      [
        "day_start_request",
        "day_start_answer",
        "mutate",
        "toggle_measurement",
        "focus_action",
      ].includes(c.cmd),
    );

  test("widget prosi główne okno o „Wznowić dzień?” zamiast startować", async ({
    page,
  }) => {
    await openDesktop(page, { window: "widget", data: ended() });
    const widget = page.locator("#widget");
    await expect(widget.locator("strong")).toHaveText(
      /^Dzień zakończony 11:30/,
    );
    await widget
      .getByRole("button", { name: "Wznów ostatnie zadanie" })
      .click();
    await expect(widget.getByRole("status")).toHaveText(
      "Wznowić dzień? Odpowiedz w głównym oknie",
    );
    await widget
      .getByRole("button", { name: "Rozpocznij blok skupienia" })
      .click();
    expect(await commands(page)).toEqual([
      { cmd: "day_start_request", args: { taskId: ids.analysis, focus: null } },
      {
        cmd: "day_start_request",
        args: { taskId: ids.analysis, focus: "start" },
      },
    ]);
    await expect(page.getByRole("dialog")).toHaveCount(0);
  });

  test("główne okno pyta po day-start-request i odpowiada backendowi", async ({
    page,
  }) => {
    await openDesktop(page, { data: ended() });
    const emit = (id: number) =>
      page.evaluate(
        (n) => (window as any).__tauriMock.emit("day-start-request", n),
        id,
      );
    const dialog = page.getByRole("dialog", { name: "Wznowić dzień?" });
    await emit(4);
    await dialog.getByRole("button", { name: "Anuluj" }).click();
    await expect(dialog).toHaveCount(0);
    await emit(5);
    await dialog.getByRole("button", { name: "Wznów i start" }).click();
    await expect(dialog).toHaveCount(0);
    expect(await commands(page)).toEqual([
      { cmd: "day_start_answer", args: { id: 4, resume: false } },
      { cmd: "day_start_answer", args: { id: 5, resume: true } },
    ]);
  });
});
