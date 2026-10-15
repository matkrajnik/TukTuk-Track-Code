import { test, expect, type Page } from "@playwright/test";
import { ids, openPreview, seed } from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";

/**
 * Kliknięcia powiadomień Windows (etap 2 PLAN_1.3.md): Rust wykonuje akcję i emituje
 * `navigate` (`notify::View`), a główne okno ustawia widok, filtr, rozwinięcie
 * i edytor na zakładce Podziel. W podglądzie to samo robi zdarzenie okna.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const navigatePreview = (page: Page, detail: object) =>
  page.evaluate(
    (d) =>
      window.dispatchEvent(new CustomEvent("todo-track-navigate", { detail: d })),
    detail,
  );
const tasksNav = (page: Page) =>
  page
    .getByRole("navigation", { name: "Nawigacja główna" })
    .getByRole("button", { name: "Zadania", exact: true });
const expandButton = (page: Page, title: string) =>
  page.getByRole("button", { name: "Przedziały: " + title, exact: true });

test("navigate z długiego pomiaru rozwija bieżące zadanie i otwiera Podziel", async ({
  page,
}) => {
  const data = await openPreview(page, {
    active: { task: ids.analysis, from: "11:00" },
  });
  const active = data.sessions.find((s) => s.endedAt === null)!;
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(tasksNav(page)).toHaveAttribute("aria-pressed", "false");

  // Kliknięcie treści: Zadania z rozwiniętym bieżącym zadaniem; wyszukiwanie czyszczone.
  await navigatePreview(page, {
    view: "tasks",
    filter: null,
    task: ids.analysis,
    split: null,
    section: null,
    dayClose: false,
  });
  await expect(tasksNav(page)).toHaveAttribute("aria-pressed", "true");
  await expect(expandButton(page, "Analiza importu")).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  await expect(expandButton(page, "Code review")).toHaveAttribute(
    "aria-expanded",
    "false",
  );

  // „Podziel…”: edytor przedziału na zakładce Podziel, pomiar nadal trwa.
  await page.getByLabel("Szukaj zadań").fill("Code");
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await navigatePreview(page, {
    view: "tasks",
    task: ids.analysis,
    split: active.id,
  });
  const dialog = page.getByRole("dialog", { name: "Edytuj przedział" });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("tab", { name: "Podziel" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByLabel("Szukaj zadań")).toHaveValue("");
  await expect(expandButton(page, "Analiza importu")).toHaveAttribute(
    "aria-expanded",
    "true",
  );
  const stored = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("todo-track-preview-v1")!),
  );
  expect(
    stored.sessions.find((s: { id: number }) => s.id === active.id).endedAt,
  ).toBeNull();
});

test("navigate z terminu włącza filtr Dzisiaj z zaległymi terminami", async ({
  page,
}) => {
  const data = seed();
  const review = data.tasks.find((t) => t.id === ids.review)!;
  review.details = { ...review.details, dueDay: "2026-10-14" };
  await openPreview(page, { data });
  const today = page.getByLabel("Dzisiaj: plan i praca");
  await expect(today).not.toBeChecked();
  await navigatePreview(page, { view: "tasks", filter: "today" });
  await expect(today).toBeChecked();
  // Termin wczoraj (bez planu i pracy dziś) jest widoczny, plan z wczoraj nie.
  await expect(expandButton(page, "Code review")).toBeVisible();
  await expect(expandButton(page, "Analiza importu")).toBeVisible();
  await expect(expandButton(page, "Wdrożenie etykiet")).toHaveCount(0);
});

test("navigate z wieczoru i celu otwiera Zamknięcie dnia na dziś", async ({
  page,
}) => {
  await openPreview(page);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await navigatePreview(page, { view: "journal", dayClose: true });
  await expect(
    page.getByRole("heading", { name: /^Zamknięcie dnia · / }),
  ).toBeVisible();
  // Zwykłe wejście do Planu i historii później nie otwiera Zamknięcia dnia samo.
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: /^Zamknięcie dnia · / }),
  ).toHaveCount(0);
});

test("navigate z backendu (Jira) przewija do nowego zadania bez zmiany danych", async ({
  page,
}) => {
  await openDesktop(page);
  await expect(expandButton(page, "Code review")).toBeVisible();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await page.evaluate(() =>
    (window as any).__tauriMock.emit("navigate", {
      view: "tasks",
      filter: null,
      task: 3,
      split: null,
      section: "Nierozpoczęte",
      dayClose: false,
    }),
  );
  await expect(tasksNav(page)).toHaveAttribute("aria-pressed", "true");
  await expect(
    page
      .getByRole("region", { name: "Nierozpoczęte", exact: true })
      .getByRole("button", { name: "Przedziały: Code review", exact: true }),
  ).toHaveAttribute("aria-expanded", "true");
  // Nieznany widok (np. tylko pokazanie okna) niczego nie zmienia.
  await page.evaluate(() =>
    (window as any).__tauriMock.emit("navigate", { view: null }),
  );
  await expect(tasksNav(page)).toHaveAttribute("aria-pressed", "true");
  const calls: { cmd: string }[] = await page.evaluate(() =>
    (window as any).__tauriMock.calls(),
  );
  expect(
    calls.filter((c) => c.cmd === "mutate"),
  ).toEqual([]);
});
