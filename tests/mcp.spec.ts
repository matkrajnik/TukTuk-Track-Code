import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures/test";
import { AI_KEY, ids, openPreview, stored, TODAY } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Serwer MCP (PLAN_BACKLOG.md, A2) w podglądzie: baner propozycji agentów
 * (`src/components/AiProposals.vue`) na symulowanej kolejce (`src/preview/mcp.ts`)
 * i stan rejestracji w Ustawieniach → Asystent AI. „Zastosuj” wykonuje zmianę
 * istniejącymi komendami, „Odrzuć” niczego nie zmienia. Protokół, narzędzia i
 * rejestrację w CLI sprawdzają testy Rust; żaden test nie uruchamia claude ani codex.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const MCP_KEY = "todo-track-preview-mcp-v1";
const BINDINGS_KEY = "todo-track-preview-agent-bindings-v1";
const EVENT = "todo-track-mcp-proposals";

const base = {
  at: 0,
  taskId: null,
  taskTitle: "",
  title: "",
  project: "",
  key: "",
  day: "",
  estimate: 0,
  text: "",
  folder: "",
};
const create = {
  ...base,
  id: 1,
  kind: "task_create",
  title: "Poprawić raport błędów",
  project: "WMS",
  day: TODAY,
  estimate: 45,
};
const note = (day: string) => ({
  ...base,
  id: 2,
  kind: "note_append",
  taskId: ids.analysis,
  taskTitle: "Analiza importu",
  day,
  text: "Parser gotowy.",
});
const plan = {
  ...base,
  id: 3,
  kind: "plan",
  taskId: ids.review,
  taskTitle: "Code review",
  day: "2026-10-16",
};
const switchTo = {
  ...base,
  id: 4,
  kind: "switch",
  taskId: ids.labels,
  taskTitle: "Wdrożenie etykiet",
};
const link = {
  ...base,
  id: 5,
  kind: "link_folder",
  taskId: ids.review,
  taskTitle: "Code review",
  folder: "C:\\Projekty\\api",
};

const banner = (page: Page) =>
  page.getByRole("region", { name: "Propozycje agentów" });
const queue = async (page: Page): Promise<{ id: number }[]> =>
  (await page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), MCP_KEY))
    .proposals ?? [];

/** Podgląd z kolejką propozycji, jakby agent wywołał `propose_*`. */
async function open(
  page: Page,
  proposals: unknown[],
  options: Parameters<typeof openPreview>[1] = {},
) {
  await openPreview(page, options);
  await page.evaluate(
    ([k, v]) => localStorage.setItem(k as string, JSON.stringify(v)),
    [MCP_KEY, { proposals }] as const,
  );
  await page.reload();
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
}

test("bez propozycji nie ma banera", async ({ page }) => {
  await open(page, []);
  await expect(banner(page)).toHaveCount(0);
});

test("baner opisuje propozycje po polsku i niczego nie zmienia, dopóki nie klikniesz", async ({ page }) => {
  await open(page, [create, note(TODAY), plan, switchTo, link]);
  const before = await stored(page);
  await expect(banner(page)).toBeVisible();
  await expect(banner(page)).toContainText("Agenci proponują zmiany (5)");
  await expect(banner(page)).toContainText("Nic się nie zmieni, dopóki nie klikniesz „Zastosuj”.");
  await expect(banner(page)).toContainText(
    `Utworzyć zadanie „Poprawić raport błędów”, w projekcie WMS, zaplanowane na ${TODAY}, z szacunkiem 45 min?`,
  );
  await expect(banner(page)).toContainText(
    `Dopisać do notatki dnia ${TODAY} zadania „Analiza importu”: „Parser gotowy.”?`,
  );
  await expect(banner(page)).toContainText("Zaplanować „Code review” na 2026-10-16?");
  await expect(banner(page)).toContainText("Przełączyć pomiar na „Wdrożenie etykiet”?");
  await expect(banner(page)).toContainText(
    "Przypisać folder C:\\Projekty\\api do zadania „Code review”?",
  );
  await page.waitForTimeout(300);
  expect(await stored(page)).toEqual(before);
});

test("Zastosuj tworzy zadanie z polami planu i zdejmuje propozycję", async ({ page }) => {
  await open(page, [create]);
  await expect(banner(page)).toContainText("Agent proponuje zmianę");
  const before = await stored(page);
  expect(before.tasks.some((t: any) => t.title === "Poprawić raport błędów")).toBe(false);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Poprawić raport błędów" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const data = await stored(page);
  const task = data.tasks.find((t: any) => t.title === "Poprawić raport błędów");
  expect(task).toBeTruthy();
  expect(task.details).toMatchObject({
    project: "WMS",
    plannedDay: TODAY,
    estimateMinutes: 45,
  });
  expect(data.tasks.length).toBe(before.tasks.length + 1);
  // Zadanie widać na liście, a kolejka jest pusta (po przeładowaniu też).
  await expect(page.getByText("Poprawić raport błędów").first()).toBeVisible();
  expect(await queue(page)).toEqual([]);
  await page.reload();
  await expect(banner(page)).toHaveCount(0);
});

test("Odrzuć nic nie zmienia i zdejmuje propozycję", async ({ page }) => {
  await open(page, [create, note(TODAY)]);
  const before = await stored(page);
  await banner(page)
    .getByRole("button", { name: "Odrzuć propozycję: Poprawić raport błędów" })
    .click();
  await expect(banner(page)).toContainText("Agent proponuje zmianę");
  expect((await queue(page)).map((p) => p.id)).toEqual([2]);
  await banner(page)
    .getByRole("button", { name: "Odrzuć propozycję: Analiza importu" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  expect(await queue(page)).toEqual([]);
  expect(await stored(page)).toEqual(before);
});

test("Zastosuj dopisuje zdanie do notatki dnia, także do istniejącej", async ({ page }) => {
  await open(page, [note("2026-10-14")]);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Analiza importu" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const analysis = (await stored(page)).tasks.find((t: any) => t.id === ids.analysis);
  expect(analysis.details.notes["2026-10-14"]).toBe("Mapowanie kolumn\nParser gotowy.");
  // Inne dni zostają.
  expect(analysis.details.notes["2026-10-13"]).toBe("Walidacja CSV");
  // Nowy dzień: sama treść.
  await open(page, [note(TODAY)]);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Analiza importu" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const next = (await stored(page)).tasks.find((t: any) => t.id === ids.analysis);
  expect(next.details.notes[TODAY]).toBe("Parser gotowy.");
});

test("Zastosuj planuje zadanie na dzień, przypisuje folder i przełącza pomiar", async ({ page }) => {
  await open(page, [plan, link, switchTo]);
  // Timer nigdy nie przełącza się sam: bez kliknięcia nie ma aktywnego pomiaru.
  await page.waitForTimeout(300);
  expect((await stored(page)).sessions.some((s: any) => s.endedAt === null)).toBe(false);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Code review" })
    .first()
    .click();
  await expect(banner(page)).toContainText("Agenci proponują zmiany (2)");
  expect(
    (await stored(page)).tasks.find((t: any) => t.id === ids.review).details.plannedDay,
  ).toBe("2026-10-16");
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Code review" })
    .click();
  await expect(banner(page)).toContainText("Agent proponuje zmianę");
  const bindings = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "[]"),
    BINDINGS_KEY,
  );
  expect(bindings).toEqual([{ folder: "C:\\Projekty\\api", taskId: ids.review }]);
  expect((await stored(page)).sessions.some((s: any) => s.endedAt === null)).toBe(false);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Wdrożenie etykiet" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const active = (await stored(page)).sessions.find((s: any) => s.endedAt === null);
  expect(active?.taskId).toBe(ids.labels);
});

test("błąd przy Zastosuj pokazuje komunikat i zostawia propozycję", async ({ page }) => {
  await open(page, [{ ...note(TODAY), taskId: 999 }]);
  const before = await stored(page);
  await banner(page)
    .getByRole("button", { name: "Zastosuj propozycję: Analiza importu" })
    .click();
  await expect(page.getByRole("alert").filter({ hasText: "Nie znaleziono zadania." })).toBeVisible();
  await expect(banner(page)).toBeVisible();
  expect((await queue(page)).length).toBe(1);
  expect(await stored(page)).toEqual(before);
});

test("propozycja dopisana przez agenta w trakcie pracy pojawia się od razu", async ({ page }) => {
  await open(page, []);
  await expect(banner(page)).toHaveCount(0);
  await page.evaluate(
    ([k, e, p]) => {
      localStorage.setItem(k as string, JSON.stringify({ proposals: [p] }));
      window.dispatchEvent(new Event(e as string));
    },
    [MCP_KEY, EVENT, create] as const,
  );
  await expect(banner(page)).toContainText("Agent proponuje zmianę");
});

// ---------------------------------------------------------------------------
// Ustawienia → Asystent AI: stan rejestracji

const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };
const codex = { path: "C:\\Users\\ala\\AppData\\Roaming\\npm\\codex.cmd", version: "0.162.1" };
const mcpStatus = (page: Page) => page.getByRole("status", { name: "Stan serwera MCP" });

test("Ustawienia pokazują rejestrację serwera MCP w wykrytych CLI", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex }, settings: { aiEngine: "claude" } });
  await openSettings(page, "Asystent AI");
  await expect(mcpStatus(page)).toContainText("Claude Code: zarejestrowany");
  await expect(mcpStatus(page)).toContainText("Codex: zarejestrowany");
  await page.getByRole("button", { name: "Zarejestruj ponownie" }).click();
  await expect(mcpStatus(page)).toContainText("Claude Code: zarejestrowany");
});

test("brak jednego CLI, wyłączony przełącznik i brak wyboru silnika", async ({ page }) => {
  await openPreview(page, { ai: { claude, codex: null }, settings: { aiEngine: "claude" } });
  await openSettings(page, "Asystent AI");
  await expect(mcpStatus(page)).toContainText("Codex: nie znaleziono");
  // Wyłączenie przełącznika: wpisy znikają z CLI (stan po synchronizacji).
  await page.getByLabel("Serwer MCP dla agentów").uncheck();
  await expect(mcpStatus(page)).toContainText("Claude Code: niezarejestrowany");
  await expect(mcpStatus(page)).toContainText("zarejestruje się po wybraniu silnika i włączeniu przełącznika");
  // Silnik jeszcze niewybrany (dialog pierwszego uruchomienia zamknięty bez wyboru): nic się nie rejestruje.
  await openPreview(page, { ai: { claude, codex } });
  await page.getByRole("dialog", { name: "Asystent AI" }).getByRole("button", { name: "Później" }).click();
  await openSettings(page, "Asystent AI");
  await expect(mcpStatus(page)).toContainText("zarejestruje się po wybraniu silnika i włączeniu przełącznika");
  await expect(mcpStatus(page)).not.toContainText("zarejestrowany");
});

test("brak todo-track-mcp.exe obok aplikacji jest widoczny w Ustawieniach", async ({ page }) => {
  await openPreview(page, { ai: { claude }, settings: { aiEngine: "claude" } });
  await page.evaluate(
    ([k, v]) => localStorage.setItem(k as string, JSON.stringify(v)),
    [MCP_KEY, { exeFound: false }] as const,
  );
  await page.reload();
  await openSettings(page, "Asystent AI");
  await expect(mcpStatus(page)).toContainText("Brak programu todo-track-mcp.exe obok aplikacji");
  await expect(mcpStatus(page)).toContainText("Claude Code: błąd rejestracji");
  // Stan symulowanego silnika nie jest nadpisany (ten sam klucz AI_KEY).
  expect(await page.evaluate((k) => localStorage.getItem(k), AI_KEY)).toContain("claude");
});
