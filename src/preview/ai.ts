/**
 * Symulowany silnik AI w podglądzie (`ai.rs`, A1): stan w localStorage, bez procesu.
 *
 * Klucz `todo-track-preview-ai-v1`: `{ claude?, codex?, script?, calls? }`.
 * - `claude` / `codex`: `{ path, version }` albo `null`; domyślnie żadne CLI nie
 *   jest wykryte (podgląd nie pokazuje dialogu pierwszego uruchomienia, dopóki
 *   test lub deweloper nie ustawi wykrytego CLI),
 * - `script`: odpowiedzi ze skryptu według rodzaju wywołania
 *   (`{ test: { error: "login" } }`); bez wpisu wywołanie kończy się sukcesem,
 * - `calls`: dziennik wywołań (najnowsze na początku), jak `ai_calls`.
 * Filtr nazw jest uproszczony w stosunku do `ai_privacy` (słowo z listy
 * „Ukrywaj te nazwy” z krótką końcówką odmiany → „Osoba N”).
 */
import { previewJson, previewSettingsKey } from "./storage";
import {
  aiMessages,
  aiTestPrompt,
  type AiCall,
  type AiCli,
  type AiDetected,
  type AiStatus,
} from "../api/ai";

export const aiKey = "todo-track-preview-ai-v1";
/** Katalog AI w podglądzie (jak `%APPDATA%\pl.todotrack.desktop\ai`). */
const previewDir = "%APPDATA%\\pl.todotrack.desktop\\ai";

function stored(): Record<string, unknown> {
  return previewJson(aiKey);
}
function save(patch: Record<string, unknown>) {
  try {
    localStorage.setItem(aiKey, JSON.stringify({ ...stored(), ...patch }));
  } catch {
    // Bez localStorage podgląd działa bez zapamiętanych zmian.
  }
}
const settings = () => previewJson(previewSettingsKey);
const engine = () =>
  typeof settings().aiEngine === "string" ? (settings().aiEngine as string) : "";

function cli(value: unknown): AiCli | null {
  if (!value || typeof value !== "object") return null;
  const { path, version } = value as Record<string, unknown>;
  return typeof path === "string" && typeof version === "string"
    ? { path, version }
    : null;
}
function detected(): AiDetected {
  const s = stored();
  return { claude: cli(s.claude), codex: cli(s.codex) };
}

/** `ai_status` w podglądzie. */
export async function aiStatus(): Promise<AiStatus> {
  const found = detected();
  const chosen = engine();
  return {
    detected: found,
    engine: chosen,
    ask: chosen === "" && !!(found.claude || found.codex),
    active:
      (chosen === "claude" && !!found.claude) ||
      (chosen === "codex" && !!found.codex),
    dir: previewDir,
  };
}

/** `ai_detect`: ponowne wykrycie (w podglądzie czyta stan z localStorage). */
export async function aiDetect(): Promise<AiStatus> {
  return aiStatus();
}

/** `ai_calls`: najnowsze wywołania bez treści. */
export async function aiCalls(): Promise<AiCall[]> {
  const calls = stored().calls;
  return Array.isArray(calls) ? (calls as AiCall[]) : [];
}

/** Słowa z „Ukrywaj te nazwy” i ich części (min. 3 znaki). */
function hiddenWords(): string[] {
  const names = settings().aiHiddenNames;
  const words = new Set<string>();
  for (const name of Array.isArray(names) ? names : []) {
    if (typeof name !== "string") continue;
    for (const word of name.split(/\s+/))
      if ([...word].length >= 3) words.add(word);
  }
  return [...words];
}

/** `ai_redact_preview`: tekst po uproszczonym filtrze nazw. */
export async function aiRedactPreview(text: string): Promise<string> {
  const numbers = new Map<string, number>();
  let out = text;
  for (const word of hiddenWords()) {
    const escaped = word.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    out = out.replace(
      new RegExp(`(?<![\\p{L}\\p{N}])${escaped}\\p{L}{0,3}(?![\\p{L}\\p{N}])`, "giu"),
      () => {
        const key = word.toLowerCase();
        if (!numbers.has(key)) numbers.set(key, numbers.size + 1);
        return `Osoba ${numbers.get(key)}`;
      },
    );
  }
  return out;
}

/** Odpowiedź ze skryptu albo błąd po polsku (jak `ai::failure_message`). */
function scripted(kind: string, chosen: string): unknown {
  const script = stored().script;
  const entry =
    script && typeof script === "object"
      ? ((script as Record<string, unknown>)[kind] as
          | { value?: unknown; error?: string }
          | undefined)
      : undefined;
  switch (entry?.error) {
    case "login":
      throw new Error(aiMessages.login(chosen));
    case "limit":
      throw new Error(aiMessages.limit(chosen));
    case "timeout":
      throw new Error(aiMessages.timeout);
    case "schema":
      throw new Error(aiMessages.schema);
    case "failed":
      throw new Error(aiMessages.failed(chosen));
    default:
      return entry?.value ?? { ok: true };
  }
}

/** Wpis dziennika (bez treści) na początku listy, najwyżej 30 wpisów. */
async function record(kind: string, prompt: string, error: string) {
  const length = [...(await aiRedactPreview(prompt))].length;
  const calls = await aiCalls();
  const call: AiCall = {
    kind,
    at: Date.now(),
    length,
    ok: error === "",
    error,
    cached: false,
  };
  save({ calls: [call, ...calls].slice(0, 30) });
}

/** Kod błędu z komunikatu symulatora (do dziennika). */
function errorCode(message: string): string {
  if (message === aiMessages.timeout) return "timeout";
  if (message === aiMessages.schema) return "schema";
  if (message.includes("nie jest zalogowany")) return "login";
  if (message.includes("Limit subskrypcji")) return "limit";
  return "error";
}

/** Wywołanie jednorazowe w podglądzie (`ai::ask`): odpowiedź ze skryptu. */
export async function aiAsk(
  kind: string,
  prompt: string,
): Promise<unknown> {
  const chosen = engine();
  if (chosen !== "claude" && chosen !== "codex") {
    await record(kind, prompt, "off");
    throw new Error(aiMessages.off);
  }
  const found = detected();
  if (!(chosen === "claude" ? found.claude : found.codex)) {
    await record(kind, prompt, "missing");
    throw new Error(
      found.claude || found.codex
        ? aiMessages.missing(chosen)
        : aiMessages.notFoundAny,
    );
  }
  try {
    const value = scripted(kind, chosen);
    await record(kind, prompt, "");
    return value;
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    await record(kind, prompt, errorCode(message));
    throw e;
  }
}

/** `ai_test` w podglądzie: próbne wywołanie z Ustawień. */
export async function aiTest(): Promise<string> {
  const value = (await aiAsk("test", aiTestPrompt)) as { ok?: unknown };
  if (value?.ok !== true) throw new Error(aiMessages.schema);
  return aiMessages.testOk;
}
