import { test, expect, type Page } from "@playwright/test";
import { at, ids, REDMINE_SITE, seed, TODAY } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * Obserwacja przypisań w Redmine (PLAN_1.9.md, R3) przez zastępcze IPC
 * (`redmineCheck` = `redmine::sync_assigned` z `watch_once`): pierwsze
 * sprawdzenie tylko zapamiętuje, nowe otwarte przypisanie trafia do „Do
 * zaplanowania” bez dnia i bez pomiaru, zgłoszenie z zadaniem w koszu się nie
 * dubluje, zdjęte i przypisane ponownie wraca, status odświeża się na kafelku.
 * Obserwacja Jiry działa niezależnie.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const column = (page: Page, name: string) =>
  page.getByRole("region", { name, exact: true });
const unplanned = (page: Page) =>
  column(page, "Do zaplanowania").locator(".week-card-title");
const redmineStatusOn = (page: Page, title: string) =>
  page
    .getByRole("article", { name: title, exact: true })
    .locator(".task-redmine-status");

function redmineData() {
  const data = seed();
  const base = data.tasks[0].details;
  (data.tasks as any[]).unshift({
    id: 30,
    title: "Import CSV zamówień",
    done: false,
    createdAt: at("2026-10-14", "09:00"),
    redmineId: 1234,
    redmineUrl: `${REDMINE_SITE}/issues/1234`,
    redmineStatus: "New",
    redmineTracker: "Funkcja",
    redmineDue: "",
    details: { ...base, priority: 0, plannedDay: TODAY, dueDay: "", notes: {}, checklist: [], estimateMinutes: 0 },
  });
  (data.trash as any[]).push({
    id: 31,
    title: "Porzucony eksport",
    done: false,
    createdAt: at("2026-10-14", "09:00"),
    redmineId: 1299,
    redmineUrl: `${REDMINE_SITE}/issues/1299`,
    redmineStatus: "New",
    details: { ...base, priority: 0, plannedDay: "", dueDay: "", notes: {}, checklist: [], estimateMinutes: 0 },
  });
  return data;
}

test("Redmine watch: first check only remembers, then new assignments land in Do zaplanowania without duplicates", async ({
  page,
}) => {
  await openDesktop(page, { data: redmineData() });
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await expect(unplanned(page)).toHaveText(["Code review"]);
  await expect(redmineStatusOn(page, "Import CSV zamówień")).toHaveText("New");

  // Pierwsze sprawdzenie: backlog zapamiętany, status odświeżony.
  const first = await backend<number[]>(page, "redmineCheck", [
    { id: 1234, subject: "Import CSV zamówień", status: "In Progress" },
    { id: 1300, subject: "Raport zwrotów", status: "New" },
  ]);
  expect(first).toEqual([]);
  await expect(unplanned(page)).toHaveText(["Code review"]);
  await expect(redmineStatusOn(page, "Import CSV zamówień")).toHaveText(
    "In Progress",
  );

  // Drugie: nowe #1301 do „Do zaplanowania”; #1300 było już przypisane,
  // #1299 ma zadanie w koszu, #1302 jest zamknięte.
  const second = await backend<number[]>(page, "redmineCheck", [
    { id: 1234, subject: "Import CSV zamówień", status: "In Progress" },
    { id: 1300, subject: "Raport zwrotów", status: "New" },
    {
      id: 1301,
      subject: "Etykiety palet",
      status: "New",
      tracker: "Błąd",
      priority: "High",
    },
    { id: 1299, subject: "Porzucony eksport", status: "New" },
    { id: 1302, subject: "Zamknięte", status: "Closed", closed: true },
  ]);
  expect(second).toEqual([1301]);
  await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
  await expect(
    column(page, "Do zaplanowania")
      .locator(".week-card", { hasText: "Etykiety palet" })
      .locator(".task-redmine-key"),
  ).toHaveText("#1301");

  // Kolejne sprawdzenie z tym samym zestawem niczego nie dubluje.
  expect(
    await backend<number[]>(page, "redmineCheck", [
      { id: 1300, subject: "Raport zwrotów", status: "New" },
      { id: 1301, subject: "Etykiety palet", status: "Feedback", tracker: "Błąd" },
    ]),
  ).toEqual([]);
  await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
  await expect(redmineStatusOn(page, "Etykiety palet")).toHaveText("Feedback");

  // #1300 zdjęte z listy, potem przypisane ponownie: wraca jako nowe zadanie.
  expect(
    await backend<number[]>(page, "redmineCheck", [
      { id: 1301, subject: "Etykiety palet", status: "Feedback" },
    ]),
  ).toEqual([]);
  expect(
    await backend<number[]>(page, "redmineCheck", [
      { id: 1300, subject: "Raport zwrotów", status: "New" },
      { id: 1301, subject: "Etykiety palet", status: "Feedback" },
    ]),
  ).toEqual([1300]);

  const snapshot = await backend<any>(page, "snapshot");
  const labels = snapshot.tasks.find((t: any) => t.redmineId === 1301);
  expect(labels).toMatchObject({
    title: "Etykiety palet",
    redmineUrl: `${REDMINE_SITE}/issues/1301`,
    redmineStatus: "Feedback",
    done: false,
  });
  expect(labels.jiraKey).toBeUndefined();
  expect(labels.details.priority).toBe(1);
  expect(labels.details.plannedDay).toBe("");
  // Bez pomiaru: nowe zadania nie startują timera.
  expect(
    snapshot.sessions.filter((s: any) => s.endedAt === null),
  ).toEqual([]);
  expect(snapshot.tasks.some((t: any) => t.redmineId === 1299)).toBe(false);
  expect(snapshot.trash.map((t: any) => t.id)).toEqual([ids.trashed, 31]);
});

test("Redmine auto-import off: nothing is added and Jira watch is independent", async ({
  page,
}) => {
  await openDesktop(page, {
    data: redmineData(),
    settings: { redmineAutoImport: false },
  });
  await backend(page, "redmineCheck", []);
  expect(
    await backend<number[]>(page, "redmineCheck", [
      { id: 1400, subject: "Nowe", status: "New" },
    ]),
  ).toEqual([]);
  // Jira importuje dalej (własny wyłącznik `jiraAutoImport`).
  await backend(page, "jiraCheck", []);
  expect(
    await backend<string[]>(page, "jiraCheck", [
      { key: "WMS-50", summary: "Nowe z Jiry", status: "Do zrobienia" },
    ]),
  ).toEqual(["WMS-50"]);
  const snapshot = await backend<any>(page, "snapshot");
  expect(snapshot.tasks.some((t: any) => t.redmineId === 1400)).toBe(false);
  expect(snapshot.tasks.some((t: any) => t.jiraKey === "WMS-50")).toBe(true);
});
