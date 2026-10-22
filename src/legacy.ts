/**
 * Klucze `localStorage` sprzed zmiany nazwy aplikacji (PLAN_BACKLOG.md, N1).
 *
 * Dane WebView2 starej instalacji są kopiowane razem z katalogiem aplikacji
 * (`legacy.rs`), ale klucze mają nowy przedrostek. Przy starcie każdy stary klucz
 * jest kopiowany pod nowy, o ile nowego jeszcze nie ma (motyw, układ okna,
 * zapamiętane wysyłki do TS). Stare klucze zostają.
 */
const OLD_PREFIX = "todo-track-";
const NEW_PREFIX = "tuktuk-track-";

export function migrateLocalStorage(storage: Storage = localStorage) {
  try {
    const keys: string[] = [];
    for (let i = 0; i < storage.length; i++) {
      const key = storage.key(i);
      if (key?.startsWith(OLD_PREFIX)) keys.push(key);
    }
    for (const key of keys) {
      const next = NEW_PREFIX + key.slice(OLD_PREFIX.length);
      if (storage.getItem(next) !== null) continue;
      const value = storage.getItem(key);
      if (value !== null) storage.setItem(next, value);
    }
  } catch {
    // Bez localStorage (zablokowane dane witryny) nie ma czego przenosić.
  }
}
