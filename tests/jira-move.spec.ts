import { test, expect, type Page } from "@playwright/test";
import {
  chooseTransition,
  previewStatusCategory,
  previewWorkflow,
  type JiraTransition,
} from "../src/store";
import { ids, openPreview, seed, stored } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";

/**
 * J1: pytanie o przesunięcie zgłoszenia w Jirze przy starcie pracy, na symulowanej
 * Jirze podglądu (`store.ts`). Dane z `fixtures/seed.ts`: „Analiza importu” jest
 * powiązana z WMS-12; testy ustawiają jej status na „Do zrobienia”.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const JIRA_KEY = "todo-track-preview-jira-v1";
const card = (page: Page, title: string) =>
  page.locator(".task-card", {
    has: page.getByRole("button", { name: "Przedziały: " + title, exact: true }),
  });
const banner = (page: Page) =>
  page.getByRole("region", { name: "Przesunięcie zgłoszenia w Jirze" });
const doneBanner = (page: Page) =>
  page.getByRole("region", {
    name: "Przesunięcie zgłoszenia po ukończeniu",
  });
/** Seed z WMS-12 w statusie „Do zrobienia”. */
function todoSeed() {
  const data = seed();
  data.tasks.find((t) => t.id === ids.analysis)!.jiraStatus = "Do zrobienia";
  return data;
}
const analysisStatus = async (page: Page) =>
  (await stored(page)).tasks.find((t: { id: number }) => t.id === ids.analysis)
    .jiraStatus;
/** Ostatnio widziany pomiar symulowanej Jiry: zadanie (start obsłużony). */
const seenTask = (page: Page) =>
  page.evaluate(
    (k) => JSON.parse(localStorage.getItem(k) ?? "{}").seen?.[1] ?? null,
    JIRA_KEY,
  );
async function start(page: Page, title: string) {
  await page.getByRole("button", { name: "Start: " + title, exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Stop: " + title, exact: true }),
  ).toBeVisible();
}
async function stop(page: Page, title: string) {
  await page.getByRole("button", { name: "Stop: " + title, exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Start: " + title, exact: true }),
  ).toBeVisible();
}

test("wybór przejścia jak w Rust: jedno, preferowana nazwa, lista", () => {
  const t = (
    id: string,
    toStatus: string,
    toCategory: string,
    name = toStatus,
  ): JiraTransition => ({ id, name, toStatus, toCategory });
  const one = [t("11", "Do zrobienia", "new"), t("21", "W toku", "indeterminate")];
  expect(chooseTransition(one, "In Progress")).toEqual({
    transitions: [one[1]],
    transitionId: "21",
  });
  const many = [
    t("21", "In Progress", "indeterminate", "Start"),
    t("31", "Code review", "indeterminate"),
    t("41", "Gotowe", "done"),
  ];
  expect(chooseTransition(many, " in progress ").transitionId).toBe("21");
  expect(chooseTransition(many, "start").transitionId).toBe("21");
  const none = chooseTransition(many, "W realizacji");
  expect(none.transitionId).toBeNull();
  expect(none.transitions.map((x) => x.id)).toEqual(["21", "31"]);
  expect(chooseTransition([many[2]], "In Progress")).toEqual({
    transitions: [],
    transitionId: null,
  });
  expect(previewStatusCategory("Do zrobienia")).toBe("new");
  expect(previewStatusCategory("")).toBe("new");
  expect(previewStatusCategory("W toku")).toBe("indeterminate");
  expect(previewStatusCategory("Gotowe")).toBe("done");
  expect(previewWorkflow.filter((x) => x.toCategory === "indeterminate")).toHaveLength(2);
});

test("start → baner → „Przesuń” zmienia status na kafelku; pomiar trwa od razu", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await expect(banner(page)).toHaveCount(0);
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Przesunąć WMS-12 do „In Progress”?",
  );
  // Nic nie przesunęło się samo, a pomiar już trwa.
  expect(await analysisStatus(page)).toBe("Do zrobienia");
  const tile = card(page, "Analiza importu");
  await expect(tile.locator(".task-jira-status")).toHaveText("Do zrobienia");
  await banner(page)
    .getByRole("button", { name: "Przesuń WMS-12 do In Progress" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(tile.locator(".task-jira-status")).toHaveText("In Progress");
  expect(await analysisStatus(page)).toBe("In Progress");
  await expect(
    page.getByRole("button", { name: "Stop: Analiza importu", exact: true }),
  ).toBeVisible();
  // Zgłoszenie w toku: kolejny start nie pyta.
  await stop(page, "Analiza importu");
  await start(page, "Analiza importu");
  await expect.poll(() => seenTask(page)).toBe(ids.analysis);
  await expect(banner(page)).toHaveCount(0);
});

test("„Nie teraz” chowa pytanie, „Nie pytaj dla tego zadania” wycisza, kafelek odwraca", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await start(page, "Analiza importu");
  await banner(page).getByRole("button", { name: "Nie teraz" }).click();
  await expect(banner(page)).toHaveCount(0);
  expect(await analysisStatus(page)).toBe("Do zrobienia");
  // Kolejny start pyta znowu.
  await stop(page, "Analiza importu");
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
  await banner(page)
    .getByRole("button", { name: "Nie pytaj dla tego zadania" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  const details = (await stored(page)).tasks.find(
    (t: { id: number }) => t.id === ids.analysis,
  ).details;
  expect(details.jiraNoAsk).toBe(true);
  // Inne pola szczegółów zostają.
  expect(details.project).toBe("WMS");
  await stop(page, "Analiza importu");
  await start(page, "Analiza importu");
  await expect.poll(() => seenTask(page)).toBe(ids.analysis);
  await expect(banner(page)).toHaveCount(0);
  // Odwrócenie w rozwiniętym kafelku.
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await page
    .getByRole("button", { name: "Pytaj o przesunięcie: Analiza importu" })
    .click();
  await expect(
    page.getByRole("button", { name: "Pytaj o przesunięcie: Analiza importu" }),
  ).toHaveCount(0);
  await stop(page, "Analiza importu");
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
});

test("bez dopasowania nazwy baner pokazuje listę przejść", async ({ page }) => {
  await openPreview(page, {
    data: todoSeed(),
    settings: { jiraMoveStatus: "W realizacji" },
  });
  await start(page, "Analiza importu");
  await expect(banner(page).getByRole("heading")).toHaveText(
    "Przesunąć WMS-12 do „W realizacji”?",
  );
  await expect(
    banner(page).getByRole("button", { name: /^Przesuń do: / }),
  ).toHaveText(["Rozpocznij → In Progress", "Do code review → Code review"]);
  await banner(page)
    .getByRole("button", { name: "Przesuń do: Code review" })
    .click();
  await expect(banner(page)).toHaveCount(0);
  await expect(
    card(page, "Analiza importu").locator(".task-jira-status"),
  ).toHaveText("Code review");
});

test("błąd Jiry zostawia pytanie z komunikatem i linkiem do zgłoszenia", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ ready: true, fail: 403 })),
    JIRA_KEY,
  );
  await start(page, "Analiza importu");
  await banner(page)
    .getByRole("button", { name: "Przesuń WMS-12 do In Progress" })
    .click();
  await expect(banner(page).getByRole("alert")).toContainText(
    "Brak uprawnień do zmiany tego zgłoszenia w Jirze.",
  );
  await expect(
    banner(page).getByRole("button", { name: "Otwórz WMS-12 w Jirze" }),
  ).toBeVisible();
  expect(await analysisStatus(page)).toBe("Do zrobienia");
});

test("przesunięcie przy ukończeniu i „Przesuń w Jirze…” w kafelku", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  // Rozwinięty kafelek: lista przejść bez domyślnego wyboru.
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await page
    .getByRole("button", { name: "Przesuń w Jirze: WMS-12" })
    .click();
  const list = page.getByRole("group", { name: "Przejścia w Jirze: WMS-12" });
  await list.getByRole("button", { name: "Przesuń WMS-12 do: Code review" }).click();
  await expect(list).toHaveCount(0);
  expect(await analysisStatus(page)).toBe("Code review");
  // Ukończenie: „Ukończone. Przesunąć WMS-12 dalej?” z listą przejść.
  await page
    .getByRole("button", { name: "Ukończ: Analiza importu", exact: true })
    .click();
  await expect(doneBanner(page).getByRole("heading")).toHaveText(
    "Ukończone. Przesunąć WMS-12 dalej?",
  );
  await expect(
    doneBanner(page).getByRole("button", { name: /^Przesuń do: / }),
  ).toHaveCount(3);
  expect(await analysisStatus(page)).toBe("Code review");
  await doneBanner(page)
    .getByRole("button", { name: "Przesuń do: Gotowe" })
    .click();
  await expect(doneBanner(page)).toHaveCount(0);
  expect(await analysisStatus(page)).toBe("Gotowe");
});

test("wyłączone zapisy do Jiry: bez pytania i bez „Przesuń w Jirze…”", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await openSettings(page, "Jira");
  const write = page.getByLabel("Pozwól aplikacji zmieniać zgłoszenia w Jirze");
  await expect(write).toBeChecked();
  await expect(
    page.getByLabel("Pytaj o przesunięcie zgłoszenia przy starcie pracy"),
  ).toBeChecked();
  await expect(
    page.getByLabel("Docelowy status przy starcie pracy"),
  ).toHaveValue("In Progress");
  await write.uncheck();
  await expect(
    page.getByLabel("Pytaj o przesunięcie zgłoszenia przy starcie pracy"),
  ).toBeDisabled();
  await page.getByRole("button", { name: "Zadania", exact: true }).click();
  await start(page, "Analiza importu");
  await expect.poll(() => seenTask(page)).toBe(ids.analysis);
  await expect(banner(page)).toHaveCount(0);
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await expect(
    page.getByRole("button", { name: "Przesuń w Jirze: WMS-12" }),
  ).toHaveCount(0);
  // Ukończenie też nie proponuje przesunięcia.
  await page
    .getByRole("button", { name: "Ukończ: Analiza importu", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        (await stored(page)).tasks.find(
          (t: { id: number }) => t.id === ids.analysis,
        ).done,
    )
    .toBe(true);
  await expect(doneBanner(page)).toHaveCount(0);
  expect(await analysisStatus(page)).toBe("Do zrobienia");
});

test("przełączenie na zadanie bez klucza albo stop chowa pytanie poprzedniego zadania", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
  // Inne zadanie bez zgłoszenia: pytanie o WMS-12 jest już nieaktualne.
  await start(page, "Wdrożenie etykiet");
  await expect(banner(page)).toHaveCount(0);
  expect(await analysisStatus(page)).toBe("Do zrobienia");
  // Powrót do zadania ze zgłoszeniem pyta znowu, a stop chowa pytanie.
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
  await stop(page, "Analiza importu");
  await expect(banner(page)).toHaveCount(0);
});

test("błąd przesunięcia z kafelka pokazuje komunikat z linkiem do zgłoszenia", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ ready: true, fail: 400 })),
    JIRA_KEY,
  );
  await page.getByLabel("Przedziały: Analiza importu", { exact: true }).click();
  await page.getByRole("button", { name: "Przesuń w Jirze: WMS-12" }).click();
  await page
    .getByRole("group", { name: "Przejścia w Jirze: WMS-12" })
    .getByRole("button", { name: "Przesuń WMS-12 do: Code review" })
    .click();
  const tile = card(page, "Analiza importu");
  await expect(tile.getByRole("alert")).toContainText("Jira odrzuciła zmianę.");
  await expect(
    tile.getByRole("button", { name: "Otwórz WMS-12 w Jirze" }),
  ).toBeVisible();
  expect(await analysisStatus(page)).toBe("Do zrobienia");
});

test("błąd „Przesuń” z widgetu zostawia pytanie z komunikatem w banerze", async ({
  page,
}) => {
  await openPreview(page, { data: todoSeed() });
  await page.evaluate(
    (k) => localStorage.setItem(k, JSON.stringify({ ready: true, fail: 403 })),
    JIRA_KEY,
  );
  await start(page, "Analiza importu");
  await expect(banner(page)).toBeVisible();
  await page.goto("/#widget");
  await page.reload();
  const widget = page.locator("#widget");
  await widget
    .getByRole("button", { name: "Przesuń WMS-12 do In Progress" })
    .click();
  await expect(widget.getByRole("status")).toContainText("Brak uprawnień");
  await page.goto("/");
  await page.reload();
  await expect(banner(page).getByRole("alert")).toContainText(
    "Brak uprawnień do zmiany tego zgłoszenia w Jirze.",
  );
  await expect(
    banner(page).getByRole("button", { name: "Otwórz WMS-12 w Jirze" }),
  ).toBeVisible();
  expect(await analysisStatus(page)).toBe("Do zrobienia");
});
