import { test as base, expect, type Page } from "@playwright/test";

export * from "@playwright/test";

/** Klucz stanu „Rano pokazano dziś” (`src/morningStart.ts`). */
export const MORNING_SHOWN_KEY = "tuktuk-track-morning-shown-v1";

/**
 * Wspólny `test` dla specyfikacji, które startują aplikację i oczekują listy
 * zadań jako pierwszego widoku. Od B11 pierwsze uruchomienie w dniu roboczym
 * bez zapisanego przeglądu otwiera Plan i historia → Rano; tutaj każda strona
 * kontekstu zaczyna ze stanem „Rano już pokazane” (dzień daleko w przyszłości,
 * więc także po zmianie dnia zegarem i po `localStorage.clear()` z
 * przeładowaniem). Zachowanie samego planu dnia testuje `tests/morning-start.spec.ts`
 * z `test` prosto z `@playwright/test`.
 */
export const test = base.extend<{ morningShown: void }>({
  morningShown: [
    async ({ context }, use) => {
      await context.addInitScript((key) => {
        try {
          localStorage.setItem(key, "9999-12-31");
        } catch {
          /* strona bez localStorage (np. about:blank) */
        }
      }, MORNING_SHOWN_KEY);
      await use();
    },
    { auto: true },
  ],
});

/**
 * „Zaplanuj” w formularzu „Nowe zadanie”: zadanie bez startu pomiaru. Przycisk
 * rozwija kafelek nowego zadania z planem; helper go zwija, żeby dalsze kroki
 * widziały listę jak po dawnym „Dodaj zadanie”. `title` = tytuł na kafelku,
 * gdy różni się od wpisanego tekstu (składnia).
 */
export async function addPlanned(page: Page, text: string, title = text) {
  await page.getByLabel("Nowe zadanie", { exact: true }).fill(text);
  await page.getByRole("button", { name: "Zaplanuj", exact: true }).click();
  const toggle = page.getByRole("button", {
    name: "Przedziały: " + title,
    exact: true,
  });
  await expect(toggle).toHaveAttribute("aria-expanded", "true");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
}
