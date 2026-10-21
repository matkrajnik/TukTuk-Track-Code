import { test, expect, type Page } from "./fixtures/test";
import { backend, openDesktop } from "./fixtures/tauri";
import { openSettings } from "./fixtures/settings";
import { ids, seed } from "./fixtures/seed";

/**
 * Aktualizacje (src/updater.ts, UpdateBanner.vue): baner przy dostępnej nowej
 * wersji, brak banera przy najnowszej wersji i cisza przy błędzie sieci.
 * Plugin updater zastępuje fixture `tauri.ts` (opcja `update`).
 */
test.use({ timezoneId: "Europe/Warsaw" });

const banner = (page: Page) =>
  page.getByRole("region", { name: "Dostępna aktualizacja" });
const checks = async (page: Page) =>
  (await backend<{ cmd: string }[]>(page, "calls")).filter(
    (c) => c.cmd === "plugin:updater|check",
  ).length;
/** Czeka na automatyczne sprawdzenie chwilę po starcie. */
async function expectChecked(page: Page) {
  await expect.poll(() => checks(page), { timeout: 10_000 }).toBe(1);
}

test("dostępna aktualizacja pokazuje baner z wersją i notatkami; „Później” go chowa", async ({
  page,
}) => {
  await openDesktop(page, {
    update: {
      version: "1.8.0",
      notes: "- Automatyczne aktualizacje\n- Poprawki tablicy tygodnia",
      date: "2026-10-20T10:00:00Z",
    },
  });
  await expect(banner(page)).toBeVisible();
  await expect(
    banner(page).getByRole("heading", {
      name: "Dostępna jest nowa wersja Todo Track 1.8.0",
    }),
  ).toBeVisible();
  await expect(banner(page)).toContainText("Automatyczne aktualizacje");
  await expect(banner(page)).toContainText("Poprawki tablicy tygodnia");
  await expect(
    banner(page).getByRole("button", { name: "Zainstaluj i uruchom ponownie" }),
  ).toBeEnabled();
  // Baner jest informacją, nie alertem: nie przerywa pracy.
  await expect(page.getByRole("alert")).toHaveCount(0);
  await banner(page).getByRole("button", { name: "Później" }).click();
  await expect(banner(page)).toHaveCount(0);
  // Ręczne sprawdzenie znajduje tę samą wersję i pokazuje baner znowu.
  await openSettings(page, "Uruchamianie");
  await page.getByRole("button", { name: "Sprawdź teraz" }).click();
  await expect(page.getByRole("status")).toContainText(
    "Dostępna jest wersja 1.8.0",
  );
  await expect(banner(page)).toBeVisible();
});

test("„Zainstaluj i uruchom ponownie” pobiera z paskiem postępu, kończy pomiar i restartuje aplikację", async ({
  page,
}) => {
  await openDesktop(page, {
    update: { version: "1.8.0" },
    data: seed({ active: { task: ids.analysis, from: "11:00" } }),
  });
  await expect(
    page.getByLabel("Stop: Analiza importu", { exact: true }),
  ).toBeVisible();
  await expect(banner(page)).toContainText(
    "Aktualizacja pobierze się i zainstaluje bez utraty zadań",
  );
  const install = banner(page).getByRole("button", {
    name: "Zainstaluj i uruchom ponownie",
  });
  await install.click();
  await expect(install).toBeDisabled();
  await expect(banner(page).getByRole("button", { name: "Później" })).toBeDisabled();
  const progress = banner(page).getByRole("status");
  await expect(progress).toContainText("Pobieranie…");
  await expect(progress.getByRole("progressbar")).toBeVisible();
  await expect(progress).toContainText("Instalowanie… Aplikacja uruchomi się ponownie.");
  await expect
    .poll(async () =>
      (await backend<{ cmd: string; args: any }[]>(page, "calls")).map(
        (c) => c.cmd,
      ),
    )
    .toContain("plugin:process|restart");
  // Kolejność: pobranie → koniec pomiaru i zapis stanu (prepare_update) →
  // instalacja → restart; pomiar „Analiza importu” jest zamknięty.
  const calls = await backend<{ cmd: string; args: any }[]>(page, "calls");
  const order = [
    "plugin:updater|download",
    "prepare_update",
    "plugin:updater|install",
    "plugin:process|restart",
  ].map((cmd) => calls.findIndex((c) => c.cmd === cmd));
  expect(order.every((i) => i >= 0)).toBe(true);
  expect([...order].sort((a, b) => a - b)).toEqual(order);
  expect(calls[order[0]].args.rid).toBe(1);
  const snapshot = await backend<any>(page, "snapshot");
  expect(snapshot.sessions.some((s: any) => s.endedAt === null)).toBe(false);
});

test("brak nowszej wersji: sprawdzenie przy starcie bez banera; „Sprawdź teraz” potwierdza", async ({
  page,
}) => {
  await openDesktop(page);
  await expectChecked(page);
  await expect(banner(page)).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await openSettings(page, "Uruchamianie");
  const toggle = page.getByLabel("Sprawdzaj aktualizacje automatycznie", {
    exact: true,
  });
  await expect(toggle).toBeChecked();
  await page.getByRole("button", { name: "Sprawdź teraz" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Masz najnowszą wersję (test).",
  );
  expect(await checks(page)).toBe(2);
  await expect(banner(page)).toHaveCount(0);
});

test("błąd sieci: nic nie pokazuje przy starcie, a „Sprawdź teraz” zgłasza brak połączenia", async ({
  page,
}) => {
  await openDesktop(page, { update: "offline" });
  await expectChecked(page);
  await expect(banner(page)).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveCount(0);
  await openSettings(page, "Uruchamianie");
  await page.getByRole("button", { name: "Sprawdź teraz" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Nie udało się połączyć z serwerem aktualizacji.",
  );
  // Błąd ręcznego sprawdzenia też nie jest alertem w głównym oknie.
  await expect(page.getByRole("alert")).toHaveCount(0);
});

test("wyłączone automatyczne sprawdzanie nie pyta serwera przy starcie i zapisuje ustawienie", async ({
  page,
}) => {
  await openDesktop(page, {
    update: { version: "1.8.0" },
    settings: { autoUpdateCheck: false },
  });
  await openSettings(page, "Uruchamianie");
  const toggle = page.getByLabel("Sprawdzaj aktualizacje automatycznie", {
    exact: true,
  });
  await expect(toggle).not.toBeChecked();
  // Chwila dłuższa niż opóźnienie pierwszego sprawdzenia po starcie.
  await page.waitForTimeout(2_600);
  expect(await checks(page)).toBe(0);
  await expect(banner(page)).toHaveCount(0);
  await toggle.check();
  await expect
    .poll(async () =>
      (await backend<{ cmd: string; args: any }[]>(page, "calls")).find(
        (c) => c.cmd === "set_settings",
      )?.args.patch,
    )
    .toEqual({ autoUpdateCheck: true });
  // „Sprawdź teraz” działa niezależnie od przełącznika.
  await page.getByRole("button", { name: "Sprawdź teraz" }).click();
  await expect(banner(page)).toBeVisible();
});
