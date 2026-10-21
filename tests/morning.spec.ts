import { test, expect, type Page } from "./fixtures/test";
import {
  JIRA_SITE,
  NOW,
  SETTINGS_KEY,
  TODAY,
  at,
  calendarFixture,
  ids,
  openPreview,
  seed,
  stored,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * P2: poranny przegląd (Plan i historia → Rano). Dane z `fixtures/seed.ts`:
 * dziś czwartek 15.10.2026 12:00; „Wdrożenie etykiet” zostało niedokończone
 * w planie środy (poprzedni dzień roboczy), wzorzec cyklu „Spotkanie zespołu”
 * nie jest niedokończonym planem. Testy dokładają zadanie z planu wtorku,
 * termin na dziś i nowe zgłoszenie z Jiry.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

const details = (extra: Record<string, unknown>) => ({
  priority: 0,
  plannedDay: "",
  dueDay: "",
  project: "",
  waiting: "",
  notes: {},
  estimateMinutes: 0,
  checklist: [],
  repeatDays: [],
  repeatFrom: "",
  order: 0,
  plannedStart: "",
  ...extra,
});

/** Seed z dodatkowymi zadaniami: plan wtorku, termin dziś, nowe z Jiry. */
function morningData() {
  const data = seed();
  data.tasks.push(
    {
      id: 20,
      title: "Testy regresji",
      done: false,
      createdAt: at("2026-10-09", "09:00"),
      details: details({ plannedDay: "2026-10-13", estimateMinutes: 90 }),
    },
    {
      id: 21,
      title: "Faktura dla klienta",
      done: false,
      createdAt: at("2026-10-09", "09:00"),
      details: details({ dueDay: TODAY }),
    },
    {
      id: 22,
      title: "Błąd etykiety kuriera",
      done: false,
      createdAt: at(TODAY, "07:30"),
      jiraKey: "WMS-40",
      jiraUrl: `${JIRA_SITE}/browse/WMS-40`,
      jiraStatus: "Do zrobienia",
      details: details({}),
    },
  );
  return data;
}

const task = (data: { tasks: { id: number; details?: unknown }[] }, id: number) =>
  data.tasks.find((t) => t.id === id)!.details as {
    plannedDay: string;
    order: number;
  };

test("reguły przeglądu: dni robocze, niedokończone, terminy, nowe z Jiry (moduł)", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(async (data) => {
    const m = await import("/src/review.ts");
    const s = {
      targetDayMinutes: 480,
      workDays: [1, 2, 3, 4, 5],
      holidaysInTarget: true,
      morningReviewAt: 0,
    };
    const tasks = data.tasks as Parameters<typeof m.unfinishedTasks>[0];
    const unfinished = m.unfinishedTasks(tasks, "2026-10-15");
    const skip = new Set(unfinished.map((t) => t.id));
    const due = m.dueTasks(tasks, "2026-10-15", skip);
    return {
      // Poniedziałek → piątek; czwartek po święcie 11.11 (środa) → wtorek.
      prevMonday: m.previousWorkDay(s, "2026-10-19"),
      prevAfterHoliday: m.previousWorkDay(s, "2026-11-12"),
      nextFriday: m.nextWorkDay(s, "2026-10-16"),
      nextNoHolidays: m.previousWorkDay(
        { ...s, holidaysInTarget: false },
        "2026-11-12",
      ),
      unfinished: unfinished.map((t) => t.id),
      due: due.map((t) => t.id),
      // Bez przeglądu: od początku poprzedniego dnia roboczego (środa 14.10).
      since: m.reviewSince(s, "2026-10-15"),
      fresh: m.newJiraTasks(
        tasks,
        m.reviewSince(s, "2026-10-15"),
        "2026-10-15",
      ).map((t) => t.id),
      freshAfterReview: m.newJiraTasks(
        tasks,
        Date.parse("2026-10-15T08:00:00+02:00"),
        "2026-10-15",
      ).map((t) => t.id),
      entries: m.planAll(tasks, unfinished, "2026-10-15"),
    };
  }, morningData());
  expect(result.prevMonday).toBe("2026-10-16");
  expect(result.prevAfterHoliday).toBe("2026-11-10");
  expect(result.nextFriday).toBe("2026-10-19");
  expect(result.nextNoHolidays).toBe("2026-11-11");
  // Najnowszy plan pierwszy; wzorzec cyklu i ukończone pominięte.
  expect(result.unfinished).toEqual([ids.labels, 20]);
  expect(result.due).toEqual([21]);
  expect(result.since).toBe(at("2026-10-14", "00:00"));
  expect(result.fresh).toEqual([22]);
  expect(result.freshAfterReview).toEqual([]);
  // Na końcu dzisiejszego planu („Analiza importu” ma order 0).
  expect(result.entries).toEqual([
    { id: ids.labels, day: TODAY, order: 1 },
    { id: 20, day: TODAY, order: 2 },
  ]);
});

test("przegląd dnia: kroki, przeniesienie niedokończonych i „Wszystkie na dziś”", async ({
  page,
}) => {
  await openPreview(page, { data: morningData(), calendar: calendarFixture() });
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  const review = page.getByRole("region", { name: "Przegląd dnia" });
  await expect(review).toBeVisible();
  const unfinished = review.getByRole("region", {
    name: "Niedokończone z poprzedniego dnia roboczego",
  });
  await expect(unfinished).toContainText("(śr., 14 paź)");
  await expect(unfinished.locator(".morning-row")).toHaveCount(2);
  await expect(unfinished).toContainText("Wdrożenie etykiet");
  await expect(unfinished).toContainText("Testy regresji");
  await expect(unfinished).not.toContainText("Spotkanie zespołu");
  await expect(review.getByRole("region", { name: "Terminy" })).toContainText(
    "Faktura dla klienta",
  );
  const jira = review.getByRole("region", { name: "Nowe z Jiry" });
  await expect(jira).toContainText("WMS-40");
  // Dziś: spotkania z kalendarza, plan i pojemność (cel 8:00 − spotkania 1:15).
  const today = review.getByRole("region", { name: "Dziś" });
  await expect(today.getByRole("list", { name: "Spotkania dziś" })).toContainText(
    "Daily WMS",
  );
  await expect(
    today.getByRole("list", { name: "Plan dnia dziś" }),
  ).toContainText("Analiza importu");
  await expect(today.getByRole("progressbar", { name: "Pojemność dnia" })).toHaveAttribute(
    "aria-valuetext",
    "10:00 / 06:45",
  );
  await expect(today).toContainText("Cel 8:00 · spotkania 1:15 · zostaje 6:45");

  // „Na pt., 16 paź”: następny dzień roboczy (piątek), tylko plan zadania.
  await review
    .getByRole("button", { name: "Na pt., 16 paź: Wdrożenie etykiet" })
    .click();
  await expect(unfinished.locator(".morning-row")).toHaveCount(1);
  let data = await stored(page);
  expect(task(data, ids.labels).plannedDay).toBe("2026-10-16");

  // Nowe z Jiry „Na dziś”.
  await jira.getByRole("button", { name: "Na dziś: Błąd etykiety kuriera" }).click();
  await expect(jira).toContainText("Brak nowych zgłoszeń");
  data = await stored(page);
  expect(task(data, 22)).toMatchObject({ plannedDay: TODAY, order: 1 });

  // „Wszystkie na dziś”: na koniec dzisiejszego planu.
  await unfinished.getByRole("button", { name: "Wszystkie na dziś" }).click();
  await expect(review.getByRole("status")).toContainText(
    "Przeniesiono niedokończone na dziś.",
  );
  await expect(unfinished.locator(".morning-row")).toHaveCount(0);
  data = await stored(page);
  expect(task(data, 20)).toMatchObject({ plannedDay: TODAY, order: 2 });
  await expect(
    today.getByRole("list", { name: "Plan dnia dziś" }).getByRole("listitem"),
  ).toHaveCount(3);
  // Timer się nie zmienił: żaden pomiar nie trwa.
  expect(data.sessions.some((s: { endedAt: number | null }) => s.endedAt === null)).toBe(
    false,
  );
});

test("przegląd dnia: „Do zaplanowania” i „Ukończ”", async ({ page }) => {
  await openPreview(page, { data: morningData() });
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  const review = page.getByRole("region", { name: "Przegląd dnia" });
  await review
    .getByRole("button", { name: "Do zaplanowania: Testy regresji" })
    .click();
  await review.getByRole("button", { name: "Ukończ: Wdrożenie etykiet" }).click();
  await expect(
    review.getByText("Wszystko z wcześniejszych planów jest zrobione"),
  ).toBeVisible();
  const data = await stored(page);
  expect(task(data, 20).plannedDay).toBe("");
  expect(data.tasks.find((t: { id: number }) => t.id === ids.labels).done).toBe(true);
});

test("„Zaczynam dzień” zapisuje dzień przeglądu i zeruje „Nowe z Jiry”", async ({
  page,
}) => {
  await openPreview(page, { data: morningData() });
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  const review = page.getByRole("region", { name: "Przegląd dnia" });
  await expect(review.getByRole("region", { name: "Nowe z Jiry" })).toContainText(
    "WMS-40",
  );
  await review.getByRole("button", { name: "Zaczynam dzień" }).click();
  await expect(review.getByText("Dzień rozpoczęty o 12:00")).toBeVisible();
  await expect(review.getByRole("region", { name: "Nowe z Jiry" })).toContainText(
    "Brak nowych zgłoszeń",
  );
  const settings = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    SETTINGS_KEY,
  );
  expect(settings).toEqual({
    morningReviewDay: TODAY,
    morningReviewAt: Date.parse(NOW),
  });
  // Plan nie zmienił się sam.
  const data = await stored(page);
  expect(task(data, ids.labels).plannedDay).toBe("2026-10-14");
  // Po przeładowaniu przegląd nadal jest zapisany.
  await page.reload();
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  await expect(page.getByText("Dzień rozpoczęty o 12:00")).toBeVisible();
});

test("Dashboard i powiadomienie otwierają zakładkę Rano; wyłącznik w Ustawieniach", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Dashboard");
  await nav(page, "Przegląd dnia");
  await expect(page.getByRole("region", { name: "Przegląd dnia" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Rano", exact: true })).toHaveClass(
    /selected/,
  );
  // Kliknięcie powiadomienia: `navigate` z `section: "morning"`.
  await nav(page, "Zadania");
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("todo-track-navigate", {
        detail: { view: "journal", section: "morning" },
      }),
    ),
  );
  await expect(page.getByRole("region", { name: "Przegląd dnia" })).toBeVisible();
  // Inne zakładki nadal działają.
  await nav(page, "Dzień");
  await expect(page.getByRole("region", { name: "Plan dnia" })).toBeVisible();

  await openSettings(page, "Powiadomienia");
  const toggle = page.getByLabel("Przypominaj o porannym przeglądzie dnia");
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
  ).toEqual({ notifyMorningReview: false });
  await page.reload();
  await openSettings(page, "Powiadomienia");
  await expect(toggle).not.toBeChecked();
});
