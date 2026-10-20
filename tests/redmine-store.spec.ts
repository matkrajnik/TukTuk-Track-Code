import { test, expect, type Page } from "@playwright/test";
import { issueRef } from "../src/issue";
import {
  previewRedmineIssues,
  previewRedmineKey,
  previewRedmineProjects,
  previewRedmineSite,
  previewRedmineStatuses,
  redmineKeyRejected,
  redmineNotConnected,
  redmineSiteError,
  redmineWriteDisabled,
} from "../src/store";
import {
  ids,
  openPreview,
  REDMINE_KEY,
  REDMINE_SITE,
  redmineIds,
  redmineSeed,
  SETTINGS_KEY,
  stored,
} from "./fixtures/seed";

/**
 * Symulowany Redmine podglądu (PLAN_1.9.md, R6): funkcje `redmine*` ze
 * `store.ts` wywołane w oknie podglądu (bez Rusta i bez sieci) — połączenie,
 * błędy (401, 403, 422, wyłączone zapisy), zmiana statusu, przypisanie,
 * komentarz, `issueRef` zadań i nasłuch zmian z innego okna. Adres jest
 * przykładowy; żaden test nie łączy się z Redmine.
 */
test.use({ timezoneId: "Europe/Warsaw" });

/** Wynik funkcji ze `store.ts` w oknie albo `{ error }` z komunikatem błędu. */
type Outcome<T> = { ok: T } | { error: string };
async function call<T>(
  page: Page,
  name: string,
  ...args: unknown[]
): Promise<Outcome<T>> {
  return page.evaluate(
    async ([fn, list]) => {
      const store: any = await import("/src/store.ts");
      try {
        return { ok: await store[fn](...list) };
      } catch (e) {
        return { error: String(e instanceof Error ? e.message : e) };
      }
    },
    [name, args] as const,
  ) as Promise<Outcome<T>>;
}
async function ok<T = any>(page: Page, name: string, ...args: unknown[]) {
  const result = await call<T>(page, name, ...args);
  if ("error" in result) throw new Error(`${name}: ${result.error}`);
  return result.ok;
}
async function fails(page: Page, name: string, ...args: unknown[]) {
  const result = await call(page, name, ...args);
  expect(result, name).toHaveProperty("error");
  return (result as { error: string }).error;
}
const redmineState = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k) ?? "{}"), REDMINE_KEY);
async function setRedmine(page: Page, state: Record<string, unknown>) {
  await page.evaluate(
    ([k, v]) => localStorage.setItem(k, v),
    [REDMINE_KEY, JSON.stringify(state)] as const,
  );
}

test("przykładowe dane: adres, statusy, projekty i siedem zgłoszeń jak w planie", async ({
  page,
}) => {
  expect(previewRedmineKey).toBe(REDMINE_KEY);
  expect(previewRedmineSite).toBe(REDMINE_SITE);
  expect(previewRedmineStatuses.map((s) => [s.name, s.isClosed])).toEqual([
    ["New", false],
    ["In Progress", false],
    ["Feedback", false],
    ["Resolved", true],
    ["Closed", true],
  ]);
  expect(previewRedmineProjects).toHaveLength(3);
  expect(previewRedmineProjects.filter((p) => p.parentId !== null)).toHaveLength(1);
  expect(previewRedmineIssues).toHaveLength(7);

  await openPreview(page);
  const account = await ok(page, "redmineAccount");
  expect(account).toEqual({
    config: {
      site: REDMINE_SITE,
      login: "jkowalski",
      userId: 12,
      displayName: "Jan Kowalski",
      acceptInvalidCert: false,
      projects: [{ id: 7, name: "Magazyn WMS", identifier: "wms", parentId: null }],
    },
    connected: true,
  });
  expect(await ok(page, "redmineProjects")).toHaveLength(3);
  expect(
    (await ok(page, "redmineStatuses", true)).map((s: any) => s.name),
  ).toEqual(["New", "In Progress", "Feedback", "Resolved", "Closed"]);

  // Śledzony projekt z podprojektem: wszystkie (także zamknięte i cudze).
  const all = await ok(page, "redmineIssues", 7, false, false);
  expect(all.issues.map((i: any) => i.id).sort()).toEqual([
    1234, 1240, 1245, 1250, 1251, 1260,
  ]);
  expect(all.tracked).toEqual([7, 8]);
  const wms = all.issues.filter((i: any) => i.projectId === 7);
  expect(wms).toHaveLength(5);
  expect(wms.filter((i: any) => i.closed).map((i: any) => i.id)).toEqual([1245]);
  expect(
    new Set(wms.filter((i: any) => !i.closed).map((i: any) => i.status)).size,
  ).toBe(3);
  expect(wms.filter((i: any) => i.assignedToId !== 12).map((i: any) => i.id)).toEqual([
    1251,
  ]);
  expect(wms.filter((i: any) => i.dueDate).map((i: any) => i.id)).toEqual([1234]);
  expect(all.issues.find((i: any) => i.id === 1260).project).toBe("WMS – integracje");

  // „Moje” i otwarte: bez #1251 (Anny Nowak) i #1245 (zamknięte).
  const mine = await ok(page, "redmineIssues", 7, true, true);
  expect(mine.issues.map((i: any) => i.id).sort()).toEqual([1234, 1240, 1250, 1260]);
  expect(mine.groups.map((g: any) => g.status)).toEqual([
    "New",
    "In Progress",
    "Feedback",
  ]);
  // Moje ze wszystkich projektów: #1270 jest spoza śledzonych projektów.
  const everywhere = await ok(page, "redmineIssues", null, true, true);
  expect(
    everywhere.issues
      .filter((i: any) => !everywhere.tracked.includes(i.projectId))
      .map((i: any) => [i.id, i.project]),
  ).toEqual([[1270, "Portal klienta"]]);
});

test("połączenie: zły adres i klucz „zly” (401), połączenie, test i rozłączenie", async ({
  page,
}) => {
  await openPreview(page, { redmine: { connected: false, site: "" } });
  expect((await ok(page, "redmineAccount")).connected).toBe(false);
  expect(await fails(page, "redmineIssues", 7, true, true)).toBe(redmineNotConnected);
  expect(await fails(page, "redmineTest")).toBe(redmineNotConnected);

  expect(
    await fails(page, "redmineConnect", "https://redmine.example.pl/?a=1", "abc", false),
  ).toBe(redmineSiteError);
  expect(
    await fails(page, "redmineConnect", "https://redmine.example.pl", "zly", false),
  ).toBe(redmineKeyRejected);
  expect((await redmineState(page)).connected).toBe(false);

  const account = await ok(
    page,
    "redmineConnect",
    " https://redmine.example.pl/ ",
    "0123456789abcdef",
    false,
  );
  expect(account.connected).toBe(true);
  expect(account.config.site).toBe(REDMINE_SITE);
  expect(account.config.displayName).toBe("Jan Kowalski");
  // Nowy adres: śledzone projekty od nowa; klucz API nigdzie w localStorage.
  expect(account.config.projects).toEqual([]);
  expect(await page.evaluate(() => JSON.stringify({ ...localStorage }))).not.toContain(
    "0123456789abcdef",
  );
  expect(await ok(page, "redmineTest")).toBe(
    "Połączono jako Jan Kowalski (jkowalski), Redmine odpowiada.",
  );

  // Login i nazwa ze stanu symulacji.
  await setRedmine(page, {
    ...(await redmineState(page)),
    login: "anowak",
    displayName: "Anna Nowak",
  });
  expect(await ok(page, "redmineTest")).toBe(
    "Połączono jako Anna Nowak (anowak), Redmine odpowiada.",
  );

  const after = await ok(page, "redmineDisconnect");
  expect(after.connected).toBe(false);
  // Adres zostaje (jak konfiguracja w Rust), klucz znika.
  expect(after.config.site).toBe(REDMINE_SITE);
  expect(await fails(page, "redmineProjects")).toBe(redmineNotConnected);
});

test("zmiana statusu: przejścia bez bieżącego, status na kafelku i w grupach", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  const allowed = await ok(page, "redmineTransitions", redmineIds.importCsv);
  expect(allowed.map((s: any) => s.name)).toEqual([
    "In Progress",
    "Feedback",
    "Resolved",
    "Closed",
  ]);
  await ok(page, "redmineTransition", redmineIds.importCsv, 2);
  const task = (await stored(page)).tasks.find(
    (t: any) => t.id === redmineIds.importCsv,
  );
  expect(task.redmineStatus).toBe("In Progress");
  expect(
    (await redmineState(page)).issues.find((i: any) => i.id === 1234).status,
  ).toBe("In Progress");
  const groups = (await ok(page, "redmineIssues", 7, true, true)).groups;
  expect(groups.find((g: any) => g.status === "In Progress").ids).toContain(1234);

  // Zamknięcie z widoku: znika z listy otwartych.
  const closed = await ok(page, "redmineIssueTransition", 1234, 5);
  expect(closed).toMatchObject({ status: "Closed", statusId: 5, closed: true });
  const open = await ok(page, "redmineIssues", 7, true, true);
  expect(open.issues.some((i: any) => i.id === 1234)).toBe(false);

  // Status spoza listy i zadanie bez zgłoszenia Redmine.
  expect(await fails(page, "redmineIssueTransition", 1240, 99)).toBe(
    "Redmine odrzucił dane: Status is invalid",
  );
  expect(await fails(page, "redmineTransitions", ids.analysis)).toBe(
    "To zadanie nie jest powiązane ze zgłoszeniem Redmine.",
  );
});

test("błędy zapisu: 403, 422 i wyłączone zapisy (stan `write` i przełącznik)", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await setRedmine(page, { fail: 403 });
  expect(await fails(page, "redmineAssignMe", 1251)).toBe(
    "Brak uprawnień w Redmine do tej operacji.",
  );
  await setRedmine(page, { fail: 422 });
  expect(await fails(page, "redmineTransition", redmineIds.importCsv, 2)).toBe(
    "Redmine odrzucił dane: Status is invalid",
  );
  expect(
    (await stored(page)).tasks.find((t: any) => t.id === redmineIds.importCsv)
      .redmineStatus,
  ).toBe("New");

  // `write: false` w stanie symulacji blokuje jak `redmine::ensure_write`.
  await setRedmine(page, { write: false });
  for (const [name, args] of [
    ["redmineTransition", [redmineIds.importCsv, 2]],
    ["redmineTransitions", [redmineIds.importCsv]],
    ["redmineIssueTransition", [1234, 2]],
    ["redmineAssignMe", [1251]],
    ["redmineComment", [1234, "Uwaga"]],
  ] as const)
    expect(await fails(page, name, ...args)).toBe(redmineWriteDisabled);
  // Odczyty działają dalej.
  expect((await ok(page, "redmineIssues", 7, true, true)).issues.length).toBe(4);

  // Ten sam efekt daje przełącznik w Ustawieniach (`redmineWrite`).
  await setRedmine(page, {});
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ redmineWrite: false })),
    SETTINGS_KEY,
  );
  expect(await fails(page, "redmineAssignMe", 1251)).toBe(redmineWriteDisabled);
  expect((await redmineState(page)).issues).toBeUndefined();
});

test("przypisanie do mnie, komentarz i nazwa gałęzi", async ({ page }) => {
  await openPreview(page, { data: redmineSeed() });
  const before = await ok(page, "redmineIssues", 7, true, true);
  expect(before.issues.some((i: any) => i.id === 1251)).toBe(false);
  const issue = await ok(page, "redmineAssignMe", 1251);
  expect(issue).toMatchObject({
    id: 1251,
    assignedToId: 12,
    assignedTo: "Jan Kowalski",
  });
  const after = await ok(page, "redmineIssues", 7, true, true);
  expect(after.issues.some((i: any) => i.id === 1251)).toBe(true);
  expect(await fails(page, "redmineAssignMe", 4242)).toContain(
    "Nie znaleziono zgłoszenia",
  );

  await ok(page, "redmineComment", 1234, "  Linia 1\r\nLinia 2 ąę  ");
  expect((await redmineState(page)).comments).toEqual({
    "1234": ["Linia 1\nLinia 2 ąę"],
  });
  expect(await fails(page, "redmineComment", 1234, "   ")).toBe(
    "Wpisz treść komentarza.",
  );
  expect(await fails(page, "redmineComment", 1234, "x".repeat(4001))).toBe(
    "Komentarz może mieć najwyżej 4000 znaków.",
  );
  expect(await ok(page, "redmineBranchName", 1234)).toBe(
    "feature/1234-import-csv-zamowien",
  );
});

test("issueRef zadań z symulowanego Redmine i z Jiry", async ({ page }) => {
  await openPreview(page, { data: redmineSeed() });
  await ok(page, "redmineStart", 1270);
  const data = await stored(page);
  const byId = (id: number) => data.tasks.find((t: any) => t.id === id);
  expect(issueRef(byId(redmineIds.importCsv))).toEqual({
    system: "redmine",
    key: "#1234",
    url: `${REDMINE_SITE}/issues/1234`,
    status: "New",
    label: "Redmine",
  });
  expect(issueRef(byId(redmineIds.labels))).toMatchObject({
    system: "redmine",
    key: "#1240",
    status: "Feedback",
  });
  const started = data.tasks.find((t: any) => t.redmineId === 1270);
  expect(started.title).toBe("Formularz reklamacji");
  expect(issueRef(started)).toEqual({
    system: "redmine",
    key: "#1270",
    url: `${REDMINE_SITE}/issues/1270`,
    status: "New",
    label: "Redmine",
  });
  // Jeden aktywny pomiar: na nowym zadaniu.
  expect(
    data.sessions.filter((s: any) => s.endedAt === null).map((s: any) => s.taskId),
  ).toEqual([started.id]);
  expect(issueRef(byId(ids.analysis))).toMatchObject({
    system: "jira",
    key: "WMS-12",
    label: "Jira",
  });
  expect(issueRef(byId(ids.review))).toBeNull();
});

test("nasłuch storage: rozłączenie i projekty z innego okna odświeżają widok Redmine", async ({
  page,
  context,
}) => {
  await openPreview(page);
  await page.getByRole("button", { name: "Redmine", exact: true }).click();
  await expect(page.getByRole("button", { name: "Rozpocznij: #1234" })).toBeVisible();

  // Drugie okno tej samej aplikacji (wspólny localStorage).
  const other = await context.newPage();
  await other.goto("/");
  await ok(other, "redmineSelectProjects", [
    { id: 9, name: "Portal klienta", identifier: "portal", parentId: null },
  ]);
  await expect(
    page.getByRole("group", { name: "Projekt Redmine" }).getByRole("button"),
  ).toHaveText(["Portal klienta"]);
  await expect(page.getByRole("button", { name: "Rozpocznij: #1270" })).toBeVisible();

  await ok(other, "redmineDisconnect");
  await expect(page.getByText("Połącz konto Redmine w Ustawieniach")).toBeVisible();
  await expect(page.getByRole("button", { name: "Rozpocznij: #1270" })).toHaveCount(0);
  await other.close();
});
