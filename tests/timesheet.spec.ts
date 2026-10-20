import { test, expect, type Page } from "@playwright/test";
import { resolveTsTask } from "../src/timesheet";
import { previewTsKey, type Task, type TsProjectMapping } from "../src/store";
import { ids, openPreview, stored } from "./fixtures/seed";

/**
 * Wysyłka Raportu TS do Ideo Timesheet (PLAN_1.8.md) na symulowanym TS
 * podglądu (`store.ts`, klucz `todo-track-preview-ts-v1`). Żaden test nie łączy
 * się z prawdziwym serwerem TS.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const task = (details: Partial<Task["details"]> & object) =>
  ({ details: { project: "", ...details } }) as Pick<Task, "details">;
const mapping = (project: string, tsTaskId: string, tsTaskName: string) => ({
  project,
  tsTaskId,
  tsTaskName,
});

test("resolveTsTask: jawny wybór, mapowanie projektu (bez wielkości liter), brak", () => {
  const map: TsProjectMapping[] = [
    mapping("WMS", "1101", "Rozwój"),
    mapping("Zespół", "2201", "Spotkania"),
  ];
  // Jawny wybór zadania wygrywa z mapowaniem projektu.
  expect(
    resolveTsTask(
      task({ project: "WMS", tsTaskId: "1103", tsTaskName: "Code review" }),
      map,
    ),
  ).toEqual({ id: "1103", name: "Code review", source: "task" });
  // Bez nazwy pokazuje się identyfikator.
  expect(resolveTsTask(task({ tsTaskId: " 77 " }), [])).toEqual({
    id: "77",
    name: "77",
    source: "task",
  });
  // Mapowanie projektu, nazwa projektu bez wielkości liter i białych znaków.
  expect(resolveTsTask(task({ project: "WMS" }), map)).toEqual({
    id: "1101",
    name: "Rozwój",
    source: "project",
  });
  expect(resolveTsTask(task({ project: " wms " }), map)?.id).toBe("1101");
  // Jak SQLite NOCASE: tylko litery ASCII (Ł ≠ ł).
  expect(resolveTsTask(task({ project: "ZESPÓŁ" }), map)).toBeNull();
  expect(resolveTsTask(task({ project: "Zespół" }), map)?.id).toBe("2201");
  // Puste id zadania = brak jawnego wyboru.
  expect(resolveTsTask(task({ project: "WMS", tsTaskId: "  " }), map)?.source).toBe(
    "project",
  );
  // Brak: bez projektu, projekt bez mapowania, zadanie bez szczegółów.
  expect(resolveTsTask(task({}), map)).toBeNull();
  expect(resolveTsTask(task({ project: "Foundation" }), map)).toBeNull();
  expect(resolveTsTask({ details: undefined } as never, map)).toBeNull();
});

const card = (page: Page, title: string) =>
  page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: title }) });
const expand = (page: Page, title: string) =>
  page.getByRole("button", { name: "Przedziały: " + title }).click();
async function setTs(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([key, value]) => localStorage.setItem(key, value),
    [previewTsKey, JSON.stringify(state)] as const,
  );
  await page.reload();
}
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);
const detailsOf = async (page: Page, id: number) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === id).details;

test("pole „Zadanie TS” w kafelku: wybór z listy, zapamiętanie i stan bez połączenia", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  await expand(page, "Code review");
  const select = page.getByRole("combobox", { name: "Zadanie TS: Code review" });
  await expect(select).toHaveValue("");
  // Grupy per projekt TS i opcja „— brak —”.
  await expect(select.locator("optgroup")).toHaveCount(2);
  await expect(select.locator("optgroup").first()).toHaveAttribute("label", "WMS");
  await expect(select.locator("option").first()).toHaveText("— brak —");
  await select.selectOption("1103");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText(
    "TS: Code review",
  );
  const saved = await detailsOf(page, ids.review);
  expect([saved.tsTaskId, saved.tsTaskName]).toEqual(["1103", "Code review"]);
  // Pozostałe szczegóły bez zmian.
  expect([saved.project, saved.estimateMinutes]).toEqual(["Foundation", 60]);

  // Po przeładowaniu wybór zostaje.
  await page.reload();
  await expand(page, "Code review");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toHaveValue("1103");

  // Zadanie zniknęło z listy TS: opcja „(niedostępne)” z ostrzeżeniem.
  await setTs(page, {
    connected: true,
    login: "jan",
    tasks: [{ name: "WMS", tasks: [{ id: "1101", name: "Rozwój" }] }],
  });
  await expand(page, "Code review");
  const gone = page.getByRole("combobox", { name: "Zadanie TS: Code review" });
  await expect(gone).toHaveValue("1103");
  await expect(gone.locator("option:checked")).toHaveText("(niedostępne) Code review");
  await expect(
    page
      .getByRole("region", { name: "Zadanie TS: Code review" })
      .getByRole("alert"),
  ).toContainText(
    "Zadania TS „Code review” nie ma na dzisiejszej liście",
  );
  // „Odśwież” pobiera listę ponownie (pamięć podręczna na czas działania).
  await page.evaluate((k) => {
    const s = JSON.parse(localStorage.getItem(k)!);
    delete s.tasks;
    localStorage.setItem(k, JSON.stringify(s));
  }, previewTsKey);
  await expect(gone.locator("optgroup")).toHaveCount(1);
  await page.getByRole("button", { name: "Odśwież listę zadań TS" }).click();
  await expect(gone.locator("optgroup")).toHaveCount(2);
  await expect(gone.locator("option:checked")).toHaveText("Code review");

  // „— brak —” usuwa wybór z zadania.
  await gone.selectOption("");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);
  const cleared = await detailsOf(page, ids.review);
  expect("tsTaskId" in cleared || "tsTaskName" in cleared).toBe(false);
  await gone.selectOption("1102");
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText(
    "TS: Błędy i wsparcie",
  );

  // Bez połączenia: zapamiętana nazwa i link do Ustawień.
  await setTs(page, { connected: false, login: "jan" });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await expect(field).toContainText("Zadanie TS: Błędy i wsparcie");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toHaveCount(0);
  await field.getByRole("button", { name: "Połącz konto TS w Ustawieniach" }).click();
  await expect(page.getByRole("searchbox", { name: "Szukaj ustawienia" })).toBeVisible();
});

test("„Ustaw jako domyślne dla projektu”: mapowanie projektu działa na zadania bez wyboru", async ({
  page,
}) => {
  await openPreview(page);
  await setTs(page, { connected: true, login: "jan" });
  await expand(page, "Wdrożenie etykiet");
  const select = page.getByRole("combobox", { name: "Zadanie TS: Wdrożenie etykiet" });
  const asDefault = page.getByRole("checkbox", {
    name: "Ustaw jako domyślne dla projektu „WMS”",
  });
  // Pole wyboru pojawia się dopiero po wybraniu zadania TS.
  await expect(asDefault).toHaveCount(0);
  await select.selectOption("1101");
  await expect(asDefault).not.toBeChecked();
  await asDefault.check();
  await expect.poll(async () => (await tsState(page)).projectMap).toEqual([
    { project: "WMS", tsTaskId: "1101", tsTaskName: "Rozwój" },
  ]);
  // Drugie zadanie projektu WMS (bez jawnego wyboru) dziedziczy mapowanie.
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Rozwój (z projektu)",
  );
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Rozwój",
  );
  // Mapowanie nie jest kopiowane do szczegółów zadania.
  expect((await detailsOf(page, ids.analysis)).tsTaskId).toBeUndefined();
  // Zadanie z innego projektu nie ma chipa.
  await expect(card(page, "Code review").locator(".task-ts")).toHaveCount(0);

  // Po przeładowaniu pole wyboru jest zaznaczone; zmiana wyboru przy
  // zaznaczonym polu przenosi też domyślne zadanie projektu.
  await page.reload();
  await expand(page, "Wdrożenie etykiet");
  await expect(asDefault).toBeChecked();
  await select.selectOption("1103");
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveText(
    "TS: Code review (z projektu)",
  );
  // Rozwinięte zadanie bez jawnego wyboru pokazuje domyślne z projektu.
  await expand(page, "Analiza importu");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Analiza importu" }),
  ).toHaveValue("");
  await expect(
    page.getByRole("region", { name: "Zadanie TS: Analiza importu" }),
  ).toContainText("Domyślnie z projektu „WMS”: Code review");

  // Odznaczenie usuwa mapowanie projektu; jawny wybór zadania zostaje.
  await asDefault.uncheck();
  await expect.poll(async () => (await tsState(page)).projectMap).toEqual([]);
  await expect(card(page, "Analiza importu").locator(".task-ts")).toHaveCount(0);
  await expect(card(page, "Wdrożenie etykiet").locator(".task-ts")).toHaveText(
    "TS: Code review",
  );
});
