import { test, expect } from "./fixtures/test";
import { NOW, openPreview, SETTINGS_KEY } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * Tablica sesji agentów (`agent_board.rs`, `views/AgentsView.vue`): czeka na
 * użytkownika / pracuje / skończone nieprzejrzane, licznik w nawigacji i
 * „Przejrzane”. Zdarzenia hooków przechodzą przez tę samą maszynę stanów co w
 * Rust (`preview/hooks.ts`).
 */
test.use({ timezoneId: "Europe/Warsaw" });

const now = Date.parse(NOW);
const min = 60_000;
const dir = (name: string) => `C:\\Projekty\\${name}`;

const ev = (
  at: number,
  agent: "claude" | "codex",
  event: string,
  sessionId: string,
  extra: Record<string, unknown> = {},
) => ({ at, agent, event, sessionId, ...extra });

const events = [
  // Pytanie od Claude (AskUserQuestion) od 4 minut.
  ev(now - 10 * min, "claude", "UserPromptSubmit", "ask", { cwd: dir("app") }),
  ev(now - 4 * min, "claude", "PreToolUse", "ask", {
    cwd: dir("app"),
    toolName: "AskUserQuestion",
  }),
  // Codex prosi o zgodę na Bash od 2 minut.
  ev(now - 3 * min, "codex", "UserPromptSubmit", "perm", { cwd: dir("api") }),
  ev(now - 2 * min, "codex", "PermissionRequest", "perm", {
    cwd: dir("api"),
    toolName: "Bash",
  }),
  // Claude pracuje od 5 minut.
  ev(now - 5 * min, "claude", "UserPromptSubmit", "work", { cwd: dir("wms") }),
  ev(now - 20_000, "claude", "PreToolUse", "work", { cwd: dir("wms") }),
  // Tura zakończona o 11:40.
  ev(now - 30 * min, "claude", "UserPromptSubmit", "done", { cwd: dir("docs") }),
  ev(now - 20 * min, "claude", "Stop", "done", { cwd: dir("docs") }),
  // Sesja bez zdarzeń od 40 minut: koniec z braku sygnału.
  ev(now - 40 * min, "claude", "UserPromptSubmit", "stale", { cwd: dir("old") }),
  // Otwarta sesja bez tury: nie ma jej na tablicy.
  ev(now - min, "claude", "SessionStart", "idle", { cwd: dir("idle") }),
];

const navButton = (page: import("@playwright/test").Page) =>
  page
    .getByRole("navigation", { name: "Nawigacja główna" })
    .getByRole("button", { name: /^Sesje agentów/ });

test("tablica pokazuje, co czeka, co pracuje i co się skończyło, a „Przejrzane” zdejmuje koniec", async ({
  page,
}) => {
  await openPreview(page, { agentHooks: { events } });
  // Koniec z braku zdarzeń jest na tablicy, ale nie w liczniku.
  const nav = navButton(page);
  await expect(nav).toHaveAccessibleName(
    "Sesje agentów: czeka 2, skończone 1",
  );
  await expect(nav.locator(".nav-badge")).toHaveText("3");
  await nav.click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText(
    "Sesje agentów",
  );

  const waiting = page.getByRole("region", { name: /Czeka na Ciebie/ });
  await expect(waiting.getByRole("listitem")).toHaveCount(2);
  // Najdłużej czekająca pierwsza, z powodem i czasem czekania.
  const ask = waiting.getByRole("listitem").nth(0);
  await expect(ask).toContainText("Claude · app");
  await expect(ask).toContainText("pytanie");
  await expect(ask).toContainText("00:04:00");
  const perm = waiting.getByRole("listitem").nth(1);
  await expect(perm).toContainText("Codex · api");
  await expect(perm).toContainText("zgoda: Bash");
  await expect(perm).toContainText("00:02:00");

  const working = page.getByRole("region", { name: /Pracuje/ });
  await expect(working.getByRole("listitem")).toHaveCount(1);
  await expect(working.getByRole("listitem")).toContainText("00:05:00");
  await expect(
    working.getByRole("button", { name: /Przejrzane/ }),
  ).toHaveCount(0);

  const done = page.getByRole("region", { name: /Skończone, nieprzejrzane/ });
  await expect(done.getByRole("listitem")).toHaveCount(2);
  await expect(done.getByRole("listitem").nth(0)).toContainText(
    "Claude · old",
  );
  await expect(done.getByRole("listitem").nth(0)).toContainText(
    "bez sygnału końca",
  );
  await expect(done.getByRole("listitem").nth(1)).toContainText("11:40");
  await expect(page.getByText("idle")).toHaveCount(0);

  // Przejrzany koniec znika z tablicy i z licznika.
  await done
    .getByRole("button", { name: "Oznacz jako przejrzane: Claude · docs" })
    .click();
  await expect(done.getByRole("listitem")).toHaveCount(1);
  await expect(nav).toHaveAccessibleName("Sesje agentów: czeka 2");
  // Kliknięty przycisk znika: fokus wraca na nagłówek sekcji, czytnik dostaje komunikat.
  await expect(done.getByRole("heading", { level: 2 })).toBeFocused();
  await expect(page.getByRole("status")).toHaveText(
    "Oznaczono jako przejrzane: Claude · docs",
  );

  // Wszystkie: końce znikają, czekające zostają jako przejrzane.
  await page
    .getByRole("button", { name: "Oznacz wszystkie jako przejrzane" })
    .click();
  await expect(done.getByRole("listitem")).toHaveCount(0);
  await expect(done).toContainText("Nie ma zakończonych sesji do przejrzenia.");
  await expect(waiting.getByRole("listitem")).toHaveCount(2);
  await expect(waiting).toContainText("przejrzane, nadal czeka");
  await expect(nav).toHaveAccessibleName("Sesje agentów");
  await expect(nav.locator(".nav-badge")).toHaveCount(0);

  // „Przejrzane” przetrwa przeładowanie, ale nowe czekanie znów się liczy.
  await page.reload();
  await expect(navButton(page)).toHaveAccessibleName("Sesje agentów");
  await page.evaluate(
    ([k, extra]) => {
      const data = JSON.parse(localStorage.getItem(k)!);
      data.events.push(...extra);
      localStorage.setItem(k, JSON.stringify(data));
    },
    [
      "tuktuk-track-preview-agent-hooks-v1",
      [
        ev(now - 60_000, "codex", "PostToolUse", "perm", { cwd: dir("api") }),
        ev(now - 30_000, "codex", "PermissionRequest", "perm", {
          cwd: dir("api"),
          toolName: "Write",
        }),
      ],
    ] as const,
  );
  await page.reload();
  await expect(navButton(page)).toHaveAccessibleName(
    "Sesje agentów: czeka 1",
  );
});

test("bez hooków tablica jest pusta i wyjaśnia, skąd biorą się sesje", async ({
  page,
}) => {
  await openPreview(page, {
    agentHooks: { events },
    settings: { agentHooks: false },
  });
  await expect(navButton(page)).toHaveAccessibleName("Sesje agentów");
  await navButton(page).click();
  await expect(page.getByText("Brak sesji do pokazania.")).toBeVisible();
});

test("Ustawienia → Powiadomienia: czekanie agenta domyślnie tak, koniec nie", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Powiadomienia");
  const waiting = page.getByRole("checkbox", {
    name: "Powiadom, gdy sesja agenta AI czeka na odpowiedź lub zgodę",
  });
  const done = page.getByRole("checkbox", {
    name: "Powiadom, gdy sesja agenta AI skończy pracę",
  });
  await expect(waiting).toBeChecked();
  await expect(done).not.toBeChecked();
  await done.check();
  await expect
    .poll(() =>
      page.evaluate(
        (k) => JSON.parse(localStorage.getItem(k) ?? "{}").agentNotifyDone,
        SETTINGS_KEY,
      ),
    )
    .toBe(true);
});
