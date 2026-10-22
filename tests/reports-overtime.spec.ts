import { test, expect, type Page } from "./fixtures/test";
import {
  JIRA_SITE,
  TODAY,
  at,
  ids,
  jiraFixture,
  openPreview,
  redmineFixture,
  redmineIds,
  redmineSeed,
  seed,
} from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";

/**
 * B5: Raport dnia jak kafelki, nadgodziny dnia (Raport dnia, Dashboard,
 * Miesiąc), znacznik lokalnego zadania na listach Jiry i Redmine oraz linia
 * stanu obserwacji Jiry.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

/**
 * Seed z nadgodzinami: sobota 10.10 (dzień wolny, 1 h — cała praca to
 * nadgodziny), środa 14.10 (6:30 + 3:00 = 9:30 → 1:30) i dziś 15.10
 * (2:30 + 6:00 = 8:30 → 0:30). Razem 3:00.
 */
function overtimeSeed() {
  const data = seed();
  let id = 100;
  const add = (taskId: number, day: string, from: string, to: string) =>
    data.sessions.push({
      id: id++,
      taskId,
      startedAt: at(day, from),
      endedAt: at(day, to),
    });
  add(ids.review, "2026-10-10", "10:00", "11:00");
  add(ids.review, "2026-10-14", "16:00", "19:00");
  add(ids.review, TODAY, "02:00", "08:00");
  data.sessions.sort((a, b) => b.startedAt - a.startedAt);
  return data;
}

test("daily report lists tasks like tiles with the day note and no timer actions", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await nav(page, "Raporty dzienne");
  await page.getByLabel("Dzień raportu").fill("2026-10-14");
  await expect(page.locator(".report-card")).toHaveCount(2);
  const analysis = page
    .locator(".report-card")
    .filter({ hasText: "Analiza importu" });
  // Projekt, klucz i status zgłoszenia, plan z procentem, termin i notatka dnia.
  await expect(analysis.locator(".task-chip").first()).toHaveText("WMS");
  await expect(analysis.locator(".task-jira-key")).toHaveText("WMS-12");
  await expect(analysis.locator(".task-jira-status")).toHaveText("W toku");
  // Plan 10 h; zmierzone 3 + 6 + 2 + 1 (trwający) = 12 h → 120%.
  await expect(analysis).toContainText("Plan 10:00 · 120%");
  await expect(analysis.locator(".estimate-over")).toHaveText("120%");
  await expect(analysis.locator(".task-deadline")).toHaveText(
    "Termin: pt., 16 paź",
  );
  await expect(analysis.locator(".task-day-note")).toHaveText(
    "Notatka: Mapowanie kolumn",
  );
  await expect(analysis).toContainText("06:00:00");
  // Bez akcji Start / Ukończ z listy zadań.
  const list = page.locator(".report-list");
  await expect(list.getByRole("button", { name: /^Start: / })).toHaveCount(0);
  await expect(list.getByRole("button", { name: /^Ukończ: / })).toHaveCount(0);
  await expect(list.locator(".row-timer, .checkbox")).toHaveCount(0);
  // Przedziały nadal rozwijają się z karty.
  await page
    .getByRole("button", {
      name: "Przedziały raportu: Analiza importu",
      exact: true,
    })
    .click();
  await expect(analysis.locator(".session-row")).toHaveCount(1);

  // Dziś: trwający pomiar na karcie jako „W trakcie”, notatki z innego dnia nie ma.
  await page.getByRole("button", { name: "Dzisiaj", exact: true }).click();
  const today = page
    .locator(".report-card")
    .filter({ hasText: "Analiza importu" });
  await expect(today.locator(".task-live")).toHaveText("W trakcie");
  await expect(today.locator(".task-day-note")).toHaveCount(0);
  await expect(page.locator(".report-overtime")).toHaveCount(0);
});

test("overtime is shown in the daily report, on the dashboard and in the month", async ({
  page,
}) => {
  await openPreview(page, { data: overtimeSeed() });

  // Raport dnia: linia „Nadgodziny” pod przepracowanym czasem.
  await nav(page, "Raporty dzienne");
  await expect(page.locator(".report-total")).toHaveText("08:30:00");
  await expect(page.locator(".report-overtime")).toHaveText("Nadgodziny: 0:30");
  await page.getByLabel("Dzień raportu").fill("2026-10-14");
  await expect(page.locator(".report-total")).toHaveText("09:30:00");
  await expect(page.locator(".report-overtime")).toHaveText("Nadgodziny: 1:30");
  // Sobota: dzień wolny, cała praca to nadgodziny.
  await page.getByLabel("Dzień raportu").fill("2026-10-10");
  await expect(page.locator(".report-overtime")).toHaveText("Nadgodziny: 1:00");
  await expect(page.locator(".report-overtime")).toHaveAttribute(
    "title",
    "Dzień wolny: cała praca to nadgodziny",
  );
  await page.getByLabel("Dzień raportu").fill("2026-10-13");
  await expect(page.locator(".report-overtime")).toHaveCount(0);

  // Dashboard: karta „Dzisiaj” i wykres 7 dni z fragmentem nadgodzin.
  await nav(page, "Dashboard");
  await expect(page.locator(".dashboard-overtime")).toHaveText(
    "Nadgodziny: 0:30",
  );
  const week = page.getByRole("region", {
    name: "Czas pracy w ostatnich 7 dniach",
  });
  await expect(week.locator(".dashboard-week-overtime")).toHaveText(
    "Nadgodziny w tych 7 dniach: 3:00",
  );
  await expect(week.locator(".dashboard-bar .overtime")).toHaveCount(3);
  const wednesday = week.getByRole("button", { name: /^Raport 2026-10-14:/ });
  await expect(wednesday).toHaveAttribute("title", "Nadgodziny: 1:30");
  // Fragment nadgodzin innym kolorem niż reszta słupka.
  const colors = await wednesday
    .locator(".dashboard-bar > span:not(.target-line)")
    .evaluateAll((spans) =>
      spans.map((s) => getComputedStyle(s).backgroundColor),
    );
  expect(colors).toHaveLength(2);
  expect(colors[0]).not.toBe(colors[1]);
  // Słupek bez nadgodzin ma jeden fragment.
  await expect(
    week
      .getByRole("button", { name: /^Raport 2026-10-13:/ })
      .locator(".dashboard-bar .overtime"),
  ).toHaveCount(0);

  // Miesiąc: kolumna „Nadgodziny” przy dniach, suma pod wykresem i w podsumowaniu.
  await nav(page, "Raporty dzienne");
  await nav(page, "Miesiąc");
  await expect(page.locator(".monthly-overtime-total")).toHaveText("03:00");
  const chart = page.getByRole("region", {
    name: "Czas pracy w dniach miesiąca",
  });
  await expect(chart.locator(".monthly-day")).toHaveCount(31);
  await expect(chart.locator(".monthly-day-head")).toContainText("Nadgodziny");
  await expect(
    chart
      .getByRole("button", { name: /^Raport 2026-10-14:/ })
      .locator(".monthly-overtime"),
  ).toHaveText("01:30");
  await expect(
    chart
      .getByRole("button", { name: /^Raport 2026-10-10:/ })
      .locator(".monthly-overtime"),
  ).toHaveText("01:00");
  await expect(
    chart
      .getByRole("button", { name: /^Raport 2026-10-13:/ })
      .locator(".monthly-overtime"),
  ).toHaveText("–");
  await expect(chart.locator(".monthly-day-foot .monthly-overtime")).toHaveText(
    "03:00",
  );
  await expect(chart.locator(".monthly-day .overtime")).toHaveCount(3);

  // Niższy cel dnia zwiększa nadgodziny od razu.
  await openSettings(page, "Czas pracy");
  const target = page.getByLabel("Cel dzienny (minuty)");
  await target.fill("420");
  await target.press("Tab");
  await nav(page, "Dashboard");
  await expect(page.locator(".dashboard-overtime")).toHaveText(
    "Nadgodziny: 1:30",
  );
});

test("Jira list marks issues with a local task and warns about archived or trashed ones", async ({
  page,
}) => {
  const data = seed();
  // Zadanie w archiwum powiązane z WMS-31 (fixture: Stary raport).
  const archived = data.tasks.find((t) => t.id === ids.archived) as any;
  archived.jiraKey = "WMS-31";
  archived.jiraUrl = `${JIRA_SITE}/browse/WMS-31`;
  const jira = jiraFixture();
  jira.issues.push({
    key: "WMS-40",
    summary: "Nowy eksport",
    status: "Do zrobienia",
    boards: ["WMS"],
    issueType: "Zadanie",
    priority: "Medium",
    assignee: { accountId: "a-me", displayName: "Użytkownik testowy" },
  });
  await openDesktop(page, { data, jira });
  await nav(page, "Jira");
  await expect(page.locator(".jira-issue")).toHaveCount(4);
  const issue = (key: string) =>
    page.locator(".jira-issue").filter({
      has: page.getByRole("button", { name: "Otwórz w Jirze: " + key }),
    });

  // WMS-12: zadanie z listy, dziś 08:30–10:30.
  await expect(issue("WMS-12").locator(".issue-task-marker")).toHaveText(
    "W zadaniach · dziś 02:00",
  );
  await expect(issue("WMS-12")).toHaveClass(/hasTask/);
  await expect(
    page.getByRole("button", { name: "Rozpocznij: WMS-12" }),
  ).toHaveAttribute(
    "title",
    "Rozpocznij pomiar istniejącego zadania „Analiza importu”",
  );
  // WMS-31: zadanie w archiwum, WMS-7: w koszu — ostrzeżenie zamiast znacznika.
  await expect(issue("WMS-31").locator(".issue-task-marker")).toHaveCount(0);
  await expect(issue("WMS-31").locator(".issue-task-warning")).toHaveText(
    "Zadanie jest w archiwum — „Rozpocznij” przywróci je zamiast tworzyć duplikat.",
  );
  await expect(issue("WMS-7").locator(".issue-task-warning")).toHaveText(
    "Zadanie jest w koszu — „Rozpocznij” przywróci je z historią zamiast tworzyć duplikat.",
  );
  await expect(
    page.getByRole("button", { name: "Rozpocznij: WMS-7" }),
  ).toHaveAttribute("title", "Przywróć zadanie z kosza i rozpocznij pomiar");
  // WMS-40: bez lokalnego zadania.
  await expect(
    issue("WMS-40").locator(".issue-task-marker, .issue-task-warning"),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Rozpocznij: WMS-40" }),
  ).toHaveAttribute("title", "Utwórz nowe zadanie i rozpocznij pomiar");
});

test("Redmine cards mark issues with a local task", async ({ page }) => {
  const data = redmineSeed();
  (data.tasks as any[]).find((t) => t.id === redmineIds.labels).archived = true;
  await openDesktop(page, { data, redmine: redmineFixture() });
  await nav(page, "Redmine");
  const card = (id: number) => page.locator(`.redmine-card[aria-label^="#${id} "]`);
  await expect(card(1234).locator(".issue-task-marker")).toHaveText(
    "W zadaniach · dziś 00:00",
  );
  await expect(card(1240).locator(".issue-task-warning")).toContainText(
    "w archiwum",
  );
  await expect(
    card(1250).locator(".issue-task-marker, .issue-task-warning"),
  ).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Rozpocznij: #1250" }),
  ).toHaveAttribute("title", "Utwórz nowe zadanie i rozpocznij pomiar");
});

test("Jira watch status line shows the last check and the last error", async ({
  page,
}) => {
  await openPreview(page);
  // Symulowana Jira: ostatnie udane sprawdzenie 11:05, kolejne kończy się 401.
  await page.evaluate(
    ([lastOk]) =>
      localStorage.setItem(
        "tuktuk-track-preview-jira-v1",
        JSON.stringify({
          watch: { lastOk, lastError: "", lastErrorAt: null },
          watchFail: "Jira odrzuciła token API (401). Połącz konto ponownie.",
        }),
      ),
    [at(TODAY, "11:05")] as const,
  );
  await page.reload();
  await nav(page, "Jira");
  const line = page.locator(".jira-watch-line");
  await expect(line).toHaveText(
    "Ostatnio sprawdzono 11:05 · błąd: Jira odrzuciła token API (401). Połącz konto ponownie.",
  );
  await expect(line).toHaveClass(/jira-watch-error/);
  await expect(line).toHaveAttribute(
    "title",
    "Błąd obserwacji przypisań o 12:00",
  );

  // Ustawienia → Jira: ta sama linia.
  await openSettings(page, "Jira");
  await expect(page.locator(".jira-watch-line")).toHaveText(
    /^Ostatnio sprawdzono 11:05 · błąd: Jira odrzuciła token API \(401\)/,
  );

  // Token poprawiony: kolejne sprawdzenie (po 5 min) czyści błąd.
  await page.evaluate(() => {
    const key = "tuktuk-track-preview-jira-v1";
    const state = JSON.parse(localStorage.getItem(key)!);
    delete state.watchFail;
    localStorage.setItem(key, JSON.stringify(state));
  });
  await page.clock.setFixedTime(new Date(at(TODAY, "12:06")));
  await page.reload();
  await nav(page, "Jira");
  await expect(line).toHaveText("Ostatnio sprawdzono 12:06");
  await expect(line).not.toHaveClass(/jira-watch-error/);

  // Wyłączona obserwacja: bez linii.
  await openSettings(page, "Jira");
  await page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie").uncheck();
  await expect(page.locator(".jira-watch-line")).toHaveCount(0);
});

test("Jira view shows the watch error reported by the backend", async ({
  page,
}) => {
  await openDesktop(page, {
    jira: jiraFixture(),
    jiraWatch: {
      lastOk: null,
      lastError: "Brak połączenia z Jirą.",
      lastErrorAt: at(TODAY, "11:55"),
    },
  });
  await nav(page, "Jira");
  await expect(page.locator(".jira-watch-line")).toHaveText(
    "Jeszcze nie sprawdzono · błąd: Brak połączenia z Jirą.",
  );
});
