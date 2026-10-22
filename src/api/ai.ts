/**
 * Asystent AI (PLAN_BACKLOG.md, A1): typy komend `ai_*` i teksty wspólne dla
 * aplikacji i podglądu (symulator w `preview/ai.ts` odpowiada tymi samymi
 * komunikatami co `ai.rs`).
 */

/** Wykryte CLI (`ai::Cli`). */
export interface AiCli {
  path: string;
  version: string;
}

/** Wynik wykrywania: `null` = nie znaleziono działającego CLI (`ai::Detected`). */
export interface AiDetected {
  claude: AiCli | null;
  codex: AiCli | null;
}

/** Stan dla dialogu pierwszego uruchomienia i Ustawień (`ai::Status`). */
export interface AiStatus {
  detected: AiDetected;
  /** Ustawienie `aiEngine`: "" (jeszcze nie pytano), "claude", "codex", "off". */
  engine: string;
  /** Pokaż dialog pierwszego uruchomienia. */
  ask: boolean;
  /** Wybrany silnik jest wykryty (funkcje AI mogą działać). */
  active: boolean;
  /** Katalog roboczy wywołań AI. */
  dir: string;
}

/** Wpis „Ostatnich wywołań” bez treści (`ai::Call`). */
export interface AiCall {
  kind: string;
  /** Początek wywołania (ms). */
  at: number;
  /** Długość wysłanego tekstu po filtrze (znaki). */
  length: number;
  ok: boolean;
  /** Rodzaj błędu: login, limit, timeout, schema, busy, spawn, error, off, missing. */
  error: string;
  /** Wynik z pamięci podręcznej (bez procesu). */
  cached: boolean;
}

/** Silniki do wyboru (wartość ustawienia `aiEngine` i nazwa w interfejsie). */
export const aiEngines = [
  { id: "claude", name: "Claude Code" },
  { id: "codex", name: "Codex" },
] as const;

/** Nazwa silnika z ustawienia; "off" i "" to brak. */
export function aiEngineName(id: string): string {
  return aiEngines.find((e) => e.id === id)?.name ?? "";
}

/** Prompt próbnego wywołania z Ustawień (`ai::TEST_PROMPT`). */
export const aiTestPrompt =
  'Test połączenia z aplikacją TukTuk Track. Odpowiedz wyłącznie obiektem JSON {"ok": true}.';

/** Wiadomości błędów jak w `ai.rs` (`failure_message`, stałe modułu). */
export const aiMessages = {
  off: "Asystent AI jest wyłączony. Wybierz silnik w Ustawieniach → Asystent AI.",
  notFoundAny:
    "Nie znaleziono Claude Code ani Codexa. Zainstaluj jedno z nich albo podaj ścieżkę w Ustawieniach → Asystent AI.",
  timeout: "Asystent nie odpowiedział w ciągu 60 sekund.",
  schema: "Odpowiedź asystenta jest niezgodna ze schematem.",
  busy: "To wywołanie asystenta już trwa.",
  testOk: "Asystent odpowiedział poprawnie.",
  login(engine: string): string {
    const hint =
      engine === "codex"
        ? "uruchom `codex login` w terminalu"
        : "uruchom `claude` w terminalu";
    return `${aiEngineName(engine)} nie jest zalogowany. Zaloguj się: ${hint}.`;
  },
  limit(engine: string): string {
    return `Limit subskrypcji ${aiEngineName(engine)} jest wyczerpany. Spróbuj później.`;
  },
  failed(engine: string): string {
    return `${aiEngineName(engine)} zakończył się błędem.`;
  },
  missing(engine: string): string {
    return `Nie znaleziono ${aiEngineName(engine)}. Wybierz inny silnik albo podaj ścieżkę w Ustawieniach → Asystent AI.`;
  },
} as const;

/** Opis rodzaju błędu z dziennika wywołań dla Ustawień. */
export function aiErrorLabel(code: string): string {
  switch (code) {
    case "login":
      return "niezalogowany";
    case "limit":
      return "limit subskrypcji";
    case "timeout":
      return "przekroczony czas";
    case "schema":
      return "odpowiedź niezgodna ze schematem";
    case "busy":
      return "wywołanie już trwało";
    case "off":
      return "asystent wyłączony";
    case "missing":
      return "brak CLI";
    case "spawn":
      return "nie udało się uruchomić";
    default:
      return "błąd";
  }
}
