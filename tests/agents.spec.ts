import { test, expect } from "./fixtures/test";
import { ids, NOW, openPreview } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

const BINDINGS_KEY = "tuktuk-track-preview-agent-bindings-v1";

const key = "tuktuk-track-preview-agents-v1";

test("dashboard lists working AI agents and the setting toggles agent activity", async ({
  page,
}) => {
  await page.clock.setFixedTime(new Date("2026-10-15T12:00:00+02:00"));
  await page.goto("/");
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  const card = page.getByRole("region", { name: "Agenci AI" });
  await expect(card).toContainText("Żaden agent teraz nie pracuje.");

  await page.evaluate((k) => {
    const now = new Date("2026-10-15T12:00:00+02:00").getTime();
    localStorage.setItem(
      k,
      JSON.stringify([
        {
          kind: "claude",
          name: "Wykrywanie agentów",
          cwd: "C:\\Projekty\\to-do-track",
          since: now - 12 * 60_000,
        },
        {
          kind: "codex",
          name: "api",
          cwd: "C:\\Projekty\\api",
          since: now - 30_000,
        },
      ]),
    );
  }, key);
  await page.reload();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(card.locator(".dashboard-count")).toHaveText([
    "Claude · Wykrywanie agentów00:12:00",
    "Codex · api00:00:30",
  ]);
  await expect(card).not.toContainText("Żaden agent");

  await openSettings(page, "Czas pracy");
  const agents = page.getByLabel(
    "Praca agentów AI (Claude Code, Codex) liczy się jako aktywność",
  );
  await expect(agents).toBeChecked();
  await agents.uncheck();
  await expect(agents).not.toBeChecked();
  await page
    .getByLabel("Zapytaj o czas po co najmniej 10 minutach bezczynności")
    .uncheck();
  await expect(agents).toBeDisabled();
  const stored = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("tuktuk-track-preview-settings-v1")!),
  );
  expect(stored).toEqual({ idleAgents: false, idleEnabled: false });
});

test.describe("rozpoznanie zadania agenta", () => {
  test.use({ timezoneId: "Europe/Warsaw" });

  test("kafelek pokazuje gałąź i dopasowane zadanie", async ({ page }) => {
    const now = Date.parse(NOW);
    await openPreview(page, {
      agents: [
        {
          kind: "claude",
          name: "Import",
          cwd: "C:\\Projekty\\wms",
          since: now - 5 * 60_000,
          branch: "feature/WMS-12-import",
        },
        {
          kind: "codex",
          name: "api",
          cwd: "C:\\Projekty\\api\\src",
          since: now - 4 * 60_000,
          branch: "main",
        },
        {
          kind: "claude",
          name: "Sesja DEV-5",
          cwd: "C:\\Projekty\\inne",
          since: now - 3 * 60_000,
          branch: "0123456",
          detached: true,
          session: "Sesja DEV-5",
        },
        {
          kind: "codex",
          name: "kosz",
          cwd: "C:\\Projekty\\kosz",
          since: now - 2 * 60_000,
          branch: "fix/WMS-7",
        },
        {
          kind: "codex",
          name: "bez-repo",
          cwd: "C:\\Temp\\bez-repo",
          since: now - 60_000,
        },
      ],
    });
    // Przypisanie folderu (z podfolderami) do „Code review”.
    await page.evaluate(
      ([k, id]) =>
        localStorage.setItem(
          k,
          JSON.stringify([{ folder: "C:\\Projekty\\api", taskId: id }]),
        ),
      [BINDINGS_KEY, ids.review] as const,
    );
    await page.reload();
    await page.getByRole("button", { name: "Dashboard", exact: true }).click();
    const card = page.getByRole("region", { name: "Agenci AI" });
    await expect(card.locator(".dashboard-count")).toHaveCount(5);
    const work = async (name: string, branch: string, task: string) => {
      const line = card.getByLabel(`Praca agenta ${name}`);
      await expect(line.locator(".agent-branch")).toHaveText(branch);
      await expect(line.locator(".agent-task")).toHaveText(task);
    };
    await work(
      "Import",
      "gałąź feature/WMS-12-import",
      "zadanie „Analiza importu”",
    );
    await work("api", "gałąź main", "zadanie „Code review” (folder)");
    // Odłączony HEAD: klucz z nazwy sesji Claude Code, bez lokalnego zadania.
    await work("Sesja DEV-5", "odłączony HEAD 0123456", "DEV-5: brak zadania");
    // Zadanie w koszu nie jest dopasowywane.
    await work("kosz", "gałąź fix/WMS-7", "WMS-7: brak zadania");
    await expect(card.getByLabel("Praca agenta bez-repo")).toHaveCount(0);
  });

  test("przypisanie folderu zmienia dopasowanie bez przeładowania", async ({
    page,
  }) => {
    const now = Date.parse(NOW);
    await openPreview(page, {
      agents: [
        {
          kind: "claude",
          name: "Import",
          cwd: "C:\\Projekty\\wms",
          since: now - 5 * 60_000,
          branch: "feature/WMS-12-import",
        },
      ],
    });
    await page.getByRole("button", { name: "Dashboard", exact: true }).click();
    const work = page
      .getByRole("region", { name: "Agenci AI" })
      .getByLabel("Praca agenta Import");
    await expect(work).toContainText("zadanie „Analiza importu”");
    // Folder wygrywa z kluczem z gałęzi; odpowiednik `agent_bind` w podglądzie.
    await page.evaluate(async (id) => {
      const store = await import("/src/store.ts");
      await store.bindAgentFolder("C:\\Projekty\\wms\\", id);
    }, ids.docs);
    await expect(work).toContainText("zadanie „Dokumentacja API” (folder)");
    await page.evaluate(async () => {
      const store = await import("/src/store.ts");
      await store.unbindAgentFolder("c:\\projekty\\WMS");
    });
    await expect(work).toContainText("zadanie „Analiza importu”");
  });
});
