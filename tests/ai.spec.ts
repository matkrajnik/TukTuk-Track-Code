import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures/test";
import { AI_KEY, NOW, openPreview, SETTINGS_KEY } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import {
  aiErrorLabel,
  aiMessages,
  aiTestPrompt,
} from "../src/api/ai";
import { aiPathError } from "../src/settings";

/**
 * Asystent AI (PLAN_BACKLOG.md, A1): dialog pierwszego uruchomienia z 0, 1 i 2
 * wykrytymi CLI, zapamiętany wybór po przeładowaniu oraz Ustawienia → Asystent AI
 * na symulowanym silniku (`src/preview/ai.ts`). Żaden test nie uruchamia
 * prawdziwego claude ani codex.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };
const codex = { path: "C:\\Users\\ala\\AppData\\Roaming\\npm\\codex.cmd", version: "0.162.1" };

const dialog = (page: Page) => page.getByRole("dialog", { name: "Asystent AI" });
const stored = (page: Page, key: string) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
    key,
  );
/**
 * Skrypt odpowiedzi symulowanego silnika (`script`) bez kasowania dziennika
 * wywołań, który podgląd trzyma w tym samym kluczu.
 */
const setScript = (page: Page, script: Record<string, unknown>) =>
  page.evaluate(
    ([key, value]) => {
      const state = JSON.parse(localStorage.getItem(key as string) ?? "{}");
      localStorage.setItem(key as string, JSON.stringify({ ...state, script: value }));
    },
    [AI_KEY, script] as const,
  );
const status = (page: Page) => page.getByRole("status", { name: "Stan asystenta AI" });

test("bez wykrytego CLI nie ma dialogu pierwszego uruchomienia", async ({ page }) => {
  await openPreview(page);
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await expect(dialog(page)).toHaveCount(0);
  await openPreview(page, { ai: { claude: null, codex: null } });
  await expect(dialog(page)).toHaveCount(0);
  // Ustawienia pokazują, co zrobić zamiast dialogu.
  await openSettings(page, "Asystent AI");
  await expect(status(page)).toContainText("Nie znaleziono Claude Code ani Codexa");
  await expect(status(page).getByRole("link", { name: "Claude Code" })).toBeVisible();
  await expect(status(page).getByRole("link", { name: "Codex" })).toBeVisible();
  // Wybór nadal jest „jeszcze nie pytano”.
  expect((await stored(page, SETTINGS_KEY)).aiEngine ?? "").toBe("");
});

test("jedno wykryte CLI: dialog z Claude Code i Nie używaj, bez Codexa", async ({ page }) => {
  await openPreview(page, { ai: { claude } });
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page)).toContainText("subskrypcji");
  await expect(dialog(page)).toContainText("ukrywane");
  await expect(dialog(page)).toContainText("bez Twojego kliknięcia");
  await expect(dialog(page).getByRole("button")).toHaveText(["Claude Code", "Nie używaj", "Później"]);
});

test("dwa wykryte CLI: wybór Codexa zapisany po przeładowaniu, dialog nie wraca", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex } });
  await expect(dialog(page).getByRole("button")).toHaveText([
    "Claude Code",
    "Codex",
    "Nie używaj",
    "Później",
  ]);
  await dialog(page).getByRole("button", { name: "Codex", exact: true }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await stored(page, SETTINGS_KEY)).aiEngine).toBe("codex");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await expect(dialog(page)).toHaveCount(0);
  await openSettings(page, "Asystent AI");
  await expect(page.getByLabel("Silnik asystenta AI")).toHaveValue("codex");
});

test("Nie używaj zapisuje wybór i dialog nie wraca", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex } });
  await dialog(page).getByRole("button", { name: "Nie używaj" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await stored(page, SETTINGS_KEY)).aiEngine).toBe("off");
  await page.reload();
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await expect(dialog(page)).toHaveCount(0);
});

test("zamknięcie dialogu niczego nie zapisuje i aplikacja pyta przy następnym starcie", async ({ page }) => {
  await openPreview(page, { ai: { claude } });
  await expect(dialog(page)).toBeVisible();
  // Vue pomija zagnieżdżone obsługi zdarzenia, gdy zegar strony stoi w miejscu
  // (znacznik obsługi i zdarzenia jest ten sam), więc zegar rusza o sekundę.
  await page.clock.setFixedTime(Date.parse(NOW) + 1000);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  expect((await stored(page, SETTINGS_KEY)).aiEngine ?? "").toBe("");
  await page.reload();
  await expect(dialog(page)).toBeVisible();
  // „Później” też zamyka bez zapisu.
  await page.clock.setFixedTime(Date.parse(NOW) + 2000);
  await dialog(page).getByRole("button", { name: "Później" }).click();
  await expect(dialog(page)).toHaveCount(0);
  expect((await stored(page, SETTINGS_KEY)).aiEngine ?? "").toBe("");
  await page.reload();
  await expect(dialog(page)).toBeVisible();
});

test("wybrany silnik z ustawień nie wywołuje dialogu, nawet gdy CLI jest wykryte", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex }, settings: { aiEngine: "claude" } });
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await expect(dialog(page)).toHaveCount(0);
});

test("Ustawienia: sekcja Asystent AI pokazuje wersje, silnik i domyślne przełączniki", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex }, settings: { aiEngine: "off" } });
  await openSettings(page, "Asystent AI");
  await expect(
    page.getByRole("tabpanel").getByRole("heading", { name: "Asystent AI", exact: true }),
  ).toBeVisible();
  await expect(status(page)).toContainText("Claude Code: wersja 2.1.304");
  await expect(status(page)).toContainText("Codex: wersja 0.162.1");
  await expect(page.getByLabel("Silnik asystenta AI")).toHaveValue("off");
  await expect(page.getByLabel("Wysyłaj tytuły spotkań")).not.toBeChecked();
  await expect(page.getByLabel("Wysyłaj tytuły okien")).not.toBeChecked();
  for (const name of [
    "Serwer MCP dla agentów",
    "Czat",
    "Podpowiedzi przypisań",
    "Asystent w Zamknięciu dnia",
  ])
    await expect(page.getByLabel(name, { exact: true })).toBeChecked();
  await expect(page.getByLabel("Model szybki")).toHaveValue("");
  await expect(page.getByLabel("Model główny")).toHaveValue("");
  await expect(page.getByText("nazwisko spoza tej listy")).toBeVisible();
  await expect(page.getByRole("button", { name: "Przetestuj silnik" })).toBeDisabled();
  await expect(page.getByText("Jeszcze nie było wywołań.")).toBeVisible();
});

test("Ustawienia: zmiana silnika, modeli, nazw i przełączników jest zapisywana", async ({ page }) => {
  await openPreview(page, { ai: { claude } });
  // Dialog pierwszego uruchomienia zasłania widok: zamykamy go bez wyboru.
  await expect(dialog(page)).toBeVisible();
  await page.clock.setFixedTime(Date.parse(NOW) + 1000);
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await openSettings(page, "Asystent AI");
  const engine = page.getByLabel("Silnik asystenta AI");
  // Niewykryty Codex nie jest do wyboru, wykryty Claude Code tak.
  await expect(engine.locator("option", { hasText: "Codex" })).toHaveAttribute("disabled", "");
  await engine.selectOption("claude");
  await page.getByLabel("Model szybki").fill("haiku");
  await page.getByLabel("Model szybki").blur();
  await page.getByLabel("Model główny").fill("claude-opus-4-5");
  await page.getByLabel("Model główny").blur();
  await page.getByLabel("Ukrywaj te nazwy (jedna w wierszu)").fill("Jan Kowalski\n\nMarzec\njan kowalski");
  await page.getByLabel("Ukrywaj te nazwy (jedna w wierszu)").blur();
  await page.getByLabel("Wysyłaj tytuły spotkań").check();
  await page.getByLabel("Czat", { exact: true }).uncheck();
  await expect
    .poll(async () => (await stored(page, SETTINGS_KEY)).aiChat)
    .toBe(false);
  const saved = await stored(page, SETTINGS_KEY);
  expect(saved).toMatchObject({
    aiEngine: "claude",
    aiModelFast: "haiku",
    aiModelMain: "claude-opus-4-5",
    aiHiddenNames: ["Jan Kowalski", "Marzec"],
    aiSendMeetingTitles: true,
    aiChat: false,
  });
  // Po przeładowaniu pola mają zapisane wartości.
  await page.reload();
  await openSettings(page, "Asystent AI");
  await expect(page.getByLabel("Silnik asystenta AI")).toHaveValue("claude");
  await expect(page.getByLabel("Model szybki")).toHaveValue("haiku");
  await expect(page.getByLabel("Ukrywaj te nazwy (jedna w wierszu)")).toHaveValue("Jan Kowalski\nMarzec");
  await expect(page.getByLabel("Czat", { exact: true })).not.toBeChecked();
});

test("Ustawienia: niepoprawna ścieżka i model dają komunikat, a pole wraca do zapisanej wartości", async ({ page }) => {
  await openPreview(page, { ai: { claude }, settings: { aiEngine: "off" } });
  await openSettings(page, "Asystent AI");
  await page.getByLabel("Ścieżka do claude").fill("claude");
  await page.getByLabel("Ścieżka do claude").blur();
  await expect(page.getByRole("alert")).toContainText(aiPathError);
  await expect(page.getByLabel("Ścieżka do claude")).toHaveValue("");
  await page.getByLabel("Model szybki").fill("zły model");
  await page.getByLabel("Model szybki").blur();
  await expect(page.getByRole("alert")).toContainText("Nazwa modelu AI");
  await expect(page.getByLabel("Model szybki")).toHaveValue("");
});

test("Ustawienia: Sprawdź ponownie wykrywa świeżo zainstalowane CLI", async ({ page }) => {
  await openPreview(page, { ai: {}, settings: { aiEngine: "off" } });
  await openSettings(page, "Asystent AI");
  await expect(status(page)).toContainText("Nie znaleziono Claude Code ani Codexa");
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key as string, JSON.stringify(value)),
    [AI_KEY, { codex }] as const,
  );
  await page.getByRole("button", { name: "Sprawdź ponownie" }).click();
  await expect(status(page)).toContainText("Codex: wersja 0.162.1");
  await expect(status(page)).toContainText("Claude Code: nie znaleziono");
  await expect(status(page)).not.toContainText("Nie znaleziono Claude Code ani Codexa");
});

test("Ustawienia: wybrany, ale niewykryty silnik jest oznaczony", async ({ page }) => {
  await openPreview(page, { ai: { claude }, settings: { aiEngine: "codex" } });
  await openSettings(page, "Asystent AI");
  await expect(status(page)).toContainText("Wybrany silnik nie jest wykryty");
  await expect(page.getByRole("button", { name: "Przetestuj silnik" })).toBeDisabled();
});

test("Ustawienia: próba silnika pokazuje wynik i dopisuje wywołanie bez treści", async ({ page }) => {
  await openPreview(page, { ai: { claude }, settings: { aiEngine: "claude" } });
  await openSettings(page, "Asystent AI");
  await page.getByRole("button", { name: "Przetestuj silnik" }).click();
  await expect(page.getByRole("status").filter({ hasText: aiMessages.testOk })).toBeVisible();
  const calls = page.getByRole("list", { name: "Ostatnie wywołania asystenta" });
  await expect(calls.getByRole("listitem")).toHaveCount(1);
  await expect(calls).toContainText("test");
  await expect(calls).toContainText("ok");
  await expect(calls).not.toContainText("Todo Track");
  const log = (await stored(page, AI_KEY)).calls;
  expect(log).toHaveLength(1);
  expect(log[0]).toMatchObject({ kind: "test", ok: true, error: "", cached: false });
  expect(JSON.stringify(log)).not.toContain("Odpowiedz");
});

test("Ustawienia: błędy silnika po polsku trafiają do wyniku i do dziennika", async ({ page }) => {
  const script = { test: { error: "login" } };
  await openPreview(page, { ai: { claude, script }, settings: { aiEngine: "claude" } });
  await openSettings(page, "Asystent AI");
  const run = page.getByRole("button", { name: "Przetestuj silnik" });
  await run.click();
  await expect(page.getByRole("alert")).toHaveText(aiMessages.login("claude"));
  expect(aiMessages.login("claude")).toContain("uruchom `claude` w terminalu");
  const calls = page.getByRole("list", { name: "Ostatnie wywołania asystenta" });
  await expect(calls).toContainText("błąd: niezalogowany");
  for (const [error, text] of [
    ["limit", aiMessages.limit("claude")],
    ["timeout", aiMessages.timeout],
    ["schema", aiMessages.schema],
  ] as const) {
    await setScript(page, { test: { error } });
    await run.click();
    await expect(page.getByRole("alert")).toHaveText(text);
  }
  await expect(calls.getByRole("listitem")).toHaveCount(4);
  await expect(calls).toContainText("błąd: limit subskrypcji");
  await expect(calls).toContainText("błąd: przekroczony czas");
});

test("Pokaż, co zostanie wysłane: tekst po filtrze nazw, bez wywołania modelu", async ({ page }) => {
  await openPreview(page, {
    ai: { claude },
    settings: { aiEngine: "claude", aiHiddenNames: ["Todo"] },
  });
  await openSettings(page, "Asystent AI");
  const preview = page.locator("details.ai-preview");
  await expect(preview.getByText("Pokaż, co zostanie wysłane")).toBeVisible();
  await preview.locator("summary").click();
  const sent = preview.getByLabel("Tekst wysyłany do modelu");
  await expect(sent).toContainText("Test połączenia z aplikacją Osoba 1 Track.");
  await expect(sent).not.toContainText("Todo");
  // Samo otwarcie podglądu nie jest wywołaniem.
  await expect(page.getByText("Jeszcze nie było wywołań.")).toBeVisible();
  expect(aiTestPrompt).toContain("Todo Track");
});

test("opisy błędów z dziennika", () => {
  expect(aiErrorLabel("login")).toBe("niezalogowany");
  expect(aiErrorLabel("limit")).toBe("limit subskrypcji");
  expect(aiErrorLabel("timeout")).toBe("przekroczony czas");
  expect(aiErrorLabel("coś")).toBe("błąd");
  expect(aiMessages.limit("codex")).toBe(
    "Limit subskrypcji Codex jest wyczerpany. Spróbuj później.",
  );
  expect(aiMessages.login("codex")).toContain("codex login");
});
