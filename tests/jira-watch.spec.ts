import { test, expect } from "./fixtures/test";
import { openSettings } from "./fixtures/settings";

test("Jira status is shown on the task card and auto-import can be switched off", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(() => {
    localStorage.setItem(
      "tuktuk-track-preview-v1",
      JSON.stringify({
        tasks: [
          {
            id: 1,
            title: "Poprawa raportu",
            done: false,
            createdAt: Date.now(),
            jiraKey: "WMS-42",
            jiraUrl: "https://asistonwms.atlassian.net/browse/WMS-42",
            jiraStatus: "Code review",
          },
          { id: 2, title: "Lokalne", done: false, createdAt: Date.now() },
        ],
        sessions: [],
      }),
    );
  });
  await page.reload();
  const status = page.locator(".task-jira-status");
  await expect(status).toHaveCount(1);
  await expect(status).toHaveText("Code review");
  await expect(status).toHaveAttribute("title", "Status w Jirze: Code review");

  await openSettings(page, "Jira");
  await expect(page.getByRole("heading", { name: "Jira", exact: true })).toBeVisible();
  const auto = page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie");
  await expect(auto).toBeChecked();
  await auto.uncheck();
  await page.reload();
  await openSettings(page, "Jira");
  await expect(
    page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie"),
  ).not.toBeChecked();
});
