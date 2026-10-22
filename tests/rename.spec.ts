import { test, expect } from "./fixtures/test";
import { openPreview } from "./fixtures/seed";
import { openSettings } from "./fixtures/settings";
import { legacyKey } from "../src/preview/legacy";

/**
 * Nowa nazwa aplikacji TukTuk Track (PLAN_BACKLOG.md, N1): tytuł, nawigacja,
 * Ustawienia, klucze `localStorage` i przeniesienie kluczy z poprzedniej nazwy.
 * Migracja katalogu, poświadczeń, rejestru i instalatora jest sprawdzana testami Rust.
 */
test.use({ timezoneId: "Europe/Warsaw" });

const OLD_PREFIX = "todo-track-";
const NEW_PREFIX = "tuktuk-track-";

test("tytuł okna i nawigacja mają nazwę TukTuk Track", async ({ page }) => {
  await openPreview(page);
  await expect(page).toHaveTitle("TukTuk Track");
  await expect(page.locator(".brand-text").first()).toHaveText(
    "tuktuktrack",
  );
  const html = await page.content();
  expect(html).not.toContain("Todo Track");
});

test("Ustawienia: nowe klucze localStorage i brak starych", async ({ page }) => {
  await openPreview(page);
  await openSettings(page, "Czas pracy");
  const keys = await page.evaluate(() =>
    Object.keys(localStorage).sort(),
  );
  expect(keys.length).toBeGreaterThan(0);
  expect(keys.filter((k) => k.startsWith(OLD_PREFIX))).toEqual([]);
  expect(keys.every((k) => k.startsWith(NEW_PREFIX))).toBe(true);
  expect(keys).toContain(NEW_PREFIX + "theme");
});

test("klucze localStorage z poprzedniej nazwy są kopiowane pod nowe, bez nadpisywania", async ({
  page,
}) => {
  await openPreview(page);
  await page.evaluate(
    ([oldPrefix, newPrefix]) => {
      localStorage.setItem(oldPrefix + "theme", "light");
      localStorage.setItem(oldPrefix + "nav-collapsed", "1");
      localStorage.setItem(oldPrefix + "chat-width", "400");
      localStorage.setItem(newPrefix + "chat-width", "520");
      localStorage.removeItem(newPrefix + "theme");
    },
    [OLD_PREFIX, NEW_PREFIX],
  );
  await page.reload();
  const read = (key: string) =>
    page.evaluate((k) => localStorage.getItem(k), key);
  await expect.poll(() => read(NEW_PREFIX + "theme")).toBe("light");
  expect(await read(NEW_PREFIX + "nav-collapsed")).toBe("1");
  // Nowy klucz ma pierwszeństwo, stary zostaje.
  expect(await read(NEW_PREFIX + "chat-width")).toBe("520");
  expect(await read(OLD_PREFIX + "chat-width")).toBe("400");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
});

test("Ustawienia → Uruchamianie: wynik migracji z poprzedniej nazwy", async ({
  page,
}) => {
  await openPreview(page);
  await openSettings(page, "Uruchamianie");
  await expect(page.getByLabel("Przeniesienie danych")).toHaveCount(0);
  await page.evaluate(
    ([key]) =>
      localStorage.setItem(
        key,
        JSON.stringify({
          dir: "both",
          reconnect: ["Jira"],
          message:
            "Znaleziono dane Todo Track i dane TukTuk Track. Używane są dane TukTuk Track.",
        }),
      ),
    [legacyKey],
  );
  await page.reload();
  await openSettings(page, "Uruchamianie");
  await expect(page.getByLabel("Przeniesienie danych")).toContainText(
    "Używane są dane TukTuk Track",
  );
});
