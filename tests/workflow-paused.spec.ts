import { test, expect, type Page } from "./fixtures/test";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
import { analysisImplementationReview } from "../src/api/workflowTemplates";

/**
 * Decyzje po E7 (PLAN_WDROZENIA.md, 15, „2026-11-06 — decyzje po E7”):
 * przebieg wstrzymany po restarcie (`paused`) czeka na „Wznów” w monitorze,
 * a „Zakończ aplikację” z traya w trakcie kroku pyta o potwierdzenie.
 * Zastępcze IPC (tests/fixtures/tauri.ts) odtwarza `workflow_resume` i
 * `app_quit_confirmed` z Rust; nic nie łączy się z prawdziwymi agentami.
 */

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
    errorCode: null,
    waitingApproval: false,
  };
}

function detail(id: number, status: string) {
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
    recovery: null,
  };
}

const paused = (extra: Record<string, unknown> = {}) => ({
  runs: [summary(1, "paused"), summary(2, "succeeded")],
  details: { 1: detail(1, "paused"), 2: detail(2, "succeeded") },
  events: { 1: [], 2: [] },
  ...extra,
});

async function openMonitor(
  page: Page,
  workflowRuns: Record<string, unknown>,
  unlocked = true,
) {
  await openDesktop(page, { workflows: { unlocked }, workflowRuns });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
}

const callsOf = async (page: Page, cmd: string) =>
  (await backend<{ cmd: string; args: any }[]>(page, "calls")).filter(
    (c) => c.cmd === cmd,
  );

test("wstrzymany przebieg: status „Wstrzymany” w liście i w filtrze", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: paused(),
  });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  const table = page.locator(".wf-table");
  await expect(table.getByRole("row", { name: /Wstrzymany/ })).toHaveCount(1);
  await page.getByLabel("Status").selectOption({ label: "Wstrzymany" });
  await expect
    .poll(async () => (await callsOf(page, "workflow_runs")).at(-1)?.args.filter.status)
    .toEqual(["paused"]);
  await expect(table.getByRole("row")).toHaveCount(2);
  await expect(table.getByText("Wstrzymany")).toBeVisible();
});

test("„Wznów” wznawia przebieg jednym wywołaniem także przy podwójnym kliknięciu", async ({
  page,
}) => {
  await openMonitor(page, paused());
  await expect(page.getByTestId("run-status")).toContainText("Wstrzymany");
  await expect(page.getByTestId("run-paused")).toContainText(
    "wstrzymany po restarcie aplikacji",
  );
  // Wstrzymana odpowiedź i dwa kliknięcia w jednym zadaniu (zanim Vue wyłączy
  // przycisk): drugie wywołanie zatrzymuje tylko strażnik `resuming`.
  await page.evaluate(() => {
    const w = window as any;
    w.__workflowResumeHold = new Promise((r) => (w.__releaseResume = r));
  });
  const resume = page.getByRole("button", { name: "Wznów" });
  await resume.evaluate((button: HTMLButtonElement) => {
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
  await page.evaluate(() => (window as any).__releaseResume());
  await expect(page.getByTestId("run-status")).toContainText("W toku");
  await expect(resume).toHaveCount(0);
  const calls = await callsOf(page, "workflow_resume");
  expect(calls).toHaveLength(1);
  expect(calls[0].args.runId).toBe(1);
  expect(typeof calls[0].args.requestId).toBe("string");
  expect(await callsOf(page, "workflow_start")).toHaveLength(0);
  await expect(page.getByRole("status").first()).toContainText("Wznowiono przebieg.");
});

test("powtórne „Wznów” po błędzie używa tego samego identyfikatora żądania", async ({
  page,
}) => {
  await openMonitor(page, paused({ resumeError: "Silnik przebiegów jeszcze się uruchamia." }));
  const resume = page.getByRole("button", { name: "Wznów" });
  await resume.click();
  await expect(page.getByRole("alert")).toContainText(
    "Nie udało się wznowić przebiegu: Silnik przebiegów jeszcze się uruchamia.",
  );
  await page.evaluate(() => {
    (window as any).__workflowRunsState.resumeError = undefined;
  });
  await resume.click();
  await expect(page.getByTestId("run-status")).toContainText("W toku");
  const calls = await callsOf(page, "workflow_resume");
  expect(calls).toHaveLength(2);
  expect(calls[1].args.requestId).toBe(calls[0].args.requestId);
});

test("wznowienie z naruszonym katalogiem kończy przebieg błędem", async ({
  page,
}) => {
  await openMonitor(page, paused({ resumeWorkspaceBroken: true }));
  await page.getByRole("button", { name: "Wznów" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Błąd");
  await expect(page.getByTestId("run-status").locator("..")).toContainText(
    "Nie udało się przygotować katalogu roboczego przebiegu.",
  );
  await expect(page.getByRole("alert")).toContainText(
    "Nie wznowiono przebiegu: katalog roboczy jest niedostępny",
  );
  await expect(page.locator(".wf-monitor")).not.toContainText("Wznowiono");
});

test("bez klucza nie ma „Wznów”, a „Zatrzymaj” na tablicy działa", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: false },
    workflowRuns: paused(),
  });
  await nav(page)
    .getByRole("button", { name: /^Agenci|Sesje agentów/ })
    .first()
    .click();
  const section = page.getByRole("region", { name: /Przebiegi procesów w toku/ });
  await expect(section).toContainText("Wstrzymany");
  await expect(section.getByRole("button", { name: "Wznów" })).toHaveCount(0);
  await expect(
    section.getByRole("button", { name: "Zatrzymaj przebieg 1" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// „Zakończ aplikację” w trakcie kroku przebiegu

const quitDialog = (page: Page) =>
  page.getByRole("dialog", { name: "Zakończyć aplikację?" });

async function askQuit(page: Page, workflow = "Proces testowy") {
  await page.evaluate(
    (name) => (window as any).__tauriMock.emit("app-quit-request", name),
    workflow,
  );
  await expect(quitDialog(page)).toBeVisible();
}

test("pytanie o zakończenie: „Zakończ” woła komendę raz", async ({ page }) => {
  await openDesktop(page, {});
  await askQuit(page);
  await expect(quitDialog(page)).toContainText(
    "Trwa krok przebiegu „Proces testowy”. Zakończenie aplikacji przerwie go, a przebieg zostanie oznaczony jako przerwany. Zakończyć?",
  );
  // Dwa kliknięcia w jednym zadaniu (zanim Vue wyłączy przycisk): drugie
  // wywołanie zatrzymuje tylko strażnik `busy`.
  await quitDialog(page)
    .getByRole("button", { name: "Zakończ" })
    .evaluate((button: HTMLButtonElement) => {
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      button.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
  await expect
    .poll(async () => (await callsOf(page, "app_quit_confirmed")).length)
    .toBe(1);
  await page.waitForTimeout(100);
  expect(await callsOf(page, "app_quit_confirmed")).toHaveLength(1);
  // Po sukcesie okno zostaje zajęte (aplikacja się kończy).
  await expect(
    quitDialog(page).getByRole("button", { name: "Zakończ" }),
  ).toBeDisabled();
});

test("pytanie o zakończenie: „Anuluj” i Esc nie kończą aplikacji, fokus wraca", async ({
  page,
}) => {
  await openDesktop(page, {});
  const nav0 = nav(page).getByRole("button");
  await nav0.first().focus();
  await askQuit(page);
  await expect(
    quitDialog(page).getByRole("button", { name: "Anuluj" }),
  ).toBeFocused();
  await quitDialog(page).getByRole("button", { name: "Anuluj" }).click();
  await expect(quitDialog(page)).toHaveCount(0);
  await expect(nav0.first()).toBeFocused();
  await askQuit(page);
  await expect(
    quitDialog(page).getByRole("button", { name: "Anuluj" }),
  ).toBeFocused();
  // Bez przesunięcia zegara Esc nie zamyka okna: fixture zamraża `Date.now()`
  // (`page.clock.setFixedTime`), a Vue przy pierwszym handlerze (`@keydown` na
  // `<section>` w ConfirmDialog) znakuje zdarzenie `_vts = Date.now()` i pomija
  // handler przodka (`@keydown.esc` na nakładce), gdy `_vts <= attached`; przy
  // zamrożonym zegarze oba czasy są równe. Ten sam zabieg w workflows-ui i
  // workflow-recovery.
  await page.clock.setFixedTime(new Date(Date.now() + 5000));
  await page.keyboard.press("Escape");
  await expect(quitDialog(page)).toHaveCount(0);
  await expect(nav0.first()).toBeFocused();
  expect(await callsOf(page, "app_quit_confirmed")).toHaveLength(0);
  // Anuluj i Esc czyszczą pytanie w backendzie.
  expect(await callsOf(page, "app_quit_cancel")).toHaveLength(2);
});

test("pytanie o zakończenie: błąd zakończenia jest widoczny, a Anuluj znów działa", async ({
  page,
}) => {
  await openDesktop(page, {
    quit: { error: "Nie udało się zatrzymać pomiaru: baza zajęta." },
  });
  await askQuit(page);
  await quitDialog(page).getByRole("button", { name: "Zakończ" }).click();
  await expect(quitDialog(page).getByRole("alert")).toHaveText(
    "Nie udało się zakończyć aplikacji: Nie udało się zatrzymać pomiaru: baza zajęta.",
  );
  await expect(
    quitDialog(page).getByRole("button", { name: "Zakończ" }),
  ).toBeEnabled();
  await quitDialog(page).getByRole("button", { name: "Anuluj" }).click();
  await expect(quitDialog(page)).toHaveCount(0);
  expect(await callsOf(page, "app_quit_cancel")).toHaveLength(1);
});

test("pytanie wysłane przed nasłuchem okna pokazuje się po starcie (app_quit_pending)", async ({
  page,
}) => {
  await openDesktop(page, { quit: { pending: "Proces z traya" } });
  await expect(quitDialog(page)).toContainText(
    "Trwa krok przebiegu „Proces z traya”.",
  );
  expect(await callsOf(page, "app_quit_pending")).toHaveLength(1);
  await quitDialog(page).getByRole("button", { name: "Anuluj" }).click();
  await expect(quitDialog(page)).toHaveCount(0);
  // Pytanie wyczyszczone: ponowne odczytanie nic nie zwraca.
  const pending = await page.evaluate(() =>
    (window as any).__TAURI_INTERNALS__.invoke("app_quit_pending"),
  );
  expect(pending).toBeNull();
});

test("okno wyłączenia Workflows ostrzega o przebiegach wstrzymanych", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: paused(),
  });
  await openSettings(page, "Workflows");
  await page.getByRole("button", { name: "Wyłącz Workflows" }).click();
  const dialog = page.getByRole("dialog", { name: "Wyłączyć Workflows?" });
  await expect(dialog).toContainText(
    "Przebiegi wstrzymane po restarcie: 1. Po wyłączeniu nie da się ich wznowić (tylko zatrzymać)",
  );
  await expect(dialog).not.toContainText("Przebiegi czekające na zgodę");
  expect(
    (await callsOf(page, "workflow_runs")).at(-1)?.args.filter.status,
  ).toEqual(["waitingForApproval", "paused"]);
});
