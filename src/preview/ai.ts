/**
 * Symulowany silnik AI w podglądzie (`ai.rs`, A1): stan w localStorage, bez procesu.
 *
 * Klucz `todo-track-preview-ai-v1`: `{ claude?, codex?, script?, chat?, calls? }`.
 * - `claude` / `codex`: `{ path, version }` albo `null`; domyślnie żadne CLI nie
 *   jest wykryte (podgląd nie pokazuje dialogu pierwszego uruchomienia, dopóki
 *   test lub deweloper nie ustawi wykrytego CLI),
 * - `script`: odpowiedzi ze skryptu według rodzaju wywołania
 *   (`{ test: { error: "login" } }`); bez wpisu wywołanie kończy się sukcesem,
 * - `chat.replies`: skryptowane odpowiedzi czatu (A3) `[{ match, text, tools, proposal }]`,
 *   sprawdzane przed wzorcami domyślnymi; odpowiedź idzie zdarzeniami okna jak `ai-chat-event`
 *   (`chat.stepMs` zwalnia odstęp między zdarzeniami),
 * - `calls`: dziennik wywołań (najnowsze na początku), jak `ai_calls`.
 * Filtr nazw jest uproszczony w stosunku do `ai_privacy` (słowo z listy
 * „Ukrywaj te nazwy” z krótką końcówką odmiany → „Osoba N”).
 */
import { previewJson, previewSettingsKey } from "./storage";
import { dayKey } from "../time";
import { mcpPropose } from "./mcp";
import type { McpProposal } from "../api/mcp";
import {
  chatMessages,
  type ChatEvent,
  type ChatPayload,
  type ChatSent,
} from "../api/aichat";
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

// ---------------------------------------------------------------------------
// Czat (A3): symulowany asystent ze skryptowanymi odpowiedziami i kartami.

/** Zdarzenie okna z odpowiedzią czatu (odpowiednik zdarzenia Rust `ai-chat-event`). */
export const aiChatEvent = "todo-track-ai-chat";
/** Odstęp między zdarzeniami symulowanej odpowiedzi (ms). */
const STEP_MS = 25;
/** `chat.stepMs` w stanie symulatora spowalnia odpowiedź (test „Zatrzymaj”). */
const stepMs = () =>
  Number((stored().chat as { stepMs?: number } | undefined)?.stepMs) || STEP_MS;

/** Jedna skryptowana odpowiedź (`chat.replies` w stanie symulatora). */
interface ChatReply {
  /** Wyrażenie regularne (bez wielkości liter) dla wiadomości po filtrze. */
  match: string;
  text?: string;
  tools?: { name: string; input: Record<string, unknown> }[];
  /** Propozycja dopisana do kolejki MCP jak po `propose_*`. */
  proposal?: Partial<McpProposal> & Pick<McpProposal, "kind">;
  error?: string;
}

let chatTurn = 0;
let chatRunning = false;
let chatTimer: ReturnType<typeof setTimeout> | undefined;

function emitChat(turn: number, event: ChatEvent) {
  window.dispatchEvent(
    new CustomEvent<ChatPayload>(aiChatEvent, { detail: { turn, ...event } }),
  );
}

/** Wiadomość bez wiersza kontekstu. */
const withoutContext = (text: string) =>
  text.replace(/^Kontekst:[^\n]*\n*/, "").trim();

/** Odpowiedź dla wiadomości po filtrze: najpierw skrypt, potem wzorce domyślne. */
function chatReply(sent: string): ChatReply {
  const script = (stored().chat as { replies?: ChatReply[] } | undefined)
    ?.replies;
  for (const reply of Array.isArray(script) ? script : []) {
    try {
      if (new RegExp(reply.match, "i").test(sent)) return reply;
    } catch {
      // Zły wzorzec w skrypcie testu jest pomijany.
    }
  }
  const task = /zadanie „(.+?)” \(id (\d+)/.exec(sent);
  const today = dayKey(Date.now());
  const question = withoutContext(sent);
  if (/utw[óo]rz|dodaj zadanie|nowe zadanie/i.test(question)) {
    const title =
      /„([^”]+)”/.exec(question)?.[1] ??
      /"([^"]+)"/.exec(question)?.[1] ??
      (question.includes(":")
        ? question.slice(question.indexOf(":") + 1).trim()
        : "") ??
      "";
    const name = title || "Nowe zadanie od asystenta";
    return {
      match: "",
      text: `Przygotowałem propozycję zadania „${name}”. Zatwierdź ją kartą poniżej, a ja niczego nie zapiszę bez Twojego kliknięcia.`,
      tools: [{ name: "propose_task_create", input: { title: name } }],
      proposal: { kind: "task_create", title: name, day: today },
    };
  }
  if (task && /dopisz|notatk/i.test(question)) {
    const id = Number(task[2]);
    return {
      match: "",
      text: "Przygotowałem propozycję notatki do tego zadania. Możesz ją zastosować, odrzucić albo poprawić.",
      tools: [
        {
          name: "propose_note_append",
          input: { task: id, text: "Podsumowanie od asystenta." },
        },
      ],
      proposal: {
        kind: "note_append",
        taskId: id,
        taskTitle: task[1],
        day: today,
        text: "Podsumowanie od asystenta.",
      },
    };
  }
  if (task) {
    return {
      match: "",
      text: `Zadanie „${task[1]}”: to odpowiedź symulatora podglądu. W aplikacji asystent czyta tu notatki, plan i czas zadania.`,
      tools: [{ name: "task_get", input: { id: Number(task[2]) } }],
    };
  }
  if (/dzi[sś]|dzie[ńn]|podsumuj|ile |tydzie[ńn]/i.test(question)) {
    return {
      match: "",
      text: "Dziś pracowałeś nad kilkoma zadaniami. To odpowiedź symulatora podglądu; w aplikacji asystent liczy ją z danych dnia.",
      tools: [{ name: "day_summary", input: { day: today } }],
    };
  }
  return {
    match: "",
    text: "Nie mam skryptowanej odpowiedzi na to pytanie (symulator podglądu).",
  };
}

/** `ai_chat_send` w podglądzie: filtr nazw, potem skryptowana odpowiedź zdarzeniami. */
export async function aiChatSend(text: string): Promise<ChatSent> {
  if (!text.trim()) throw new Error(chatMessages.empty);
  if (chatRunning) throw new Error(chatMessages.busy);
  const chosen = engine();
  if (chosen !== "claude" && chosen !== "codex") throw new Error(aiMessages.off);
  if (settings().aiChat === false) throw new Error(chatMessages.off);
  if (settings().aiMcp === false) throw new Error(chatMessages.mcp);
  const found = detected();
  if (!(chosen === "claude" ? found.claude : found.codex))
    throw new Error(
      found.claude || found.codex
        ? aiMessages.missing(chosen)
        : aiMessages.notFoundAny,
    );
  const sent = await aiRedactPreview(text.trim());
  const reply = chatReply(sent);
  const turn = ++chatTurn;
  chatRunning = true;
  const steps: (() => void)[] = [];
  for (const tool of reply.tools ?? []) {
    steps.push(() => emitChat(turn, { kind: "tool", ...tool }));
    steps.push(async () => {
      if (reply.proposal && tool.name.startsWith("propose_"))
        await mcpPropose(reply.proposal);
      emitChat(turn, { kind: "toolDone", error: false });
    });
  }
  if (reply.error) steps.push(() => emitChat(turn, { kind: "error", message: reply.error! }));
  else {
    if (reply.text) steps.push(() => emitChat(turn, { kind: "text", text: reply.text! }));
    steps.push(() => emitChat(turn, { kind: "done" }));
  }
  const run = (index: number) => {
    chatTimer = setTimeout(async () => {
      await steps[index]();
      if (index + 1 < steps.length) run(index + 1);
      else chatRunning = false;
    }, stepMs());
  };
  run(0);
  return { turn, sent };
}

/**
 * `ai_chat_stop` w podglądzie: przerywa odpowiedź; z `turn` tylko tę turę
 * (spóźniona wysyłka). Historii symulator nie prowadzi, więc `discard` nic nie zmienia.
 */
export async function aiChatStop(turn: number | null = null): Promise<void> {
  if (!chatRunning) return;
  if (turn !== null && turn !== chatTurn) return;
  clearTimeout(chatTimer);
  chatRunning = false;
  emitChat(chatTurn, { kind: "stopped" });
}

/** `ai_chat_reset` w podglądzie: kończy odpowiedź bez zdarzenia. */
export async function aiChatReset(): Promise<void> {
  clearTimeout(chatTimer);
  chatRunning = false;
}
