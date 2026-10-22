/**
 * Wynik migracji z poprzedniej nazwy aplikacji (PLAN_BACKLOG.md, N1): typ komendy
 * `legacy_status` (`legacy.rs`). Odczyt jest w `store.ts` (`readLegacyStatus`),
 * w podglądzie symulator z `preview/legacy.ts`.
 */

/** `legacy::Status`. */
export interface LegacyStatus {
  /** Czas zakończenia migracji bez błędów (ms); `null`: jeszcze nie albo z błędami. */
  migratedAt: number | null;
  /** `none`, `copied`, `alreadyMoved`, `both`, `failed`. */
  dir: string;
  /** Moduły, których logowanie trzeba połączyć ponownie. */
  reconnect: string[];
  /** Zdanie do pokazania w Ustawieniach; pusty tekst: nic do pokazania. */
  message: string;
}
