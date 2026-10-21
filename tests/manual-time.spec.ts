import { test, expect, type Page } from "./fixtures/test";
import { TODAY, at, ids, openPreview, seed, stored } from "./fixtures/seed";
import { pick } from "./fixtures/searchSelect";

/**
 * B11: domyślne godziny „Dodaj czas ręcznie”. Dane z `fixtures/seed.ts`: dziś
 * czwartek 15.10.2026 12:00, ostatni przedział dziś kończy się o 10:30, w środę
 * o 15:30, piątek 16.10 jest pusty.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const startField = (page: Page) => page.getByLabel("Początek ręcznego czasu");
const endField = (page: Page) => page.getByLabel("Koniec ręcznego czasu");
const open = (page: Page) =>
  page.getByRole("button", { name: "Dodaj czas ręcznie", exact: true }).click();

test("domyślne godziny: koniec ostatniego przedziału do teraz, inne dni od dayStart", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Raporty dzienne");
  await open(page);
  await expect(startField(page)).toHaveValue(`${TODAY}T10:30`);
  await expect(endField(page)).toHaveValue(`${TODAY}T12:00`);

  // Wcześniejszy dzień: od końca ostatniego przedziału, godzina.
  await open(page);
  await page.getByLabel("Dzień raportu").fill("2026-10-14");
  await open(page);
  await expect(startField(page)).toHaveValue("2026-10-14T15:30");
  await expect(endField(page)).toHaveValue("2026-10-14T16:30");

  // Dzień bez przedziałów: od początku dnia z ustawień (domyślnie 08:00).
  await open(page);
  await page.getByLabel("Dzień raportu").fill("2026-10-16");
  await open(page);
  await expect(startField(page)).toHaveValue("2026-10-16T08:00");
  await expect(endField(page)).toHaveValue("2026-10-16T09:00");
});

test("początek dnia z ustawień i trwający pomiar", async ({ page }) => {
  const data = seed({ active: { task: ids.review, from: "11:00" } });
  // Dziś bez zakończonych przedziałów, tylko trwający pomiar od 11:00.
  data.sessions = data.sessions.filter(
    (x) => x.endedAt === null || x.startedAt < at(TODAY, "00:00"),
  );
  await openPreview(page, { settings: { dayStart: "07:30" }, data });
  await nav(page, "Plan i historia");
  await open(page);
  // Bez zakończonych przedziałów: od 07:30 do startu trwającego pomiaru.
  await expect(startField(page)).toHaveValue(`${TODAY}T07:30`);
  await expect(endField(page)).toHaveValue(`${TODAY}T11:00`);
});

test("moduł manualRange: przed początkiem dnia i przedział do północy", async ({
  page,
}) => {
  await page.goto("/");
  const result = await page.evaluate(
    async ([today, early, lateEnd]) => {
      const m = await import("/src/manualRange.ts");
      const t = await import("/src/time.ts");
      const fmt = ([a, b]: [number, number]) =>
        `${t.localInput(a)}–${t.localInput(b)}`;
      return {
        // 06:10, dzień zaczyna się o 08:00: ostatnia godzina do teraz.
        early: fmt(m.manualRange(today, [], early, "08:00")),
        // Wczoraj przedział do 23:59: wolna reszta doby (godzina przed północą
        // nachodziłaby na ten przedział).
        late: fmt(
          m.manualRange(
            "2026-10-14",
            [{ id: 1, taskId: 1, startedAt: lateEnd - 3_600_000, endedAt: lateEnd }],
            early,
            "08:00",
          ),
        ),
      };
    },
    [TODAY, at(TODAY, "06:10"), at("2026-10-14", "23:59")] as const,
  );
  expect(result.early).toBe(`${TODAY}T05:10:00–${TODAY}T06:10:00`);
  expect(result.late).toBe("2026-10-14T23:59:00–2026-10-15T00:00:00");
});

/** Dziś tylko podane przedziały (bez przedziałów z `seed()`) i trwający pomiar. */
function todayData(
  closed: [number, string, string][],
  running: { task: number; from: number },
) {
  const data = seed();
  data.sessions = data.sessions.filter((x) => x.startedAt < at(TODAY, "00:00"));
  closed.forEach(([taskId, from, to], i) =>
    data.sessions.push({
      id: 200 + i,
      taskId,
      startedAt: at(TODAY, from),
      endedAt: at(TODAY, to),
    }),
  );
  data.sessions.push({
    id: 300,
    taskId: running.task,
    startedAt: running.from,
    endedAt: null,
  });
  return data;
}
/** Zapis domyślnych godzin formularza: przedział trafia do bazy bez błędu nakładania. */
async function saveDefault(page: Page, start: number, end: number) {
  const before = (await stored(page)).sessions.length;
  await pick(page.getByLabel("Zadanie do uzupełnienia"), "Code review");
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect
    .poll(async () => (await stored(page)).sessions.length)
    .toBe(before + 1);
  const sessions = (await stored(page)).sessions as {
    taskId: number;
    startedAt: number;
    endedAt: number | null;
  }[];
  expect(
    sessions.some(
      (s) =>
        s.taskId === ids.review && s.startedAt === start && s.endedAt === end,
    ),
  ).toBe(true);
}

test("trwający pomiar od końca ostatniego przedziału: wolna godzina przed nimi", async ({
  page,
}) => {
  // 08:00–09:00 zamknięte, przełączenie zadania o 09:00 i pomiar trwa do teraz.
  const data = todayData([[ids.standup, "08:00", "09:00"]], {
    task: ids.analysis,
    from: at(TODAY, "09:00"),
  });
  await openPreview(page, { data });
  await nav(page, "Plan i historia");
  await open(page);
  await expect(startField(page)).toHaveValue(`${TODAY}T07:00`);
  await expect(endField(page)).toHaveValue(`${TODAY}T08:00`);
  await saveDefault(page, at(TODAY, "07:00"), at(TODAY, "08:00"));
});

test("pomiar trwa od przed początkiem dnia: przedział kończy się na jego starcie", async ({
  page,
}) => {
  // Dzień od 08:00, bez zamkniętych przedziałów, pomiar trwa od 07:30.
  const data = todayData([], { task: ids.analysis, from: at(TODAY, "07:30") });
  await openPreview(page, { data });
  await nav(page, "Plan i historia");
  await open(page);
  await expect(startField(page)).toHaveValue(`${TODAY}T06:30`);
  await expect(endField(page)).toHaveValue(`${TODAY}T07:30`);
  await saveDefault(page, at(TODAY, "06:30"), at(TODAY, "07:30"));
});
