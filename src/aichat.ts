/**
 * Czat w aplikacji (PLAN_BACKLOG.md, A3): stan rozmowy w pamięci, komendy
 * `ai_chat_*` z odpowiednikami w podglądzie (`preview/ai.ts`) i obsługa
 * strumienia odpowiedzi. Historia żyje tylko do zamknięcia aplikacji albo
 * „Nowej rozmowy”. Model tylko proponuje: propozycje z serwera MCP trafiają do
 * rozmowy jako karty, a „Zastosuj” wykonuje istniejące komendy
 * (`aiproposals.ts`), tak jak baner propozycji agentów.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { computed, reactive, ref } from "vue";
import {
  contextLabel,
  contextText,
  toolLabel,
  type ChatContext,
  type ChatPayload,
  type ChatRequest,
  type ChatSent,
} from "./api/aichat";
import { mcpProposals, type McpProposal } from "./mcpapi";
import * as aiSim from "./preview/ai";
import { useSettings } from "./target";

export * from "./api/aichat";

const desktop = isTauri();

/** `ai_chat_send`: filtr nazwisk, start procesu i odpowiedź zdarzeniami; zwraca tekst po filtrze. */
export async function chatSend(text: string): Promise<ChatSent> {
  if (desktop) return invoke("ai_chat_send", { text });
  return aiSim.aiChatSend(text);
}
/**
 * `ai_chat_stop`: „Zatrzymaj”. `turn`: tylko ta tura (wysyłka, która wróciła
 * po zatrzymaniu), `discard`: usuń ją z historii (wysyłka sprzed „Nowej rozmowy”).
 */
export async function chatStop(
  turn: number | null = null,
  discard = false,
): Promise<void> {
  if (desktop) return invoke("ai_chat_stop", { turn, discard });
  return aiSim.aiChatStop(turn);
}
/** `ai_chat_reset`: „Nowa rozmowa”, kończy proces i kasuje historię. */
export async function chatReset(): Promise<void> {
  if (desktop) return invoke("ai_chat_reset");
  return aiSim.aiChatReset();
}
/** Nasłuch zdarzeń odpowiedzi (`ai-chat-event` albo zdarzenie okna w podglądzie). */
export async function onChatEvent(
  handler: (payload: ChatPayload) => void,
): Promise<() => void> {
  if (desktop) return listen<ChatPayload>("ai-chat-event", (e) => handler(e.payload));
  const listener = (e: Event) => handler((e as CustomEvent<ChatPayload>).detail);
  window.addEventListener(aiSim.aiChatEvent, listener);
  return () => window.removeEventListener(aiSim.aiChatEvent, listener);
}

/** Narzędzie użyte w odpowiedzi („przeczytał: dzień 20.10”). */
export interface ChatTool {
  name: string;
  label: string;
  done: boolean;
  error: boolean;
}
/** Stan karty akcji: `edited` = „Popraw” otworzyło zwykły formularz. */
export type CardState = "open" | "applying" | "applied" | "dismissed" | "edited";
/** Propozycja z MCP pokazana w rozmowie. */
export interface ChatCard {
  proposal: McpProposal;
  state: CardState;
  error: string;
  /**
   * „Popraw” przy notatce i planie: pola karty wypełnione propozycją; „Zastosuj”
   * zapisuje poprawione wartości, a propozycja czeka w kolejce do decyzji.
   */
  draft: { text: string; day: string } | null;
  /** Informacja po „Popraw” bez pól do poprawy (folder, przełączenie). */
  info: string;
}
export interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  text: string;
  /** Tekst wysłany do modelu po filtrze danych osobowych („Pokaż, co zostało wysłane”). */
  sent: string;
  /** Etykieta kontekstu wiadomości (zadanie, dzień). */
  context: string;
  tools: ChatTool[];
  cards: ChatCard[];
  error: string;
  stopped: boolean;
  pending: boolean;
}

const OPEN_KEY = "todo-track-chat-open";
const WIDTH_KEY = "todo-track-chat-width";
export const chatWidthRange = { min: 300, max: 640, initial: 380 } as const;

function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function persist(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Bez localStorage panel nie zapamięta ustawienia.
  }
}
export const clampWidth = (width: number) =>
  Math.min(chatWidthRange.max, Math.max(chatWidthRange.min, Math.round(width)));

const savedWidth = Number(stored(WIDTH_KEY));

/** Stan panelu i rozmowy (jedna rozmowa na aplikację). */
export const chat = reactive({
  open: stored(OPEN_KEY) === "1",
  width: Number.isFinite(savedWidth) && savedWidth > 0
    ? clampWidth(savedWidth)
    : chatWidthRange.initial,
  busy: false,
  messages: [] as ChatMessage[],
  context: null as ChatContext | null,
  draft: "",
  /** Rośnie przy każdej prośbie o fokus w polu wiadomości. */
  focus: 0,
});
/** Propozycje pokazane jako karty w rozmowie: baner agentów ich nie dubluje. */
export const claimedProposals = ref(new Set<number>());

/** Czat jest dostępny: wybrany silnik (nie „off” ani „jeszcze nie pytano”) i włączony `aiChat`. */
export function useChatEnabled() {
  const settings = useSettings();
  return computed(
    () =>
      (settings.value.aiEngine === "claude" ||
        settings.value.aiEngine === "codex") &&
      settings.value.aiChat,
  );
}

export function setChatOpen(open: boolean) {
  chat.open = open;
  persist(OPEN_KEY, open ? "1" : "0");
}
export function setChatWidth(width: number) {
  chat.width = clampWidth(width);
  persist(WIDTH_KEY, String(chat.width));
}

interface Turn {
  /** 0 do chwili, gdy `ai_chat_send` zwróci numer tury. */
  turn: number;
  assistant: ChatMessage;
  baseline: Set<number>;
  /** Zdarzenia, które przyszły przed numerem tury. */
  early: ChatPayload[];
  /** „Zatrzymaj” przed powrotem `ai_chat_send`. */
  stopped: boolean;
}
let current: Turn | null = null;
let sequence = 0;
/** Rośnie przy „Nowej rozmowie”: wysyłka sprzed niej jest po powrocie odwoływana. */
let conversation = 0;

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** Nowe propozycje z tej tury (po `propose_*`) jako karty w rozmowie. */
async function claim(turn: Turn) {
  if (!turn.assistant.tools.some((t) => t.name.startsWith("propose_"))) return;
  let list: McpProposal[] = [];
  try {
    list = await mcpProposals();
  } catch {
    return;
  }
  const fresh = list.filter(
    (p) => !turn.baseline.has(p.id) && !claimedProposals.value.has(p.id),
  );
  if (!fresh.length) return;
  claimedProposals.value = new Set([
    ...claimedProposals.value,
    ...fresh.map((p) => p.id),
  ]);
  for (const proposal of fresh)
    turn.assistant.cards.push({
      proposal,
      state: "open",
      error: "",
      draft: null,
      info: "",
    });
}

async function handle(turn: Turn, payload: ChatPayload) {
  const reply = turn.assistant;
  switch (payload.kind) {
    case "text":
      reply.text += (reply.text ? "\n\n" : "") + payload.text;
      break;
    case "tool":
      reply.tools.push({
        name: payload.name,
        label: toolLabel(payload.name, payload.input),
        done: false,
        error: false,
      });
      break;
    case "toolDone": {
      const tool = reply.tools.find((t) => !t.done);
      if (tool) {
        tool.done = true;
        tool.error = payload.error;
      }
      await claim(turn);
      break;
    }
    case "done":
      await claim(turn);
      finish(turn);
      break;
    case "error":
      reply.error = payload.message;
      await claim(turn);
      finish(turn);
      break;
    case "stopped":
      reply.stopped = true;
      finish(turn);
      break;
  }
}

function finish(turn: Turn) {
  turn.assistant.pending = false;
  // Tura z poprzedniej rozmowy nie odblokowuje pola nowej.
  if (current !== turn) return;
  chat.busy = false;
  current = null;
}

/** Strumień odpowiedzi do bieżącej tury; spóźnione zdarzenia starych tur są pomijane. */
export function onPayload(payload: ChatPayload) {
  const turn = current;
  if (!turn) return;
  if (turn.turn === 0) turn.early.push(payload);
  else if (payload.turn === turn.turn) void handle(turn, payload);
}

/** Zaczyna nasłuch zdarzeń czatu (okno główne); zwraca funkcję kończącą. */
export async function startChat(): Promise<() => void> {
  return onChatEvent(onPayload);
}

/** Wysyła wiadomość z kontekstem; odpowiedź przychodzi strumieniem. */
export async function sendMessage(text: string): Promise<void> {
  const body = text.trim();
  if (!body || chat.busy) return;
  const prefix = contextText(chat.context);
  const user = reactive<ChatMessage>({
    id: ++sequence,
    role: "user",
    text: body,
    sent: "",
    context: chat.context ? contextLabel(chat.context) : "",
    tools: [],
    cards: [],
    error: "",
    stopped: false,
    pending: false,
  });
  const assistant = reactive<ChatMessage>({
    id: ++sequence,
    role: "assistant",
    text: "",
    sent: "",
    context: "",
    tools: [],
    cards: [],
    error: "",
    stopped: false,
    pending: true,
  });
  chat.messages.push(user, assistant);
  chat.draft = "";
  chat.busy = true;
  // Tura jest bieżąca od razu, żeby „Zatrzymaj” i „Nowa rozmowa” w trakcie
  // oczekiwania na kolejkę i `ai_chat_send` ją widziały.
  const turn: Turn = {
    turn: 0,
    assistant,
    baseline: new Set(),
    early: [],
    stopped: false,
  };
  current = turn;
  const generation = conversation;
  const abandoned = () => generation !== conversation;
  try {
    turn.baseline = new Set((await mcpProposals()).map((p) => p.id));
  } catch {
    // Bez kolejki propozycji karty nie powstaną, rozmowa działa dalej.
  }
  if (turn.stopped || abandoned()) return;
  let sent: ChatSent;
  try {
    sent = await chatSend(prefix ? `${prefix}\n\n${body}` : body);
  } catch (e) {
    if (turn.stopped || abandoned()) return;
    assistant.error = message(e);
    finish(turn);
    return;
  }
  if (abandoned()) {
    // Wysyłka sprzed „Nowej rozmowy”: proces kończy się, a tura znika z historii.
    void chatStop(sent.turn, true).catch(() => {});
    return;
  }
  user.sent = sent.sent;
  if (turn.stopped) {
    // „Zatrzymaj” przed powrotem wysyłki: odpowiedź nie leci bez odbiorcy.
    void chatStop(sent.turn).catch(() => {});
    return;
  }
  turn.turn = sent.turn;
  for (const payload of turn.early.splice(0))
    if (payload.turn === sent.turn) await handle(turn, payload);
}

/** „Zatrzymaj”: kończy odpowiedź; bez aktywnej tury tylko odblokowuje pole. */
export async function stopChat(): Promise<void> {
  const turn = current;
  if (turn) turn.stopped = true;
  try {
    await chatStop(turn?.turn || null);
  } catch (e) {
    if (turn) turn.assistant.error = message(e);
  }
  if (turn && current === turn) {
    turn.assistant.stopped = true;
    finish(turn);
  }
}

/** „Nowa rozmowa”: kończy proces, kasuje historię i kontekst. Karty bez decyzji wracają do banera. */
export async function newConversation(): Promise<void> {
  // Wysyłka w toku należy do starej rozmowy: po powrocie zostanie odwołana.
  conversation++;
  if (current) current.stopped = true;
  current = null;
  chat.messages = [];
  chat.context = null;
  chat.draft = "";
  claimedProposals.value = new Set();
  // Do końca resetu nowa wiadomość nie wychodzi (reset ubiłby jej proces).
  chat.busy = true;
  try {
    await chatReset();
  } catch {
    // Rozmowa i tak jest czyszczona po stronie interfejsu.
  }
  chat.busy = false;
}

/** Otwiera panel z kontekstem z kafelka, Zamknięcia dnia albo pytaniem z Ctrl+K. */
export function openChat(request: ChatRequest = {}) {
  setChatOpen(true);
  if (request.task) chat.context = { kind: "task", ...request.task };
  else if (request.day) chat.context = { kind: "day", day: request.day };
  if (request.question) {
    if (request.send && !chat.busy) void sendMessage(request.question);
    else chat.draft = request.question;
  }
  chat.focus++;
}
