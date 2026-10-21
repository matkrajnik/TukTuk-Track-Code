import { test, expect, type Page } from "./fixtures/test";
import {
  PREVIEW_KEY,
  at,
  ids,
  jiraFixture,
  openPreview,
  redmineSeed,
  seed,
  stored,
} from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";

/**
 * B2 — układ i nawigacja: przewijanie przy zmianie widoku, szeroki układ
 * tablic (`main.wide`), karta „Zagrożone teraz” w średnim oknie, zmiana dnia
 * po północy, „jutro” jako następny dzień roboczy, wspólne wyszukiwanie po
 * kluczu zgłoszenia i limit 30 dni na liście ukończonych.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const mainWidth = async (page: Page) =>
  (await page.locator("main#main").boundingBox())!.width;

/** Dane z 25 dodatkowymi zadaniami, żeby lista Zadań była długa. */
function longSeed() {
  const data = seed();
  for (let i = 0; i < 25; i++)
    (data.tasks as any[]).push({
      id: 100 + i,
      title: `Zadanie dodatkowe ${i + 1}`,
      done: false,
      createdAt: at("2026-10-09", "09:00"),
    });
  return data;
}

test("zmiana widoku zaczyna od góry, Zadania wracają w zapamiętane miejsce", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1180, height: 600 });
  await openPreview(page, { data: longSeed() });
  // Zadania (KeepAlive): przewinięte w dół.
  await page.evaluate(() => window.scrollTo(0, 500));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(500);

  await nav(page, "Dashboard");
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
  await page.evaluate(() => window.scrollTo(0, 300));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(300);

  // Ustawienia i Plan i historia otwierają się od góry.
  await nav(page, "Ustawienia");
  await expect(
    page.getByRole("heading", { name: "Czas pracy", exact: true }),
  ).toBeVisible();
  expect(await page.evaluate(() => window.scrollY)).toBe(0);
  await page.evaluate(() => window.scrollTo(0, 250));
  await nav(page, "Plan i historia");
  expect(await page.evaluate(() => window.scrollY)).toBe(0);

  // Powrót do Zadań: ta sama pozycja co przed wyjściem.
  await nav(page, "Zadania");
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(500);
  // Dashboard nie jest w KeepAlive: znowu od góry.
  await nav(page, "Dashboard");
  expect(await page.evaluate(() => window.scrollY)).toBe(0);
});

test("navigate do Zadań zaczyna nową listę od góry", async ({
  page,
}) => {
  const navigatePreview = (detail: object) =>
    page.evaluate(
      (d) =>
        window.dispatchEvent(
          new CustomEvent("todo-track-navigate", { detail: d }),
        ),
      detail,
    );
  await page.setViewportSize({ width: 1180, height: 600 });
  await openPreview(page, { data: longSeed() });
  await page.evaluate(() => window.scrollTo(0, 500));
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(500);

  const lastTask = page.getByText("Zadanie dodatkowe 25", { exact: true });
  const scrollY = () => page.evaluate(() => window.scrollY);

  // Z innego widoku: zapamiętana pozycja starej listy nie wraca, choć nowa
  // lista (wszystkie aktywne) jest równie długa.
  await nav(page, "Dashboard");
  await expect.poll(scrollY).toBe(0);
  await navigatePreview({ view: "tasks" });
  await expect(lastTask).toBeAttached();
  await page.waitForTimeout(100);
  expect(await scrollY()).toBe(0);

  // Już na Zadaniach: żądanie też przewija na górę.
  await page.evaluate(() => window.scrollTo(0, 400));
  await expect.poll(scrollY).toBe(400);
  await navigatePreview({ view: "tasks" });
  await expect.poll(scrollY).toBe(0);

  // Zwykły powrót przyciskiem nadal przywraca zapamiętane miejsce.
  await page.evaluate(() => window.scrollTo(0, 300));
  await expect.poll(scrollY).toBe(300);
  await nav(page, "Dashboard");
  await nav(page, "Zadania");
  await expect.poll(scrollY).toBe(300);
});

test("main.wide: tablica tygodnia na całą szerokość, listy w 1120 px", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1920, height: 800 });
  await openPreview(page);
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
  expect(await mainWidth(page)).toBeLessThanOrEqual(1120);

  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  await expect(page.getByRole("region", { name: "Tablica tygodnia" })).toBeVisible();
  await expect(page.locator("main#main")).toHaveClass(/wide/);
  expect(await mainWidth(page)).toBeGreaterThan(1300);
  // Osiem kolumn mieści się bez poziomego przewijania.
  const board = page.locator(".week-board");
  expect(
    await board.evaluate((el) => el.scrollWidth <= el.clientWidth + 1),
  ).toBe(true);

  // Podsumowanie i Dzień to listy: wracają do 1120 px.
  await nav(page, "Podsumowanie");
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
  expect(await mainWidth(page)).toBeLessThanOrEqual(1120);
  await nav(page, "Tablica");
  await expect(page.locator("main#main")).toHaveClass(/wide/);
  await nav(page, "Dzień");
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);

  // Wyjście z widoku z tablicą zdejmuje szeroki układ.
  await nav(page, "Tydzień");
  await expect(page.locator("main#main")).toHaveClass(/wide/);
  await nav(page, "Zadania");
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
  await nav(page, "Ustawienia");
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
});

test("main.wide: tablica Jiry na całą szerokość, lista zgłoszeń w 1120 px", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1700, height: 800 });
  await openDesktop(page, { data: seed(), jira: jiraFixture() });
  await nav(page, "Jira");
  await expect(page.locator(".jira-issue")).toHaveCount(3);
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
  expect(await mainWidth(page)).toBeLessThanOrEqual(1120);
  await page.getByRole("button", { name: "Tablica", exact: true }).click();
  await expect(page.locator(".jira-card")).toHaveCount(3);
  await expect(page.locator("main#main")).toHaveClass(/wide/);
  expect(await mainWidth(page)).toBeGreaterThan(1300);
  await nav(page, "Zadania");
  await expect(page.locator("main#main")).not.toHaveClass(/wide/);
});

test("Dashboard przy 1000 px: tytuł w „Zagrożone teraz” nie łamie się po literze", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await openPreview(page);
  await nav(page, "Dashboard");
  const risky = page.getByRole("region", { name: "Zagrożone teraz" });
  const rows = risky.locator(".dashboard-count");
  await expect(rows.filter({ hasText: "Analiza importu" })).toHaveCount(1);
  // Każdy tytuł w jednej linii i szerszy niż kilka liter; liczby w jednej linii.
  for (const row of await rows.all()) {
    const title = row.locator("> span");
    const box = (await title.boundingBox())!;
    expect(box.width).toBeGreaterThan(80);
    const lineHeight = await title.evaluate((el) =>
      parseFloat(getComputedStyle(el).lineHeight),
    );
    expect(box.height).toBeLessThan(lineHeight * 2);
    const numbers = (await row.locator("> strong").boundingBox())!;
    expect(numbers.height).toBeLessThan(lineHeight * 2);
  }
});

/** Podgląd z zegarem, który płynie (`clock.install`), zamiast stałego czasu. */
async function openRunning(page: Page, time: string) {
  await page.clock.install({ time: new Date(time) });
  await page.goto("/");
  await page.evaluate(
    ([key, value]) => {
      localStorage.clear();
      localStorage.setItem(key, value);
    },
    [PREVIEW_KEY, JSON.stringify(seed())] as const,
  );
  await page.reload();
}

test("po północy Plan i historia oraz Miesiąc przechodzą na nowy dzień", async ({
  page,
}) => {
  await openRunning(page, "2026-10-31T23:59:30+01:00");
  await nav(page, "Raporty dzienne");
  await nav(page, "Miesiąc");
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-10");
  await nav(page, "Plan i historia");
  await expect(page.getByLabel("Dzień planu")).toHaveValue("2026-10-31");

  await page.clock.runFor(45_000);
  await expect(page.getByLabel("Dzień planu")).toHaveValue("2026-11-01");
  await expect(
    page.getByRole("region", { name: "Plan dnia" }).getByRole("heading"),
  ).toContainText("1 lis");
  // Raporty (KeepAlive) z otwartym Miesiącem: listopad.
  await nav(page, "Raporty dzienne");
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-11");
});

test("po północy ręcznie wybrany dzień i miesiąc zostają", async ({ page }) => {
  await openRunning(page, "2026-10-31T23:59:30+01:00");
  await nav(page, "Raporty dzienne");
  await nav(page, "Miesiąc");
  await page.getByLabel("Miesiąc raportu").fill("2026-09");
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-09");
  await nav(page, "Plan i historia");
  await page.getByLabel("Dzień planu").fill("2026-10-20");
  await expect(page.getByLabel("Dzień planu")).toHaveValue("2026-10-20");

  await page.clock.runFor(45_000);
  // Zegar na pasku już po północy, a wybór się nie zmienił.
  await nav(page, "Dzisiaj");
  await expect(page.getByLabel("Dzień planu")).toHaveValue("2026-11-01");
  await nav(page, "Raporty dzienne");
  await expect(page.getByLabel("Miesiąc raportu")).toHaveValue("2026-09");
});

/** Piątek 16.10.2026: następny dzień roboczy to poniedziałek 19.10. */
const FRIDAY = "2026-10-16T12:00:00+02:00";
function fridayData() {
  const data = seed();
  const labels = (data.tasks as any[]).find((t) => t.id === ids.labels);
  labels.details.plannedDay = "2026-10-16";
  const review = (data.tasks as any[]).find((t) => t.id === ids.review);
  // Niedokończone z czwartku (poprzedni dzień roboczy) do Rano.
  review.details.plannedDay = "2026-10-15";
  return data;
}

test("„jutro” w piątek to poniedziałek: Zamknięcie dnia, Dzień i Rano", async ({
  page,
}) => {
  await openPreview(page, { now: FRIDAY, data: fridayData() });
  await nav(page, "Plan i historia");

  // Dzień: „Przełóż na pon., 19 paź”.
  const plan = page.getByRole("region", { name: "Plan dnia" });
  await expect(plan).toContainText("Wdrożenie etykiet");
  await plan
    .locator("article")
    .filter({ hasText: "Wdrożenie etykiet" })
    .getByRole("button", { name: "Przełóż na pon., 19 paź", exact: true })
    .click();
  await expect
    .poll(async () =>
      (await stored(page)).tasks.find((t: any) => t.id === ids.labels).details
        .plannedDay,
    )
    .toBe("2026-10-19");

  // Zamknięcie dnia: piątkowy plan na poniedziałek, data w etykiecie.
  const data = await stored(page);
  data.tasks.find((t: any) => t.id === ids.labels).details.plannedDay =
    "2026-10-16";
  await page.evaluate(
    ([k, v]) => localStorage.setItem(k, v),
    [PREVIEW_KEY, JSON.stringify(data)] as const,
  );
  await page.reload();
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await expect(
    page.getByText("do przeniesienia na następny dzień roboczy (pon., 19 paź)"),
  ).toBeVisible();
  await page
    .getByRole("button", {
      name: "Przenieś niedokończone na pon., 19 paź",
      exact: true,
    })
    .click();
  await expect
    .poll(async () =>
      (await stored(page)).tasks
        .filter((t: any) => t.id === ids.labels)
        .map((t: any) => t.details.plannedDay),
    )
    .toEqual(["2026-10-19"]);

  // Rano: „Na pon., 19 paź” przy niedokończonym z czwartku.
  await nav(page, "Rano");
  const review = page.getByRole("region", { name: "Przegląd dnia" });
  await review
    .getByRole("button", { name: "Na pon., 19 paź: Code review", exact: true })
    .click();
  await expect
    .poll(async () =>
      (await stored(page)).tasks.find((t: any) => t.id === ids.review).details
        .plannedDay,
    )
    .toBe("2026-10-19");
});

test("„jutro” omija dni wolne z ustawień: czwartek przy pracy pon.–czw.", async ({
  page,
}) => {
  await openPreview(page, {
    settings: { workDays: [1, 2, 3, 4] },
  });
  await nav(page, "Plan i historia");
  await expect(
    page
      .getByRole("region", { name: "Plan dnia" })
      .getByRole("button", { name: "Przełóż na pon., 19 paź", exact: true })
      .first(),
  ).toBeVisible();
  await nav(page, "Zamknięcie dnia");
  await expect(
    page.getByRole("button", {
      name: "Przenieś niedokończone na pon., 19 paź",
      exact: true,
    }),
  ).toBeVisible();
});

test("wyszukiwanie po kluczu Jiry i numerze Redmine: Zadania, Ctrl+K, tablica tygodnia", async ({
  page,
}) => {
  const data = redmineSeed();
  // Zadanie bez planu (kolumna „Do zaplanowania”) z kluczem Jiry.
  Object.assign((data.tasks as any[]).find((t) => t.id === ids.review), {
    jiraKey: "API-7",
    jiraUrl: "https://asistonwms.atlassian.net/browse/API-7",
  });
  await openPreview(page, { data });
  const search = page.getByLabel("Szukaj zadań");
  const titles = page.locator(".task-card .task-title");
  await search.fill("wms-12");
  await expect(titles).toHaveText(["Analiza importu"]);
  await search.fill("#1234");
  await expect(titles).toHaveText(["Import CSV zamówień"]);
  // Projekt i słowo tytułu razem.
  await search.fill("wms wdrożenie");
  await expect(titles).toHaveText(["Wdrożenie etykiet"]);
  await search.fill("");

  await page.keyboard.press("Control+k");
  const dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await dialog.getByLabel("Znajdź zadanie do uruchomienia").fill("WMS-12");
  await expect(dialog.locator(".quick-result > span")).toHaveText([
    "Analiza importu",
  ]);
  await dialog.getByLabel("Znajdź zadanie do uruchomienia").fill("1240");
  await expect(dialog.locator(".quick-result > span")).toHaveText([
    "Błąd zapisu etykiet",
  ]);
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);

  await nav(page, "Plan i historia");
  await nav(page, "Tydzień");
  // Do zaplanowania: szukanie po kluczu i po projekcie.
  const backlog = page.getByLabel("Szukaj w Do zaplanowania");
  const cards = page.locator(".week-backlog .week-card-title");
  await backlog.fill("api-7");
  await expect(cards).toHaveText(["Code review"]);
  await backlog.fill("wms-12");
  await expect(cards).toHaveCount(0);
  await backlog.fill("foundation");
  await expect(cards).toHaveText(["Code review"]);
});

const DAY = 86_400_000;
const NOW_MS = Date.parse("2026-10-15T12:00:00+02:00");
/** Ukończone: dziś, 10 dni temu, 29 dni temu (granica), 40 dni temu i stare bez `doneAt`. */
function doneData() {
  const data = seed();
  const done = (id: number, title: string, doneAt: number | null, extra = {}) => ({
    id,
    title,
    done: true,
    createdAt: Date.parse("2026-08-01T09:00:00+02:00"),
    doneAt,
    ...extra,
  });
  (data.tasks as any[]).push(
    done(30, "Ukończone dziś", NOW_MS - 3_600_000),
    done(31, "Ukończone 10 dni temu", NOW_MS - 10 * DAY),
    done(32, "Ukończone na granicy", Date.parse("2026-09-16T08:00:00+02:00")),
    done(33, "Ukończone dawno", NOW_MS - 40 * DAY),
    // Ukończone przed 2.0 (bez `doneAt`): dzień z ostatniego pomiaru.
    done(34, "Stare bez daty", null),
  );
  (data.sessions as any[]).push({
    id: 90,
    taskId: 34,
    startedAt: Date.parse("2026-08-20T09:00:00+02:00"),
    endedAt: Date.parse("2026-08-20T10:00:00+02:00"),
  });
  // „Dokumentacja API” z seeda: bez `doneAt`, praca 12.10 → w ostatnich 30 dniach.
  return data;
}

test("Ukończone: ostatnie 30 dni po dniu ukończenia, „Pokaż starsze”", async ({
  page,
}) => {
  await openPreview(page, { data: doneData() });
  await nav(page, "Ukończone");
  const done = page.getByRole("region", { name: "Ukończone", exact: true });
  await expect(page.locator(".task-group-heading")).toHaveText(["Ukończone 4"]);
  await expect(done.locator(".done-day-heading")).toHaveText([
    "czw., 15 paź",
    "pon., 12 paź",
    "pon., 5 paź",
    "śr., 16 wrz",
  ]);
  await expect(done.locator(".task-title")).toHaveText([
    "Ukończone dziś",
    "Dokumentacja API",
    "Ukończone 10 dni temu",
    "Ukończone na granicy",
  ]);
  await expect(page.getByText("Ukończone z ostatnich 30 dni.")).toBeVisible();

  await page.getByRole("button", { name: "Pokaż starsze (2)" }).click();
  await expect(page.locator(".task-group-heading")).toHaveText(["Ukończone 6"]);
  await expect(done.locator(".done-day-heading")).toHaveText([
    "czw., 15 paź",
    "pon., 12 paź",
    "pon., 5 paź",
    "śr., 16 wrz",
    "sob., 5 wrz",
    "czw., 20 sie",
  ]);
  await page.getByRole("button", { name: "Tylko ostatnie 30 dni" }).click();
  await expect(page.locator(".task-group-heading")).toHaveText(["Ukończone 4"]);

  // „Wszystkie”: ten sam limit dla ukończonych, otwarte bez zmian.
  await nav(page, "Wszystkie");
  await expect(done.locator(".task-title")).toHaveCount(4);
  await expect(page.getByRole("button", { name: "Pokaż starsze (2)" })).toBeVisible();
  // Wyszukiwanie liczy też ukryte starsze.
  await page.getByLabel("Szukaj zadań").fill("dawno");
  await expect(page.getByText("Brak pasujących zadań.")).toBeVisible();
  await page.getByRole("button", { name: "Pokaż starsze (1)" }).click();
  await expect(done.locator(".task-title")).toHaveText(["Ukończone dawno"]);
});

test("Ukończenie zapisuje chwilę ukończenia; zadanie trafia pod dzisiejszy dzień", async ({
  page,
}) => {
  await openPreview(page, { data: doneData() });
  await page.getByLabel("Ukończ: Code review", { exact: true }).click();
  await expect
    .poll(async () =>
      (await stored(page)).tasks.find((t: any) => t.id === ids.review).doneAt,
    )
    .toBe(NOW_MS);
  await nav(page, "Ukończone");
  const done = page.getByRole("region", { name: "Ukończone", exact: true });
  await expect(done.locator(".done-day-heading").first()).toHaveText(
    "czw., 15 paź",
  );
  await expect(done.locator(".task-title").nth(1)).toHaveText("Code review");
  // Przywrócenie czyści chwilę ukończenia.
  await page.getByLabel("Przywróć: Code review", { exact: true }).click();
  await expect
    .poll(async () =>
      (await stored(page)).tasks.find((t: any) => t.id === ids.review).doneAt,
    )
    .toBeNull();
});

test("Ukończone bez nowych zadań w 30 dniach: komunikat i „Pokaż starsze”", async ({
  page,
}) => {
  const data = seed();
  (data.tasks as any[]).find((t) => t.id === ids.docs).doneAt =
    NOW_MS - 45 * DAY;
  await openPreview(page, { data });
  await nav(page, "Ukończone");
  await expect(
    page.getByText("Brak zadań ukończonych w ostatnich 30 dniach."),
  ).toBeVisible();
  await page.getByRole("button", { name: "Pokaż starsze (1)" }).click();
  await expect(page.locator(".task-card .task-title")).toHaveText([
    "Dokumentacja API",
  ]);
});
