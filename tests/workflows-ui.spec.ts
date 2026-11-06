import { test, expect, type Page } from "./fixtures/test";
import { NOW, openPreview } from "./fixtures/seed";
import { openDesktop, TEST_WORKFLOWS_KEY } from "./fixtures/tauri";
import { analysisImplementationReview } from "../src/api/workflowTemplates";

/**
 * Moduł Workflows, etap E2 (docs/workflows/PLAN_WDROZENIA.md, 4.1–4.3):
 * odblokowanie kluczem w Ustawieniach, grupy menu, animacja odsłonięcia,
 * lista definicji i edytor. Aplikacja: zastępcze IPC z testowym kluczem
 * (`TEST_WORKFLOWS_KEY`); podgląd: stan odblokowany wpisany do localStorage
 * (prawdziwego klucza nie ma w repozytorium).
 */

const WORKFLOWS_KEY = "tuktuk-track-preview-workflows-v1";
const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });

async function openWorkflowSettings(page: Page) {
  await nav(page).getByRole("button", { name: /^Ustawienia/ }).click();
  await page.getByRole("tab", { name: "Workflows" }).click();
}

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

const validation = (page: Page) =>
  page
    .getByRole("region", { name: "Edytor procesu" })
    .getByRole("heading", { name: /^Walidacja/ });

test("menu ma grupy, a bez klucza nie ma Workflows", async ({ page }) => {
  await openDesktop(page);
  const menu = nav(page);
  for (const group of ["Praca", "Agenci", "Integracje", "Zasoby"])
    await expect(menu.getByRole("group", { name: group })).toBeVisible();
  await expect(menu.getByRole("group", { name: "Workflows" })).toHaveCount(0);
  await expect(
    menu.getByRole("group", { name: "Praca" }).getByRole("button"),
  ).toHaveText(["Zadania", "Dashboard", "Plan i historia", "Raporty dzienne"]);
});

test("błędny klucz zostaje przy polu i nie zmienia menu", async ({ page }) => {
  await openDesktop(page);
  await openWorkflowSettings(page);
  await page.getByLabel("Klucz dostępu").fill("ZLY-KLUCZ");
  await page.getByRole("button", { name: "Odblokuj Workflows" }).click();
  await expect(page.getByRole("alert")).toHaveText(
    "Nieprawidłowy klucz dostępu.",
  );
  await expect(page.getByLabel("Klucz dostępu")).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  await expect(
    nav(page).getByRole("group", { name: "Workflows" }),
  ).toHaveCount(0);
  await expect(page.getByTestId("workflow-reveal")).toHaveCount(0);
});

test("poprawny klucz odsłania Workflows raz, Escape pomija animację", async ({
  page,
}) => {
  await openDesktop(page);
  await openWorkflowSettings(page);
  // Wielkość liter i spacje dookoła nie mają znaczenia (jak w Rust).
  await page
    .getByLabel("Klucz dostępu")
    .fill(`  ${TEST_WORKFLOWS_KEY.toLowerCase()} `);
  await page.getByRole("button", { name: "Odblokuj Workflows" }).click();
  await expect(page.getByTestId("workflow-reveal")).toBeAttached();
  await expect(page.getByText("Workflows odblokowane.")).toBeVisible();
  const group = nav(page).getByRole("group", { name: "Workflows" });
  await expect(group.getByRole("button")).toHaveText(["Przegląd", "Definicje", "Uruchomienia"]);
  // Formularz znika razem z przyciskiem wysyłki: fokus na następnej akcji.
  await expect(
    page.getByRole("button", { name: "Otwórz Workflows" }),
  ).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("workflow-reveal")).toHaveCount(0);
  // Powrót do Ustawień nie odtwarza animacji.
  await nav(page).getByRole("button", { name: "Zadania" }).click();
  await openWorkflowSettings(page);
  await expect(page.getByTestId("workflow-reveal")).toHaveCount(0);
  await page.getByRole("button", { name: "Otwórz Workflows" }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Workflows" }),
  ).toBeVisible();
});

test("nakładka animacji nie blokuje kliknięć w menu", async ({ page }) => {
  await openDesktop(page);
  await openWorkflowSettings(page);
  await page.getByLabel("Klucz dostępu").fill(TEST_WORKFLOWS_KEY);
  await page.getByRole("button", { name: "Odblokuj Workflows" }).click();
  await expect(page.getByTestId("workflow-reveal")).toBeAttached();
  await nav(page).getByRole("button", { name: "Dashboard" }).click();
  await expect(
    nav(page).getByRole("button", { name: "Dashboard" }),
  ).toHaveAttribute("aria-current", "page");
});

test("przy ograniczonym ruchu odsłonięcie bez animacji grafu", async ({
  page,
}) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openDesktop(page);
  await openWorkflowSettings(page);
  await page.getByLabel("Klucz dostępu").fill(TEST_WORKFLOWS_KEY);
  await page.getByRole("button", { name: "Odblokuj Workflows" }).click();
  await expect(
    nav(page).getByRole("group", { name: "Workflows" }),
  ).toBeVisible();
  await expect(page.getByTestId("workflow-reveal")).toHaveCount(0);
});

test("prośba o widok Workflows bez klucza otwiera Ustawienia → Workflows", async ({
  page,
}) => {
  await openDesktop(page);
  await page.evaluate(async () => {
    const navigation: { openView: (r: unknown) => void } = await import(
      /* @vite-ignore */ "/src/navigation.ts"
    );
    navigation.openView({ view: "workflow-definitions" });
  });
  await expect(page.getByLabel("Klucz dostępu")).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 1, name: "Definicje procesów" }),
  ).toHaveCount(0);
});

test("odblokowany moduł po starcie: bez animacji, z grupą w menu", async ({
  page,
}) => {
  await openDesktop(page, { workflows: { unlocked: true } });
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await expect(
    page.getByRole("heading", { level: 1, name: "Definicje procesów" }),
  ).toBeVisible();
  await expect(page.getByTestId("workflow-reveal")).toHaveCount(0);
});

test.describe("podgląd: lista i edytor", () => {
  test("pusty stan prowadzi do szablonu, edytor zapisuje i publikuje", async ({
    page,
  }) => {
    await previewUnlocked(page);
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await expect(page.locator(".empty-state")).toContainText(
      "Analiza → implementacja → review",
    );
    await page.getByRole("button", { name: "Z szablonu" }).click();
    const dialog = page.getByRole("dialog", { name: "Nowy proces" });
    await expect(dialog.getByLabel("Nazwa")).toHaveValue(
      "Analiza → implementacja → review",
    );
    await dialog.getByRole("button", { name: "Utwórz" }).click();

    const editor = page.getByRole("region", { name: "Edytor procesu" });
    await expect(validation(page)).toHaveText("Walidacja: brak problemów");
    const steps = editor.getByRole("complementary", { name: "Kroki procesu" });
    await expect(steps.getByRole("listitem")).toHaveCount(9);

    // Odłączenie wyjścia: problemy walidacji, publikacja zablokowana, cofnięcie.
    await steps.getByRole("button", { name: /Decyzja użytkownika/ }).click();
    await editor
      .getByLabel("Wyjście „odrzucenie” prowadzi do")
      .selectOption("");
    await expect(validation(page)).toHaveText("Walidacja: 2 problemy");
    await expect(editor.getByText("niezapisane zmiany")).toBeVisible();
    await expect(
      editor.getByRole("button", { name: "Opublikuj" }),
    ).toBeDisabled();
    await editor.getByRole("button", { name: "Cofnij" }).click();
    await expect(validation(page)).toHaveText("Walidacja: brak problemów");

    // Zmiana nazwy kroku, zapis i publikacja.
    await editor.getByLabel("Nazwa kroku").fill("Decyzja zespołu");
    await editor.getByLabel("Nazwa kroku").press("Tab");
    await editor.getByRole("button", { name: "Zapisz" }).click();
    await expect(editor.getByText("Zapisano szkic.")).toBeVisible();
    await editor.getByRole("button", { name: "Opublikuj" }).click();
    await expect(editor.getByText("Opublikowano wersję 1.")).toBeVisible();
    await expect(
      editor.getByRole("button", { name: "Opublikuj" }),
    ).toBeDisabled();

    await editor.getByRole("button", { name: "Definicje" }).click();
    await expect(
      page.getByRole("cell", { name: "v1 • opublikowany" }),
    ).toBeVisible();
  });

  test("dodanie kroku i przejście z listy problemów do kroku", async ({
    page,
  }) => {
    await previewUnlocked(page);
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page.getByRole("button", { name: "Nowy", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Nowy proces" });
    await dialog.getByLabel("Nazwa").fill("Mój proces");
    await dialog.getByRole("button", { name: "Utwórz" }).click();

    const editor = page.getByRole("region", { name: "Edytor procesu" });
    await editor.getByLabel("Typ nowego kroku").selectOption("agent");
    await editor.getByRole("button", { name: "Dodaj krok" }).click();
    // Nowy agent bez instrukcji i połączeń: problemy wskazują ten krok.
    await expect(validation(page)).toContainText("problem");
    await editor
      .getByRole("button", { name: "Przejdź do kroku" })
      .first()
      .click();
    await expect(editor.locator('[data-step="agent-1"]')).toBeFocused();

    // Wyjście z niezapisanymi zmianami pyta o porzucenie.
    await editor.getByRole("button", { name: "Definicje" }).click();
    await page
      .getByRole("dialog", { name: "Porzucić niezapisane zmiany?" })
      .getByRole("button", { name: "Porzuć zmiany" })
      .click();
    await expect(
      page.getByRole("heading", { level: 1, name: "Definicje procesów" }),
    ).toBeVisible();
  });

  test("okna przenoszą fokus do środka i oddają go po Esc", async ({
    page,
  }) => {
    await previewUnlocked(page);
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    const opener = page.getByRole("button", { name: "Z szablonu" });
    await opener.click();
    const dialog = page.getByRole("dialog", { name: "Nowy proces" });
    await expect(dialog.getByLabel("Nazwa")).toBeFocused();
    // Vue pomija handler przodka, gdy czas zdarzenia nie jest późniejszy niż
    // podpięcie handlera; przy zamrożonym zegarze testu oba są równe. Zegar
    // idzie o sekundę dalej, jak w prawdziwej aplikacji między otwarciem a Esc.
    await page.clock.setFixedTime(new Date(new Date(NOW).getTime() + 1000));
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(opener).toBeFocused();

    await page.getByRole("button", { name: "Importuj JSON" }).click();
    await expect(
      page
        .getByRole("dialog", { name: "Import procesu" })
        .getByLabel("JSON procesu"),
    ).toBeFocused();
  });

  test("niezapisane zmiany wracają po przejściu do innego widoku", async ({
    page,
  }) => {
    await previewUnlocked(page);
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page.getByRole("button", { name: "Z szablonu" }).click();
    await page
      .getByRole("dialog", { name: "Nowy proces" })
      .getByRole("button", { name: "Utwórz" })
      .click();
    const editor = page.getByRole("region", { name: "Edytor procesu" });
    await editor.getByLabel("Nazwa kroku").fill("Start z zadania");
    await editor.getByLabel("Nazwa kroku").press("Tab");
    await expect(editor.getByText("niezapisane zmiany")).toBeVisible();

    // Inny widok z menu, bez zapisu: zmiany nie przepadają.
    await nav(page).getByRole("button", { name: "Zadania" }).click();
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page
      .getByRole("button", { name: "Edytuj Analiza → implementacja → review" })
      .click();
    await expect(
      editor.getByText("Przywrócono niezapisane zmiany z poprzedniego otwarcia."),
    ).toBeVisible();
    await expect(editor.getByLabel("Nazwa kroku")).toHaveValue(
      "Start z zadania",
    );

    // „Porzuć zmiany” nie zostawia niczego do przywrócenia.
    await editor.getByRole("button", { name: "Definicje" }).click();
    await page
      .getByRole("dialog", { name: "Porzucić niezapisane zmiany?" })
      .getByRole("button", { name: "Porzuć zmiany" })
      .click();
    await page
      .getByRole("button", { name: "Edytuj Analiza → implementacja → review" })
      .click();
    await expect(editor.getByLabel("Nazwa kroku")).toHaveValue("Start");
    await expect(editor.getByText("niezapisane zmiany")).toHaveCount(0);
  });

  test("import JSON z błędem i eksport w oknie", async ({ page }) => {
    await previewUnlocked(page);
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page.getByRole("button", { name: "Importuj JSON" }).click();
    const dialog = page.getByRole("dialog", { name: "Import procesu" });
    await dialog.getByLabel("JSON procesu").fill("{}");
    await dialog.getByRole("button", { name: "Importuj" }).click();
    await expect(dialog.getByRole("alert")).toHaveText(
      "To nie jest poprawny plik procesu TukTuk Track.",
    );
    await dialog.getByRole("button", { name: "Anuluj" }).click();

    await page.getByRole("button", { name: "Z szablonu" }).click();
    await page
      .getByRole("dialog", { name: "Nowy proces" })
      .getByRole("button", { name: "Utwórz" })
      .click();
    await page
      .getByRole("region", { name: "Edytor procesu" })
      .getByRole("button", { name: "Definicje" })
      .click();
    await page
      .getByRole("button", {
        name: "Eksportuj Analiza → implementacja → review",
      })
      .click();
    const exported = page.getByRole("dialog", { name: "Eksport procesu" });
    await expect(exported.getByLabel("JSON procesu")).toHaveValue(
      /"format": "tuktuk-track-workflow"/,
    );
  });
});

/** Tworzy i publikuje szablon w podglądzie; zwraca edytor z panelem gotowości. */
async function publishTemplate(page: Page) {
  await previewUnlocked(page);
  await nav(page).getByRole("button", { name: "Definicje" }).click();
  await page.getByRole("button", { name: "Z szablonu" }).click();
  await page
    .getByRole("dialog", { name: "Nowy proces" })
    .getByRole("button", { name: "Utwórz" })
    .click();
  const editor = page.getByRole("region", { name: "Edytor procesu" });
  await editor.getByRole("button", { name: "Opublikuj" }).click();
  await expect(editor.getByText("Opublikowano wersję 1.")).toBeVisible();
  return editor;
}

const claudeReview = {
  nodeId: "review",
  provider: "claude",
  access: "readOnly",
  cliFound: true,
  version: "2.1.318",
  supported: true,
  enforcement: "cliControl",
  warning: null,
  blocked: null,
};
const codexBlocked = {
  nodeId: "implementation",
  provider: "codex",
  access: "workspaceWrite",
  cliFound: true,
  version: "0.163.0",
  supported: true,
  enforcement: "none",
  warning: null,
  blocked: {
    code: "permissionViolation",
    // Tekst z `preflight.rs` (MSG_CODEX_NO_SANDBOX).
    message:
      'Codex nie ma skonfigurowanego sandboxa Windows. Ustaw [windows] sandbox = "elevated" w config.toml Codexa.',
  },
};

/** Opublikowana definicja z szablonu (`workflows::store::Detail`) dla zastępczego IPC. */
const publishedDetail = {
  id: 5,
  name: "Proces E4",
  description: "",
  draftRevision: 2,
  createdAt: NOW,
  updatedAt: NOW,
  archivedAt: null,
  publishedVersion: 1,
  publishedAt: NOW,
  unpublishedChanges: false,
  draft: analysisImplementationReview(),
  issues: [],
  versions: [{ id: 42, version: 1, hash: "abc", createdAt: NOW }],
};

test.describe("podgląd: gotowość agentów (E4)", () => {
  test("podgląd zawsze blokuje węzły agentów i nie ma przycisku Uruchom", async ({
    page,
  }) => {
    const editor = await publishTemplate(page);
    const panel = editor.getByRole("region", { name: "Gotowość agentów" });
    await expect(panel.getByRole("status")).toContainText(
      "Gotowość sprawdzona: zablokowane węzły",
    );
    const blocked = panel.getByText("Zablokowane: Podgląd nie uruchamia agentów");
    const nodes = panel.getByRole("listitem");
    await expect(nodes.first()).toBeVisible();
    const agentCount = await nodes.count();
    await expect(blocked).toHaveCount(agentCount);
    await expect(panel).not.toContainText("kontrola CLI");
    // Wpis w localStorage, który kiedyś podmieniał wynik, niczego nie zmienia.
    await page.evaluate(() =>
      localStorage.setItem(
        "tuktuk-track-preview-workflows-preflight",
        JSON.stringify([]),
      ),
    );
    await panel.getByRole("button", { name: "Sprawdź ponownie" }).click();
    await expect(panel.getByRole("status")).toContainText(
      `zablokowane węzły: ${agentCount}`,
    );
    await expect(blocked).toHaveCount(agentCount);
    await expect(page.getByRole("button", { name: "Uruchom", exact: true })).toHaveCount(0);
  });

  test("Sprawdź ponownie czyta wersję od nowa, dostępne klawiaturą", async ({
    page,
  }) => {
    const editor = await publishTemplate(page);
    const panel = editor.getByRole("region", { name: "Gotowość agentów" });
    await expect(panel).toContainText("Claude Code");
    // Zmiana zapisanej wersji pod spodem: tylko ponowne wywołanie ją pokaże.
    await page.evaluate((key) => {
      const state = JSON.parse(localStorage.getItem(key) ?? "{}");
      for (const workflow of state.workflows)
        for (const version of workflow.versions)
          for (const node of version.graph.nodes)
            if (node.kind.type === "agent") node.kind.provider = "codex";
      localStorage.setItem(key, JSON.stringify(state));
    }, WORKFLOWS_KEY);
    await expect(panel).toContainText("Claude Code");
    const recheck = panel.getByRole("button", { name: "Sprawdź ponownie" });
    await recheck.focus();
    await page.keyboard.press("Enter");
    await expect(panel).not.toContainText("Claude Code");
    await expect(panel).toContainText("Codex");
    await expect(panel.getByRole("status")).toContainText("Gotowość sprawdzona");
  });
});

test.describe("aplikacja: workflow_preflight przez IPC (E4)", () => {
  test("panel pokazuje kontrolę CLI i blokadę Codexa, Sprawdź ponownie woła IPC", async ({
    page,
  }) => {
    await openDesktop(page, {
      workflows: { unlocked: true, detail: publishedDetail },
      preflight: [claudeReview, codexBlocked],
    });
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page.getByRole("button", { name: "Edytuj Proces E4" }).click();
    const panel = page
      .getByRole("region", { name: "Edytor procesu" })
      .getByRole("region", { name: "Gotowość agentów" });
    await expect(panel.getByRole("status")).toContainText(
      "zablokowane węzły: 1",
    );
    await expect(panel).toContainText("kontrola CLI");
    await expect(panel).toContainText("nie izolacja systemu plików");
    await expect(panel).toContainText(
      "Zablokowane: Codex nie ma skonfigurowanego sandboxa Windows",
    );
    const calls = () =>
      page.evaluate(() => {
        const w = window as unknown as {
          __preflightCalls?: number;
          __tauriMock: { calls: () => { cmd: string; args: unknown }[] };
        };
        return {
          count: w.__preflightCalls ?? 0,
          args: w.__tauriMock
            .calls()
            .filter((c) => c.cmd === "workflow_preflight")
            .map((c) => c.args),
        };
      });
    expect(await calls()).toEqual({ count: 1, args: [{ versionId: 42 }] });
    await panel.getByRole("button", { name: "Sprawdź ponownie" }).click();
    await expect.poll(async () => (await calls()).count).toBe(2);
    expect((await calls()).args).toEqual([{ versionId: 42 }, { versionId: 42 }]);
    await expect(panel).toContainText("kontrola CLI");
    await expect(page.getByRole("button", { name: "Uruchom", exact: true })).toHaveCount(0);
  });

  test("publikacja w trakcie sprawdzania: panel pokazuje nową wersję, stara odpowiedź odpada", async ({
    page,
  }) => {
    const codexReady = {
      ...codexBlocked,
      enforcement: "codexSandbox",
      blocked: null,
    };
    await openDesktop(page, {
      workflows: {
        unlocked: true,
        detail: { ...publishedDetail, unpublishedChanges: true },
      },
      preflightByVersion: {
        42: [claudeReview, codexBlocked],
        43: [claudeReview, codexReady],
      },
    });
    // Pierwsze sprawdzenie (wersja 42) czeka, aż test je zwolni.
    await page.evaluate(() => {
      const w = window as unknown as {
        __preflightHold?: Promise<void>;
        __releasePreflight?: () => void;
      };
      w.__preflightHold = new Promise((r) => (w.__releasePreflight = r));
    });
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await page.getByRole("button", { name: "Edytuj Proces E4" }).click();
    const editor = page.getByRole("region", { name: "Edytor procesu" });
    const panel = editor.getByRole("region", { name: "Gotowość agentów" });
    await expect(panel.getByRole("status")).toContainText("Sprawdzam");
    await editor.getByRole("button", { name: "Opublikuj" }).click();
    await expect(editor.getByText("Opublikowano wersję 2.")).toBeVisible();
    await expect(panel.getByRole("status")).toContainText(
      "wszystkie węzły gotowe",
    );
    await expect(panel).toContainText("sandbox Codexa");
    // Spóźniona odpowiedź dla wersji 42 nie nadpisuje wyniku wersji 43.
    await page.evaluate(() =>
      (window as unknown as { __releasePreflight: () => void }).__releasePreflight(),
    );
    const args = await page.evaluate(() =>
      (
        window as unknown as {
          __tauriMock: { calls: () => { cmd: string; args: unknown }[] };
        }
      ).__tauriMock
        .calls()
        .filter((c) => c.cmd === "workflow_preflight")
        .map((c) => c.args),
    );
    expect(args).toEqual([{ versionId: 42 }, { versionId: 43 }]);
    await page.waitForTimeout(100);
    await expect(panel).not.toContainText("Zablokowane");
    await expect(panel.getByRole("status")).toContainText(
      "wszystkie węzły gotowe",
    );
  });

  test("każde sprawdzenie woła IPC z versionId i zwraca wynik z Rust", async ({
    page,
  }) => {
    await openDesktop(page, {
      workflows: { unlocked: true },
      preflight: [claudeReview, codexBlocked],
    });
    const check = () =>
      page.evaluate(async () => {
        const workflows: {
          workflowPreflight: (versionId: number) => Promise<unknown[]>;
        } = await import(/* @vite-ignore */ "/src/workflows.ts");
        return workflows.workflowPreflight(42);
      });
    const calls = () =>
      page.evaluate(() => {
        const w = window as unknown as {
          __preflightCalls?: number;
          __tauriMock: { calls: () => { cmd: string; args: unknown }[] };
        };
        return {
          count: w.__preflightCalls ?? 0,
          args: w.__tauriMock
            .calls()
            .filter((c) => c.cmd === "workflow_preflight")
            .map((c) => c.args),
        };
      });
    expect(await check()).toEqual([claudeReview, codexBlocked]);
    expect(await calls()).toEqual({ count: 1, args: [{ versionId: 42 }] });
    expect(await check()).toEqual([claudeReview, codexBlocked]);
    expect(await calls()).toEqual({
      count: 2,
      args: [{ versionId: 42 }, { versionId: 42 }],
    });
  });
});

// E7: wyłączenie modułu (workflows::workflow_lock).
test.describe("wyłączenie Workflows", () => {
  const runSummary = (status: string) => ({
    id: 1,
    workflowId: 1,
    workflowName: "Proces testowy",
    version: 1,
    status,
    taskId: null,
    taskTitle: null,
    taskExists: false,
    repo: "C:\\repo",
    createdAt: 1_700_000_000_001,
    startedAt: null,
    finishedAt: null,
    errorCode: null,
    waitingApproval: status === "waitingForApproval",
  });
  const locks = (page: Page) =>
    page.evaluate(() => (window as any).__workflowLocks ?? 0);
  const lockButton = (page: Page) =>
    page.getByRole("button", { name: "Wyłącz Workflows" });
  const keyField = (page: Page) => page.getByLabel("Klucz dostępu");

  test("potwierdzenie: Esc zwraca fokus, podwójne kliknięcie wywołuje komendę raz", async ({
    page,
  }) => {
    await openDesktop(page, { workflows: { unlocked: true } });
    await openWorkflowSettings(page);
    await expect(
      nav(page).getByRole("group", { name: "Workflows" }),
    ).toBeVisible();
    // Odblokowany moduł: formularz klucza się nie pokazuje.
    await expect(keyField(page)).toHaveCount(0);
    await expect(
      page.getByRole("button", { name: "Odblokuj Workflows" }),
    ).toHaveCount(0);
    await lockButton(page).click();
    const dialog = page.getByRole("dialog", { name: "Wyłączyć Workflows?" });
    await expect(dialog).toContainText(
      "Historia i definicje zostają. Ponowne włączenie wymaga klucza.",
    );
    // Vue pomija handler przodka z tym samym znacznikiem czasu: zegar idzie dalej.
    await page.clock.setFixedTime(new Date(new Date(NOW).getTime() + 1000));
    await page.keyboard.press("Escape");
    await expect(dialog).toHaveCount(0);
    await expect(lockButton(page)).toBeFocused();
    expect(await locks(page)).toBe(0);
    // Anulowanie zostawia moduł odblokowany, bez formularza klucza.
    await expect(keyField(page)).toHaveCount(0);

    await lockButton(page).click();
    // Bez przebiegów czekających na zgodę okno nie ostrzega.
    await expect(dialog).not.toContainText("Przebiegi czekające na zgodę");
    // Wstrzymana odpowiedź: okno zostaje otwarte przez całe podwójne kliknięcie.
    await page.evaluate(() => {
      const w = window as any;
      w.__workflowLockHold = new Promise((r) => (w.__releaseLock = r));
    });
    const confirm = dialog.getByRole("button", { name: "Wyłącz Workflows" });
    await confirm.dblclick();
    await expect(dialog).toBeVisible();
    await expect(confirm).toBeDisabled();
    expect(await locks(page)).toBe(1);
    await page.evaluate(() => (window as any).__releaseLock());
    await expect(dialog).toHaveCount(0);
    expect(await locks(page)).toBe(1);
    await expect(
      nav(page).getByRole("group", { name: "Workflows" }),
    ).toHaveCount(0);
    await expect(keyField(page)).toBeVisible();
    await expect(lockButton(page)).toHaveCount(0);
    // Jeden komunikat w stałym regionie statusu, fokus na polu klucza.
    await expect(
      page.getByRole("status").filter({ hasText: "Workflows" }),
    ).toHaveText("Workflows wyłączone. Historia i definicje zostają.");
    await expect(keyField(page)).toBeFocused();
  });

  test("okno wyłączenia ostrzega o przebiegach czekających na zgodę", async ({
    page,
  }) => {
    await openDesktop(page, {
      workflows: { unlocked: true },
      workflowRuns: {
        runs: [
          runSummary("waitingForApproval"),
          { ...runSummary("succeeded"), id: 2 },
        ],
      },
    });
    await openWorkflowSettings(page);
    await lockButton(page).click();
    const dialog = page.getByRole("dialog", { name: "Wyłączyć Workflows?" });
    await expect(dialog).toContainText(
      "Przebiegi czekające na zgodę: 1. Po wyłączeniu nie da się ich zatwierdzić ani odrzucić (tylko zatrzymać)",
    );
    // Ostrzeżenie nie blokuje: wyłączenie się udaje.
    await dialog.getByRole("button", { name: "Wyłącz Workflows" }).click();
    await expect(dialog).toHaveCount(0);
    await expect(keyField(page)).toBeVisible();
  });

  test("po wyłączeniu widok Workflows prowadzi do Ustawień z kluczem", async ({
    page,
  }) => {
    await openDesktop(page, { workflows: { unlocked: true } });
    await openWorkflowSettings(page);
    await expect(keyField(page)).toHaveCount(0);
    await lockButton(page).click();
    await page
      .getByRole("dialog", { name: "Wyłączyć Workflows?" })
      .getByRole("button", { name: "Wyłącz Workflows" })
      .click();
    await expect(keyField(page)).toBeVisible();
    // Z innego widoku: prośba o Workflows prowadzi z powrotem do Ustawień.
    await nav(page).getByRole("button", { name: "Dashboard" }).click();
    await expect(keyField(page)).toHaveCount(0);
    await page.evaluate(async () => {
      const nav: { openView: (r: { view: string }) => void } = await import(
        /* @vite-ignore */ "/src/navigation.ts"
      );
      nav.openView({ view: "workflows" });
    });
    await expect(
      nav(page).getByRole("button", { name: /^Ustawienia/ }),
    ).toHaveAttribute("aria-current", "page");
    await expect(
      page.getByRole("tab", { name: "Workflows" }),
    ).toHaveAttribute("aria-selected", "true");
    await expect(keyField(page)).toBeVisible();
    await expect(
      page.getByRole("heading", { level: 1, name: "Workflows" }),
    ).toHaveCount(0);
  });

  test("przebieg w toku blokuje wyłączenie komunikatem w alercie", async ({
    page,
  }) => {
    await openDesktop(page, {
      workflows: { unlocked: true },
      workflowRuns: { runs: [runSummary("running")] },
    });
    await openWorkflowSettings(page);
    await lockButton(page).click();
    const dialog = page.getByRole("dialog", { name: "Wyłączyć Workflows?" });
    await dialog.getByRole("button", { name: "Wyłącz Workflows" }).click();
    await expect(dialog.getByRole("alert")).toContainText(
      "Wyłączenie wstrzymane. Przebiegi Workflows w toku: 1.",
    );
    await page.keyboard.press("Escape");
    await expect(
      nav(page).getByRole("group", { name: "Workflows" }),
    ).toBeVisible();
  });

  test("po wyłączeniu tablica agentów nadal pozwala zatrzymać przebieg", async ({
    page,
  }) => {
    await openDesktop(page, {
      workflows: { unlocked: true },
      workflowRuns: { runs: [runSummary("waitingForApproval")] },
    });
    await openWorkflowSettings(page);
    await lockButton(page).click();
    await page
      .getByRole("dialog", { name: "Wyłączyć Workflows?" })
      .getByRole("button", { name: "Wyłącz Workflows" })
      .click();
    await expect(keyField(page)).toBeVisible();
    await nav(page).getByRole("button", { name: /Agenci|Sesje agentów/ }).first().click();
    const section = page.getByRole("region", {
      name: /Przebiegi procesów w toku/,
    });
    await section.getByRole("button", { name: "Zatrzymaj przebieg 1" }).click();
    await page
      .getByRole("dialog", { name: "Zatrzymać przebieg?" })
      .getByRole("button", { name: "Zatrzymaj" })
      .click();
    await expect(section.getByRole("status")).toContainText(
      "Zatrzymywanie przebiegu 1",
    );
    expect(
      await page.evaluate(() => (window as any).__workflowCancels),
    ).toBe(1);
    await expect(page.getByRole("alert")).toHaveCount(0);
  });

  // Jak `workflows::lock` (tekst pilnuje też test Rust `lock_is_refused_while_runs_are_active`).
  test("podgląd: odmowa wyłączenia ma regułę i komunikat z Rust", async ({
    page,
  }) => {
    await previewUnlocked(page);
    const result = await page.evaluate(async () => {
      const w: any = await import(/* @vite-ignore */ "/src/preview/workflows.ts");
      const runs: any = await import(/* @vite-ignore */ "/src/preview/workflowRuns.ts");
      return {
        none: w.lockRefusal(0),
        two: w.lockRefusal(2),
        active: runs.activeRunCount(),
      };
    });
    expect(result.none).toBeNull();
    expect(result.two).toBe(
      "Wyłączenie wstrzymane. Przebiegi Workflows w toku: 2. Poczekaj na ich koniec albo je zatrzymaj (przebieg czekający na zgodę nie blokuje).",
    );
    // Przykład czeka na zgodę albo jest przerwany: nie blokuje wyłączenia.
    expect(result.active).toBe(0);
  });

  test("podgląd: wyłączenie zachowuje definicje, ponowne odblokowanie je pokazuje", async ({
    page,
  }) => {
    await previewUnlocked(page);
    await page.evaluate(
      async ([key]) => {
        const w: any = await import(/* @vite-ignore */ "/src/workflows.ts");
        await w.createWorkflow("Zachowany", "", false);
        await w.lockWorkflows();
        const raw = JSON.parse(localStorage.getItem(key) ?? "{}");
        (window as any).__after = {
          unlocked: raw.unlocked,
          count: raw.workflows.length,
        };
      },
      [WORKFLOWS_KEY],
    );
    expect(await page.evaluate(() => (window as any).__after)).toEqual({
      unlocked: false,
      count: 1,
    });
    await page.evaluate((key) => {
      const raw = JSON.parse(localStorage.getItem(key) ?? "{}");
      localStorage.setItem(key, JSON.stringify({ ...raw, unlocked: true }));
    }, WORKFLOWS_KEY);
    await page.reload();
    await nav(page).getByRole("button", { name: "Definicje" }).click();
    await expect(page.getByText("Zachowany").first()).toBeVisible();
  });
});
