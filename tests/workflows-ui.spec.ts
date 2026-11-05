import { test, expect, type Page } from "./fixtures/test";
import { NOW, openPreview } from "./fixtures/seed";
import { openDesktop, TEST_WORKFLOWS_KEY } from "./fixtures/tauri";

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
  await expect(group.getByRole("button")).toHaveText(["Przegląd", "Definicje"]);
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
