/**
 * Historia powiadomień (`inbox.rs`): dzwoneczek w górnym pasku. W aplikacji lista
 * przychodzi z Rust (`notification_log`, zdarzenie `notification-log-changed`).
 * Podgląd nie wysyła powiadomień, więc lista leży w localStorage
 * (`previewInboxKey`) i zmienia ją tylko `previewNotify` albo test.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop, type NavigateRequest } from "./store";

/** Odpowiednik `inbox::Entry`. */
export interface InboxEntry {
  id: number;
  /** Chwila powiadomienia (ms). */
  at: number;
  title: string;
  body: string;
  /** Dokąd prowadzi kliknięcie (`notify::View`); `null` tylko pokazuje okno. */
  open: NavigateRequest | null;
  read: boolean;
}

export const previewInboxKey = "tuktuk-track-preview-notifications-v1";
const previewInboxEvent = "preview-inbox-changed";
/** Jak `inbox::KEEP`. */
const PREVIEW_KEEP = 200;

function isEntry(value: unknown): value is InboxEntry {
  if (!value || typeof value !== "object") return false;
  const e = value as Record<string, unknown>;
  return (
    typeof e.id === "number" &&
    typeof e.at === "number" &&
    typeof e.title === "string" &&
    typeof e.body === "string" &&
    typeof e.read === "boolean"
  );
}

function previewRead(): InboxEntry[] {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem(previewInboxKey) ?? "[]",
    );
    return Array.isArray(value)
      ? value
          .filter(isEntry)
          .map((e) => ({ ...e, open: e.open ?? null }))
          .sort((a, b) => b.at - a.at || b.id - a.id)
      : [];
  } catch {
    return [];
  }
}

function previewWrite(list: InboxEntry[]): InboxEntry[] {
  const kept = list.slice(0, PREVIEW_KEEP);
  try {
    localStorage.setItem(previewInboxKey, JSON.stringify(kept));
  } catch {
    // Podgląd bez localStorage: lista żyje do przeładowania.
  }
  window.dispatchEvent(new CustomEvent(previewInboxEvent, { detail: kept }));
  return kept;
}

/** Podgląd `inbox::record`: dopisuje powiadomienie na początek historii. */
export function previewNotify(
  title: string,
  body: string,
  open: NavigateRequest | null = null,
): InboxEntry[] {
  const list = previewRead();
  const id = Math.max(0, ...list.map((e) => e.id)) + 1;
  return previewWrite([
    { id, at: Date.now(), title, body, open, read: false },
    ...list,
  ]);
}

export async function readInbox(): Promise<InboxEntry[]> {
  if (desktop) return invoke("notification_log");
  return previewRead();
}

/** „Przeczytane”: jeden wpis albo wszystkie (`id` pominięte). */
export async function markInboxRead(id?: number): Promise<InboxEntry[]> {
  if (desktop) return invoke("notification_log_read", { id: id ?? null });
  return previewWrite(
    previewRead().map((e) =>
      id === undefined || e.id === id ? { ...e, read: true } : e,
    ),
  );
}

export async function clearInbox(): Promise<InboxEntry[]> {
  if (desktop) return invoke("notification_log_clear");
  return previewWrite([]);
}

export async function onInboxChanged(
  handler: (list: InboxEntry[]) => void,
): Promise<() => void> {
  if (desktop)
    return listen<InboxEntry[]>("notification-log-changed", (e) =>
      handler(e.payload),
    );
  const listener = (e: Event) =>
    handler((e as CustomEvent<InboxEntry[]>).detail ?? []);
  window.addEventListener(previewInboxEvent, listener);
  return () => window.removeEventListener(previewInboxEvent, listener);
}

export function unreadCount(list: readonly InboxEntry[]): number {
  return list.filter((e) => !e.read).length;
}
