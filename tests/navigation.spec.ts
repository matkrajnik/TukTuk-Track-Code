import { test, expect, type Page } from "./fixtures/test";
import { ids, openPreview } from "./fixtures/seed";

/**
 * Lewy panel nawigacji według Windows NavigationView: rozwinięty od 1008 px,
 * same ikony przy 641–1007 px, schowany pod przyciskiem menu poniżej 641 px.
 * Ręczne zwinięcie jest zapamiętane; Ustawienia stoją na dole panelu.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const nav = (page: Page) =>
  page.getByRole("navigation", { name: "Nawigacja główna" });
/** Szerokość panelu: 240 px rozwinięty, 56 px same ikony. */
const paneWidth = async (page: Page) =>
  (await page.locator(".nav-pane").boundingBox())!.width;

test("panel rozwinięty od 1008 px, same ikony poniżej; Ustawienia na dole", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1180, height: 700 });
  await openPreview(page);
  expect(await paneWidth(page)).toBe(240);
  await expect(
    nav(page).getByRole("button", { name: "Plan i historia" }),
  ).toContainText("Plan i historia");
  const settings = nav(page).getByRole("button", { name: "Ustawienia" });
  const box = await settings.boundingBox();
  // Ustawienia przy dolnej krawędzi okna, nie pod resztą listy.
  expect(box!.y + box!.height).toBeGreaterThan(640);

  await page.setViewportSize({ width: 760, height: 700 });
  // Etykieta zostaje w dostępnej nazwie (i w podpowiedzi), ale panel ma same ikony.
  await expect.poll(() => paneWidth(page)).toBe(56);
  await expect(
    nav(page).getByRole("button", { name: "Plan i historia" }),
  ).toHaveAttribute("title", "Plan i historia");
  await nav(page).getByRole("button", { name: "Plan i historia" }).click();
  await expect(
    nav(page).getByRole("button", { name: "Plan i historia" }),
  ).toHaveAttribute("aria-current", "page");
});

test("przycisk menu zwija panel i wybór przeżywa przeładowanie", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1180, height: 700 });
  await openPreview(page);
  await page.getByRole("button", { name: "Zwiń nawigację" }).click();
  await expect.poll(() => paneWidth(page)).toBe(56);
  await page.reload();
  await expect.poll(() => paneWidth(page)).toBe(56);
  await page.getByRole("button", { name: "Rozwiń nawigację" }).click();
  await expect.poll(() => paneWidth(page)).toBe(240);
});

test("poniżej 641 px panel otwiera się nad treścią, Esc i wybór go zamykają", async ({
  page,
}) => {
  await page.setViewportSize({ width: 480, height: 700 });
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await expect(nav(page)).toBeHidden();
  const menu = page.getByRole("button", { name: "Pokaż nawigację" });
  await menu.click();
  await expect(nav(page)).toBeVisible();
  await expect(nav(page).getByRole("button", { name: "Zadania" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(nav(page)).toBeHidden();
  await expect(menu).toBeFocused();

  await menu.click();
  await nav(page).getByRole("button", { name: "Dashboard" }).click();
  await expect(nav(page)).toBeHidden();
  await expect(
    page.getByRole("heading", { level: 1, name: "Dashboard" }),
  ).toBeVisible();
  // Mierzone zadanie zostaje w pasku także w wąskim oknie.
  await expect(
    page.getByRole("button", { name: "Zatrzymaj mierzone zadanie" }),
  ).toBeVisible();
});
