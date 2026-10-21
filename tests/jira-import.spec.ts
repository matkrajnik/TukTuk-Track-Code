import { test, expect, type Page } from "./fixtures/test";
import { ids } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";
test.use({ timezoneId: "Europe/Warsaw" });

const column = (page: Page, name: string) =>
  page.getByRole("region", { name, exact: true });
const unplanned = (page: Page) =>
  column(page, "Do zaplanowania").locator(".week-card-title");
// „Code review” to też tytuł innego zadania, więc status szukamy w kafelku.
const jiraStatusOn = (page: Page, title: string) =>
  page
    .getByRole("article", { name: title, exact: true })
    .locator(".task-jira-status");

test("Jira watch: first check only remembers, next one adds new issues to Do zaplanowania without duplicating trashed ones", async ({
  page,
}) => {
  await openDesktop(page);
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page.getByRole("button", { name: "Tydzień", exact: true }).click();
  await expect(unplanned(page)).toHaveText(["Code review"]);
  await expect(jiraStatusOn(page, "Analiza importu")).toHaveText("W toku");

  // Pierwsze sprawdzenie dla konta: przypisania są tylko zapamiętywane.
  const first = await backend<string[]>(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "W toku" },
    { key: "WMS-20", summary: "Raport zwrotów", status: "Do zrobienia" },
  ]);
  expect(first).toEqual([]);
  await expect(unplanned(page)).toHaveText(["Code review"]);

  // Drugie: nowe przypisanie WMS-21 trafia do „Do zaplanowania”. WMS-20 było
  // przypisane już wcześniej, a WMS-7 ma zadanie w koszu, więc nie są dodawane.
  const second = await backend<string[]>(page, "jiraCheck", [
    { key: "WMS-12", summary: "Analiza importu", status: "Code review" },
    { key: "WMS-20", summary: "Raport zwrotów", status: "Do zrobienia" },
    { key: "WMS-21", summary: "Etykiety palet", status: "Do zrobienia" },
    { key: "WMS-7", summary: "Porzucony prototyp", status: "Do zrobienia" },
    {
      key: "WMS-30",
      summary: "Zamknięte zgłoszenie",
      status: "Gotowe",
      open: false,
    },
  ]);
  expect(second).toEqual(["WMS-21"]);
  await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
  await expect(
    column(page, "Do zaplanowania").getByText("WMS-21"),
  ).toBeVisible();
  // Status istniejącego zgłoszenia odświeża się na kafelku.
  await expect(jiraStatusOn(page, "Analiza importu")).toHaveText("Code review");

  // Kolejne sprawdzenie z tym samym zestawem niczego nie dubluje.
  expect(
    await backend<string[]>(page, "jiraCheck", [
      { key: "WMS-21", summary: "Etykiety palet", status: "W toku" },
      { key: "WMS-7", summary: "Porzucony prototyp", status: "Do zrobienia" },
    ]),
  ).toEqual([]);
  await expect(unplanned(page)).toHaveText(["Etykiety palet", "Code review"]);
  await expect(jiraStatusOn(page, "Etykiety palet")).toHaveText("W toku");

  // Zadania: nowe zgłoszenie z kluczem i statusem, zadanie z kosza nie wraca.
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await expect(
    page.getByLabel("Start: Etykiety palet", { exact: true }),
  ).toHaveCount(1);
  await expect(
    page.getByLabel("Start: Porzucony prototyp", { exact: true }),
  ).toHaveCount(0);
  const snapshot = await backend<any>(page, "snapshot");
  expect(
    snapshot.tasks.filter((t: any) => t.jiraKey === "WMS-21"),
  ).toMatchObject([
    {
      title: "Etykiety palet",
      jiraUrl: "https://asistonwms.atlassian.net/browse/WMS-21",
      jiraStatus: "W toku",
      done: false,
    },
  ]);
  expect(
    snapshot.tasks.find((t: any) => t.jiraKey === "WMS-12").jiraStatus,
  ).toBe("Code review");
  expect(snapshot.tasks.some((t: any) => t.jiraKey === "WMS-7")).toBe(false);
  expect(snapshot.trash.map((t: any) => t.id)).toEqual([ids.trashed]);

  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  await page.getByRole("button", { name: "Kosz", exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Przywróć: Porzucony prototyp" }),
  ).toHaveCount(1);
});
