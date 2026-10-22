/**
 * Symulowany wynik migracji z poprzedniej nazwy aplikacji (`legacy_status`, N1).
 *
 * Klucz `tuktuk-track-preview-legacy-v1`: `{ migratedAt?, dir?, reconnect?, message? }`;
 * bez klucza migracja „nie miała nic do zrobienia” (pusty komunikat).
 */
import type { LegacyStatus } from "../api/legacy";
import { previewJson } from "./storage";

export const legacyKey = "tuktuk-track-preview-legacy-v1";

export function previewLegacyStatus(): LegacyStatus {
  const stored = previewJson(legacyKey);
  return {
    migratedAt: typeof stored.migratedAt === "number" ? stored.migratedAt : null,
    dir: typeof stored.dir === "string" ? stored.dir : "none",
    reconnect: Array.isArray(stored.reconnect)
      ? stored.reconnect.map(String)
      : [],
    message: typeof stored.message === "string" ? stored.message : "",
  };
}
