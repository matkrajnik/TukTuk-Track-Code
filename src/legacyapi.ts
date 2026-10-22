/**
 * Komenda `legacy_status` (N1) z odpowiednikiem w podglądzie (`preview/legacy.ts`);
 * osobno, żeby `store.ts` został cienką warstwą.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { LegacyStatus } from "./api/legacy";
import { previewLegacyStatus } from "./preview/legacy";

export type { LegacyStatus } from "./api/legacy";

const desktop = isTauri();

/** Wynik migracji z poprzedniej nazwy aplikacji (katalog danych, logowania do połączenia ponownie). */
export async function readLegacyStatus(): Promise<LegacyStatus> {
  if (desktop) return invoke("legacy_status");
  return previewLegacyStatus();
}
