/** Wspólne pomocnicze symulatorów podglądu: localStorage i zdarzenia okna. */

/** Adres Jiry w podglądzie (domyślny z `jira::Config`). */
export const previewJiraSite = "https://asistonwms.atlassian.net";
export const previewSettingsKey = "tuktuk-track-preview-settings-v1";
export function previewJson(name: string): Record<string, unknown> {
  try {
    const value = JSON.parse(localStorage.getItem(name) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value)
      ? value
      : {};
  } catch {
    return {};
  }
}
/**
 * Odpowiednik zdarzenia Rust `data-changed` w podglądzie (to samo okno):
 * App odczytuje dane na nowo po zmianach z symulowanej Jiry.
 */
export const previewDataEvent = "preview-data-changed";
export function notifyPreviewData() {
  window.dispatchEvent(new Event(previewDataEvent));
}
