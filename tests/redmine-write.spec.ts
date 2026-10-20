import { test, expect, type Page } from "@playwright/test";
import {
  chooseRedmineStatus,
  previewRedmineStatuses,
  redmineTransitionOf,
} from "../src/store";
import {
  at,
  openPreview,
  REDMINE_KEY,
  REDMINE_SITE,
  seed,
  stored,
  TODAY,
} from "./fixtures/seed";

/**
 * Zapisy do Redmine (PLAN_1.9.md, R4) na symulowanym Redmine podglądu: pytanie
 * o przesunięcie przy starcie pracy (ten sam baner co dla Jiry), „Nie pytaj”,
 * propozycja po ukończeniu, „Przesuń w Redmine…” na kafelku, „Przesuń do…” w
 * widoku Redmine, „Przypisz do mnie”, komentarz, nazwa gałęzi i główny
 * przełącznik `redmineWrite`. Żaden test nie łączy się z Redmine.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const JIRA_KEY = "todo-track-preview-jira-v1";
const IMPORT = "Import CSV zamówień";

/** Seed z zadaniem `#1234` (w symulowanym Redmine „New”) na dziś i `#1240` („Feedback”). */
function redmineSeed() {
  const data = seed();
  const base = data.tasks[0].details;
  data.tasks.unshift(
    {
      id: 20,
      title: IMPORT,
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1234,
      redmineUrl: `${REDMINE_SITE}/issues/1234`,
      redmineStatus: "New",
      redmineTracker: "Funkcja",
      redmineDue: "2026-10-30",
      details: {
        ...base,
        priority: 0,
        project: "Sklep",
        plannedDay: TODAY,
        dueDay: "",
        estimateMinutes: 0,
        notes: {},
        checklist: [],
      },
    } as any,
    {
      id: 21,
      title: "Błąd zapisu etykiet",
      done: false,
      createdAt: at("2026-10-14", "09:00"),
      redmineId: 1240,
      redmineUrl: `${REDMINE_SITE}/issues/1240`,
      redmineStatus: "Feedback",
      redmineTracker: "Błąd",
      redmineDue: "",
      details: {
        ...base,
        priority: 0,
        project: "",
        plannedDay: TODAY,
        dueDay: "",
        estimateMinutes: 0,
        notes: {},
        checklist: [],
      },
    } as any,
  );
  return data;
}

const banner = (page: Page) =>
  page.getByRole("region", { name: "Przesunięcie zgłoszenia w Redmine" });
const jiraBanner = (page: Page) =>
  page.getByRole("region", { name: "Przesunięcie zgłoszenia w Jirze" });
const doneBanner = (page: Page) =>
  page.getByRole("region", { name: "Przesunięcie zgłoszenia po ukończeniu" });
const card = (page: Page, title: string) =>
  page.locator(".task-card", {
    has: page.getByRole("button", {
      name: "Przedziały: " + title,
      exact: true,
    }),
  });
const expand = (page: Page, title: string) =>
  page.getByLabel("Przedziały: " + title, { exact: true }).click();
const taskStatus = async (page: Page, id = 20) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === id)
    .redmineStatus;
/** Stan symulowanego Redmine (zgłoszenia po zapisie, komentarze). */
const redmineState = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}"),
    REDMINE_KEY,
  );
const simStatus = async (page: Page, id: number) =>
  ((await redmineState(page)).issues ?? []).find(
    (i: { id: number }) => i.id === id,
  )?.status;
const seenTask = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").seen?.[1] ?? null,
    JIRA_KEY,
  );
async function setRedmine(page: Page, state: Record<string, unknown>) {
  await page.evaluate(([key, value]) => localStorage.setItem(key, value), [
    REDMINE_KEY,
    JSON.stringify(state),
  ] as const);
  await page.reload();
}
async function start(page: Page, title: string) {
  await page
    .getByRole("button", { name: "Start: " + title, exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Stop: " + title, exact: true }),
  ).toBeVisible();
}
async function stop(page: Page, title: string) {
  await page
    .getByRole("button", { name: "Stop: " + title, exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Start: " + title, exact: true }),
  ).toBeVisible();
}
const openRedmine = (page: Page) =>
  page.getByRole("button", { name: "Redmine", exact: true }).click();
const statusGroup = (page: Page, status: string) =>
  page.getByRole("region", { name: `Status ${status}`, exact: true });
const issueCard = (page: Page, id: number, subject: string) =>
  page.getByRole("article", { name: `#${id} ${subject}`, exact: true });

test("cel pytania jak w Rust: dopasowanie po nazwie, lista bez zamkniętych i bieżącego", () => {
  const all = [...previewRedmineStatuses];
  const one = chooseRedmineStatus(all, 1, " in progress ");
  expect(one.statusId).toBe(2);
  expect(one.statuses.map((s) => s.name)).toEqual(["In Progress", "Feedback"]);
  const none = chooseRedmineStatus(all, 1, "W toku");
  expect(none.statusId).toBeNull();
  expect(none.statuses).toHaveLength(2);
  expect(chooseRedmineStatus(all, 1, "Closed").statusId).toBeNull();
  expect(chooseRedmineStatus(all.slice(3), 1, "Closed").statuses).toEqual([]);
  expect(redmineTransitionOf(all[3])).toEqual({
    id: "4",
    name: "Resolved",
    toStatus: "Resolved",
    toCategory: "done",
  });
});

test("start → baner → „Przesuń” zmienia status #1234 na kafelku; pomiar trwa od razu", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await expect(banner(page)).toHaveCount(0);
  await start(page, IMPORT);
  await expect(banner(page)).toBeVisible();
  await expect(jiraBanner(page)).toHaveCount(0);
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Przesunąć #1234 do „In Progress”?",
  );
  await expect(banner(page)).toContainText("ma w Redmine status New");
  // Nic nie zmieniło się samo, a pomiar już trwa.
  expect(await taskStatus(page)).toBe("New");
  expect(await simStatus(page, 1234)).toBeUndefined();
  const tile = card(page, IMPORT);
  await expect(tile.locator(".task-issue-status")).toHaveText("New");
  await banner(page)
    .getByRole("button", { name: "Przesuń #1234 do In Progress" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(tile.locator(".task-issue-status")).toHaveText("In Progress");
  expect(await taskStatus(page)).toBe("In Progress");
  expect(await simStatus(page, 1234)).toBe("In Progress");
  await expect(
    page.getByRole("button", { name: "Stop: " + IMPORT, exact: true }),
  ).toBeVisible();
  // Zgłoszenie w toku: kolejny start nie pyta.
  await stop(page, IMPORT);
  await start(page, IMPORT);
  await expect.poll(() => seenTask(page)).toBe(20);
  await expect(banner(page)).toHaveCount(0);
  // Status spoza „Statusów początkowych” (Feedback) też nie pyta.
  await start(page, "Błąd zapisu etykiet");
  await expect.poll(() => seenTask(page)).toBe(21);
  await expect(banner(page)).toHaveCount(0);
});

test("bez dopasowania nazwy baner pokazuje dozwolone statusy niezamknięte", async ({
  page,
}) => {
  await openPreview(page, {
    data: redmineSeed(),
    settings: { redmineMoveStatus: "W toku", jiraMoveStatus: "In Progress" },
  });
  await start(page, IMPORT);
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Przesunąć #1234 do „W toku”?",
  );
  await expect(banner(page)).toContainText("Wybierz status:");
  await expect(
    banner(page).getByRole("button", { name: /^Przesuń do: / }),
  ).toHaveText(["In Progress", "Feedback"]);
  await banner(page)
    .getByRole("button", { name: "Przesuń do: Feedback" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(card(page, IMPORT).locator(".task-issue-status")).toHaveText(
    "Feedback",
  );
});

test("„Nie pytaj dla tego zadania” wycisza #1234, kafelek odwraca", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await start(page, IMPORT);
  await banner(page).getByRole("button", { name: "Nie teraz" }).click();
  await expect(banner(page)).toHaveCount(0);
  await stop(page, IMPORT);
  await start(page, IMPORT);
  await expect(banner(page)).toBeVisible();
  await banner(page)
    .getByRole("button", { name: "Nie pytaj dla tego zadania" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const details = (await stored(page)).tasks.find(
    (t: { id: number }) => t.id === 20,
  ).details;
  // To samo pole co dla Jiry (zadanie ma jedno zgłoszenie).
  expect(details.jiraNoAsk).toBe(true);
  expect(details.project).toBe("Sklep");
  expect(await taskStatus(page)).toBe("New");
  await stop(page, IMPORT);
  await start(page, IMPORT);
  await expect.poll(() => seenTask(page)).toBe(20);
  await expect(banner(page)).toHaveCount(0);
  await expand(page, IMPORT);
  await expect(card(page, IMPORT).locator(".jira-no-ask")).toContainText(
    "Bez pytania o przesunięcie #1234 przy starcie pracy.",
  );
  await page
    .getByRole("button", { name: "Pytaj o przesunięcie: " + IMPORT })
    .click();
  await expect(
    page.getByRole("button", { name: "Pytaj o przesunięcie: " + IMPORT }),
  ).toHaveCount(0);
  await stop(page, IMPORT);
  await start(page, IMPORT);
  await expect(banner(page)).toBeVisible();
});

test("błąd Redmine zostawia pytanie z komunikatem i linkiem do #1234", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await setRedmine(page, { fail: 422 });
  await start(page, IMPORT);
  await banner(page)
    .getByRole("button", { name: "Przesuń #1234 do In Progress" })
    .click();
  await expect(banner(page).getByRole("alert")).toContainText(
    "Redmine odrzucił dane: Status is invalid",
  );
  await banner(page)
    .getByRole("button", { name: "Otwórz #1234 w Redmine" })
    .click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        JSON.parse(
          localStorage.getItem("todo-track-preview-redmine-opened-v1") ?? "[]",
        ),
      ),
    )
    .toEqual([`${REDMINE_SITE}/issues/1234`]);
  expect(await taskStatus(page)).toBe("New");
  // Redmine przyjął zapis bez zmiany statusu.
  await setRedmine(page, { unchanged: true });
  await expand(page, IMPORT);
  await page.getByRole("button", { name: "Przesuń w Redmine: #1234" }).click();
  await page
    .getByRole("group", { name: "Statusy w Redmine: #1234" })
    .getByRole("button", { name: "Przesuń #1234 do: In Progress" })
    .click();
  const tile = card(page, IMPORT);
  await expect(tile.getByRole("alert")).toContainText(
    "Redmine nie zmienił statusu #1234 (nadal „New”)",
  );
  await expect(
    tile.getByRole("button", { name: "Otwórz #1234 w Redmine" }),
  ).toBeVisible();
  expect(await taskStatus(page)).toBe("New");
});

test("„Przesuń w Redmine…” w kafelku i propozycja po ukończeniu", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await expand(page, IMPORT);
  await page.getByRole("button", { name: "Przesuń w Redmine: #1234" }).click();
  const list = page.getByRole("group", { name: "Statusy w Redmine: #1234" });
  // Dozwolone statusy bez bieżącego, także zamknięte.
  await expect(
    list.getByRole("button", { name: /^Przesuń #1234 do: / }),
  ).toHaveText(["In Progress", "Feedback", "Resolved", "Closed"]);
  await list
    .getByRole("button", { name: "Przesuń #1234 do: Feedback" })
    .click();
  await expect(list).toHaveCount(0);
  expect(await taskStatus(page)).toBe("Feedback");
  await expect(card(page, IMPORT).locator(".task-issue-status")).toHaveText(
    "Feedback",
  );
  // Ukończenie: „Ukończone. Przesunąć #1234 dalej?” z listą statusów.
  await page
    .getByRole("button", { name: "Ukończ: " + IMPORT, exact: true })
    .click();
  await expect(doneBanner(page).getByRole("heading")).toHaveText(
    "Ukończone. Przesunąć #1234 dalej?",
  );
  await expect(
    doneBanner(page).getByRole("button", { name: /^Przesuń do: / }),
  ).toHaveText(["New", "In Progress", "Resolved", "Closed"]);
  expect(await taskStatus(page)).toBe("Feedback");
  await doneBanner(page)
    .getByRole("button", { name: "Przesuń do: Resolved" })
    .click();
  await expect(doneBanner(page)).toHaveCount(0);
  expect(await taskStatus(page)).toBe("Resolved");
  expect(await simStatus(page, 1234)).toBe("Resolved");
});

test("„Przesuń do…” w widoku Redmine przenosi kartę do grupy, błąd ją cofa", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await openRedmine(page);
  const issue = issueCard(statusGroup(page, "New"), 1234, IMPORT);
  await expect(issue).toBeVisible();
  await issue.getByRole("button", { name: "Akcje: #1234" }).click();
  await issue.getByRole("button", { name: "Przesuń do: #1234" }).click();
  await expect(
    issue.getByRole("button", { name: /^Przesuń #1234 do: / }),
  ).toHaveText(["In Progress", "Feedback", "Resolved", "Closed"]);
  await issue
    .getByRole("button", { name: "Przesuń #1234 do: In Progress" })
    .click();
  const moved = issueCard(statusGroup(page, "In Progress"), 1234, IMPORT);
  await expect(moved).toBeVisible();
  await expect(moved.locator(".redmine-status")).toHaveText("In Progress");
  await expect(issueCard(statusGroup(page, "New"), 1234, IMPORT)).toHaveCount(
    0,
  );
  expect(await simStatus(page, 1234)).toBe("In Progress");
  // Kafelek powiązanego zadania dostaje nowy status.
  await expect.poll(() => taskStatus(page)).toBe("In Progress");
  // Odrzucenie przez Redmine: karta wraca do swojej grupy z komunikatem.
  await setRedmine(page, { ...(await redmineState(page)), fail: 403 });
  await openRedmine(page);
  const back = issueCard(statusGroup(page, "In Progress"), 1234, IMPORT);
  await back.getByRole("button", { name: "Akcje: #1234" }).click();
  await back.getByRole("button", { name: "Przesuń do: #1234" }).click();
  await back
    .getByRole("button", { name: "Przesuń #1234 do: Feedback" })
    .click();
  await expect(page.locator(".redmine-view").getByRole("alert")).toContainText(
    "Nie udało się przesunąć #1234: Brak uprawnień w Redmine do tej operacji.",
  );
  await expect(
    issueCard(statusGroup(page, "In Progress"), 1234, IMPORT),
  ).toBeVisible();
  await expect(
    statusGroup(page, "Feedback").locator(".redmine-number"),
  ).not.toContainText(["#1234"]);
  expect(await simStatus(page, 1234)).toBe("In Progress");
});

test("„Przypisz do mnie” w widoku i na kafelku, komentarz i nazwa gałęzi", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await openRedmine(page);
  await page
    .getByRole("group", { name: "Zakres zgłoszeń" })
    .getByRole("button", { name: "Wszystkie otwarte" })
    .click();
  const other = issueCard(page, 1251, "Raport stanów magazynowych");
  await expect(other.getByLabel("Przypisane: Anna Nowak")).toBeVisible();
  await other.getByRole("button", { name: "Akcje: #1251" }).click();
  await other.getByRole("button", { name: "Przypisz do mnie: #1251" }).click();
  await expect(other.getByRole("status")).toHaveText(
    "Przypisano #1251 do Ciebie.",
  );
  await expect(other.getByLabel("Przypisane: Anna Nowak")).toHaveCount(0);
  const saved = ((await redmineState(page)).issues ?? []).find(
    (i: { id: number }) => i.id === 1251,
  );
  expect(saved.assignedToId).toBeUndefined();
  // Moje zgłoszenie: bez „Przypisz do mnie”.
  const mine = issueCard(page, 1234, IMPORT);
  await mine.getByRole("button", { name: "Akcje: #1234" }).click();
  await expect(
    mine.getByRole("button", { name: "Przypisz do mnie: #1234" }),
  ).toHaveCount(0);
  // Kafelek zadania: komentarz i nazwa gałęzi.
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expand(page, IMPORT);
  const actions = page.getByRole("group", { name: "Akcje Redmine: #1234" });
  await expect(actions).toBeVisible();
  await actions.getByRole("button", { name: "Dodaj komentarz: #1234" }).click();
  const tile = card(page, IMPORT);
  await tile
    .getByLabel("Komentarz do #1234")
    .fill("Zaczynam.\nPoprawka po przeglądzie — ąę");
  await tile.getByRole("button", { name: "Wyślij komentarz" }).click();
  await expect(
    tile.locator(".redmine-issue-actions").getByRole("status"),
  ).toHaveText("Dodano komentarz do #1234.");
  expect((await redmineState(page)).comments).toEqual({
    "1234": ["Zaczynam.\nPoprawka po przeglądzie — ąę"],
  });
  await actions
    .getByRole("button", { name: "Kopiuj nazwę gałęzi: #1234" })
    .click();
  await expect(
    tile.locator(".redmine-issue-actions").getByRole("status"),
  ).toContainText("feature/1234-import-csv-zamowien");
  // „Przypisz do mnie” na kafelku.
  await actions
    .getByRole("button", { name: "Przypisz do mnie: #1234" })
    .click();
  await expect(
    tile.locator(".redmine-issue-actions").getByRole("status"),
  ).toHaveText("Przypisano #1234 do Ciebie.");
});

test("wyłączone zapisy do Redmine: bez pytania, bez „Przesuń…”, przypisania i komentarza", async ({
  page,
}) => {
  await openPreview(page, {
    data: redmineSeed(),
    settings: { redmineWrite: false },
  });
  await start(page, IMPORT);
  await expect.poll(() => seenTask(page)).toBe(20);
  await expect(banner(page)).toHaveCount(0);
  await expand(page, IMPORT);
  const tile = card(page, IMPORT);
  await expect(
    tile.getByRole("button", { name: "Przesuń w Redmine: #1234" }),
  ).toHaveCount(0);
  await expect(
    tile.getByRole("button", { name: "Przypisz do mnie: #1234" }),
  ).toHaveCount(0);
  await expect(
    tile.getByRole("button", { name: "Dodaj komentarz: #1234" }),
  ).toHaveCount(0);
  await expect(tile).toContainText(
    "Zmiany w Redmine są wyłączone w Ustawieniach → Redmine.",
  );
  // Nazwa gałęzi nie jest zapisem.
  await tile
    .getByRole("button", { name: "Kopiuj nazwę gałęzi: #1234" })
    .click();
  await expect(
    tile.locator(".redmine-issue-actions").getByRole("status"),
  ).toContainText("feature/1234-import-csv-zamowien");
  // Ukończenie nie proponuje przesunięcia.
  await page
    .getByRole("button", { name: "Ukończ: " + IMPORT, exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await stored(page)).tasks.find((t: { id: number }) => t.id === 20)
          .done,
    )
    .toBe(true);
  await expect(doneBanner(page)).toHaveCount(0);
  // Widok Redmine: menu karty bez „Przesuń do…” i „Przypisz do mnie”.
  await openRedmine(page);
  await page
    .getByRole("group", { name: "Zakres zgłoszeń" })
    .getByRole("button", { name: "Wszystkie otwarte" })
    .click();
  const other = issueCard(page, 1251, "Raport stanów magazynowych");
  await other.getByRole("button", { name: "Akcje: #1251" }).click();
  await expect(
    other.getByRole("button", { name: "Przesuń do: #1251" }),
  ).toHaveCount(0);
  await expect(
    other.getByRole("button", { name: "Przypisz do mnie: #1251" }),
  ).toHaveCount(0);
  await expect(
    other.getByRole("button", { name: "Kopiuj nazwę gałęzi: #1251" }),
  ).toBeVisible();
  expect((await redmineState(page)).issues).toBeUndefined();
  // Jira bez zmian: przełącznik Redmine jej nie dotyczy.
  expect(
    (
      await page.evaluate(() =>
        JSON.parse(localStorage.getItem("todo-track-preview-settings-v1")!),
      )
    ).jiraWrite,
  ).toBeUndefined();
});

test("widget pokazuje pytanie Redmine i przesuwa jednym kliknięciem", async ({
  page,
}) => {
  await openPreview(page, { data: redmineSeed() });
  await start(page, IMPORT);
  await expect(banner(page)).toBeVisible();
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  const button = widget.getByRole("button", {
    name: "Przesuń #1234 do In Progress",
  });
  await expect(button).toHaveAttribute(
    "title",
    "Redmine: przesunąć #1234 do „In Progress”?",
  );
  await button.click();
  await expect(widget.getByRole("status")).toContainText(
    "Redmine: #1234 → In Progress",
  );
  expect(await taskStatus(page)).toBe("In Progress");
});
