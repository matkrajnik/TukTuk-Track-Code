import { test, expect, type Page } from "./fixtures/test";
import {
  TODAY,
  at,
  calendarEvents,
  calendarFixture,
  ids,
  openPreview,
  stored,
} from "./fixtures/seed";
import { pick } from "./fixtures/searchSelect";

/**
 * K2: spotkania w planie i pojemności. Pojemność dnia (`src/capacity.ts`) i bloki
 * planu omijające spotkania jako moduły, a w podglądzie: tor „Spotkania” na osi
 * dnia, pojemność na tablicy tygodnia, nadchodzące spotkania w Zadaniach →
 * „Dzisiaj” i „Dodaj jako czas” w Zamknięciu dnia. Dane z `fixtures/seed.ts`:
 * dziś (czw. 15.10) daily 9:15–9:30 i przegląd 13:00–14:00 „wstępnie”, anulowane
 * i odrzucone spotkanie, jutro całodniowy urlop, w poniedziałek daily.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

test("pojemność dnia: nakładanie, przez północ, całodniowy urlop, tentative i cel", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const m = await import("/src/capacity.ts");
    const t = (s: string) => new Date(s).getTime();
    const H = 3_600_000;
    const ev = (
      id: string,
      start: string,
      end: string,
      extra: Record<string, unknown> = {},
    ) => ({
      id,
      start: t(start),
      end: t(end),
      subject: id,
      showAs: "busy",
      allDay: false,
      cancelled: false,
      response: "accepted",
      ...extra,
    });
    const overlapping = [
      ev("a", "2026-10-15T09:00:00+02:00", "2026-10-15T10:00:00+02:00"),
      ev("b", "2026-10-15T09:30:00+02:00", "2026-10-15T11:00:00+02:00"),
      ev("c", "2026-10-15T13:00:00+02:00", "2026-10-15T13:30:00+02:00", {
        showAs: "workingElsewhere",
      }),
      ev("free", "2026-10-15T14:00:00+02:00", "2026-10-15T15:00:00+02:00", {
        showAs: "free",
      }),
      ev("x", "2026-10-15T15:00:00+02:00", "2026-10-15T16:00:00+02:00", {
        cancelled: true,
      }),
      ev("d", "2026-10-15T16:00:00+02:00", "2026-10-15T17:00:00+02:00", {
        response: "declined",
      }),
    ];
    const night = [
      ev("n", "2026-10-15T22:00:00+02:00", "2026-10-16T02:00:00+02:00"),
    ];
    const oof = ev("u", "2026-10-16T00:00:00+02:00", "2026-10-17T00:00:00+02:00", {
      showAs: "oof",
      allDay: true,
    });
    const tentative = [
      ev("t", "2026-10-15T13:00:00+02:00", "2026-10-15T14:00:00+02:00", {
        showAs: "tentative",
      }),
    ];
    const goalEvents = [
      ev("a", "2026-10-15T09:00:00+02:00", "2026-10-15T10:00:00+02:00"),
      ev("b", "2026-10-15T13:00:00+02:00", "2026-10-15T14:00:00+02:00"),
    ];
    const measured = [
      {
        id: 1,
        taskId: 1,
        startedAt: t("2026-10-15T09:00:00+02:00"),
        endedAt: t("2026-10-15T10:00:00+02:00"),
      },
    ];
    const now = t("2026-10-15T18:00:00+02:00");
    const h = (ms: number) => ms / H;
    return {
      overlap: m.dayCapacity(8 * H, overlapping, "2026-10-15"),
      floor: m.dayCapacity(H, overlapping, "2026-10-15").capacity,
      nightThu: h(m.dayCapacity(8 * H, night, "2026-10-15").meetings),
      nightFri: h(m.dayCapacity(8 * H, night, "2026-10-16").meetings),
      oofFri: m.dayCapacity(8 * H, [oof], "2026-10-16"),
      oofThu: m.dayCapacity(8 * H, [oof], "2026-10-15").off,
      oofCancelled: h(
        m.dayCapacity(8 * H, [{ ...oof, cancelled: true }], "2026-10-16")
          .capacity,
      ),
      tentative: h(m.dayCapacity(8 * H, tentative, "2026-10-15").capacity),
      goal: h(m.dayGoal(8 * H, goalEvents, [], "2026-10-15", now)),
      goalMeasured: h(
        m.dayGoal(8 * H, goalEvents, measured, "2026-10-15", now),
      ),
      goalOof: m.dayGoal(8 * H, [oof], [], "2026-10-16", now),
    };
  });
  // 9:00–11:00 (złączone) i 13:00–13:30; wolne, anulowane i odrzucone pominięte.
  expect(result.overlap.meetings).toBe(2.5 * 3_600_000);
  expect(result.overlap.capacity).toBe(5.5 * 3_600_000);
  expect(result.overlap.off).toBeNull();
  expect(result.floor).toBe(0);
  expect(result.nightThu).toBe(2);
  expect(result.nightFri).toBe(2);
  expect(result.oofFri.capacity).toBe(0);
  expect(result.oofFri.off).not.toBeNull();
  expect(result.oofThu).toBeNull();
  expect(result.oofCancelled).toBe(8);
  expect(result.tentative).toBe(7);
  expect(result.goal).toBe(6);
  // Spotkanie dodane jako czas nie zmniejsza celu drugi raz.
  expect(result.goalMeasured).toBe(7);
  expect(result.goalOof).toBe(0);
});

test("całodniowe wydarzenie inne niż urlop nie zmniejsza pojemności ani celu", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(async () => {
    const m = await import("/src/capacity.ts");
    const t = (s: string) => new Date(s).getTime();
    const H = 3_600_000;
    const meeting = {
      id: "a",
      start: t("2026-10-16T09:00:00+02:00"),
      end: t("2026-10-16T10:00:00+02:00"),
      subject: "Daily",
      showAs: "busy",
      allDay: false,
      cancelled: false,
      response: "accepted",
    };
    return ["busy", "workingElsewhere", "tentative"].map((showAs) => {
      const allDay = {
        ...meeting,
        id: "z",
        subject: "Praca zdalna",
        showAs,
        allDay: true,
        start: t("2026-10-16T00:00:00+02:00"),
        end: t("2026-10-17T00:00:00+02:00"),
      };
      const events = [allDay, meeting];
      const c = m.dayCapacity(8 * H, events, "2026-10-16");
      return {
        showAs,
        off: c.off,
        meetings: c.meetings / H,
        capacity: c.capacity / H,
        goal:
          m.dayGoal(
            8 * H,
            events,
            [],
            "2026-10-16",
            t("2026-10-16T18:00:00+02:00"),
          ) / H,
        spans: m.meetingSpans(events, "2026-10-16").length,
      };
    });
  });
  for (const r of result) {
    expect(r, r.showAs).toEqual({
      showAs: r.showAs,
      off: null,
      meetings: 1,
      capacity: 7,
      goal: 7,
      spans: 1,
    });
  }
});

test("bloki planu omijają spotkania: przesunięcie i podział", async ({
  page,
}) => {
  await page.goto("/");
  const blocks = await page.evaluate(async () => {
    const d = await import("/src/dayplan.ts");
    const t = (hm: string) => new Date(`2026-10-15T${hm}:00+02:00`).getTime();
    const task = (id: number, minutes: number, extra = {}) => ({
      id,
      title: "T" + id,
      done: false,
      createdAt: 0,
      details: {
        plannedDay: "2026-10-15",
        estimateMinutes: minutes,
        order: id,
        ...extra,
      },
    });
    const meetings: [number, number][] = [
      [t("09:00"), t("09:30")],
      [t("10:30"), t("11:00")],
    ];
    const clock = (ms: number) =>
      new Date(ms).toLocaleTimeString("pl-PL", {
        hour: "2-digit",
        minute: "2-digit",
        timeZone: "Europe/Warsaw",
      });
    return d
      .planBlocks(
        [
          task(1, 60), // 8:00–9:00, przed spotkaniem
          task(2, 90), // 9:30–10:30 i 11:00–11:30 (podział)
          task(3, 30, { plannedStart: "10:45" }), // przypięte: zostaje
        ] as never,
        "2026-10-15",
        "08:00",
        meetings,
      )
      .map(
        (b) =>
          `${b.task.id}:${clock(b.start)}-${clock(b.end)}:${b.part}/${b.parts}`,
      );
  });
  expect(blocks).toEqual([
    "1:08:00-09:00:1/1",
    "2:09:30-10:30:1/2",
    "2:11:00-11:30:2/2",
    "3:10:45-11:15:1/1",
  ]);
  const shifted = await page.evaluate(async () => {
    const d = await import("/src/dayplan.ts");
    const t = (hm: string) => new Date(`2026-10-15T${hm}:00+02:00`).getTime();
    // Dzień zaczyna się w trakcie spotkania: blok przesuwa się za nie.
    const [b] = d.planBlocks(
      [
        {
          id: 1,
          title: "A",
          done: false,
          createdAt: 0,
          details: { plannedDay: "2026-10-15", estimateMinutes: 30 },
        },
      ] as never,
      "2026-10-15",
      "08:00",
      [[t("07:30"), t("08:15")]],
    );
    return [b.start === t("08:15"), b.end === t("08:45"), b.parts];
  });
  expect(shifted).toEqual([true, true, 1]);
});

test("oś dnia: tor Spotkania nad planem, plan omija spotkania", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  const chart = page.getByRole("group", { name: "Oś planu i rzeczywistości" });
  await expect(
    chart.getByRole("img", { name: "Spotkanie: Daily WMS · 09:15–09:30" }),
  ).toBeVisible();
  await expect(
    chart.getByRole("img", {
      name: "Spotkanie: Przegląd sprintu · 13:00–14:00 · wstępnie",
    }),
  ).toHaveClass(/tentative/);
  // Anulowane i odrzucone nie zajmują czasu.
  await expect(chart.getByRole("img")).toHaveCount(2);
  await expect(page.locator(".dtl-labels")).toHaveText(
    /Spotkania\s*Plan\s*Rzeczywistość/,
  );
  // Analiza importu (10 h od 8:00) dzieli się na trzy części wokół spotkań.
  const plan = chart.locator(".dtl-plan");
  await expect(plan).toHaveCount(3);
  await expect(plan.nth(0)).toHaveAttribute(
    "aria-label",
    "Plan: Analiza importu · 08:00–09:15 · część 1 z 3",
  );
  await expect(plan.nth(1)).toHaveAttribute(
    "aria-label",
    "Plan: Analiza importu · 09:30–13:00 · część 2 z 3",
  );
  await expect(plan.nth(2)).toHaveAttribute(
    "aria-label",
    "Plan: Analiza importu · 14:00–19:15 · część 3 z 3",
  );
  // Jeden wiersz w tabeli mimo trzech bloków.
  await expect(
    page
      .getByRole("table", { name: "Plan i rzeczywistość" })
      .locator("tbody tr", { hasText: "Analiza importu" }),
  ).toHaveCount(1);
  await expect(
    page.getByRole("list", { name: "Legenda osi dnia" }),
  ).toContainText("Spotkania (2)");

  // Wyłączone tytuły: „Spotkanie”.
  await openPreview(page, {
    calendar: calendarFixture(),
    settings: { calendarShowTitles: false },
  });
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  await expect(
    page.getByRole("img", { name: "Spotkanie: Spotkanie · 09:15–09:30" }),
  ).toBeVisible();

  // Bez połączonego kalendarza toru nie ma.
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  await expect(page.locator(".dtl-labels")).toHaveText(/^\s*Plan\s*Rzeczywistość\s*$/);
  await expect(page.locator(".dtl-meeting")).toHaveCount(0);
});

test("tablica tygodnia: pojemność pomniejszona o spotkania, urlop jako dzień wolny", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Tablica");
  const column = (name: string) =>
    page.getByRole("region", { name, exact: true });
  const bar = (day: string) =>
    page.getByRole("progressbar", { name: "Pojemność: " + day, exact: true });

  // Czwartek: cel 8:00, daily 0:15 + przegląd (wstępnie) 1:00.
  const thursday = column("Czwartek 2026-10-15");
  await expect(thursday.locator(".week-estimate")).toHaveText("10:00 / 06:45");
  await expect(bar("Czwartek 2026-10-15")).toHaveAttribute(
    "title",
    "Cel 8:00 · spotkania 1:15 · zostaje 6:45",
  );
  await expect(bar("Czwartek 2026-10-15")).toHaveAttribute(
    "aria-valuetext",
    "10:00 / 06:45",
  );
  await expect(bar("Czwartek 2026-10-15")).toHaveClass(/\bover\b/);
  await expect(
    bar("Czwartek 2026-10-15").locator(".week-capacity-meetings"),
  ).toHaveAttribute("style", /width: 15\.625%/);
  await expect(thursday.locator(".week-meetings")).toHaveText(
    "Spotkania 01:15",
  );

  // Poniedziałek: daily 0:15.
  await expect(
    column("Poniedziałek 2026-10-12").locator(".week-estimate"),
  ).toHaveText("04:30 / 07:45");

  // Piątek: całodniowy urlop = dzień wolny.
  const friday = column("Piątek 2026-10-16");
  await expect(friday).toHaveClass(/\bfree\b/);
  await expect(friday.locator(".week-estimate")).toHaveText("00:00 / wolne");
  await expect(friday.locator(".week-holiday")).toHaveText("Urlop");
  await expect(bar("Piątek 2026-10-16")).toHaveAttribute(
    "title",
    "Poza biurem: Urlop · dzień wolny",
  );

  // Środa bez spotkań: jak wcześniej.
  await expect(
    column("Środa 2026-10-14").locator(".week-estimate"),
  ).toHaveText("08:00 / 08:00");
  await expect(
    bar("Środa 2026-10-14").locator(".week-capacity-meetings"),
  ).toHaveCount(0);
});

test("Zadania → Dzisiaj: nadchodzące spotkania i cel pomniejszony o spotkania", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture() });
  // 8:00–10:30 pracy; daily (9:15) pokryte pomiarem, przegląd 13–14 nie: cel 7:00.
  await expect(page.locator(".daily-target .target-text")).toHaveText(
    "02:30 / 07:00",
  );
  await expect(page.getByRole("region", { name: "Spotkania dziś" })).toHaveCount(
    0,
  );
  await page.getByLabel("Dzisiaj: plan i praca").check();
  const list = page.getByRole("region", { name: "Spotkania dziś" });
  // Daily już minęło, anulowane i odrzucone się nie liczą.
  await expect(list.locator("li")).toHaveCount(1);
  await expect(list.locator("li").first()).toHaveText(
    /13:00–14:00\s*Przegląd sprintu\s*wstępnie/,
  );
});

test("Zamknięcie dnia: „Dodaj jako czas” otwiera ręczne dodanie z godzinami spotkania", async ({
  page,
}) => {
  const calendar = calendarFixture();
  const call = {
    ...calendarEvents()[1],
    id: "m-call",
    subject: "Telefon z klientem",
    start: at(TODAY, "11:00"),
    end: at(TODAY, "11:30"),
  };
  calendar.events = [...calendar.events, call];
  await openPreview(page, { calendar });
  const before = (await stored(page)).sessions.length;
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  const meetings = page.getByRole("list", { name: "Spotkania dnia" });
  await expect(meetings.locator("li")).toHaveCount(3);
  // Daily nakłada się na pomiar Analizy importu: ostrzeżenie zamiast przycisku.
  await expect(meetings.locator("li").nth(0)).toContainText(
    "Nakłada się na zapisany pomiar",
  );
  await expect(
    meetings.locator("li").nth(0).getByRole("button"),
  ).toHaveCount(0);
  // Przegląd jeszcze się nie odbył.
  await expect(meetings.locator("li").nth(2)).toContainText(
    "Jeszcze się nie skończyło",
  );
  // Nic nie dodaje się samo.
  expect((await stored(page)).sessions.length).toBe(before);

  await page
    .getByRole("button", {
      name: "Dodaj jako czas: Telefon z klientem 11:00–11:30",
    })
    .click();
  await expect(
    page.getByRole("button", { name: "Dodaj czas ręcznie" }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(page.getByLabel("Początek ręcznego czasu")).toHaveValue(
    `${TODAY}T11:00`,
  );
  await expect(page.getByLabel("Koniec ręcznego czasu")).toHaveValue(
    `${TODAY}T11:30`,
  );
  await pick(page.getByLabel("Zadanie do uzupełnienia"), "Code review");
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect(
    page.getByRole("button", { name: "Dodaj czas ręcznie" }),
  ).toHaveAttribute("aria-expanded", "false");
  const data = await stored(page);
  expect(data.sessions.length).toBe(before + 1);
  expect(
    data.sessions.some(
      (s: { taskId: number; startedAt: number; endedAt: number }) =>
        s.taskId === ids.review &&
        s.startedAt === at(TODAY, "11:00") &&
        s.endedAt === at(TODAY, "11:30"),
    ),
  ).toBe(true);
  // Teraz spotkanie nakłada się na zapisany czas; wybór zadania jest zapamiętany.
  await expect(meetings.locator("li").nth(1)).toContainText(
    "Nakłada się na zapisany pomiar",
  );
  expect(
    await page.evaluate(() => localStorage.getItem("tuktuk-track-meeting-task")),
  ).toBe(String(ids.review));
});
