import { test, expect, type Page } from "./fixtures/test";
import {
  AI_KEY,
  at,
  ids,
  NOW,
  openPreview,
  stored,
  TODAY,
  tsFixture,
  tsSeed,
} from "./fixtures/seed";
import { previewTsKey } from "../src/store";

/**
 * Asystent przypisań (PLAN_BACKLOG.md, A4) na symulowanym silniku AI
 * (`src/preview/ai.ts`): baner sesji agenta bez dopasowania (Przypisz folder,
 * Przełącz timer, Nie), „Podpowiedz” w polu „Zadanie TS” kafelka i „Podpowiedz
 * wszystkie” w podglądzie „Wyślij do TS”. Żaden test nie uruchamia prawdziwego
 * claude ani codex i nie łączy się z TS.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });
test.describe.configure({ timeout: 60_000 });

const MIN = 60_000;
const BINDINGS_KEY = "todo-track-preview-agent-bindings-v1";
const now = Date.parse(NOW);
const claude = { path: "C:\\Users\\ala\\.local\\bin\\claude.exe", version: "2.1.304" };
const settings = { aiEngine: "claude" };

/** Sesja Claude Code bez gałęzi z kluczem i bez przypisania folderu: heurystyka nic nie da. */
const session = {
  kind: "claude",
  name: "export-api",
  cwd: "C:\\Projekty\\export-api",
  since: now - 5 * MIN,
  branch: "main",
  sessionId: "sess-1",
  firstPrompt: "napraw eksport CSV",
};
const reply = (taskId: number | null, confidence: number, reason = "Pasuje do folderu.") => ({
  claude,
  script: { assign: { value: { taskId, confidence, reason } } },
});

const banner = (page: Page) =>
  page.getByRole("region", { name: "Propozycja przełączenia" });
const calls = async (page: Page, kind: string) =>
  ((await page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").calls ?? [],
    AI_KEY,
  )) as { kind: string }[]).filter((c) => c.kind === kind).length;
const activeTask = async (page: Page) =>
  (await stored(page)).sessions.find((s: { endedAt: number | null }) => s.endedAt === null)
    ?.taskId ?? null;
/** Czas na przeliczenie propozycji w podglądzie, zanim sprawdzimy, że nic się nie stało. */
async function settle(page: Page) {
  await expect(page.getByRole("heading", { name: "Zadania", level: 1 })).toBeVisible();
  await page.waitForTimeout(400);
}

// ---------------------------------------------------------------------------
// Baner sesji agenta

test("baner asystenta: do którego zadania należy sesja, trzy przyciski, jedno wywołanie", async ({
  page,
}) => {
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.review, 0.82),
  });
  const b = banner(page);
  await expect(b).toBeVisible();
  await expect(b.getByRole("heading")).toHaveText(
    "Asystent: do którego zadania należy ta sesja?",
  );
  await expect(b).toContainText(
    "Agent w „export-api” pracuje nad „napraw eksport CSV” — to zadanie „Code review”?",
  );
  await expect(b.getByRole("button")).toHaveText([
    "Przypisz folder",
    "Przełącz timer",
    "Nie",
  ]);
  // Baner nie przełączył timera sam, a model dostał jedno pytanie.
  expect(await activeTask(page)).toBeNull();
  expect(await calls(page, "assign")).toBe(1);
  // Przeliczenia banera (zdarzenia, zmiana timera) nie wołają modelu ponownie.
  await page.evaluate(() => window.dispatchEvent(new Event("agent-suggestions")));
  await settle(page);
  expect(await calls(page, "assign")).toBe(1);
});

test("„Przypisz folder” zapisuje przypisanie, a kolejne dopasowanie idzie bez modelu", async ({
  page,
}) => {
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.review, 0.9),
  });
  await banner(page).getByRole("button", { name: "Przypisz folder" }).click();
  await expect
    .poll(() =>
      page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "[]"), BINDINGS_KEY),
    )
    .toEqual([{ folder: "C:\\Projekty\\export-api", taskId: ids.review }]);
  // Teraz dopasowuje folder, więc to zwykła propozycja agenta (bez źródła „Asystent”).
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Agent pracuje nad innym zadaniem",
  );
  expect(await activeTask(page)).toBeNull();
  expect(await calls(page, "assign")).toBe(1);
});

test("„Nie” wycisza parę: baner znika i nie wraca po przeładowaniu", async ({ page }) => {
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.review, 0.9),
  });
  await banner(page).getByRole("button", { name: "Nie", exact: true }).click();
  await expect(banner(page)).toHaveCount(0);
  expect(await activeTask(page)).toBeNull();
  await page.reload();
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  // Nic nie zostało przypisane ani przełączone.
  expect(await page.evaluate((k) => localStorage.getItem(k), BINDINGS_KEY)).toBeNull();
  expect(await activeTask(page)).toBeNull();
});

test("„Przełącz timer” zaczyna pomiar zaproponowanego zadania", async ({ page }) => {
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.labels, 0.7),
  });
  await expect(banner(page)).toContainText("„Wdrożenie etykiet”");
  await banner(page).getByRole("button", { name: "Przełącz timer" }).click();
  await expect.poll(() => activeTask(page)).toBe(ids.labels);
  // Timer mierzy już to zadanie: baner znika.
  await expect(banner(page)).toHaveCount(0);
});

test("próg pewności: poniżej 0,6 i odpowiedź „bez zadania” nie dają banera", async ({
  page,
}) => {
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.review, 0.59),
  });
  await expect.poll(() => calls(page, "assign")).toBe(1);
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(null, 0.95, "Żadne zadanie nie pasuje."),
  });
  await expect.poll(() => calls(page, "assign")).toBe(1);
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  // Zadanie spoza listy otwartych (ukończone) też jest odrzucane.
  await openPreview(page, {
    agents: [session],
    settings,
    ai: reply(ids.docs, 0.95),
  });
  await expect.poll(() => calls(page, "assign")).toBe(1);
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
});

test("bez wywołania: heurystyka dopasowała, przełącznik wyłączony, krótka praca, wyciszony folder", async ({
  page,
}) => {
  const ai = reply(ids.review, 0.9);
  // Gałąź z kluczem WMS-12: dopasowanie z `agents::resolve`, model nie jest pytany.
  await openPreview(page, {
    agents: [{ ...session, branch: "feature/WMS-12-import" }],
    settings,
    ai,
  });
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Agent pracuje nad innym zadaniem",
  );
  await settle(page);
  expect(await calls(page, "assign")).toBe(0);
  // Przełącznik „Podpowiedzi przypisań” wyłączony.
  await openPreview(page, {
    agents: [session],
    settings: { ...settings, aiAssignSuggest: false },
    ai,
  });
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  expect(await calls(page, "assign")).toBe(0);
  // Sesja pracuje krócej niż 2 minuty.
  await openPreview(page, {
    agents: [{ ...session, since: now - MIN }],
    settings,
    ai,
  });
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  expect(await calls(page, "assign")).toBe(0);
  // Wyciszony folder (z podfolderami).
  await openPreview(page, {
    agents: [session],
    settings: { ...settings, agentMutedFolders: ["C:\\Projekty"] },
    ai,
  });
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  expect(await calls(page, "assign")).toBe(0);
  // Bez wybranego silnika asystent nic nie robi.
  await openPreview(page, { agents: [session], ai });
  await settle(page);
  await expect(banner(page)).toHaveCount(0);
  expect(await calls(page, "assign")).toBe(0);
});

test("pierwszy prompt z hooka trafia do podpowiedzi tylko przy włączonym asystencie", async ({
  page,
}) => {
  const hookEvents = [
    {
      at: now - 5 * MIN,
      agent: "claude",
      event: "UserPromptSubmit",
      sessionId: "hook-1",
      cwd: "C:\\Projekty\\import-csv",
      prompt: "dodaj walidację importu",
    },
    {
      at: now - 4 * MIN,
      agent: "claude",
      event: "PreToolUse",
      sessionId: "hook-1",
      cwd: "C:\\Projekty\\import-csv",
    },
    {
      at: now - MIN / 2,
      agent: "claude",
      event: "PreToolUse",
      sessionId: "hook-1",
      cwd: "C:\\Projekty\\import-csv",
    },
  ];
  await openPreview(page, {
    settings,
    ai: reply(ids.analysis, 0.8),
    agentHooks: { enabled: true, events: hookEvents },
  });
  await expect(banner(page)).toContainText("„import-csv”");
  await expect(banner(page)).toContainText("„dodaj walidację importu”");
  expect(await calls(page, "assign")).toBe(1);
});

// ---------------------------------------------------------------------------
// Zadanie TS w kafelku

const card = (page: Page, title: string) =>
  page
    .locator(".task-card")
    .filter({ has: page.locator(".task-title", { hasText: title }) });
const expand = (page: Page, title: string) =>
  page.getByRole("button", { name: "Przedziały: " + title }).click();
const tsReply = (values: Record<string, unknown>) => ({
  claude,
  script: { ts_task: { values } },
});
const detailsOf = async (page: Page, id: number) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === id).details;
const tsState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), previewTsKey);

test("„Podpowiedz” w polu Zadanie TS: sugestia z uzasadnieniem, zapis dopiero po wyborze", async ({
  page,
}) => {
  await openPreview(page, {
    settings,
    ts: tsFixture(),
    ai: tsReply({
      [ids.review]: { tsTaskId: "1103", confidence: 0.85, reason: "Przegląd kodu to Code review." },
    }),
  });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await field.getByRole("button", { name: "Podpowiedz zadanie TS: Code review" }).click();
  const suggestion = field.getByRole("status", { name: "Podpowiedź zadania TS: Code review" });
  await expect(suggestion).toContainText("Asystent proponuje: Code review");
  await expect(suggestion).toContainText("Przegląd kodu to Code review.");
  // Projekt „Foundation” nie ma mapowania: pole domyślne zaznaczone.
  const asDefault = suggestion.getByRole("checkbox", {
    name: "Ustaw jako domyślne dla projektu „Foundation”",
  });
  await expect(asDefault).toBeChecked();
  // Do wyboru nic nie jest zapisane.
  expect((await detailsOf(page, ids.review)).tsTaskId ?? "").toBe("");
  expect((await tsState(page)).projectMap ?? []).not.toContainEqual(
    expect.objectContaining({ project: "Foundation" }),
  );
  await suggestion.getByRole("button", { name: "Użyj podpowiedzi" }).click();
  await expect(suggestion).toHaveCount(0);
  const saved = await detailsOf(page, ids.review);
  expect([saved.tsTaskId, saved.tsTaskName]).toEqual(["1103", "Code review"]);
  expect((await tsState(page)).projectMap).toContainEqual({
    project: "Foundation",
    tsTaskId: "1103",
    tsTaskName: "Code review",
  });
  await expect(card(page, "Code review").locator(".task-ts")).toHaveText("TS: Code review");
  expect(await calls(page, "ts_task")).toBe(1);
});

test("„Podpowiedz”: „Odrzuć” nic nie zapisuje, a mapowany projekt nie ma zaznaczonego domyślnego", async ({
  page,
}) => {
  const map = [
    ...tsFixture().projectMap,
    { project: "Foundation", tsTaskId: "1102", tsTaskName: "Błędy i wsparcie" },
  ];
  await openPreview(page, {
    settings,
    ts: { ...tsFixture(), projectMap: map },
    ai: tsReply({
      [ids.review]: { tsTaskId: "1103", confidence: 0.7, reason: "Code review." },
    }),
  });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await field.getByRole("button", { name: "Podpowiedz zadanie TS: Code review" }).click();
  const suggestion = field.getByRole("status", { name: "Podpowiedź zadania TS: Code review" });
  await expect(suggestion).toBeVisible();
  await expect(
    suggestion.getByRole("checkbox", {
      name: "Ustaw jako domyślne dla projektu „Foundation”",
    }),
  ).not.toBeChecked();
  await suggestion.getByRole("button", { name: "Odrzuć" }).click();
  await expect(suggestion).toHaveCount(0);
  expect((await detailsOf(page, ids.review)).tsTaskId ?? "").toBe("");
  expect((await tsState(page)).projectMap).toEqual(map);
});

test("„Podpowiedz”: niska pewność i odpowiedź spoza listy to komunikat, nie sugestia", async ({
  page,
}) => {
  await openPreview(page, {
    settings,
    ts: tsFixture(),
    ai: tsReply({
      [ids.review]: { tsTaskId: "1103", confidence: 0.4, reason: "Nie jestem pewien." },
      [ids.labels]: { tsTaskId: "9999", confidence: 0.9, reason: "Takie." },
    }),
  });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await field.getByRole("button", { name: "Podpowiedz zadanie TS: Code review" }).click();
  const suggestion = field.getByRole("status", { name: "Podpowiedź zadania TS: Code review" });
  await expect(suggestion).toContainText("Asystent nie znalazł pewnego dopasowania.");
  await expect(suggestion).toContainText("Nie jestem pewien.");
  await expect(suggestion.getByRole("button", { name: "Użyj podpowiedzi" })).toHaveCount(0);
  await suggestion.getByRole("button", { name: "Zamknij" }).click();
  expect((await detailsOf(page, ids.review)).tsTaskId ?? "").toBe("");
  await expand(page, "Wdrożenie etykiet");
  const labels = page.getByRole("region", { name: "Zadanie TS: Wdrożenie etykiet" });
  await labels.getByRole("button", { name: "Podpowiedz zadanie TS: Wdrożenie etykiet" }).click();
  await expect(
    labels.getByRole("status", { name: "Podpowiedź zadania TS: Wdrożenie etykiet" }),
  ).toContainText("Asystent wskazał zadanie spoza listy TS.");
});

test("„Podpowiedz” pokazuje błąd silnika, a przycisku nie ma przy wyłączonym przełączniku ani silniku", async ({
  page,
}) => {
  await openPreview(page, {
    settings,
    ts: tsFixture(),
    ai: { claude, script: { ts_task: { error: "limit" } } },
  });
  await expand(page, "Code review");
  const field = page.getByRole("region", { name: "Zadanie TS: Code review" });
  await field.getByRole("button", { name: "Podpowiedz zadanie TS: Code review" }).click();
  await expect(page.getByRole("alert").filter({ hasText: "Limit subskrypcji" })).toBeVisible();
  // Przełącznik „Podpowiedzi przypisań” wyłączony.
  await openPreview(page, {
    settings: { ...settings, aiAssignSuggest: false },
    ts: tsFixture(),
    ai: { claude },
  });
  await expand(page, "Code review");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /^Podpowiedz/ })).toHaveCount(0);
  // Bez wybranego silnika.
  await openPreview(page, {
    settings: { aiEngine: "off" },
    ts: tsFixture(),
    ai: { claude },
  });
  await expand(page, "Code review");
  await expect(
    page.getByRole("combobox", { name: "Zadanie TS: Code review" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /^Podpowiedz/ })).toHaveCount(0);
});

// ---------------------------------------------------------------------------
// „Wyślij do TS”: Podpowiedz wszystkie

const dialog = (page: Page) => page.getByRole("dialog", { name: "Wyślij do TS" });
const warnings = (page: Page) =>
  dialog(page).getByRole("note", { name: "Ostrzeżenia wysyłki" });
async function openSend(page: Page) {
  await page.getByRole("button", { name: "Raporty dzienne", exact: true }).click();
  await page.getByRole("button", { name: "Raport TS", exact: true }).click();
  await page
    .getByRole("region", { name: "Raport TS" })
    .getByRole("button", { name: "Wyślij do TS" })
    .click();
  await expect(dialog(page)).toBeVisible();
}
/** Dzień z dwoma zadaniami bez zadania TS: „Code review” i „Dokumentacja API” (obie „Foundation”). */
function twoUnmapped() {
  const data = tsSeed();
  data.sessions.push({
    id: 700,
    taskId: ids.docs,
    startedAt: at(TODAY, "09:00"),
    endedAt: at(TODAY, "09:30"),
  });
  return data;
}
const emptyTs = () => ({ ...tsFixture(), filled: {} });

test("„Podpowiedz wszystkie”: sugestie dla każdego zadania bez zadania TS, zapis dopiero po wyborze", async ({
  page,
}) => {
  await openPreview(page, {
    data: twoUnmapped(),
    settings,
    ts: emptyTs(),
    ai: tsReply({
      [ids.review]: { tsTaskId: "1103", confidence: 0.9, reason: "To przegląd kodu." },
      [ids.docs]: { tsTaskId: "1102", confidence: 0.75, reason: "Dokumentacja to wsparcie." },
    }),
  });
  await openSend(page);
  await expect(warnings(page)).toContainText("Bez zadania TS: 2 zadania");
  await warnings(page).getByRole("button", { name: "Podpowiedz wszystkie" }).click();
  const review = warnings(page).getByRole("status", {
    name: "Podpowiedź zadania TS: Code review",
  });
  const docs = warnings(page).getByRole("status", {
    name: "Podpowiedź zadania TS: Dokumentacja API",
  });
  await expect(review).toContainText("Asystent proponuje: Code review");
  await expect(review).toContainText("To przegląd kodu.");
  await expect(docs).toContainText("Asystent proponuje: Błędy i wsparcie");
  expect(await calls(page, "ts_task")).toBe(2);
  // Projekt „Foundation” nie ma mapowania: „domyślne dla projektu” zaznaczone w obu wierszach.
  const defaults = warnings(page).getByRole("checkbox", {
    name: "domyślne dla projektu „Foundation”",
  });
  await expect(defaults).toHaveCount(2);
  await expect(defaults.first()).toBeChecked();
  await expect(defaults.last()).toBeChecked();
  // Do wyboru nic nie zostało zapisane.
  expect((await detailsOf(page, ids.review)).tsTaskId ?? "").toBe("");
  expect((await detailsOf(page, ids.docs)).tsTaskId ?? "").toBe("");
  expect((await tsState(page)).projectMap ?? []).not.toContainEqual(
    expect.objectContaining({ project: "Foundation" }),
  );
  // Wybór jednej sugestii (bez domyślnego projektu) zapisuje tylko to zadanie, druga czeka.
  await defaults.first().uncheck();
  await warnings(page).getByRole("button", { name: "Użyj podpowiedzi: Code review" }).click();
  await expect(warnings(page)).toContainText("Bez zadania TS: 1 zadanie");
  const saved = await detailsOf(page, ids.review);
  expect([saved.tsTaskId, saved.tsTaskName]).toEqual(["1103", "Code review"]);
  expect((await tsState(page)).projectMap ?? []).not.toContainEqual(
    expect.objectContaining({ project: "Foundation" }),
  );
  expect((await detailsOf(page, ids.docs)).tsTaskId ?? "").toBe("");
  await expect(docs).toBeVisible();
  // „Odrzuć podpowiedź” usuwa sugestię bez zapisu.
  await warnings(page)
    .getByRole("button", { name: "Odrzuć podpowiedź: Dokumentacja API" })
    .click();
  await expect(docs).toHaveCount(0);
  expect((await detailsOf(page, ids.docs)).tsTaskId ?? "").toBe("");
});

test("„Podpowiedz” przy jednym wierszu: wybór ręczny z listy zostaje dostępny", async ({
  page,
}) => {
  await openPreview(page, {
    data: twoUnmapped(),
    settings,
    ts: emptyTs(),
    ai: tsReply({
      [ids.review]: { tsTaskId: "1101", confidence: 0.2, reason: "Nie wiem." },
    }),
  });
  await openSend(page);
  await warnings(page)
    .getByRole("button", { name: "Podpowiedz zadanie TS: Code review" })
    .click();
  const suggestion = warnings(page).getByRole("status", {
    name: "Podpowiedź zadania TS: Code review",
  });
  await expect(suggestion).toContainText("Asystent nie znalazł pewnego dopasowania.");
  // Drugi wiersz bez podpowiedzi, a pole wyboru działa jak dotąd.
  await expect(
    warnings(page).getByRole("status", { name: "Podpowiedź zadania TS: Dokumentacja API" }),
  ).toHaveCount(0);
  await expect(
    warnings(page).getByRole("combobox", { name: "Przypisz zadanie TS: Code review" }),
  ).toBeVisible();
  expect(await calls(page, "ts_task")).toBe(1);
});

test("w podglądzie „Wyślij do TS” nie ma przycisków asystenta bez silnika", async ({ page }) => {
  await openPreview(page, {
    data: twoUnmapped(),
    settings: { aiEngine: "off" },
    ts: emptyTs(),
    ai: { claude },
  });
  await openSend(page);
  await expect(warnings(page)).toContainText("Bez zadania TS: 2 zadania");
  await expect(dialog(page).getByRole("button", { name: /^Podpowiedz/ })).toHaveCount(0);
});
