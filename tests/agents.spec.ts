import { test, expect } from "@playwright/test";

const key = "todo-track-preview-agents-v1";

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
        { kind: "codex", name: "api", cwd: "C:\\Projekty\\api", since: now - 30_000 },
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

  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
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
    JSON.parse(localStorage.getItem("todo-track-preview-settings-v1")!),
  );
  expect(stored).toEqual({ idleAgents: false, idleEnabled: false });
});
