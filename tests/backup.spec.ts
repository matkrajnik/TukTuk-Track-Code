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

test("backup export reports Workflows diffs left out of the copy", async ({
  page,
}) => {
  await openDesktop(page, {
    backupExport: {
      path: "C:\Kopie\TukTuk-Track-2026-10-01.sqlite",
      skippedArtifacts: 2,
      overLimitArtifacts: 1,
    },
  });
  await openBackups(page);
  await page.getByRole("button", { name: "Eksportuj kopię" }).click();
  const message = page.getByText(/Zapisano kopię:/);
  await expect(message).toContainText(
    "Zapisano kopię: C:\Kopie\TukTuk-Track-2026-10-01.sqlite.",
  );
  await expect(message).toContainText(
    "Pominięto diffy Workflows bez pliku albo z niezgodną treścią: 2.",
  );
  await expect(message).toContainText(
    "Diffy Workflows ponad limit 256 MB kopii nie trafiły do niej: 1.",
  );
});

test("backup export without skipped diffs shows only the path", async ({
  page,
}) => {
  await openDesktop(page, {
    backupExport: {
      path: "C:\Kopie\kopia.sqlite",
      skippedArtifacts: 0,
      overLimitArtifacts: 0,
    },
  });
  await openBackups(page);
  await page.getByRole("button", { name: "Eksportuj kopię" }).click();
  await expect(page.getByText(/Zapisano kopię:/)).toHaveText(
    "Zapisano kopię: C:\Kopie\kopia.sqlite.",
  );
});

test("backup restore summary warns up front about an unfinished Workflows run", async ({
  page,
}) => {
  await openDesktop(page, {
    backup: { preview, snapshot: restored },
    workflowRuns: {
      runs: [
        {
          id: 1,
          workflowId: 1,
          workflowName: "Proces",
          version: 1,
          status: "waitingForApproval",
        },
      ],
    },
  });
  await openBackups(page);
  await page
    .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
    .click();
  await expect(page.getByTestId("restore-workflow-blocker")).toContainText(
    "Przywrócenie zostanie odrzucone. Nieskończone przebiegi Workflows: 1.",
  );
});

test("backup restore summary has no Workflows warning without unfinished runs", async ({
  page,
}) => {
  await openDesktop(page, { backup: { preview, snapshot: restored } });
  await openBackups(page);
  await page
    .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
    .click();
  await expect(
    page.getByRole("heading", { name: "TukTuk-Track-2026-10-01.sqlite" }),
  ).toBeVisible();
  await expect(page.getByTestId("restore-workflow-blocker")).toHaveCount(0);
});
