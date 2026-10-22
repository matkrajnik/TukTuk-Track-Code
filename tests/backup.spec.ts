import { test, expect, type Page } from "./fixtures/test";
import { at, seed } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw" });

const restored = {
  tasks: [
    {
      id: 1,
      title: "Zadanie z kopii",
      done: false,
      createdAt: at("2026-10-01", "09:00"),
    },
  ],
  sessions: [
    {
      id: 1,
      taskId: 1,
      startedAt: at("2026-10-01", "09:00"),
      endedAt: at("2026-10-01", "10:00"),
    },
  ],
  trash: [],
  trashSessions: [],
};
const preview = {
  name: "TukTuk-Track-2026-10-01.sqlite",
  tasks: 1,
  sessions: 1,
  trash: 0,
};

async function openBackups(page: Page) {
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Kopie", exact: true }).click();
}
const restoreCalls = async (page: Page) =>
  (await backend<any[]>(page, "calls")).filter(
    (c) => c.cmd === "restore_backup",
  ).length;

test("backup restore shows a summary and requires the confirmation checkbox", async ({
  page,
}) => {
  await openDesktop(page, { backup: { preview, snapshot: restored } });
  await expect(
    page.getByLabel("Start: Analiza importu", { exact: true }),
  ).toBeVisible();
  await openBackups(page);
  await page
    .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
    .click();
  await expect(
    page.getByRole("heading", { name: "TukTuk-Track-2026-10-01.sqlite" }),
  ).toBeVisible();
  await expect(
    page.getByText("Zadania: 1, przedziały czasu: 1, zadania w koszu: 0."),
  ).toBeVisible();
  const restore = page.getByRole("button", { name: "Przywróć wybraną kopię" });
  const confirm = page.getByLabel("Chcę zastąpić dane wybraną kopią");
  await expect(confirm).not.toBeChecked();
  await expect(restore).toBeDisabled();

  // Anuluj zamyka podsumowanie; ponowny wybór zaczyna od pustego potwierdzenia.
  await confirm.check();
  await page.getByRole("button", { name: "Anuluj", exact: true }).click();
  await expect(restore).toHaveCount(0);
  await page
    .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
    .click();
  await expect(confirm).not.toBeChecked();
  await expect(restore).toBeDisabled();
  expect(await restoreCalls(page)).toBe(0);

  await confirm.check();
  await expect(restore).toBeEnabled();
  await restore.click();
  await expect(
    page.getByText(
      "Przywrócono kopię. Poprzednia baza została zachowana w automatycznej kopii bezpieczeństwa.",
    ),
  ).toBeVisible();
  await expect(restore).toHaveCount(0);
  expect(await restoreCalls(page)).toBe(1);

  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(
    page.getByLabel("Start: Zadanie z kopii", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("Start: Analiza importu", { exact: true }),
  ).toHaveCount(0);
});

test("backup restore: cancelling the file dialog shows no summary", async ({
  page,
}) => {
  await openDesktop(page, { data: seed(), backup: null });
  await openBackups(page);
  await page
    .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
    .click();
  await expect(
    page.getByRole("button", { name: "Przywróć wybraną kopię" }),
  ).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  expect(await restoreCalls(page)).toBe(0);
});
