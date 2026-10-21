import { test, expect } from "@playwright/test";
test.use({ timezoneId: "Europe/Warsaw" });
const key = "todo-track-preview-v1";
test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date("2026-10-14T17:00:00+02:00"));
  await page.goto("/");
});
test("inline priority preserves unsaved planning and reorders collapsed tasks; quick switch keeps one timer", async ({
  page,
}) => {
  for (const title of ["A", "B"]) {
    await page.getByLabel("Nowe zadanie", { exact: true }).fill(title);
    await page
      .getByRole("button", { name: "Dodaj zadanie", exact: true })
      .click();
  }
  await page.getByLabel("Priorytet: A", { exact: true }).selectOption("1");
  await expect(page.locator(".task-card .task-title")).toHaveText(["A", "B"]);
  await expect(page.locator(".task-details")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Przedziały: A", exact: true })
    .click();
  await page.getByLabel("Projekt / obszar", { exact: true }).fill("WMS");
  await page.getByLabel("Priorytet: A", { exact: true }).selectOption("-1");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await page.reload();
  await expect(page.getByLabel("Priorytet: A", { exact: true })).toHaveValue(
    "-1",
  );
  await page.getByLabel("Start: A", { exact: true }).click();
  await page.keyboard.press("Control+k");
  await page.getByLabel("Znajdź zadanie do uruchomienia").fill("B");
  await page.keyboard.press("Enter");
  await expect(page.getByLabel("Stop: B", { exact: true })).toBeVisible();
  const data = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    key,
  );
  expect(data.sessions.filter((s: any) => s.endedAt === null)).toHaveLength(1);
  expect(data.tasks.find((t: any) => t.title === "A").details.project).toBe(
    "WMS",
  );
  await page.screenshot({
    path: "test-results/organizer-priority.png",
    fullPage: true,
  });
});
test("archive and trash preserve recoverable history and searchable notes", async ({
  page,
}) => {
  await page.evaluate(
    (k) =>
      localStorage.setItem(
        k,
        JSON.stringify({
          tasks: [
            {
              id: 1,
              title: "Import",
              done: false,
              createdAt: 0,
              details: {
                notes: { "2026-10-14": "walidacja CSV" },
                project: "WMS",
              },
            },
          ],
          sessions: [
            {
              id: 1,
              taskId: 1,
              startedAt: +new Date("2026-10-14T09:00:00"),
              endedAt: +new Date("2026-10-14T10:30:00"),
            },
          ],
        }),
      ),
    key,
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Przedziały: Import", exact: true })
    .click();
  await page.getByRole("button", { name: "Archiwizuj", exact: true }).click();
  await expect(page.locator(".task-card")).toHaveCount(0);
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page
    .getByLabel("Szukaj tytułu, notatki, projektu lub daty")
    .fill("CSV");
  await expect(page.locator(".library-row")).toContainText("01:30:00");
  await page.getByRole("button", { name: "Archiwum", exact: true }).click();
  await page
    .getByRole("button", { name: "Przywróć: Import", exact: true })
    .click();
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await page.getByRole("button", { name: "Usuń: Import", exact: true }).click();
  await page.getByRole("button", { name: "Usuń zadanie", exact: true }).click();
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await expect(page.locator(".library-row")).toHaveCount(0);
  await page.getByRole("button", { name: "Kosz", exact: true }).click();
  await page
    .getByRole("button", { name: "Przywróć: Import", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Szukaj w historii", exact: true })
    .click();
  await expect(page.locator(".library-row")).toContainText("01:30:00");
});
test("day closing saves multiple notes, reports decimal time and moves unfinished plan", async ({
  page,
}) => {
  await page.evaluate(
    (k) =>
      localStorage.setItem(
        k,
        JSON.stringify({
          tasks: [1, 2].map((id) => ({
            id,
            title: "Zadanie " + id,
            done: false,
            createdAt: 0,
            details: { plannedDay: "2026-10-14" },
          })),
          sessions: [
            {
              id: 1,
              taskId: 1,
              startedAt: +new Date("2026-10-14T09:00:00"),
              endedAt: +new Date("2026-10-14T10:00:00"),
            },
            {
              id: 2,
              taskId: 2,
              startedAt: +new Date("2026-10-14T11:00:00"),
              endedAt: +new Date("2026-10-14T11:30:00"),
            },
          ],
        }),
      ),
    key,
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknięcie dnia", exact: true })
    .click();
  await page
    .getByLabel("Notatka: Zadanie 1", { exact: true })
    .fill("Import CSV");
  await page
    .getByLabel("Notatka: Zadanie 2", { exact: true })
    .fill("Spotkanie");
  await page
    .getByRole("button", { name: "Zapisz notatki dnia", exact: true })
    .click();
  await expect(page.getByLabel("Raport TS zamknięcia dnia")).toHaveValue(
    /Import CSV\t1,00/,
  );
  await expect(page.getByLabel("Raport TS zamknięcia dnia")).toHaveValue(
    /Spotkanie\t0,50/,
  );
  await page
    .getByRole("button", {
      name: "Przenieś niedokończone na czw., 15 paź",
      exact: true,
    })
    .click();
  await page
    .getByRole("button", { name: "Oznacz dzień jako sprawdzony", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Otwórz dzień ponownie" }),
  ).toBeVisible();
  await page.screenshot({
    path: "test-results/organizer-day-close.png",
    fullPage: true,
  });
  const data = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    key,
  );
  expect(data.closedDays).toContain("2026-10-14");
  expect(
    data.tasks.every((t: any) => t.details.plannedDay === "2026-10-15"),
  ).toBe(true);
});
test("recurrence creates separate dated tasks once and resets checklist", async ({
  page,
}) => {
  await page.getByLabel("Nowe zadanie", { exact: true }).fill("Daily");
  await page
    .getByRole("button", { name: "Dodaj zadanie", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Przedziały: Daily", exact: true })
    .click();
  await page.getByLabel("Szacowany czas (minuty)").fill("30");
  await page.getByLabel("Rodzaj cyklu").selectOption("");
  await page.getByLabel("Początek cyklu").fill("2026-10-12");
  await page.getByLabel("Pon", { exact: true }).check();
  await page.getByLabel("Śr", { exact: true }).check();
  await page.getByLabel("Nowy krok", { exact: true }).fill("Sprawdź logi");
  await page.getByRole("button", { name: "Dodaj krok", exact: true }).click();
  await page.getByLabel("Gotowe: Sprawdź logi", { exact: true }).check();
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await page.reload();
  await expect(page.locator(".task-card")).toHaveCount(2);
  await page.reload();
  await expect(page.locator(".task-card")).toHaveCount(2);
  const data = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    key,
  );
  const clone = data.tasks.find(
    (t: any) => t.details.plannedDay === "2026-10-14",
  );
  expect(clone.details.estimateMinutes).toBe(30);
  expect(clone.details.checklist[0].done).toBe(false);
  expect(clone.details.repeatDays).toEqual([]);
});
