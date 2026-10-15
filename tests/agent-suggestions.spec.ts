import { test, expect, type Page } from "@playwright/test";
import {
  at,
  ids,
  NOW,
  openPreview,
  seed,
  SETTINGS_KEY,
  stored,
  TODAY,
} from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";

test.use({ timezoneId: "Europe/Warsaw" });

const MIN = 60_000;
const BINDINGS_KEY = "todo-track-preview-agent-bindings-v1";
const now = Date.parse(NOW);
/** Agent nad „Analizą importu” (klucz WMS-12 z gałęzi) od 5 minut. */
const importAgent = {
  kind: "claude",
  name: "Import",
  cwd: "C:\\Projekty\\wms",
  since: now - 5 * MIN,
  branch: "feature/WMS-12-import",
};
/** Agent w folderze przypisanym do „Code review” od 3 minut. */
const reviewAgent = {
  kind: "codex",
  name: "api",
  cwd: "C:\\Projekty\\api\\src",
  since: now - 3 * MIN,
  branch: "main",
};
const banner = (page: Page) =>
  page.getByRole("region", { name: "Propozycja przełączenia" });

/** Podgląd z agentami i przypisaniem folderu api → „Code review”. */
async function open(
  page: Page,
  agents: unknown[],
  options: Parameters<typeof openPreview>[1] = {},
) {
  await openPreview(page, { agents, ...options });
  await page.evaluate(
    ([k, id]) =>
      localStorage.setItem(
        k,
        JSON.stringify([{ folder: "C:\\Projekty\\api", taskId: id }]),
      ),
    [BINDINGS_KEY, ids.review] as const,
  );
  await page.reload();
}
const activeTask = async (page: Page) =>
  (await stored(page)).sessions.find((s: any) => s.endedAt === null)?.taskId ??
  null;

test("baner z jednym agentem: „Przełącz” zmienia timer, reguła 2 minut", async ({
  page,
}) => {
  await open(
    page,
    [
      importAgent,
      // Pracuje dopiero minutę: jeszcze bez propozycji.
      { ...reviewAgent, since: now - MIN },
    ],
    { active: { task: ids.docs, from: "11:00" } },
  );
  await expect(banner(page)).toBeVisible();
  await expect(banner(page)).toContainText("Agent pracuje nad innym zadaniem");
  await expect(banner(page)).toContainText(
    "Claude · Import pracuje nad „Analiza importu”. Timer mierzy „Dokumentacja API”.",
  );
  await expect(banner(page)).not.toContainText("Code review");
  expect(await activeTask(page)).toBe(ids.docs);

  await banner(page).getByRole("button", { name: "Przełącz" }).click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  const data = await stored(page);
  expect(
    data.sessions.find(
      (s: any) => s.taskId === ids.docs && s.startedAt === at(TODAY, "11:00"),
    ),
  ).toMatchObject({ endedAt: now });
  expect(await activeTask(page)).toBe(ids.analysis);
});

test("timer nie przełącza się sam, a mierzone zadanie nie dostaje propozycji", async ({
  page,
}) => {
  await open(page, [importAgent], {
    active: { task: ids.analysis, from: "11:00" },
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  await page.waitForTimeout(300);
  await expect(banner(page)).toHaveCount(0);
  // Stop: timer stoi, więc propozycja się pojawia, ale niczego nie uruchamia.
  await page.getByLabel("Stop: Analiza importu", { exact: true }).click();
  await expect(banner(page)).toContainText("Timer stoi.");
  await page.waitForTimeout(300);
  expect(await activeTask(page)).toBe(null);
});

test("baner z dwoma agentami pokazuje listę; „Nie teraz” i „Przełącz na”", async ({
  page,
}) => {
  await open(page, [importAgent, reviewAgent]);
  await expect(banner(page)).toContainText(
    "Agenci pracują nad innymi zadaniami",
  );
  await expect(banner(page)).toContainText("Timer stoi.");
  const items = banner(page).getByRole("listitem");
  await expect(items).toHaveCount(2);
  await expect(items.nth(0)).toContainText(
    "„Analiza importu” · Claude · Import",
  );
  await expect(items.nth(1)).toContainText("„Code review” · Codex · api");

  await banner(page)
    .getByRole("button", { name: "Nie teraz: Analiza importu" })
    .click();
  // Zostaje jedna propozycja: zwykły baner z jednym przyciskiem.
  await expect(banner(page)).toContainText("Agent pracuje nad innym zadaniem");
  await expect(banner(page)).toContainText("„Code review”");
  expect(await activeTask(page)).toBe(null);
  // Odrzucenie przetrwa przeładowanie (agent nadal pracuje).
  await page.reload();
  await expect(banner(page)).toContainText("„Code review”");
  await expect(banner(page)).not.toContainText("Analiza importu");

  await banner(page).getByRole("button", { name: "Przełącz" }).click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Code review", { exact: true }),
  ).toBeVisible();
  expect(await activeTask(page)).toBe(ids.review);
});

test("„Przełącz na” z listy zmienia timer na wybrane zadanie", async ({
  page,
}) => {
  await open(page, [importAgent, reviewAgent], {
    active: { task: ids.docs, from: "11:00" },
  });
  await banner(page)
    .getByRole("button", { name: "Przełącz na: Code review" })
    .click();
  await expect(
    page.getByLabel("Stop: Code review", { exact: true }),
  ).toBeVisible();
  expect(await activeTask(page)).toBe(ids.review);
  // Pozostała propozycja: „Analiza importu”, timer mierzy „Code review”.
  await expect(banner(page)).toContainText(
    "pracuje nad „Analiza importu”. Timer mierzy „Code review”.",
  );
});

test("„Nie proponuj dla tego folderu” zapisuje folder, odwracalnie w Ustawieniach", async ({
  page,
}) => {
  await open(page, [importAgent]);
  await banner(page)
    .getByRole("button", { name: "Nie proponuj dla tego folderu" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const settings = await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k)!),
    SETTINGS_KEY,
  );
  expect(settings).toEqual({ agentMutedFolders: ["C:\\Projekty\\wms"] });
  expect(await activeTask(page)).toBe(null);

  // Ustawienia → Czas pracy: lista folderów i przywrócenie propozycji.
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  const list = page.getByRole("list", {
    name: "Foldery bez propozycji agentów",
  });
  await expect(list).toContainText("C:\\Projekty\\wms");
  await page
    .getByRole("button", { name: "Proponuj znowu dla C:\\Projekty\\wms" })
    .click();
  await expect(list).toHaveCount(0);
  await expect(banner(page)).toBeVisible();

  // Wyłącznik całej funkcji.
  const toggle = page.getByLabel(
    "Proponuj przełączenie na zadanie, nad którym pracuje agent AI",
  );
  await expect(toggle).toBeChecked();
  await toggle.uncheck();
  await expect(banner(page)).toHaveCount(0);
  await toggle.check();
  await expect(banner(page)).toBeVisible();
});

test("widget pokazuje linię propozycji i przełącza jednym kliknięciem", async ({
  page,
}) => {
  await open(page, [importAgent], {
    active: { task: ids.docs, from: "11:00" },
  });
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  await expect(widget.getByRole("status")).toHaveText("Agent: Analiza importu");
  await widget
    .getByRole("button", { name: "Przełącz na: Analiza importu" })
    .click();
  await expect(widget.locator("strong")).toHaveText("Analiza importu");
  await expect(
    widget.getByRole("button", { name: "Przełącz na: Analiza importu" }),
  ).toHaveCount(0);
  expect(await activeTask(page)).toBe(ids.analysis);
});

test("baner bezczynności: „Przypisz przerwę do X” dzieli pomiar w chwili początku przerwy", async ({
  page,
}) => {
  await openDesktop(page, {
    data: seed({
      active: { task: ids.analysis, from: "11:00" },
    }),
    now: at(TODAY, "11:30"),
    settings: { idleAgents: false },
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  const review = {
    taskId: ids.review,
    title: "Code review",
    key: null,
    source: "folder",
  };
  const analysis = {
    taskId: ids.analysis,
    title: "Analiza importu",
    key: "WMS-12",
    source: "task",
  };
  const tick = async (time: string, userIdle: number, working: unknown[]) => {
    await page.clock.setFixedTime(at(TODAY, time));
    return backend<boolean>(page, "idleTick", { userIdle, working });
  };
  // Użytkownik wychodzi o 11:30; agent pracuje nad „Code review” (i nad mierzonym).
  expect(await tick("11:30", 0, [])).toBe(false);
  expect(await tick("11:40", 10 * MIN, [review, analysis])).toBe(false);
  expect(await tick("11:50", 20 * MIN, [review])).toBe(false);
  expect(await tick("12:00", 0, [])).toBe(true);
  const idle = page
    .getByRole("alert")
    .filter({ hasText: "Wracasz po przerwie?" });
  await expect(idle).toBeVisible();
  await expect(
    idle.getByRole("button", { name: "Przypisz przerwę do „Analiza importu”" }),
  ).toHaveCount(0);
  await idle
    .getByRole("button", { name: "Przypisz przerwę do „Code review”" })
    .click();
  await expect(idle).toHaveCount(0);
  await expect(
    page.getByLabel("Stop: Code review", { exact: true }),
  ).toBeVisible();
  const snap = await backend<any>(page, "snapshot");
  expect(
    snap.sessions.find((s: any) => s.startedAt === at(TODAY, "11:00")),
  ).toMatchObject({ taskId: ids.analysis, endedAt: at(TODAY, "11:30") });
  expect(
    snap.sessions.find((s: any) => s.startedAt === at(TODAY, "11:30")),
  ).toMatchObject({ taskId: ids.review, endedAt: null });
  const calls = await backend<any[]>(page, "calls");
  expect(calls.filter((c) => c.cmd === "idle_resolve")).toEqual([
    { cmd: "idle_resolve", args: { trim: false, assign: ids.review } },
  ]);
});
