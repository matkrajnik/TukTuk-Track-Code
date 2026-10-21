import { test, expect, type Page } from "./fixtures/test";

test.use({ timezoneId: "Europe/Warsaw" });

type SeedTask = {
  id: number;
  title: string;
  notes?: Record<string, string>;
};
type SeedSession = [taskId: number, start: string, end: string | null];

async function seed(
  page: Page,
  now: string,
  tasks: SeedTask[],
  sessions: SeedSession[],
) {
  await page.clock.setFixedTime(new Date(now));
  await page.goto("/");
  await page.evaluate(
    ({ tasks, sessions }) => {
      const at = (s: string) => new Date(s).getTime();
      localStorage.removeItem("todo-track-report-days-v1");
      localStorage.setItem(
        "todo-track-preview-v1",
        JSON.stringify({
          tasks: tasks.map((t) => ({
            id: t.id,
            title: t.title,
            done: false,
            createdAt: at("2026-09-01T08:00:00"),
            details: t.notes ? { notes: t.notes } : undefined,
          })),
          sessions: sessions.map(([taskId, start, end], i) => ({
            id: i + 1,
            taskId,
            startedAt: at(start),
            endedAt: end === null ? null : at(end),
          })),
        }),
      );
    },
    { tasks, sessions },
  );
  await page.reload();
}

const openMultiDay = async (page: Page) => {
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByRole("button", { name: "Wybrane dni", exact: true }).click();
};
const day = (page: Page, label: string) =>
  page.getByRole("button", { name: label, exact: true });

test("TS report from selected days: two days, both layouts, clipboard and month switch", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await seed(
    page,
    "2026-10-27T12:00:00+01:00",
    [
      {
        id: 1,
        title: "Projekt strony",
        notes: { "2026-10-26": "Import CSV", "2026-10-27": "Testy" },
      },
      { id: 2, title: "Spotkanie zespołu" },
    ],
    [
      [1, "2026-10-26T09:00:00", "2026-10-26T11:00:00"],
      [2, "2026-10-26T11:00:00", "2026-10-26T11:30:00"],
      [1, "2026-10-27T08:00:00", "2026-10-27T09:15:00"],
    ],
  );
  await openMultiDay(page);
  await expect(page.locator(".month-label")).toHaveText("październik 2026");
  const content = page.getByRole("textbox", {
    name: "Treść raportu TS z wybranych dni",
  });
  await expect(day(page, "26 października 2026")).toHaveAttribute(
    "title",
    "Praca: 02:30:00",
  );
  await expect(
    day(page, "26 października 2026").locator(".work-dot"),
  ).toBeVisible();
  await expect(
    day(page, "25 października 2026").locator(".work-dot"),
  ).toHaveCount(0);

  await day(page, "27 października 2026").click();
  await day(page, "26 października 2026").click();
  await expect(day(page, "26 października 2026")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(page.getByLabel("Suma wybranych dni")).toHaveText("03:45:00");
  const list = page.getByRole("list", { name: "Wybrane dni" });
  await expect(list.getByRole("listitem")).toHaveCount(2);
  await expect(list.getByRole("listitem").first()).toContainText("2026-10-26");

  const byDay = [
    "Raport TS — 2026-10-26 – 2026-10-27 (2 dni)",
    "Dzień\tZadanie\tCzas (h)",
    "2026-10-26\tProjekt strony — Import CSV\t2,00",
    "2026-10-26\tSpotkanie zespołu\t0,50",
    "Razem 2026-10-26\t\t2,50",
    "2026-10-27\tProjekt strony — Testy\t1,25",
    "Razem 2026-10-27\t\t1,25",
    "Razem\t\t3,75",
  ].join("\n");
  await expect(content).toHaveValue(byDay);
  await page.getByRole("button", { name: "Kopiuj raport" }).click();
  await expect(page.getByRole("status")).toHaveText("Skopiowano raport.");
  const clipboard = async () =>
    (await page.evaluate(() => navigator.clipboard.readText())).replace(
      /\r\n/g,
      "\n",
    );
  expect(await clipboard()).toBe(byDay);

  await page.getByRole("button", { name: "Zbiorczo", exact: true }).click();
  const summary = [
    "Raport TS — 2026-10-26 – 2026-10-27 (2 dni)",
    "Zadanie\tCzas (h)",
    "Projekt strony — Import CSV; Testy\t3,25",
    "Spotkanie zespołu\t0,50",
    "Razem\t3,75",
  ].join("\n");
  await expect(content).toHaveValue(summary);
  await page.getByRole("button", { name: "Kopiuj raport" }).click();
  await expect(page.getByRole("status")).toHaveText("Skopiowano raport.");
  expect(await clipboard()).toBe(summary);
  await page.getByLabel("Dołącz notatki").uncheck();
  await expect(content).toHaveValue(
    summary.replace(" — Import CSV; Testy", ""),
  );
  await page.getByLabel("Dołącz notatki").check();

  // Zmiana miesiąca zachowuje zaznaczenie; dzień bez pracy nie trafia do treści.
  await page.getByRole("button", { name: "Poprzedni miesiąc" }).click();
  await expect(page.locator(".month-label")).toHaveText("wrzesień 2026");
  await day(page, "30 września 2026").click();
  await expect(list.getByRole("listitem")).toHaveCount(3);
  await expect(content).toHaveValue(
    summary.replace(
      "2026-10-26 – 2026-10-27 (2 dni)",
      "2026-09-30 – 2026-10-27 (3 dni)",
    ),
  );
  await page.getByRole("button", { name: "Następny miesiąc" }).click();
  await expect(page.locator(".month-label")).toHaveText("październik 2026");
  await expect(day(page, "27 października 2026")).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page
    .getByRole("button", { name: "Usuń 2026-09-30 z zestawu", exact: true })
    .click();
  await expect(content).toHaveValue(summary);

  await page.setViewportSize({ width: 760, height: 900 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/report-days.png",
    fullPage: true,
  });

  // Ostatnie zaznaczenie i układ są pamiętane po ponownym uruchomieniu.
  await page.reload();
  await openMultiDay(page);
  await expect(list.getByRole("listitem")).toHaveCount(2);
  await expect(
    page.getByRole("button", { name: "Zbiorczo", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(content).toHaveValue(summary);

  // Skróty zaznaczenia.
  await page.getByRole("button", { name: "Wyczyść", exact: true }).click();
  await expect(list).toContainText("Kliknij dni w kalendarzu");
  await expect(content).toHaveValue(
    "Raport TS — brak dni\nZadanie\tCzas (h)\nRazem\t0,00",
  );
  await page
    .getByRole("button", { name: "Poprzedni tydzień", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (7)" }),
  ).toBeVisible();
  await expect(list.getByRole("listitem").first()).toContainText("2026-10-19");
  await page.getByRole("button", { name: "Ten tydzień", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (2)" }),
  ).toBeVisible();
  await expect(content).toHaveValue(summary);
  await page.getByRole("button", { name: "Ten miesiąc", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (27)" }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Dni robocze tego miesiąca", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (19)" }),
  ).toBeVisible();
  await expect(day(page, "25 października 2026")).toHaveAttribute(
    "aria-pressed",
    "false",
  );

  // Klik w dzień z listy otwiera jego raport dzienny.
  await list.getByRole("button", { name: /^2026-10-26/ }).click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-10-26");
  await expect(page.locator(".report-total")).toHaveText("02:30:00");
});

test("TS report from selected days counts the 25-hour DST day", async ({
  page,
}) => {
  await seed(
    page,
    "2026-10-28T12:00:00+01:00",
    [{ id: 1, title: "Zmiana czasu" }],
    [
      [1, "2026-10-24T22:00:00", "2026-10-25T00:00:00"],
      [1, "2026-10-25T00:00:00", "2026-10-26T00:00:00"],
      [1, "2026-10-26T00:00:00", "2026-10-26T00:30:00"],
    ],
  );
  await openMultiDay(page);
  await day(page, "24 października 2026").click();
  await day(page, "25 października 2026").click();
  await expect(day(page, "25 października 2026")).toHaveAttribute(
    "title",
    "Praca: 25:00:00",
  );
  const content = page.getByRole("textbox", {
    name: "Treść raportu TS z wybranych dni",
  });
  await expect(content).toHaveValue(
    [
      "Raport TS — 2026-10-24 – 2026-10-25 (2 dni)",
      "Dzień\tZadanie\tCzas (h)",
      "2026-10-24\tZmiana czasu\t2,00",
      "Razem 2026-10-24\t\t2,00",
      "2026-10-25\tZmiana czasu\t25,00",
      "Razem 2026-10-25\t\t25,00",
      "Razem\t\t27,00",
    ].join("\n"),
  );
  await page.getByRole("button", { name: "Zbiorczo", exact: true }).click();
  await expect(content).toHaveValue(
    [
      "Raport TS — 2026-10-24 – 2026-10-25 (2 dni)",
      "Zadanie\tCzas (h)",
      "Zmiana czasu\t27,00",
      "Razem\t27,00",
    ].join("\n"),
  );
  await expect(page.getByLabel("Suma wybranych dni")).toHaveText("27:00:00");
});

test("TS report from selected days rounds rows separately and sums full time", async ({
  page,
}) => {
  await seed(
    page,
    "2026-10-14T12:00:00+02:00",
    [
      { id: 1, title: "A" },
      { id: 2, title: "B" },
      { id: 3, title: "C" },
    ],
    [
      [1, "2026-10-13T08:00:00", "2026-10-13T08:10:00"],
      [2, "2026-10-13T09:00:00", "2026-10-13T09:10:00"],
      [3, "2026-10-13T10:00:00", "2026-10-13T10:10:00"],
      [1, "2026-10-14T08:00:00", "2026-10-14T08:10:00"],
    ],
  );
  await openMultiDay(page);
  await day(page, "13 października 2026").click();
  await day(page, "14 października 2026").click();
  // 10 min = 0,17 h w każdym wierszu; sumy z pełnego czasu: 30 min = 0,50 h.
  await expect(
    page.getByRole("textbox", { name: "Treść raportu TS z wybranych dni" }),
  ).toHaveValue(
    [
      "Raport TS — 2026-10-13 – 2026-10-14 (2 dni)",
      "Dzień\tZadanie\tCzas (h)",
      "2026-10-13\tA\t0,17",
      "2026-10-13\tB\t0,17",
      "2026-10-13\tC\t0,17",
      "Razem 2026-10-13\t\t0,50",
      "2026-10-14\tA\t0,17",
      "Razem 2026-10-14\t\t0,17",
      "Razem\t\t0,67",
    ].join("\n"),
  );
});

test("selected days: month shortcuts use the current month, midnight keeps the mode, live totals", async ({
  page,
}) => {
  await seed(
    page,
    "2026-10-31T23:59:50+01:00",
    [{ id: 1, title: "Wdrożenie" }],
    [
      [1, "2026-10-30T09:00:00", "2026-10-30T10:00:00"],
      [1, "2026-10-31T22:00:00", null],
    ],
  );
  await openMultiDay(page);
  const list = page.getByRole("list", { name: "Wybrane dni" });
  const content = page.getByRole("textbox", {
    name: "Treść raportu TS z wybranych dni",
  });

  // „Ten miesiąc” liczy się od dzisiaj, nie od miesiąca w kalendarzu.
  await page.getByRole("button", { name: "Poprzedni miesiąc" }).click();
  await expect(page.locator(".month-label")).toHaveText("wrzesień 2026");
  await page.getByRole("button", { name: "Ten miesiąc", exact: true }).click();
  await expect(page.locator(".month-label")).toHaveText("październik 2026");
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (31)" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Poprzedni miesiąc" }).click();
  await page
    .getByRole("button", { name: "Dni robocze tego miesiąca", exact: true })
    .click();
  await expect(page.locator(".month-label")).toHaveText("październik 2026");
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (22)" }),
  ).toBeVisible();

  // Sumy dni w kalendarzu uwzględniają trwający pomiar.
  await page.getByRole("button", { name: "Wyczyść", exact: true }).click();
  await day(page, "30 października 2026").click();
  await day(page, "31 października 2026").click();
  await expect(day(page, "30 października 2026")).toHaveAttribute(
    "title",
    "Praca: 01:00:00",
  );
  await expect(day(page, "31 października 2026")).toHaveAttribute(
    "title",
    "Praca: 01:59:50",
  );
  await expect(
    page.getByText("Trwający pomiar jest uwzględniony"),
  ).toBeVisible();

  // Północ nie przełącza trybu „Wybrane dni”.
  await page.clock.setFixedTime(new Date("2026-11-01T00:00:10+01:00"));
  await expect(day(page, "31 października 2026")).toHaveAttribute(
    "title",
    "Praca: 02:00:00",
  );
  await expect(
    page.getByRole("button", { name: "Wybrane dni", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(list.getByRole("listitem")).toHaveCount(2);
  await expect(content).toHaveValue(
    "Raport TS — 2026-10-30 – 2026-10-31 (2 dni)\nDzień\tZadanie\tCzas (h)\n2026-10-30\tWdrożenie\t1,00\nRazem 2026-10-30\t\t1,00\n2026-10-31\tWdrożenie\t2,00\nRazem 2026-10-31\t\t2,00\nRazem\t\t3,00",
  );

  // Jawne przejście z Dashboardu pokazuje raport jednego dnia.
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await page
    .getByRole("button", { name: "Raport dzisiejszy", exact: true })
    .click();
  await expect(page.getByLabel("Dzień raportu")).toHaveValue("2026-11-01");
  await expect(
    page.getByRole("button", { name: "Jeden dzień", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
});

test("selected days: work-day shortcut skips holidays unless time was recorded", async ({
  page,
}) => {
  // Do 20.11.2026: 15 dni Pon–Pt, w tym święto 11.11 (środa); przyszłe dni pomijane.
  await seed(
    page,
    "2026-11-20T12:00:00+01:00",
    [{ id: 1, title: "Wdrożenie" }],
    [],
  );
  await openMultiDay(page);
  const workDays = page.getByRole("button", {
    name: "Dni robocze tego miesiąca",
    exact: true,
  });
  await workDays.click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (14)" }),
  ).toBeVisible();

  // Praca w święto zostaje w raporcie TS.
  await seed(
    page,
    "2026-11-20T12:00:00+01:00",
    [{ id: 1, title: "Wdrożenie" }],
    [[1, "2026-11-11T09:00:00", "2026-11-11T10:00:00"]],
  );
  await openMultiDay(page);
  await workDays.click();
  await expect(
    page.getByRole("heading", { name: "Wybrane dni (15)" }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("list", { name: "Wybrane dni" })
      .getByRole("button", { name: /^2026-11-11/ }),
  ).toBeVisible();
});

test("day close copies the TS report with the selection fallback", async ({
  page,
  context,
}) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  await seed(
    page,
    "2026-10-27T18:00:00+01:00",
    [{ id: 1, title: "Import" }],
    [[1, "2026-10-27T09:00:00", "2026-10-27T10:30:00"]],
  );
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknięcie dnia", exact: true })
    .click();
  const area = page.getByLabel("Raport TS zamknięcia dnia");
  const expected = await area.inputValue();
  expect(expected).toContain("Import\t1,50");
  const copy = page.getByRole("button", { name: "Kopiuj raport TS" });
  await copy.click();
  await expect(page.getByRole("status")).toHaveText("Skopiowano raport.");
  expect(
    (await page.evaluate(() => navigator.clipboard.readText())).replace(
      /\r\n/g,
      "\n",
    ),
  ).toBe(expected);
  await page.evaluate(() => {
    Object.defineProperty(navigator.clipboard, "writeText", {
      value: () => Promise.reject(new Error("unavailable")),
    });
    document.execCommand = () => false;
  });
  await copy.click();
  await expect(page.getByRole("status")).toContainText("Ctrl+C");
  expect(
    await area.evaluate((element: HTMLTextAreaElement) =>
      element.value.slice(element.selectionStart, element.selectionEnd),
    ),
  ).toBe(expected);
});
