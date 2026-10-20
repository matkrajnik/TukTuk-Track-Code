import { test, expect, type Page } from "@playwright/test";
import { previewTsKey } from "../src/store";
import { matchingSections, sectionInMessage } from "../src/navigation";
import { openPreview } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Ustawienia → Timesheet (TS) (PLAN_1.8.md, T4) na symulowanym TS podglądu
 * (`store.ts`, klucz `todo-track-preview-ts-v1`). Żaden test nie łączy się
 * z prawdziwym serwerem TS.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const SECTION = "Timesheet (TS)";
const panel = (page: Page) => page.getByRole("tabpanel");
async function setTs(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [previewTsKey, JSON.stringify(state)] as const,
  );
  await page.reload();
}
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);
const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();
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
  await expect(project.locator("option")).toHaveText([
    "— wybierz —",
    "Foundation",
    "WMS",
    "Zespół",
  ]);
  const add = panel(page).getByRole("button", { name: "Dodaj mapowanie" });
  await expect(add).toBeDisabled();
  await project.selectOption("WMS");
  await panel(page).getByLabel("Domyślne zadanie TS").selectOption({ label: "Code review" });
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
      new CustomEvent("todo-track-navigate", {
        detail: { view: "settings", section: "timesheet" },
      }),
    ),
  );
  await expectTimesheet(page);
});
