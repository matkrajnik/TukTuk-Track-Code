import { test, expect, type Page } from "@playwright/test";
import { ids, NOW, openPreview } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";

/**
 * Podpowiedzi z aktywnego okna (PLAN_1.7.md, W3). Rozpoznanie tytułów i reguła
 * 2 minut są w testach Rust (`foreground.rs`); tutaj zastępcze IPC podaje gotową
 * propozycję ze źródłem „Okno” (`suggest::refresh`), a sprawdzamy baner, widget
 * i odpowiedzi.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const MIN = 60_000;
const now = Date.parse(NOW);
const analysis = {
  taskId: ids.analysis,
  title: "Analiza importu",
  key: "WMS-12",
  source: "task",
  done: false,
};
/** Chrome z WMS-12 w tytule od 3 minut (bez folderu). */
const chrome = {
  task: analysis,
  agents: [
    { kind: "window", name: "Google Chrome", cwd: "", since: now - 3 * MIN },
  ],
};
/** VS Code z folderem wms (gałąź → WMS-12) od 3 minut. */
const vscode = {
  task: analysis,
  agents: [
    {
      kind: "window",
      name: "Visual Studio Code · wms",
      cwd: "C:\\Projekty\\wms",
      since: now - 3 * MIN,
    },
  ],
};
const banner = (page: Page) =>
  page.getByRole("region", { name: "Propozycja przełączenia" });
const activeTask = async (page: Page) =>
  (await backend<any>(page, "snapshot")).sessions.find(
    (s: any) => s.endedAt === null,
  )?.taskId ?? null;

test("baner ze źródłem „Okno”: zdarzenie backendu, „Przełącz” zmienia timer", async ({
  page,
}) => {
  await openDesktop(page, { settings: { windowSuggest: true } });
  // Baner nasłuchuje już zdarzenia propozycji.
  await expect
    .poll(async () =>
      (await backend<any[]>(page, "calls")).some(
        (c) =>
          c.cmd === "plugin:event|listen" &&
          c.args.event === "agent-suggestions",
      ),
    )
    .toBe(true);
  await expect(banner(page)).toHaveCount(0);

  // Pętla co 5 s wysyła nową listę zdarzeniem `agent-suggestions`.
  await backend(page, "suggest", [chrome]);
  await expect(banner(page)).toBeVisible();
  await expect(banner(page)).toContainText(
    "Pracujesz teraz przy innym zadaniu?",
  );
  await expect(banner(page)).toContainText(
    "Okno · Google Chrome pokazuje „Analiza importu”. Timer stoi. Przełączyć pomiar?",
  );
  // Bez folderu nie ma wyciszania ani zapamiętania folderu.
  await expect(
    banner(page).getByRole("button", { name: "Nie proponuj dla tego folderu" }),
  ).toHaveCount(0);
  await expect(
    banner(page).getByRole("button", { name: "Zapamiętaj dla tego folderu" }),
  ).toHaveCount(0);
  expect(await activeTask(page)).toBeNull();

  await banner(page).getByRole("button", { name: "Przełącz" }).click();
  await expect(banner(page)).toHaveCount(0);
  expect(await activeTask(page)).toBe(ids.analysis);
  const calls = await backend<any[]>(page, "calls");
  expect(calls.find((c) => c.cmd === "agent_switch")?.args).toEqual({
    taskId: ids.analysis,
    key: "WMS-12",
  });
});

test("okno edytora: „Nie teraz” i wyciszenie folderu jak u agentów", async ({
  page,
}) => {
  await openDesktop(page, {
    settings: { windowSuggest: true },
    suggestions: [vscode],
  });
  await expect(banner(page)).toContainText(
    "Okno · Visual Studio Code · wms pokazuje „Analiza importu”.",
  );
  await banner(page).getByRole("button", { name: "Nie teraz" }).click();
  await expect(banner(page)).toHaveCount(0);
  let calls = await backend<any[]>(page, "calls");
  expect(calls.find((c) => c.cmd === "agent_dismiss")?.args).toEqual({
    taskId: ids.analysis,
    key: "WMS-12",
  });

  // Kolejna obserwacja: „Nie proponuj dla tego folderu” zapisuje folder edytora.
  await backend(page, "suggest", [vscode]);
  await banner(page)
    .getByRole("button", { name: "Nie proponuj dla tego folderu" })
    .click();
  await expect
    .poll(async () =>
      (await backend<any[]>(page, "calls"))
        .filter((c) => c.cmd === "set_settings")
        .map((c) => c.args.patch.agentMutedFolders),
    )
    .toContainEqual(["C:\\Projekty\\wms"]);
  calls = await backend<any[]>(page, "calls");
  expect(calls.some((c) => c.cmd === "agent_switch")).toBe(false);
});

test("widget pokazuje linię „Okno: …” i przełącza jednym kliknięciem", async ({
  page,
}) => {
  await openDesktop(page, {
    settings: { windowSuggest: true },
    suggestions: [chrome],
    window: "widget",
  });
  const widget = page.locator("#widget");
  await expect(widget.getByRole("status")).toHaveText("Okno: Analiza importu");
  await widget
    .getByRole("button", { name: "Przełącz na: Analiza importu" })
    .click();
  await expect(widget.locator("strong")).toHaveText("Analiza importu");
  expect(await activeTask(page)).toBe(ids.analysis);
});

test("ustawienie podpowiedzi z okna jest domyślnie wyłączone", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const toggle = page.getByLabel(
    "Podpowiadaj zadanie na podstawie aktywnego okna",
  );
  await expect(toggle).not.toBeChecked();
  await expect(page.getByText("Tytuły zostają tylko w pamięci")).toBeVisible();
  await toggle.check();
  await page.reload();
  await openSettings(page, "Czas pracy");
  await expect(
    page.getByLabel("Podpowiadaj zadanie na podstawie aktywnego okna"),
  ).toBeChecked();
});
