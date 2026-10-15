import type { NavigateRequest } from "./store";

/**
 * Przejścia między widokami z wnętrza widoków (linki „Ustawienia → …”,
 * komunikaty błędów). Główne okno rejestruje obsługę tak samo jak zdarzenie
 * `navigate` z powiadomień; okno szybkiego przełączania przekazuje przejście
 * głównemu oknu (`navigateMain`). Bez rejestracji nic się nie dzieje.
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
      // Etykiety pól dokładnie jak na ekranie (SettingsPanel).
      "Zapytaj o czas po co najmniej 10 minutach bezczynności",
      "Praca agentów AI (Claude Code, Codex) liczy się jako aktywność",
      "Proponuj przełączenie na zadanie, nad którym pracuje agent AI",
      "Proponuj znowu",
      "Cel dzienny (minuty)",
      "Uwzględniaj święta w celu",
      "szacunki",
      "poprawka szacunków",
      "Planuj z poprawką szacunków",
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
      // Etykiety pól (CalendarSettings, SettingsPanel).
      "Identyfikator aplikacji (client ID)",
      "Pokazuj tytuły spotkań",
      "Entra ID",
      "Odśwież",
      "Rozłącz",
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
      "tablice",
      "Dodawaj nowe zgłoszenia przypisane do mnie",
      "Połączenie z Jirą",
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
      "Przypominaj o terminach na dziś i przekroczonych",
      "Długi pomiar bez Stop (minuty, 0 wyłącza)",
      "Wieczorne „pomiar nadal trwa”",
      "Przypominaj wieczorem, że pomiar nadal trwa",
      "Powiadom o osiągnięciu celu dnia",
      "przegląd dnia",
      "poranny przegląd",
      "Przypominaj o porannym przeglądzie dnia",
    ],
  },
  {
    id: "focus",
    name: "Skupienie",
    labels: [
      "blok skupienia",
      "przerwa",
      "pomodoro",
      "Przerwa zatrzymuje pomiar",
      "Blok skupienia (minuty)",
      "Przerwa (minuty)",
      "Blok 25 min",
      "Blok 50 min",
      "Przerwa 5 min",
      "Przerwa 10 min",
    ],
  },
  {
    id: "templates",
    name: "Szablony",
    labels: [
      "szablon",
      "szablony zadań",
      "Nowy szablon",
      "/review",
      "nazwa",
      "tytuł",
      "szacunek",
      "projekt",
      "priorytet",
      "kroki",
    ],
  },
  {
    id: "windows",
    name: "Okna",
    labels: [
      "mini-widget",
      "widget",
      "zawsze na wierzchu",
      "główne okno",
      "Pokazuj mini-widget zawsze na wierzchu",
      "Tylko gdy główne okno jest schowane lub bez fokusu",
    ],
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
      "Ctrl+K",
      "Szybkie przełączanie (globalnie)",
      "Start/Stop (globalnie)",
    ],
  },
  {
    id: "startup",
    name: "Uruchamianie",
    labels: [
      "autostart",
      "Uruchamiaj z Windows",
      "Uruchamiaj schowane do traya",
      "tray",
      "wersja",
    ],
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

/** Słowa tekstu po `fold`, bez cudzysłowów i znaków przestankowych na brzegach. */
function words(text: string) {
  return fold(text)
    .split(/\s+/)
    .map((w) => w.replace(/^[^\p{L}\p{N}/+]+|[^\p{L}\p{N}/+]+$/gu, ""))
    .filter(Boolean);
}

/**
 * Sekcje, których nazwa albo etykiety ustawień zawierają wpisany tekst:
 * każde słowo zapytania musi wystąpić w nazwie lub etykietach sekcji, więc
 * wklejona etykieta pola („Pokazuj tytuły spotkań”) też znajduje sekcję.
 */
export function matchingSections(query: string) {
  const q = words(query);
  if (!q.length) return [...settingsSections];
  return settingsSections.filter((s) => {
    const text = [s.name, ...s.labels].map(fold).join("\n");
    return q.every((w) => text.includes(w));
  });
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
