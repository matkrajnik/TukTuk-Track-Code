import { test, expect, type Page } from "./fixtures/test";
import {
  TODAY,
  at,
  ids,
  openPreview,
  seed,
  stored,
} from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * B6 — wydajność: okno, które samo wywołało komendę, przyjmuje migawkę z jej
 * wyniku i pomija `data-changed` oznaczone własną etykietą (bez drugiego
 * `get_snapshot`); zmiany z innych okien i z backendu nadal odświeżają.
 * Zamknięcie dnia zapisuje notatki jedną komendą, a „Przenieś niedokończone”
 * zmienia tylko dzień planu i kolejność.
 */
test.use({ timezoneId: "Europe/Warsaw", locale: "pl-PL" });

const calls = (page: Page) =>
  backend<{ cmd: string; args: any }[]>(page, "calls");
const snapshots = async (page: Page) =>
  (await calls(page)).filter((c) => c.cmd === "get_snapshot").length;
const emitChange = (page: Page, payload: unknown) =>
  page.evaluate(
    (p) => (window as any).__tauriMock.emit("data-changed", p),
    payload,
  );

test("akcja w głównym oknie nie odczytuje migawki drugi raz, zmiana z innego okna tak", async ({
  page,
}) => {
  await openDesktop(page);
  await expect(
    page.getByLabel("Start: Code review", { exact: true }),
  ).toBeVisible();
  await expect.poll(() => snapshots(page)).toBeGreaterThan(0);
  const before = await snapshots(page);

  // Start i Stop (`mutate`): wynik komendy wystarcza, bez `get_snapshot`.
  await page.getByLabel("Start: Code review", { exact: true }).click();
  await expect(
    page.getByLabel("Stop: Code review", { exact: true }),
  ).toBeVisible();
  await page.getByLabel("Stop: Code review", { exact: true }).click();
  await expect(
    page.getByLabel("Start: Code review", { exact: true }),
  ).toBeVisible();
  const own = (await calls(page)).filter((c) => c.cmd === "mutate");
  expect(own.map((c) => c.args.action)).toEqual(["start", "stop"]);
  expect(await snapshots(page)).toBe(before);

  // Zdarzenie z własną etykietą (np. spóźnione) też niczego nie odczytuje.
  await emitChange(page, { source: "main" });
  // Zmiana z widgetu i z backendu (wątek cykli, tray: `null`) odświeża.
  await emitChange(page, { source: "widget" });
  await expect.poll(() => snapshots(page)).toBe(before + 1);
  await emitChange(page, null);
  await expect.poll(() => snapshots(page)).toBe(before + 2);
});

test("widget pomija własne zmiany i odświeża się po zmianie z głównego okna", async ({
  page,
}) => {
  await openDesktop(page, { window: "widget" });
  await expect.poll(() => snapshots(page)).toBeGreaterThan(0);
  const before = await snapshots(page);
  await emitChange(page, { source: "widget" });
  await emitChange(page, { source: "main" });
  await expect.poll(() => snapshots(page)).toBe(before + 1);
  await emitChange(page, { source: "quick" });
  await expect.poll(() => snapshots(page)).toBe(before + 2);
});

test("Zamknięcie dnia: notatki zapisane razem, przeniesienie zmienia tylko dzień i kolejność", async ({
  page,
}) => {
  const data = seed();
  const tasks = data.tasks as any[];
  const analysis = tasks.find((t) => t.id === ids.analysis);
  // Przypięte zadanie z kolejnością; jutro jest już zadanie z order 7.
  Object.assign(analysis.details, { pinnedAt: at(TODAY, "07:00"), order: 2 });
  Object.assign(tasks.find((t) => t.id === ids.labels).details, {
    plannedDay: "2026-10-16",
    order: 7,
  });
  await openPreview(page, { data });
  await page
    .getByRole("button", { name: "Plan i historia", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Zamknięcie dnia", exact: true })
    .click();

  await page
    .getByLabel("Notatka: Analiza importu", { exact: true })
    .fill("Import CSV");
  await page
    .getByLabel("Notatka: Spotkanie zespołu", { exact: true })
    .fill("Planowanie sprintu");
  await page
    .getByRole("button", { name: "Zapisz notatki dnia", exact: true })
    .click();
  await expect(page.getByText("Zapisano notatki.")).toBeVisible();
  const notesOf = async (id: number) =>
    (await stored(page)).tasks.find((t: any) => t.id === id).details.notes;
  await expect.poll(() => notesOf(ids.analysis)).toEqual({
    "2026-10-13": "Walidacja CSV",
    "2026-10-14": "Mapowanie kolumn",
    [TODAY]: "Import CSV",
  });
  expect((await notesOf(ids.standup))[TODAY]).toBe("Planowanie sprintu");

  // Pusta notatka usuwa wpis dnia, inne dni zostają.
  await page.getByLabel("Notatka: Analiza importu", { exact: true }).fill("");
  await page
    .getByRole("button", { name: "Zapisz notatki dnia", exact: true })
    .click();
  await expect
    .poll(async () => Object.keys(await notesOf(ids.analysis)))
    .toEqual(["2026-10-13", "2026-10-14"]);

  await page
    .getByRole("button", {
      name: "Przenieś niedokończone na pt., 16 paź",
      exact: true,
    })
    .click();
  await expect
    .poll(async () => {
      const d = (await stored(page)).tasks.find(
        (t: any) => t.id === ids.analysis,
      ).details;
      return [d.plannedDay, d.order, d.pinnedAt, d.estimateMinutes];
    })
    .toEqual(["2026-10-16", 8, at(TODAY, "07:00"), 600]);
});

test("Szukaj w historii: czas według dnia i zadania z trwającym pomiarem", async ({
  page,
}) => {
  await openPreview(page, {
    data: seed({ active: { task: ids.review, from: "11:00" } }),
  });
  await page
    .getByRole("button", { name: "Historia i kopie", exact: true })
    .click();
  const rows = page.locator(".library-row");
  // Dziś najpierw, w kolejności zadań: Analiza, Code review (trwa od 11:00), Spotkanie.
  await expect(rows.nth(0)).toContainText("Analiza importu");
  await expect(rows.nth(1)).toContainText("Code review");
  await expect(rows.nth(1)).toContainText("01:00:00");
  await expect(rows.nth(2)).toContainText("Spotkanie zespołu");
  await page
    .getByLabel("Szukaj tytułu, notatki, projektu lub daty")
    .fill("analiza");
  await expect(rows).toHaveCount(3);
  await expect(rows.nth(0)).toContainText("02:00:00");
  await expect(rows.nth(1)).toContainText("Mapowanie kolumn");
  await expect(rows.nth(1)).toContainText("06:00:00");
  await expect(rows.nth(2)).toContainText("03:00:00");
  await page.getByLabel("Od dnia").fill("2026-10-14");
  await page.getByLabel("Do dnia").fill("2026-10-14");
  await expect(rows).toHaveCount(1);
});
