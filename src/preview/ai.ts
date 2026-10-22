/**
 * Symulowany silnik AI w podglądzie (`ai.rs`, A1): stan w localStorage, bez procesu.
 *
 * Klucz `todo-track-preview-ai-v1`: `{ claude?, codex?, script?, chat?, calls? }`.
 * - `claude` / `codex`: `{ path, version }` albo `null`; domyślnie żadne CLI nie
 *   jest wykryte (podgląd nie pokazuje dialogu pierwszego uruchomienia, dopóki
 *   test lub deweloper nie ustawi wykrytego CLI),
 * - `script`: odpowiedzi ze skryptu według rodzaju wywołania
 *   (`{ test: { error: "login" } }`); bez wpisu wywołanie kończy się sukcesem.
 *   Asystent przypisań (A4): `assign: { value: { taskId, confidence, reason } }` dla
 *   sesji agentów, `ts_task: { value, values: { "<id zadania>": … } }` dla zadań TS
 *   (`values` ma pierwszeństwo przed `value`); `delayMs` we wpisie wydłuża wywołanie
 *   (dwa wywołania jednego rodzaju naraz dają błąd zajętości jak `ai::acquire`),
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
import { read } from "./snapshot";
import {
  MIN_CONFIDENCE,
  OPEN_MAX,
  TEXT_MAX,
  type TsOption,
  type TsSuggestion,
} from "../api/aiassign";
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

/** Wpis skryptu dla rodzaju wywołania. */
function scriptEntry(kind: string) {
  const script = stored().script;
  return script && typeof script === "object"
    ? ((script as Record<string, unknown>)[kind] as
        | { value?: unknown; error?: string; delayMs?: unknown }
        | undefined)
    : undefined;
}
/** Odpowiedź ze skryptu albo błąd po polsku (jak `ai::failure_message`). */
function scripted(kind: string, chosen: string, key = ""): unknown {
  const entry = scriptEntry(kind);
  const keyed = (entry as { values?: Record<string, unknown> } | undefined)?.values?.[key];
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
      return keyed ?? entry?.value ?? { ok: true };
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
  if (message === aiMessages.busy) return "busy";
  if (message.includes("nie jest zalogowany")) return "login";
  if (message.includes("Limit subskrypcji")) return "limit";
  return "error";
}

/** Rodzaje wywołań w toku (symulacja `ai::acquire`). */
const running = new Set<string>();
/**
 * Wywołanie jednorazowe w podglądzie (`ai::ask`): odpowiedź ze skryptu. Drugie
 * wywołanie tego samego rodzaju w trakcie pierwszego to błąd zajętości.
 */
export async function aiAsk(
  kind: string,
  prompt: string,
  /** Klucz odpowiedzi skryptu (`values`), np. id zadania. */
  key = "",
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
  // Jedno wywołanie danego rodzaju naraz, jak `ai::acquire`.
  if (running.has(kind)) {
    await record(kind, prompt, "busy");
    throw new Error(aiMessages.busy);
  }
  running.add(kind);
  try {
    const delay = scriptEntry(kind)?.delayMs;
    if (typeof delay === "number" && delay > 0)
      await new Promise((resolve) => setTimeout(resolve, delay));
    const value = scripted(kind, chosen, key);
    await record(kind, prompt, "");
    return value;
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    await record(kind, prompt, errorCode(message));
    throw e;
  } finally {
    running.delete(kind);
  }
}

/** Kolejki wywołań jednego rodzaju (jak muteksy w `assign::State`). */
const turns = new Map<string, Promise<unknown>>();
/** Uruchamia `f` po zakończeniu poprzedniego wywołania w kolejce `name`. */
function inTurn<T>(name: string, f: () => Promise<T>): Promise<T> {
  const run = (turns.get(name) ?? Promise.resolve()).then(f);
  turns.set(name, run.catch(() => undefined));
  return run;
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

// ---------------------------------------------------------------------------
// Asystent przypisań (A4): symulowane `assign` i `ts_task` ze skryptu.

const clip = (text: string, max: number) => [...text.trim()].slice(0, max).join("");
const confident = (value: unknown): number | null => {
  const c = (value as { confidence?: unknown } | null)?.confidence;
  return typeof c === "number" && c >= MIN_CONFIDENCE && c <= 1 ? c : null;
};

/** Otwarte zadanie dla modelu (jak `assign::OpenTask`). */
export interface AssignTask {
  id: number;
  title: string;
  project: string;
  key: string;
}

/** Sesja agenta w pytaniu o przypisanie. */
export interface AssignSession {
  folder: string;
  branch: string | null;
  prompt: string | null;
}

/** Otwarte zadania podglądu: nieukończone i niezarchiwizowane, najnowsze pierwsze. */
export async function openAssignTasks(): Promise<AssignTask[]> {
  const data = await read();
  return [...data.tasks]
    .filter((t) => !t.done && !t.archived)
    .sort((a, b) => b.id - a.id)
    .slice(0, OPEN_MAX)
    .map((t) => ({
      id: t.id,
      title: t.title,
      project: t.details?.project ?? "",
      key: t.jiraKey ?? (t.redmineId ? `#${t.redmineId}` : ""),
    }));
}

/** Pamięć po sesji: jedno wywołanie na sesję (także nieudane), jak `assign::Memory`. */
const assigned = new Map<string, Promise<number | null>>();

/**
 * `ai::ask("assign", …)` dla sesji bez dopasowania: `taskId` z listy przy pewności
 * co najmniej 0,6, inaczej `null`. Błąd wywołania to też `null`, bez ponawiania.
 */
export function aiAssign(
  sessionKey: string,
  session: AssignSession,
  tasks: AssignTask[],
): Promise<number | null> {
  const known = assigned.get(sessionKey);
  if (known) return known;
  const run = (async () => {
    if (!tasks.length) return null;
    const prompt = `Dane:
${JSON.stringify({
      session: { ...session, prompt: clip(session.prompt ?? "", TEXT_MAX) },
      openTasks: tasks,
    })}`;
    try {
      // Sesje czekają w kolejce (`assign::State::worker`).
      const value = (await inTurn("assign", () => aiAsk("assign", prompt))) as {
        taskId?: unknown;
      } | null;
      const id = value?.taskId;
      return typeof id === "number" &&
        confident(value) !== null &&
        tasks.some((t) => t.id === id)
        ? id
        : null;
    } catch {
      return null;
    }
  })();
  assigned.set(sessionKey, run);
  return run;
}

/** Treść pytania o zadanie TS (`assign::ts_request` + `ts_text`) albo błąd jak w aplikacji. */
async function tsPrompt(taskId: number, day: string, options: TsOption[]): Promise<string> {
  if (settings().aiAssignSuggest === false)
    throw new Error("Podpowiedzi przypisań są wyłączone w Ustawieniach → Asystent AI.");
  if (!options.length)
    throw new Error("Brak listy zadań TS: połącz konto TS i odśwież listę.");
  const task = (await read()).tasks.find((t) => t.id === taskId);
  if (!task) throw new Error("Nie znaleziono zadania.");
  return `Dane:
${JSON.stringify({
    task: {
      title: task.title,
      project: task.details?.project ?? "",
      key: task.jiraKey ?? (task.redmineId ? `#${task.redmineId}` : ""),
      dayNote: clip(task.details?.notes?.[day] ?? "", TEXT_MAX),
    },
    tsTasks: options,
  })}`;
}

/** `ai_suggest_ts_preview`: treść pytania o zadanie TS po filtrze nazw; nic nie wysyła. */
export async function aiSuggestTsPreview(
  taskId: number,
  day: string,
  options: TsOption[],
): Promise<string> {
  return aiRedactPreview(await tsPrompt(taskId, day, options));
}

/** `ai_suggest_ts_task` w podglądzie: pytanie o zadanie TS z listy `options`. */
export function aiSuggestTsTask(
  taskId: number,
  day: string,
  options: TsOption[],
): Promise<TsSuggestion> {
  // Pytania z kilku pól czekają w kolejce (`assign::State::ts_turn`).
  return inTurn("ts_task", () => suggestTs(taskId, day, options));
}
async function suggestTs(
  taskId: number,
  day: string,
  options: TsOption[],
): Promise<TsSuggestion> {
  const prompt = await tsPrompt(taskId, day, options);
  const value = (await aiAsk("ts_task", prompt, String(taskId))) as {
    tsTaskId?: unknown;
    confidence?: unknown;
    reason?: unknown;
  } | null;
  const reason = clip(typeof value?.reason === "string" ? value.reason : "", 300);
  const confidence = typeof value?.confidence === "number" ? value.confidence : 0;
  const none = (why: string): TsSuggestion => ({
    tsTaskId: null,
    confidence,
    reason: reason ? `${why} ${reason}` : why,
  });
  const id = value?.tsTaskId;
  if (typeof id !== "string" || confident(value) === null)
    return none("Asystent nie znalazł pewnego dopasowania.");
  if (!options.some((o) => o.id === id))
    return none("Asystent wskazał zadanie spoza listy TS.");
  return { tsTaskId: id, confidence, reason };
}
