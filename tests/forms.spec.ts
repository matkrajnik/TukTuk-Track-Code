import { test, expect, type Page } from "./fixtures/test";
import { filterOptions } from "../src/searchSelect";
import { intervalText } from "../src/time";
import { at, ids, openPreview, seed, stored, TODAY } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import { backend, openDesktop } from "./fixtures/tauri";
import { pick } from "./fixtures/searchSelect";

/**
 * B1: pole wyboru z wyszukiwaniem (SearchSelect), „Bez cyklu”, format
 * przedziałów pracy, data w podpowiedzi Zamknięcia dnia i walidacja pól
 * liczbowych Ustawień w kliencie.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const nav = (page: Page, name: string) =>
  page.getByRole("button", { name, exact: true }).click();

async function openDayPlan(page: Page) {
  await nav(page, "Plan i historia");
  await nav(page, "Dzień");
  return page.getByRole("combobox", { name: "Zadanie do planu" });
}

test("filterOptions: słowa w dowolnej kolejności, bez wielkości liter i ogonków, także nazwa grupy", () => {
  const options = [
    { value: 1, label: "Wdrożenie etykiet" },
    { value: 2, label: "Code review", group: "Foundation" },
    { value: 3, label: "Łódź — spotkanie" },
  ];
  const labels = (q: string) => filterOptions(options, q).map((o) => o.label);
  expect(labels("")).toEqual(options.map((o) => o.label));
  expect(labels("WDROZ")).toEqual(["Wdrożenie etykiet"]);
  expect(labels("etykiet wdro")).toEqual(["Wdrożenie etykiet"]);
  expect(labels("lodz")).toEqual(["Łódź — spotkanie"]);
  expect(labels("foundation")).toEqual(["Code review"]);
  expect(labels("xyz")).toEqual([]);
});

test("SearchSelect: filtrowanie, strzałki, Enter i aria-activedescendant", async ({
  page,
}) => {
  await openPreview(page);
  const combo = await openDayPlan(page);
  const add = page.getByRole("button", { name: "Dodaj do planu", exact: true });
  await expect(combo).toHaveAttribute("aria-expanded", "false");
  await expect(combo).toHaveAttribute("placeholder", "Wybierz zadanie");
  await expect(add).toBeDisabled();
  // Zamknięte pole nie ma listy w stronie.
  await expect(page.getByRole("listbox")).toHaveCount(0);

  await combo.click();
  await expect(combo).toHaveAttribute("aria-expanded", "true");
  const list = page.getByRole("listbox");
  await expect(list.getByRole("option", { name: "Code review" })).toBeVisible();
  // Bez ogonków i wielkości liter.
  await combo.fill("WDROZ");
  await expect(list.getByRole("option")).toHaveText(["Wdrożenie etykiet"]);
  await combo.fill("e");
  const count = await list.getByRole("option").count();
  expect(count).toBeGreaterThan(1);
  // Pierwsza pasująca pozycja jest aktywna; strzałka w dół przechodzi dalej.
  const active = async () => {
    const id = await combo.getAttribute("aria-activedescendant");
    expect(id).toBeTruthy();
    return page.locator(`[id="${id}"]`);
  };
  await expect(await active()).toHaveText(
    (await list.getByRole("option").first().textContent())!.trim(),
  );
  await combo.press("ArrowDown");
  const second = (await list.getByRole("option").nth(1).textContent())!.trim();
  await expect(await active()).toHaveText(second);
  await expect(await active()).toHaveClass(/active/);
  await combo.press("ArrowUp");
  await combo.press("ArrowDown");
  await combo.press("Enter");
  await expect(combo).toHaveAttribute("aria-expanded", "false");
  await expect(combo).toHaveValue(second);
  await expect(combo).not.toHaveAttribute("aria-activedescendant");
  await expect(add).toBeEnabled();
  await add.click();
  await expect(page.getByRole("region", { name: "Plan dnia" })).toContainText(
    second,
  );
});

test("SearchSelect: Escape zamyka listę bez zmiany wyboru; brak wyników", async ({
  page,
}) => {
  await openPreview(page);
  const combo = await openDayPlan(page);
  await pick(combo, "Code review", "code");
  await expect(combo).toHaveValue("Code review");
  const value = await combo.getAttribute("data-value");
  expect(value).toBe(String(ids.review));

  await combo.press("ArrowDown");
  await expect(combo).toHaveAttribute("aria-expanded", "true");
  // Aktywna jest bieżąca pozycja.
  const id = await combo.getAttribute("aria-activedescendant");
  await expect(page.locator(`[id="${id}"]`)).toHaveText("Code review");
  await combo.fill("xyz");
  await expect(page.getByRole("listbox")).toContainText("Brak pasujących pozycji");
  await expect(page.getByRole("listbox").getByRole("option")).toHaveCount(0);
  // Enter bez pasującej pozycji nie wybiera niczego i nie wysyła formularza.
  await combo.press("Enter");
  await expect(combo).toHaveAttribute("aria-expanded", "true");
  await combo.press("Escape");
  await expect(combo).toHaveAttribute("aria-expanded", "false");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(combo).toHaveValue("Code review");
  await expect(combo).toHaveAttribute("data-value", value!);
  // Kliknięcie poza polem też zamyka listę bez zmiany.
  await combo.click();
  await combo.fill("wdro");
  await page.getByRole("heading", { name: /^Plan na/ }).click();
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(combo).toHaveValue("Code review");
});

test("SearchSelect: Escape w oknie podziału zamyka tylko listę", async ({
  page,
}) => {
  await openPreview(page, { active: { task: ids.analysis, from: "11:00" } });
  await page.getByRole("button", { name: "Podziel bieżący pomiar" }).click();
  const dialog = page.getByRole("dialog");
  const combo = dialog.getByRole("combobox", { name: "Zadanie drugiej części" });
  await combo.click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await combo.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await pick(combo, "Code review");
  await expect(combo).toHaveValue("Code review");
});

test("cykl: „Bez cyklu” jako pierwsza opcja, parametry dopiero po wyborze", async ({
  page,
}) => {
  await openPreview(page);
  await page
    .getByRole("button", { name: "Przedziały: Code review", exact: true })
    .click();
  const kind = page.getByLabel("Rodzaj cyklu");
  await expect(kind.locator("option").first()).toHaveText("Bez cyklu");
  await expect(kind).toHaveValue("none");
  await expect(page.getByLabel("Początek cyklu")).toHaveCount(0);
  await expect(page.getByLabel("Pon", { exact: true })).toHaveCount(0);

  // W wybrane dni tygodnia: dni i początek cyklu pojawiają się od razu,
  // także zanim zaznaczy się pierwszy dzień.
  await kind.selectOption("");
  await expect(page.getByLabel("Pon", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Początek cyklu")).toBeVisible();
  await page.getByLabel("Pon", { exact: true }).check();
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByRole("status")).toHaveText("Zapisano plan i notatki.");
  let task = (await stored(page)).tasks.find((t: any) => t.id === ids.review);
  expect(task.details.repeatDays).toEqual([1]);
  await expect(kind).toHaveValue("");

  // Co N tygodni: pole „Co ile tygodni” i dni tygodnia.
  await kind.selectOption("weeks");
  await expect(page.getByLabel("Co ile tygodni")).toHaveValue("2");
  await expect(page.getByLabel("Pon", { exact: true })).toBeChecked();

  // „Bez cyklu” wyłącza cykl i chowa parametry.
  await kind.selectOption("none");
  await expect(page.getByLabel("Co ile tygodni")).toHaveCount(0);
  await expect(page.getByLabel("Pon", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Zapisz plan i notatki" }).click();
  await expect(page.getByRole("status")).toHaveText("Zapisano plan i notatki.");
  task = (await stored(page)).tasks.find((t: any) => t.id === ids.review);
  expect(task.details.repeatDays).toEqual([]);
  expect(task.details.repeatRule?.kind ?? "").toBe("");
  await expect(kind).toHaveValue("none");

  // Zadanie z cyklem pokazuje swój rodzaj, nie „Bez cyklu”.
  await page
    .getByRole("button", { name: "Przedziały: Spotkanie zespołu", exact: true })
    .click();
  const standup = page
    .locator(".task-card", { hasText: "Spotkanie zespołu" })
    .getByLabel("Rodzaj cyklu");
  await expect(standup).toHaveValue("");
});

test("przedziały pracy: bez powtórzonej daty i bez sekund, data przy końcu tylko przez północ", async ({
  page,
}) => {
  expect(intervalText(at(TODAY, "08:30"), at(TODAY, "10:30"))).toEqual([
    "15 paź 2026, 08:30",
    "10:30",
  ]);
  const data = seed();
  data.sessions.push({
    id: 50,
    taskId: ids.analysis,
    // 15 s po pełnej minucie: podgląd bez sekund, edycja z sekundami.
    startedAt: at("2026-10-14", "23:00") + 15_000,
    endedAt: at(TODAY, "01:00"),
  });
  data.sessions.sort((a, b) => b.startedAt - a.startedAt);
  await openPreview(page, { data });
  await page
    .getByRole("button", { name: "Przedziały: Analiza importu", exact: true })
    .click();
  const rows = page.locator(".session-row .interval");
  await expect(rows.first()).toHaveText("15 paź 2026, 08:30→10:30");
  await expect(rows.nth(1)).toHaveText("14 paź 2026, 23:00→15 paź 2026, 01:00");
  await expect(rows.nth(2)).toHaveText("14 paź 2026, 09:30→15:30");
  // Sekundy zostają w edycji przedziału.
  await page.getByRole("button", { name: "Edytuj przedział 50" }).click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await expect(page.getByRole("dialog").locator("#session-start")).toHaveValue(
    "2026-10-14T23:00:15",
  );
});

test("Zamknięcie dnia: podpowiedź opisu z datą w formacie nagłówka", async ({
  page,
}) => {
  await openPreview(page);
  await nav(page, "Plan i historia");
  await nav(page, "Zamknięcie dnia");
  await expect(
    page.getByRole("heading", { name: "Zamknięcie dnia · czw., 15 paź" }),
  ).toBeVisible();
  await expect(page.getByLabel("Notatka: Analiza importu")).toHaveAttribute(
    "placeholder",
    "Uzupełnij opis pracy z czw., 15 paź",
  );
});

test("Ustawienia: puste pole liczbowe daje komunikat o zakresie, bez zapisu", async ({
  page,
}) => {
  await openDesktop(page);
  await openSettings(page);
  const target = page.getByLabel("Cel dzienny (minuty)");
  await expect(target).toHaveValue("480");
  await target.fill("");
  await target.press("Tab");
  await expect(page.getByRole("alert")).toHaveText(
    "Cel dzienny musi mieć od 0 do 1440 minut.",
  );
  // Pole wraca do zapisanej wartości; do backendu nic nie poszło.
  await expect(target).toHaveValue("480");
  const calls = await backend<{ cmd: string; args: any }[]>(page, "calls");
  expect(
    calls.filter(
      (c) => c.cmd === "set_settings" && "targetDayMinutes" in c.args.patch,
    ),
  ).toEqual([]);
  // Poprawna wartość zapisuje się jak wcześniej i czyści komunikat.
  await target.fill("450");
  await target.press("Tab");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(target).toHaveValue("450");

  // Inna sekcja: zakres bloku skupienia.
  await page.getByRole("tab", { name: "Skupienie", exact: true }).click();
  const block = page.getByLabel("Blok skupienia (minuty)");
  await block.fill("");
  await block.press("Tab");
  await expect(page.getByRole("alert")).toHaveText(
    "Blok skupienia musi mieć od 1 do 240 minut.",
  );
  await expect(block).toHaveValue("25");
});
