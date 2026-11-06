import { test, expect, type Page } from "./fixtures/test";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
import { openPreview } from "./fixtures/seed";
import { analysisImplementationReview } from "../src/api/workflowTemplates";

/**
 * Workflows E6: odzyskanie przerwanego przebiegu, diff usunięty przez retencję,
 * blokada aktualizacji i przywracania kopii przy przebiegu w toku oraz pole
 * retencji w Ustawieniach. Zastępcze IPC (tests/fixtures/tauri.ts) odtwarza
 * `workflow_recover`, `update_blocker` i `restore_locked` z Rust; żaden test nie
 * łączy się z prawdziwymi agentami ani usługami.
 */

const HASH = "c".repeat(64);
const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });

function summary(id: number, status: string) {
  return {
    id,
    workflowId: 1,
    workflowName: "Proces testowy",
    version: 1,
    status,
    taskId: null,
    taskTitle: null,
    taskExists: false,
    repo: "C:\\repo",
    createdAt: 1_700_000_000_000 + id,
    startedAt: null,
    finishedAt: null,
    errorCode: status === "interrupted" ? "interrupted" : null,
    waitingApproval: status === "waitingForApproval",
  };
}

function detail(id: number, status: string, over: Record<string, unknown> = {}) {
  return {
    summary: summary(id, status),
    goal: null,
    graph: analysisImplementationReview(),
    definitionHash: "h",
    revision: 4,
    workspace: "C:\\dane\\workflows\\worktrees\\1",
    baseCommit: null,
    error: null,
    executions: [],
    approvals: [],
    agentMs: 0,
    waitingMs: 0,
    tokens: null,
    recovery:
      status === "interrupted"
        ? {
            workspaceReserved: true,
            processesUnconfirmed: 1,
            unconfirmedPids: [4242],
            attempts: 0,
          }
        : null,
    ...over,
  };
}

function stateEvent(sequence: number) {
  return {
    runId: 1,
    sequence,
    executionId: null,
    type: "runStateChanged",
    timestamp: 1_700_000_000_000 + sequence,
    payload: { type: "runStateChanged", from: "queued", to: "running", code: null },
  };
}

async function openMonitor(page: Page, workflowRuns: Record<string, unknown>) {
  await openDesktop(page, { workflows: { unlocked: true }, workflowRuns });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
}

const callsOf = async (page: Page, cmd: string) =>
  (await backend<{ cmd: string }[]>(page, "calls")).filter((c) => c.cmd === cmd)
    .length;

const recoveryRegion = (page: Page) =>
  page.getByRole("region", { name: "Odzyskanie przebiegu" });
const confirmDialog = (page: Page) =>
  page.getByRole("dialog", { name: "Odzyskać przebieg?" });
const confirmButton = (page: Page) =>
  confirmDialog(page).getByRole("button", {
    name: "Sprawdź procesy i zwolnij katalog",
  });

/** Otwiera okno potwierdzenia odzyskania. */
async function openRecoverDialog(page: Page) {
  await recoveryRegion(page)
    .getByRole("button", { name: "Odzyskaj przebieg" })
    .click();
  await expect(confirmDialog(page)).toBeVisible();
}

test("przerwany przebieg: „Odzyskaj przebieg” zwalnia katalog i niczego nie wznawia", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "interrupted")],
    details: { 1: detail(1, "interrupted") },
    events: { 1: [1, 2, 3, 4].map(stateEvent) },
  });
  await expect(page.getByTestId("run-status")).toContainText("Przerwany");
  const recovery = page.getByRole("region", { name: "Odzyskanie przebiegu" });
  await expect(recovery.getByTestId("run-recovery")).toContainText(
    "Nie potwierdzono końca procesu agenta (PID 4242)",
  );
  await openRecoverDialog(page);
  expect(await callsOf(page, "workflow_recover")).toBe(0);
  await confirmButton(page).click();
  await expect(confirmDialog(page)).toHaveCount(0);
  await expect(page.getByRole("status").first()).toContainText(
    "Przebieg odzyskany: katalog roboczy zwolniony.",
  );
  await expect(recovery.getByTestId("run-recovery")).toContainText(
    "Przebieg odzyskany",
  );
  await expect(
    recovery.getByRole("button", { name: "Odzyskaj przebieg" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("run-status")).toContainText("Przerwany");
  expect(await callsOf(page, "workflow_recover")).toBe(1);
  expect(await callsOf(page, "workflow_start")).toBe(0);
  expect(await callsOf(page, "workflow_decide")).toBe(0);
  // Zdarzenie `recovery` trafia na oś zdarzeń.
  await expect(page.getByText("#5 recovery")).toBeVisible();
});

test("niepotwierdzony koniec procesu zostawia blokadę katalogu", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "interrupted")],
    details: { 1: detail(1, "interrupted") },
    events: { 1: [] },
    recoverUnconfirmed: true,
    recoverUnconfirmedPid: 5150,
  });
  const recovery = recoveryRegion(page);
  await openRecoverDialog(page);
  await confirmButton(page).click();
  // Komunikat z PID jako alert: użytkownik może zakończyć proces ręcznie.
  const alert = page.getByRole("alert");
  await expect(alert).toContainText("PID 5150");
  await expect(alert).toContainText("katalog roboczy pozostaje zablokowany");
  await expect(recovery.getByTestId("run-recovery")).toContainText("PID 5150");
  await expect(
    recovery.getByRole("button", { name: "Odzyskaj przebieg" }),
  ).toBeEnabled();
});

test("podwójne kliknięcie potwierdzenia wysyła jedno odzyskanie", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "interrupted")],
    details: { 1: detail(1, "interrupted") },
    events: { 1: [] },
  });
  await openRecoverDialog(page);
  await confirmButton(page).dblclick();
  await expect(page.getByRole("status").first()).toContainText(
    "Przebieg odzyskany",
  );
  expect(await callsOf(page, "workflow_recover")).toBe(1);
});

test("Esc zamyka potwierdzenie odzyskania i przywraca fokus", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "interrupted")],
    details: { 1: detail(1, "interrupted") },
    events: { 1: [] },
  });
  const trigger = recoveryRegion(page).getByRole("button", {
    name: "Odzyskaj przebieg",
  });
  await openRecoverDialog(page);
  // Zamrożony zegar: Vue pomija handler przodka z tym samym znacznikiem czasu.
  await page.clock.setFixedTime(new Date(Date.now() + 5000));
  await page.keyboard.press("Escape");
  await expect(confirmDialog(page)).toHaveCount(0);
  await expect(trigger).toBeFocused();
  expect(await callsOf(page, "workflow_recover")).toBe(0);
});

test("podgląd: odzyskanie przykładowego przerwanego przebiegu", async ({
  page,
}) => {
  await openPreview(page);
  await page.evaluate(() =>
    localStorage.setItem(
      "tuktuk-track-preview-workflows-v1",
      JSON.stringify({ unlocked: true, nextId: 1, workflows: [] }),
    ),
  );
  await page.reload();
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await expect(page.getByRole("row", { name: /przerwany/ })).toContainText(
    "Przerwany",
  );
  await page.getByRole("button", { name: "Otwórz przebieg 2" }).click();
  await expect(recoveryRegion(page).getByTestId("run-recovery")).toContainText(
    "Katalog roboczy jest zablokowany",
  );
  await openRecoverDialog(page);
  await confirmButton(page).click();
  await expect(page.getByRole("status").first()).toContainText(
    "Przebieg odzyskany: katalog roboczy zwolniony.",
  );
  await expect(
    recoveryRegion(page).getByRole("button", { name: "Odzyskaj przebieg" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("run-status")).toContainText("Przerwany");
});

test("przebieg zakończony nie ma sekcji odzyskania", async ({ page }) => {
  await openMonitor(page, {
    runs: [summary(1, "succeeded")],
    details: { 1: detail(1, "succeeded") },
    events: { 1: [] },
  });
  await expect(page.getByTestId("run-status")).toBeVisible();
  await expect(recoveryRegion(page)).toHaveCount(0);
});

test("przebieg w toku nie ma sekcji odzyskania", async ({ page }) => {
  await openMonitor(page, {
    runs: [summary(1, "running")],
    details: { 1: detail(1, "running") },
    events: { 1: [] },
  });
  await expect(page.getByTestId("run-status")).toContainText("W toku");
  await expect(
    page.getByRole("region", { name: "Odzyskanie przebiegu" }),
  ).toHaveCount(0);
});

test("diff usunięty przez retencję: komunikat zamiast błędu odczytu", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "succeeded")],
    details: {
      1: detail(1, "succeeded", {
        approvals: [
          {
            id: "ap-1",
            executionId: "e-1",
            status: "approved",
            artifactHash: HASH,
            comment: null,
            decidedAt: 1,
            artifactAvailable: false,
            artifactPurged: true,
          },
        ],
      }),
    },
    events: { 1: [] },
  });
  await expect(page.getByTestId("artifact-purged")).toContainText(
    "Diff zgody usunięto po okresie przechowywania",
  );
});

test("brak pliku diffu (np. po przywróceniu kopii): komunikat o niedostępnym pliku", async ({
  page,
}) => {
  await openMonitor(page, {
    runs: [summary(1, "succeeded")],
    details: {
      1: detail(1, "succeeded", {
        approvals: [
          {
            id: "ap-1",
            executionId: "e-1",
            status: "approved",
            artifactHash: HASH,
            comment: null,
            decidedAt: 1,
            artifactAvailable: false,
            artifactPurged: false,
          },
        ],
      }),
    },
    events: { 1: [] },
  });
  await expect(page.getByTestId("artifact-unavailable")).toContainText(
    "Pliku diffu zgody nie ma na dysku",
  );
  await expect(page.getByTestId("artifact-purged")).toHaveCount(0);
});

const banner = (page: Page) =>
  page.getByRole("region", { name: "Dostępna aktualizacja" });

test("aktualizacja jest wstrzymana, gdy przebieg Workflows działa", async ({
  page,
}) => {
  await openDesktop(page, {
    update: { version: "9.9.9" },
    workflowRuns: { runs: [summary(1, "running")] },
  });
  await banner(page)
    .getByRole("button", { name: "Zainstaluj i uruchom ponownie" })
    .click();
  await expect(banner(page).getByRole("alert")).toContainText(
    "Aktualizacja wstrzymana. Przebiegi Workflows w toku: 1.",
  );
  expect(await callsOf(page, "prepare_update")).toBe(1);
  expect(await callsOf(page, "plugin:updater|install")).toBe(0);
});

test("przebieg czekający na zgodę nie blokuje aktualizacji", async ({ page }) => {
  await openDesktop(page, {
    update: { version: "9.9.9" },
    workflowRuns: { runs: [summary(1, "waitingForApproval")] },
  });
  await banner(page)
    .getByRole("button", { name: "Zainstaluj i uruchom ponownie" })
    .click();
  await expect
    .poll(() => callsOf(page, "plugin:updater|install"))
    .toBe(1);
});

for (const status of ["cancelling", "waitingForApproval"])
  test(`przywracanie kopii jest odrzucane przy przebiegu ${status}`, async ({
    page,
  }) => {
    await openDesktop(page, {
      backup: {
        preview: { name: "kopia.sqlite", tasks: 0, sessions: 0, trash: 0 },
        snapshot: { tasks: [], sessions: [], trash: [], trashSessions: [] },
      },
      workflowRuns: { runs: [summary(1, status)] },
    });
    await page
      .getByRole("button", { name: "Historia i kopie", exact: true })
      .click();
    await page.getByRole("button", { name: "Kopie", exact: true }).click();
    await page
      .getByRole("button", { name: "Wybierz kopię do przywrócenia" })
      .click();
    await page.getByLabel("Chcę zastąpić dane wybraną kopią").check();
    await page.getByRole("button", { name: "Przywróć wybraną kopię" }).click();
    await expect(page.getByRole("alert")).toContainText(
      "Nie można teraz przywrócić kopii. Nieskończone przebiegi Workflows: 1.",
    );
    expect(await callsOf(page, "restore_backup")).toBe(1);
    // Baza robocza bez zmian: zadania sprzed próby przywrócenia zostają.
    const snapshot = await backend<any>(page, "snapshot");
    expect(snapshot.tasks.some((t: any) => t.title === "Analiza importu")).toBe(
      true,
    );
  });

test("Ustawienia → Workflows: retencja diffów zapisuje dni i odrzuca zły zakres", async ({
  page,
}) => {
  await openDesktop(page, { workflows: { unlocked: true } });
  await openSettings(page, "Workflows");
  const field = page.getByLabel(
    "Przechowywanie diffów zakończonych przebiegów (dni)",
  );
  await expect(field).toHaveValue("30");
  await field.fill("7");
  await field.press("Enter");
  await field.blur();
  await expect
    .poll(async () =>
      (await backend<{ cmd: string; args: any }[]>(page, "calls"))
        .filter((c) => c.cmd === "set_settings")
        .map((c) => c.args.patch.workflowRetentionDays),
    )
    .toContain(7);
  // Krótsza niż tydzień retencja mogłaby usunąć diff w trakcie przeglądu.
  await field.fill("6");
  await field.blur();
  await expect(page.getByRole("alert")).toContainText(
    "Przechowywanie diffów Workflows: od 7 do 365 dni.",
  );
  await expect(field).toHaveValue("7");
});

test("bez klucza pola retencji nie ma", async ({ page }) => {
  await openDesktop(page);
  await openSettings(page, "Workflows");
  await expect(
    page.getByLabel("Przechowywanie diffów zakończonych przebiegów (dni)"),
  ).toHaveCount(0);
});
