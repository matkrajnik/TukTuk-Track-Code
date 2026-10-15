import type { NavigateRequest } from "./store";

/**
 * Przejścia między widokami z wnętrza widoków (linki „Ustawienia → …”,
 * komunikaty błędów). Główne okno rejestruje obsługę tak samo jak zdarzenie
 * `navigate` z powiadomień; bez rejestracji (inne okna) nic się nie dzieje.
 */
let handler: ((request: NavigateRequest) => void) | null = null;

export function setNavigator(next: ((request: NavigateRequest) => void) | null) {
  handler = next;
}

export function openView(request: NavigateRequest) {
  handler?.(request);
}

/** Sekcje zakładki Ustawienia, w kolejności listy. */
export const settingsSections = [
  {
    id: "work",
    name: "Czas pracy",
    labels: [
      "bezczynność",
      "przerwa",
      "agenci AI",
      "Claude Code",
      "Codex",
      "propozycje przełączenia",
      "foldery bez propozycji",
      "cel dzienny",
      "cel tygodnia",
      "początek dnia",
      "dni robocze",
      "święta",
    ],
  },
  {
    id: "calendar",
    name: "Kalendarz",
    labels: [
      "Microsoft",
      "Outlook",
      "Microsoft 365",
      "spotkania",
      "client ID",
      "identyfikator aplikacji",
      "dzierżawca",
      "połącz konto Microsoft",
      "tytuły spotkań",
    ],
  },
  {
    id: "jira",
    name: "Jira",
    labels: [
      "zgłoszenia",
      "przypisane do mnie",
      "import",
      "konto Jiry",
      "token API",
    ],
  },
  {
    id: "notifications",
    name: "Powiadomienia",
    labels: [
      "terminy",
      "przypomnienia",
      "długi pomiar",
      "wieczorne",
      "pomiar nadal trwa",
      "osiągnięcie celu dnia",
    ],
  },
  {
    id: "focus",
    name: "Skupienie",
    labels: ["blok skupienia", "przerwa", "pomodoro", "przerwa zatrzymuje pomiar"],
  },
  {
    id: "templates",
    name: "Szablony",
    labels: ["szablon", "szablony zadań", "nowy szablon", "/review"],
  },
  {
    id: "windows",
    name: "Okna",
    labels: ["mini-widget", "widget", "zawsze na wierzchu", "główne okno"],
  },
  {
    id: "shortcuts",
    name: "Skróty",
    labels: [
      "skróty klawiszowe",
      "klawisze",
      "szybkie przełączanie",
      "Start/Stop",
      "globalnie",
      "Ctrl",
    ],
  },
  {
    id: "startup",
    name: "Uruchamianie",
    labels: ["autostart", "uruchamiaj z Windows", "schowane do traya", "tray", "wersja"],
  },
] as const;

export type SettingsSection = (typeof settingsSections)[number]["id"];

export function isSettingsSection(value: unknown): value is SettingsSection {
  return settingsSections.some((s) => s.id === value);
}

/** Małe litery bez polskich znaków, żeby „skrot” znalazło „Skróty”. */
function fold(text: string) {
  return text
    .toLocaleLowerCase("pl")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/ł/g, "l")
    .trim();
}

/** Sekcje, których nazwa albo etykiety ustawień zawierają wpisany tekst. */
export function matchingSections(query: string) {
  const q = fold(query);
  if (!q) return [...settingsSections];
  return settingsSections.filter((s) =>
    [s.name, ...s.labels].some((label) => fold(label).includes(q)),
  );
}

const SECTION_KEY = "todo-track-settings-section";

/** Ostatnio otwarta sekcja Ustawień (pamięć przeglądarki, wygoda widoku). */
export function readSettingsSection(): SettingsSection {
  try {
    const saved = localStorage.getItem(SECTION_KEY);
    if (isSettingsSection(saved)) return saved;
  } catch {}
  return "work";
}

export function saveSettingsSection(section: SettingsSection) {
  try {
    localStorage.setItem(SECTION_KEY, section);
  } catch {}
}

/** Zakładka Ustawienia na wybranej sekcji (bez sekcji: ostatnio otwarta). */
export function openSettings(section?: SettingsSection) {
  openView({ view: "settings", section: section ?? null });
}

/**
 * Sekcja wskazana w komunikacie „… w Ustawieniach → Kalendarz.”, żeby przy
 * błędzie pokazać przycisk prowadzący prosto do niej.
 */
export function sectionInMessage(message: string): SettingsSection | null {
  const match = /Ustawienia(?:ch)?\s*→\s*([\p{L} ]+)/u.exec(message);
  if (!match) return null;
  const name = fold(match[1]);
  return (
    settingsSections.find((s) => name.startsWith(fold(s.name)))?.id ?? null
  );
}
