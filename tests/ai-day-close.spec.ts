import { test, expect, type Page } from "./fixtures/test";
import {
  AI_KEY,
  agentSessions,
  at,
  ids,
  manualMeetings,
  openPreview,
  PREVIEW_KEY,
  SETTINGS_KEY,
  seed,
  stored,
  TODAY,
  tsFixture,
  tsSeed,
} from "./fixtures/seed";

/**
 * Zamknięcie dnia z asystentem (PLAN_BACKLOG.md, A5) na symulowanym silniku AI
 * (`src/preview/ai.ts`, `src/preview/aiday.ts`): sekcja „Asystent”, każda karta
 * (luka, czas agenta, notatka, opis TS), brak zmian bez kliknięcia i proponowany
 * opis w podglądzie „Wyślij do TS”. Żaden test nie uruchamia prawdziwego claude
 * ani codex i nie łączy się z TS. Dane: dziś czwartek 15.10.2026 12:00; pomiary
 * 8:00–8:30 (spotkanie zespołu), 8:30–10:30 (analiza importu), dodane 11:00–11:30
 * (code review) i 11:40–11:55 (etykiety), więc luki 10:30–11:00 i 11:30–11:40.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const BINDINGS_KEY = "todo-track-preview-agent-bindings-v1";
const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };
const settings = { aiEngine: "claude" };

function dayData() {
  const data = seed();
  data.sessions.push(
    { id: 60, taskId: ids.review, startedAt: at(TODAY, "11:00"), endedAt: at(TODAY, "11:30") },
    { id: 61, taskId: ids.labels, startedAt: at(TODAY, "11:40"), endedAt: at(TODAY, "11:55") },
  );
  data.sessions.sort((a, b) => b.startedAt - a.startedAt);
  return data;
}

type Item = {
  kind: string;
  from?: string;
  to?: string;
  taskId: number | null;
  text?: string;
  reason?: string;
};
const item = (i: Item) => ({ from: "", to: "", text: "", reason: "Powód propozycji.", ...i });
const ai = (proposals: Item[], extra: Record<string, unknown> = {}) => ({
  claude,
  script: { day_close: { value: { proposals: proposals.map(item) } } },
  dayData: {
    commits: [
      {
        repo: "api",
        at: at(TODAY, "10:40"),
        subject: "fix eksportu CSV dla Kowalskiego",
        taskId: ids.analysis,
      },
    ],
    statusChanges: [
      {
        system: "Jira",
        key: "WMS-12",
        from: "W toku",
        to: "Do review",
        at: at(TODAY, "10:45"),
        taskId: ids.analysis,
      },
    ],
  },
  ...extra,
});

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const assistant = (page: Page) => page.getByRole("region", { name: "Asystent" });
const cards = (page: Page) =>
  assistant(page).getByRole("list", { name: "Propozycje asystenta" }).locator("li");
const calls = async (page: Page, kind: string) =>
  ((await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").calls ?? [],
    AI_KEY,
  )) as { kind: string }[]).filter((c) => c.kind === kind).length;
const analyzeButton = (page: Page) =>
  assistant(page).getByRole("button", { name: "Przeanalizuj dzień" });

async function openClose(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await expect(page.getByRole("heading", { name: /^Zamknięcie dnia/ })).toBeVisible();
}
/** Wszystko, co test mógłby zmienić: dane, ustawienia i przypisania folderów. */
const snapshot = (page: Page) =>
  page.evaluate(
    ([keys]) => keys.map((k) => localStorage.getItem(k)),
    [[PREVIEW_KEY, SETTINGS_KEY, BINDINGS_KEY]] as const,
  );

/** Cztery rodzaje propozycji jednocześnie (luka cała i fragment, agent, notatki, opis TS). */
const everything: Item[] = [
  { kind: "gap", from: "10:30", to: "11:00", taskId: ids.analysis, reason: "Commit „fix eksportu CSV” o 10:40." },
  { kind: "gap", from: "11:32", to: "11:38", taskId: ids.standup },
  { kind: "unassigned", from: "11:00", to: "11:40", taskId: ids.review, reason: "Folder narzedzia to przegląd." },
  { kind: "note", taskId: ids.analysis, text: "Naprawiono eksport CSV." },
  { kind: "note", taskId: ids.review, text: "Przegląd PR." },
  { kind: "ts_description", taskId: ids.analysis, text: "Eksport CSV: poprawka i test", reason: "Brak notatki dnia." },
];

// ---------------------------------------------------------------------------
// Sekcja

test("sekcja „Asystent” jest tylko przy wybranym silniku i włączonym przełączniku", async ({
  page,
}) => {
  await openPreview(page, { data: dayData() });
  await openClose(page);
  await expect(page.getByRole("heading", { name: "Asystent", level: 3 })).toHaveCount(0);

  await openPreview(page, { data: dayData(), settings, ai: ai([]) });
  await openClose(page);
  await expect(assistant(page).getByRole("heading", { name: "Asystent" })).toBeVisible();
  await expect(analyzeButton(page)).toBeVisible();

  await openPreview(page, {
    data: dayData(),
    settings: { ...settings, aiDayClose: false },
    ai: ai([]),
  });
  await openClose(page);
  await expect(assistant(page)).toHaveCount(0);

  await openPreview(page, { data: dayData(), settings: { aiEngine: "off" }, ai: ai([]) });
  await openClose(page);
  await expect(assistant(page)).toHaveCount(0);
});

test("otwarcie dnia nie woła modelu; analiza rusza dopiero po kliknięciu i niczego nie zapisuje", async ({
  page,
}) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai(everything),
  });
  await openClose(page);
  await expect(analyzeButton(page)).toBeVisible();
  await expect(cards(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await calls(page, "day_close")).toBe(0);
  const before = await snapshot(page);

  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(6);
  expect(await calls(page, "day_close")).toBe(1);
  await expect(assistant(page).getByRole("status").first()).toContainText(
    "Propozycje asystenta: 6",
  );
  // Karty tylko pokazują propozycje: dane, ustawienia i przypisania bez zmian.
  expect(await snapshot(page)).toEqual(before);
  // Przeładowanie wraca do stanu bez analizy i bez wywołania modelu.
  await page.reload();
  await openClose(page);
  await expect(cards(page)).toHaveCount(0);
  expect(await calls(page, "day_close")).toBe(1);
});

test("„Pokaż, co zostanie wysłane”: oś dnia, commity i statusy po filtrze nazwisk, bez tytułów spotkań i bez wywołania", async ({
  page,
}) => {
  await openPreview(page, {
    data: dayData(),
    settings: { ...settings, aiHiddenNames: ["Kowalski"] },
    agentSessions: agentSessions(),
    meetings: manualMeetings(),
    ai: ai([]),
  });
  await openClose(page);
  const preview = assistant(page).locator("details.ai-preview");
  await preview.locator("summary").click();
  const text = preview.getByLabel("Tekst wysyłany do modelu");
  await expect(text).toContainText('"gaps":[{"from":"10:30","to":"11:00"');
  await expect(text).toContainText('"subject":"fix eksportu CSV dla Osoba 1"');
  await expect(text).toContainText('"statusChanges":[{"time":"10:45","system":"Jira","key":"WMS-12"');
  await expect(text).toContainText('"agentWork"');
  await expect(text).toContainText('"folder":"narzedzia"');
  await expect(text).not.toContainText("Kowalski");
  // Spotkania są, ale bez tytułów (przełącznik wyłączony).
  await expect(text).toContainText('"meetings":[{"from":"11:00","to":"11:30"}');
  await expect(text).not.toContainText("Telefon z magazynem");
  // Podgląd niczego nie wysyła.
  expect(await calls(page, "day_close")).toBe(0);
});

test("tytuły spotkań trafiają do podglądu tylko przy włączonym przełączniku", async ({ page }) => {
  await openPreview(page, {
    data: dayData(),
    settings: { ...settings, aiSendMeetingTitles: true },
    meetings: manualMeetings(),
    ai: ai([]),
  });
  await openClose(page);
  const preview = assistant(page).locator("details.ai-preview");
  await preview.locator("summary").click();
  await expect(preview.getByLabel("Tekst wysyłany do modelu")).toContainText(
    '"title":"Telefon z magazynem"',
  );
});

// ---------------------------------------------------------------------------
// Karty

test("luka: „Dodaj jako czas” otwiera ręczny czas z godzinami i zadaniem, zapis dopiero po kliknięciu", async ({
  page,
}) => {
  await openPreview(page, { data: dayData(), settings, ai: ai(everything) });
  await openClose(page);
  const before = (await stored(page)).sessions.length;
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Luka 10:30–11:00" });
  await expect(card).toContainText("Analiza importu");
  await expect(card).toContainText("Commit „fix eksportu CSV” o 10:40.");
  await card.getByRole("button", { name: "Dodaj jako czas: luka 10:30–11:00" }).click();
  await expect(page.getByLabel("Początek ręcznego czasu")).toHaveValue(`${TODAY}T10:30`);
  await expect(page.getByLabel("Koniec ręcznego czasu")).toHaveValue(`${TODAY}T11:00`);
  await expect(page.getByRole("combobox", { name: "Zadanie do uzupełnienia" })).toHaveValue(
    "Analiza importu",
  );
  // Otwarcie formularza niczego nie zapisało.
  expect((await stored(page)).sessions.length).toBe(before);
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  const data = await stored(page);
  expect(data.sessions.length).toBe(before + 1);
  expect(
    data.sessions.some(
      (s: { taskId: number; startedAt: number; endedAt: number }) =>
        s.taskId === ids.analysis &&
        s.startedAt === at(TODAY, "10:30") &&
        s.endedAt === at(TODAY, "11:00"),
    ),
  ).toBe(true);
  await expect(card).toContainText("Zastosowano.");
  await expect(card.getByRole("button")).toHaveCount(0);
});

test("luka z zadaniem sąsiedniego pomiaru: „Dołącz do poprzedniego pomiaru” przesuwa koniec pomiaru (fill_gap)", async ({
  page,
}) => {
  await openPreview(page, { data: dayData(), settings, ai: ai(everything) });
  await openClose(page);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Luka 10:30–11:00" });
  const before = await stored(page);
  const sessions = (d: typeof before) => d.sessions.length;
  await card
    .getByRole("button", { name: "Dołącz do poprzedniego pomiaru: luka 10:30–11:00" })
    .click();
  await expect(card).toContainText("Zastosowano.");
  const data = await stored(page);
  expect(sessions(data)).toBe(sessions(before));
  const analysis = data.sessions.find((s: { id: number }) => s.id === 8);
  expect(analysis.endedAt).toBe(at(TODAY, "11:00"));
});

test("luka po pomiarze zatrzymanym w ułamku sekundy: „Dodaj jako czas” zapisuje się bez nakładania", async ({
  page,
}) => {
  const data = dayData();
  // Pomiar „Analiza importu” zatrzymany o 10:30:30.456 (db::now() ma milisekundy).
  const stopped = data.sessions.find(
    (s: { taskId: number; endedAt?: number | null }) =>
      s.taskId === ids.analysis && s.endedAt === at(TODAY, "10:30"),
  )!;
  stopped.endedAt = at(TODAY, "10:30") + 30_456;
  await openPreview(page, {
    data,
    settings,
    ai: ai([{ kind: "gap", from: "10:30", to: "11:00", taskId: ids.review }]),
  });
  await openClose(page);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Luka 10:30–11:00" });
  await card.getByRole("button", { name: "Dodaj jako czas: luka 10:30–11:00" }).click();
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect(card).toContainText("Zastosowano.");
  const saved = (await stored(page)).sessions.find(
    (s: { taskId: number; endedAt: number }) =>
      s.taskId === ids.review && s.endedAt === at(TODAY, "11:00"),
  );
  expect(saved.startedAt).toBe(at(TODAY, "10:30") + 31_000);
});

test("ponowna analiza zamyka otwarty formularz czasu i nie oznacza kart nowej odpowiedzi", async ({
  page,
}) => {
  await openPreview(page, { data: dayData(), settings, ai: ai(everything) });
  await openClose(page);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Luka 11:32–11:38" });
  await card.getByRole("button", { name: "Dodaj jako czas: luka 11:32–11:38" }).click();
  await expect(page.getByLabel("Początek ręcznego czasu")).toBeVisible();
  await analyzeButton(page).click();
  await expect.poll(() => calls(page, "day_close")).toBe(2);
  await expect(page.getByLabel("Początek ręcznego czasu")).toHaveCount(0);
  await expect(cards(page)).toHaveCount(5);
  await expect(card.getByRole("button", { name: /^Dodaj jako czas/ })).toBeVisible();
  await expect(assistant(page).getByText("Zastosowano.")).toHaveCount(0);
});

test("fragment luki i luka z cudzym zadaniem mają tylko „Dodaj jako czas”", async ({ page }) => {
  await openPreview(page, { data: dayData(), settings, ai: ai(everything) });
  await openClose(page);
  await analyzeButton(page).click();
  const fragment = cards(page).filter({ hasText: "Luka 11:32–11:38" });
  await expect(fragment).toContainText("Spotkanie zespołu");
  await expect(fragment.getByRole("button")).toHaveText(["Dodaj jako czas", "Odrzuć"]);
});

test("czas agenta bez zadania: „Przypisz” zapisuje przypisanie folderu, nic wcześniej", async ({
  page,
}) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai(everything),
  });
  await openClose(page);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Praca agenta w „narzedzia”" });
  await expect(card).toContainText("11:00–11:40");
  await expect(card).toContainText("Code review");
  await expect(card).toContainText("Folder narzedzia to przegląd.");
  expect(await page.evaluate((k) => localStorage.getItem(k), BINDINGS_KEY)).toBeNull();
  await card.getByRole("button", { name: "Przypisz: narzedzia" }).click();
  await expect(card).toContainText("Zastosowano.");
  await expect
    .poll(() => page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), BINDINGS_KEY))
    .toEqual([{ folder: "C:\\Projekty\\narzedzia", taskId: ids.review }]);
  await expect(card).not.toContainText("ostatnich 7 dni");
});

test("„Przypisz” dla dnia starszego niż 7 dni: folder przypisany, karta mówi, że czas z tego dnia został bez zadania", async ({
  page,
}) => {
  const OLD = "2026-10-07";
  const periods = [
    ...agentSessions(),
    {
      id: 9,
      agent: "codex",
      folder: "C:\\Projekty\\stare",
      taskId: null,
      start: at(OLD, "09:00"),
      end: at(OLD, "10:00"),
    },
  ];
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: periods,
    ai: ai([{ kind: "unassigned", from: "09:00", to: "10:00", taskId: ids.review }]),
  });
  await openClose(page);
  await page.getByLabel("Dzień planu").fill(OLD);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Praca agenta w „stare”" });
  await card.getByRole("button", { name: "Przypisz: stare" }).click();
  await expect(card).toContainText("Zastosowano.");
  await expect(card).toContainText("przypisanie uzupełnia tylko okresy z ostatnich 7 dni");
  await expect
    .poll(() => page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), BINDINGS_KEY))
    .toEqual([{ folder: "C:\\Projekty\\stare", taskId: ids.review }]);
});

test("notatka: edytowalna przed zapisem, „Dodaj do notatki” dopisuje do notatki dnia", async ({
  page,
}) => {
  await openPreview(page, { data: dayData(), settings, ai: ai(everything) });
  await openClose(page);
  const field = page.getByLabel("Notatka: Analiza importu", { exact: true });
  // Niezapisana zmiana w polu notatki nie ginie przy dopisaniu.
  await field.fill("Ręczna notatka.");
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Notatka dnia" }).first();
  const draft = card.getByLabel("Notatka do dopisania: Analiza importu");
  await expect(draft).toHaveValue("Naprawiono eksport CSV.");
  await draft.fill("Naprawiono eksport CSV i dodano test.");
  const task = async () =>
    (await stored(page)).tasks.find((t: { id: number }) => t.id === ids.analysis);
  expect((await task()).details.notes[TODAY]).toBeUndefined();
  await card.getByRole("button", { name: "Dodaj do notatki: Analiza importu" }).click();
  await expect(card).toContainText("Zastosowano.");
  expect((await task()).details.notes[TODAY]).toBe(
    "Ręczna notatka.\nNaprawiono eksport CSV i dodano test.",
  );
  await expect(field).toHaveValue("Ręczna notatka.\nNaprawiono eksport CSV i dodano test.");
  // Wcześniejsze notatki innych dni zostały.
  expect((await task()).details.notes["2026-10-14"]).toBe("Mapowanie kolumn");
});

test("„Zastosuj wszystkie” dotyczy tylko notatek: czas, folder i luka zostają", async ({ page }) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai(everything),
  });
  await openClose(page);
  await analyzeButton(page).click();
  const sessions = (await stored(page)).sessions.length;
  await assistant(page)
    .getByRole("button", { name: "Zastosuj wszystkie notatki (2)" })
    .click();
  const data = await stored(page);
  const note = (id: number) => data.tasks.find((t: { id: number }) => t.id === id).details.notes[TODAY];
  expect(note(ids.analysis)).toBe("Naprawiono eksport CSV.");
  expect(note(ids.review)).toBe("Przegląd PR.");
  expect(data.sessions.length).toBe(sessions);
  expect(await page.evaluate((k) => localStorage.getItem(k), BINDINGS_KEY)).toBeNull();
  await expect(cards(page).filter({ hasText: "Notatka dnia" }).first()).toContainText(
    "Zastosowano.",
  );
  // Karty czasu i folderu czekają dalej na własne kliknięcie.
  await expect(
    cards(page).filter({ hasText: "Luka 10:30–11:00" }).getByRole("button", { name: /^Dodaj jako czas/ }),
  ).toBeVisible();
  await expect(assistant(page).getByRole("button", { name: /^Zastosuj wszystkie/ })).toHaveCount(0);
});

test("opis TS: karta tylko informuje, a opis czeka w podglądzie „Wyślij do TS”", async ({
  page,
}) => {
  await openPreview(page, {
    data: tsSeed(),
    settings,
    ts: tsFixture(),
    ai: ai([
      { kind: "ts_description", taskId: ids.labels, text: "Eksport CSV: poprawka i test", reason: "Brak notatki dnia." },
    ]),
  });
  await openClose(page);
  await analyzeButton(page).click();
  const card = cards(page).filter({ hasText: "Opis wpisu TS" });
  await expect(card).toContainText("„Eksport CSV: poprawka i test”");
  await expect(card).toContainText("Pojawi się w podglądzie „Wyślij do TS”");
  await expect(card.getByRole("button")).toHaveText(["Odrzuć"]);
  const before = await snapshot(page);

  await page.getByRole("button", { name: "Wyślij do TS" }).click();
  const dialog = page.getByRole("dialog", { name: "Wyślij do TS" });
  const description = dialog.getByLabel("Opis wpisu TS: Rozwój");
  // Pole ma opis z notatek dnia (dwa zadania w jednym wpisie); propozycja jest obok i niczego nie zmienia.
  await expect(description).toHaveValue("Import CSV; Wdrożenie etykiet");
  const hint = dialog.getByRole("status", { name: "Proponowany opis: Rozwój" });
  await expect(hint).toContainText("Asystent proponuje opis: „Eksport CSV: poprawka i test”");
  await expect(hint).toContainText("Brak notatki dnia.");
  expect(await snapshot(page)).toEqual(before);
  await hint.getByRole("button", { name: "Użyj opisu: Rozwój" }).click();
  await expect(description).toHaveValue("Eksport CSV: poprawka i test");
  await expect(hint).toHaveCount(0);
  // Użycie opisu zmienia tylko pole wysyłki, nie notatkę zadania.
  expect(await snapshot(page)).toEqual(before);
});

test("„Pomiń” przy opisie TS chowa propozycję w podglądzie, a odrzucona karta nie trafia do wysyłki", async ({
  page,
}) => {
  await openPreview(page, {
    data: tsSeed(),
    settings,
    ts: tsFixture(),
    ai: ai([
      { kind: "ts_description", taskId: ids.labels, text: "Opis A" },
      { kind: "ts_description", taskId: ids.review, text: "Opis B" },
    ]),
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(2);
  // Odrzucona karta „Opis B” znika i nie podpowiada opisu przy wysyłce.
  await cards(page)
    .filter({ hasText: "Opis B" })
    .getByRole("button", { name: /^Odrzuć propozycję/ })
    .click();
  await expect(cards(page)).toHaveCount(1);
  await page.getByRole("button", { name: "Wyślij do TS" }).click();
  const dialog = page.getByRole("dialog", { name: "Wyślij do TS" });
  const hint = dialog.getByRole("status", { name: "Proponowany opis: Rozwój" });
  await expect(hint).toContainText("Opis A");
  await hint.getByRole("button", { name: "Pomiń opis: Rozwój" }).click();
  await expect(hint).toHaveCount(0);
  await expect(dialog.getByLabel("Opis wpisu TS: Rozwój")).toHaveValue("Import CSV; Wdrożenie etykiet");
});

test("opis TS dla zadania z notatką dnia jest pomijany (wiersz ma już opis z notatki)", async ({
  page,
}) => {
  await openPreview(page, {
    data: tsSeed(),
    settings,
    ts: tsFixture(),
    ai: ai([
      { kind: "ts_description", taskId: ids.analysis, text: "Opis zadania z notatką" },
      { kind: "ts_description", taskId: ids.labels, text: "Opis zadania bez notatki" },
    ]),
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(1);
  await expect(cards(page).first()).toContainText("Wdrożenie etykiet");
  await expect(cards(page).first()).toContainText("Opis zadania bez notatki");
});

test("„Odrzuć” usuwa kartę bez żadnej zmiany", async ({ page }) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai(everything),
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(6);
  const before = await snapshot(page);
  await cards(page)
    .filter({ hasText: "Luka 10:30–11:00" })
    .getByRole("button", { name: "Odrzuć propozycję: 10:30–11:00" })
    .click();
  await expect(cards(page)).toHaveCount(5);
  await expect(assistant(page).getByRole("status").first()).toContainText(
    "Propozycje asystenta: 5",
  );
  expect(await snapshot(page)).toEqual(before);
});

// ---------------------------------------------------------------------------
// Wyniki bez propozycji i błędy

test("propozycje spoza danych (zadanie spoza listy, luka spoza osi, pusty tekst) są pomijane", async ({
  page,
}) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai([
      { kind: "gap", from: "13:00", to: "13:30", taskId: ids.analysis },
      { kind: "gap", from: "10:30", to: "11:00", taskId: 999 },
      { kind: "gap", from: "10:30", to: "11:00", taskId: ids.archived },
      { kind: "note", taskId: ids.analysis, text: "  " },
      { kind: "unassigned", from: "10:00", to: "11:30", taskId: ids.review },
      { kind: "note", taskId: ids.analysis, text: "Jedyna poprawna." },
    ]),
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(1);
  await expect(
    cards(page).first().getByLabel("Notatka do dopisania: Analiza importu"),
  ).toHaveValue("Jedyna poprawna.");
});

test("brak propozycji: komunikat, bez kart", async ({ page }) => {
  await openPreview(page, { data: dayData(), settings, ai: ai([]) });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(assistant(page).getByRole("status")).toHaveText(
    "Asystent nie znalazł nic do zaproponowania.",
  );
  await expect(cards(page)).toHaveCount(0);
  expect(await calls(page, "day_close")).toBe(1);
});

test("dzień bez danych nie woła modelu", async ({ page }) => {
  await openPreview(page, { data: { tasks: [], sessions: [] }, settings, ai: { claude } });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(assistant(page).getByRole("status")).toHaveText(
    "Brak danych z tego dnia do analizy.",
  );
  expect(await calls(page, "day_close")).toBe(0);
});

test("błąd asystenta (niezalogowany) pokazuje komunikat po polsku i nie zostawia kart", async ({
  page,
}) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    ai: { claude, script: { day_close: { error: "login" } } },
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(assistant(page).getByRole("alert")).toContainText(
    "Claude Code nie jest zalogowany",
  );
  await expect(cards(page)).toHaveCount(0);
  // Przycisk wraca i można spróbować ponownie.
  await expect(analyzeButton(page)).toBeEnabled();
});

test("zmiana dnia czyści propozycje poprzedniego dnia", async ({ page }) => {
  await openPreview(page, {
    data: dayData(),
    settings,
    agentSessions: agentSessions(),
    ai: ai(everything),
  });
  await openClose(page);
  await analyzeButton(page).click();
  await expect(cards(page)).toHaveCount(6);
  await page.getByRole("button", { name: "Poprzedni okres" }).click();
  await expect(cards(page)).toHaveCount(0);
  await expect(assistant(page)).toBeVisible();
  // Nowa analiza dopiero po kliknięciu.
  expect(await calls(page, "day_close")).toBe(1);
});
