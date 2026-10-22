import { test, expect } from "./fixtures/test";
import { NOW, openPreview, SETTINGS_KEY } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Hooki agentów (PLAN_BACKLOG.md, B8): kafelek Agenci AI zasilany zdarzeniami hooków
 * przez tę samą maszynę stanów co w Rust i sekcja Ustawień „Agenci”.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const now = Date.parse(NOW);
const min = 60_000;
const app = "C:\\Projekty\\to-do-track";
const api = "C:\\Projekty\\api";
const wms = "C:\\Projekty\\wms";

const ev = (
  at: number,
  agent: "claude" | "codex",
  event: string,
  sessionId: string,
  extra: Record<string, unknown> = {},
) => ({ at, agent, event, sessionId, ...extra });

/** Zdarzenia z fixture: praca, czekanie na zgodę, subagent, timeout i koniec. */
const events = [
  // Claude pracuje od 12 minut, ostatnie zdarzenie przed chwilą.
  ev(now - 12 * min, "claude", "UserPromptSubmit", "claude-app", {
    cwd: app,
    prompt: "Dodaj hooki agentów",
  }),
  ev(now - 20_000, "claude", "PreToolUse", "claude-app", { cwd: app }),
  // Codex czeka na zgodę: nie liczy się jako praca.
  ev(now - 3 * min, "codex", "UserPromptSubmit", "codex-api", { cwd: api }),
  ev(now - 2 * min, "codex", "PermissionRequest", "codex-api", { cwd: api }),
  // Sesja bez zdarzeń od ponad 10 minut kończy się sama.
  ev(now - 30 * min, "claude", "UserPromptSubmit", "claude-stale", { cwd: wms }),
  // (PostToolUse: przy niezakończonym narzędziu limit jest dłuższy.)
  ev(now - 11 * min, "claude", "PostToolUse", "claude-stale", { cwd: wms }),
  // Stop przy działającym subagencie nie kończy sesji.
  ev(now - 90_000, "claude", "UserPromptSubmit", "claude-sub", {
    cwd: "C:\\Projekty\\sub",
  }),
  ev(now - 80_000, "claude", "SubagentStart", "claude-sub", {
    cwd: "C:\\Projekty\\sub",
    agentId: "agent-1",
  }),
  ev(now - 10_000, "claude", "Stop", "claude-sub", { cwd: "C:\\Projekty\\sub" }),
  // Zakończona sesja.
  ev(now - 5 * min, "codex", "UserPromptSubmit", "codex-done", { cwd: wms }),
  ev(now - 4 * min, "codex", "SessionEnd", "codex-done", { cwd: wms }),
];

test("kafelek Agenci AI liczy pracę z hooków: praca tak, czekanie na zgodę i timeout nie", async ({
  page,
}) => {
  await openPreview(page, { agentHooks: { events } });
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  const card = page.getByRole("region", { name: "Agenci AI" });
  await expect(card.locator(".dashboard-count")).toHaveText([
    "Claude · to-do-track00:12:00",
    "Claude · sub00:01:30",
  ]);
  await expect(card).not.toContainText("Codex");
  await expect(card).not.toContainText("wms");

  // Zgoda w Codexie i wznowienie pracy: sesja wraca na listę od nowa.
  await page.evaluate(
    ([k, extra]) => {
      const data = JSON.parse(localStorage.getItem(k)!);
      data.events.push(extra);
      localStorage.setItem(k, JSON.stringify(data));
    },
    [
      "tuktuk-track-preview-agent-hooks-v1",
      ev(now - 5_000, "codex", "PostToolUse", "codex-api", { cwd: api }),
    ] as const,
  );
  await page.reload();
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await expect(card.locator(".dashboard-count")).toHaveText([
    "Claude · to-do-track00:12:00",
    "Claude · sub00:01:30",
    "Codex · api00:00:05",
  ]);
});

test("długie narzędzie trzyma sesję w pracy, a zgubiony SubagentStop nie", async ({
  page,
}) => {
  await openPreview(page, {
    agentHooks: {
      events: [
        // Build trwa od 25 minut: PreToolUse bez PostToolUse.
        ev(now - 30 * min, "claude", "UserPromptSubmit", "claude-build", {
          cwd: app,
        }),
        ev(now - 25 * min, "claude", "PreToolUse", "claude-build", { cwd: app }),
        // Stop przy subagencie, którego SubagentStop się zgubił.
        ev(now - 8 * min, "claude", "UserPromptSubmit", "claude-lost", {
          cwd: api,
        }),
        ev(now - 7 * min, "claude", "SubagentStart", "claude-lost", {
          cwd: api,
          agentId: "agent-9",
        }),
        ev(now - 6 * min, "claude", "Stop", "claude-lost", { cwd: api }),
      ],
    },
  });
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  const card = page.getByRole("region", { name: "Agenci AI" });
  await expect(card.locator(".dashboard-count")).toHaveText([
    "Claude · to-do-track00:30:00",
  ]);
  await expect(card).not.toContainText("api");
});

test("wyłączone hooki nie zasilają kafelka, a lista agentów z localStorage działa jak dotąd", async ({
  page,
}) => {
  await openPreview(page, {
    agentHooks: { events },
    settings: { agentHooks: false },
    agents: [
      { kind: "codex", name: "stary", cwd: "C:\\Projekty\\stary", since: now - min },
    ],
  });
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  const card = page.getByRole("region", { name: "Agenci AI" });
  await expect(card.locator(".dashboard-count")).toHaveText([
    "Codex · stary00:01:00",
  ]);
});

test("Ustawienia → Agenci: wyłącznik, stan rejestracji, zaufanie Codexa i ostatnie zdarzenia", async ({
  page,
}) => {
  await openPreview(page, { agentHooks: { events } });
  await openSettings(page, "Agenci");
  await expect(
    page.getByRole("heading", { name: "Agenci", exact: true }),
  ).toBeVisible();
  const toggle = page.getByLabel("Hooki agentów (Claude Code, Codex)");
  await expect(toggle).toBeChecked();
  const status = page.getByRole("status", { name: "Stan hooków agentów" });
  await expect(status).toContainText("Claude Code: zarejestrowane");
  await expect(status).toContainText("Codex: zarejestrowane");
  // Codex wymaga zatwierdzenia przez /hooks, dopóki nie jest zaufany.
  await expect(status).toContainText("hooki czekają na zatwierdzenie");
  await expect(status).toContainText("/hooks");

  // Ostatnie zdarzenia od najnowszego, bez treści promptów.
  const list = page.getByRole("list", { name: "Ostatnie zdarzenia agentów" });
  await expect(list.getByRole("listitem")).toHaveCount(events.length);
  await expect(list.getByRole("listitem").first()).toContainText(
    "Claude · Stop",
  );
  await expect(list).toContainText("Claude · PreToolUse");
  await expect(list).toContainText("czeka");
  await expect(list).not.toContainText("Dodaj hooki agentów");
  // Windowsowe cwd z hooków: sama nazwa folderu, bez pełnej ścieżki.
  await expect(list).toContainText("· api");
  await expect(list).not.toContainText("C:\\Projekty");

  // Wyłączenie zapisuje ustawienie i zmienia stan rejestracji.
  await toggle.uncheck();
  await expect(toggle).not.toBeChecked();
  await expect(status).toContainText("Claude Code: wyłączone");
  await expect(status).not.toContainText("hooki czekają");
  const stored = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    SETTINGS_KEY,
  );
  expect(stored).toEqual({ agentHooks: false });
  await toggle.check();
  await expect(status).toContainText("Claude Code: zarejestrowane");
});

test("Ustawienia → Agenci: zatwierdzony Codex, brak agenta i błąd rejestracji", async ({
  page,
}) => {
  await openPreview(page, {
    agentHooks: {
      events: [],
      codexTrust: "trusted",
      claude: { found: false },
      error: "Claude Code: Plik settings.json nie jest poprawnym JSON-em.",
    },
  });
  await openSettings(page, "Agenci");
  const status = page.getByRole("status", { name: "Stan hooków agentów" });
  await expect(status).toContainText("Codex: hooki zatwierdzone.");
  await expect(status).toContainText(
    "Claude Code: nie znaleziono (agent nie jest zainstalowany)",
  );
  await expect(status).toContainText("nie jest poprawnym JSON-em");
  await expect(page.getByText("Jeszcze nie przyszło żadne zdarzenie.")).toBeVisible();

  // Częściowe zatwierdzenie.
  await page.evaluate(() => {
    const k = "tuktuk-track-preview-agent-hooks-v1";
    const data = JSON.parse(localStorage.getItem(k)!);
    data.codexTrust = "partial";
    localStorage.setItem(k, JSON.stringify(data));
  });
  // Stan odświeża się co 5 s.
  await expect(status).toContainText("część hooków czeka na zatwierdzenie", {
    timeout: 10_000,
  });
});

test("wyszukiwarka ustawień znajduje sekcję Agenci po etykiecie", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  await page
    .getByRole("searchbox", { name: "Szukaj ustawienia" })
    .fill("zatwierdzenie hooków");
  await expect(
    page.getByRole("tablist", { name: "Sekcje ustawień" }).getByRole("tab"),
  ).toHaveText(["Agenci"]);
});
