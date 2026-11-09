import { test, expect, type Page } from "./fixtures/test";
import { openPreview } from "./fixtures/seed";
import { openDesktop } from "./fixtures/tauri";
import {
  analysisImplementationReview,
  dynamicDefault,
} from "../src/api/workflowTemplates";
import {
  buildDynamic,
  defaultDynamicSpec,
} from "../src/api/workflowDynamic";

/**
 * Workflows E8b: tryb dynamiczny (orkiestrator Claude z subagentami). Edytor
 * testujemy w podglądzie (`src/preview/workflows.ts`), start i preflight na
 * zastępczym IPC (fixture liczy preflight 1:1 z `preflight.rs`). Żaden test nie
 * łączy się z prawdziwym Claude.
 */

const WORKFLOWS_KEY = "tuktuk-track-preview-workflows-v1";
const NOW_MS = 1_700_000_000_000;
const COST_NOTE =
  "Orkiestrator uruchamia subagentów; każdy zużywa tokeny osobno, więc koszt może być wielokrotnie wyższy niż w zwykłym kroku.";

const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });

async function previewUnlocked(page: Page) {
  await openPreview(page);
  await page.evaluate((key) => {
    localStorage.setItem(
      key,
      JSON.stringify({ unlocked: true, nextId: 1, workflows: [] }),
    );
  }, WORKFLOWS_KEY);
  await page.reload();
}

async function createDynamicInPreview(page: Page) {
  await previewUnlocked(page);
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await page.getByRole("button", { name: "Dynamiczny (Claude)" }).click();
  await page
    .getByRole("dialog", { name: "Nowy proces" })
    .getByRole("button", { name: "Utwórz" })
    .click();
  return page.getByRole("region", { name: "Edytor procesu" });
}

test("nowy proces dynamiczny otwiera formularz zamiast canvasu", async ({ page }) => {
  const editor = await createDynamicInPreview(page);
  const form = editor.getByRole("form", { name: "Proces dynamiczny" });
  await expect(form).toBeVisible();
  await expect(form.getByLabel("Instrukcja orkiestratora")).not.toHaveValue("");
  await expect(form.getByLabel(/Liczba subagentów/)).toHaveValue("3");
  await expect(form.getByLabel("Dodaj review i poprawki")).toBeChecked();
  await expect(editor.getByRole("complementary", { name: "Kroki procesu" })).toHaveCount(0);
  await expect(editor.getByRole("heading", { name: "Walidacja: brak problemów" })).toBeVisible();
});

test("formularz dynamiczny zapisuje i publikuje poprawny graf", async ({ page }) => {
  const editor = await createDynamicInPreview(page);
  const form = editor.getByRole("form", { name: "Proces dynamiczny" });
  await form.getByLabel("Instrukcja orkiestratora").fill("Nowa instrukcja orkiestratora");
  await form.getByLabel(/Liczba subagentów/).fill("5");
  await form.getByLabel(/Liczba subagentów/).blur();
  await editor.getByRole("button", { name: "Zapisz" }).click();
  await editor.getByRole("button", { name: "Opublikuj" }).click();
  await expect(editor.getByText("Opublikowano wersję 1.")).toBeVisible();
  const stored = await page.evaluate(
    (key) => localStorage.getItem(key) ?? "",
    WORKFLOWS_KEY,
  );
  expect(stored).toContain("Nowa instrukcja orkiestratora");
  expect(stored).toContain('"maxSubagents":5');
  expect(stored).toContain('"role":"orchestrator"');
});

test("wyczyszczone pole liczby nie zmienia grafu procesu dynamicznego", async ({ page }) => {
  const editor = await createDynamicInPreview(page);
  const form = editor.getByRole("form", { name: "Proces dynamiczny" });
  for (const label of [/Liczba subagentów/, "Limit czasu (minuty)"]) {
    const field = form.getByLabel(label);
    const before = await field.inputValue();
    await field.fill("");
    await field.blur();
    // Pole wraca do poprzedniej wartości, a walidacja nie zgłasza problemu.
    await expect(field).toHaveValue(before);
  }
  await expect(editor.getByRole("heading", { name: "Walidacja: brak problemów" })).toBeVisible();
  // Graf się nie zmienił, więc nie ma czego zapisać (NaN dałby `maxSubagents: null`).
  await expect(editor.getByRole("button", { name: "Zapisz" })).toBeDisabled();
});

test("review w trybie dynamicznym wymaga dostępu z zapisem", async ({ page }) => {
  const editor = await createDynamicInPreview(page);
  const form = editor.getByRole("form", { name: "Proces dynamiczny" });
  const review = form.getByLabel("Dodaj review i poprawki");
  await expect(review).toBeEnabled();
  await form.getByLabel("Tylko odczyt").check();
  await expect(review).toBeDisabled();
  await expect(review).not.toBeChecked();
  await expect(form).toContainText("Review i poprawki wymagają dostępu z zapisem.");
  await expect(editor.getByRole("heading", { name: "Walidacja: brak problemów" })).toBeVisible();
  await form.getByLabel("Odczyt i zapis w katalogu roboczym").check();
  await expect(review).toBeEnabled();
});

// ---------------------------------------------------------------------------
// Start i preflight (zastępcze IPC)

function dynamicDetail(graph = dynamicDefault()) {
  return {
    id: 5,
    name: "Proces dynamiczny",
    description: "",
    draftRevision: 2,
    createdAt: NOW_MS,
    updatedAt: NOW_MS,
    archivedAt: null,
    publishedVersion: 1,
    publishedAt: NOW_MS,
    unpublishedChanges: false,
    draft: graph,
    issues: [],
    versions: [{ id: 42, version: 1, hash: "h1", createdAt: NOW_MS }],
  };
}

function standardDetail() {
  return {
    ...dynamicDetail(analysisImplementationReview()),
    name: "Proces zwykły",
  };
}

async function openStart(
  page: Page,
  options: {
    detail: ReturnType<typeof dynamicDetail>;
    claude: string;
    subagentsConfirmed?: boolean;
  },
) {
  await openDesktop(page, {
    workflows: {
      unlocked: true,
      detail: options.detail,
      versionGraphs: { 42: options.detail.draft },
    },
    detectedAgents: {
      claude: options.claude,
      codex: "0.163.0",
      codexSandbox: true,
      subagentsConfirmed: options.subagentsConfirmed,
    },
    workflowRuns: { startResult: { runId: 1, created: true }, runs: [], details: {} },
  });
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await page.getByRole("button", { name: `Uruchom ${options.detail.name}` }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("status")).toContainText("Gotowość sprawdzona");
  return dialog;
}

const run = (dialog: ReturnType<Page["getByRole"]>) =>
  dialog.getByRole("button", { name: "Uruchom", exact: true });

test("formularz startu ostrzega o koszcie trybu dynamicznego", async ({ page }) => {
  const dialog = await openStart(page, {
    detail: dynamicDetail(),
    claude: "2.1.319",
  });
  const note = dialog.getByRole("note");
  await expect(note).toHaveText(COST_NOTE);
});

test("zwykły proces nie pokazuje ostrzeżenia o koszcie", async ({ page }) => {
  const dialog = await openStart(page, {
    detail: standardDetail(),
    claude: "2.1.319",
  });
  await expect(dialog.getByRole("note")).toHaveCount(0);
  await expect(dialog).not.toContainText("zużywa tokeny osobno");
});

test("start dynamicznego na zbyt starym Claude jest zablokowany", async ({ page }) => {
  const dialog = await openStart(page, {
    detail: dynamicDetail(),
    claude: "2.1.318",
  });
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await expect(dialog).toContainText(
    "Tryb dynamiczny wymaga Claude Code 2.1.319 lub nowszego (wykryto 2.1.318). Zaktualizuj Claude Code.",
  );
  await expect(run(dialog)).toBeDisabled();
});

test("start dynamicznego blokuje się, gdy fixture ma subagentsConfirmed=false", async ({
  page,
}) => {
  const dialog = await openStart(page, {
    detail: dynamicDetail(),
    claude: "2.1.319",
    subagentsConfirmed: false,
  });
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await expect(dialog).toContainText("nie potwierdzono, że subagenci dziedziczą");
  await expect(run(dialog)).toBeDisabled();
});

test("start dynamicznego z Codexem jako orkiestratorem jest zablokowany", async ({
  page,
}) => {
  // Graf zapisany ręcznie (np. import): walidator zgłasza błąd, a preflight
  // (fixture 1:1 z `preflight.rs`, codex_orchestrator_is_blocked) blokuje start.
  const base = buildDynamic(defaultDynamicSpec());
  const graph = {
    ...base,
    nodes: base.nodes.map((n) =>
      n.kind.type === "agent" && n.kind.role === "orchestrator"
        ? { ...n, kind: { ...n.kind, provider: "codex" as const } }
        : n,
    ),
  };
  expect(
    graph.nodes.some(
      (n) => n.kind.type === "agent" && n.kind.role === "orchestrator" && n.kind.provider === "codex",
    ),
  ).toBe(true);
  const dialog = await openStart(page, {
    detail: dynamicDetail(graph),
    claude: "2.1.319",
  });
  await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
  await dialog.getByLabel("Cel przebiegu").fill("Zrób zadanie dynamicznie");
  await expect(dialog).toContainText("Tryb dynamiczny działa tylko z Claude.");
  await expect(run(dialog)).toBeDisabled();
});

for (const mode of ["readOnly", "workspaceWrite"] as const) {
  test(`start dynamicznego (${mode}) wysyła żądanie startu`, async ({ page }) => {
    const graph = buildDynamic({
      ...defaultDynamicSpec(),
      access: mode,
      review: mode === "workspaceWrite",
    });
    const dialog = await openStart(page, {
      detail: dynamicDetail(graph),
      claude: "2.1.319",
    });
    await expect(dialog.getByRole("alert")).toHaveCount(0);
    await dialog.getByLabel(/Repozytorium/).fill("C:\\repo");
    await dialog.getByLabel("Cel przebiegu").fill("Zrób zadanie dynamicznie");
    await expect(run(dialog)).toBeEnabled();
    await run(dialog).click();
    await expect
      .poll(async () =>
        (await page.evaluate(() => (window as any).__tauriMock.calls())).filter(
          (c: any) => c.cmd === "workflow_start",
        ).length,
      )
      .toBe(1);
    const calls = await page.evaluate(() => (window as any).__tauriMock.calls());
    const start = calls.find((c: any) => c.cmd === "workflow_start");
    expect(start.args.request.versionId).toBe(42);
    expect(start.args.request.goal).toBe("Zrób zadanie dynamicznie");
  });
}
