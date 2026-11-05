import { test, expect, type Page } from "./fixtures/test";
import { ids, NOW, openPreview } from "./fixtures/seed";
import { backend, openDesktop } from "./fixtures/tauri";

/**
 * Historia powiadomień (`inbox.rs`, `components/NotificationBell.vue`):
 * dzwoneczek w górnym pasku z licznikiem nieprzeczytanych, lista od
 * najnowszych, „przeczytane” pojedynczo i wszystkie, czyszczenie historii
 * i przejście do widoku z powiadomienia.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const now = Date.parse(NOW);
const entry = (
  id: number,
  minutesAgo: number,
  title: string,
  body: string,
  open: object = { view: null },
  read = false,
) => ({ id, at: now - minutesAgo * 60_000, title, body, open, read });

const bell = (page: Page) =>
  page.getByRole("button", { name: /^Powiadomienia/ });
const panel = (page: Page) =>
  page.getByRole("dialog", { name: "Historia powiadomień" });

test("dzwoneczek pokazuje nieprzeczytane i całą historię od najnowszych", async ({
  page,
}) => {
  await openDesktop(page, {
    notifications: [
      entry(3, 5, "Nowe zadanie z Redmine", "#481 Raport magazynowy"),
      entry(2, 30, "Spotkanie za 5 min", "Daily 10:00"),
      entry(1, 90, "Nowe zadanie z Jiry", "WMS-31 Import", {}, true),
    ],
  });
  await expect(bell(page)).toHaveAccessibleName(
    "Powiadomienia: nieprzeczytane 2",
  );
  await bell(page).click();
  const items = panel(page).getByRole("listitem");
  await expect(items).toHaveCount(3);
  await expect(items.nth(0)).toContainText("Nowe zadanie z Redmine");
  await expect(items.nth(1)).toContainText("Spotkanie za 5 min");
  await expect(items.nth(2)).toContainText("WMS-31 Import");
  await expect(
    panel(page).getByRole("button", { name: /^Nieprzeczytane: / }),
  ).toHaveCount(2);

  // Nowe powiadomienie z backendu trafia na górę i zwiększa licznik.
  await backend(page, "notify", {
    title: "Przypomnienie",
    body: "Wyślij raport",
  });
  await expect(items).toHaveCount(4);
  await expect(items.nth(0)).toContainText("Wyślij raport");
  await expect(bell(page)).toHaveAccessibleName(
    "Powiadomienia: nieprzeczytane 3",
  );

  await panel(page)
    .getByRole("button", { name: "Oznacz wszystkie jako przeczytane" })
    .click();
  await expect(bell(page)).toHaveAccessibleName("Powiadomienia");
  await expect(
    panel(page).getByRole("button", { name: /^Nieprzeczytane: / }),
  ).toHaveCount(0);

  // Escape zamyka panel i wraca na dzwoneczek.
  await page.keyboard.press("Escape");
  await expect(panel(page)).toBeHidden();
  await expect(bell(page)).toBeFocused();
});

test("kliknięcie wpisu oznacza go jako przeczytany i otwiera widok", async ({
  page,
}) => {
  await openDesktop(page, {
    notifications: [
      entry(2, 1, "Nowe zadanie z Jiry", "Analiza importu", {
        view: "tasks",
        task: ids.analysis,
        section: "Nierozpoczęte",
      }),
      entry(1, 10, "Spotkanie za 5 min", "Daily 10:00"),
    ],
  });
  await page.getByRole("button", { name: "Dashboard", exact: true }).click();
  await bell(page).click();
  await panel(page)
    .getByRole("button", { name: /Nieprzeczytane: Nowe zadanie z Jiry/ })
    .click();
  await expect(panel(page)).toBeHidden();
  await expect(
    page
      .getByRole("navigation", { name: "Nawigacja główna" })
      .getByRole("button", { name: "Zadania", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await expect(
    page.getByRole("button", { name: "Przedziały: Analiza importu", exact: true }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(bell(page)).toHaveAccessibleName(
    "Powiadomienia: nieprzeczytane 1",
  );
  const read = (await backend<{ cmd: string; args: any }[]>(page, "calls"))
    .filter((c) => c.cmd === "notification_log_read")
    .map((c) => c.args.id);
  expect(read).toEqual([2]);
});

test("wyczyszczenie historii zostawia pusty panel", async ({ page }) => {
  await openDesktop(page, {
    notifications: [entry(1, 1, "Spotkanie za 5 min", "Daily 10:00")],
  });
  await bell(page).click();
  await panel(page)
    .getByRole("button", { name: "Wyczyść historię powiadomień" })
    .click();
  await expect(panel(page)).toContainText("Brak powiadomień");
  await expect(bell(page)).toHaveAccessibleName("Powiadomienia");
});

test("podgląd trzyma historię w localStorage", async ({ page }) => {
  await openPreview(page);
  await page.evaluate(
    ([key, list]) => localStorage.setItem(key, JSON.stringify(list)),
    [
      "tuktuk-track-preview-notifications-v1",
      [entry(1, 2, "Spotkanie za 5 min", "Daily 10:00")],
    ] as const,
  );
  await page.reload();
  await expect(bell(page)).toHaveAccessibleName(
    "Powiadomienia: nieprzeczytane 1",
  );
  await bell(page).click();
  await panel(page)
    .getByRole("button", { name: /Nieprzeczytane: Spotkanie/ })
    .click();
  await expect(bell(page)).toHaveAccessibleName("Powiadomienia");
  const stored = await page.evaluate(() =>
    JSON.parse(
      localStorage.getItem("tuktuk-track-preview-notifications-v1") ?? "[]",
    ),
  );
  expect(stored[0].read).toBe(true);
});
