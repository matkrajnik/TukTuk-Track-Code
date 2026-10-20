import { test, expect, type Page } from "@playwright/test";
import { tsCheckSaved, tsEntries } from "../src/timesheet";
import { dayTotal, decimalHours } from "../src/report";
import { previewTsKey, type Snapshot, type TsProjectMapping } from "../src/store";
import { at, ids, openPreview, seed, stored, TODAY } from "./fixtures/seed";

/**
 * Podgląd i wysyłka dnia do Ideo TS (PLAN_1.8.md, T3) na symulowanym TS
 * podglądu (`store.ts`, klucz `todo-track-preview-ts-v1`). Żaden test nie łączy
 * się z prawdziwym serwerem TS.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const DAY = "2026-10-15";
const t = (id: number, title: string, details: Record<string, unknown> = {}) => ({
  id,
  title,
  done: false,
  createdAt: 0,
  details: { project: "", notes: {}, ...details },
});
const s = (
  id: number,
  taskId: number,
  from: string,
  to: string | null,
  day = DAY,
) => ({
  id,
  taskId,
  startedAt: at(day, from),
  endedAt: to === null ? null : at(day, to),
});
const snapshot = (tasks: unknown[], sessions: unknown[]) =>
  ({ tasks, sessions }) as unknown as Snapshot;

test("tsEntries: scalanie dwóch zadań na jedno zadanie TS, opisy i tytuł zamiast notatki", () => {
  const data = snapshot(
    [
      t(1, "Analiza importu", {
        tsTaskId: "1101",
        tsTaskName: "Rozwój",
        notes: { [DAY]: "  Walidacja\n CSV  " },
      }),
      t(2, "Wdrożenie etykiet", { tsTaskId: "1101", tsTaskName: "Rozwój" }),
      // Ta sama notatka co w zadaniu 1 — bez powtórzenia w opisie.
      t(3, "Poprawki", {
        tsTaskId: "1101",
        tsTaskName: "Rozwój",
        notes: { [DAY]: "Walidacja CSV" },
      }),
      t(4, "Daily", { project: "zespół" }),
    ],
    [
      s(1, 1, "08:00", "10:00"),
      s(2, 2, "10:00", "10:30"),
      s(3, 3, "10:30", "10:45"),
      s(4, 4, "11:00", "11:15"),
    ],
  );
  const map: TsProjectMapping[] = [
    { project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" },
  ];
  const result = tsEntries(DAY, data, at(DAY, "12:00"), map);
  expect(
    result.entries.map(({ tsTaskId, tsTaskName, hours, overtime, description }) => ({
      tsTaskId,
      tsTaskName,
      hours,
      overtime,
      description,
    })),
  ).toEqual([
    {
      tsTaskId: "1101",
      tsTaskName: "Rozwój",
      hours: "2,75",
      overtime: "0",
      description: "Walidacja CSV; Wdrożenie etykiet",
    },
    {
      tsTaskId: "2201",
      tsTaskName: "Spotkania",
      hours: "0,25",
      overtime: "0",
      description: "Daily",
    },
  ]);
  expect(result.entries[0].tasks.map((task) => task.id)).toEqual([1, 2, 3]);
  expect(result.unmapped).toEqual([]);
  expect(result.totalMs).toBe(3 * 3_600_000);
  expect(result.running).toBe(false);
  // Bez notatek: zawsze tytuł zadania.
  expect(
    tsEntries(DAY, data, at(DAY, "12:00"), map, { includeNotes: false })
      .entries[0].description,
  ).toBe("Analiza importu; Wdrożenie etykiet; Poprawki");
});

test("tsEntries: zero pominięte, trwający pomiar do now, zadania bez zadania TS z czasem", () => {
  const data = snapshot(
    [
      t(1, "Analiza", { tsTaskId: "1101", tsTaskName: "Rozwój" }),
      t(2, "Code review", { project: "Foundation" }),
      // Pomiar rozpoczęty dokładnie teraz: zero czasu, nie trafia nigdzie.
      t(3, "Nowe", { tsTaskId: "1102", tsTaskName: "Błędy" }),
      t(4, "Wczoraj", { tsTaskId: "1103" }),
    ],
    [
      s(1, 1, "11:00", null),
      s(2, 2, "08:00", "09:30"),
      s(3, 3, "12:00", null),
      s(4, 4, "08:00", "09:00", "2026-10-14"),
    ],
  );
  const result = tsEntries(DAY, data, at(DAY, "12:00"), []);
  expect(result.entries.map((e) => [e.tsTaskId, e.hours])).toEqual([
    ["1101", "1,00"],
  ]);
  expect(result.running).toBe(true);
  expect(result.unmapped.map((u) => [u.task.title, u.ms])).toEqual([
    ["Code review", 90 * 60_000],
  ]);
  expect(result.unmappedMs).toBe(90 * 60_000);
  expect(result.totalMs).toBe(60 * 60_000);
  // Pomiar trwający od wczoraj liczy się w dniu od lokalnej północy.
  const overnight = snapshot(
    [t(1, "Noc", { tsTaskId: "1101" })],
    [s(1, 1, "23:00", null, "2026-10-14")],
  );
  expect(tsEntries(DAY, overnight, at(DAY, "00:30"), []).entries[0].hours).toBe(
    "0,50",
  );
});

test("tsEntries: zaokrąglenie po scaleniu zgodne z sumą raportu", () => {
  // Trzy razy 20 min na to samo zadanie TS: per wiersz 0,33 + 0,33 + 0,33 = 0,99,
  // po scaleniu 1,00 — jak „Razem” raportu.
  const data = snapshot(
    [
      t(1, "A", { tsTaskId: "1101" }),
      t(2, "B", { tsTaskId: "1101" }),
      t(3, "C", { tsTaskId: "1101" }),
    ],
    [s(1, 1, "08:00", "08:20"), s(2, 2, "08:20", "08:40"), s(3, 3, "09:00", "09:20")],
  );
  const now = at(DAY, "12:00");
  const result = tsEntries(DAY, data, now, []);
  expect(result.entries).toHaveLength(1);
  expect(result.entries[0].hours).toBe("1,00");
  expect(result.entries[0].hours).toBe(decimalHours(dayTotal(data, DAY, now)));
  // Brak nazwy zadania TS: identyfikator.
  expect(result.entries[0].tsTaskName).toBe("1101");
});

test("tsCheckSaved: liczba wpisów i suma po zapisie", () => {
  const e = (hours: string) => ({ hours });
  expect(tsCheckSaved([e("1,50"), e("0,25")], [e("1.5"), e("0,25")])).toEqual({
    count: 2,
    hours: "1,75",
    differs: false,
  });
  // Różnica 0,01 h mieści się w tolerancji, większa już nie.
  expect(tsCheckSaved([e("1,50")], [e("1,51")]).differs).toBe(false);
  expect(tsCheckSaved([e("1,50")], [e("1,52")]).differs).toBe(true);
  expect(tsCheckSaved([e("1,37")], [e("1,25")]).differs).toBe(true);
  // Inna liczba wpisów (np. TS pominął wpis z zerem).
  expect(tsCheckSaved([e("1"), e("0")], [e("1")])).toEqual({
    count: 1,
    hours: "1,00",
    differs: true,
  });
  expect(tsCheckSaved([], []).differs).toBe(false);
});

// ---------------------------------------------------------------------------
// Interfejs (podgląd przeglądarkowy, symulowany TS).

/**
 * Dzień z fixture z mapowaniami: „Analiza importu” (notatka dnia) i „Wdrożenie
 * etykiet” na to samo zadanie TS „Rozwój”, „Spotkanie zespołu” przez domyślne
 * zadanie projektu „Zespół”, „Code review” bez zadania TS (trwający pomiar).
 */
function tsSeed() {
  const data = seed({ active: { task: ids.review, from: "11:00" } });
  const task = (id: number) => data.tasks.find((x) => x.id === id)!;
  Object.assign(task(ids.analysis).details, {
    tsTaskId: "1101",
    tsTaskName: "Rozwój",
    notes: { ...task(ids.analysis).details.notes, [TODAY]: "Import CSV" },
  });
  Object.assign(task(ids.labels).details, {
    tsTaskId: "1101",
    tsTaskName: "Rozwój",
  });
  data.sessions.push({
    id: 100,
    taskId: ids.labels,
    startedAt: at(TODAY, "10:30"),
    endedAt: at(TODAY, "11:00"),
  });
  return data;
}
const existing = [
  { id: "2201", hours: "0,25", overtime: "0", description: "Daily" },
  { id: "2202", hours: "1.5", overtime: "0", description: "Administracja zespołu" },
];
async function setTs(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [previewTsKey, JSON.stringify(state)] as const,
  );
  await page.reload();
}
const patchTs = (page: Page, patch: Record<string, unknown>) =>
  page.evaluate(
    ([key, value]) =>
      localStorage.setItem(
        key,
        JSON.stringify({
          ...JSON.parse(localStorage.getItem(key) ?? "{}"),
          ...JSON.parse(value),
        }),
      ),
    [previewTsKey, JSON.stringify(patch)] as const,
  );
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);
const connected = (filled: unknown[] = existing) => ({
  connected: true,
  login: "jan",
  projectMap: [{ project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" }],
  filled: { [TODAY]: filled },
});
const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
async function openDayClose(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await expect(page.getByRole("heading", { name: /^Zamknięcie dnia/ })).toBeVisible();
}
async function openReport(page: Page) {
  await nav(page, "Raporty dzienne");
  await nav(page, "Raport TS");
  await expect(page.getByRole("region", { name: "Raport TS" })).toBeVisible();
}
const dialog = (page: Page) => page.getByRole("dialog", { name: "Wyślij do TS" });
const rows = (page: Page) =>
  dialog(page).getByRole("table", { name: "Wpisy do wysłania" }).locator("tbody tr");

test("Zamknięcie dnia: podgląd ze scalonymi wierszami, ostrzeżeniami i „Zachowaj” dokleja wpisy TS", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed() });
  await setTs(page, connected());
  await openDayClose(page);
  const section = page.locator(".dashboard-card", {
    has: page.getByRole("heading", { name: /^Zamknięcie dnia/ }),
  });
  // Kolejność kroków: Kopiuj raport TS → Wyślij do TS → Oznacz dzień.
  await expect(section.locator(".planning-actions button")).toHaveText([
    "Kopiuj raport TS",
    "Wyślij do TS",
    "Oznacz dzień jako sprawdzony",
  ]);
  await section.getByRole("button", { name: "Wyślij do TS" }).click();
  const d = dialog(page);
  await expect(d).toBeVisible();
  // Rozwój: Analiza 08:30–10:30 + Etykiety 10:30–11:00; Spotkania z projektu.
  await expect(rows(page)).toHaveCount(2);
  await expect(rows(page).nth(0).locator("td")).toHaveText([
    "Analiza importu, Wdrożenie etykiet",
    "Rozwój",
    "2,50",
    "",
  ]);
  await expect(
    d.getByRole("textbox", { name: "Opis wpisu TS: Rozwój" }),
  ).toHaveValue("Import CSV; Wdrożenie etykiet");
  await expect(rows(page).nth(1).locator("td").nth(1)).toHaveText(
    "Spotkaniazastąpi 0,25 h",
  );
  await expect(
    d.getByRole("textbox", { name: "Opis wpisu TS: Spotkania" }),
  ).toHaveValue("Spotkanie zespołu");
  await expect(d.locator("tfoot")).toContainText("Razem3,00");
  // Ostrzeżenia: zadanie bez zadania TS (trwający pomiar 11:00–12:00).
  const warnings = d.getByRole("note", { name: "Ostrzeżenia wysyłki" });
  await expect(warnings).toContainText(
    "Bez zadania TS: 1 zadanie (1,00 h) — ten czas nie zostanie wysłany.",
  );
  await expect(warnings).toContainText("Trwający pomiar liczony do teraz");
  await expect(warnings.getByRole("button", { name: "Pokaż zadanie: Code review" })).toBeVisible();
  await expect(warnings).not.toContainText("nie występuje na liście");
  // Istniejące wpisy TS (nazwy z listy zadań TS, godziny z przecinkiem).
  const existingBlock = d.getByRole("region", { name: "Wpisy w TS" });
  await expect(existingBlock).toContainText(
    "W TS na ten dzień są już 2 wpisy (razem 1,75 h)",
  );
  const items = existingBlock.locator("li");
  await expect(items.nth(0)).toContainText("Spotkania0,25 hDailyzostanie zastąpiony");
  await expect(items.nth(1)).toContainText("Administracja1,50 hAdministracja zespołu");
  await expect(items.nth(1)).not.toContainText("zastąpiony");
  await expect(d.getByRole("radio", { name: "Zachowaj pozostałe wpisy TS" })).toBeChecked();

  // Opis edytowany w podglądzie nie zmienia notatki dnia.
  await d
    .getByRole("textbox", { name: "Opis wpisu TS: Rozwój" })
    .fill("  Import   CSV i etykiety ");
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 3 wpisy, razem 4,50 h.",
  );
  await expect(d.getByRole("alert")).toHaveCount(0);
  expect((await tsState(page)).filled[TODAY]).toEqual([
    { id: "1101", hours: "2,50", overtime: "0", description: "Import CSV i etykiety" },
    { id: "2201", hours: "0,50", overtime: "0", description: "Spotkanie zespołu" },
    { id: "2202", hours: "1.5", overtime: "0", description: "Administracja zespołu" },
  ]);
  const note = (await stored(page)).tasks.find(
    (x: { id: number }) => x.id === ids.analysis,
  ).details.notes[TODAY];
  expect(note).toBe("Import CSV");
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(d).toHaveCount(0);
  // Znacznik wysyłki; dzień nie zostaje zamknięty.
  await expect(section.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (4,50 h)",
  );
  expect((await stored(page)).closedDays).not.toContain(TODAY);
  await expect(
    section.getByRole("button", { name: "Oznacz dzień jako sprawdzony" }),
  ).toBeVisible();
});

test("Raport dnia: „Zastąp wszystko” usuwa wpisy TS, znacznik i zmiana raportu po wysyłce", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed() });
  // Lista zadań TS tego dnia bez „Spotkań”: ostrzeżenie w podglądzie.
  await setTs(page, {
    ...connected(),
    tasks: [{ name: "WMS", tasks: [{ id: "1101", name: "Rozwój" }] }],
  });
  await openReport(page);
  const region = page.getByRole("region", { name: "Raport TS" });
  await region.getByRole("button", { name: "Wyślij do TS" }).click();
  const d = dialog(page);
  await expect(rows(page)).toHaveCount(2);
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toContainText(
    "Zadanie TS „Spotkania” nie występuje na liście tego dnia.",
  );
  // Nazwa wpisu spoza listy: identyfikator.
  await expect(d.getByRole("region", { name: "Wpisy w TS" })).toContainText(
    "Zadanie TS 2202",
  );
  await d.getByRole("radio", { name: "Zastąp wszystko" }).check();
  await expect(d.getByRole("region", { name: "Wpisy w TS" }).locator("li").first()).toContainText(
    "zniknie",
  );
  await expect(rows(page).nth(1)).not.toContainText("zastąpi");
  await d.getByRole("button", { name: "Zastąp i zapisz" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 2 wpisy, razem 3,00 h.",
  );
  expect((await tsState(page)).filled[TODAY].map((f: { id: string }) => f.id)).toEqual([
    "1101",
    "2201",
  ]);
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(region.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (3,00 h)",
  );
  // Ten sam znacznik w Zamknięciu dnia.
  await openDayClose(page);
  await expect(page.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (3,00 h)",
  );
  // Nowy pomiar zadania z mapowaniem: raport różni się od wysłanego.
  await page.evaluate(
    ([key, start, end]) => {
      const data = JSON.parse(localStorage.getItem(key)!);
      data.sessions.push({ id: 200, taskId: 2, startedAt: start, endedAt: end });
      localStorage.setItem(key, JSON.stringify(data));
    },
    ["todo-track-preview-v1", at(TODAY, "07:00"), at(TODAY, "07:30")] as const,
  );
  await page.reload();
  await openReport(page);
  await expect(region.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (3,00 h) · raport zmienił się od wysyłki",
  );
});

test("błąd zapisu zostawia dane w dialogu, a pusty dzień w TS i link do kafelka", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed() });
  await setTs(page, { ...connected([]), ignoreSave: true });
  await openReport(page);
  await page
    .getByRole("region", { name: "Raport TS" })
    .getByRole("button", { name: "Wyślij do TS" })
    .click();
  const d = dialog(page);
  await expect(d.getByRole("region", { name: "Wpisy w TS" })).toHaveText(
    "Dzień w TS jest pusty.",
  );
  await expect(d.getByRole("radio")).toHaveCount(0);
  const description = d.getByRole("textbox", { name: "Opis wpisu TS: Rozwój" });
  await description.fill("Opis po poprawce");
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("alert")).toHaveText(
    "TS nie potwierdził zapisu — sprawdź dzień w przeglądarce.",
  );
  // Dane zostają: wiersze, edytowany opis, przycisk zapisu.
  await expect(rows(page)).toHaveCount(2);
  await expect(description).toHaveValue("Opis po poprawce");
  await expect(page.locator(".ts-sent-marker")).toHaveCount(0);
  // Po usunięciu przyczyny ponowna próba zapisuje.
  await patchTs(page, { ignoreSave: false });
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 2 wpisy, razem 3,00 h.",
  );
  expect((await tsState(page)).filled[TODAY][0].description).toBe("Opis po poprawce");
  await d.getByRole("button", { name: "Zamknij" }).click();

  // Link do kafelka zadania bez zadania TS.
  await page
    .getByRole("region", { name: "Raport TS" })
    .getByRole("button", { name: "Wyślij do TS" })
    .click();
  await dialog(page).getByRole("button", { name: "Pokaż zadanie: Code review" }).click();
  await expect(dialog(page)).toHaveCount(0);
  await expect(page.getByRole("region", { name: "Zadanie TS: Code review" })).toBeVisible();
});

test("zapis bez odczytu dnia po zapisie: ostrzeżenie i znacznik z sumą wysłaną", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed() });
  await setTs(page, { ...connected([]), failReadBack: true });
  await openDayClose(page);
  await page.getByRole("button", { name: "Wyślij do TS" }).click();
  const d = dialog(page);
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "Wysłano do TS 2 wpisy, razem 3,00 h z raportu.",
  );
  await expect(d.getByRole("alert")).toHaveText(
    "Zapisano w TS, ale nie udało się odczytać dnia — sprawdź w przeglądarce.",
  );
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(page.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (3,00 h)",
  );
});

test("przycisk „Wyślij do TS”: bez konta link do Ustawień, wyłączony przełącznik, dzień z przyszłości", async ({
  page,
}) => {
  await openPreview(page, { data: tsSeed() });
  await openDayClose(page);
  await expect(page.getByRole("button", { name: "Wyślij do TS" })).toHaveCount(0);
  await page.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expect(page.getByRole("searchbox", { name: "Szukaj ustawienia" })).toBeVisible();

  // Wyłączony przełącznik „Wysyłaj raport TS”: ani przycisku, ani linku.
  await openPreview(page, { data: tsSeed(), settings: { tsSend: false } });
  await setTs(page, connected());
  await openDayClose(page);
  await expect(page.getByRole("button", { name: "Wyślij do TS" })).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }),
  ).toHaveCount(0);
  await openReport(page);
  await expect(page.getByRole("button", { name: "Wyślij do TS" })).toHaveCount(0);

  // Połączone konto: dzień z przyszłości blokuje przycisk.
  await openPreview(page, { data: tsSeed() });
  await setTs(page, connected());
  await openReport(page);
  const send = page
    .getByRole("region", { name: "Raport TS" })
    .getByRole("button", { name: "Wyślij do TS" });
  await expect(send).toBeEnabled();
  await page.getByRole("button", { name: "Następny dzień" }).click();
  await expect(send).toBeDisabled();
});
