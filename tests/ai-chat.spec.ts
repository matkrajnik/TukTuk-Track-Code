import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures/test";
import { ids, openPreview, stored, TODAY } from "./fixtures/seed";
import { quickText } from "../src/api/aichat";

/**
 * Czat w aplikacji (PLAN_BACKLOG.md, A3) na symulowanym asystencie
 * (`src/preview/ai.ts`): panel po prawej z Ctrl+J, pytanie i odpowiedź, karty
 * akcji z propozycji MCP („Zastosuj”, „Odrzuć”, „Popraw”), kontekst z kafelka,
 * Zamknięcia dnia i Ctrl+K, „Pokaż, co zostało wysłane” po filtrze nazw,
 * „Zatrzymaj” i „Nowa rozmowa”. Parser strumienia i zarządzanie procesem
 * sprawdzają testy Rust; żaden test nie uruchamia claude ani codex.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const MCP_KEY = "todo-track-preview-mcp-v1";
const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };

const panel = (page: Page) => page.getByRole("complementary", { name: "Asystent AI" });
const input = (page: Page) => page.getByRole("textbox", { name: "Wiadomość do asystenta" });
const navItem = (page: Page) => page.getByRole("button", { name: "Asystent", exact: true });
const answers = (page: Page) => panel(page).getByRole("article", { name: "Odpowiedź asystenta" });

/** Podgląd z wybranym i wykrytym Claude Code. */
async function open(
  page: Page,
  options: Parameters<typeof openPreview>[1] = {},
) {
  await openPreview(page, {
    ai: { claude },
    ...options,
    settings: { aiEngine: "claude", ...options.settings },
  });
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
}

async function ask(page: Page, text: string) {
  await input(page).fill(text);
  await input(page).press("Enter");
}

const queue = async (page: Page): Promise<{ id: number }[]> =>
  (await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), MCP_KEY))
    .proposals ?? [];

test("panel jest ukryty bez wybranego silnika, przy „Nie używaj” i przy wyłączonym czacie", async ({ page }) => {
  // Domyślnie silnik nie jest wybrany: ani pozycji w nawigacji, ani skrótu.
  await openPreview(page, { ai: { claude } });
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await expect(navItem(page)).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toHaveCount(0);
  await openPreview(page, { ai: { claude }, settings: { aiEngine: "off" } });
  await expect(navItem(page)).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toHaveCount(0);
  await open(page, { settings: { aiChat: false } });
  await expect(navItem(page)).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toHaveCount(0);
  // Zadanie rozwinięte nie ma przycisku „Zapytaj asystenta”.
  await page.getByRole("button", { name: "Przedziały: Analiza importu" }).click();
  await expect(page.getByRole("button", { name: /^Zapytaj asystenta/ })).toHaveCount(0);
});

test("Ctrl+J i pozycja w nawigacji pokazują i chowają panel; stan i szerokość są zapamiętane", async ({ page }) => {
  await open(page);
  await expect(navItem(page)).toBeVisible();
  await expect(panel(page)).toHaveCount(0);
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toBeVisible();
  await expect(input(page)).toBeFocused();
  await expect(navItem(page)).toHaveAttribute("aria-pressed", "true");
  await expect(panel(page)).toContainText("niczego nie zapisze bez Twojego kliknięcia");
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toHaveCount(0);
  await navItem(page).click();
  await expect(panel(page)).toBeVisible();
  // Szerokość: strzałki na uchwycie, zapamiętana po przeładowaniu.
  const handle = panel(page).getByRole("separator", { name: "Szerokość asystenta" });
  const before = Number(await handle.getAttribute("aria-valuenow"));
  await handle.focus();
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press("ArrowLeft");
  await expect(handle).toHaveAttribute("aria-valuenow", String(before + 40));
  const width = (await panel(page).boundingBox())!.width;
  expect(Math.round(width)).toBe(before + 40);
  await page.reload();
  await expect(panel(page)).toBeVisible();
  await expect(
    panel(page).getByRole("separator", { name: "Szerokość asystenta" }),
  ).toHaveAttribute("aria-valuenow", String(before + 40));
  await panel(page).getByRole("button", { name: "Schowaj asystenta" }).click();
  await expect(panel(page)).toHaveCount(0);
  await page.reload();
  await expect(panel(page)).toHaveCount(0);
});

test("pytanie dostaje odpowiedź z symulatora i listę użytych narzędzi", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+j");
  await ask(page, "Podsumuj mój dzień");
  await expect(panel(page).getByRole("article", { name: "Twoja wiadomość" })).toContainText(
    "Podsumuj mój dzień",
  );
  await expect(answers(page)).toContainText("Dziś pracowałeś nad kilkoma zadaniami.");
  await expect(answers(page).getByRole("list", { name: "Użyte narzędzia" })).toContainText(
    `przeczytał: dzień ${TODAY}`,
  );
  await expect(input(page)).toHaveValue("");
  // Odpowiedź skryptowana w stanie symulatora ma pierwszeństwo.
  await open(page, {
    ai: {
      claude,
      chat: {
        replies: [
          {
            match: "tydzień",
            text: "Tydzień był spokojny.",
            tools: [{ name: "week_summary", input: { week: "2026-10-12" } }],
          },
        ],
      },
    },
  });
  await page.keyboard.press("Control+j");
  await ask(page, "Jak minął tydzień?");
  await expect(answers(page)).toContainText("Tydzień był spokojny.");
  await expect(answers(page)).toContainText("przeczytał: tydzień 2026-10-12");
});

test("karta „Utwórz zadanie”: Zastosuj tworzy zadanie istniejącą komendą", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+j");
  const before = await stored(page);
  await ask(page, "Utwórz zadanie „Poprawić raport błędów”");
  const card = panel(page).getByRole("region", { name: "Propozycja: Poprawić raport błędów" });
  await expect(card).toBeVisible();
  await expect(card.getByRole("heading", { name: "Utwórz zadanie" })).toBeVisible();
  await expect(card).toContainText("Utworzyć zadanie „Poprawić raport błędów”");
  await expect(answers(page)).toContainText("zaproponował: zadanie „Poprawić raport błędów”");
  // Baner agentów nie dubluje propozycji z czatu, a samo pytanie niczego nie zapisało.
  await expect(page.getByRole("region", { name: "Propozycje agentów" })).toHaveCount(0);
  expect((await stored(page)).tasks.length).toBe(before.tasks.length);
  await card
    .getByRole("button", { name: "Zastosuj propozycję: Poprawić raport błędów" })
    .click();
  await expect(card).toContainText("Zastosowano.");
  const data = await stored(page);
  expect(data.tasks.length).toBe(before.tasks.length + 1);
  const task = data.tasks.find((t: { title: string }) => t.title === "Poprawić raport błędów");
  expect(task).toBeTruthy();
  expect(task.details).toMatchObject({ plannedDay: TODAY });
  // Kolejka jest pusta, a zadanie widać na liście.
  expect(await queue(page)).toEqual([]);
  await expect(page.getByRole("button", { name: "Przedziały: Poprawić raport błędów" })).toBeVisible();
});

test("karta „Odrzuć” nic nie zmienia i zdejmuje propozycję z kolejki", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+j");
  await ask(page, "Utwórz zadanie „Zadanie do odrzucenia”");
  const card = panel(page).getByRole("region", { name: "Propozycja: Zadanie do odrzucenia" });
  await expect(card).toBeVisible();
  const before = await stored(page);
  await card.getByRole("button", { name: "Odrzuć propozycję: Zadanie do odrzucenia" }).click();
  await expect(card).toContainText("Odrzucono, nic się nie zmieniło.");
  await expect(card.getByRole("button", { name: /^Zastosuj/ })).toHaveCount(0);
  await page.waitForTimeout(200);
  expect(await stored(page)).toEqual(before);
  expect(await queue(page)).toEqual([]);
  await expect(page.getByRole("region", { name: "Propozycje agentów" })).toHaveCount(0);
});

test("karta „Popraw” otwiera zwykły formularz z wypełnionymi polami", async ({ page }) => {
  await open(page, {
    ai: {
      claude,
      chat: {
        replies: [
          {
            match: "raport",
            text: "Proponuję takie zadanie.",
            tools: [{ name: "propose_task_create", input: { title: "Raport kwartalny" } }],
            proposal: {
              kind: "task_create",
              title: "Raport kwartalny",
              project: "WMS",
              day: "2026-10-20",
              estimate: 90,
            },
          },
        ],
      },
    },
  });
  await page.keyboard.press("Control+j");
  const before = await stored(page);
  await ask(page, "Dodaj raport");
  const card = panel(page).getByRole("region", { name: "Propozycja: Raport kwartalny" });
  await card.getByRole("button", { name: "Popraw propozycję: Raport kwartalny" }).click();
  await expect(card).toContainText("Otwarto do poprawy w zwykłym formularzu.");
  await expect(page.getByRole("textbox", { name: "Nowe zadanie" })).toHaveValue(
    "Raport kwartalny ~1h30 @20.10 #WMS",
  );
  await expect(page.getByRole("textbox", { name: "Nowe zadanie" })).toBeFocused();
  // Nic nie zapisano, a propozycja zeszła z kolejki.
  expect(await stored(page)).toEqual(before);
  expect(await queue(page)).toEqual([]);
});

test("składnia „Popraw” zgadza się ze składnią szybkiego dodawania", () => {
  const base = {
    id: 1,
    kind: "task_create" as const,
    at: 0,
    taskId: null,
    taskTitle: "",
    title: "Naprawić #123",
    project: "",
    key: "WMS-12",
    day: "",
    estimate: 45,
    text: "",
    folder: "",
  };
  expect(quickText(base)).toBe("WMS-12 Naprawić \\#123 ~45m");
  expect(quickText({ ...base, key: "", estimate: 0, title: "@dom" })).toBe("\\@dom");
});

test("zadanie z kafelka daje kontekst, a „Pokaż, co zostało wysłane” pokazuje tekst po filtrze nazw", async ({ page }) => {
  await open(page, { settings: { aiHiddenNames: ["Jan Kowalski"] } });
  await page.getByRole("button", { name: "Przedziały: Analiza importu" }).click();
  await page.getByRole("button", { name: "Zapytaj asystenta: Analiza importu" }).click();
  await expect(panel(page)).toBeVisible();
  await expect(panel(page)).toContainText("Zadanie: Analiza importu");
  await expect(input(page)).toBeFocused();
  await ask(page, "Co zostało do zrobienia dla Kowalskiego?");
  await expect(answers(page)).toContainText("Zadanie „Analiza importu”");
  await expect(answers(page)).toContainText("przeczytał: zadanie 1");
  const sent = panel(page).getByRole("article", { name: "Twoja wiadomość" });
  await expect(sent).toContainText("Zadanie: Analiza importu");
  await sent.getByText("Pokaż, co zostało wysłane").click();
  const text = sent.getByLabel("Tekst wysłany do modelu");
  await expect(text).toContainText("Kontekst: zadanie „Analiza importu” (id 1");
  // Nazwisko nie opuszcza aplikacji: model dostaje „Osoba 1”.
  await expect(text).toContainText("Co zostało do zrobienia dla Osoba 1?");
  await expect(text).not.toContainText("Kowalski");
  // Kontekst można usunąć.
  await panel(page).getByRole("button", { name: "Usuń kontekst rozmowy" }).click();
  await expect(panel(page).getByText("Zadanie: Analiza importu").first()).toBeVisible();
  await expect(panel(page).locator(".chat-context")).toHaveCount(0);
});

test("karta notatki z kontekstu zadania dopisuje notatkę dnia po „Zastosuj”", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Przedziały: Analiza importu" }).click();
  await page.getByRole("button", { name: "Zapytaj asystenta: Analiza importu" }).click();
  await ask(page, "Dopisz notatkę o postępie");
  const card = panel(page).getByRole("region", { name: "Propozycja: Analiza importu" });
  await expect(card.getByRole("heading", { name: "Dopisz notatkę" })).toBeVisible();
  await card.getByRole("button", { name: "Zastosuj propozycję: Analiza importu" }).click();
  await expect(card).toContainText("Zastosowano.");
  const task = (await stored(page)).tasks.find((t: { id: number }) => t.id === ids.analysis);
  expect(task.details.notes[TODAY]).toContain("Podsumowanie od asystenta.");
});

test("Zamknięcie dnia otwiera czat z dniem jako kontekstem", async ({ page }) => {
  await open(page);
  await page.getByRole("button", { name: "Plan i historia", exact: true }).click();
  await page.getByRole("button", { name: "Zamknięcie dnia", exact: true }).click();
  await page.getByRole("button", { name: "Zapytaj asystenta o ten dzień" }).click();
  await expect(panel(page)).toContainText(/Dzień \d{4}-\d{2}-\d{2}/);
  await ask(page, "Czego brakuje w tym dniu?");
  await expect(answers(page)).toContainText("Nie mam skryptowanej odpowiedzi");
  const sent = panel(page).getByRole("article", { name: "Twoja wiadomość" });
  await sent.getByText("Pokaż, co zostało wysłane").click();
  await expect(sent.getByLabel("Tekst wysłany do modelu")).toContainText("Kontekst: dzień");
});

test("Ctrl+K: tekst po „?” idzie do czatu zamiast do listy zadań", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+k");
  const quick = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await quick.getByRole("textbox").fill("? podsumuj dzień");
  await expect(quick.getByRole("button", { name: /Zapytaj asystenta: podsumuj dzień/ })).toBeVisible();
  // Bez tworzenia zadania i bez listy zadań.
  await expect(quick.getByText(/Utwórz/)).toHaveCount(0);
  await quick.getByRole("textbox").press("Enter");
  await expect(quick).toHaveCount(0);
  await expect(panel(page)).toBeVisible();
  await expect(panel(page).getByRole("article", { name: "Twoja wiadomość" })).toContainText(
    "podsumuj dzień",
  );
  await expect(answers(page)).toContainText("Dziś pracowałeś");
});

test("Ctrl+K bez czatu traktuje „?” jak zwykły tekst wyszukiwania", async ({ page }) => {
  await openPreview(page, { ai: { claude } });
  await page.keyboard.press("Control+k");
  const quick = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  await quick.getByRole("textbox").fill("? coś");
  await expect(quick.getByRole("button", { name: /Zapytaj asystenta/ })).toHaveCount(0);
});

test("Zatrzymaj przerywa odpowiedź, a Nowa rozmowa czyści rozmowę", async ({ page }) => {
  await open(page, {
    ai: {
      claude,
      chat: { stepMs: 700 },
    },
  });
  await page.keyboard.press("Control+j");
  await ask(page, "Podsumuj dzień");
  const stop = panel(page).getByRole("button", { name: "Zatrzymaj" });
  await expect(stop).toBeVisible();
  await expect(panel(page).getByRole("button", { name: "Wyślij" })).toHaveCount(0);
  await stop.click();
  await expect(answers(page)).toContainText("Zatrzymano.");
  await expect(panel(page).getByRole("button", { name: "Wyślij" })).toBeVisible();
  // Po zatrzymaniu spóźniona odpowiedź nie dopisuje się do rozmowy.
  await page.waitForTimeout(1600);
  await expect(answers(page)).not.toContainText("Dziś pracowałeś");
  await panel(page).getByRole("button", { name: "Nowa rozmowa" }).click();
  await expect(panel(page).getByRole("article")).toHaveCount(0);
  await expect(panel(page)).toContainText("Zapytaj o swój dzień");
});

test("błąd silnika pokazuje komunikat po polsku, a pole zostaje do użycia", async ({ page }) => {
  // Serwer MCP wyłączony: czat czyta dane przez MCP.
  await open(page, { settings: { aiMcp: false } });
  await page.keyboard.press("Control+j");
  await ask(page, "Co dziś?");
  await expect(answers(page).getByRole("alert")).toContainText(
    "Czat czyta dane przez serwer MCP.",
  );
  await expect(panel(page).getByRole("button", { name: "Wyślij" })).toBeDisabled();
  await input(page).fill("Jeszcze raz");
  await expect(panel(page).getByRole("button", { name: "Wyślij" })).toBeEnabled();
});

test("panel na wąskim oknie nie wychodzi poza ekran", async ({ page }) => {
  await page.setViewportSize({ width: 760, height: 800 });
  await open(page);
  await page.keyboard.press("Control+j");
  await expect(panel(page)).toBeVisible();
  const box = (await panel(page).boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(760);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
  ).toBe(true);
});
