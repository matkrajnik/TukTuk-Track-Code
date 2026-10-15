import type { Page } from "@playwright/test";

/**
 * Zakładka Ustawienia z nawigacji głównej (od 1.7 osobna, wcześniej
 * „Historia i kopie → Ustawienia”) i, opcjonalnie, wybrana sekcja z listy.
 */
export async function openSettings(page: Page, section?: string) {
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  if (section)
    await page.getByRole("tab", { name: section, exact: true }).click();
}
