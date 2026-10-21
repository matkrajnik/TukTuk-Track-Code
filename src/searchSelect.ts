/** Pozycja listy `SearchSelect`: wartość, etykieta i opcjonalna grupa. */
export interface SearchOption<T extends string | number = string | number> {
  value: T;
  label: string;
  /** Nagłówek grupy (np. projekt TS); kolejne pozycje z tą samą grupą stoją razem. */
  group?: string;
  disabled?: boolean;
}

/** Znaki łączące po rozkładzie NFD (ogonki, kreski, kropki). */
const marks = new RegExp("[\\u0300-\\u036f]", "g");

/** Tekst do porównań: małe litery, bez polskich znaków diakrytycznych. */
export function fold(text: string): string {
  return text
    .toLocaleLowerCase("pl-PL")
    .normalize("NFD")
    .replace(marks, "")
    .replace(/ł/g, "l");
}

/**
 * Pozycje pasujące do wpisanego tekstu: każde słowo zapytania musi wystąpić
 * w etykiecie albo w nazwie grupy (wielkość liter i ogonki bez znaczenia).
 */
export function filterOptions<T extends SearchOption>(
  options: readonly T[],
  query: string,
): T[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (!words.length) return [...options];
  return options.filter((o) => {
    const text = fold(`${o.label} ${o.group ?? ""}`);
    return words.every((w) => text.includes(w));
  });
}
