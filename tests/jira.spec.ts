import { test, expect } from "./fixtures/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const w = window as any;
    w.isTauri = true;
    const boards = [
      { id: 1, name: "WMS" },
      { id: 2, name: "Foundation" },
    ];
    const config = {
      site: "https://asistonwms.atlassian.net",
      email: "test@example.invalid",
      displayName: "Użytkownik testowy",
      boards,
    };
    let snapshot = { tasks: [] as any[], sessions: [] as any[] };
    let maximized = false;
    w.testCalls = [];
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    w.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { label: "main" },
      },
      transformCallback: () => 1,
      unregisterCallback: () => {},
      invoke: async (cmd: string, args: any = {}) => {
        w.testCalls.push({ cmd, args });
        if (cmd === "get_snapshot") return structuredClone(snapshot);
        if (cmd === "plugin:window|is_maximized") return maximized;
        if (cmd === "plugin:window|toggle_maximize") {
          maximized = !maximized;
          return;
        }
        if (cmd.startsWith("plugin:")) return 1;
        if (cmd === "jira_settings") return { config, connected: true };
        if (cmd === "jira_boards") return { values: boards, nextStart: null };
        if (cmd === "jira_select_boards") {
          config.boards = args.boards;
          return;
        }
        if (cmd === "jira_issues")
          return {
            issues: [
              {
                key: "WMS-1",
                summary: "Obsługa przyjęcia towaru",
                status: "W toku",
                boards: ["WMS", "Foundation"],
                url: config.site + "/browse/WMS-1",
              },
              {
                key: "WMS-2",
                summary: "Weryfikacja etykiet",
                status: "Do zrobienia",
                boards: ["WMS"],
                url: config.site + "/browse/WMS-2",
              },
            ],
            warnings: w.jiraWarnings ?? [],
          };
        if (cmd === "jira_open") return;
        if (cmd === "jira_start") {
          let task = snapshot.tasks.find((t) => t.jiraKey === args.key);
          if (!task) {
            task = {
              id: snapshot.tasks.length + 1,
              title:
                args.key === "WMS-1"
                  ? "Obsługa przyjęcia towaru"
                  : "Weryfikacja etykiet",
              done: false,
              createdAt: Date.now(),
              jiraKey: args.key,
              jiraUrl: config.site + "/browse/" + args.key,
            };
            snapshot.tasks.push(task);
          }
          snapshot.sessions.forEach((s) => {
            if (s.endedAt === null) s.endedAt = Date.now();
          });
          snapshot.sessions.push({
            id: snapshot.sessions.length + 1,
            taskId: task.id,
            startedAt: Date.now(),
            endedAt: null,
          });
          return structuredClone(snapshot);
        }
        if (cmd === "mutate" && args.action === "stop") {
          snapshot.sessions.forEach((s) => {
            if (s.endedAt === null) s.endedAt = Date.now();
          });
          return structuredClone(snapshot);
        }
        throw Error("Unexpected test command " + cmd);
      },
    };
  });
});

test("Jira view selects multiple boards, searches, opens and starts a linked timer", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Jira", exact: true }).click();
  await expect(page.locator(".jira-issue")).toHaveCount(2);
  await expect(page.locator(".jira-board-chips")).toContainText("Foundation");
  await page.getByRole("button", { name: "Wybierz tablice" }).click();
  await expect(page.getByRole("checkbox", { name: /WMS/ })).toBeChecked();
  await expect(
    page.getByRole("checkbox", { name: /Foundation/ }),
  ).toBeChecked();
  await page.getByRole("button", { name: "Zapisz tablice" }).click();
  await page.getByLabel("Szukaj moich zgłoszeń").fill("etykiet");
  await expect(page.locator(".jira-issue")).toHaveCount(1);
  await page
    .getByRole("button", { name: "Otwórz w Jirze: WMS-2", exact: true })
    .click();
  await page.getByLabel("Szukaj moich zgłoszeń").fill("");
  await page
    .getByRole("button", { name: "Rozpocznij: WMS-1", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Zatrzymaj: WMS-1", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Rozpocznij: WMS-2", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Zatrzymaj: WMS-2", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(page.locator(".active-title")).toHaveText("Weryfikacja etykiet");
  await page
    .getByRole("button", {
      name: "Przedziały: Weryfikacja etykiet",
      exact: true,
    })
    .click();
  await expect(
    page.getByRole("button", { name: "Otwórz w Jirze: WMS-2", exact: true }),
  ).toBeVisible();
  const calls = await page.evaluate(() => (window as any).testCalls);
  expect(
    calls.some(
      (c: any) => c.cmd === "jira_open" && c.args.url.endsWith("/browse/WMS-2"),
    ),
  ).toBe(true);
});

test("custom titlebar dispatches window actions and Jira shows partial failure", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.locator(".titlebar")).toBeVisible();
  await page
    .getByRole("button", { name: "Maksymalizuj okno", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Przywróć rozmiar okna", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Minimalizuj okno", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknij okno do traya", exact: true })
    .click();
  await page.evaluate(() => {
    (window as any).jiraWarnings = ["Foundation: Brak uprawnień."];
  });
  await page.getByRole("button", { name: "Jira", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Nie wszystkie tablice");
  await expect(page.locator(".jira-issue")).toHaveCount(2);
  await page.screenshot({ path: "test-results/jira-dark.png", fullPage: true });
  await page.setViewportSize({ width: 760, height: 600 });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  const calls = await page.evaluate(() =>
    (window as any).testCalls.map((c: any) => c.cmd),
  );
  expect(calls).toContain("plugin:window|minimize");
  expect(calls).toContain("plugin:window|close");
});

test("reminder provides snooze and focuses task entry through backend action", async ({
  page,
}) => {
  await page.addInitScript(() => {
    (window as any).__TAURI_INTERNALS__.metadata.currentWindow.label =
      "reminder";
    const original = (window as any).__TAURI_INTERNALS__.invoke;
    (window as any).__TAURI_INTERNALS__.invoke = async (
      cmd: string,
      args: any,
    ) => {
      if (cmd === "reminder_action") {
        (window as any).testCalls.push({ cmd, args });
        return;
      }
      return original(cmd, args);
    };
  });
  await page.setViewportSize({ width: 370, height: 210 });
  await page.goto("/#reminder");
  await expect(
    page.getByRole("heading", { name: "Nad czym teraz pracujesz?" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Za 5 minut", exact: true }).click();
  await page
    .getByRole("button", { name: "Wpisz zadanie", exact: true })
    .click();
  await page.screenshot({ path: "test-results/reminder.png" });
  expect(
    await page.evaluate(() =>
      (window as any).testCalls
        .filter((c: any) => c.cmd === "reminder_action")
        .map((c: any) => c.args.action),
    ),
  ).toEqual(["snooze", "open"]);
});
