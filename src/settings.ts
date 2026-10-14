import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop } from "./store";

/** Odpowiednik `AppSettings` z src-tauri/src/settings.rs (klucze camelCase). */
export interface AppSettings {
  idleEnabled: boolean;
  targetDayMinutes: number;
  workDays: number[];
  dayStart: string;
  widgetEnabled: boolean;
  widgetBackgroundOnly: boolean;
  autostartHidden: boolean;
  notifyDue: boolean;
  notifyLongMinutes: number;
  notifyEvening: string;
  shortcutQuick: string;
  shortcutToggle: string;
}
export interface AppInfo {
  version: string;
  shortcutError: string;
}
export const defaultSettings: Readonly<AppSettings> = Object.freeze({
  idleEnabled: true,
  targetDayMinutes: 480,
  workDays: [1, 2, 3, 4, 5],
  dayStart: "08:00",
  widgetEnabled: false,
  widgetBackgroundOnly: false,
  autostartHidden: false,
  notifyDue: true,
  notifyLongMinutes: 240,
  notifyEvening: "17:30",
  shortcutQuick: "Control+Shift+Space",
  shortcutToggle: "Control+Alt+Space",
});
const key = "todo-track-preview-settings-v1";
const eventName = "settings-changed";
const time = /^([01]\d|2[0-3]):[0-5]\d$/;
const shortcut = /^(?:[A-Za-z]+\+)*[A-Za-z0-9]+$/;

const fresh = (): AppSettings => ({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
});
const minutes = (v: unknown) =>
  typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 1440;

/** Ta sama walidacja co w Rust; zwraca nowe ustawienia albo rzuca błąd po polsku. */
export function applySettings(
  current: AppSettings,
  patch: Partial<AppSettings>,
): AppSettings {
  if (!patch || typeof patch !== "object" || Array.isArray(patch))
    throw new Error("Zmiana ustawień musi być obiektem.");
  const next: Record<string, unknown> = { ...current };
  for (const [name, value] of Object.entries(patch)) {
    if (!(name in defaultSettings))
      throw new Error(`Nieznane ustawienie: ${name}.`);
    const expected = (defaultSettings as Record<string, unknown>)[name];
    const ok = Array.isArray(expected)
      ? Array.isArray(value)
      : typeof value === typeof expected;
    if (!ok) throw new Error("Niepoprawna wartość ustawienia.");
    next[name] = Array.isArray(value) ? [...value] : value;
  }
  const s = next as unknown as AppSettings;
  if (!minutes(s.targetDayMinutes))
    throw new Error("Cel dzienny musi mieć od 0 do 1440 minut.");
  if (!minutes(s.notifyLongMinutes))
    throw new Error("Próg długiego pomiaru musi mieć od 0 do 1440 minut.");
  if (s.workDays.some((d) => !Number.isInteger(d) || d < 1 || d > 7))
    throw new Error("Dni robocze muszą być liczbami od 1 do 7.");
  s.workDays = [...new Set(s.workDays)].sort((a, b) => a - b);
  if (!time.test(s.dayStart))
    throw new Error("Początek dnia musi mieć format GG:MM.");
  if (!time.test(s.notifyEvening))
    throw new Error(
      "Godzina wieczornego przypomnienia musi mieć format GG:MM.",
    );
  if (
    [s.shortcutQuick, s.shortcutToggle].some(
      (v) => !v || v.length > 64 || !shortcut.test(v),
    )
  )
    throw new Error("Niepoprawny skrót klawiszowy.");
  if (s.shortcutQuick.toLowerCase() === s.shortcutToggle.toLowerCase())
    throw new Error(
      "Skróty szybkiego przełączania i Start/Stop muszą być różne.",
    );
  return s;
}

function previewLoad(): AppSettings {
  let stored: Record<string, unknown> = {};
  try {
    stored = JSON.parse(localStorage.getItem(key) ?? "{}") ?? {};
  } catch {
    stored = {};
  }
  let settings = fresh();
  // Jak w Rust: każda zapisana wartość osobno, uszkodzona zostaje pominięta.
  for (const [name, value] of Object.entries(stored)) {
    try {
      settings = applySettings(settings, { [name]: value });
    } catch {
      /* wartość domyślna */
    }
  }
  return settings;
}

export async function readSettings(): Promise<AppSettings> {
  if (desktop) return invoke("get_settings");
  return previewLoad();
}

export async function writeSettings(
  patch: Partial<AppSettings>,
): Promise<AppSettings> {
  if (desktop) return invoke("set_settings", { patch });
  const next = applySettings(previewLoad(), patch);
  let stored: Record<string, unknown> = {};
  try {
    stored = JSON.parse(localStorage.getItem(key) ?? "{}") ?? {};
  } catch {
    stored = {};
  }
  for (const name of Object.keys(patch))
    stored[name] = next[name as keyof AppSettings];
  localStorage.setItem(key, JSON.stringify(stored));
  window.dispatchEvent(new CustomEvent(eventName, { detail: next }));
  return next;
}

/** Nasłuch zmian ustawień (zdarzenie Tauri albo zdarzenie okna w podglądzie). */
export async function onSettingsChanged(
  handler: (settings: AppSettings) => void,
): Promise<() => void> {
  if (desktop)
    return listen<AppSettings>(eventName, (e) => handler(e.payload));
  const listener = (e: Event) =>
    handler((e as CustomEvent<AppSettings>).detail);
  window.addEventListener(eventName, listener);
  return () => window.removeEventListener(eventName, listener);
}

export async function readAppInfo(): Promise<AppInfo> {
  if (desktop) return invoke("app_info");
  return { version: "", shortcutError: "" };
}
