import { test, expect, type Page } from "@playwright/test";
import {
  CALENDAR_CLIENT_ID,
  CALENDAR_KEY,
  SETTINGS_KEY,
  calendarFixture,
  openPreview,
} from "./fixtures/seed";

test.use({ timezoneId: "Europe/Warsaw" });

const openSettings = async (page: Page) => {
  await page.getByRole("button", { name: "Otwórz ustawienia" }).click();
  await expect(
    page.getByRole("heading", { name: "Kalendarz", exact: true }),
  ).toBeVisible();
};
const calendar = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "null"), CALENDAR_KEY);

test("bez client ID kalendarz jest wyłączony, a połączenie kończy się czytelnym błędem", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page);
  await expect(
    page.getByText(
      "Kalendarz jest wyłączony: podaj identyfikator aplikacji (client ID).",
    ),
  ).toBeVisible();
  await expect(page.getByLabel("Dzierżawca")).toHaveValue("organizations");
  await page.getByRole("button", { name: "Połącz konto Microsoft" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "Podaj identyfikator aplikacji (client ID) z rejestracji w Entra ID, np. 00000000-0000-0000-0000-000000000000.",
  );
  await page
    .getByLabel("Identyfikator aplikacji (client ID)")
    .fill(CALENDAR_CLIENT_ID);
  await page.getByLabel("Dzierżawca").fill("zła/nazwa");
  await page.getByRole("button", { name: "Połącz konto Microsoft" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "Podaj dzierżawcę: organizations, identyfikator albo domenę firmy.",
  );
  expect(await calendar(page)).toBeNull();
});

test("połączenie pobiera spotkania, a rozłączenie je usuwa i zostawia client ID", async ({
  page,
}) => {
  await openPreview(page, { calendar: calendarFixture("ready") });
  await openSettings(page);
  await expect(page.getByText("Kalendarz jest wyłączony")).toHaveCount(0);
  await expect(
    page.getByLabel("Identyfikator aplikacji (client ID)"),
  ).toHaveValue(CALENDAR_CLIENT_ID);
  await page.getByRole("button", { name: "Połącz konto Microsoft" }).click();
  await expect(
    page.getByText("Połączono jako jan.kowalski@firma.example"),
  ).toBeVisible();
  // Daily i „wstępnie” przyjęty przegląd; anulowane i odrzucone nie zajmują czasu.
  await expect(page.getByText(/spotkania dziś: 2\./)).toBeVisible();
  await expect(page.getByText(/Ostatnie pobranie: 15 paź 2026, 12:00:00/)).toBeVisible();
  await expect(
    page.getByLabel("Identyfikator aplikacji (client ID)"),
  ).toBeDisabled();
  expect((await calendar(page)).events).toHaveLength(6);

  await page.getByRole("button", { name: "Rozłącz", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Połącz konto Microsoft" }),
  ).toBeVisible();
  await expect(page.getByText(/Połączono jako/)).toHaveCount(0);
  const after = await calendar(page);
  expect(after.events).toEqual([]);
  expect(after.email).toBe("");
  expect(after.clientId).toBe(CALENDAR_CLIENT_ID);
  await expect(
    page.getByLabel("Identyfikator aplikacji (client ID)"),
  ).toHaveValue(CALENDAR_CLIENT_ID);
});

test("„Odśwież” pobiera spotkania ponownie, a wyłącznik tytułów zapisuje się w ustawieniach", async ({
  page,
}) => {
  const fixture = calendarFixture();
  // W „chmurze” zostało tylko daily: przegląd usunięto z kalendarza.
  fixture.sample = fixture.sample.filter((e) => e.id !== "m-review");
  await openPreview(page, { calendar: fixture });
  await openSettings(page);
  await expect(page.getByText(/Ostatnie pobranie: 15 paź 2026, 11:50:00/)).toBeVisible();
  await expect(page.getByText(/spotkania dziś: 2\./)).toBeVisible();
  await page.getByRole("button", { name: "Odśwież", exact: true }).click();
  await expect(page.getByText(/Ostatnie pobranie: 15 paź 2026, 12:00:00/)).toBeVisible();
  await expect(page.getByText(/spotkania dziś: 1\./)).toBeVisible();
  expect(
    (await calendar(page)).events.map((e: { id: string }) => e.id),
  ).not.toContain("m-review");

  const titles = page.getByLabel("Pokazuj tytuły spotkań");
  await expect(titles).toBeChecked();
  await titles.uncheck();
  await expect(titles).not.toBeChecked();
  const settings = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    SETTINGS_KEY,
  );
  expect(settings).toEqual({ calendarShowTitles: false });
  await page.reload();
  await openSettings(page);
  await expect(page.getByLabel("Pokazuj tytuły spotkań")).not.toBeChecked();
  await expect(
    page.getByText("Połączono jako jan.kowalski@firma.example"),
  ).toBeVisible();
});
