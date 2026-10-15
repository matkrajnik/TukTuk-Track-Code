import { test, expect } from "@playwright/test";
import { at, ids, NOW, openPreview, stored, TODAY } from "./fixtures/seed";
test.use({ timezoneId: "Europe/Warsaw" });

test("quick add: full syntax with chips, chip removal and Tab completion", async ({
  page,
}) => {
  await openPreview(page);
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  const addButton = page.getByRole("button", {
    name: "Dodaj zadanie",
    exact: true,
  });
  const chips = page.getByLabel("Rozpoznane pola");

  await input.fill("WMS-300 Eksport etykiet ~2h @pt do:20.10 ! #wm");
  // Tab uzupełnia projekt pisownią istniejącego projektu.
  await input.press("Tab");
  await expect(input).toHaveValue(
    "WMS-300 Eksport etykiet ~2h @pt do:20.10 ! #WMS ",
  );
  await expect(input).toBeFocused();
  await expect(chips.getByRole("button")).toHaveText([
    "Jira: WMS-300",
    "Szacunek: 2 h",
    "Plan: jutro 16.10",
    "Termin: wt 20.10",
    "Priorytet: wysoki",
    "Projekt: WMS",
  ]);
  // Kliknięcie chipa usuwa zapis z pola.
  await chips.getByRole("button", { name: "Usuń: Szacunek: 2 h" }).click();
  await expect(input).toHaveValue(
    "WMS-300 Eksport etykiet @pt do:20.10 ! #WMS ",
  );
  await expect(input).toBeFocused();
  await input.press("End");
  await input.pressSequentially("~1h30");
  await expect(
    chips.getByRole("button", { name: "Usuń: Szacunek: 1 h 30 min" }),
  ).toBeVisible();

  await addButton.click();
  await expect(input).toHaveValue("");
  await expect(chips).toHaveCount(0);
  const card = page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: "Eksport etykiet" }) });
  await expect(card).toBeVisible();

  const data = await stored(page);
  const task = data.tasks.find((t: any) => t.title === "Eksport etykiet");
  expect(task).toMatchObject({
    jiraKey: "WMS-300",
    done: false,
    details: {
      estimateMinutes: 90,
      plannedDay: "2026-10-16",
      dueDay: "2026-10-20",
      priority: 1,
      project: "WMS",
    },
  });
  // Dodanie nie uruchamia pomiaru.
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toEqual([]);
});

test("quick add: impossible date blocks adding and plain titles stay untouched", async ({
  page,
}) => {
  await openPreview(page);
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  const addButton = page.getByRole("button", {
    name: "Dodaj zadanie",
    exact: true,
  });
  await input.fill("Raport @31.02");
  await expect(page.getByText("Nie ma dnia 31.02.2027.")).toBeVisible();
  await expect(addButton).toBeDisabled();
  await input.press("Enter");
  expect(
    (await stored(page)).tasks.some((t: any) => t.title.startsWith("Raport")),
  ).toBe(false);

  // Bez prefiksów: tytuł dokładnie jak wpisany, bez szczegółów i chipów.
  await input.fill("Spotkanie 1h z zespołem C\\#");
  await expect(page.getByLabel("Rozpoznane pola")).toHaveCount(0);
  await input.fill("Spotkanie 1h z zespołem");
  await addButton.click();
  await expect(input).toHaveValue("");
  const task = (await stored(page)).tasks.find(
    (t: any) => t.title === "Spotkanie 1h z zespołem",
  );
  expect(task).toBeTruthy();
  expect(task.details).toBeUndefined();
  expect(task.jiraKey).toBeUndefined();
});

test("quick switch: no results offers create-and-start (Enter) and create (Shift+Enter)", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();

  // Shift+Enter: tylko tworzy, pomiar trwa dalej.
  await page.keyboard.press("Control+k");
  let dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  let search = dialog.getByLabel("Znajdź zadanie do uruchomienia");
  await search.fill("Notatka z rozmowy !niski");
  await expect(dialog.locator(".quick-chip")).toHaveText(["Priorytet: niski"]);
  await expect(
    dialog.getByRole("button", { name: "Utwórz: Notatka z rozmowy" }),
  ).toBeVisible();
  await page.keyboard.press("Shift+Enter");
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  let data = await stored(page);
  expect(
    data.tasks.find((t: any) => t.title === "Notatka z rozmowy"),
  ).toMatchObject({ details: { priority: -1 } });

  // Enter: tworzy i uruchamia, poprzedni pomiar staje.
  await page.keyboard.press("Control+k");
  dialog = page.getByRole("dialog", { name: "Szybkie przełączanie" });
  search = dialog.getByLabel("Znajdź zadanie do uruchomienia");
  await search.fill("Przegląd planu ~30m #zesp");
  await search.press("Tab");
  await expect(search).toHaveValue("Przegląd planu ~30m #Zespół ");
  await expect(dialog.locator(".quick-result")).toHaveCount(2);
  await page.keyboard.press("Enter");
  await expect(dialog).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Przegląd planu", { exact: true }),
  ).toBeVisible();

  data = await stored(page);
  const created = data.tasks.find((t: any) => t.title === "Przegląd planu");
  expect(created).toMatchObject({
    details: { estimateMinutes: 30, project: "Zespół" },
  });
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toEqual([
    expect.objectContaining({
      taskId: created.id,
      startedAt: Date.parse(NOW),
    }),
  ]);
  expect(
    data.sessions.find(
      (s: any) =>
        s.taskId === ids.analysis && s.startedAt === at(TODAY, "11:00"),
    ),
  ).toMatchObject({ endedAt: Date.parse(NOW) });
});
