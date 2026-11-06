import { test, expect, type Page } from "./fixtures/test";
import { openPreview } from "./fixtures/seed";
import { openDesktop, TEST_WORKFLOWS_KEY } from "./fixtures/tauri";
import { analysisImplementationReview } from "../src/api/workflowTemplates";

/**
 * Workflows E5: uruchomienia, monitor, zgoda i historia na zastępczym IPC
 * (`workflow_runs` w tests/fixtures/tauri.ts). Żaden test nie łączy się z
 * prawdziwymi agentami ani usługami.
 */

const HASH = "b".repeat(64);
const emitEvent = (page: Page, event: object) =>
  page.evaluate(
    (e) => (window as any).__tauriMock.emit("workflow-event", e),
    event,
  );
const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });

function summary(id: number, over: Record<string, unknown> = {}) {
  return {
    id,
    workflowId: 1,
    workflowName: "Proces testowy",
    version: 1,
    status: "waitingForApproval",
    taskId: null,
    taskTitle: null,
    taskExists: false,
    repo: "C:\\repo",
    createdAt: 1_700_000_000_000 + id,
    startedAt: null,
    finishedAt: null,
    errorCode: null,
    waitingApproval: true,
    ...over,
  };
}

function detail(id: number, over: Record<string, unknown> = {}) {
  return {
    summary: summary(id),
    goal: "Napraw odejmowanie",
    graph: analysisImplementationReview(),
    definitionHash: "h",
    revision: 3,
    workspace: null,
    baseCommit: null,
    error: null,
    executions: [],
    approvals: [
      {
        id: "ap-1",
        executionId: "e-1",
        status: "pending",
        artifactHash: HASH,
        comment: null,
        decidedAt: null,
        artifactAvailable: true,
      },
    ],
    agentMs: 1000,
    waitingMs: 2000,
    tokens: null,
    ...over,
  };
}

/**
 * Zdarzenie w kształcie hosta (`StoredEvent.payload` = zserializowany `RunEvent`):
 * komunikat runnera ma tekst w `payload.payload.text`, pozostałe typy są płaskie.
 */
function ev(sequence: number, type = "message", payload: object = {}) {
  const inner =
    type === "message"
      ? { executionId: "e-1", payload: { text: `linia ${sequence}`, ...payload } }
      : payload;
  return {
    runId: 1,
    sequence,
    executionId: "e-1",
    type,
    timestamp: 1_700_000_000_000 + sequence,
    payload: { type, ...inner },
  };
}

const DIFF = {
  text: "+poprawka\n",
  truncated: false,
  size: 10,
  hash: HASH,
};

async function openRuns(page: Page, workflowRuns: object) {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns,
  });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
}

const approvalFixture = () => ({
  runs: [summary(1)],
  details: { 1: detail(1) },
  events: { 1: [ev(1), ev(2), ev(3)] },
  diffs: { [HASH]: DIFF },
});

test("historia pokazuje status tekstem i otwiera monitor", async ({ page }) => {
  await openRuns(page, approvalFixture());
  const row = page.getByRole("row", { name: /Proces testowy/ });
  await expect(row).toContainText("Czeka na zgodę");
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Czeka na zgodę");
  await expect(page.getByText("Skrót zmian:")).toBeVisible();
  await expect(page.getByLabel("Zmiany do zaakceptowania")).toContainText(
    "+poprawka",
  );
  await expect(page.getByText("tokeny: brak danych")).toBeVisible();
});

test("podwójne kliknięcie Akceptuj daje jeden skutek", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  const accept = page.getByRole("button", { name: "Akceptuj" });
  await accept.dblclick();
  await expect(page.getByTestId("run-status")).toContainText("Zakończony");
  const count = await page.evaluate(
    () => (window as any).__workflowDecideCount,
  );
  expect(count).toBe(1);
});

test("odrzucenie bez komentarza jest zablokowane", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("button", { name: "Odrzuć" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "Odrzucenie wymaga komentarza.",
  );
  await expect(page.getByTestId("run-status")).toContainText("Czeka na zgodę");
  await page.getByLabel(/Komentarz/).fill("Brak testu");
  await page.getByRole("button", { name: "Odrzuć" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Odrzucony");
});

test("unieważniona zgoda pokazuje komunikat", async ({ page }) => {
  await openRuns(page, { ...approvalFixture(), invalidate: true });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "unieważniły zgodę",
  );
});

test("5000 komunikatów renderuje najwyżej 500 linii", async ({ page }) => {
  const events = Array.from({ length: 5000 }, (_, i) => ev(i + 1));
  await openRuns(page, {
    ...approvalFixture(),
    events: { 1: events },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  const lines = page.getByRole("list", { name: "Dziennik agentów" }).locator("li");
  await expect(lines).toHaveCount(500);
  await expect(
    page.getByRole("button", { name: "Pokaż wcześniejsze (4500)" }),
  ).toBeVisible();
});

test("monitor ignoruje duplikat i dociąga lukę od ostatniej sekwencji", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const before = await page.evaluate(() => (window as any).__tauriMock.calls());
  const fetched = (calls: any[]) =>
    calls.filter((c) => c.cmd === "workflow_run_events");
  await emitEvent(page, ev(2));
  // Luka: przychodzi sekwencja 7, więc monitor prosi o zdarzenia po 3.
  await emitEvent(page, ev(7));
  await expect
    .poll(async () => {
      const calls = await page.evaluate(() => (window as any).__tauriMock.calls());
      return fetched(calls).length - fetched(before).length;
    })
    .toBeGreaterThan(0);
  const calls = await page.evaluate(() => (window as any).__tauriMock.calls());
  const last = fetched(calls).at(-1);
  expect(last.args.after).toBe(3);
});

test("filtr historii po statusie", async ({ page }) => {
  await openRuns(page, {
    runs: [
      summary(1, { status: "succeeded", waitingApproval: false }),
      summary(2, { status: "failed", errorCode: "workspaceUnavailable" }),
    ],
    details: {},
  });
  await expect(page.getByRole("row")).toHaveCount(3);
  await page.getByLabel("Status").selectOption("failed");
  await expect(page.getByRole("row")).toHaveCount(2);
  await expect(page.getByRole("row", { name: /Błąd/ })).toBeVisible();
});

test("zatrzymanie wymaga potwierdzenia, a Esc zamyka okno", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("button", { name: "Zatrzymaj" }).click();
  const dialog = page.getByRole("dialog", { name: "Zatrzymać przebieg?" });
  await expect(dialog).toBeVisible();
  // Zegar jest zamrożony: Vue pomija handler przodka z tym samym znacznikiem czasu.
  await page.clock.setFixedTime(new Date(Date.now() + 5000));
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  const calls = await page.evaluate(() => (window as any).__tauriMock.calls());
  expect(calls.some((c: any) => c.cmd === "workflow_cancel")).toBe(false);
});

test("przebieg nie wywołuje timera ani integracji", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const before = (await page.evaluate(() => (window as any).__tauriMock.calls()))
    .length;
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Zakończony");
  const calls = (
    await page.evaluate(() => (window as any).__tauriMock.calls())
  ).slice(before);
  const cmds = calls.map((c: any) => c.cmd);
  for (const forbidden of ["start", "stop", "jira_start", "redmine_start"])
    expect(cmds).not.toContain(forbidden);
  expect(cmds.some((c: string) => /^(jira|redmine|ts)_/.test(c))).toBe(false);
});

test("podgląd: przykładowy przebieg widoczny, start odrzucony komunikatem", async ({
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
  await expect(page.getByRole("row", { name: /przykład/ })).toContainText(
    "Czeka na zgodę",
  );
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByLabel(/Komentarz/).fill("x");
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "Podgląd nie uruchamia agentów.",
  );
});

test("klucz testowy nie trafia do spec", () => {
  expect(TEST_WORKFLOWS_KEY.length).toBeGreaterThan(0);
});

// ---------------------------------------------------------------------------
// Runda 1 poprawek E5: formularz startu, karta zadania, monitor, tablica agentów

const NOW_MS = 1_700_000_000_000;

/** Szablon z innym modelem w kroku analizy: szkic różni się od opublikowanej wersji. */
function graphWithModel(model: string) {
  const graph = analysisImplementationReview();
  for (const node of graph.nodes)
    if (node.kind.type === "agent" && node.id === "analysis") node.kind.model = model;
  return graph;
}

function definition(over: Record<string, unknown> = {}) {
  return {
    id: 5,
    name: "Proces E5",
    description: "",
    draftRevision: 3,
    createdAt: NOW_MS,
    updatedAt: NOW_MS,
    archivedAt: null,
    publishedVersion: 2,
    publishedAt: NOW_MS,
    unpublishedChanges: true,
    draft: graphWithModel("model-ze-szkicu"),
    issues: [],
    // Jak `store::get`: od najnowszej wersji.
    versions: [
      { id: 42, version: 2, hash: "h2", createdAt: NOW_MS },
      { id: 41, version: 1, hash: "h1", createdAt: NOW_MS },
    ],
    ...over,
  };
}

const READY = [
  {
    nodeId: "analysis",
    provider: "claude",
    access: "readOnly",
    cliFound: true,
    version: "2.1.318",
    supported: true,
    enforcement: "cliControl",
    warning: null,
    blocked: null,
  },
];

async function openStartable(
  page: Page,
  options: { preflight?: unknown[]; startError?: string } = {},
) {
  await openDesktop(page, {
    workflows: {
      unlocked: true,
      detail: definition(),
      versionGraphs: {
        42: graphWithModel("model-z-wersji-2"),
        41: graphWithModel("model-z-wersji-1"),
      },
    },
    preflight: options.preflight ?? READY,
    workflowRuns: {
      ...approvalFixture(),
      startResult: { runId: 1, created: true },
      startError: options.startError,
    },
  });
}

async function openStartForm(page: Page) {
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await page.getByRole("button", { name: "Uruchom Proces E5" }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  return dialog;
}

const starts = (page: Page) =>
  page.evaluate(() => (window as any).__workflowStarts ?? []);
const mockCalls = (page: Page) =>
  page.evaluate(() => (window as any).__tauriMock.calls());

test("formularz pokazuje kroki opublikowanej wersji, nie szkicu", async ({ page }) => {
  await openStartable(page);
  const dialog = await openStartForm(page);
  const steps = dialog.getByRole("region", {
    name: "Kroki agentów (tylko do odczytu)",
  });
  await expect(steps).toContainText("model-z-wersji-2");
  await expect(steps).not.toContainText("model-ze-szkicu");
  await expect(steps).not.toContainText("model-z-wersji-1");
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByRole("button", { name: "Uruchom", exact: true }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const sent = await starts(page);
  expect(sent).toHaveLength(1);
  expect(sent[0].versionId).toBe(42);
});

test("start z listy definicji otwiera monitor przebiegu", async ({ page }) => {
  await openStartable(page);
  const dialog = await openStartForm(page);
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByLabel("Cel przebiegu").fill("Napraw odejmowanie");
  await dialog.getByRole("button", { name: "Uruchom", exact: true }).click();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Uruchomienia" })).toBeVisible();
  await expect(page.getByTestId("run-status")).toContainText("Czeka na zgodę");
  const sent = await starts(page);
  expect(sent[0].goal).toBe("Napraw odejmowanie");
  expect(sent[0].repo).toBe("C:\\repo");
});

test("podwójne kliknięcie Uruchom wysyła jeden requestId i tworzy jeden przebieg", async ({
  page,
}) => {
  await openStartable(page);
  const dialog = await openStartForm(page);
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByRole("button", { name: "Uruchom", exact: true }).dblclick();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const sent = await starts(page);
  expect(sent.length).toBeGreaterThanOrEqual(1);
  expect(new Set(sent.map((s: any) => s.requestId)).size).toBe(1);
  const created = await page.evaluate(
    () => Object.keys((window as any).__workflowRequestIds ?? {}).length,
  );
  expect(created).toBe(1);
});

test("Uruchom nieaktywne przy blokadzie preflightu", async ({ page }) => {
  await openStartable(page, {
    preflight: [
      {
        ...READY[0],
        cliFound: false,
        blocked: { code: "cliMissing", message: "Brak CLI" },
      },
    ],
  });
  const dialog = await openStartForm(page);
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await expect(
    dialog.getByRole("button", { name: "Uruchom", exact: true }),
  ).toBeDisabled();
  expect(await starts(page)).toHaveLength(0);
});

test("błąd startu zmienia requestId przed ponowieniem", async ({ page }) => {
  await openStartable(page, {
    startError: "Nie udało się przygotować katalogu roboczego.",
  });
  const dialog = await openStartForm(page);
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  const run = dialog.getByRole("button", { name: "Uruchom", exact: true });
  await run.click();
  await expect(dialog).toContainText("katalogu roboczego");
  await page.evaluate(() => {
    (window as any).__workflowRunsState.startError = undefined;
  });
  await run.click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const ids = (await mockCalls(page))
    .filter((c: any) => c.cmd === "workflow_start")
    .map((c: any) => c.args.request.requestId);
  expect(ids).toHaveLength(2);
  expect(ids[0]).not.toBe(ids[1]);
});

const taskCard = (page: Page) =>
  page.getByRole("button", { name: "Przedziały: Analiza importu", exact: true });
const taskActions = (page: Page) =>
  page.getByRole("group", { name: "Akcje zadania" });

test("karta zadania bez klucza nie pokazuje akcji Uruchom proces", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: false, detail: definition() },
  });
  await taskCard(page).click();
  await expect(
    taskActions(page).getByRole("button", {
      name: "Przypomnij: Analiza importu",
    }),
  ).toBeVisible();
  await expect(
    taskActions(page).getByRole("button", { name: /Uruchom proces/ }),
  ).toHaveCount(0);
});

test("karta zadania po odblokowaniu otwiera formularz z zadaniem i startuje jeden przebieg", async ({
  page,
}) => {
  await openStartable(page);
  await taskCard(page).click();
  const before = (await mockCalls(page)).length;
  await taskActions(page)
    .getByRole("button", { name: "Uruchom proces: Analiza importu" })
    .click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Proces").selectOption({ label: "Proces E5 (v2)" });
  await expect(dialog.getByLabel("Cel przebiegu")).toHaveValue("Analiza importu");
  await expect(
    dialog.getByRole("region", { name: "Kroki agentów (tylko do odczytu)" }),
  ).toContainText("model-z-wersji-2");
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByRole("button", { name: "Uruchom", exact: true }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const sent = await starts(page);
  expect(sent).toHaveLength(1);
  expect(sent[0].taskId).not.toBeNull();
  // Start z karty nie rusza timera ani integracji.
  const calls = (await mockCalls(page)).slice(before);
  const cmds = calls.map((c: any) => c.cmd);
  expect(cmds).not.toContain("mutate");
  expect(
    calls.some(
      (c: any) => c.cmd === "change" && /start|stop/.test(String(c.args?.action)),
    ),
  ).toBe(false);
  expect(cmds.some((c: string) => /^(jira|redmine|ts)_/.test(c))).toBe(false);
});

test("Esc zamyka formularz z karty, a fokus wraca na przycisk", async ({
  page,
}) => {
  await openStartable(page);
  await taskCard(page).click();
  const opener = taskActions(page).getByRole("button", {
    name: "Uruchom proces: Analiza importu",
  });
  await opener.click();
  await expect(page.getByRole("dialog")).toBeVisible();
  // Zegar jest zamrożony: Vue pomija handler przodka z tym samym znacznikiem czasu.
  await page.clock.setFixedTime(new Date(Date.now() + 5000));
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(opener).toBeFocused();
  expect(await starts(page)).toHaveLength(0);
});

test("podgląd: formularz startu zablokowany komunikatem, start niemożliwy", async ({
  page,
}) => {
  await openPreview(page);
  await page.evaluate((graph) => {
    const now = Date.UTC(2026, 10, 5);
    localStorage.setItem(
      "tuktuk-track-preview-workflows-v1",
      JSON.stringify({
        unlocked: true,
        nextId: 2,
        workflows: [
          {
            id: 1,
            name: "Proces podglądu",
            description: "",
            draft: graph,
            draftRevision: 1,
            createdAt: now,
            updatedAt: now,
            archivedAt: null,
            versions: [{ id: 1, version: 1, hash: "x", createdAt: now, graph }],
          },
        ],
      }),
    );
  }, analysisImplementationReview());
  await page.reload();
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await page.getByRole("button", { name: "Uruchom Proces podglądu" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await expect(dialog).toContainText("Podgląd nie uruchamia agentów");
  await expect(
    dialog.getByRole("button", { name: "Uruchom", exact: true }),
  ).toBeDisabled();
});

test("monitor po ponownym otwarciu widoku odtwarza zdarzenia od snapshotu", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  const log = page.getByRole("list", { name: "Dziennik agentów" }).locator("li");
  await expect(log).toHaveCount(3);
  await nav(page).getByRole("button", { name: "Zadania" }).click();
  await expect(page.getByTestId("run-status")).toHaveCount(0);
  const before = (await mockCalls(page)).length;
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(log).toHaveCount(3);
  const calls = (await mockCalls(page)).slice(before);
  const pages = calls.filter((c: any) => c.cmd === "workflow_run_events");
  expect(pages[0].args.after).toBe(0);
});

test("nasłuch workflow-event usunięty po zamknięciu monitora", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  await nav(page).getByRole("button", { name: "Zadania" }).click();
  await expect(page.getByTestId("run-status")).toHaveCount(0);
  const calls = await mockCalls(page);
  const of = (cmd: string) =>
    calls.filter((c: any) => c.cmd === cmd && c.args.event === "workflow-event")
      .length;
  expect(of("plugin:event|listen")).toBeGreaterThan(0);
  expect(of("plugin:event|unlisten")).toBe(of("plugin:event|listen"));
  const before = calls.length;
  await emitEvent(page, ev(9));
  await page.waitForTimeout(200);
  const after = (await mockCalls(page)).slice(before);
  expect(after.filter((c: any) => c.cmd.startsWith("workflow_run"))).toHaveLength(0);
});

test("nasłuch rejestrowany przed snapshotem: zdarzenie z czasu odczytu nie ginie", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const calls = await mockCalls(page);
  const listen = calls.findIndex(
    (c: any) => c.cmd === "plugin:event|listen" && c.args.event === "workflow-event",
  );
  const snapshot = calls.findIndex((c: any) => c.cmd === "workflow_run_get");
  expect(listen).toBeGreaterThanOrEqual(0);
  expect(listen).toBeLessThan(snapshot);
});

test("Więcej doładowuje kolejną stronę historii", async ({ page }) => {
  const runs = Array.from({ length: 25 }, (_, i) =>
    summary(i + 1, { status: "succeeded", waitingApproval: false }),
  );
  await openRuns(page, { runs, details: {} });
  await expect(page.getByRole("row")).toHaveCount(21);
  await page.getByRole("button", { name: "Więcej" }).click();
  await expect(page.getByRole("row")).toHaveCount(26);
  const last = (await mockCalls(page))
    .filter((c: any) => c.cmd === "workflow_runs")
    .at(-1);
  expect(last.args.filter.beforeId).toBe(6);
});

test("Akceptuj nieaktywne, dopóki diff nie jest widoczny; błąd diffu daje ponowienie", async ({
  page,
}) => {
  await openRuns(page, { ...approvalFixture(), diffs: {} });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByRole("alert")).toContainText("Nie udało się wczytać zmian");
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeDisabled();
  await page.evaluate((diff) => {
    (window as any).__workflowRunsState.diffs = { [diff.hash]: diff };
  }, DIFF);
  await page.getByRole("button", { name: "Spróbuj ponownie" }).click();
  await expect(page.getByLabel("Zmiany do zaakceptowania")).toContainText(
    "+poprawka",
  );
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeEnabled();
});

test("druga zgoda w tym samym przebiegu dostaje nowy requestId", async ({
  page,
}) => {
  await openRuns(page, { ...approvalFixture(), followUpApproval: true });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByRole("status").first()).toContainText("Zaakceptowano");
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeEnabled();
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Zakończony");
  const ids = await page.evaluate(
    () => (window as any).__workflowDecideRequests,
  );
  expect(ids).toHaveLength(2);
  expect(ids[0]).not.toBe(ids[1]);
});

test("nieaktualna rewizja jest pokazana jak unieważnienie", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByLabel("Zmiany do zaakceptowania")).toBeVisible();
  await page.evaluate(() => {
    (window as any).__workflowRunsState.details[1].revision = 99;
  });
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByRole("status").first()).toContainText(
    "Przejrzyj zgodę jeszcze raz",
  );
  await expect(page.getByTestId("run-status")).toContainText("Czeka na zgodę");
});

test("zatrzymanie zakończonego przebiegu zwraca błąd jak w Rust", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("button", { name: "Zatrzymaj" }).click();
  await page.evaluate(() => {
    (window as any).__workflowRunsState.details[1].summary.status = "succeeded";
  });
  await page
    .getByRole("dialog", { name: "Zatrzymać przebieg?" })
    .getByRole("button", { name: "Zatrzymaj" })
    .click();
  await expect(page.getByRole("alert")).toContainText(
    "Przebieg już się zakończył.",
  );
});

test("monitor pokazuje graf ze statusem kroków tekstem", async ({ page }) => {
  const execution = (id: string, nodeId: string, status: string) => ({
    id,
    nodeId,
    iteration: 1,
    attempt: 1,
    status,
    startedAt: 1,
    finishedAt: status === "completed" ? 2 : null,
    exitCode: null,
    errorCode: null,
    output: null,
  });
  await openRuns(page, {
    ...approvalFixture(),
    details: {
      1: detail(1, {
        executions: [
          execution("e-1", "analysis", "completed"),
          execution("e-2", "implementation", "running"),
        ],
      }),
    },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("tab", { name: "Graf" }).click();
  const graph = page.getByRole("img", { name: /Graf procesu/ });
  await expect(graph).toHaveAttribute("aria-label", /Analiza: Gotowy; Implementacja: W toku/);
  await expect(graph).toHaveAttribute("aria-label", /nie uruchomiony/);
  await expect(graph).toContainText("W toku");
});

test("graf w monitorze nie pozwala przeciągać węzłów", async ({ page }) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await page.getByRole("tab", { name: "Graf" }).click();
  const node = page.locator(".wf-monitor .wf-node").first();
  const before = await node.getAttribute("transform");
  const box = await node.boundingBox();
  if (!box) throw new Error("brak węzła");
  await page.mouse.move(box.x + 10, box.y + 10);
  await page.mouse.down();
  await page.mouse.move(box.x + 120, box.y + 80, { steps: 5 });
  await page.mouse.up();
  await expect(node).toHaveAttribute("transform", before ?? "");
});

const monitorTabs = (page: Page) =>
  page.getByRole("tablist", { name: "Widok przebiegu" });

/** Oczekiwane powiększenie dużego widoku: dopasowanie do szerokości, 100–160%. */
async function expectedGraphZoom(page: Page): Promise<number> {
  return page.evaluate(() => {
    const scroller = document.querySelector(".wf-canvas.large .wf-canvas-scroll");
    const svg = scroller?.querySelector("svg");
    if (!scroller || !svg) throw new Error("brak dużego grafu");
    const width = Number(svg.getAttribute("viewBox")?.split(" ")[2]);
    const ratio = (scroller.clientWidth - 16) / width;
    return Math.min(160, Math.max(100, Math.floor(ratio * 10) * 10));
  });
}

test("monitor ma zakładki Przebieg i Graf przełączane klawiaturą", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  const tabs = monitorTabs(page);
  const runTab = tabs.getByRole("tab", { name: "Przebieg" });
  const graphTab = tabs.getByRole("tab", { name: "Graf" });
  // Przebieg czeka na zgodę: domyślnie „Przebieg” (tam są przyciski decyzji).
  await expect(runTab).toHaveAttribute("aria-selected", "true");
  await expect(graphTab).toHaveAttribute("aria-selected", "false");
  await expect(runTab).toHaveAttribute("aria-controls", "wf-monitor-panel-run");
  await expect(graphTab).toHaveAttribute("aria-controls", "wf-monitor-panel-graph");
  const runPanel = page.getByRole("tabpanel", { name: "Przebieg" });
  await expect(runPanel).toHaveAttribute("id", "wf-monitor-panel-run");
  await expect(runPanel.getByRole("button", { name: "Akceptuj" })).toBeVisible();
  await expect(runPanel.getByRole("img", { name: /Graf procesu/ })).toHaveCount(0);
  // Panel grafu montuje się dopiero po wejściu w zakładkę.
  await expect(page.locator("#wf-monitor-panel-graph")).toHaveCount(0);
  await expect(page.getByRole("img", { name: /Graf procesu/ })).toHaveCount(0);

  await runTab.focus();
  await page.keyboard.press("ArrowRight");
  await expect(graphTab).toBeFocused();
  await expect(graphTab).toHaveAttribute("aria-selected", "true");
  await expect(runTab).toHaveAttribute("aria-selected", "false");
  await expect(runTab).toHaveAttribute("tabindex", "-1");
  const graphPanel = page.getByRole("tabpanel", { name: "Graf" });
  await expect(graphPanel).toHaveAttribute("id", "wf-monitor-panel-graph");
  const graph = graphPanel.getByRole("img", { name: /Graf procesu/ });
  await expect(graph).toBeVisible();
  await expect(graph).toContainText("Implementacja");
  await expect(graph).toContainText("Analiza");
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeHidden();
  // Graf zajmuje co najmniej ok. 60% wysokości okna.
  const box = await graphPanel.locator(".wf-canvas-scroll").boundingBox();
  const height = page.viewportSize()?.height ?? 0;
  expect(box?.height ?? 0).toBeGreaterThanOrEqual(height * 0.59);
  // Etykieta statusu węzła ma rozmiar jak reszta interfejsu (nie 11 px).
  const font = await graph
    .locator(".wf-node-status")
    .first()
    .evaluate((el) => Number.parseFloat(getComputedStyle(el).fontSize));
  expect(font).toBeGreaterThanOrEqual(13);

  // Skróty z modyfikatorem (np. Ctrl+strzałka) nie przełączają zakładki.
  await page.keyboard.press("Control+ArrowLeft");
  await expect(graphTab).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Alt+Home");
  await expect(graphTab).toHaveAttribute("aria-selected", "true");

  await page.keyboard.press("ArrowLeft");
  await expect(runTab).toBeFocused();
  await expect(runTab).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("#wf-monitor-panel-graph")).toHaveCount(0);
  await page.keyboard.press("End");
  await expect(graphTab).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Home");
  await expect(runTab).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeVisible();
});

/** Szablon z węzłami ułożonymi ciasno (szerokość ok. 700 px): mieści się z zapasem. */
function compactGraph() {
  const graph = analysisImplementationReview();
  graph.nodes.forEach((n, i) => {
    n.position = { x: (i % 3) * 220, y: Math.floor(i / 3) * 120 };
  });
  return graph;
}

test("zakładka Graf: graf węższy niż widok powiększa się do szerokości, szerszy zostaje na 100% z przewijaniem", async ({
  page,
}) => {
  // Treść widoku ma stałą maksymalną szerokość (1120 px), więc o powiększeniu
  // decyduje szerokość grafu, nie okna: ciasny graf dostaje > 100%.
  await openRuns(page, {
    ...approvalFixture(),
    details: { 1: detail(1, { graph: compactGraph() }) },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await monitorTabs(page).getByRole("tab", { name: "Graf" }).click();
  const zoom = page.getByRole("tabpanel", { name: "Graf" }).locator(".wf-zoom");
  const wide = await expectedGraphZoom(page);
  expect(wide).toBeGreaterThan(100);
  await expect(zoom).toHaveText(`${wide}%`);
  const fits = await page
    .locator(".wf-canvas.large .wf-canvas-scroll")
    .evaluate((el) => el.scrollWidth <= el.clientWidth);
  expect(fits).toBe(true);
});

test("zakładka Graf: w wąskim oknie graf szablonu zostaje na 100% i przewija się poziomo", async ({
  page,
}) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await monitorTabs(page).getByRole("tab", { name: "Graf" }).click();
  const zoom = page.getByRole("tabpanel", { name: "Graf" }).locator(".wf-zoom");
  expect(await expectedGraphZoom(page)).toBe(100);
  await expect(zoom).toHaveText("100%");
  const scrolls = await page
    .locator(".wf-canvas.large .wf-canvas-scroll")
    .evaluate((el) => el.scrollWidth > el.clientWidth);
  expect(scrolls).toBe(true);
  // Dopasowanie nie schodzi poniżej 100% także po ręcznym pomniejszeniu.
  await page.getByRole("button", { name: "Pomniejsz" }).click();
  await expect(zoom).toHaveText("80%");
  await page.getByRole("button", { name: "Dopasuj widok" }).click();
  await expect(zoom).toHaveText("100%");
});

test("zdarzenie na żywo zmienia status węzła w zakładce Graf", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await monitorTabs(page).getByRole("tab", { name: "Graf" }).click();
  const graph = page.getByRole("img", { name: /Graf procesu/ });
  await expect(graph).toHaveAttribute("aria-label", /Implementacja: nie uruchomiony/);
  // Backend zapisał nowy krok; zdarzenie `nodeStateChanged` każe monitorowi odczytać szczegóły.
  await page.evaluate(() => {
    const state = (window as any).__workflowRunsState;
    state.details[1].executions.push({
      id: "e-9",
      nodeId: "implementation",
      iteration: 1,
      attempt: 1,
      status: "running",
      startedAt: 1,
      finishedAt: null,
      exitCode: null,
      errorCode: null,
      error: null,
      output: null,
      providerSessionId: null,
    });
    state.details[1].revision += 1;
  });
  await emitEvent(page, ev(4, "nodeStateChanged", { status: "running" }));
  await expect(graph).toHaveAttribute("aria-label", /Implementacja: W toku/);
  await expect(graph.locator(".wf-node-status").filter({ hasText: "W toku" })).toHaveCount(1);
});

/**
 * Prawdziwy kształt zdarzenia hosta: `StoredEvent.payload` to zserializowany
 * `RunEvent` (`ports.rs`), więc komunikat runnera ma tekst w `payload.payload.text`
 * (`{"type":"message","executionId":…,"payload":{"text":…}}`), a narzędzie w
 * `payload.payload.{phase,name}`.
 */
function hostMessage(sequence: number, text: string) {
  return {
    runId: 1,
    sequence,
    executionId: "e-1",
    type: "message",
    timestamp: 1_700_000_000_000 + sequence,
    payload: { type: "message", executionId: "e-1", payload: { text } },
  };
}

test("dziennik agentów pokazuje tekst komunikatu z prawdziwego kształtu zdarzenia (nie JSON)", async ({
  page,
}) => {
  await openRuns(page, {
    ...approvalFixture(),
    events: {
      1: [
        hostMessage(1, "Analizuję repozytorium."),
        {
          ...hostMessage(2, ""),
          type: "toolStarted",
          payload: {
            type: "tool",
            executionId: "e-1",
            payload: { phase: "started", name: "Read" },
          },
        },
        // Znacznik przycięcia: komunikat bez tekstu (`driver/messages.rs`).
        {
          ...hostMessage(3, ""),
          payload: {
            type: "message",
            executionId: "e-1",
            payload: { truncated: true, originalBytes: 70000 },
          },
        },
      ],
    },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  const log = page.getByRole("list", { name: "Dziennik agentów" });
  await expect(log.getByRole("listitem")).toHaveCount(2);
  await expect(log.getByRole("listitem").first()).toHaveText("Analizuję repozytorium.");
  await expect(log).not.toContainText("{");
  await expect(log).not.toContainText("executionId");
  await expect(log.getByRole("listitem").nth(1)).toHaveText(
    "[komunikat przycięty: 70000 B]",
  );
  await emitEvent(page, hostMessage(4, "Poprawiam operator odejmowania."));
  await expect(log.getByRole("listitem").nth(2)).toHaveText(
    "Poprawiam operator odejmowania.",
  );
  await expect(log).not.toContainText("{");
});

const agentsNav = (page: Page) =>
  nav(page).getByRole("button", { name: /^Sesje agentów/ });

test("tablica agentów pokazuje aktywny przebieg i otwiera monitor", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: {
      ...approvalFixture(),
      runs: [summary(1), summary(2, { status: "succeeded", waitingApproval: false })],
    },
  });
  await agentsNav(page).click();
  const section = page.getByRole("region", { name: /Przebiegi procesów w toku/ });
  await expect(section).toContainText("Czeka na zgodę");
  await expect(
    section.getByRole("button", { name: "Otwórz przebieg 1" }),
  ).toBeVisible();
  await expect(
    section.getByRole("button", { name: "Otwórz przebieg 2" }),
  ).toHaveCount(0);
  await section.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Czeka na zgodę");
});

test("zdarzenie końca przebiegu usuwa go z sekcji aktywnych", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: { ...approvalFixture(), runs: [summary(1)] },
  });
  await agentsNav(page).click();
  const section = page.getByRole("region", { name: /Przebiegi procesów w toku/ });
  await expect(section).toBeVisible();
  await page.evaluate(() => {
    (window as any).__workflowRunsState.runs[0].status = "succeeded";
  });
  await emitEvent(
    page,
    ev(4, "runStateChanged", { from: "waitingForApproval", to: "succeeded" }),
  );
  await expect(section).toHaveCount(0);
});

test("tablica agentów bez klucza i bez aktywnych przebiegów nie ma sekcji", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: false },
    workflowRuns: {
      ...approvalFixture(),
      runs: [summary(1, { status: "succeeded", waitingApproval: false })],
    },
  });
  await agentsNav(page).click();
  await expect(page.getByRole("heading", { name: /Sesje agentów/ })).toBeVisible();
  await expect(
    page.getByRole("region", { name: /Przebiegi procesów w toku/ }),
  ).toHaveCount(0);
});

// Odzyskiwanie dostępu (DoD 13 pkt 3): bez klucza aktywny przebieg widać na
// tablicy agentów i można go zatrzymać, ale nie da się otworzyć monitora.
test("tablica agentów bez klucza pozwala zatrzymać aktywny przebieg", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: false },
    workflowRuns: { ...approvalFixture(), runs: [summary(1)] },
  });
  await agentsNav(page).click();
  const section = page.getByRole("region", { name: /Przebiegi procesów w toku/ });
  await expect(section).toContainText("Czeka na zgodę");
  await expect(section).toContainText("przebieg można tylko zatrzymać");
  await expect(section.getByRole("button", { name: "Otwórz przebieg 1" })).toHaveCount(0);
  await section.getByRole("button", { name: "Zatrzymaj przebieg 1" }).click();
  await page
    .getByRole("dialog", { name: "Zatrzymać przebieg?" })
    .getByRole("button", { name: "Zatrzymaj" })
    .click();
  await expect(section.getByRole("status")).toContainText("Zatrzymywanie przebiegu 1");
  expect(await page.evaluate(() => (window as any).__workflowCancels)).toBe(1);
});

test("komunikaty agentów nie odświeżają listy na tablicy agentów", async ({
  page,
}) => {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: { ...approvalFixture(), runs: [summary(1)] },
  });
  await agentsNav(page).click();
  await expect(
    page.getByRole("region", { name: /Przebiegi procesów w toku/ }),
  ).toBeVisible();
  const listCalls = async () =>
    (await mockCalls(page)).filter((c: any) => c.cmd === "workflow_runs").length;
  const before = await listCalls();
  for (let i = 4; i < 24; i++) await emitEvent(page, ev(i));
  await emitEvent(page, ev(24, "runStateChanged", { from: "running", to: "waitingForApproval" }));
  await expect.poll(listCalls).toBe(before + 1);
});

test("przycięty diff nie pozwala zaakceptować, tylko odrzucić", async ({
  page,
}) => {
  await openRuns(page, {
    ...approvalFixture(),
    diffs: { [HASH]: { ...DIFF, truncated: true, size: 3 * 1024 * 1024 } },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByLabel("Zmiany do zaakceptowania")).toContainText("+poprawka");
  await expect(page.getByText("Tych zmian nie można zaakceptować")).toBeVisible();
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Odrzuć" })).toBeEnabled();
});

test("diff z ukrytymi sekretami pokazuje jawną informację", async ({ page }) => {
  await openRuns(page, {
    ...approvalFixture(),
    diffs: { [HASH]: { ...DIFF, text: "+KEY=[ukryte]\n", redacted: 1 } },
  });
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByText("Ukryte fragmenty przypominające sekrety: 1")).toBeVisible();
  await expect(page.getByRole("button", { name: "Akceptuj" })).toBeEnabled();
});

test("komunikaty nie czytają szczegółów, a starsza odpowiedź nie nadpisuje nowszej", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(page.getByLabel("Zmiany do zaakceptowania")).toBeVisible();
  const getCalls = async () =>
    (await mockCalls(page)).filter((c: any) => c.cmd === "workflow_run_get").length;
  const before = await getCalls();
  for (let i = 4; i < 30; i++) await emitEvent(page, ev(i));
  await expect(page.getByText("linia 29")).toBeVisible();
  expect(await getCalls()).toBe(before);

  // Odczyt A (rewizja 3) wisi; odczyt B (rewizja 30) wraca pierwszy.
  await page.evaluate(() => {
    const w = window as any;
    w.__runGetHold = new Promise((resolve) => (w.__releaseRunGet = resolve));
  });
  await emitEvent(page, ev(30, "nodeStateChanged"));
  await expect.poll(getCalls).toBe(before + 1);
  await page.evaluate(() => {
    const w = window as any;
    w.__runGetHold = null;
    w.__workflowRunsState.details[1].revision = 31;
  });
  await emitEvent(page, ev(31, "runStateChanged", { from: "running", to: "waitingForApproval" }));
  await expect.poll(getCalls).toBe(before + 2);
  await page.evaluate(() => (window as any).__releaseRunGet());
  await page.getByRole("button", { name: "Akceptuj" }).click();
  await expect(page.getByRole("status").first()).toContainText("Zaakceptowano");
  const decide = (await mockCalls(page)).find((c: any) => c.cmd === "workflow_decide");
  expect(decide.args.command.expectedRevision).toBe(31);
});

test("filtr repozytorium nie zależy od wielkości liter i separatora", async ({
  page,
}) => {
  await openRuns(page, approvalFixture());
  const repo = page.getByLabel("Repozytorium");
  await repo.fill("c:/REPO/");
  await expect(page.getByRole("button", { name: "Otwórz przebieg 1" })).toBeVisible();
  await repo.fill("C:/rep%");
  await expect(page.getByRole("button", { name: "Otwórz przebieg 1" })).toHaveCount(0);
});

// E7: otwarte z E5 — podwójne kliknięcie Uruchom w formularzu z karty zadania.
test("karta zadania: podwójne kliknięcie Uruchom wysyła jeden requestId i tworzy jeden przebieg", async ({
  page,
}) => {
  await openStartable(page);
  await taskCard(page).click();
  await taskActions(page)
    .getByRole("button", { name: "Uruchom proces: Analiza importu" })
    .click();
  const dialog = page.getByRole("dialog");
  await dialog.getByLabel("Proces").selectOption({ label: "Proces E5 (v2)" });
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByRole("button", { name: "Uruchom", exact: true }).dblclick();
  await expect(page.getByTestId("run-status")).toBeVisible();
  const sent = await starts(page);
  expect(sent.length).toBeGreaterThanOrEqual(1);
  expect(new Set(sent.map((s: any) => s.requestId)).size).toBe(1);
  expect(sent[0].repo).toBe("C:\\repo");
  const created = await page.evaluate(
    () => Object.keys((window as any).__workflowRequestIds ?? {}).length,
  );
  expect(created).toBe(1);
});

// E7: śledzenie bez klucza i po wyłączeniu modułu: żadnych komend z listy KEYED,
// start i stop pomiaru działają tak samo (DoD 1).
const KEYLESS_WORKFLOW = new Set([
  "workflow_access",
  "workflow_unlock",
  "workflow_lock",
  "workflow_runs",
  "workflow_run_get",
  "workflow_run_events",
  "workflow_cancel",
  "workflow_recover",
  "workflow_faults",
]);

async function walkMenuAndMeasure(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  const before = (await mockCalls(page)).length;
  const items = nav(page).getByRole("button");
  const count = await items.count();
  for (let i = 0; i < count; i++) {
    await items.nth(i).click();
    await page.waitForTimeout(50);
  }
  await nav(page).getByRole("button", { name: "Zadania" }).click();
  await taskCard(page).click();
  await page.getByRole("button", { name: "Start: Analiza importu" }).click();
  await page.getByRole("button", { name: "Stop: Analiza importu" }).click();
  await expect(
    page.getByRole("button", { name: "Start: Analiza importu" }),
  ).toBeVisible();
  const calls = (await mockCalls(page)).slice(before);
  const keyed = calls
    .map((c: any) => c.cmd as string)
    .filter((c) => c.startsWith("workflow_") && !KEYLESS_WORKFLOW.has(c));
  return { errors, keyed, cmds: calls.map((c: any) => c.cmd as string) };
}

test("bez klucza nawigacja i pomiar nie wołają komend Workflows z kluczem", async ({
  page,
}) => {
  await openDesktop(page, { workflows: { unlocked: false } });
  const result = await walkMenuAndMeasure(page);
  expect(result.errors).toEqual([]);
  expect(result.keyed).toEqual([]);
});

test("po wyłączeniu modułu pomiar daje ten sam wynik co bez klucza", async ({
  page,
}) => {
  await openDesktop(page, { workflows: { unlocked: true } });
  await page.evaluate(async () => {
    const m: any = await import(/* @vite-ignore */ "/src/workflowAccess.ts");
    await m.lockModule();
  });
  const result = await walkMenuAndMeasure(page);
  expect(result.errors).toEqual([]);
  expect(result.keyed).toEqual([]);
});
