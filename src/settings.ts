import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop, previewWidgetSettingsChanged } from "./store";

/** Odpowiednik `AppSettings` z src-tauri/src/settings.rs (klucze camelCase). */
export interface AppSettings {
  idleEnabled: boolean;
  idleAgents: boolean;
  /** Proponuj przełączenie na zadanie, nad którym pracuje agent AI. */
  agentSuggest: boolean;
  /** Foldery (z podfolderami) z wyłączonymi propozycjami agentów. */
  agentMutedFolders: string[];
  /** Podpowiadaj zadanie na podstawie aktywnego okna (tytuły tylko w pamięci). */
  windowSuggest: boolean;
  /** Rozszerzenie przeglądarki Chrome i Edge (wpisy hosta w rejestrze, W4). */
  browserExtension: boolean;
  /** Hooki agentów (Claude Code, Codex): wpisy w ich plikach ustawień (B8). */
  agentHooks: boolean;
  targetDayMinutes: number;
  workDays: number[];
  /** Święta ustawowe nie są dniami roboczymi w celu. */
  holidaysInTarget: boolean;
  dayStart: string;
  widgetEnabled: boolean;
  widgetBackgroundOnly: boolean;
  autostartHidden: boolean;
  notifyDue: boolean;
  notifyLongMinutes: number;
  notifyEvening: string;
  notifyEveningEnabled: boolean;
  notifyTarget: boolean;
  jiraAutoImport: boolean;
  /** Główny przełącznik zapisów do Jiry (J1, J2); wyłączony blokuje też backend. */
  jiraWrite: boolean;
  /** Pytaj o przesunięcie zgłoszenia „Do zrobienia” przy starcie pracy. */
  jiraAskMove: boolean;
  /** Preferowany status docelowy przy kilku przejściach „w toku”. */
  jiraMoveStatus: string;
  /** Prefiks nazwy gałęzi w „Kopiuj nazwę gałęzi” (J2). */
  jiraBranchPrefix: string;
  /** Pola Jiry na kafelku zadania, karcie tygodnia i kartach widoku Jira (identyfikatory, w kolejności). */
  jiraCardFields: string[];
  shortcutQuick: string;
  shortcutToggle: string;
  /** Długość bloku skupienia w minutach (1–240). */
  focusBlockMinutes: number;
  /** Długość przerwy po bloku w minutach (1–60). */
  focusBreakMinutes: number;
  /** Przerwa zatrzymuje pomiar. */
  focusBreakStops: boolean;
  /** Tytuły spotkań z kalendarza; wyłączone = „Spotkanie”. */
  calendarShowTitles: boolean;
  /** Szacunki z poprawką z historii w pojemności, osi planu i „Rozłóż”. */
  planWithCorrection: boolean;
  /** Powiadomienie „Przegląd dnia” w dzień roboczy po początku dnia. */
  notifyMorningReview: boolean;
  /** Dzień ostatniego porannego przeglądu (RRRR-MM-DD, pusty = nigdy). */
  morningReviewDay: string;
  /** Chwila ostatniego „Rozpocznij dzień” (ms, 0 = nigdy). */
  morningReviewAt: number;
  /** Przypomnienie o blokadzie „Czekam na” po tylu dniach roboczych (0 wyłącza). */
  waitingNotifyDays: number;
  /** Statusy Jiry oznaczające czekanie (propozycje na kafelku). */
  waitingJiraStatuses: string[];
  /** Powiadomienie „Przegląd tygodnia” w ostatni dzień roboczy tygodnia. */
  notifyWeekReview: boolean;
  /** Godzina powiadomienia „Przegląd tygodnia” (GG:MM). */
  weekReviewTime: string;
  /** Sprawdzaj aktualizacje przy starcie i raz dziennie (`updater.ts`). */
  autoUpdateCheck: boolean;
  /**
   * Plan dnia (Rano) przy pierwszym uruchomieniu albo pokazaniu okna w dniu
   * roboczym bez zapisanego przeglądu (B11, `morningStart.ts`).
   */
  openMorningOnStart: boolean;
  /** Główny przełącznik wysyłki Raportu TS (Ideo Timesheet); wyłączony blokuje też backend. */
  tsSend: boolean;
  /** Statusy Redmine oznaczające czekanie (PLAN_1.9.md, R2; pole w Ustawieniach od R5). */
  waitingRedmineStatuses: string[];
  /** Co 5 min dodawaj nowe zgłoszenia przypisane w Redmine (PLAN_1.9.md, R3; przełącznik w Ustawieniach od R5). */
  redmineAutoImport: boolean;
  /** Główny przełącznik zapisów do Redmine (PLAN_1.9.md, R4); wyłączony blokuje też backend. */
  redmineWrite: boolean;
  /** Pytaj o przesunięcie zgłoszenia Redmine w statusie początkowym przy starcie pracy. */
  redmineAskMove: boolean;
  /** Status Redmine „w toku” — cel pytania przy starcie pracy. */
  redmineMoveStatus: string;
  /** Statusy początkowe Redmine („Do zrobienia”; Redmine nie ma kategorii statusów). */
  redmineNewStatuses: string[];
  /** Prefiks nazwy gałęzi w „Kopiuj nazwę gałęzi” dla zgłoszeń Redmine. */
  redmineBranchPrefix: string;
}
/** Błędy rejestracji skrótów globalnych; pusty tekst = skrót działa. */
export interface ShortcutErrors {
  quick: string;
  toggle: string;
}
export interface AppInfo {
  version: string;
  shortcutErrors: ShortcutErrors;
}
export const defaultSettings: Readonly<AppSettings> = Object.freeze({
  idleEnabled: true,
  idleAgents: true,
  agentSuggest: true,
  agentMutedFolders: [],
  windowSuggest: false,
  browserExtension: false,
  agentHooks: true,
  targetDayMinutes: 480,
  workDays: [1, 2, 3, 4, 5],
  holidaysInTarget: true,
  dayStart: "08:00",
  widgetEnabled: false,
  widgetBackgroundOnly: false,
  autostartHidden: false,
  notifyDue: true,
  notifyLongMinutes: 240,
  notifyEvening: "17:30",
  notifyEveningEnabled: true,
  notifyTarget: false,
  jiraAutoImport: true,
  jiraWrite: true,
  jiraAskMove: true,
  jiraMoveStatus: "In Progress",
  jiraBranchPrefix: "feature/",
  jiraCardFields: ["labels", "assignee"],
  shortcutQuick: "Control+Shift+Space",
  shortcutToggle: "Control+Alt+Space",
  focusBlockMinutes: 25,
  focusBreakMinutes: 5,
  focusBreakStops: true,
  calendarShowTitles: true,
  planWithCorrection: false,
  notifyMorningReview: true,
  morningReviewDay: "",
  morningReviewAt: 0,
  waitingNotifyDays: 3,
  waitingJiraStatuses: [
    "Code review",
    "Review",
    "Blocked",
    "Waiting for customer",
    "Oczekuje",
  ],
  notifyWeekReview: true,
  weekReviewTime: "15:00",
  autoUpdateCheck: true,
  openMorningOnStart: true,
  tsSend: true,
  waitingRedmineStatuses: ["Feedback"],
  redmineAutoImport: true,
  redmineWrite: true,
  redmineAskMove: true,
  redmineMoveStatus: "In Progress",
  redmineNewStatuses: ["New", "Nowy"],
  redmineBranchPrefix: "feature/",
});
const key = "todo-track-preview-settings-v1";
const eventName = "settings-changed";
const time = /^([01]\d|2[0-3]):[0-5]\d$/;

// Odpowiednik parsera z global-hotkey 0.8 (Shortcut::from_str w Rust): modyfikatory
// z aliasami i dokładnie jeden klawisz na końcu.
const modifiers: Record<string, string> = {
  OPTION: "Alt",
  ALT: "Alt",
  CONTROL: "Control",
  CTRL: "Control",
  COMMAND: "Super",
  CMD: "Super",
  SUPER: "Super",
  SHIFT: "Shift",
  COMMANDORCONTROL: "Control",
  COMMANDORCTRL: "Control",
  CMDORCTRL: "Control",
  CMDORCONTROL: "Control",
};
const keyAliases: Record<string, string> = {
  "`": "BACKQUOTE",
  "\\": "BACKSLASH",
  "[": "BRACKETLEFT",
  "]": "BRACKETRIGHT",
  PAUSEBREAK: "PAUSE",
  ",": "COMMA",
  "=": "EQUAL",
  "-": "MINUS",
  ".": "PERIOD",
  "'": "QUOTE",
  ";": "SEMICOLON",
  "/": "SLASH",
  DOWN: "ARROWDOWN",
  LEFT: "ARROWLEFT",
  RIGHT: "ARROWRIGHT",
  UP: "ARROWUP",
  NUMPLUS: "NUMPADADD",
  NUMPADPLUS: "NUMPADADD",
  ESC: "ESCAPE",
  VOLUMEDOWN: "AUDIOVOLUMEDOWN",
  VOLUMEUP: "AUDIOVOLUMEUP",
  VOLUMEMUTE: "AUDIOVOLUMEMUTE",
  MEDIATRACKPREV: "MEDIATRACKPREVIOUS",
};
const namedKeys = new Set(
  (
    "BACKQUOTE BACKSLASH BRACKETLEFT BRACKETRIGHT PAUSE COMMA EQUAL MINUS PERIOD " +
    "QUOTE SEMICOLON SLASH BACKSPACE CAPSLOCK ENTER SPACE TAB DELETE END HOME " +
    "INSERT PAGEDOWN PAGEUP PRINTSCREEN SCROLLLOCK ARROWDOWN ARROWLEFT ARROWRIGHT " +
    "ARROWUP NUMLOCK ESCAPE AUDIOVOLUMEDOWN AUDIOVOLUMEUP AUDIOVOLUMEMUTE " +
    "MEDIAPLAY MEDIAPAUSE MEDIAPLAYPAUSE MEDIASTOP MEDIATRACKNEXT MEDIATRACKPREVIOUS"
  ).split(" "),
);
const numpad =
  /^NUM(?:PAD)?([0-9]|ADD|DECIMAL|DIVIDE|ENTER|EQUAL|MULTIPLY|SUBTRACT)$/;
function parseKey(raw: string): string | null {
  const upper = raw.toUpperCase();
  const k = keyAliases[upper] ?? upper;
  if (/^[A-Z]$/.test(k)) return "KEY" + k;
  if (/^[0-9]$/.test(k)) return "DIGIT" + k;
  if (/^(KEY[A-Z]|DIGIT[0-9]|F([1-9]|1[0-9]|2[0-4]))$/.test(k)) return k;
  const num = numpad.exec(k);
  if (num) return "NUMPAD" + num[1];
  return namedKeys.has(k) ? k : null;
}
/** Postać kanoniczna skrótu (np. „Alt+Control+KEYK”) albo null, gdy Rust by go odrzucił. */
export function parseShortcut(value: string): string | null {
  if (!value || value.length > 64) return null;
  const tokens = value.split("+");
  if (tokens.length === 1) return parseKey(tokens[0]);
  const mods = new Set<string>();
  let key: string | null = null;
  for (const raw of tokens) {
    const token = raw.trim();
    if (!token || key) return null;
    const mod = modifiers[token.toUpperCase()];
    if (mod) mods.add(mod);
    else if (!(key = parseKey(token))) return null;
  }
  return key && [...[...mods].sort(), key].join("+");
}

export const altGrMessage =
  "Ta kombinacja to polska litera (AltGr) – wybierz inną.";
/**
 * Ctrl+Alt (z Shift lub bez, bez Win) z A, C, E, L, N, O, S, X, Z to w Windows AltGr,
 * czyli ą ć ę ł ń ó ś ź ż (altgr_letter w Rust).
 */
export function altGrLetter(value: string): boolean {
  const parsed = parseShortcut(value);
  return !!parsed && /^Alt\+Control\+(Shift\+)?KEY[ACELNOSXZ]$/.test(parsed);
}

/** Komunikat dla niepoprawnego prefiksu gałęzi (`BRANCH_PREFIX_ERROR` w Rust). */
export const branchPrefixError =
  "Prefiks gałęzi może mieć najwyżej 40 znaków: litery łacińskie, cyfry, „/”, „-”, „_” i „.”.";
/** Prefiks gałęzi jak `valid_branch_prefix` w Rust. */
export const validBranchPrefix = (value: string) =>
  value.length <= 40 &&
  /^[A-Za-z0-9/_.-]*$/.test(value) &&
  !value.includes("..") &&
  !/^[/.-]/.test(value);

/**
 * Lista statusów oznaczających czekanie (Jira albo Redmine) jak
 * `settings::clean_statuses`: przycięte, bez pustych i bez powtórek, najwyżej
 * 50 nazw po 80 znaków. `system` to dopełniacz w komunikacie („Jiry”, „Redmine”).
 */
function cleanStatuses(
  list: readonly unknown[],
  system: string,
  purpose = "oznaczających czekanie",
): string[] {
  const statuses: string[] = [];
  for (const raw of list) {
    const status = typeof raw === "string" ? raw.trim() : "";
    if ([...status].length > 80)
      throw new Error(`Status ${system} może mieć najwyżej 80 znaków.`);
    if (
      status &&
      !statuses.some((v) => v.toLowerCase() === status.toLowerCase())
    )
      statuses.push(status);
  }
  if (statuses.length > 50)
    throw new Error(`Za dużo statusów ${system} ${purpose}.`);
  return statuses;
}
const fresh = (): AppSettings => ({
  ...defaultSettings,
  workDays: [...defaultSettings.workDays],
  agentMutedFolders: [],
  waitingJiraStatuses: [...defaultSettings.waitingJiraStatuses],
  waitingRedmineStatuses: [...defaultSettings.waitingRedmineStatuses],
  redmineNewStatuses: [...defaultSettings.redmineNewStatuses],
  jiraCardFields: [...defaultSettings.jiraCardFields],
});
/** Najwięcej pól Jiry na kafelku (`settings::MAX_CARD_FIELDS`). */
export const maxCardFields = 6;
/** Komunikat dla niepoprawnej listy pól (`settings::CARD_FIELDS_ERROR`). */
export const cardFieldsError =
  "Pola Jiry na kafelku: najwyżej 6 identyfikatorów pól (np. labels, customfield_10020), bez summary i status.";
/** Identyfikator pola Jiry jak `jira_fields::valid_field_id`. */
export const validFieldId = (id: string) => /^[A-Za-z][A-Za-z0-9_]{0,59}$/.test(id);
/** Folder do porównań jak `agents::norm_folder`. */
export const normFolder = (folder: string) =>
  folder.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
const minutes = (v: unknown) =>
  typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 1440;

/** Dzień RRRR-MM-DD istniejący w kalendarzu (valid_day w Rust). */
function validDay(value: string): boolean {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (!m) return false;
  const d = new Date(Date.UTC(+m[1], +m[2] - 1, +m[3]));
  return (
    d.getUTCFullYear() === +m[1] &&
    d.getUTCMonth() === +m[2] - 1 &&
    d.getUTCDate() === +m[3]
  );
}

const range = (v: unknown, min: number, max: number) =>
  typeof v === "number" && Number.isInteger(v) && v >= min && v <= max;

/** Walidacja pól osobno, bez reguł łączących pola (validate_fields w Rust). */
function applyFields(
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
  if (!range(s.focusBlockMinutes, 1, 240))
    throw new Error("Blok skupienia musi mieć od 1 do 240 minut.");
  if (!range(s.focusBreakMinutes, 1, 60))
    throw new Error("Przerwa musi mieć od 1 do 60 minut.");
  if (s.workDays.some((d) => !Number.isInteger(d) || d < 1 || d > 7))
    throw new Error("Dni robocze muszą być liczbami od 1 do 7.");
  s.workDays = [...new Set(s.workDays)].sort((a, b) => a - b);
  // Jak w Rust: bez pustych i bez powtórek tego samego folderu.
  const folders: string[] = [];
  for (const raw of s.agentMutedFolders) {
    const folder = typeof raw === "string" ? raw.trim() : "";
    if (!folder || [...folder].length > 1024)
      throw new Error("Niepoprawny folder agenta.");
    if (!folders.some((f) => normFolder(f) === normFolder(folder)))
      folders.push(folder);
  }
  if (folders.length > 200)
    throw new Error("Za dużo wyciszonych folderów agentów.");
  s.agentMutedFolders = folders;
  if (!range(s.waitingNotifyDays, 0, 30))
    throw new Error(
      "Przypomnienie o blokadzie musi mieć od 0 do 30 dni roboczych.",
    );
  // Jak w Rust: bez pustych i bez powtórek (wielkość liter bez znaczenia).
  s.waitingJiraStatuses = cleanStatuses(s.waitingJiraStatuses, "Jiry");
  s.waitingRedmineStatuses = cleanStatuses(
    Array.isArray(s.waitingRedmineStatuses) ? s.waitingRedmineStatuses : [],
    "Redmine",
  );
  s.jiraMoveStatus = s.jiraMoveStatus.trim();
  if (!s.jiraMoveStatus || [...s.jiraMoveStatus].length > 80)
    throw new Error("Nazwa statusu w Jirze musi mieć od 1 do 80 znaków.");
  s.jiraBranchPrefix = s.jiraBranchPrefix.trim();
  if (!validBranchPrefix(s.jiraBranchPrefix)) throw new Error(branchPrefixError);
  // Redmine (R4): osobne pola, te same reguły co w Rust.
  s.redmineNewStatuses = cleanStatuses(
    Array.isArray(s.redmineNewStatuses) ? s.redmineNewStatuses : [],
    "Redmine",
    "początkowych",
  );
  s.redmineMoveStatus = s.redmineMoveStatus.trim();
  if (!s.redmineMoveStatus || [...s.redmineMoveStatus].length > 80)
    throw new Error("Nazwa statusu w Redmine musi mieć od 1 do 80 znaków.");
  s.redmineBranchPrefix = s.redmineBranchPrefix.trim();
  if (!validBranchPrefix(s.redmineBranchPrefix))
    throw new Error(branchPrefixError);
  // Jak w Rust: poprawne identyfikatory, bez powtórek, bez summary i status.
  const fields: string[] = [];
  for (const raw of s.jiraCardFields) {
    const id = typeof raw === "string" ? raw.trim() : "";
    if (!validFieldId(id) || id === "summary" || id === "status")
      throw new Error(cardFieldsError);
    if (!fields.includes(id)) fields.push(id);
  }
  if (fields.length > maxCardFields) throw new Error(cardFieldsError);
  s.jiraCardFields = fields;
  if (!time.test(s.dayStart))
    throw new Error("Początek dnia musi mieć format GG:MM.");
  if (s.morningReviewDay !== "" && !validDay(s.morningReviewDay))
    throw new Error("Dzień przeglądu musi mieć format RRRR-MM-DD.");
  if (!Number.isSafeInteger(s.morningReviewAt) || s.morningReviewAt < 0)
    throw new Error("Niepoprawna chwila przeglądu dnia.");
  if (!time.test(s.weekReviewTime))
    throw new Error("Godzina przeglądu tygodnia musi mieć format GG:MM.");
  if (!time.test(s.notifyEvening))
    throw new Error(
      "Godzina wieczornego przypomnienia musi mieć format GG:MM.",
    );
  if (
    [s.shortcutQuick, s.shortcutToggle].some((v) => parseShortcut(v) === null)
  )
    throw new Error("Niepoprawny skrót klawiszowy.");
  if ([s.shortcutQuick, s.shortcutToggle].some(altGrLetter))
    throw new Error(altGrMessage);
  return s;
}

/**
 * Walidacja zmiany w kliencie przed zapisem (B1): każde pole osobno na tle
 * wartości domyślnych, jak `applyFields` przy wczytywaniu. Puste pole liczbowe
 * (NaN) daje komunikat o zakresie pola zamiast ogólnego błędu z Rust
 * („Niepoprawna wartość ustawienia.”, bo NaN przechodzi przez JSON jako null).
 */
export function checkSettingsPatch(patch: Partial<AppSettings>): void {
  applyFields(fresh(), patch);
}

/** Ta sama walidacja co w Rust; zwraca nowe ustawienia albo rzuca błąd po polsku. */
export function applySettings(
  current: AppSettings,
  patch: Partial<AppSettings>,
): AppSettings {
  const s = applyFields(current, patch);
  if (parseShortcut(s.shortcutQuick) === parseShortcut(s.shortcutToggle))
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
  // Jak w Rust: najpierw każde pole osobno na tle domyślnych (uszkodzone zostaje
  // domyślne), potem raz reguły łączące pola; kolidujące skróty wracają do domyślnych.
  const settings = fresh();
  for (const [name, value] of Object.entries(stored)) {
    if (!(name in defaultSettings)) continue;
    try {
      const alone = applyFields(fresh(), { [name]: value });
      const field = name as keyof AppSettings;
      (settings as unknown as Record<string, unknown>)[field] = alone[field];
    } catch {
      /* wartość domyślna */
    }
  }
  if (
    parseShortcut(settings.shortcutQuick) ===
    parseShortcut(settings.shortcutToggle)
  ) {
    settings.shortcutQuick = defaultSettings.shortcutQuick;
    settings.shortcutToggle = defaultSettings.shortcutToggle;
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
  previewWidgetSettingsChanged(next.widgetEnabled);
  window.dispatchEvent(new CustomEvent(eventName, { detail: next }));
  return next;
}

/** Nasłuch zmian ustawień (zdarzenie Tauri albo zdarzenie okna w podglądzie). */
export async function onSettingsChanged(
  handler: (settings: AppSettings) => void,
): Promise<() => void> {
  if (desktop) return listen<AppSettings>(eventName, (e) => handler(e.payload));
  const listener = (e: Event) =>
    handler((e as CustomEvent<AppSettings>).detail);
  window.addEventListener(eventName, listener);
  return () => window.removeEventListener(eventName, listener);
}

export async function readAppInfo(): Promise<AppInfo> {
  if (desktop) return invoke("app_info");
  return { version: "", shortcutErrors: { quick: "", toggle: "" } };
}

/** Nasłuch wyniku ponownej rejestracji skrótów po zmianie ustawień (tylko aplikacja). */
export async function onShortcutsChanged(
  handler: (errors: ShortcutErrors) => void,
): Promise<() => void> {
  if (desktop)
    return listen<ShortcutErrors>("shortcuts-changed", (e) =>
      handler(e.payload),
    );
  return () => {};
}

const modifierCodes =
  /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$|^(ContextMenu|Fn|FnLock|CapsLock|NumLock|ScrollLock)$/;
/**
 * Zamienia naciśnięcie klawiszy na skrót w formacie pluginu („Control+Alt+Space”).
 * null: sam modyfikator, czekamy dalej. { message }: kombinacja nie nadaje się na skrót.
 */
export function shortcutFromEvent(
  e: Pick<
    KeyboardEvent,
    "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey"
  >,
): { value: string } | { message: string } | null {
  if (!e.code || modifierCodes.test(e.code)) return null;
  const mods = [
    e.ctrlKey && "Control",
    e.altKey && "Alt",
    e.shiftKey && "Shift",
    e.metaKey && "Super",
  ].filter((m): m is string => !!m);
  const key = /^Key[A-Z]$/.test(e.code)
    ? e.code.slice(3)
    : /^Digit[0-9]$/.test(e.code)
      ? e.code.slice(5)
      : e.code;
  const value = [...mods, key].join("+");
  if (!parseShortcut(value))
    return { message: "Tego klawisza nie można użyć w skrócie." };
  if (!mods.length)
    return { message: "Dodaj Ctrl, Alt, Shift lub Win do klawisza." };
  if (altGrLetter(value)) return { message: altGrMessage };
  return { value };
}
