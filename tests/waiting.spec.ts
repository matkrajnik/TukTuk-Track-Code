import { test, expect, type Page } from "@playwright/test";
import { waitingAge, waitingProposal } from "../src/recurrence";
import { at, ids, openPreview, seed, stored } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";

/**
 * P3: „Czekam na” jako lista. Dane z `fixtures/seed.ts` (dziś czwartek
 * 15.10.2026 12:00): „Analiza importu” jest powiązana z WMS-12 (W toku),
 * „Code review” nie była jeszcze mierzona.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const headings = (page: Page) => page.locator(".task-group-heading");
const group = (page: Page, name: string) =>
  page.getByRole("region", { name, exact: true });
const card = (page: Page, title: string) =>
  page.locator(".task-card", {
    has: page.getByRole("button", { name: "Przedziały: " + title, exact: true }),
  });

test("waiting: age text and proposals are pure functions", () => {
  const now = Date.parse("2026-10-15T12:00:00+02:00");
  expect(waitingAge(0, now)).toBe("");
  expect(waitingAge(Date.parse("2026-10-15T08:00:00+02:00"), now)).toBe(
    "od dziś",
  );
  expect(waitingAge(Date.parse("2026-10-14T23:00:00+02:00"), now)).toBe(
    "od 1 dnia",
  );
  expect(waitingAge(Date.parse("2026-10-12T10:00:00+02:00"), now)).toBe(
    "od 3 dni",
  );
  const statuses = ["Code review", "Blocked"];
  const jira = { done: false, jiraKey: "WMS-1" };
  expect(
    waitingProposal({ ...jira, jiraStatus: "code REVIEW" }, statuses),
  ).toEqual({ action: "block", status: "code REVIEW" });
  expect(waitingProposal({ ...jira, jiraStatus: "W toku" }, statuses)).toBeNull();
  // Odrzucona dla tego statusu, ukończone, bez Jiry: nic.
  expect(
    waitingProposal(
      {
        ...jira,
        jiraStatus: "Blocked",
        details: { waitingDismissed: "Blocked" },
      },
      statuses,
    ),
  ).toBeNull();
  expect(
    waitingProposal({ ...jira, done: true, jiraStatus: "Blocked" }, statuses),
  ).toBeNull();
  expect(
    waitingProposal({ done: false, jiraStatus: "Blocked" }, statuses),
  ).toBeNull();
  // Blokada z czasu statusu czekania, status już inny: odblokować?
  expect(
    waitingProposal(
      {
        ...jira,
        jiraStatus: "W toku",
        details: { waiting: "Jira: Blocked", waitingStatus: "Blocked" },
      },
      statuses,
    ),
  ).toEqual({ action: "unblock", status: "W toku" });
  // Ręczna blokada przy zwykłym statusie: bez propozycji.
  expect(
    waitingProposal(
      {
        ...jira,
        jiraStatus: "Gotowe",
        details: { waiting: "klient", waitingStatus: "W toku" },
      },
      statuses,
    ),
  ).toBeNull();
});

test("waiting group sits under started tasks with age and Odblokuj", async ({
  page,
}) => {
  const data = seed();
  const review = data.tasks.find((t) => t.id === ids.review)!;
  Object.assign(review.details, {
    waiting: "odpowiedź klienta",
    waitingSince: at("2026-10-12", "10:00"),
  });
  data.tasks.push({
    id: 20,
    title: "Raport zwrotów",
    done: false,
    createdAt: at("2026-10-09", "09:00"),
    details: { ...review.details, waiting: "", waitingSince: 0 },
  } as (typeof data.tasks)[number]);
  await openPreview(page, { data });
  await expect(headings(page)).toHaveText([
    "Rozpoczęte 3",
    "Czekam na… 1",
    "Nierozpoczęte 1",
  ]);
  await expect(card(page, "Code review")).toContainText(
    "Czekam na: odpowiedź klienta · od 3 dni",
  );
  // Blokada z formularza zapamiętuje początek: „od dziś”.
  await page
    .getByRole("button", { name: "Przedziały: Raport zwrotów", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Czekam na…", exact: true })
    .fill("dostawca");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(group(page, "Czekam na…")).toContainText(
    "Czekam na: dostawca · od dziś",
  );
  await expect(headings(page)).toHaveText(["Rozpoczęte 3", "Czekam na… 2"]);
  // „Odblokuj” zdejmuje blokadę bez startu pomiaru.
  await page.getByRole("button", { name: "Odblokuj: Code review" }).click();
  await expect(headings(page)).toHaveText([
    "Rozpoczęte 3",
    "Czekam na… 1",
    "Nierozpoczęte 1",
  ]);
  await expect(group(page, "Nierozpoczęte")).toContainText("Code review");
  const saved = await stored(page);
  const after = saved.tasks.find((t: any) => t.id === ids.review);
  expect(after.details.waiting).toBe("");
  expect(after.details.waitingSince).toBe(0);
  expect(saved.sessions.some((s: any) => s.endedAt === null)).toBe(false);
  const fresh = saved.tasks.find((t: any) => t.id === 20);
  expect(fresh.details.waitingSince).toBe(Date.parse("2026-10-15T12:00:00+02:00"));
});

test("an edited plan form does not undo Nie teraz on save", async ({ page }) => {
  const data = seed();
  data.tasks.find((t) => t.id === ids.analysis)!.jiraStatus = "Code review";
  await openPreview(page, { data });
  const proposal = page.getByRole("group", {
    name: "Propozycja z Jiry: Analiza importu",
  });
  await expect(proposal).toContainText("Jira: Code review");
  // Rozwinięty kafelek z niezapisaną zmianą w formularzu planu.
  await page
    .getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
    .click();
  await page.getByLabel("Projekt / obszar").fill("Sklep");
  await proposal.getByRole("button", { name: "Nie teraz", exact: true }).click();
  await expect(proposal).toHaveCount(0);
  await expect(page.getByLabel("Projekt / obszar")).toHaveValue("Sklep");
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByText("Zapisano plan i notatki.")).toBeVisible();
  // Zapis szkicu nie przywraca propozycji odrzuconej z kafelka.
  await expect(proposal).toHaveCount(0);
  const snap = await stored(page);
  const details = snap.tasks.find((t: any) => t.id === ids.analysis).details;
  expect(details.waitingDismissed).toBe("Code review");
  expect(details.project).toBe("Sklep");
});

test("a stale remind is ignored and a new block starts without it", async ({
  page,
}) => {
  await openPreview(page);
  const remind = (id: number) =>
    page.evaluate(async (taskId) => {
      const store = await import("/src/store.ts");
      await store.setWaiting(taskId, { action: "remind", text: "2026-10-16" });
    }, id);
  await remind(ids.review);
  let saved = await stored(page);
  expect(
    saved.tasks.find((t: any) => t.id === ids.review).details.waitingRemindDay ??
      "",
  ).toBe("");
  // Nowa blokada ze szkicu z dawnym dniem przypomnienia: dzień znika.
  await page.evaluate(async (taskId) => {
    const store = await import("/src/store.ts");
    const snap = await store.read();
    const task = snap.tasks.find((t) => t.id === taskId)!;
    await store.saveDetails(taskId, {
      ...store.detailsOf(task),
      waiting: "klient",
      waitingRemindDay: "2026-10-16",
    });
  }, ids.review);
  saved = await stored(page);
  expect(
    saved.tasks.find((t: any) => t.id === ids.review).details.waitingRemindDay,
  ).toBe("");
  // Trwająca blokada przyjmuje przypomnienie.
  await remind(ids.review);
  saved = await stored(page);
  expect(
    saved.tasks.find((t: any) => t.id === ids.review).details.waitingRemindDay,
  ).toBe("2026-10-16");
});

test("Jira status proposes waiting and unblocking, nothing changes by itself", async ({
  page,
}) => {
  await openDesktop(page);
  const analysis = card(page, "Analiza importu");
  const proposal = page.getByRole("group", {
    name: "Propozycja z Jiry: Analiza importu",
  });
  await expect(analysis.locator(".task-jira-status")).toHaveText("W toku");
  await expect(proposal).toHaveCount(0);

  // Zgłoszenie wchodzi w „Code review”: tylko propozycja, zadanie bez zmian.
  await backend(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "Code review" },
  ]);
  await expect(proposal).toContainText(
    "Jira: Code review — oznaczyć jako czekam na?",
  );
  await expect(group(page, "Czekam na…")).toHaveCount(0);
  let snap = await backend<any>(page, "snapshot");
  expect(snap.tasks.find((t: any) => t.id === ids.analysis).details.waiting).toBe(
    "",
  );

  await proposal
    .getByRole("button", { name: "Oznacz jako czekam na", exact: true })
    .click();
  await expect(group(page, "Czekam na…")).toContainText(
    "Czekam na: Jira: Code review · od dziś",
  );
  await expect(proposal).toHaveCount(0);

  // Zgłoszenie wychodzi z czekania: propozycja odblokowania.
  await backend(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "In Progress" },
  ]);
  await expect(proposal).toContainText("Jira: In Progress — odblokować?");
  await expect(group(page, "Czekam na…")).toContainText("Analiza importu");
  await proposal.getByRole("button", { name: "Odblokuj", exact: true }).click();
  await expect(group(page, "Czekam na…")).toHaveCount(0);
  await expect(group(page, "Rozpoczęte")).toContainText("Analiza importu");
  await expect(proposal).toHaveCount(0);

  // „Nie teraz” chowa propozycję dla tego statusu.
  await backend(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "Blocked" },
  ]);
  await expect(proposal).toContainText("Jira: Blocked");
  await proposal.getByRole("button", { name: "Nie teraz", exact: true }).click();
  await expect(proposal).toHaveCount(0);
  snap = await backend<any>(page, "snapshot");
  const details = snap.tasks.find((t: any) => t.id === ids.analysis).details;
  expect(details.waiting).toBe("");
  expect(details.waitingDismissed).toBe("Blocked");

  // Własna lista statusów w Ustawieniach → Jira.
  await openSettings(page, "Jira");
  await page
    .getByLabel("Statusy oznaczające „Czekam na” (jeden w wierszu)")
    .fill("QA\nCzeka na klienta");
  await page.getByRole("heading", { name: "Jira", exact: true }).click();
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await backend(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "Code review" },
  ]);
  await expect(analysis.locator(".task-jira-status")).toHaveText("Code review");
  await expect(proposal).toHaveCount(0);
  await backend(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "QA" },
  ]);
  await expect(proposal).toContainText("Jira: QA — oznaczyć jako czekam na?");
});

test("notification settings keep the waiting reminder threshold", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Powiadomienia");
  const days = page.getByLabel(
    "Przypomnij o „Czekam na” po (dni robocze, 0 wyłącza)",
  );
  await expect(days).toHaveValue("3");
  await days.fill("5");
  await days.press("Tab");
  await page.reload();
  await openSettings(page, "Powiadomienia");
  await expect(
    page.getByLabel("Przypomnij o „Czekam na” po (dni robocze, 0 wyłącza)"),
  ).toHaveValue("5");
  await days.fill("31");
  await days.press("Tab");
  await expect(page.getByRole("alert")).toContainText("od 0 do 30");
});
