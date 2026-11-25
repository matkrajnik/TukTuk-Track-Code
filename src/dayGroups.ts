import { ref, watch, type Ref } from "vue";

/** Pozycje zadań o tej samej nazwie zebrane w jeden wiersz (raport pod TS). */
export interface TitleGroup<T> {
  title: string;
  total: number;
  items: T[];
}

/**
 * Grupuje pozycje po nazwie zadania (dokładnie ta sama nazwa po przycięciu
 * spacji). Kolejność grup: pierwsza pozycja grupy w kolejności wejścia, więc
 * oś dnia zostaje chronologiczna.
 */
export function groupByTitle<T>(
  items: readonly T[],
  titleOf: (item: T) => string,
  durationOf: (item: T) => number,
): TitleGroup<T>[] {
  const groups = new Map<string, TitleGroup<T>>();
  for (const item of items) {
    const title = titleOf(item).trim();
    const group = groups.get(title) ?? { title, total: 0, items: [] };
    groups.set(title, {
      ...group,
      total: group.total + durationOf(item),
      items: [...group.items, item],
    });
  }
  return [...groups.values()];
}

/**
 * Zapamiętany wybór „Grupuj po zadaniu” (per widok). Pamięć przeglądarki może
 * być niedostępna: wtedy wybór działa do zamknięcia okna.
 */
export function useGroupedPreference(key: string): Ref<boolean> {
  let initial = false;
  try {
    initial = localStorage.getItem(key) === "1";
  } catch {
    initial = false;
  }
  const grouped = ref(initial);
  watch(grouped, (on) => {
    try {
      localStorage.setItem(key, on ? "1" : "0");
    } catch {
      // Brak pamięci przeglądarki: wybór zostaje tylko w tym oknie.
    }
  });
  return grouped;
}
