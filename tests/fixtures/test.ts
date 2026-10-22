import { test as base } from "@playwright/test";

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
