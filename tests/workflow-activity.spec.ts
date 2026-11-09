import { test, expect, type Page } from "./fixtures/test";
import { openPreview } from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";
import {
  analysisImplementationReview,
  dynamicDefault,
} from "../src/api/workflowTemplates";

/**
 * Workflows E8.1: dziennik agentów z czytelnymi liniami narzędzi. Zdarzenia mają
 * kształt hosta (`StoredEvent.payload` = `RunEvent`, dane w `payload.payload`).
 * Żaden test nie łączy się z prawdziwymi agentami.
 */

const HASH = "b".repeat(64);
const emitEvent = (page: Page, event: object) =>
  page.evaluate(
    (e) => (window as any).__tauriMock.emit("workflow-event", e),
    event,
  );
const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });

const summary = {
  id: 1,
  workflowId: 1,
  workflowName: "Proces testowy",
  version: 1,
  status: "waitingForApproval",
  taskId: null,
  taskTitle: null,
  taskExists: false,
  repo: "C:/repo",
  createdAt: 1_700_000_000_001,
  startedAt: null,
  finishedAt: null,
  errorCode: null,
  waitingApproval: true,
};

const detail = {
  summary,
  goal: "Cel",
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
};

function tool(
  sequence: number,
  phase: "started" | "finished",
  name: string,
  extra: Record<string, unknown> = {},
) {
  return {
    runId: 1,
    sequence,
    executionId: "e-1",
    type: phase === "started" ? "toolStarted" : "toolFinished",
    timestamp: 1_700_000_000_000 + sequence,
    payload: {
      type: "tool",
      executionId: "e-1",
      payload: { phase, name, ...extra },
    },
  };
}

function plain(sequence: number, type: string, payload: object = {}) {
  return {
    runId: 1,
    sequence,
    executionId: "e-1",
    type,
    timestamp: 1_700_000_000_000 + sequence,
    payload: { type, ...payload },
  };
}

async function openRun(page: Page, events: object[]) {
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: {
      runs: [summary],
      details: { 1: detail },
      events: { 1: events },
      diffs: {
        [HASH]: { text: "+x\n", truncated: false, size: 3, hash: HASH },
      },
    },
  });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
}

test("dziennik pokazuje czytelne linie narzędzi z prawdziwym kształtem zdarzeń", async ({
  page,
}) => {
  await openRun(page, [
    tool(1, "started", "Read", {
      toolUseId: "t1",
      args: { file_path: "src/a.ts" },
    }),
    tool(2, "finished", "Read", { toolUseId: "t1", ok: true }),
    tool(3, "started", "Grep", {
      toolUseId: "t2",
      args: { pattern: "foo", path: "src" },
    }),
    tool(4, "finished", "Grep", { toolUseId: "t2", ok: true }),
    tool(5, "started", "Edit", {
      toolUseId: "t3",
      args: { file_path: "src/a.ts" },
    }),
    tool(6, "finished", "Edit", { toolUseId: "t3", ok: false }),
    tool(7, "started", "Agent", {
      toolUseId: "t4",
      args: { description: "Przeczytaj a.txt", subagent_type: "general-purpose" },
    }),
    tool(8, "started", "Read", {
      toolUseId: "t5",
      parentToolUseId: "t4",
      args: { file_path: "x.txt", outside: true },
    }),
  ]);
  const log = page.getByRole("list", { name: "Dziennik agentów" });
  const items = log.getByRole("listitem");
  await expect(items).toHaveText([
    "Czyta src/a.ts",
    "Szuka „foo” w src",
    "Edytuje src/a.ts — błąd",
    "Uruchamia subagenta: Przeczytaj a.txt",
    "Czyta x.txt (poza katalogiem roboczym)",
  ]);
  await expect(log).not.toContainText("{");
  await expect(log).not.toContainText("toolUseId");
  await expect(log).not.toContainText("phase");
  await emitEvent(
    page,
    tool(9, "started", "Write", {
      toolUseId: "t6",
      args: { file_path: "src/b.ts" },
    }),
  );
  await expect(items).toHaveCount(6);
  await expect(items.nth(5)).toHaveText("Zapisuje src/b.ts");
});

test("oś zdarzeń nie pokazuje już zdarzeń narzędzi", async ({ page }) => {
  await openRun(page, [
    plain(1, "nodeStateChanged", { status: "completed" }),
    tool(2, "started", "Read", { toolUseId: "t1" }),
    tool(3, "finished", "Read", { toolUseId: "t1", ok: true }),
  ]);
  const timeline = page.getByRole("list", { name: "Oś zdarzeń" });
  await expect(timeline).toContainText("nodeStateChanged");
  await expect(timeline).not.toContainText("toolStarted");
  await expect(timeline).not.toContainText("toolFinished");
});

test("stara historia bez identyfikatorów narzędzi wyświetla się bez błędu", async ({
  page,
}) => {
  await openRun(page, [
    tool(1, "started", "Read"),
    tool(2, "finished", "Read", { ok: false }),
    tool(3, "started", "mcp__x__y"),
    // Start bez id i wynik z id (jak stos zapasowy runnera): jedna linia.
    tool(4, "started", "Read"),
    tool(5, "finished", "Read", { toolUseId: "t9", ok: false }),
    // Zakończenie bez sparowanego startu i bez nazwy: bez podwójnej spacji.
    tool(6, "finished", "", { toolUseId: "t10", ok: false }),
  ]);
  const log = page.getByRole("list", { name: "Dziennik agentów" });
  await expect(log.getByRole("listitem")).toHaveText([
    "Czyta plik — błąd",
    "Narzędzie mcp__x__y",
    "Czyta plik — błąd",
    "Narzędzie — błąd",
  ]);
  await expect(page.getByRole("alert")).toHaveCount(0);
});

// ---------------------------------------------------------------------------
// E8.6: drzewo orkiestrator → subagenci (zdarzenia w kształcie z `_parallel.jsonl`).
// Claude Code sam przenosi równoległych subagentów w tło: ich `tool_result`
// („Async agent launched”) przychodzi od razu, ale host nie zapisuje go jako
// końca (`runners/mod.rs`); `toolFinished` subagenta w tle pochodzi dopiero
// z `system/task_notification`.

const orchestratorExecution = {
  id: "e-1",
  nodeId: "orchestrator",
  iteration: 1,
  attempt: 1,
  status: "running",
  startedAt: 1_700_000_000_000,
  finishedAt: null,
  exitCode: null,
  errorCode: null,
  error: null,
  output: null,
  providerSessionId: null,
};

async function openDynamicRun(
  page: Page,
  events: object[],
  graph = dynamicDefault(),
  run: { runStatus: string; executionStatus: string } = {
    runStatus: "running",
    executionStatus: "running",
  },
) {
  const runSummary = { ...summary, status: run.runStatus, waitingApproval: false };
  await openDesktop(page, {
    workflows: { unlocked: true },
    workflowRuns: {
      runs: [runSummary],
      details: {
        1: {
          ...detail,
          summary: runSummary,
          graph,
          executions: [{ ...orchestratorExecution, status: run.executionStatus }],
          approvals: [],
        },
      },
      events: { 1: events },
    },
  });
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
}

const agentStart = (seq: number, id: string, description: string, name = "Agent") =>
  tool(seq, "started", name, {
    toolUseId: id,
    args: { description, subagent_type: "general-purpose" },
  });

const tree = (page: Page) => page.getByRole("region", { name: /^Drzewo agentów/ });

const parallelEvents = () => [
  agentStart(1, "toolu_agent1", "Agent 1: liczenie"),
  agentStart(2, "toolu_agent2", "Agent 2: liczenie"),
  agentStart(3, "toolu_agent3", "Agent 3: liczenie"),
  tool(4, "started", "Glob", {
    toolUseId: "toolu_glob1",
    parentToolUseId: "toolu_agent1",
    args: { pattern: "e-data/**/*" },
  }),
  tool(5, "started", "Read", {
    toolUseId: "toolu_read1",
    parentToolUseId: "toolu_agent2",
    args: { file_path: "e-data/plik1.txt" },
  }),
];

test("monitor przebiegu dynamicznego pokazuje drzewo orchestratora i subagentów z licznikiem", async ({
  page,
}) => {
  await openDynamicRun(page, parallelEvents());
  const t = tree(page);
  await expect(t).toBeVisible();
  // Start w tle („Async agent launched”) nie kończy subagenta: wszyscy trzej pracują.
  await expect(t).toContainText("Subagenci: 3 aktywnych / 3 łącznie");
  await expect(t.getByText("Agent 1: liczenie")).toBeVisible();
  await expect(t.getByText("Agent 2: liczenie")).toBeVisible();
  await expect(t.getByText("Agent 3: liczenie")).toBeVisible();
  const second = t.locator("li[data-state]").filter({ hasText: "Agent 2: liczenie" });
  await expect(second).toContainText("Czyta e-data/plik1.txt");
  await expect(second).toContainText("w toku");
  await expect(
    t.locator("li[data-state]").filter({ hasText: "Agent 1: liczenie" }),
  ).toHaveAttribute("data-state", "running");
});

test("bieżąca czynność subagenta zmienia się po zdarzeniu na żywo", async ({ page }) => {
  await openDynamicRun(page, parallelEvents());
  const item = tree(page).locator("li[data-state]").filter({ hasText: "Agent 3: liczenie" });
  await expect(item).not.toContainText("Szuka");
  await emitEvent(
    page,
    tool(6, "started", "Grep", {
      toolUseId: "toolu_grep1",
      parentToolUseId: "toolu_agent3",
      args: { pattern: "linia" },
    }),
  );
  await expect(item).toContainText("Szuka „linia”");
  // Koniec z `task_notification` (host zapisuje go jako `toolFinished` subagenta).
  await emitEvent(page, tool(7, "finished", "Agent", { toolUseId: "toolu_agent3", ok: true }));
  await expect(tree(page)).toContainText("Subagenci: 2 aktywnych / 3 łącznie");
});

test("subagent z błędem narzędzia i zakończony subagent mają stan tekstem", async ({
  page,
}) => {
  await openDynamicRun(page, [
    agentStart(1, "toolu_agent1", "Pierwszy"),
    agentStart(2, "toolu_agent2", "Drugi"),
    tool(3, "started", "Read", {
      toolUseId: "toolu_read1",
      parentToolUseId: "toolu_agent1",
      args: { file_path: "brak.txt" },
    }),
    tool(4, "finished", "Read", { toolUseId: "toolu_read1", ok: false }),
    tool(5, "finished", "Agent", { toolUseId: "toolu_agent1", ok: false }),
    tool(6, "finished", "Agent", { toolUseId: "toolu_agent2", ok: true }),
  ]);
  const t = tree(page);
  const first = t.locator("li[data-state]").filter({ hasText: "Pierwszy" });
  await expect(first).toContainText("błąd");
  await expect(first).toContainText("Czyta brak.txt — błąd");
  const second = t.locator("li[data-state]").filter({ hasText: "Drugi" });
  await expect(second).toContainText("zakończony");
  await expect(t).toContainText("Subagenci: 0 aktywnych / 2 łącznie");
});

test("przekroczony limit zdarzeń kroku pokazuje uczciwą informację", async ({ page }) => {
  await openDynamicRun(page, [
    agentStart(1, "toolu_agent1", "Pierwszy"),
    plain(2, "message", {
      executionId: "e-1",
      payload: { truncated: true, reason: "limit zdarzeń kroku" },
    }),
  ]);
  await expect(tree(page).getByRole("note")).toHaveText(
    "Osiągnięto limit zapisanych zdarzeń kroku; bieżące czynności nie są już aktualizowane.",
  );
});

test("anulowany orkiestrator z trzema rozpoczętymi subagentami nie pokazuje aktywnych", async ({
  page,
}) => {
  await openDynamicRun(
    page,
    [
      agentStart(1, "toolu_agent1", "Agent 1: liczenie"),
      agentStart(2, "toolu_agent2", "Agent 2: liczenie"),
      agentStart(3, "toolu_agent3", "Agent 3: liczenie"),
    ],
    dynamicDefault(),
    { runStatus: "cancelled", executionStatus: "cancelled" },
  );
  const t = tree(page);
  await expect(t).toContainText("Subagenci: 0 aktywnych / 3 łącznie");
  await expect(t.locator('li[data-state="interrupted"]')).toHaveCount(3);
  await expect(t.locator('li[data-state="running"]')).toHaveCount(0);
  await expect(
    t.locator("li[data-state]").filter({ hasText: "Agent 2: liczenie" }),
  ).toContainText("przerwany");
  await expect(t.locator("li[data-orchestrator-state]")).toHaveAttribute(
    "data-orchestrator-state",
    "interrupted",
  );
});

test("zakończony krok z uciętym ogonem zdarzeń nie zostawia subagentów w toku", async ({
  page,
}) => {
  await openDynamicRun(
    page,
    [
      ...parallelEvents(),
      // Agent 1 skończył (`task_notification` → `toolFinished`), reszta nie zdążyła.
      tool(6, "finished", "Agent", { toolUseId: "toolu_agent1", ok: true }),
      plain(7, "message", {
        executionId: "e-1",
        payload: { truncated: true, reason: "limit zdarzeń kroku" },
      }),
    ],
    dynamicDefault(),
    { runStatus: "succeeded", executionStatus: "completed" },
  );
  const t = tree(page);
  await expect(t).toContainText("Subagenci: 0 aktywnych / 3 łącznie");
  // Agent 1 ma własny koniec, więc zostaje „zakończony”; reszta jest przerwana.
  await expect(
    t.locator("li[data-state]").filter({ hasText: "Agent 1: liczenie" }),
  ).toHaveAttribute("data-state", "done");
  await expect(t.locator('li[data-state="interrupted"]')).toHaveCount(2);
  await expect(t.locator("li[data-orchestrator-state]")).toHaveAttribute(
    "data-orchestrator-state",
    "done",
  );
});

test("orkiestrator w toku ma stan „w toku”", async ({ page }) => {
  await openDynamicRun(page, parallelEvents());
  await expect(tree(page).locator("li[data-orchestrator-state]")).toHaveAttribute(
    "data-orchestrator-state",
    "running",
  );
});

test("drzewo po ponownym otwarciu odtwarza się ze zdarzeń", async ({ page }) => {
  await openDynamicRun(page, parallelEvents());
  await expect(tree(page)).toContainText("Subagenci: 3 aktywnych / 3 łącznie");
  await page.getByRole("button", { name: "Uruchomienia", exact: true }).first().click();
  await nav(page).getByRole("button", { name: "Dashboard" }).click();
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 1" }).click();
  await expect(tree(page)).toContainText("Subagenci: 3 aktywnych / 3 łącznie");
  await expect(tree(page).getByText("Agent 2: liczenie")).toBeVisible();
});

test("zwykły przebieg nie pokazuje drzewa", async ({ page }) => {
  await openRun(page, [tool(1, "started", "Read", { toolUseId: "t1" })]);
  await expect(page.getByRole("region", { name: /^Drzewo agentów/ })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "Drzewo agentów" })).toHaveCount(0);
});

test("narzędzie Task (stara nazwa) też tworzy subagenta", async ({ page }) => {
  await openDynamicRun(page, [
    agentStart(1, "toolu_task1", "Stary subagent", "Task"),
    tool(2, "started", "Read", {
      toolUseId: "toolu_read1",
      parentToolUseId: "toolu_task1",
      args: { file_path: "a.txt" },
    }),
  ]);
  const t = tree(page);
  await expect(t).toContainText("Subagenci: 1 aktywnych / 1 łącznie");
  await expect(t.locator("li[data-state]").filter({ hasText: "Stary subagent" })).toContainText(
    "Czyta a.txt",
  );
});

test("podgląd pokazuje przykładowy przebieg dynamiczny z subagentami", async ({ page }) => {
  await openPreview(page);
  await page.evaluate(() => {
    localStorage.setItem(
      "tuktuk-track-preview-workflows-v1",
      JSON.stringify({ unlocked: true, nextId: 1, workflows: [] }),
    );
  });
  await page.reload();
  await nav(page).getByRole("button", { name: "Uruchomienia" }).click();
  await page.getByRole("button", { name: "Otwórz przebieg 3" }).click();
  await expect(page.getByTestId("run-status")).toContainText("Zakończony");
  const t = tree(page);
  await expect(t).toContainText("Subagenci: 0 aktywnych / 2 łącznie");
  await expect(t.getByText("Sprawdź src/a.ts")).toBeVisible();
  await expect(t.getByText("Sprawdź src/b.ts")).toBeVisible();
  await expect(page.getByRole("heading", { name: /Dynamiczny \(przykład\)/ })).toBeVisible();
});
