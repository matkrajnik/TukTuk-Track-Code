import { test, expect, type Page } from "./fixtures/test";
import { openDesktop } from "./fixtures/tauri";
import { analysisImplementationReview } from "../src/api/workflowTemplates";

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
