// Bez wspólnego `./fixtures/test`: ten plik sprawdza samo otwieranie Rano,
// więc strona startuje bez stanu „Rano już pokazane”.
import { test, expect, type Page } from "@playwright/test";
import {
  NOW,
  PREVIEW_KEY,
  SETTINGS_KEY,
  TODAY,
  at,
  manualMeetings,
  openPreview,
  seed,
} from "./fixtures/seed";
import { openDesktop, backend } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
import { MORNING_SHOWN_KEY } from "./fixtures/test";

/**
 * B11: plan dnia (Plan i historia → Rano) przy pierwszym uruchomieniu albo
 * pokazaniu okna w dniu roboczym bez zapisanego przeglądu. Dane z
 * `fixtures/seed.ts`: dziś czwartek 15.10.2026 12:00 (dzień roboczy).
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
const review = (page: Page) =>
  page.getByRole("region", { name: "Przegląd dnia" });
const journal = (page: Page) =>
  page.getByRole("button", { name: "Plan i historia", exact: true });
const newTask = (page: Page) => page.getByLabel("Nowe zadanie", { exact: true });
const shown = (page: Page) =>
  page.evaluate((k) => localStorage.getItem(k), MORNING_SHOWN_KEY);

/** Bieżące dane podglądu bez czyszczenia `localStorage` (ponowne uruchomienie). */
const restart = (page: Page) => page.reload();

test("pierwsze uruchomienie bez przeglądu otwiera Rano, raz dziennie", async ({
  page,
}) => {
  await openPreview(page);
  await expect(review(page)).toBeVisible();
  await expect(journal(page)).toHaveAttribute("aria-current", "page");
  await expect(newTask(page)).toHaveCount(0);
  expect(await shown(page)).toBe(TODAY);

  // Ręczne przejście gdzie indziej: aplikacja nie wraca do Rano tego dnia,
  // także po ponownym uruchomieniu.
  await nav(page, "Zadania");
  await expect(newTask(page)).toBeVisible();
  await restart(page);
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  // Nic nie trafiło do bazy ani ustawień.
  expect(
    await page.evaluate((k) => localStorage.getItem(k), SETTINGS_KEY),
  ).toBeNull();
});

test("start z zapisanym przeglądem dnia zostaje na liście zadań", async ({
  page,
}) => {
  await openPreview(page, {
    settings: { morningReviewDay: TODAY, morningReviewAt: at(TODAY, "08:05") },
  });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  expect(await shown(page)).toBeNull();
});

test("„Zaczynam dzień” po automatycznym Rano: kolejne uruchomienie na liście", async ({
  page,
}) => {
  await openPreview(page);
  await review(page).getByRole("button", { name: "Zaczynam dzień" }).click();
  await expect(review(page).getByText("Dzień rozpoczęty o 12:00")).toBeVisible();
  // Nawet bez zapamiętanego „pokazano dziś” zapisany przegląd wystarcza.
  await page.evaluate((k) => localStorage.removeItem(k), MORNING_SHOWN_KEY);
  await restart(page);
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
});

test("wyłączony przełącznik w Ustawienia → Uruchamianie", async ({ page }) => {
  await openPreview(page, { settings: { openMorningOnStart: false } });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);

  await openSettings(page, "Uruchamianie");
  const toggle = page.getByLabel(
    "Otwieraj plan dnia przy pierwszym uruchomieniu",
    { exact: true },
  );
  await expect(toggle).not.toBeChecked();
  await toggle.check();
  await expect
    .poll(() =>
      page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
    )
    .toEqual({ openMorningOnStart: true });
  // Włączony: następne uruchomienie tego dnia otwiera Rano.
  await restart(page);
  await expect(review(page)).toBeVisible();
  // Wyłączenie z domyślnego stanu zapisuje się w ustawieniach.
  await openSettings(page, "Uruchamianie");
  await toggle.uncheck();
  await expect
    .poll(() =>
      page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), SETTINGS_KEY),
    )
    .toEqual({ openMorningOnStart: false });
});

test("święto, weekend i całodniowy urlop z kalendarza: bez Rano", async ({
  page,
}) => {
  // Środa 11.11.2026: Święto Niepodległości.
  await openPreview(page, { now: "2026-11-11T09:00:00+01:00" });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  // Sobota 17.10.2026.
  await openPreview(page, { now: "2026-10-17T09:00:00+02:00" });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  // Czwartek z całodniowym „poza biurem” (ręczne spotkanie, B12).
  await openPreview(page, {
    meetings: [
      ...manualMeetings(),
      {
        id: "manual:3",
        start: at(TODAY, "00:00"),
        end: at("2026-10-16", "00:00"),
        subject: "Urlop",
        showAs: "oof",
        allDay: true,
        taskId: null,
      },
    ],
  });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  expect(await shown(page)).toBeNull();
});

test("zmiana dnia zegarem: po północy Rano otwiera się raz", async ({ page }) => {
  // Środa 14.10 23:59:30, przegląd środy zapisany: zostaje lista zadań.
  await page.clock.install({ time: new Date("2026-10-14T23:59:30+02:00") });
  await page.goto("/");
  await page.evaluate(
    ([dataKey, data, settingsKey, settings]) => {
      localStorage.clear();
      localStorage.setItem(dataKey, data);
      localStorage.setItem(settingsKey, settings);
    },
    [
      PREVIEW_KEY,
      JSON.stringify(seed()),
      SETTINGS_KEY,
      JSON.stringify({
        morningReviewDay: "2026-10-14",
        morningReviewAt: at("2026-10-14", "08:00"),
      }),
    ] as const,
  );
  await page.reload();
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);

  // Czwartek 15.10: nowy dzień roboczy bez przeglądu.
  await page.clock.runFor(45_000);
  await expect(review(page)).toBeVisible();
  expect(await shown(page)).toBe(TODAY);
  await nav(page, "Zadania");
  await page.clock.runFor(60_000);
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
});

test("aplikacja: start schowany do traya otwiera Rano przy pierwszym pokazaniu okna", async ({
  page,
}) => {
  await openDesktop(page, { hidden: true });
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
  expect(await shown(page)).toBeNull();

  await backend(page, "show");
  await expect(review(page)).toBeVisible();
  expect(await shown(page)).toBe(TODAY);
  // Kolejne pokazanie tego dnia nie wraca do Rano.
  await nav(page, "Zadania");
  await backend(page, "show");
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
});

test("aplikacja: widoczne okno z zapisanym przeglądem zostaje na liście", async ({
  page,
}) => {
  await openDesktop(page, {
    settings: { morningReviewDay: TODAY, morningReviewAt: Date.parse(NOW) },
  });
  await expect(newTask(page)).toBeVisible();
  await backend(page, "show");
  await expect(newTask(page)).toBeVisible();
  await expect(review(page)).toHaveCount(0);
});

test("aplikacja: widoczne okno bez przeglądu otwiera Rano od razu", async ({
  page,
}) => {
  await openDesktop(page);
  await expect(review(page)).toBeVisible();
  await expect(journal(page)).toHaveAttribute("aria-current", "page");
});
