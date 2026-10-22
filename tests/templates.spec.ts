import { test, expect, type Page } from "./fixtures/test";
import { jiraFixture, openPreview, stored } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
test.use({ timezoneId: "Europe/Warsaw" });

const TEMPLATES_KEY = "tuktuk-track-preview-templates-v1";
const storedTemplates = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), TEMPLATES_KEY);

/** Rozwija kafelek i zapisuje zadanie jako szablon o podanej nazwie. */
async function saveFromTile(page: Page, title: string, name: string) {
  await page.getByLabel(`Przedziały: ${title}`, { exact: true }).click();
  await page
    .getByRole("button", { name: `Zapisz jako szablon: ${title}` })
    .click();
  const nameInput = page.getByLabel(`Nazwa szablonu: ${title}`, { exact: true });
  await expect(nameInput).toBeFocused();
  await nameInput.fill(name);
  await page.getByRole("button", { name: "Zapisz szablon", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: `Zapisano szablon „${name}”` })).toBeVisible();
}

/** Ustawienia → Szablony: nowy tytuł i kroki szablonu. */
async function editInSettings(page: Page, name: string) {
  await openSettings(page, "Szablony");
  const form = page.getByRole("form", { name: `Szablon ${name}` });
  await form
    .getByLabel(`Tytuł: ${name}`, { exact: true })
    .fill("Code review {klucz}: {tytuł}");
  await form
    .getByLabel(`Kroki: ${name}`, { exact: true })
    .fill("Przeczytaj opis\n\nSprawdź testy");
  await form.getByRole("button", { name: "Zapisz szablon" }).click();
  await expect(form.getByRole("status")).toHaveText("Zapisano.");
  return form;
}

test("templates: save from a tile, edit in settings and use with explicit fields", async ({
  page,
}) => {
  await openPreview(page);
  await saveFromTile(page, "Code review", "review");
  expect(await storedTemplates(page)).toEqual([
    {
      id: 1,
      name: "review",
      title: "Code review",
      estimateMinutes: 60,
      steps: [],
      project: "Foundation",
      priority: 0,
    },
  ]);

  // Drugi szablon; nazwa różniąca się tylko wielkością liter jest odrzucana.
  await saveFromTile(page, "Wdrożenie etykiet", "etykiety");
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await page
    .getByRole("button", { name: "Zapisz jako szablon: Analiza importu" })
    .click();
  await page
    .getByLabel("Nazwa szablonu: Analiza importu", { exact: true })
    .fill("Review");
  await page.getByRole("button", { name: "Zapisz szablon", exact: true }).click();
  await expect(page.getByRole("alert").first()).toContainText(
    "Szablon „review” już istnieje.",
  );

  await editInSettings(page, "review");
  // Usunięcie wymaga potwierdzenia.
  const other = page.getByRole("form", { name: "Szablon etykiety" });
  await other.getByRole("button", { name: "Usuń szablon etykiety" }).click();
  await other.getByRole("button", { name: "Tak, usuń" }).click();
  await expect(other).toHaveCount(0);
  expect((await storedTemplates(page)).map((t: any) => t.name)).toEqual([
    "review",
  ]);

  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  const input = page.getByLabel("Nowe zadanie", { exact: true });
  await input.fill("/re");
  await input.press("Tab");
  await expect(input).toHaveValue("/review ");
  await input.press("End");
  await input.pressSequentially("WMS-300 Poprawki ~1h30");
  await expect(page.getByLabel("Rozpoznane pola").getByRole("button")).toHaveText(
    ["Szablon: review", "Jira: WMS-300", "Szacunek: 1 h 30 min"],
  );
  await page.getByRole("button", { name: "Dodaj zadanie", exact: true }).click();
  await expect(input).toHaveValue("");

  const task = (await stored(page)).tasks.find(
    (t: any) => t.title === "Code review WMS-300: Poprawki",
  );
  // Szacunek z linii wygrywa, projekt i kroki pochodzą z szablonu.
  expect(task).toMatchObject({
    jiraKey: "WMS-300",
    details: {
      estimateMinutes: 90,
      project: "Foundation",
      checklist: [
        { text: "Przeczytaj opis", done: false },
        { text: "Sprawdź testy", done: false },
      ],
    },
  });
  // Szablon nie uruchamia pomiaru.
  expect(
    (await stored(page)).sessions.filter((s: any) => s.endedAt === null),
  ).toEqual([]);
});

test("templates: /review WMS-31 takes the title from Jira in the app", async ({
  page,
}) => {
  await openDesktop(page, { jira: jiraFixture() });
  await saveFromTile(page, "Code review", "review");
  await editInSettings(page, "review");
  await page.getByRole("button", { name: "Zadania", exact: true }).click();

  const input = page.getByLabel("Nowe zadanie", { exact: true });
  await input.fill("/review WMS-31");
  await expect(page.getByLabel("Rozpoznane pola").getByRole("button")).toHaveText(
    ["Szablon: review", "Jira: WMS-31"],
  );
  await input.press("Enter");
  await expect(input).toHaveValue("");
  await expect(
    page.locator(".task-title", {
      hasText: "Code review WMS-31: Obsługa przyjęcia towaru",
    }),
  ).toBeVisible();

  const snapshot = await backend<any>(page, "snapshot");
  const task = snapshot.tasks.find((t: any) => t.jiraKey === "WMS-31");
  expect(task).toMatchObject({
    title: "Code review WMS-31: Obsługa przyjęcia towaru",
    details: {
      estimateMinutes: 60,
      project: "Foundation",
      checklist: [
        { text: "Przeczytaj opis", done: false },
        { text: "Sprawdź testy", done: false },
      ],
    },
  });
  const call = (await backend<any[]>(page, "calls")).find(
    (c) => c.cmd === "quick_add",
  );
  expect(call.args.task).toMatchObject({
    title: "",
    jiraKey: "WMS-31",
    template: "review",
  });
});

test("templates: editing the estimate in settings saves minutes", async ({
  page,
}) => {
  await openPreview(page);
  await saveFromTile(page, "Code review", "review");
  await openSettings(page, "Szablony");
  const form = page.getByRole("form", { name: "Szablon review" });
  const estimate = form.getByLabel("Szacunek: review", { exact: true });
  await estimate.fill("45");
  await form.getByRole("button", { name: "Zapisz szablon" }).click();
  await expect(form.getByRole("status")).toHaveText("Zapisano.");
  await expect(form.getByRole("alert")).toHaveCount(0);
  expect((await storedTemplates(page))[0].estimateMinutes).toBe(45);

  // Puste pole oznacza brak szacunku.
  await estimate.fill("");
  await form.getByRole("button", { name: "Zapisz szablon" }).click();
  await expect(form.getByRole("status")).toHaveText("Zapisano.");
  expect((await storedTemplates(page))[0].estimateMinutes).toBe(0);
});
