import type { Locator } from "@playwright/test";

/**
 * Wybór pozycji w `SearchSelect` (pole `combobox` z listą): otwiera listę,
 * wpisuje fragment etykiety i klika pozycję o dokładnie tej etykiecie.
 * Zastępuje `selectOption` z czasów natywnego `<select>`.
 */
export async function pick(
  combo: Locator,
  label: string,
  query: string = label,
): Promise<void> {
  await combo.click();
  await combo.fill(query);
  await combo
    .page()
    .getByRole("listbox")
    .getByRole("option", { name: label, exact: true })
    .click();
}

/** Etykiety pozycji otwartej listy pola (bez nagłówków grup). */
export async function optionLabels(combo: Locator): Promise<string[]> {
  await combo.click();
  const labels = await combo
    .page()
    .getByRole("listbox")
    .getByRole("option")
    .allTextContents();
  await combo.press("Escape");
  return labels.map((l) => l.trim());
}
