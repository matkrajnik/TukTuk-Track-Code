import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures/test";
import {
  ids,
  openPreview,
  ORCA_KEY,
  redmineSeed,
  SETTINGS_KEY,
} from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import {
  orcaCleanPrompt,
  orcaCommandLine,
  orcaCreateArgs,
  orcaWorktreeName,
} from "../src/api/orca";

/**
 * Worktree Orki z kafelka zadania (PLAN_BACKLOG.md, B9): dialog w podglądzie
 * z symulowaną Orką (podgląd polecenia, wybór repozytorium i agenta, komunikat
 * „Orca nie działa”) i sekcja Ustawienia → Agenci. Żaden test nie uruchamia
 * prawdziwego orca.cmd.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const BINDINGS_KEY = "tuktuk-track-preview-agent-bindings-v1";
const ANALYSIS = "Analiza importu";
const IMPORT = "Import CSV zamówień";

const expand = (page: Page, title: string) =>
  page.getByLabel("Przedziały: " + title, { exact: true }).click();
const dialog = (page: Page) =>
  page.getByRole("dialog", { name: "Otwórz worktree w Orce" });
const command = (page: Page) => dialog(page).getByLabel("Polecenie Orki");
const orcaState = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
    ORCA_KEY,
  );
async function openDialog(page: Page, title: string, key: string) {
  await expand(page, title);
  await page
    .getByRole("button", { name: "Otwórz worktree w Orce: " + key })
    .click();
  await expect(dialog(page)).toBeVisible();
}

test("nazwa worktree z klucza i tytułu, a Redmine jako redmine-numer", () => {
  expect(orcaWorktreeName("WMS-12", "Analiza importu")).toBe(
    "wms-12-analiza-importu",
  );
  expect(orcaWorktreeName("#1234", "Błąd zapisu: etykiet (pilne!)")).toBe(
    "redmine-1234-blad-zapisu-etykiet-pilne",
  );
  expect(
    orcaWorktreeName("WMS-1", "Bardzo długi tytuł ".repeat(10)).length,
  ).toBeLessThanOrEqual(60);
  expect(orcaWorktreeName("", "")).toBe("worktree");
});

test("polecenie podglądu jest takie samo jak w Rust (ten sam przypadek co test jednostkowy)", () => {
  const args = orcaCreateArgs({
    name: "wms-200-nowy",
    repo: "C:\\Projekty\\wms",
    reference: "WMS-200",
    agent: "claude",
    prompt: 'WMS-200: Nowy eksport\n\nNotatka z "dnia" 50%',
  });
  expect(args).toEqual([
    "worktree",
    "create",
    "--json",
    "--name",
    "wms-200-nowy",
    "--repo",
    "path:C:\\Projekty\\wms",
    "--reference",
    "WMS-200",
    "--agent",
    "claude",
    "--prompt",
    "WMS-200: Nowy eksport Notatka z 'dnia' 50",
  ]);
  expect(orcaCommandLine(args)).toBe(
    "orca.cmd worktree create --json --name wms-200-nowy --repo path:C:\\Projekty\\wms --reference WMS-200 --agent claude --prompt \"WMS-200: Nowy eksport Notatka z 'dnia' 50\"",
  );
  expect(orcaCleanPrompt('a\r\n\tb  "c" %d%')).toBe("a b 'c' d");
  // Bez agenta prompt nie jest wysyłany.
  expect(
    orcaCreateArgs({ name: "x", repo: "C:\\r", prompt: "tekst", agent: null }),
  ).not.toContain("--prompt");
  expect(() => orcaCreateArgs({ name: "", repo: "C:\\r" })).toThrow();
  expect(() => orcaCreateArgs({ name: "a%b", repo: "C:\\r" })).toThrow();
  expect(() =>
    orcaCreateArgs({ name: "x", repo: "C:\\r", agent: "gpt" as never }),
  ).toThrow();
});

test("dialog pokazuje polecenie, pozwala wybrać repozytorium i agenta, a start dopiero po kliknięciu", async ({
  page,
}) => {
  await openPreview(page);
  await openDialog(page, ANALYSIS, "WMS-12");
  await expect(dialog(page).getByLabel("Nazwa worktree")).toHaveValue(
    "wms-12-analiza-importu",
  );
  // Prompt z tytułu i notatki dnia (do poprawienia).
  await expect(dialog(page).getByLabel("Prompt")).toHaveValue(
    /^WMS-12: Analiza importu/,
  );
  // Bez przypisanego folderu pierwsze repozytorium Orki.
  await expect(command(page)).toContainText(
    "orca.cmd worktree create --json --name wms-12-analiza-importu --repo path:C:\\Projekty\\wms --reference WMS-12 --agent claude --prompt",
  );

  // Wybór innego repozytorium z listy Orki.
  await dialog(page).getByRole("combobox", { name: "Repozytorium" }).click();
  await page.getByRole("option", { name: /api/ }).click();
  await expect(command(page)).toContainText("--repo path:C:\\Projekty\\api");

  // Agent Codex, potem bez agenta: prompt znika z polecenia i pole się blokuje.
  await dialog(page).getByLabel("Agent").selectOption("codex");
  await expect(command(page)).toContainText("--agent codex");
  await dialog(page).getByLabel("Agent").selectOption("");
  await expect(command(page)).not.toContainText("--agent");
  await expect(command(page)).not.toContainText("--prompt");
  await expect(dialog(page).getByLabel("Prompt")).toBeDisabled();

  // Do tej pory Orka nic nie dostała.
  expect((await orcaState(page)).created ?? []).toHaveLength(0);

  await dialog(page).getByRole("button", { name: "Utwórz worktree" }).click();
  await expect(dialog(page).getByRole("status").last()).toContainText(
    "Utworzono worktree:",
  );
  const created = (await orcaState(page)).created;
  expect(created).toHaveLength(1);
  expect(created[0].args).toContain("--reference");
  expect(created[0].args).toContain("path:C:\\Projekty\\api");
  expect(created[0].args).not.toContain("--agent");

  // Zamknięcie dialogu wraca do kafelka.
  await dialog(page).getByRole("button", { name: "Zamknij" }).click();
  await expect(dialog(page)).toHaveCount(0);
});

test("przypisany folder zadania jest pierwszym wyborem repozytorium", async ({
  page,
}) => {
  await openPreview(page);
  await page.evaluate(
    ([k, id]) =>
      localStorage.setItem(
        k as string,
        JSON.stringify([{ folder: "C:\\Projekty\\wms-wt", taskId: id }]),
      ),
    [BINDINGS_KEY, ids.analysis] as const,
  );
  await page.reload();
  await openDialog(page, ANALYSIS, "WMS-12");
  await expect(command(page)).toContainText("--repo path:C:\\Projekty\\wms-wt");
  await dialog(page).getByRole("combobox", { name: "Repozytorium" }).click();
  await expect(page.getByRole("listbox").getByRole("option")).toHaveCount(3);
});

test("zgłoszenie Redmine: nazwa redmine-numer i adres zgłoszenia jako --reference", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await openDialog(page, IMPORT, "#1234");
  await expect(dialog(page).getByLabel("Nazwa worktree")).toHaveValue(
    "redmine-1234-import-csv-zamowien",
  );
  await expect(command(page)).toContainText("--reference");
  await expect(command(page)).toContainText("/issues/1234");
});

test("gdy Orca nie działa: komunikat „Orca nie działa”, bez tworzenia, a „Sprawdź ponownie” po uruchomieniu", async ({
  page,
}) => {
  await openPreview(page, { orca: { running: false } });
  await openDialog(page, ANALYSIS, "WMS-12");
  await expect(dialog(page).getByRole("alert").first()).toContainText(
    "Orca nie działa.",
  );
  await expect(
    dialog(page).getByRole("button", { name: "Utwórz worktree" }),
  ).toBeDisabled();
  // Przypisany folder działa bez Orki; polecenie nadal da się obejrzeć po wyborze repo.
  await page.evaluate(
    (k) =>
      localStorage.setItem(
        k,
        JSON.stringify({ running: true }),
      ),
    ORCA_KEY,
  );
  await dialog(page).getByRole("button", { name: "Sprawdź ponownie" }).click();
  await expect(dialog(page).getByRole("alert")).toHaveCount(0);
  await expect(
    dialog(page).getByRole("button", { name: "Utwórz worktree" }),
  ).toBeEnabled();
});

test("„Uruchamiaj Orkę, gdy nie działa” uruchamia Orkę przy otwarciu dialogu", async ({
  page,
}) => {
  await openPreview(page, {
    orca: { running: false },
    settings: { orcaAutostart: true },
  });
  await openDialog(page, ANALYSIS, "WMS-12");
  await expect(dialog(page).getByRole("alert")).toHaveCount(0);
  await expect(dialog(page)).toContainText(
    "wms",
  );
  await expect(
    dialog(page).getByRole("button", { name: "Utwórz worktree" }),
  ).toBeEnabled();
  expect((await orcaState(page)).running).toBe(true);
  // Po starcie Orki znika informacja o jej uruchamianiu.
  await expect(dialog(page)).not.toContainText("zostanie uruchomiona");
});

test("brak orca.cmd: komunikat z podpowiedzią o Ustawieniach", async ({
  page,
}) => {
  await openPreview(page, { orca: { found: false } });
  await openDialog(page, ANALYSIS, "WMS-12");
  await expect(dialog(page).getByRole("alert").first()).toContainText(
    "Nie znaleziono orca.cmd",
  );
  await expect(
    dialog(page).getByRole("button", { name: "Utwórz worktree" }),
  ).toBeDisabled();
});

test("zadanie bez zgłoszenia nie ma akcji Orki", async ({ page }) => {
  await openPreview(page);
  await expand(page, "Code review");
  await expect(
    page.getByRole("button", { name: /Otwórz worktree w Orce/ }),
  ).toHaveCount(0);
});

test("Ustawienia → Agenci: ścieżka do orca.cmd i uruchamianie Orki", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Agenci");
  const path = page.getByLabel("Ścieżka do orca.cmd");
  await expect(page.getByLabel("Stan orca.cmd")).toContainText("Znaleziono:");
  const auto = page.getByRole("checkbox", {
    name: "Uruchamiaj Orkę, gdy nie działa",
  });
  await expect(auto).not.toBeChecked();

  await auto.check();
  await path.fill("D:\\Orca\\bin\\orca.cmd");
  await path.blur();
  await expect(page.getByLabel("Stan orca.cmd")).toContainText(
    "D:\\Orca\\bin\\orca.cmd",
  );
  const saved = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    SETTINGS_KEY,
  );
  expect(saved).toMatchObject({
    orcaAutostart: true,
    orcaPath: "D:\\Orca\\bin\\orca.cmd",
  });

  // Niepoprawna ścieżka: komunikat i powrót do zapisanej wartości.
  await path.fill("D:\\Orca\\orca.txt");
  await path.blur();
  await expect(page.getByRole("alert").first()).toContainText(
    "Ścieżka do orca.cmd musi wskazywać plik .cmd albo .exe",
  );
  await expect(path).toHaveValue("D:\\Orca\\bin\\orca.cmd");

  // Wyczyszczona ścieżka wraca do wykrywania.
  await path.fill("");
  await path.blur();
  await expect(path).toHaveValue("");
});
