import { test, expect, type Page } from "@playwright/test";
import { pick } from "./fixtures/searchSelect";
import { overtime, overtimeOf } from "../src/report";
import { tsEntries, tsSplitOvertime } from "../src/timesheet";
import { previewTsKey, type Snapshot } from "../src/store";
import {
  at,
  ids,
  openPreview,
  stored,
  TODAY,
  TS_EXISTING,
  tsFixture,
  tsSeed,
} from "./fixtures/seed";

/**
 * B4: nadgodziny w wysyłce do TS (wspólna funkcja `overtime` z `report.ts`,
 * podział od ostatnich chronologicznie wpisów), edycja godzin i nadgodzin,
 * przypisanie zadania TS z podglądu, porównanie z listą faktycznie wysłaną
 * i wysyłka wielu dni. Wszystko na symulowanym TS podglądu — żaden test nie
 * łączy się z prawdziwym TS.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const norm8 = { targetDayMinutes: 480, workDays: [1, 2, 3, 4, 5] };
const H = 3_600_000;
const session = (id: number, taskId: number, day: string, from: string, to: string | null) => ({
  id,
  taskId,
  startedAt: at(day, from),
  endedAt: to === null ? null : at(day, to),
});

// ---------------------------------------------------------------------------
// Funkcje: overtime() i podział.

test("overtime: norma dnia z ustawień, dzień wolny, święto, północ i trwający pomiar", () => {
  const thursday = "2026-10-15";
  const sessions = [
    session(1, 1, thursday, "08:00", "12:00"),
    session(2, 2, thursday, "12:30", "18:00"),
  ];
  // 9,5 h pracy przy normie 8 h → 1,5 h nadgodzin.
  expect(overtime(thursday, sessions, norm8)).toBe(1.5 * H);
  // Poniżej normy: zero.
  expect(overtime(thursday, sessions.slice(0, 1), norm8)).toBe(0);
  // Inny cel dzienny (7 h).
  expect(overtime(thursday, sessions, { ...norm8, targetDayMinutes: 420 })).toBe(
    2.5 * H,
  );
  // Sobota: dzień wolny, cała praca to nadgodziny.
  const saturday = "2026-10-17";
  expect(overtime(saturday, [session(3, 1, saturday, "10:00", "12:15")], norm8)).toBe(
    2.25 * H,
  );
  // Sobota jako dzień roboczy w ustawieniach: zwykła norma.
  expect(
    overtime(saturday, [session(3, 1, saturday, "08:00", "17:00")], {
      ...norm8,
      workDays: [1, 2, 3, 4, 5, 6],
    }),
  ).toBe(1 * H);
  // Święto (11 listopada, środa): cała praca to nadgodziny; bez świąt w celu — norma.
  const holiday = "2026-11-11";
  const work = [
    {
      id: 4,
      taskId: 1,
      startedAt: Date.parse(`${holiday}T09:00:00+01:00`),
      endedAt: Date.parse(`${holiday}T12:00:00+01:00`),
    },
  ];
  expect(overtime(holiday, work, norm8)).toBe(3 * H);
  expect(overtime(holiday, work, { ...norm8, holidaysInTarget: false })).toBe(0);
  // Przedział przez północ: liczy się część w danym dniu; inne dni pominięte.
  const late = {
    id: 5,
    taskId: 1,
    startedAt: at(thursday, "22:00"),
    endedAt: at("2026-10-16", "02:00"),
  };
  expect(overtime(thursday, [...sessions, late], norm8)).toBe(3.5 * H);
  expect(overtime("2026-10-16", [...sessions, late], norm8)).toBe(0);
  // Trwający pomiar liczony do `now`.
  const running = [session(6, 1, thursday, "08:00", null)];
  expect(overtime(thursday, running, norm8, at(thursday, "17:30"))).toBe(1.5 * H);
  expect(overtimeOf(9 * H, thursday, norm8)).toBe(1 * H);
});

const row = (id: string, total: string, lastEnd: number) => ({
  tsTaskId: id,
  total,
  hours: total,
  overtime: "0",
  lastEnd,
});

test("tsSplitOvertime: nadwyżka od ostatnich chronologicznie wpisów, kolejność bez zmian", () => {
  const rows = [row("a", "5,00", 3), row("b", "2,00", 1), row("c", "1,50", 2)];
  // 1 h nadgodzin: cała z wpisu kończącego dzień („a”).
  expect(
    tsSplitOvertime(rows, 100).map((r) => [r.tsTaskId, r.hours, r.overtime]),
  ).toEqual([
    ["a", "4,00", "1,00"],
    ["b", "2,00", "0"],
    ["c", "1,50", "0"],
  ]);
  // 6 h: „a” w całości, reszta z „c” (przedostatni), „b” bez zmian.
  expect(
    tsSplitOvertime(rows, 600).map((r) => [r.tsTaskId, r.hours, r.overtime]),
  ).toEqual([
    ["a", "0,00", "5,00"],
    ["b", "2,00", "0"],
    ["c", "0,50", "1,00"],
  ]);
  // Więcej niż wszystkie wpisy: każdy najwyżej do swojego czasu.
  expect(tsSplitOvertime(rows, 10_000).map((r) => r.overtime)).toEqual([
    "5,00",
    "2,00",
    "1,50",
  ]);
  // Bez nadgodzin: godziny = czas wpisu.
  expect(tsSplitOvertime(rows, 0).map((r) => [r.hours, r.overtime])).toEqual([
    ["5,00", "0"],
    ["2,00", "0"],
    ["1,50", "0"],
  ]);
});

/** Dzień z nadgodzinami: Rozwój 8,5 h (do 17:00), Spotkania 1 h (do 17:30). */
function overtimeSeed() {
  const data = tsSeed({});
  data.sessions.push(
    session(300, ids.analysis, TODAY, "11:00", "17:00"),
    session(301, ids.standup, TODAY, "17:00", "17:30"),
  );
  return data;
}
const MAP = [{ project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" }];

test("tsEntries z normą: suma do normy w godzinach, nadwyżka od końca dnia, dzień wolny", () => {
  const data = overtimeSeed() as unknown as Snapshot;
  const evening = at(TODAY, "18:00");
  const plan = tsEntries(TODAY, data, evening, MAP, { settings: norm8 });
  // 9,5 h razem, norma 8 h: 1,5 h nadgodzin — 1 h ze Spotkań (koniec 17:30),
  // 0,5 h z Rozwoju (koniec 17:00).
  expect(
    plan.entries.map((e) => [e.tsTaskName, e.total, e.hours, e.overtime]),
  ).toEqual([
    ["Rozwój", "8,50", "8,00", "0,50"],
    ["Spotkania", "1,00", "0,00", "1,00"],
  ]);
  expect([plan.totalHours, plan.norm, plan.overtime]).toEqual(["9,50", 800, 150]);
  // Bez ustawień: nadgodziny „0”, godziny = czas (jak przed B4).
  const plain = tsEntries(TODAY, data, evening, MAP);
  expect(plain.entries.map((e) => [e.hours, e.overtime])).toEqual([
    ["8,50", "0"],
    ["1,00", "0"],
  ]);
  expect([plain.totalHours, plain.norm, plain.overtime]).toEqual(["9,50", null, 0]);
  // Poniżej normy: bez nadgodzin.
  const short = tsEntries(TODAY, tsSeed({}) as unknown as Snapshot, evening, MAP, {
    settings: norm8,
  });
  expect(short.entries.map((e) => e.overtime)).toEqual(["0", "0"]);
  // Dzień wolny (sobota): cała praca w nadgodzinach.
  const saturday = "2026-10-17";
  const weekend = {
    ...data,
    sessions: [
      session(400, ids.analysis, saturday, "09:00", "11:00"),
      session(401, ids.standup, saturday, "11:00", "11:30"),
    ],
  } as unknown as Snapshot;
  const off = tsEntries(saturday, weekend, evening + 3 * 24 * H, MAP, {
    settings: norm8,
  });
  expect(off.entries.map((e) => [e.hours, e.overtime])).toEqual([
    ["0,00", "2,00"],
    ["0,00", "0,50"],
  ]);
  expect([off.norm, off.overtime]).toEqual([0, 250]);
});

// ---------------------------------------------------------------------------
// Interfejs na symulowanym TS.

const EVENING = "2026-10-15T18:00:00+02:00";
const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const dialog = (page: Page) => page.getByRole("dialog", { name: "Wyślij do TS" });
const rows = (page: Page) =>
  dialog(page).getByRole("table", { name: "Wpisy do wysłania" }).locator("tbody tr");
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);
async function openReportSend(page: Page) {
  await nav(page, "Raporty dzienne");
  await nav(page, "Raport TS");
  await page
    .getByRole("region", { name: "Raport TS" })
    .getByRole("button", { name: "Wyślij do TS" })
    .click();
  await expect(dialog(page)).toBeVisible();
}
const emptyTs = (extra: Record<string, unknown> = {}) => ({
  ...tsFixture(),
  filled: {},
  ...extra,
});

test("podgląd: podział na godziny i nadgodziny, edycja, „Przywróć z pomiarów” i zapis z nadgodzinami", async ({
  page,
}) => {
  await openPreview(page, { data: overtimeSeed(), ts: emptyTs(), now: EVENING });
  await openReportSend(page);
  const d = dialog(page);
  await expect(rows(page)).toHaveCount(2);
  await expect(rows(page).nth(0).locator("td").nth(2)).toHaveText("8,00 + 0,50 nadg.");
  await expect(rows(page).nth(1).locator("td").nth(2)).toHaveText("0,00 + 1,00 nadg.");
  const foot = d.locator("tfoot");
  await expect(foot).toContainText("Razem9,50");
  await expect(foot.getByLabel("Suma godzin")).toHaveText("8,00");
  await expect(foot.getByLabel("Suma nadgodzin")).toHaveText("1,50");
  await expect(foot).toContainText("norma dnia 8,00 h");
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toHaveCount(0);

  // Edycja: dwa miejsca po przecinku, przecinek albo kropka.
  await d.getByRole("button", { name: "Edytuj godziny" }).click();
  const hours = d.getByRole("textbox", { name: "Godziny: Rozwój", exact: true });
  const extra = d.getByRole("textbox", { name: "Nadgodziny: Rozwój" });
  await expect(hours).toHaveValue("8,00");
  await expect(extra).toHaveValue("0,50");
  await hours.fill("8,5");
  await expect(foot.getByLabel("Suma godzin")).toHaveText("8,50");
  await expect(foot).toContainText("Razem10,00");
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toContainText(
    "Suma godzin 8,50 h przekracza normę dnia 8,00 h — nadwyżkę wpisz w nadgodzinach.",
  );
  // Niepoprawna wartość blokuje zapis.
  await extra.fill("0,555");
  await expect(extra).toHaveAttribute("aria-invalid", "true");
  await expect(d.getByText("Podaj liczbę z najwyżej dwoma miejscami po przecinku")).toBeVisible();
  await expect(d.getByRole("button", { name: "Zapisz w TS" })).toBeDisabled();
  // „Przywróć z pomiarów” wraca do podziału z pomiarów.
  await d.getByRole("button", { name: "Przywróć z pomiarów: Rozwój" }).click();
  await expect(hours).toHaveValue("8,00");
  await expect(extra).toHaveValue("0,50");
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toHaveCount(0);
  await expect(d.getByRole("button", { name: "Przywróć z pomiarów: Rozwój" })).toHaveCount(0);
  // Wpis bez godzin i nadgodzin jest niepoprawny.
  const meetingHours = d.getByRole("textbox", { name: "Godziny: Spotkania", exact: true });
  const meetingExtra = d.getByRole("textbox", { name: "Nadgodziny: Spotkania" });
  await meetingExtra.fill("0");
  await expect(d.getByText("Wpis musi mieć godziny albo nadgodziny.")).toBeVisible();
  await meetingExtra.fill("1.25");
  await meetingHours.fill("0");
  await expect(foot.getByLabel("Suma nadgodzin")).toHaveText("1,75");
  await d.getByRole("button", { name: "Zakończ edycję godzin" }).click();
  await expect(rows(page).nth(1).locator("td").nth(2)).toHaveText(
    "0,00 + 1,25 nadg. zmienione",
  );

  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 2 wpisy, razem 9,75 h (w tym nadgodziny 1,75 h).",
  );
  await expect(d.getByRole("alert")).toHaveCount(0);
  expect((await tsState(page)).filled[TODAY]).toEqual([
    {
      id: "1101",
      hours: "8,00",
      overtime: "0,50",
      description: "Import CSV; Wdrożenie etykiet",
    },
    { id: "2201", hours: "0,00", overtime: "1,25", description: "Spotkanie zespołu" },
  ]);
  await d.getByRole("button", { name: "Zamknij" }).click();
  // Znacznik: godziny razem z nadgodzinami; raport (z pomiarów) bez zmian.
  await expect(
    page.getByRole("region", { name: "Raport TS" }).locator(".ts-sent-marker"),
  ).toHaveText("Wysłano do TS 15.10 18:00 (9,75 h)");
});

test("podgląd: dzień wolny — cała praca w nadgodzinach", async ({ page }) => {
  const data = tsSeed({});
  const saturday = "2026-10-17";
  data.sessions.push(session(500, ids.analysis, saturday, "10:00", "12:00"));
  await openPreview(page, {
    data,
    ts: emptyTs(),
    now: "2026-10-17T13:00:00+02:00",
  });
  await openReportSend(page);
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).nth(0).locator("td").nth(2)).toHaveText("0,00 + 2,00 nadg.");
  await expect(dialog(page).locator("tfoot")).toContainText(
    "dzień wolny — cała praca w nadgodzinach",
  );
  await dialog(page).getByRole("button", { name: "Zapisz w TS" }).click();
  expect((await tsState(page)).filled[saturday]).toEqual([
    { id: "1101", hours: "0,00", overtime: "2,00", description: "Analiza importu" },
  ]);
});

test("przypisanie zadania TS z podglądu: wiersz bez zadania TS, opcja „domyślne dla projektu”", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed(), ts: emptyTs() });
  await openReportSend(page);
  const d = dialog(page);
  const warnings = d.getByRole("note", { name: "Ostrzeżenia wysyłki" });
  await expect(warnings).toContainText("Bez zadania TS: 1 zadanie (1,00 h)");
  await expect(rows(page)).toHaveCount(2);
  await warnings
    .getByRole("checkbox", { name: "domyślne dla projektu „Foundation”" })
    .check();
  await pick(
    warnings.getByRole("combobox", { name: "Przypisz zadanie TS: Code review" }),
    "Code review",
  );
  // Wiersz trafia do wpisów bez wychodzenia z podglądu.
  await expect(rows(page)).toHaveCount(3);
  await expect(rows(page).filter({ hasText: "Code review" }).locator("td").first()).toHaveText(
    "Code review",
  );
  await expect(warnings).not.toContainText("Bez zadania TS");
  const task = (await stored(page)).tasks.find(
    (t: { id: number }) => t.id === ids.review,
  );
  expect([task.details.tsTaskId, task.details.tsTaskName]).toEqual([
    "1103",
    "Code review",
  ]);
  expect((await tsState(page)).projectMap).toContainEqual({
    project: "Foundation",
    tsTaskId: "1103",
    tsTaskName: "Code review",
  });
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText("W TS zapisano 3 wpisy, razem 4,00 h.");
});

test("przypisanie bez „domyślne dla projektu” zmienia tylko zadanie", async ({ page }) => {
  await openPreview(page, { data: tsSeed(), ts: emptyTs() });
  await openReportSend(page);
  await pick(
    dialog(page).getByRole("combobox", { name: "Przypisz zadanie TS: Code review" }),
    "Rozwój",
  );
  // To samo zadanie TS co „Analiza importu”: wiersze scalone.
  await expect(rows(page)).toHaveCount(2);
  await expect(rows(page).nth(0).locator("td").nth(0)).toHaveText(
    "Analiza importu, Code review, Wdrożenie etykiet",
  );
  expect((await tsState(page)).projectMap ?? []).not.toContainEqual(
    expect.objectContaining({ project: "Foundation" }),
  );
});

test("porównanie po zapisie z listą faktycznie wysłaną: zmiana w TS po otwarciu podglądu nie daje rozjazdu", async ({
  page,
}) => {
  const added = { id: "2203", hours: "0,75", overtime: "0", description: "Dopisane w TS" };
  await openPreview(page, {
    data: tsSeed(),
    ts: { ...tsFixture(), changeBeforeSave: { [TODAY]: [added] } },
  });
  await openReportSend(page);
  const d = dialog(page);
  // Przy otwarciu TS ma jeden wpis; tuż przed zapisem ktoś dopisuje drugi.
  await expect(d.getByRole("region", { name: "Wpisy w TS" })).toContainText(
    "W TS na ten dzień jest już 1 wpis (razem 1,00 h)",
  );
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText("W TS zapisano 4 wpisy, razem 4,75 h.");
  await expect(d.getByRole("alert")).toHaveCount(0);
  expect((await tsState(page)).filled[TODAY].map((f: { id: string }) => f.id)).toEqual([
    "1101",
    "2201",
    TS_EXISTING.id,
    added.id,
  ]);
});

test("rozjazd nadgodzin po zapisie: TS zapisał inne nadgodziny niż wysłane", async ({
  page,
}) => {
  await openPreview(page, {
    data: overtimeSeed(),
    ts: emptyTs({ dropOvertime: true }),
    now: EVENING,
  });
  await openReportSend(page);
  await dialog(page).getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(dialog(page).getByRole("alert")).toHaveText(
    "TS zapisał inne wartości niż wysłane — sprawdź w przeglądarce.",
  );
});

test("„Wyślij dni do TS”: jeden podgląd z zakładkami dni, nadgodziny i jedno podsumowanie", async ({
  page,
}) => {
  const data = tsSeed({});
  // 14 października: Rozwój 6 h + 2,5 h (do 18:00), Spotkania 0,5 h → 1 h nadgodzin.
  data.sessions.push(session(600, ids.labels, "2026-10-14", "15:30", "18:00"));
  await openPreview(page, { data, ts: tsFixture() });
  await nav(page, "Raporty dzienne");
  await nav(page, "Wybrane dni");
  const report = page.getByRole("region", { name: "Raport TS z wybranych dni" });
  const send = report.getByRole("button", { name: "Wyślij dni do TS" });
  await expect(send).toBeDisabled();
  for (const label of [
    "11 października 2026",
    "13 października 2026",
    "14 października 2026",
    "15 października 2026",
  ])
    await page.getByRole("button", { name: label, exact: true }).click();
  await send.click();
  const d = dialog(page);
  // Niedziela 11 października bez pracy: pominięta.
  await expect(d.getByRole("heading", { level: 2 })).toHaveText("Wyślij do TS · 3 dni");
  const tabs = d.getByRole("tablist", { name: "Dni wysyłki" }).getByRole("tab");
  await expect(tabs).toHaveCount(3);
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(tabs.nth(0)).toContainText("7,00");
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).nth(0).locator("td").nth(2)).toHaveText("7,00");
  await tabs.nth(1).click();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(rows(page)).toHaveCount(2);
  await expect(rows(page).nth(0).locator("td").nth(2)).toHaveText("7,50 + 1,00 nadg.");
  await tabs.nth(2).click();
  await expect(d.getByRole("region", { name: "Wpisy w TS" })).toContainText(
    "W TS na ten dzień jest już 1 wpis (razem 1,00 h)",
  );
  await d.getByRole("button", { name: "Zapisz w TS (3 dni)" }).click();
  const summary = d.getByRole("region", { name: "Podsumowanie wysyłki" });
  await expect(summary.getByRole("status")).toHaveText(
    "Zapisano w TS 3 z 3 dni, razem 20,00 h.",
  );
  await expect(summary.locator("li")).toHaveText([
    "wt., 13 paź: 1 wpis w TS, razem 7,00 h",
    "śr., 14 paź: 2 wpisy w TS, razem 9,00 h (w tym nadgodziny 1,00 h)",
    "czw., 15 paź: 3 wpisy w TS, razem 4,00 h",
  ]);
  await expect(tabs.nth(0)).toContainText("wysłano");
  const filled = (await tsState(page)).filled;
  expect(filled["2026-10-13"]).toEqual([
    {
      id: "1101",
      hours: "7,00",
      overtime: "0",
      description: "Wdrożenie etykiet; Walidacja CSV",
    },
  ]);
  expect(filled["2026-10-14"]).toEqual([
    {
      id: "1101",
      hours: "7,50",
      overtime: "1,00",
      description: "Mapowanie kolumn; Wdrożenie etykiet",
    },
    { id: "2201", hours: "0,50", overtime: "0", description: "Spotkanie zespołu" },
  ]);
  expect(filled[TODAY].map((f: { id: string }) => f.id)).toEqual([
    "1101",
    "2201",
    TS_EXISTING.id,
  ]);
  await expect(d.getByRole("button", { name: /^Zapisz w TS/ })).toHaveCount(0);
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(d).toHaveCount(0);
  // Znaczniki wysyłki dni (jak po wysyłce z Raportu dnia).
  expect(Object.keys((await tsState(page)).sent).sort()).toEqual([
    "2026-10-13",
    "2026-10-14",
    TODAY,
  ]);
});

test("„Wyślij dni do TS”: błąd jednego dnia nie zatrzymuje pozostałych, ponowienie wysyła resztę", async ({
  page,
}) => {
  const data = tsSeed({});
  await openPreview(page, { data, ts: emptyTs() });
  await nav(page, "Raporty dzienne");
  await nav(page, "Wybrane dni");
  for (const label of ["13 października 2026", "14 października 2026"])
    await page.getByRole("button", { name: label, exact: true }).click();
  await page.getByRole("button", { name: "Wyślij dni do TS" }).click();
  const d = dialog(page);
  await expect(d.getByRole("tab")).toHaveCount(2);
  // TS przyjmuje zapis, ale dzień się nie zmienia: oba dni z błędem.
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    s.ignoreSave = true;
    localStorage.setItem(k, JSON.stringify(s));
  }, previewTsKey);
  await d.getByRole("button", { name: "Zapisz w TS (2 dni)" }).click();
  const summary = d.getByRole("region", { name: "Podsumowanie wysyłki" });
  await expect(summary.getByRole("status")).toHaveText(
    "Zapisano w TS 0 z 2 dni, razem 0,00 h.",
  );
  await expect(summary.locator("li").first()).toContainText(
    "błąd: TS nie potwierdził zapisu",
  );
  await expect(d.getByRole("tab").first()).toContainText("błąd");
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    s.ignoreSave = false;
    localStorage.setItem(k, JSON.stringify(s));
  }, previewTsKey);
  await d.getByRole("button", { name: "Zapisz w TS (2 dni)" }).click();
  await expect(summary.getByRole("status")).toHaveText(
    "Zapisano w TS 2 z 2 dni, razem 13,50 h.",
  );
});
