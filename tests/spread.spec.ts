import { test, expect, type Page } from "@playwright/test";
import {
  correction,
  corrections,
  correctedMinutes,
  plannedMs,
  usuallyText,
} from "../src/estimates";
import { spread, type SpreadDay } from "../src/spread";
import type { Snapshot, Task } from "../src/store";
import {
  at,
  ids,
  openPreview,
  seed,
  SETTINGS_KEY,
  stored,
  TODAY,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

test.use({ timezoneId: "Europe/Warsaw" });

// ---------------------------------------------------------------------------
// Poprawka szacunków (testy jednostkowe, bez przeglądarki).
// „Teraz” to czwartek 15.10.2026 12:00; okres 90 dni zaczyna się 18.07.2026.
// ---------------------------------------------------------------------------
const NOW_MS = Date.parse("2026-10-15T12:00:00+02:00");
const H = 3_600_000;

/** Ukończone zadanie z szacunkiem 60 min i czasem `ratio` × 60 min w dniu `day`. */
function history(
  rows: { project: string; ratio: number; day?: string; done?: boolean }[],
): Snapshot {
  const tasks: Task[] = [];
  const sessions: Snapshot["sessions"] = [];
  rows.forEach((row, i) => {
    const id = i + 1;
    tasks.push({
      id,
      title: `Zadanie ${id}`,
      done: row.done ?? true,
      createdAt: 0,
      details: {
        priority: 0,
        plannedDay: "",
        dueDay: "",
        project: row.project,
        waiting: "",
        notes: {},
        estimateMinutes: 60,
        checklist: [],
        repeatDays: [],
        repeatFrom: "",
        order: 0,
        plannedStart: "",
      },
    });
    const start = Date.parse(`${row.day ?? "2026-10-05"}T09:00:00+02:00`);
    sessions.push({
      id,
      taskId: id,
      startedAt: start,
      endedAt: start + Math.round(row.ratio * H),
    });
  });
  return { tasks, sessions };
}

test("correction: fewer than 5 finished tasks means no correction", () => {
  const data = history([
    { project: "WMS", ratio: 2 },
    { project: "WMS", ratio: 2 },
    { project: "WMS", ratio: 2 },
    { project: "Portal", ratio: 2 },
  ]);
  expect(correction("WMS", data, NOW_MS)).toEqual({
    factor: 1,
    count: 0,
    scope: "none",
    project: "",
  });
  expect(usuallyText(120, correction("WMS", data, NOW_MS))).toBe("");
});

test("correction: project with 5 tasks wins, otherwise all tasks", () => {
  const data = history([
    // WMS: 5 zadań, mediana 2.
    { project: "WMS", ratio: 1.5 },
    { project: "WMS", ratio: 2 },
    { project: "WMS", ratio: 2 },
    { project: "wms ", ratio: 2.5 }, // ten sam projekt (wielkość liter, spacje)
    { project: "WMS", ratio: 3 },
    // Portal: 2 zadania, za mało na własny mnożnik.
    { project: "Portal", ratio: 0.5 },
    { project: "Portal", ratio: 0.75 },
    // Otwarte i bez projektu: otwarte się nie liczy, bez projektu tylko we „wszystkich”.
    { project: "WMS", ratio: 9, done: false },
    { project: "", ratio: 1 },
  ]);
  const of = corrections(data, NOW_MS);
  expect(of("WMS")).toEqual({
    factor: 2,
    count: 5,
    scope: "project",
    project: "WMS",
  });
  // Wszystkie 8 ukończonych: 0,5 0,75 1 1,5 2 2 2,5 3 → mediana 1,75.
  expect(of("Portal")).toEqual({
    factor: 1.75,
    count: 8,
    scope: "all",
    project: "",
  });
  expect(of("")).toEqual(of("Portal"));
  expect(of(undefined).scope).toBe("all");
  // Jedno miejsce reguły: `correction(project)` to samo co `corrections`.
  expect(correction("wms", data, NOW_MS)).toEqual(of("WMS"));
  expect(usuallyText(120, of("WMS"))).toBe("zwykle ~4 h");
  expect(usuallyText(80, of("Portal"))).toBe("zwykle ~2 h 20 min");
  expect(usuallyText(0, of("WMS"))).toBe("");
});

test("correction: only the last 90 days count", () => {
  const recent = Array.from({ length: 4 }, () => ({
    project: "WMS",
    ratio: 2,
  }));
  const old = { project: "WMS", ratio: 2, day: "2026-07-01" };
  expect(correction("WMS", history([...recent, old]), NOW_MS).scope).toBe(
    "none",
  );
  const edge = { project: "WMS", ratio: 2, day: "2026-07-18" };
  expect(correction("WMS", history([...recent, edge]), NOW_MS)).toMatchObject({
    factor: 2,
    count: 5,
    scope: "project",
  });
});

test("correction: factor is clamped to 0.5–3", () => {
  const slow = history(
    Array.from({ length: 5 }, () => ({ project: "WMS", ratio: 6 })),
  );
  expect(correction("WMS", slow, NOW_MS).factor).toBe(3);
  const fast = history(
    Array.from({ length: 5 }, () => ({ project: "WMS", ratio: 0.1 })),
  );
  expect(correction("WMS", fast, NOW_MS).factor).toBe(0.5);
  expect(correctedMinutes(60, correction("WMS", fast, NOW_MS))).toBe(30);
  expect(correctedMinutes(5, correction("WMS", fast, NOW_MS))).toBe(5);
});

test("correction: planned time uses the factor only when asked", () => {
  const data = history(
    Array.from({ length: 5 }, () => ({ project: "WMS", ratio: 1.5 })),
  );
  const task = { ...data.tasks[0], done: false };
  expect(plannedMs(task)).toBe(H);
  expect(plannedMs(task, corrections(data, NOW_MS))).toBe(1.5 * H);
  const none = { ...task, details: { ...task.details!, estimateMinutes: 0 } };
  expect(plannedMs(none, corrections(data, NOW_MS))).toBe(0);
});

// ---------------------------------------------------------------------------
// „Rozłóż” (testy jednostkowe algorytmu).
// ---------------------------------------------------------------------------
let nextId = 100;
function task(
  title: string,
  details: Partial<NonNullable<Task["details"]>> = {},
  extra: Partial<Task> = {},
): Task {
  return {
    id: ++nextId,
    title,
    done: false,
    createdAt: 0,
    details: {
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
      ...details,
    },
    ...extra,
  };
}
const est = (t: Task) => (t.details?.estimateMinutes ?? 0) * 60_000;
/** Czwartek–niedziela, cel 8 h w dni robocze. */
const week = (over: Partial<Record<string, Partial<SpreadDay>>> = {}) =>
  ["2026-10-15", "2026-10-16", "2026-10-17", "2026-10-18"].map((day, i) => ({
    day,
    capacity: i < 2 ? 8 * H : 0,
    load: 0,
    ...over[day],
  }));
const where = (result: ReturnType<typeof spread>) =>
  Object.fromEntries(result.placed.map((p) => [p.task.title, p.day]));
const why = (result: ReturnType<typeof spread>) =>
  Object.fromEntries(result.missed.map((m) => [m.task.title, m.reason]));

test("spread: due date first and never after the due date", () => {
  const late = task("Bez terminu", { estimateMinutes: 300 });
  const due = task("Na piątek", { estimateMinutes: 300, dueDay: "2026-10-16" });
  const today = task("Na dziś", { estimateMinutes: 300, dueDay: "2026-10-15" });
  const past = task("Po terminie", { estimateMinutes: 30, dueDay: "2026-10-14" });
  const result = spread([late, due, today, past], week(), est);
  // Termin dziś wypełnia dziś, termin w piątek ląduje w piątek; bez terminu
  // nie mieści się już nigdzie (po 5 h w obu dniach zostają 3 h).
  expect(where(result)).toEqual({
    "Na dziś": "2026-10-15",
    "Na piątek": "2026-10-16",
  });
  expect(why(result)).toEqual({
    "Bez terminu": "Brak miejsca do końca tygodnia",
    "Po terminie": "Termin 14.10 już minął",
  });
  // Termin w piątek przy pełnym piątku: nie przechodzi na sobotę ani dalej.
  const full = spread(
    [task("Piątek", { estimateMinutes: 120, dueDay: "2026-10-16" })],
    week({ "2026-10-15": { load: 7 * H }, "2026-10-16": { load: 7 * H } }),
    est,
  );
  expect(full.placed).toEqual([]);
  expect(why(full)).toEqual({ Piątek: "Brak miejsca do terminu 16.10" });
});

test("spread: priority and manual order decide who gets the space", () => {
  const low = task("Niski", { estimateMinutes: 240, priority: -1 });
  const second = task("Drugi", { estimateMinutes: 240, order: 2 });
  const first = task("Pierwszy", { estimateMinutes: 240, order: 1 });
  const high = task("Wysoki", { estimateMinutes: 240, priority: 1 });
  const result = spread([low, second, first, high], week(), est);
  expect(result.placed.map((p) => [p.task.title, p.day])).toEqual([
    ["Wysoki", "2026-10-15"],
    ["Pierwszy", "2026-10-15"],
    ["Drugi", "2026-10-16"],
    ["Niski", "2026-10-16"],
  ]);
  expect(result.days[0].after).toBe(8 * H);
});

test("spread: overloaded days are skipped, before/after load is reported", () => {
  const a = task("A", { estimateMinutes: 120 });
  const b = task("Bez szacunku");
  const result = spread(
    [a, b],
    week({ "2026-10-15": { load: 9 * H } }),
    est,
  );
  // Dziś już ponad pojemność: oba na piątek; bez szacunku = 30 min.
  expect(where(result)).toEqual({ A: "2026-10-16", "Bez szacunku": "2026-10-16" });
  expect(result.days[0]).toMatchObject({ load: 9 * H, after: 9 * H, tasks: [] });
  expect(result.days[1]).toMatchObject({ load: 0, after: 2.5 * H });
  expect(
    result.placed.find((p) => p.task.title === "Bez szacunku")!.duration,
  ).toBe(0.5 * H);
});

test("spread: free days (weekend, all-day out of office) get nothing", () => {
  const tasks = Array.from({ length: 3 }, (_, i) =>
    task(`Zadanie ${i}`, { estimateMinutes: 480 }),
  );
  // Piątek wolny (urlop całodniowy): pojemność 0.
  const result = spread(tasks, week({ "2026-10-16": { capacity: 0 } }), est);
  expect(result.placed.map((p) => p.day)).toEqual(["2026-10-15"]);
  expect(result.missed.map((m) => m.reason)).toEqual([
    "Brak miejsca do końca tygodnia",
    "Brak miejsca do końca tygodnia",
  ]);
  const weekend = spread(
    [task("Sobota", { estimateMinutes: 30, dueDay: "2026-10-17" })],
    week().slice(2),
    est,
  );
  expect(why(weekend)).toEqual({
    Sobota: "Brak dnia roboczego do terminu 17.10",
  });
  expect(why(spread([task("X")], [], est))).toEqual({
    X: "Brak dni roboczych do końca tygodnia",
  });
});

test("spread: a task longer than a day's capacity does not fit", () => {
  const huge = task("Migracja", { estimateMinutes: 600 });
  const small = task("Poprawka", { estimateMinutes: 60 });
  const result = spread([huge, small], week(), est);
  expect(why(result)).toEqual({ Migracja: "Dłuższe niż pojemność dnia (8:00)" });
  expect(where(result)).toEqual({ Poprawka: "2026-10-15" });
});

test("spread: blocked, recurring, done and archived tasks are left alone", () => {
  const waiting = task("Czeka", { estimateMinutes: 30, waiting: "klient" });
  const recurring = task("Daily", { estimateMinutes: 15, repeatDays: [1, 2] });
  const done = task("Zrobione", { estimateMinutes: 30 }, { done: true });
  const archived = task("Archiwum", { estimateMinutes: 30 }, { archived: true });
  const ok = task("Do zrobienia", { estimateMinutes: 30 });
  const result = spread([waiting, recurring, done, archived, ok], week(), est);
  expect(result.placed.map((p) => p.task.title)).toEqual(["Do zrobienia"]);
  expect(result.skipped.map((t) => t.title)).toEqual(["Czeka", "Daily"]);
  expect(result.missed).toEqual([]);
});

test("spread: corrected durations are used when given", () => {
  const data = history(
    Array.from({ length: 5 }, () => ({ project: "WMS", ratio: 2 })),
  );
  const of = corrections(data, NOW_MS);
  const big = task("WMS 5 h", { estimateMinutes: 300, project: "WMS" });
  expect(where(spread([big], week(), (t) => plannedMs(t)))).toEqual({
    "WMS 5 h": "2026-10-15",
  });
  // 5 h × 2 = 10 h: większe niż dzień.
  expect(why(spread([big], week(), (t) => plannedMs(t, of)))).toEqual({
    "WMS 5 h": "Dłuższe niż pojemność dnia (8:00)",
  });
});

// ---------------------------------------------------------------------------
// e2e: podgląd i zastosowanie na tablicy tygodnia (dane z seed.ts).
// ---------------------------------------------------------------------------
async function openWeek(page: Page) {
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
}
const extra = (
  id: number,
  title: string,
  details: Record<string, unknown>,
) => ({
  id,
  title,
  done: false,
  createdAt: at("2026-10-01", "09:00"),
  details: {
    ...seed().tasks[2].details,
    project: "",
    estimateMinutes: 0,
    ...details,
  },
});

function spreadData() {
  const data = seed();
  data.tasks.push(
    // Termin dziś, a dziś (Analiza importu 10 h) już ponad pojemność.
    extra(20, "Pilna poprawka", { estimateMinutes: 60, dueDay: TODAY }),
    // Zaległe sprzed tygodnia, bez szacunku: 30 min.
    extra(21, "Stara notatka", { plannedDay: "2026-10-05" }),
    // Większe niż dzień.
    extra(22, "Migracja bazy", { estimateMinutes: 900 }),
    // Zablokowane: pomijane.
    extra(23, "Czeka na klienta", { estimateMinutes: 60, waiting: "klient" }),
  );
  return data;
}

test("spread e2e: preview, cancel and apply", async ({ page }) => {
  await openPreview(page, { data: spreadData() });
  await openWeek(page);
  const before = await stored(page);

  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  const preview = page.getByRole("region", { name: "Podgląd: Rozłóż" });
  await expect(preview).toBeVisible();
  // Czeka na klienta i cykliczne Spotkanie zespołu (plan z poniedziałku).
  await expect(preview).toContainText("Pominięte (czekam na, cykliczne): 2.");
  // Dni od dziś (czwartek) do niedzieli; przed → po względem pojemności.
  const thursday = preview.getByRole("listitem", {
    name: "Rozłóż: Czwartek 2026-10-15",
  });
  await expect(thursday).toContainText("10:00 → 10:00 / 08:00");
  await expect(thursday).toContainText("Bez nowych zadań.");
  const friday = preview.getByRole("listitem", {
    name: "Rozłóż: Piątek 2026-10-16",
  });
  await expect(friday).toContainText("00:00 → 01:30 / 08:00");
  await expect(friday.locator(".spread-tasks li")).toHaveText([
    // Bez terminu i priorytetu: ręczna kolejność, potem nowsze wyżej.
    /Stara notatka\s+00:30/,
    /Code review\s+01:00/,
  ]);
  await expect(
    preview.getByRole("listitem", { name: "Rozłóż: Sobota 2026-10-17" }),
  ).toContainText("wolne");
  const missed = preview.getByRole("region", { name: "Nie zmieściło się" });
  await expect(missed.getByRole("listitem")).toHaveText([
    /Pilna poprawka\s+01:00 — Brak miejsca do terminu 15\.10/,
    /Migracja bazy\s+15:00 — Dłuższe niż pojemność dnia \(8:00\)/,
    // Zaplanowane na środę (miniony dzień tego tygodnia) też jest zaległe.
    /Wdrożenie etykiet\s+08:00 — Brak miejsca do końca tygodnia/,
  ]);

  // Anuluj: podgląd znika, dane bez zmian.
  await preview.getByRole("button", { name: "Anuluj" }).click();
  await expect(preview).toBeHidden();
  expect(await stored(page)).toEqual(before);

  // Zastosuj: oba zadania w piątku, reszta bez zmian.
  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  await preview.getByRole("button", { name: "Zastosuj" }).click();
  await expect(preview).toBeHidden();
  const fridayColumn = page.getByRole("region", {
    name: "Piątek 2026-10-16",
    exact: true,
  });
  await expect(fridayColumn.locator(".week-card-title")).toHaveText([
    "Stara notatka",
    "Code review",
  ]);
  const after = await stored(page);
  const day = (id: number) =>
    after.tasks.find((t: Task) => t.id === id).details.plannedDay;
  expect(day(ids.review)).toBe("2026-10-16");
  expect(day(21)).toBe("2026-10-16");
  expect(day(20)).toBe("");
  expect(day(22)).toBe("");
  expect(day(23)).toBe("");
  expect(day(ids.labels)).toBe("2026-10-14");
  expect(day(ids.standup)).toBe(
    before.tasks.find((t: Task) => t.id === ids.standup).details.plannedDay,
  );
  // Szczegóły poza dniem i kolejnością zostają nietknięte.
  const review = after.tasks.find((t: Task) => t.id === ids.review).details;
  expect(review.project).toBe("Foundation");
  expect(review.estimateMinutes).toBe(60);
});

test("spread: a due date is past only before today", () => {
  // Widoczny tydzień w przyszłości: termin w bieżącym tygodniu nie minął.
  const next = ["2026-10-19", "2026-10-20"].map((day) => ({
    day,
    capacity: 8 * H,
    load: 0,
  }));
  const friday = task("Na piątek", { estimateMinutes: 30, dueDay: "2026-10-16" });
  const old = task("Na środę", { estimateMinutes: 30, dueDay: "2026-10-14" });
  expect(why(spread([friday, old], next, est, "2026-10-15"))).toEqual({
    "Na piątek": "Brak dnia roboczego do terminu 16.10",
    "Na środę": "Termin 14.10 już minął",
  });
});

test("spread e2e: next week keeps this week's plan and fresh deadlines", async ({
  page,
}) => {
  const data = seed();
  data.tasks.push(
    // Zaplanowane na jutro (piątek): nie jest zaległe, zostaje w piątku.
    extra(24, "Jutrzejsze", { estimateMinutes: 60, plannedDay: "2026-10-16" }),
    // Termin w piątek: przy następnym tygodniu nie ma na nie dnia, ale termin nie minął.
    extra(25, "Termin w piątek", { estimateMinutes: 30, dueDay: "2026-10-16" }),
  );
  await openPreview(page, { data });
  await openWeek(page);
  await page.getByRole("button", { name: "Następny okres" }).click();
  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  const preview = page.getByRole("region", { name: "Podgląd: Rozłóż" });
  await expect(preview).toBeVisible();
  // Cały następny tydzień; dni bieżącego tygodnia nie ma w podglądzie.
  await expect(
    preview.getByRole("listitem", { name: /^Rozłóż: / }),
  ).toHaveCount(7);
  const monday = preview.getByRole("listitem", {
    name: "Rozłóż: Poniedziałek 2026-10-19",
  });
  // Code review (1 h) i zaległe ze środy Wdrożenie (8 h) na następny tydzień.
  await expect(monday.locator(".spread-tasks li")).toContainText([
    "Code review",
  ]);
  await expect(
    preview
      .getByRole("listitem", { name: "Rozłóż: Wtorek 2026-10-20" })
      .locator(".spread-tasks li"),
  ).toHaveText([/Wdrożenie etykiet\s+08:00/]);
  await expect(preview.locator(".spread-tasks li")).toHaveCount(2);
  await expect(
    preview.getByRole("region", { name: "Nie zmieściło się" }),
  ).toContainText(
    /Termin w piątek\s+00:30 — Brak dnia roboczego do terminu 16\.10/,
  );

  await preview.getByRole("button", { name: "Zastosuj" }).click();
  await expect(preview).toBeHidden();
  const after = await stored(page);
  const day = (id: number) =>
    after.tasks.find((t: Task) => t.id === id).details.plannedDay;
  expect(day(24)).toBe("2026-10-16");
  expect(day(ids.analysis)).toBe(TODAY);
  expect(day(ids.review)).toBe("2026-10-19");
  expect(day(ids.labels)).toBe("2026-10-20");
  expect(day(25)).toBe("");
});

test("spread e2e: the preview follows changes made while it is open", async ({
  page,
}) => {
  await openPreview(page, { data: spreadData() });
  await openWeek(page);
  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  const preview = page.getByRole("region", { name: "Podgląd: Rozłóż" });
  const friday = preview.getByRole("listitem", {
    name: "Rozłóż: Piątek 2026-10-16",
  });
  await expect(friday.locator(".spread-tasks li")).toHaveText([
    /Stara notatka/,
    /Code review/,
  ]);
  // Użytkownik sam planuje Code review na sobotę, gdy podgląd jest otwarty.
  await page
    .getByRole("button", { name: "Przenieś na…: Code review", exact: true })
    .click();
  await page
    .getByRole("menuitem", { name: "Sobota, 17 paź", exact: true })
    .click();
  await expect(friday.locator(".spread-tasks li")).toHaveText([
    /Stara notatka/,
  ]);
  await preview.getByRole("button", { name: "Zastosuj" }).click();
  await expect(preview).toBeHidden();
  const after = await stored(page);
  const day = (id: number) =>
    after.tasks.find((t: Task) => t.id === id).details.plannedDay;
  // „Zastosuj” nie nadpisuje ręcznego przeniesienia.
  expect(day(ids.review)).toBe("2026-10-17");
  expect(day(21)).toBe("2026-10-16");
});

/** Pięć ukończonych zadań Foundation z czasem 2× szacunek (z ostatnich dni). */
function correctionData() {
  const data = seed();
  for (let i = 0; i < 5; i++) {
    const id = 30 + i;
    data.tasks.push({
      ...extra(id, `Foundation ${i}`, {
        project: "Foundation",
        estimateMinutes: 30,
      }),
      done: true,
    });
    data.sessions.push({
      id: 100 + i,
      taskId: id,
      startedAt: at("2026-10-0" + (i + 1), "08:00"),
      endedAt: at("2026-10-0" + (i + 1), "09:00"),
    });
  }
  return data;
}

test("correction e2e: hint on the week card and planning with correction", async ({
  page,
}) => {
  await openPreview(page, { data: correctionData() });
  await openWeek(page);
  const card = page.getByRole("article", { name: "Code review" });
  await expect(card.locator(".week-usual")).toHaveText("zwykle ~2 h");
  // Bez ustawienia „Rozłóż” planuje 1 h; z ustawieniem 2 h.
  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  const friday = page
    .getByRole("region", { name: "Podgląd: Rozłóż" })
    .getByRole("listitem", { name: "Rozłóż: Piątek 2026-10-16" });
  await expect(friday).toContainText("00:00 → 01:00 / 08:00");

  await openPreview(page, {
    data: correctionData(),
    settings: { planWithCorrection: true },
  });
  await openWeek(page);
  await page.getByRole("button", { name: "Rozłóż", exact: true }).click();
  await expect(friday).toContainText("00:00 → 02:00 / 08:00");
  // Pojemność dnia liczy szacunki z poprawką: WMS ma za mało własnych danych,
  // więc Wdrożenie etykiet (8 h) dostaje mnożnik wszystkich projektów (×2).
  await expect(
    page.getByRole("progressbar", { name: "Pojemność: Środa 2026-10-14" }),
  ).toHaveAttribute("aria-valuetext", "16:00 / 08:00");
});

test("correction e2e: hint in quick add and in the expanded task", async ({
  page,
}) => {
  await openPreview(page, { data: correctionData() });
  await page
    .getByRole("textbox", { name: "Nowe zadanie" })
    .fill("Przegląd ~1h #Foundation");
  await expect(page.locator(".quick-chip-hint")).toHaveText("· zwykle ~2 h");
  await page.getByRole("textbox", { name: "Nowe zadanie" }).fill("Przegląd ~1h");
  // Bez projektu: wszystkie ukończone (5 Foundation + Dokumentacja API 4 h / 4 h).
  await expect(page.locator(".quick-chip-hint")).toHaveText(
    "· zwykle ~2 h",
  );
  // Bez szacunku w linii podpowiedzi nie ma.
  await page.getByRole("textbox", { name: "Nowe zadanie" }).fill("Przegląd #Foundation");
  await expect(page.locator(".quick-chip-hint")).toHaveCount(0);

  // Rozwinięty kafelek: podpowiedź pod polem szacunku, z bieżącego szkicu.
  await page
    .getByRole("button", { name: "Przedziały: Code review", exact: true })
    .click();
  const estimate = page.getByLabel("Szacowany czas (minuty)");
  await expect(estimate).toHaveAccessibleDescription(
    "Szacunek 60 min · zwykle ~2 h",
  );
  await estimate.fill("90");
  await expect(page.locator(".estimate-hint")).toHaveText(
    "Szacunek 90 min · zwykle ~3 h",
  );
  await estimate.fill("0");
  await expect(page.locator(".estimate-hint")).toHaveCount(0);
});

test("correction e2e: no hint without enough history", async ({ page }) => {
  await openPreview(page);
  await page.getByRole("textbox", { name: "Nowe zadanie" }).fill("Przegląd ~1h");
  await expect(page.locator(".quick-chip-hint")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Przedziały: Code review", exact: true })
    .click();
  await expect(page.locator(".estimate-hint")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await expect(page.locator(".week-usual")).toHaveCount(0);
});

test("correction e2e: plan timeline uses corrected estimates when enabled", async ({
  page,
}) => {
  const data = () => {
    const d = correctionData();
    for (const t of d.tasks) {
      if (t.id === ids.analysis) t.details.plannedDay = "";
      if (t.id === ids.review) t.details.plannedDay = TODAY;
    }
    return d;
  };
  const timeline = () =>
    page.getByRole("region", { name: "Plan kontra rzeczywistość" });
  await openPreview(page, { data: data() });
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await expect(
    timeline().getByRole("button", { name: "Plan: Code review · 08:00–09:00" }),
  ).toBeVisible();

  await openPreview(page, {
    data: data(),
    settings: { planWithCorrection: true },
  });
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await expect(
    timeline().getByRole("button", { name: "Plan: Code review · 08:00–10:00" }),
  ).toBeVisible();
});

test("settings: „Planuj z poprawką szacunków” is off by default and saved", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const box = page.getByLabel("Planuj z poprawką szacunków");
  await expect(box).not.toBeChecked();
  await box.check();
  await expect(box).toBeChecked();
  expect(
    await page.evaluate(
      (k) => JSON.parse(localStorage.getItem(k)!).planWithCorrection,
      SETTINGS_KEY,
    ),
  ).toBe(true);
});
