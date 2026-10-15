import { test, expect } from "@playwright/test";

test("Jira status is shown on the task card and auto-import can be switched off", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(() => {
    localStorage.setItem(
      "todo-track-preview-v1",
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

  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Jira", exact: true })).toBeVisible();
  const auto = page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie");
  await expect(auto).toBeChecked();
  await auto.uncheck();
  await page.reload();
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await expect(
    page.getByLabel("Dodawaj nowe zgłoszenia przypisane do mnie"),
  ).not.toBeChecked();
});
