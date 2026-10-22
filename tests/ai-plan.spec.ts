import { test, expect, type Page } from "./fixtures/test";
import {
  AI_KEY,
  at,
  ids,
  manualMeetings,
  openPreview,
  PREVIEW_KEY,
  SETTINGS_KEY,
  seed,
  stored,
  TODAY,
} from "./fixtures/seed";

/**
 * Rano i przegląd tygodnia z asystentem (PLAN_BACKLOG.md, A6) na symulowanym
 * silniku AI (`src/preview/ai.ts`, `src/preview/aiplan.ts`): „Komentarz asystenta”
 * w Rano z kartami przesunięć („Zastosuj” / „Odrzuć”) i „Szkic wniosków” w
 * przeglądzie tygodnia, edytowalny przed zapisem. Żaden test nie uruchamia
 * prawdziwego claude ani codex. Dane: dziś czwartek 15.10.2026 12:00; w planie
 * dnia „Analiza importu” (10 h, termin piątek 16.10), „Wdrożenie etykiet” (8 h)
 * i „Code review” (1 h) przy celu 8 h, więc plan jest za duży. Najbliższe dni
 * robocze to pt. 16.10, pon. 19.10, wt. 20.10, śr. 21.10 i czw. 22.10.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const WEEK_NOTES_KEY = "todo-track-preview-week-notes-v1";
const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };
const settings = { aiEngine: "claude" };

/** Plan dziś: trzy zadania i jedno ukończone. */
function planData() {
  const data = seed();
  const task = (id: number) => data.tasks.find((t) => t.id === id)!;
  task(ids.labels).details.plannedDay = TODAY;
  task(ids.review).details.plannedDay = TODAY;
  task(ids.review).details.estimateMinutes = 60;
  task(ids.docs).details.plannedDay = TODAY;
  task(ids.docs).done = true;
  return data;
}

const ai = (script: Record<string, unknown> = {}, extra: Record<string, unknown> = {}) => ({
  claude,
  script,
  ...extra,
});
const morningScript = (value: unknown) => ({ morning: { value } });
const weekScript = (value: unknown) => ({ week_notes: { value } });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const calls = async (page: Page, kind: string) =>
  ((await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").calls ?? [],
    AI_KEY,
  )) as { kind: string }[]).filter((c) => c.kind === kind).length;
/** Wszystko, co test mógłby zmienić: dane, ustawienia i wnioski tygodnia. */
const snapshot = (page: Page) =>
  page.evaluate(
    ([keys]) => keys.map((k) => localStorage.getItem(k)),
    [[PREVIEW_KEY, SETTINGS_KEY, WEEK_NOTES_KEY]] as const,
  );
const plannedDay = async (page: Page, id: number) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === id).details as {
    plannedDay: string;
    order: number;
  };

// ---------------------------------------------------------------------------
// Rano

const assistant = (page: Page) => page.getByRole("region", { name: "Asystent planu dnia" });
const commentButton = (page: Page) =>
  assistant(page).getByRole("button", { name: "Komentarz asystenta" });
const cards = (page: Page) =>
  assistant(page).getByRole("list", { name: "Propozycje przesunięć" }).locator("li");

async function openMorning(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Rano");
  await expect(page.getByRole("region", { name: "Przegląd dnia" })).toBeVisible();
}

/** Odpowiedź z dobrymi i niedozwolonymi przesunięciami (te drugie mają zniknąć). */
const answer = {
  comment: "Plan ma 19 godzin przy 8 godzinach pojemności. Analiza importu ma termin jutro.",
  moves: [
    // Niedozwolone: dzień wolny, po terminie, ukończone, nieznane, spoza planu.
    { taskId: ids.review, toDay: "2026-10-17", reason: "Sobota." },
    { taskId: ids.analysis, toDay: "2026-10-19", reason: "Po terminie." },
    { taskId: ids.docs, toDay: "2026-10-16", reason: "Ukończone." },
    { taskId: 999, toDay: "2026-10-16", reason: "Nieznane." },
    { taskId: ids.standup, toDay: "2026-10-16", reason: "Spoza planu." },
    // Dozwolone.
    { taskId: ids.labels, toDay: "2026-10-19", reason: "Wdrożenie etykiet zajmie cały dzień." },
    { taskId: ids.review, toDay: "2026-10-16", reason: "Code review mieści się jutro." },
  ],
};

test("sekcja „Asystent” w Rano jest tylko przy wybranym silniku i włączonym przełączniku", async ({
  page,
}) => {
  await openPreview(page, { data: planData() });
  await openMorning(page);
  await expect(assistant(page)).toHaveCount(0);

  await openPreview(page, { data: planData(), settings, ai: ai() });
  await openMorning(page);
  await expect(assistant(page).getByRole("heading", { name: "Asystent" })).toBeVisible();
  await expect(commentButton(page)).toBeVisible();

  await openPreview(page, {
    data: planData(),
    settings: { ...settings, aiPlanAssist: false },
    ai: ai(),
  });
  await openMorning(page);
  await expect(assistant(page)).toHaveCount(0);

  await openPreview(page, { data: planData(), settings: { aiEngine: "off" }, ai: ai() });
  await openMorning(page);
  await expect(assistant(page)).toHaveCount(0);
});

test("otwarcie Rano nie woła modelu; komentarz rusza po kliknięciu i niczego nie zmienia w planie", async ({
  page,
}) => {
  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai(morningScript(answer)),
  });
  await openMorning(page);
  await expect(cards(page)).toHaveCount(0);
  await page.waitForTimeout(300);
  expect(await calls(page, "morning")).toBe(0);
  const before = await snapshot(page);

  await commentButton(page).click();
  await expect(assistant(page).getByLabel("Komentarz", { exact: true })).toContainText(
    "Plan ma 19 godzin przy 8 godzinach pojemności.",
  );
  // Tylko dwa dozwolone przesunięcia zostają kartami.
  await expect(cards(page)).toHaveCount(2);
  await expect(cards(page).nth(0)).toContainText("Przesuń „Wdrożenie etykiet”");
  await expect(cards(page).nth(0)).toContainText("dziś → pon., 19 paź");
  await expect(cards(page).nth(0)).toContainText("Wdrożenie etykiet zajmie cały dzień.");
  await expect(cards(page).nth(1)).toContainText("Przesuń „Code review”");
  await expect(cards(page).nth(1)).toContainText("dziś → pt., 16 paź");
  expect(await calls(page, "morning")).toBe(1);
  // Komentarz i karty niczego nie zapisują: dane, ustawienia i wnioski bez zmian.
  expect(await snapshot(page)).toEqual(before);
  // Plan dnia w widoku nadal ma wszystkie zadania.
  await expect(
    page
      .getByRole("list", { name: "Plan dnia dziś" })
      .getByRole("listitem"),
  ).toHaveCount(4);
  // Przeładowanie wraca do stanu bez komentarza i bez nowego wywołania.
  await page.reload();
  await openMorning(page);
  await expect(cards(page)).toHaveCount(0);
  expect(await calls(page, "morning")).toBe(1);
});

test("„Zastosuj” przenosi zadanie na koniec planu proponowanego dnia, reszta planu bez zmian", async ({
  page,
}) => {
  const data = planData();
  // Zadanie już zaplanowane na poniedziałek: przeniesione ląduje po nim.
  data.tasks.push({
    id: 30,
    title: "Raport miesięczny",
    done: false,
    createdAt: at("2026-10-09", "09:00"),
    details: {
      ...data.tasks[0].details,
      priority: 0,
      plannedDay: "2026-10-19",
      dueDay: "",
      estimateMinutes: 30,
      order: 3,
      notes: {},
      checklist: [],
    },
  } as never);
  await openPreview(page, { data, settings, ai: ai(morningScript(answer)) });
  await openMorning(page);
  await commentButton(page).click();
  await expect(cards(page)).toHaveCount(2);
  const sessions = (await stored(page)).sessions;
  const before = await snapshot(page);

  const card = cards(page).filter({ hasText: "Wdrożenie etykiet" });
  await card.getByRole("button", { name: "Zastosuj: Wdrożenie etykiet na pon., 19 paź" }).click();
  await expect(card).toContainText("Zastosowano.");
  await expect(card.getByRole("button")).toHaveCount(0);
  expect(await plannedDay(page, ids.labels)).toMatchObject({ plannedDay: "2026-10-19", order: 4 });
  // Druga karta nic nie zmienia, dopóki nie zostanie kliknięta.
  expect(await plannedDay(page, ids.review)).toMatchObject({ plannedDay: TODAY, order: 0 });
  expect(await plannedDay(page, 30)).toMatchObject({ plannedDay: "2026-10-19", order: 3 });
  // Timer i pomiary nie ruszyły się.
  expect((await stored(page)).sessions).toEqual(sessions);
  expect(await snapshot(page)).not.toEqual(before);
  // Plan dnia w widoku stracił to zadanie.
  const plan = page.getByRole("list", { name: "Plan dnia dziś" });
  await expect(plan).not.toContainText("Wdrożenie etykiet");
  await expect(plan).toContainText("Code review");
});

test("„Odrzuć” usuwa kartę i niczego nie zmienia; odrzucenie wszystkich pokazuje komunikat", async ({
  page,
}) => {
  await openPreview(page, { data: planData(), settings, ai: ai(morningScript(answer)) });
  await openMorning(page);
  await commentButton(page).click();
  await expect(cards(page)).toHaveCount(2);
  const before = await snapshot(page);
  await cards(page)
    .filter({ hasText: "Wdrożenie etykiet" })
    .getByRole("button", { name: "Odrzuć propozycję: Wdrożenie etykiet" })
    .click();
  await expect(cards(page)).toHaveCount(1);
  await cards(page)
    .first()
    .getByRole("button", { name: "Odrzuć propozycję: Code review" })
    .click();
  await expect(cards(page)).toHaveCount(0);
  await expect(assistant(page)).toContainText("Wszystkie propozycje odrzucono.");
  // Komentarz zostaje, plan bez zmian.
  await expect(assistant(page).getByLabel("Komentarz", { exact: true })).toBeVisible();
  expect(await snapshot(page)).toEqual(before);
});

test("sam komentarz bez przesunięć, brak uwag i pusty plan", async ({ page }) => {
  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai(morningScript({ comment: "Plan się mieści.", moves: [] })),
  });
  await openMorning(page);
  await commentButton(page).click();
  await expect(assistant(page).getByLabel("Komentarz", { exact: true })).toHaveText(
    "Plan się mieści.",
  );
  await expect(cards(page)).toHaveCount(0);

  // Pusta odpowiedź modelu.
  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai(morningScript({ comment: "", moves: [] })),
  });
  await openMorning(page);
  await commentButton(page).click();
  await expect(assistant(page)).toContainText("Asystent nie ma uwag do planu.");

  // Dzień bez planu i terminów nie woła modelu.
  const empty = seed();
  empty.tasks.forEach((t) => {
    t.details.plannedDay = "";
    t.details.dueDay = "";
  });
  await openPreview(page, { data: empty, settings, ai: ai(morningScript(answer)) });
  await openMorning(page);
  await commentButton(page).click();
  await expect(assistant(page)).toContainText(
    "Brak zadań w planie dnia ani terminów do oceny.",
  );
  expect(await calls(page, "morning")).toBe(0);
});

test("błąd wywołania (limit, wylogowanie) wraca przy przycisku i nie zmienia planu", async ({
  page,
}) => {
  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai({ morning: { error: "limit" } }),
  });
  await openMorning(page);
  const before = await snapshot(page);
  await commentButton(page).click();
  await expect(assistant(page).getByRole("alert")).toContainText("Limit subskrypcji");
  await expect(cards(page)).toHaveCount(0);
  expect(await snapshot(page)).toEqual(before);
});

test("„Pokaż, co zostanie wysłane”: plan, terminy i pojemność po filtrze nazwisk, bez tytułów spotkań i bez wywołania", async ({
  page,
}) => {
  const data = planData();
  data.tasks.find((t) => t.id === ids.review)!.title = "Rozmowa z Kowalskim";
  await openPreview(page, {
    data,
    settings: { ...settings, aiHiddenNames: ["Kowalski"] },
    meetings: manualMeetings(),
    ai: ai(morningScript(answer)),
  });
  await openMorning(page);
  const preview = assistant(page).locator("details.ai-preview");
  await preview.locator("summary").click();
  const text = preview.getByLabel("Tekst wysyłany do modelu");
  await expect(text).toContainText('"plan":[');
  await expect(text).toContainText('"estimateMinutes":600');
  await expect(text).toContainText('"title":"Rozmowa z Osoba 1"');
  await expect(text).not.toContainText("Kowalski");
  await expect(text).toContainText('"nextDays":[{"day":"2026-10-16"');
  await expect(text).toContainText('"capacityMinutes"');
  await expect(text).not.toContainText("Telefon z magazynem");
  expect(await calls(page, "morning")).toBe(0);
});

test("tytuły spotkań trafiają do podglądu Rano tylko przy włączonym przełączniku", async ({
  page,
}) => {
  await openPreview(page, {
    data: planData(),
    settings: { ...settings, aiSendMeetingTitles: true },
    meetings: manualMeetings(),
    ai: ai(),
  });
  await openMorning(page);
  const preview = assistant(page).locator("details.ai-preview");
  await preview.locator("summary").click();
  await expect(preview.getByLabel("Tekst wysyłany do modelu")).toContainText(
    '"title":"Telefon z magazynem"',
  );
});

test("ponowny komentarz zastępuje karty nowymi i nie oznacza ich jako zastosowanych", async ({
  page,
}) => {
  await openPreview(page, { data: planData(), settings, ai: ai(morningScript(answer)) });
  await openMorning(page);
  await commentButton(page).click();
  await cards(page)
    .filter({ hasText: "Code review" })
    .getByRole("button", { name: /^Zastosuj:/ })
    .click();
  await expect(assistant(page).getByText("Zastosowano.")).toHaveCount(1);
  await commentButton(page).click();
  await expect.poll(() => calls(page, "morning")).toBe(2);
  // Code review już poza dzisiejszym planem: jego przesunięcie odpada, karta etykiet jest nowa.
  await expect(cards(page)).toHaveCount(1);
  await expect(cards(page).first()).toContainText("Wdrożenie etykiet");
  await expect(assistant(page).getByText("Zastosowano.")).toHaveCount(0);
});

// ---------------------------------------------------------------------------
// Przegląd tygodnia

const notesField = (page: Page) => page.getByLabel("Wnioski z tygodnia");
const draftButton = (page: Page) => page.getByRole("button", { name: "Szkic wniosków" });
const saveNotes = (page: Page) => page.getByRole("button", { name: "Zapisz wnioski" });
const conclusions = (page: Page) => page.getByRole("region", { name: "Wnioski" });

async function openWeek(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await nav(page, "Przegląd");
  await expect(notesField(page)).toBeVisible();
}

const draft = {
  notes: "- Za dużo przełączeń w czwartek.\n- Analiza importu przekroczyła szacunek.",
};

test("„Szkic wniosków” jest tylko przy wybranym silniku i włączonym przełączniku", async ({
  page,
}) => {
  await openPreview(page, { data: planData() });
  await openWeek(page);
  await expect(draftButton(page)).toHaveCount(0);

  await openPreview(page, { data: planData(), settings, ai: ai() });
  await openWeek(page);
  await expect(draftButton(page)).toBeVisible();

  await openPreview(page, {
    data: planData(),
    settings: { ...settings, aiPlanAssist: false },
    ai: ai(),
  });
  await openWeek(page);
  await expect(draftButton(page)).toHaveCount(0);
});

test("szkic wypełnia pole „Wnioski” do edycji; zapis dopiero po „Zapisz wnioski”", async ({
  page,
}) => {
  await openPreview(page, { data: planData(), settings, ai: ai(weekScript(draft)) });
  await openWeek(page);
  await expect(notesField(page)).toHaveValue("");
  await page.waitForTimeout(300);
  expect(await calls(page, "week_notes")).toBe(0);
  const before = await snapshot(page);

  await draftButton(page).click();
  await expect(notesField(page)).toHaveValue(draft.notes);
  expect(await calls(page, "week_notes")).toBe(1);
  await expect(conclusions(page).getByRole("status")).toContainText(
    "Szkic asystenta jest w polu poniżej.",
  );
  // Szkic jest tylko w polu: nic nie zapisano.
  expect(await snapshot(page)).toEqual(before);
  await expect(saveNotes(page)).toBeEnabled();
  // Przeładowanie przed zapisem gubi szkic, wnioski nadal puste.
  await page.reload();
  await openWeek(page);
  await expect(notesField(page)).toHaveValue("");

  // Szkic edytowalny: poprawka i dopiero wtedy zapis.
  await draftButton(page).click();
  await expect(notesField(page)).toHaveValue(draft.notes);
  await notesField(page).fill("- Za dużo przełączeń w czwartek (poprawione).");
  expect(await snapshot(page)).toEqual(before);
  await saveNotes(page).click();
  await expect(conclusions(page).getByRole("status").filter({ hasText: "Zapisano" })).toBeVisible();
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), WEEK_NOTES_KEY),
  ).toEqual({ "2026-10-12": "- Za dużo przełączeń w czwartek (poprawione)." });
});

test("szkic dopisuje się pod tekstem, który już jest w polu, i niczego nie nadpisuje", async ({
  page,
}) => {
  await openPreview(page, { data: planData(), settings, ai: ai(weekScript(draft)) });
  await openWeek(page);
  await notesField(page).fill("Moje wnioski.");
  await draftButton(page).click();
  await expect(notesField(page)).toHaveValue(`Moje wnioski.\n\n${draft.notes}`);
  expect(
    await page.evaluate((k) => localStorage.getItem(k), WEEK_NOTES_KEY),
  ).toBeNull();
});

test("szkic nie zmienia zapisanych wniosków do czasu zapisu i nie dotyczy innego tygodnia", async ({
  page,
}) => {
  await openPreview(page, { data: planData(), settings, ai: ai(weekScript(draft)) });
  await openWeek(page);
  await notesField(page).fill("Zapisane wnioski.");
  await saveNotes(page).click();
  await expect(conclusions(page).getByRole("status")).toHaveText("Zapisano");
  await draftButton(page).click();
  await expect(notesField(page)).toHaveValue(`Zapisane wnioski.\n\n${draft.notes}`);
  expect(
    await page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), WEEK_NOTES_KEY),
  ).toEqual({ "2026-10-12": "Zapisane wnioski." });
  // Następny tydzień ma własne, puste wnioski i bez danych nie woła modelu.
  await page.getByRole("button", { name: "Następny okres" }).click();
  await expect(notesField(page)).toHaveValue("");
  await expect(conclusions(page).getByRole("status")).toHaveCount(0);
  await draftButton(page).click();
  await expect(conclusions(page)).toContainText("Brak danych z tego tygodnia do szkicu.");
  await expect(notesField(page)).toHaveValue("");
  expect(await calls(page, "week_notes")).toBe(1);
});

test("pusta odpowiedź i błąd wywołania nie ruszają pola", async ({ page }) => {
  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai(weekScript({ notes: "  " })),
  });
  await openWeek(page);
  await notesField(page).fill("Mój tekst.");
  await draftButton(page).click();
  await expect(conclusions(page)).toContainText("Asystent nie napisał szkicu wniosków.");
  await expect(notesField(page)).toHaveValue("Mój tekst.");

  await openPreview(page, {
    data: planData(),
    settings,
    ai: ai({ week_notes: { error: "login" } }),
  });
  await openWeek(page);
  await notesField(page).fill("Mój tekst.");
  await draftButton(page).click();
  await expect(conclusions(page).getByRole("alert")).toContainText("nie jest zalogowany");
  await expect(notesField(page)).toHaveValue("Mój tekst.");
});

test("„Pokaż, co zostanie wysłane”: tydzień po filtrze nazwisk, z notatkami i bez wywołania", async ({
  page,
}) => {
  const data = planData();
  data.tasks.find((t) => t.id === ids.analysis)!.details.notes["2026-10-13"] =
    "Rozmowa z Kowalskim o walidacji";
  await openPreview(page, {
    data,
    settings: { ...settings, aiHiddenNames: ["Kowalski"] },
    ai: ai(weekScript(draft)),
  });
  await openWeek(page);
  const preview = conclusions(page).locator("details.ai-preview");
  await preview.locator("summary").click();
  const text = preview.getByLabel("Tekst wysyłany do modelu");
  await expect(text).toContainText('"week":"2026-10-12"');
  await expect(text).toContainText('"title":"Analiza importu"');
  await expect(text).toContainText('"note":"Rozmowa z Osoba 1 o walidacji"');
  await expect(text).not.toContainText("Kowalski");
  await expect(text).toContainText('"plan":{');
  expect(await calls(page, "week_notes")).toBe(0);
});
