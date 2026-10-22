import { test, expect, type Page } from "./fixtures/test";
import { pick } from "./fixtures/searchSelect";
import { resolveTsTask, tsCheckSaved, tsEntries } from "../src/timesheet";
import { dayTotal, decimalHours } from "../src/report";
import {
  previewTsKey,
  type Snapshot,
  type Task,
  type TsProjectMapping,
} from "../src/store";
import { matchingSections, sectionInMessage } from "../src/navigation";
import {
  at,
  ids,
  openPreview,
  stored,
  TODAY,
  TS_EXISTING,
  TS_KEY,
  tsFixture,
  tsSeed,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Wysyłka Raportu TS do Ideo Timesheet (PLAN_1.8.md, T2–T5) na symulowanym TS
 * podglądu (`store.ts`, klucz `tuktuk-track-preview-ts-v1`). Żaden test nie łączy
 * się z prawdziwym serwerem TS. Reguły samego symulowanego TS (walidacja,
 * „keep”, suma godzin) są w `timesheet-store.spec.ts`.
 *
 * Kolejność: T2 (mapowanie zadań), T3 (podgląd i wysyłka), T4 (Ustawienia),
 * T5 (pełna ścieżka i nasłuch zmian symulowanego TS z innego okna).
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const task = (details: Partial<Task["details"]> & object) =>
  ({ details: { project: "", ...details } }) as Pick<Task, "details">;
const mapping = (project: string, tsTaskId: string, tsTaskName: string) => ({
  project,
  tsTaskId,
  tsTaskName,
});

test("resolveTsTask: jawny wybór, mapowanie projektu (bez wielkości liter), brak", () => {
  const map: TsProjectMapping[] = [
    mapping("WMS", "1101", "Rozwój"),
    mapping("Zespół", "2201", "Spotkania"),
  ];
  // Jawny wybór zadania wygrywa z mapowaniem projektu.
  expect(
    resolveTsTask(
      task({ project: "WMS", tsTaskId: "1103", tsTaskName: "Code review" }),
      map,
    ),
  ).toEqual({ id: "1103", name: "Code review", source: "task" });
  // Bez nazwy pokazuje się identyfikator.
  expect(resolveTsTask(task({ tsTaskId: " 77 " }), [])).toEqual({
    id: "77",
    name: "77",
    source: "task",
  });
  // Mapowanie projektu, nazwa projektu bez wielkości liter i białych znaków.
  expect(resolveTsTask(task({ project: "WMS" }), map)).toEqual({
    id: "1101",
    name: "Rozwój",
    source: "project",
  });
  expect(resolveTsTask(task({ project: " wms " }), map)?.id).toBe("1101");
  // Jak SQLite NOCASE: tylko litery ASCII (Ł ≠ ł).
  expect(resolveTsTask(task({ project: "ZESPÓŁ" }), map)).toBeNull();
  expect(resolveTsTask(task({ project: "Zespół" }), map)?.id).toBe("2201");
  // Puste id zadania = brak jawnego wyboru.
  expect(resolveTsTask(task({ project: "WMS", tsTaskId: "  " }), map)?.source).toBe(
    "project",
  );
  // Brak: bez projektu, projekt bez mapowania, zadanie bez szczegółów.
  expect(resolveTsTask(task({}), map)).toBeNull();
  expect(resolveTsTask(task({ project: "Foundation" }), map)).toBeNull();
  expect(resolveTsTask({ details: undefined } as never, map)).toBeNull();
});

const card = (page: Page, title: string) =>
  page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: title }) });
const expand = (page: Page, title: string) =>
  page.getByRole("button", { name: "Przedziały: " + title }).click();
async function setTs(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [previewTsKey, JSON.stringify(state)] as const,
  );
  await page.reload();
}
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);
/** Nagłówki grup (projekty TS) otwartej listy pola SearchSelect. */
const groups = (page: Page) =>
  page.getByRole("listbox").locator(".search-select-group");
const detailsOf = async (page: Page, id: number) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === id).details;

test("SearchSelect: kliknięcie w nagłówek grupy i pusty wynik nie zamyka listy", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  await expand(page, "Code review");
  const select = page.getByRole("combobox", { name: "Zadanie TS: Code review" });
  await select.click();
  await groups(page).first().click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await expect(select).toBeFocused();
  await expect(select).toHaveAttribute("aria-expanded", "true");
  // Wiersz „Brak pasujących pozycji” też nie zabiera fokusu.
  await select.fill("zzz-brak");
  await page.locator(".search-select-empty").click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await expect(select).toBeFocused();
  // Wybór dalej działa po kliknięciach w listę.
  await select.fill("Code");
  await page
    .getByRole("listbox")
    .getByRole("option", { name: "Code review", exact: true })
    .click();
  await expect(select).toHaveAttribute("data-value", "1103");
});

test("pole „Zadanie TS” w kafelku: wybór z listy, zapamiętanie i stan bez połączenia", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  await expand(page, "Code review");
  const select = page.getByRole("combobox", { name: "Zadanie TS: Code review" });
  await expect(select).toHaveAttribute("data-value", "");
  // Grupy per projekt TS i opcja „— brak —”.
  await select.click();
  await expect(groups(page)).toHaveCount(2);
  await expect(groups(page).first()).toHaveText("WMS");
  await expect(
    page.getByRole("listbox").getByRole("option").first(),
  ).toHaveText("— brak —");
  await select.press("Escape");
  await pick(select, "Code review");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText(
    "TS: Code review",
  );
  const saved = await detailsOf(page, ids.review);
  expect([saved.tsTaskId, saved.tsTaskName]).toEqual(["1103", "Code review"]);
  // Pozostałe szczegóły bez zmian.
  expect([saved.project, saved.estimateMinutes]).toEqual(["Foundation", 60]);

  // Po przeładowaniu wybór zostaje.
  await page.reload();
  await expand(page, "Code review");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toHaveAttribute("data-value", "1103");

  // Zadanie zniknęło z listy TS: opcja „(niedostępne)” z ostrzeżeniem.
  await setTs(page, {
    connected: true,
    login: "jan",
    tasks: [{ name: "WMS", tasks: [{ id: "1101", name: "Rozwój" }] }],
  });
  await expand(page, "Code review");
  const gone = page.getByRole("combobox", { name: "Zadanie TS: Code review" });
  await expect(gone).toHaveAttribute("data-value", "1103");
  await expect(gone).toHaveValue("(niedostępne) Code review");
  await expect(
    page
      .getByRole("region", { name: "Zadanie TS: Code review" })
      .getByRole("alert"),
  ).toContainText(
    "Zadania TS „Code review” nie ma na dzisiejszej liście",
  );
  // „Odśwież” pobiera listę ponownie (pamięć podręczna na czas działania).
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    delete s.tasks;
    localStorage.setItem(k, JSON.stringify(s));
  }, previewTsKey);
  await gone.click();
  await expect(groups(page)).toHaveCount(1);
  await gone.press("Escape");
  await page.getByRole("button", { name: "Odśwież listę zadań TS" }).click();
  await gone.click();
  await expect(groups(page)).toHaveCount(2);
  await gone.press("Escape");
  await expect(gone).toHaveValue("Code review");

  // „— brak —” usuwa wybór z zadania.
  await pick(gone, "— brak —", "brak");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);
  const cleared = await detailsOf(page, ids.review);
  expect("tsTaskId" in cleared || "tsTaskName" in cleared).toBe(false);
  await pick(gone, "Błędy i wsparcie");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText(
    "TS: Błędy i wsparcie",
  );

  // Bez połączenia: zapamiętana nazwa i link do Ustawień.
  await setTs(page, { connected: false, login: "jan" });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await expect(field).toContainText("Zadanie TS: Błędy i wsparcie");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toHaveCount(0);
  await field.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expect(page.getByRole("searchbox", { name: "Szukaj ustawienia" })).toBeVisible();
});

test("„Ustaw jako domyślne dla projektu”: mapowanie projektu działa na zadania bez wyboru", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  await expand(page, "Wdrożenie etykiet");
  const select = page.getByRole("combobox", { name: "Zadanie TS: Wdrożenie etykiet" });
  const asDefault = page.getByRole("checkbox", {
    name: "Ustaw jako domyślne dla projektu „WMS”",
  });
  // Pole wyboru pojawia się dopiero po wybraniu zadania TS.
  await expect(asDefault).toHaveCount(0);
  await pick(select, "Rozwój");
  await expect(asDefault).not.toBeChecked();
  await asDefault.check();
  await expect.poll(async () => (await tsState(page)).projectMap).toEqual([
    { project: "WMS", tsTaskId: "1101", tsTaskName: "Rozwój" },
  ]);
  // Drugie zadanie projektu WMS (bez jawnego wyboru) dziedziczy mapowanie.
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Rozwój (z projektu)",
  );
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Rozwój",
  );
  // Mapowanie nie jest kopiowane do szczegółów zadania.
  expect((await detailsOf(page, ids.analysis)).tsTaskId).toBeUndefined();
  // Zadanie z innego projektu nie ma chipa.
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);

  // Po przeładowaniu pole wyboru jest zaznaczone; zmiana wyboru w zadaniu
  // NIE zmienia domyślnego zadania projektu (pole wyboru się odznacza).
  await page.reload();
  await expand(page, "Wdrożenie etykiet");
  await expect(asDefault).toBeChecked();
  await pick(select, "Code review");
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Code review",
  );
  await expect(asDefault).not.toBeChecked();
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Rozwój (z projektu)",
  );
  expect((await tsState(page)).projectMap).toEqual([
    { project: "WMS", tsTaskId: "1101", tsTaskName: "Rozwój" },
  ]);
  // „— brak —” przy zadaniu równym domyślnemu: zadanie dziedziczy z projektu.
  await pick(select, "— brak —", "brak");
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Rozwój (z projektu)",
  );
  await pick(select, "Code review");
  // Dopiero zaznaczenie pola przenosi domyślne zadanie projektu.
  await asDefault.check();
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Code review (z projektu)",
  );
  // Rozwinięte zadanie bez jawnego wyboru pokazuje domyślne z projektu.
  await expand(page, "Analiza importu");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Analiza importu" }),
  ).toHaveAttribute("data-value", "");
  await expect(
    page.getByRole("region", { name: "Zadanie TS: Analiza importu" }),
  ).toContainText("Domyślnie z projektu „WMS”: Code review");

  // Odznaczenie usuwa mapowanie projektu; jawny wybór zadania zostaje.
  await asDefault.uncheck();
  await expect.poll(async () => (await tsState(page)).projectMap).toEqual([]);
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveCount(0);
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Code review",
  );
});

// ---------------------------------------------------------------------------
// T3: podgląd i wysyłka dnia.

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

test("tsEntries: wpis poniżej 0,01 h po scaleniu nie idzie do wysyłki", () => {
  // 10 s + 5 s na zadanie TS 1102: po scaleniu 15 s → „0,00”. Taki wpis przy
  // „Zachowaj” zastąpiłby wpis TS o tym zadaniu zerem, więc trafia do tooShort.
  const sec = (h: string, sx: number) => at(DAY, h) + sx * 1000;
  const data = {
    tasks: [
      t(1, "Analiza", { tsTaskId: "1101", tsTaskName: "Rozwój" }),
      t(2, "Krótkie A", { tsTaskId: "1102", tsTaskName: "Błędy" }),
      t(3, "Krótkie B", { tsTaskId: "1102", tsTaskName: "Błędy" }),
    ],
    sessions: [
      { id: 1, taskId: 1, startedAt: at(DAY, "08:00"), endedAt: at(DAY, "09:00") },
      { id: 2, taskId: 2, startedAt: at(DAY, "09:00"), endedAt: sec("09:00", 10) },
      { id: 3, taskId: 3, startedAt: at(DAY, "09:01"), endedAt: sec("09:01", 5) },
    ],
  } as unknown as Snapshot;
  const result = tsEntries(DAY, data, at(DAY, "12:00"), []);
  expect(result.entries.map((e) => [e.tsTaskId, e.hours])).toEqual([
    ["1101", "1,00"],
  ]);
  expect(result.tooShort.map((e) => [e.tsTaskId, e.hours, e.ms])).toEqual([
    ["1102", "0,00", 15_000],
  ]);
  expect(result.tooShort[0].tasks.map((x) => x.title)).toEqual([
    "Krótkie A",
    "Krótkie B",
  ]);
  expect(result.totalMs).toBe(3_600_000);
  expect(result.totalHours).toBe("1,00");
});

test("tsEntries: totalHours to suma godzin wpisów po zaokrągleniu", () => {
  // Trzy zadania TS po 20 min: per wpis 0,33 → razem 0,99 (a nie 1,00 z sumy ms),
  // tak jak suma wpisów wysłanych do TS.
  const data = snapshot(
    [
      t(1, "A", { tsTaskId: "1101" }),
      t(2, "B", { tsTaskId: "1102" }),
      t(3, "C", { tsTaskId: "1103" }),
    ],
    [s(1, 1, "08:00", "08:20"), s(2, 2, "08:20", "08:40"), s(3, 3, "09:00", "09:20")],
  );
  const result = tsEntries(DAY, data, at(DAY, "12:00"), []);
  expect(result.entries.map((e) => e.hours)).toEqual(["0,33", "0,33", "0,33"]);
  expect(result.totalHours).toBe("0,99");
  expect(decimalHours(result.totalMs)).toBe("1,00");
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

// Dane: `tsSeed()` z `fixtures/seed.ts` — „Analiza importu” (notatka dnia)
// i „Wdrożenie etykiet” na to samo zadanie TS „Rozwój”, „Spotkanie zespołu”
// przez domyślne zadanie projektu „Zespół”, „Code review” bez zadania TS
// (trwający pomiar).
const existing = [
  { id: "2201", hours: "0,25", overtime: "0", description: "Daily" },
  { id: "2202", hours: "1.5", overtime: "0", description: "Administracja zespołu" },
];
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
const connected = (filled: unknown[] = existing) => ({
  ...tsFixture(),
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
    ["tuktuk-track-preview-v1", at(TODAY, "07:00"), at(TODAY, "07:30")] as const,
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

test("wpis poniżej 0,01 h nie zastępuje wpisu TS, a znacznik bez lokalnej sumy nie zgłasza zmiany", async ({
  page,
}) => {
  const data = tsSeed();
  const template = data.tasks.find((x) => x.id === ids.analysis)!;
  data.tasks.push({
    ...structuredClone(template),
    id: 900,
    title: "Krótka poprawka",
    details: {
      ...structuredClone(template.details),
      notes: {},
      tsTaskId: "2202",
      tsTaskName: "Administracja",
    },
  });
  // 10 s pracy: po zaokrągleniu „0,00” h.
  data.sessions.push({
    id: 901,
    taskId: 900,
    startedAt: at(TODAY, "06:00"),
    endedAt: at(TODAY, "06:00") + 10_000,
  });
  await openPreview(page, { data });
  await setTs(page, connected());
  await openReport(page);
  const region = page.getByRole("region", { name: "Raport TS" });
  await region.getByRole("button", { name: "Wyślij do TS" }).click();
  const d = dialog(page);
  await expect(rows(page)).toHaveCount(2);
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toContainText(
    "Zadanie TS „Administracja” ma mniej niż 0,01 h — ten wpis nie zostanie wysłany, a wpis w TS o tym zadaniu zostanie bez zmian.",
  );
  // Wpis TS „Administracja” nie jest oznaczony do zastąpienia.
  await expect(
    d.getByRole("region", { name: "Wpisy w TS" }).locator("li").nth(1),
  ).not.toContainText("zastąpiony");
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 3 wpisy, razem 4,50 h.",
  );
  await expect(d.getByRole("alert")).toHaveCount(0);
  expect((await tsState(page)).filled[TODAY]).toContainEqual({
    id: "2202",
    hours: "1.5",
    overtime: "0",
    description: "Administracja zespołu",
  });
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(region.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (4,50 h)",
  );
  // Inne urządzenie albo wyczyszczona pamięć: brak lokalnej sumy wysłanych
  // wpisów raportu. Suma z TS (4,50 h, z wpisem spoza raportu) różni się od
  // raportu (3,00 h), ale znacznik nie zgłasza zmiany raportu.
  await page.evaluate(() =>
    localStorage.removeItem("tuktuk-track-ts-sent-report-v1"),
  );
  await page.reload();
  await openReport(page);
  await expect(region.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (4,50 h)",
  );
});

// ---------------------------------------------------------------------------
// T4: Ustawienia → Timesheet (TS).

const SECTION = "Timesheet (TS)";
const panel = (page: Page) => page.getByRole("tabpanel");
async function expectTimesheet(page: Page) {
  await expect(page.getByRole("tab", { name: SECTION, exact: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(
    panel(page).getByRole("heading", { name: SECTION, exact: true }),
  ).toBeVisible();
}

test("sekcja Timesheet: wyszukiwanie, komunikat i navigate prowadzą do sekcji (moduł)", () => {
  for (const q of ["Timesheet", "TS", "ideo"])
    expect(matchingSections(q).map((s) => s.id)).toEqual(["timesheet"]);
  expect(
    sectionInMessage("Wysyłka do TS jest wyłączona w Ustawieniach → Timesheet."),
  ).toBe("timesheet");
  expect(
    sectionInMessage("Połącz konto Microsoft w Ustawieniach → Kalendarz."),
  ).toBe("calendar");
});

test("połącz, błąd przy złym haśle, test połączenia i rozłącz", async ({ page }) => {
  await openPreview(page);
  await openSettings(page, SECTION);
  await expectTimesheet(page);
  await expect(panel(page).getByText("Nie połączono", { exact: true })).toBeVisible();
  await expect(panel(page).getByRole("button", { name: "Testuj połączenie" })).toHaveCount(0);

  const login = panel(page).getByLabel("Login", { exact: true });
  const password = panel(page).getByLabel("Hasło", { exact: true });
  await expect(password).toHaveAttribute("type", "password");
  const connect = panel(page).getByRole("button", { name: "Połącz", exact: true });
  await expect(connect).toBeDisabled();

  // Złe hasło: komunikat z backendu, nadal niepołączone, hasło nie zostaje w polu.
  await login.fill("jan");
  await password.fill("zle");
  await connect.click();
  await expect(panel(page).getByRole("alert")).toHaveText(
    "TS odrzucił logowanie. Sprawdź login i hasło.",
  );
  await expect(password).toHaveValue("");
  await expect(panel(page).getByText("Nie połączono", { exact: true })).toBeVisible();
  expect((await tsState(page)).connected).not.toBe(true);

  // Zaawansowane: adres API domyślny, identyfikator urządzenia tylko do odczytu.
  await panel(page).getByText("Zaawansowane", { exact: true }).click();
  await expect(panel(page).getByLabel("Adres API")).toHaveValue("https://api.ts.ideo.pl");
  const device = panel(page).getByLabel("Identyfikator urządzenia");
  await expect(device).toHaveAttribute("readonly", "");
  await expect(device).toHaveValue("");

  await password.fill("tajne-haslo");
  await connect.click();
  await expect(panel(page).getByText("Połączono jako jan", { exact: true })).toBeVisible();
  await expect(password).toHaveValue("");
  await expect(
    panel(page).getByText("Hasło jest zapisane w magazynie poświadczeń Windows", {
      exact: false,
    }),
  ).toBeVisible();
  await expect(device).toHaveValue(/^[0-9a-f]{32}$/);
  // Hasło nigdzie nie trafia do pamięci podglądu.
  const stored = await page.evaluate(() => JSON.stringify({ ...localStorage }));
  expect(stored).not.toContain("tajne-haslo");

  // Test połączenia: lista zadań z formularza dnia (przykładowe: 5 zadań w 2 projektach).
  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("status")).toHaveText(
    "Połączono: 5 zadań w 2 projektach.",
  );
  // Błąd testu (np. sesja wygasła po stronie TS) pokazuje komunikat.
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    localStorage.setItem(k, JSON.stringify({ ...s, connected: false }));
  }, previewTsKey);
  await panel(page).getByRole("button", { name: "Testuj połączenie" }).click();
  await expect(panel(page).getByRole("alert")).toHaveText(
    "Połącz konto TS: podaj login i hasło.",
  );
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    localStorage.setItem(k, JSON.stringify({ ...s, connected: true }));
  }, previewTsKey);

  // Rozłącz z potwierdzeniem: Anuluj nic nie zmienia.
  await panel(page).getByRole("button", { name: "Rozłącz" }).click();
  const confirm = page.getByRole("dialog", { name: "Rozłączyć konto TS?" });
  await expect(confirm).toBeVisible();
  await confirm.getByRole("button", { name: "Anuluj" }).click();
  await expect(confirm).toHaveCount(0);
  await expect(panel(page).getByText("Połączono jako jan", { exact: true })).toBeVisible();
  await panel(page).getByRole("button", { name: "Rozłącz" }).click();
  await confirm.getByRole("button", { name: "Rozłącz" }).click();
  await expect(confirm).toHaveCount(0);
  await expect(panel(page).getByText("Nie połączono", { exact: true })).toBeVisible();
  // Login i identyfikator urządzenia zostają.
  await expect(login).toHaveValue("jan");
  const state = await tsState(page);
  expect(state.connected).toBe(false);
  expect(state.login).toBe("jan");
  expect(state.deviceId).toMatch(/^[0-9a-f]{32}$/);
});

test("przełącznik „Wysyłaj raport TS” chowa przyciski w Zamknięciu dnia i Raporcie dnia", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  const dayCloseSend = async () => {
    await nav(page, "Plan i historia");
    await nav(page, "Zamknięcie dnia");
    await expect(page.getByRole("heading", { name: /^Zamknięcie dnia/ })).toBeVisible();
    return page.getByRole("button", { name: "Wyślij do TS" });
  };
  const reportSend = async () => {
    await nav(page, "Raporty dzienne");
    const toggle = page.getByRole("button", { name: "Raport TS" });
    const region = page.getByRole("region", { name: "Raport TS" });
    // „Raport TS” przełącza sekcję, a widok (KeepAlive) pamięta, że była otwarta.
    if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
    await expect(region).toBeVisible();
    return region.getByRole("button", { name: "Wyślij do TS" });
  };
  await expect(await dayCloseSend()).toBeVisible();
  await expect(await reportSend()).toBeVisible();

  await openSettings(page, SECTION);
  const toggle = panel(page).getByLabel("Wysyłaj raport TS");
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect(toggle).not.toBeChecked();
  await expect(await dayCloseSend()).toHaveCount(0);
  await expect(await reportSend()).toHaveCount(0);
  // Ustawienie przeżywa przeładowanie.
  await page.reload();
  await openSettings(page, SECTION);
  await expect(panel(page).getByLabel("Wysyłaj raport TS")).not.toBeChecked();

  await panel(page).getByLabel("Wysyłaj raport TS").check();
  await expect(await dayCloseSend()).toBeVisible();
  await expect(await reportSend()).toBeVisible();
});

test("domyślne zadania TS projektów: dodanie, lista i usunięcie mapowania", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, SECTION);
  // Bez połączenia lista zadań TS jest pusta, a dodawanie wyłączone.
  await expect(panel(page).getByText("Brak domyślnych zadań TS.")).toBeVisible();
  await expect(panel(page).getByLabel("Domyślne zadanie TS")).toBeDisabled();
  await expect(
    panel(page).getByText("Lista zadań TS pojawi się po połączeniu konta."),
  ).toBeVisible();

  await setTs(page, {
    connected: true,
    login: "jan",
    projectMap: [{ project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" }],
  });
  await openSettings(page, SECTION);
  const table = panel(page).getByRole("table", {
    name: "Domyślne zadania TS dla projektów",
  });
  await expect(table.locator("tbody tr")).toHaveText([/Zespół\s*Wewnętrzne → Spotkania\s*Usuń/]);

  // Projekty z listy projektów zadań, zadania TS z formularza dnia (dzisiaj).
  const project = panel(page).getByLabel("Projekt", { exact: true });
  await expect(project).toHaveAttribute("placeholder", "— wybierz —");
  await project.click();
  await expect(page.getByRole("listbox").getByRole("option")).toHaveText([
    "Foundation",
    "WMS",
    "Zespół",
  ]);
  await project.press("Escape");
  const add = panel(page).getByRole("button", { name: "Dodaj mapowanie" });
  await expect(add).toBeDisabled();
  await pick(project, "WMS");
  await pick(panel(page).getByLabel("Domyślne zadanie TS"), "Code review");
  await add.click();
  await expect(table.locator("tbody tr")).toHaveText([
    /WMS\s*WMS → Code review\s*Usuń/,
    /Zespół\s*Wewnętrzne → Spotkania\s*Usuń/,
  ]);
  expect((await tsState(page)).projectMap).toEqual(
    expect.arrayContaining([
      { project: "WMS", tsTaskId: "1103", tsTaskName: "Code review" },
    ]),
  );
  await expect(project).toHaveValue("");

  await panel(page).getByRole("button", { name: "Usuń mapowanie projektu Zespół" }).click();
  await expect(table.locator("tbody tr")).toHaveText([/WMS\s*WMS → Code review/]);
  await panel(page).getByRole("button", { name: "Usuń mapowanie projektu WMS" }).click();
  await expect(panel(page).getByText("Brak domyślnych zadań TS.")).toBeVisible();
  expect((await tsState(page)).projectMap).toEqual([]);
});

test("„Połącz konto TS w Ustawieniach” i navigate z sekcją timesheet otwierają sekcję", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await page.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expectTimesheet(page);

  await nav(page, "Raporty dzienne");
  await nav(page, "Raport TS");
  await page.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expectTimesheet(page);
  // Po połączeniu Raport dnia (KeepAlive) od razu pokazuje „Wyślij do TS”.
  await panel(page).getByLabel("Login", { exact: true }).fill("jan");
  await panel(page).getByLabel("Hasło", { exact: true }).fill("haslo");
  await panel(page).getByRole("button", { name: "Połącz", exact: true }).click();
  await expect(panel(page).getByText("Połączono jako jan", { exact: true })).toBeVisible();
  await nav(page, "Raporty dzienne");
  await expect(
    page
      .getByRole("region", { name: "Raport TS" })
      .getByRole("button", { name: "Wyślij do TS" }),
  ).toBeVisible();

  await openSettings(page, "Kalendarz");
  await nav(page, "Dashboard");
  await page.evaluate(() =>
    window.dispatchEvent(
      new CustomEvent("tuktuk-track-navigate", {
        detail: { view: "settings", section: "timesheet" },
      }),
    ),
  );
  await expectTimesheet(page);
});

// ---------------------------------------------------------------------------
// T5: pełna ścieżka i nasłuch zmian symulowanego TS.

test("pełna ścieżka: Ustawienia → mapowanie projektu → Zamknięcie dnia → podgląd → zapis → znacznik", async ({
  page,
}) => {
  // Konto rozłączone (login zapamiętany), bez mapowań; w TS jeden wpis na dziś.
  await openPreview(page, {
    data: tsSeed(),
    ts: { ...tsFixture({ connected: false }), projectMap: [] },
  });
  // Bez konta: zamiast przycisku link do Ustawień.
  await openDayClose(page);
  await expect(page.getByRole("button", { name: "Wyślij do TS" })).toHaveCount(0);
  await page.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expectTimesheet(page);

  // 1. Połączenie konta.
  await expect(panel(page).getByLabel("Login", { exact: true })).toHaveValue("jan");
  await panel(page).getByLabel("Hasło", { exact: true }).fill("haslo-testowe");
  await panel(page).getByRole("button", { name: "Połącz", exact: true }).click();
  await expect(panel(page).getByText("Połączono jako jan", { exact: true })).toBeVisible();

  // 2. Domyślne zadanie TS projektu „Zespół” → „Spotkania”.
  await pick(panel(page).getByLabel("Projekt", { exact: true }), "Zespół");
  await pick(panel(page).getByLabel("Domyślne zadanie TS"), "Spotkania");
  await panel(page).getByRole("button", { name: "Dodaj mapowanie" }).click();
  await expect(
    panel(page)
      .getByRole("table", { name: "Domyślne zadania TS dla projektów" })
      .locator("tbody tr"),
  ).toHaveText([/Zespół\s*Wewnętrzne → Spotkania\s*Usuń/]);

  // Kafelek zadania projektu „Zespół” od razu pokazuje zadanie z projektu.
  await nav(page, "Zadania");
  await expect(card(page, "Spotkanie zespołu").locator(".task-ts")).toHaveText(
    "TS: Spotkania (z projektu)",
  );
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Rozwój",
  );
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);

  // 3. Zamknięcie dnia → podgląd.
  await openDayClose(page);
  const section = page.locator(".dashboard-card", {
    has: page.getByRole("heading", { name: /^Zamknięcie dnia/ }),
  });
  await expect(section.locator(".ts-sent-marker")).toHaveCount(0);
  await section.getByRole("button", { name: "Wyślij do TS" }).click();
  const d = dialog(page);
  await expect(rows(page)).toHaveCount(2);
  // Dwa zadania TukTuk Track scalone w jeden wpis TS.
  await expect(rows(page).nth(0).locator("td")).toHaveText([
    "Analiza importu, Wdrożenie etykiet",
    "Rozwój",
    "2,50",
    "",
  ]);
  await expect(
    d.getByRole("textbox", { name: "Opis wpisu TS: Rozwój" }),
  ).toHaveValue("Import CSV; Wdrożenie etykiet");
  await expect(rows(page).nth(1).locator("td").nth(0)).toHaveText("Spotkanie zespołu");
  await expect(rows(page).nth(1).locator("td").nth(1)).toHaveText("Spotkania");
  await expect(rows(page).nth(1).locator("td").nth(2)).toHaveText("0,50");
  await expect(d.locator("tfoot")).toContainText("Razem3,00");
  await expect(d.getByRole("note", { name: "Ostrzeżenia wysyłki" })).toContainText(
    "Bez zadania TS: 1 zadanie (1,00 h) — ten czas nie zostanie wysłany.",
  );
  // Istniejący wpis TS (spoza raportu) zostanie przy „Zachowaj”.
  const inTs = d.getByRole("region", { name: "Wpisy w TS" });
  await expect(inTs).toContainText("W TS na ten dzień jest już 1 wpis (razem 1,00 h)");
  await expect(inTs.locator("li")).toHaveCount(1);
  await expect(inTs.locator("li").first()).toContainText(
    "Administracja1,00 hRozliczenie delegacji",
  );
  await expect(inTs.locator("li").first()).not.toContainText("zastąpiony");
  await expect(d.getByRole("radio", { name: "Zachowaj pozostałe wpisy TS" })).toBeChecked();
  // Nic nie zostało zapisane przed kliknięciem „Zapisz w TS”.
  expect((await tsState(page)).filled[TODAY]).toEqual([TS_EXISTING]);
  expect((await tsState(page)).sent).toBeUndefined();

  // 4. Zapis.
  await d.getByRole("button", { name: "Zapisz w TS" }).click();
  await expect(d.getByRole("status")).toHaveText(
    "W TS zapisano 3 wpisy, razem 4,00 h.",
  );
  await expect(d.getByRole("alert")).toHaveCount(0);
  expect((await tsState(page)).filled[TODAY]).toEqual([
    {
      id: "1101",
      hours: "2,50",
      overtime: "0",
      description: "Import CSV; Wdrożenie etykiet",
    },
    { id: "2201", hours: "0,50", overtime: "0", description: "Spotkanie zespołu" },
    TS_EXISTING,
  ]);
  await d.getByRole("button", { name: "Zamknij" }).click();
  await expect(d).toHaveCount(0);

  // 5. Znacznik w Zamknięciu dnia i w Raporcie dnia; dzień nie zamknięty.
  await expect(section.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (4,00 h)",
  );
  expect((await stored(page)).closedDays).not.toContain(TODAY);
  await openReport(page);
  await expect(
    page.getByRole("region", { name: "Raport TS" }).locator(".ts-sent-marker"),
  ).toHaveText("Wysłano do TS 15.10 12:00 (4,00 h)");
  // Znacznik przeżywa przeładowanie (symulowane `ts_sent_days`).
  await page.reload();
  await openDayClose(page);
  await expect(page.locator(".ts-sent-marker")).toHaveText(
    "Wysłano do TS 15.10 12:00 (4,00 h)",
  );
});

test("zmiana symulowanego TS w innym oknie odświeża przyciski, Ustawienia i chipy bez przeładowania", async ({
  page,
  context,
}) => {
  await openPreview(page, {
    data: tsSeed(),
    ts: tsFixture({ connected: false }),
  });
  await openReport(page);
  const region = page.getByRole("region", { name: "Raport TS" });
  await expect(
    region.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }),
  ).toBeVisible();

  // Drugie okno tego samego źródła (plik źródłowy, bez uruchamiania aplikacji)
  // zmienia stan symulowanego TS: w pierwszym oknie przychodzi zdarzenie `storage`.
  const other = await context.newPage();
  await other.goto("/src/timesheet.ts");
  const write = (state: unknown) =>
    other.evaluate(
      ([key, value]) => localStorage.setItem(key, value),
      [TS_KEY, JSON.stringify(state)] as const,
    );
  await write(tsFixture());
  await expect(region.getByRole("button", { name: "Wyślij do TS" })).toBeVisible();

  await openSettings(page, SECTION);
  await expect(panel(page).getByText("Połączono jako jan", { exact: true })).toBeVisible();
  const table = panel(page).getByRole("table", {
    name: "Domyślne zadania TS dla projektów",
  });
  await expect(table.locator("tbody tr")).toHaveText([/Zespół\s*Wewnętrzne → Spotkania/]);
  // Nowe mapowanie i rozłączenie w drugim oknie.
  await write({
    ...tsFixture({ connected: false }),
    projectMap: [
      ...tsFixture().projectMap,
      { project: "Foundation", tsTaskId: "1103", tsTaskName: "Code review" },
    ],
  });
  await expect(panel(page).getByText("Nie połączono", { exact: true })).toBeVisible();
  await expect(table.locator("tbody tr")).toHaveCount(2);
  await nav(page, "Zadania");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText(
    "TS: Code review (z projektu)",
  );
  // Usunięcie mapowania w drugim oknie zdejmuje chip.
  await write(tsFixture({ connected: false }));
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);
  await other.close();
});

test("ramka „Wersja niefinalna” w Ustawienia → TS przed połączeniem i po nim", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, SECTION);
  const note = panel(page).getByRole("note", { name: "Wersja niefinalna" });
  await expect(note).toBeVisible();
  await expect(note).toContainText(
    "Integracja z Ideo Timesheet nie jest wersją finalną. Korzystasz z niej na własną odpowiedzialność",
  );
  await expect(panel(page).getByText("Nie połączono", { exact: true })).toBeVisible();

  await setTs(page, { connected: true, login: "jan" });
  await openSettings(page, SECTION);
  await expect(panel(page).getByText("Połączono jako jan")).toBeVisible();
  await expect(note).toBeVisible();
});
