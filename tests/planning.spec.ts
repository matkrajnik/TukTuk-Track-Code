import { test, expect, addPlanned } from "./fixtures/test";
import { pick } from "./fixtures/searchSelect";
test.use({ timezoneId: "Europe/Warsaw" });
test("planning, priorities, daily notes, waiting and time reconstruction persist", async ({
  page,
}) => {
  await page.clock.setFixedTime(new Date("2026-10-14T17:00:00+02:00"));
  await page.goto("/");
  await page.evaluate(() =>
    localStorage.setItem(
      "tuktuk-track-preview-v1",
      JSON.stringify({
        tasks: [
          { id: 1, title: "Import WMS", done: false, createdAt: 0 },
          { id: 2, title: "Spotkanie", done: false, createdAt: 0 },
        ],
        sessions: [
          {
            id: 1,
            taskId: 1,
            startedAt: +new Date("2026-10-14T09:00:00"),
            endedAt: +new Date("2026-10-14T10:00:00"),
          },
        ],
      }),
    ),
  );
  await page.reload();
  await page
    .getByRole("button", { name: "Przedziały: Import WMS", exact: true })
    .click();
  await page
    .getByLabel("Priorytet: Import WMS", { exact: true })
    .selectOption("1");
  await page.getByLabel("Planowany dzień", { exact: true }).fill("2026-10-14");
  await page.getByLabel("Termin", { exact: true }).fill("2026-10-15");
  await page.getByLabel("Projekt / obszar", { exact: true }).fill("WMS");
  await page
    .getByLabel("Co zrobiłem", { exact: true })
    .fill("Poprawiłem walidację importu");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByRole("status")).toHaveText("Zapisano plan i notatki.");
  await page.reload();
  await expect(page.getByLabel("Priorytet: Import WMS", {exact:true})).toHaveValue("1");
  await pick(page.getByRole("combobox", { name: "Filtr projektu" }), "WMS");
  await expect(page.locator(".task-card")).toHaveCount(1);
  await page
    .getByRole("button", { name: "Raporty dzienne", exact: true })
    .click();
  await page.getByRole("button", { name: "Raport TS", exact: true }).click();
  await expect(page.getByLabel("Treść raportu TS")).toHaveValue(
    /Import WMS — Poprawiłem walidację importu\t1,00/,
  );
  await page.getByLabel("Dołącz notatki z tego dnia").uncheck();
  await expect(page.getByLabel("Treść raportu TS")).not.toHaveValue(
    /walidację/,
  );
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await expect(page.getByRole("region", { name: "Plan dnia" })).toContainText(
    "Import WMS",
  );
  await page
    .getByRole("button", { name: "Dodaj czas ręcznie", exact: true })
    .click();
  await pick(page.getByLabel("Zadanie do uzupełnienia"), "Spotkanie");
  await page.getByLabel("Początek ręcznego czasu").fill("2026-10-14T09:30");
  await page.getByLabel("Koniec ręcznego czasu").fill("2026-10-14T10:30");
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect(page.getByRole("alert")).toContainText("nakłada");
  await page.getByLabel("Początek ręcznego czasu").fill("2026-10-14T11:00");
  await page.getByLabel("Koniec ręcznego czasu").fill("2026-10-14T11:30");
  await page.getByRole("button", { name: "Zapisz ręczny czas" }).click();
  await expect(page.locator(".timeline-row")).toHaveCount(3);
  await expect(page.locator(".timeline-row.gap")).toContainText("01:00:00");
  await page
    .getByRole("button", { name: "Przełóż na czw., 15 paź", exact: true })
    .click();
  await expect(page.getByRole("region", { name: "Plan dnia" })).toContainText(
    "Brak zaplanowanych",
  );
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await page.getByRole("button", { name: "Podsumowanie", exact: true }).click();
  await expect(page.locator(".report-total")).toHaveText("01:30:00");
  await expect(page.getByText("WMS", { exact: true })).toBeVisible();
  await page.screenshot({
    path: "test-results/planning-week.png",
    fullPage: true,
  });
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await page
    .getByRole("button", { name: "Przedziały: Import WMS", exact: true })
    .click();
  await page
    .getByLabel("Czekam na…", { exact: true })
    .fill("Odpowiedź klienta");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(
    page.getByRole("region", { name: "Czekam na…", exact: true }),
  ).toContainText("Import WMS");
  await page.getByLabel("Start: Import WMS", { exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Czekam na…", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("region", { name: "Rozpoczęte", exact: true }),
  ).toContainText("Import WMS");
  await page.reload();
  await expect(
    page.getByLabel("Stop: Import WMS", { exact: true }),
  ).toBeVisible();
});

test("higher priority sorts first and plan works on an empty day", async ({
  page,
}) => {
  await page.goto("/");
  for (const title of ["Ważne", "Zwykłe"]) {
    await addPlanned(page, title);
  }
  await page
    .getByRole("button", { name: "Przedziały: Ważne", exact: true })
    .click();
  await page.getByLabel("Priorytet: Ważne", { exact: true }).selectOption("1");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.locator(".task-card .task-title")).toHaveText([
    "Ważne",
    "Zwykłe",
  ]);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByLabel("Dzień planu").fill("2026-11-02");
  await pick(page.getByLabel("Zadanie do planu"), "Ważne");
  await page
    .getByRole("button", { name: "Dodaj do planu", exact: true })
    .click();
  await expect(page.getByRole("region", { name: "Plan dnia" })).toContainText(
    "Ważne",
  );
  await expect(page.locator(".timeline-row")).toHaveCount(0);
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    path: "test-results/planning-day.png",
    fullPage: true,
  });
});
