import { test, expect, type Page } from "./fixtures/test";
import { validateWorkflow } from "../src/api/workflowValidate";
import {
  analysisImplementationReview,
  blankWorkflow,
} from "../src/api/workflowTemplates";
import type {
  WorkflowEdge,
  WorkflowGraph,
  WorkflowNode,
} from "../src/api/workflows";

/**
 * Moduł Workflows, etap E1 (docs/workflows/PLAN_WDROZENIA.md):
 * - walidator TS (`src/api/workflowValidate.ts`) daje te same kody co
 *   `workflow_core::validation` — przypadki jak w
 *   `src-tauri/crates/workflow-core/tests/validation.rs`;
 * - symulator podglądu (`src/preview/workflows.ts`) zachowuje się jak
 *   `workflows::store`: blokada bez klucza, rewizje, niezmienne wersje,
 *   archiwum, eksport i import.
 * Klucza dostępu nie ma w repozytorium; testy odblokowują podgląd stanem.
 */

const codes = (graph: WorkflowGraph) => validateWorkflow(graph).map((i) => i.code);

function node(graph: WorkflowGraph, id: string): WorkflowNode {
  const found = graph.nodes.find((n) => n.id === id);
  if (!found) throw new Error(`brak węzła ${id}`);
  return found;
}

const edge = (
  id: string,
  source: string,
  sourcePort: WorkflowEdge["sourcePort"],
  target: string,
): WorkflowEdge => ({ id, source, sourcePort, target });

test.describe("walidator jak w Rust", () => {
  test("szablony są poprawne", () => {
    expect(validateWorkflow(analysisImplementationReview())).toEqual([]);
    expect(validateWorkflow(blankWorkflow())).toEqual([]);
  });

  test("zła wersja formatu zatrzymuje dalsze reguły", () => {
    const graph = { ...analysisImplementationReview(), schemaVersion: 99 };
    expect(codes(graph)).toEqual(["unsupportedSchema"]);
  });

  test("brak startu i końca", () => {
    expect(codes({ schemaVersion: 1, nodes: [], edges: [] })).toEqual([
      "missingTrigger",
      "missingEnd",
    ]);
  });

  test("osierocony krok wskazany po identyfikatorze", () => {
    const graph = analysisImplementationReview();
    graph.nodes.push({ ...structuredClone(node(graph, "analysis")), id: "orphan" });
    graph.edges.push(edge("o", "orphan", "out", "done"));
    expect(validateWorkflow(graph)).toContainEqual(
      expect.objectContaining({ code: "unreachable", nodeId: "orphan" }),
    );
  });

  test("brakujące i zdublowane wyjście", () => {
    const graph = analysisImplementationReview();
    graph.edges = graph.edges.filter((e) => e.id !== "approval-rejected");
    graph.edges.push(edge("dup", "analysis", "out", "review"));
    const issues = validateWorkflow(graph);
    expect(issues).toContainEqual(
      expect.objectContaining({ code: "missingConnection", nodeId: "approval" }),
    );
    expect(issues).toContainEqual(
      expect.objectContaining({ code: "duplicateConnection", nodeId: "analysis" }),
    );
  });

  test("pętla bez warunku i pętla przez akceptację są odrzucane", () => {
    const noCondition = analysisImplementationReview();
    noCondition.edges = noCondition.edges.filter(
      (e) => e.id !== "implementation-review",
    );
    noCondition.edges.push(edge("loop", "implementation", "out", "analysis"));
    expect(codes(noCondition)).toContain("invalidCycle");

    const throughApproval = analysisImplementationReview();
    throughApproval.edges = throughApproval.edges.filter(
      (e) => e.id !== "approval-rejected",
    );
    throughApproval.edges.push(edge("again", "approval", "reject", "fix"));
    expect(codes(throughApproval)).toContain("invalidCycle");
  });

  test("limit rund poprawek 1–3 tylko w pętli", () => {
    const graph = analysisImplementationReview();
    const decision = node(graph, "decision").kind;
    if (decision.type !== "condition") throw new Error("warunek");
    decision.repairLimit = 4;
    expect(codes(graph)).toContain("invalidRepairLimit");

    const outside = analysisImplementationReview();
    outside.edges = outside.edges.filter((e) => e.id !== "fix-review");
    outside.edges.push(edge("fix-end", "fix", "out", "rejected"));
    expect(codes(outside)).toContain("repairLimitOutsideLoop");
  });

  test("review nie może zapisywać, agent potrzebuje instrukcji i limitu", () => {
    const graph = analysisImplementationReview();
    const review = node(graph, "review").kind;
    const analysis = node(graph, "analysis").kind;
    if (review.type !== "agent" || analysis.type !== "agent")
      throw new Error("agent");
    review.access = "workspaceWrite";
    analysis.instruction = " ";
    analysis.timeoutSecs = 5;
    analysis.model = " ";
    // Kolejność jak w Rust: węzły po kolei, w agencie instrukcja, model, limit, dostęp.
    expect(codes(graph)).toEqual([
      "emptyInstruction",
      "emptyModel",
      "invalidTimeout",
      "accessNotAllowed",
    ]);
    expect(validateWorkflow(graph)).toContainEqual(
      expect.objectContaining({ code: "accessNotAllowed", nodeId: "review" }),
    );
  });

  test("warunek sprawdza pole, operator i typ wartości", () => {
    const graph = analysisImplementationReview();
    const decision = node(graph, "decision").kind;
    if (decision.type !== "condition") throw new Error("warunek");
    decision.value = "maybe";
    expect(codes(graph)).toContain("typeMismatch");
    decision.value = "approved";
    decision.op = "greaterThan";
    expect(codes(graph)).toContain("invalidOperator");
    decision.op = "equals";
    decision.field = { nodeId: "approval", field: "verdict" };
    expect(codes(graph)).toContain("invalidFieldRef");
    decision.field = { nodeId: "review", field: "findingCount" };
    decision.op = "lessThan";
    decision.value = 1;
    expect(validateWorkflow(graph)).toEqual([]);
  });

  test("warunek nie czyta wyniku kroku spoza każdej drogi do niego", () => {
    const graph = analysisImplementationReview();
    const decision = node(graph, "decision").kind;
    if (decision.type !== "condition") throw new Error("warunek");
    // Poprawki leżą za warunkiem w pętli: w pierwszej rundzie nie mają wyniku.
    decision.field = { nodeId: "fix", field: "changedFileCount" };
    decision.op = "greaterThan";
    decision.value = 0;
    expect(validateWorkflow(graph)).toContainEqual(
      expect.objectContaining({ code: "fieldNotAvailable", nodeId: "decision" }),
    );
  });

  test("komunikaty po polsku jak w Rust", () => {
    const graph = analysisImplementationReview();
    graph.edges = graph.edges.filter((e) => e.id !== "approval-rejected");
    expect(validateWorkflow(graph)).toContainEqual({
      code: "missingConnection",
      nodeId: "approval",
      message:
        "Wyjście „odrzucenie” kroku „Decyzja użytkownika” nie jest połączone.",
    });
  });
});

const WORKFLOWS_KEY = "tuktuk-track-preview-workflows-v1";

async function openWorkflowsPreview(page: Page, unlocked: boolean) {
  await page.goto("/");
  await page.evaluate(
    ([key, unlocked]) => {
      localStorage.clear();
      if (unlocked)
        localStorage.setItem(
          key as string,
          JSON.stringify({ unlocked: true, nextId: 1, workflows: [] }),
        );
    },
    [WORKFLOWS_KEY, unlocked] as const,
  );
}

/** Wywołuje funkcję z `src/workflows.ts` w oknie podglądu. */
function call(page: Page, name: string, ...args: unknown[]): Promise<unknown> {
  return page.evaluate(
    async ([name, args]) => {
      const module: Record<string, (...a: unknown[]) => Promise<unknown>> =
        await import(/* @vite-ignore */ "/src/workflows.ts");
      try {
        return { ok: await module[name as string](...(args as unknown[])) };
      } catch (e) {
        return { error: e instanceof Error ? e.message : String(e) };
      }
    },
    [name, args] as const,
  );
}

test.describe("podgląd modułu Workflows", () => {
  test("bez odblokowania moduł odmawia, błędny klucz nie odblokowuje", async ({
    page,
  }) => {
    await openWorkflowsPreview(page, false);
    expect(await call(page, "workflowAccess")).toEqual({
      ok: { unlocked: false },
    });
    expect(await call(page, "listWorkflows")).toEqual({
      error:
        "Moduł Workflows nie jest odblokowany. Wpisz klucz dostępu w Ustawieniach.",
    });
    expect(
      await call(page, "unlockWorkflows", "TUKTUK-WF-00000-00000-00000-00000"),
    ).toEqual({ error: "Nieprawidłowy klucz dostępu." });
    expect(await call(page, "workflowAccess")).toEqual({
      ok: { unlocked: false },
    });
  });

  test("szkic, konflikt rewizji, publikacja i archiwum", async ({ page }) => {
    await openWorkflowsPreview(page, true);
    const created = (await call(page, "createWorkflow", "Proces", "Opis", true)) as {
      ok: { id: number; draftRevision: number; issues: unknown[]; draft: WorkflowGraph };
    };
    expect(created.ok.draftRevision).toBe(1);
    expect(created.ok.issues).toEqual([]);
    const id = created.ok.id;

    const saved = (await call(
      page,
      "saveWorkflowDraft",
      id,
      1,
      "Proces 2",
      "",
      created.ok.draft,
    )) as { ok: { draftRevision: number } };
    expect(saved.ok.draftRevision).toBe(2);
    expect(
      await call(page, "saveWorkflowDraft", id, 1, "Inne okno", "", created.ok.draft),
    ).toEqual({
      error:
        "Proces zmienił się w innym oknie. Odśwież go przed zapisem, żeby nie nadpisać zmian.",
    });

    const published = (await call(page, "publishWorkflow", id, 2)) as {
      ok: { publishedVersion: number; unpublishedChanges: boolean; versions: unknown[] };
    };
    expect(published.ok.publishedVersion).toBe(1);
    expect(published.ok.unpublishedChanges).toBe(false);
    // Podwójne kliknięcie nie tworzy drugiej wersji.
    const again = (await call(page, "publishWorkflow", id, 2)) as {
      ok: { versions: unknown[] };
    };
    expect(again.ok.versions).toHaveLength(1);

    await call(page, "archiveWorkflow", id, true);
    expect(await call(page, "listWorkflows")).toEqual({ ok: [] });
    const all = (await call(page, "listWorkflows", true)) as { ok: unknown[] };
    expect(all.ok).toHaveLength(1);
  });

  test("błędny szkic zapisuje się, ale nie publikuje", async ({ page }) => {
    await openWorkflowsPreview(page, true);
    const created = (await call(page, "createWorkflow", "Pusty", "", false)) as {
      ok: { id: number; draft: WorkflowGraph };
    };
    const broken = {
      ...created.ok.draft,
      edges: [] as WorkflowEdge[],
    };
    const saved = (await call(
      page,
      "saveWorkflowDraft",
      created.ok.id,
      1,
      "Pusty",
      "",
      broken,
    )) as { ok: { issues: { code: string }[] } };
    expect(saved.ok.issues.map((i) => i.code)).toContain("missingConnection");
    expect(await call(page, "publishWorkflow", created.ok.id, 2)).toEqual({
      // Wyjście Startu niepołączone, Start bez drogi do Końca, Koniec nieosiągalny.
      error: "Proces ma błędy (3); popraw je przed publikacją.",
    });
  });

  test("eksport i import tworzą nieopublikowany szkic", async ({ page }) => {
    await openWorkflowsPreview(page, true);
    const created = (await call(page, "createWorkflow", "Proces", "", true)) as {
      ok: { id: number };
    };
    await call(page, "publishWorkflow", created.ok.id, 1);
    const text = (await call(page, "exportWorkflow", created.ok.id)) as {
      ok: string;
    };
    const imported = (await call(page, "importWorkflow", text.ok)) as {
      ok: { id: number; versions: unknown[]; draft: WorkflowGraph };
    };
    expect(imported.ok.id).not.toBe(created.ok.id);
    expect(imported.ok.versions).toEqual([]);
    expect(imported.ok.draft).toEqual(analysisImplementationReview());
    expect(await call(page, "importWorkflow", "{}")).toEqual({
      error: "To nie jest poprawny plik procesu TukTuk Track.",
    });
    const foreign = JSON.parse(text.ok);
    foreign.format = "inny";
    expect(await call(page, "importWorkflow", JSON.stringify(foreign))).toEqual({
      error: "Nieobsługiwany format pliku procesu.",
    });
  });

  test("import uzupełnia pola domyślne jak serde, a zły plik niczego nie zapisuje", async ({
    page,
  }) => {
    await openWorkflowsPreview(page, true);
    const graph = analysisImplementationReview() as unknown as {
      nodes: Record<string, unknown>[];
    };
    for (const n of graph.nodes) {
      delete n.position;
      const kind = n.kind as Record<string, unknown>;
      delete kind.model;
      if (kind.type === "condition") kind.repairLimit = 3;
    }
    const file = {
      format: "tuktuk-track-workflow",
      formatVersion: 1,
      name: "Bez pól domyślnych",
      graph: { ...graph, schemaVersion: 1, extra: "pomijane" },
    };
    const imported = (await call(page, "importWorkflow", JSON.stringify(file))) as {
      ok: { draft: WorkflowGraph; issues: unknown[] };
    };
    expect(imported.ok.issues).toEqual([]);
    expect(imported.ok.draft.nodes[0].position).toEqual({ x: 0, y: 0 });
    expect(Object.keys(imported.ok.draft)).toEqual([
      "schemaVersion",
      "nodes",
      "edges",
    ]);

    const broken = structuredClone(file);
    (broken.graph.nodes[1].kind as Record<string, unknown>).type = "loop";
    const result = (await call(page, "importWorkflow", JSON.stringify(broken))) as {
      error: string;
    };
    expect(result.error).toMatch(
      /^To nie jest poprawny plik procesu TukTuk Track: pole graph\.nodes\[1\]\.kind\.type/,
    );
    const all = (await call(page, "listWorkflows", true)) as { ok: unknown[] };
    expect(all.ok).toHaveLength(1);
  });

  test("ten sam graf z inną kolejnością kluczy nie jest zmianą do publikacji", async ({
    page,
  }) => {
    await openWorkflowsPreview(page, true);
    const created = (await call(page, "createWorkflow", "Proces", "", true)) as {
      ok: { id: number; draft: WorkflowGraph };
    };
    await call(page, "publishWorkflow", created.ok.id, 1);
    const reordered = {
      edges: created.ok.draft.edges,
      nodes: created.ok.draft.nodes.map((n) => ({
        kind: n.kind,
        position: n.position,
        label: n.label,
        id: n.id,
      })),
      schemaVersion: 1,
    };
    const saved = (await call(
      page,
      "saveWorkflowDraft",
      created.ok.id,
      1,
      "Proces",
      "",
      reordered,
    )) as { ok: { unpublishedChanges: boolean } };
    expect(saved.ok.unpublishedChanges).toBe(false);
  });
});
