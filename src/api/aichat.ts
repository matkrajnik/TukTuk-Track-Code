/**
 * Czat w aplikacji (PLAN_BACKLOG.md, A3): typy zdarzeń `ai-chat-event`, teksty
 * i funkcje czyste wspólne dla aplikacji i podglądu (symulator w
 * `preview/ai.ts`). Model tylko proponuje: propozycje z serwera MCP są kartami
 * „Zastosuj” / „Odrzuć” / „Popraw”, a zapis robią istniejące komendy.
 */
import type { McpProposal, McpProposalKind } from "./mcp";

/** Zdarzenie jednej tury (`ai_chat::Event`). */
export type ChatEvent =
  | { kind: "text"; text: string }
  | { kind: "tool"; name: string; input: Record<string, unknown> }
  | { kind: "toolDone"; error: boolean }
  | { kind: "done" }
  | { kind: "error"; message: string }
  | { kind: "stopped" };

/** Zdarzenie `ai-chat-event` z numerem tury (`ai_chat::Payload`). */
export type ChatPayload = { turn: number } & ChatEvent;

/** Wynik `ai_chat_send`: numer tury i tekst po filtrze danych osobowych (`ai_chat::Sent`). */
export interface ChatSent {
  turn: number;
  sent: string;
}

/** Miejsce, z którego otwarto czat: zadanie z kafelka albo dzień z Zamknięcia dnia. */
export type ChatContext =
  | { kind: "task"; id: number; title: string; key?: string }
  | { kind: "day"; day: string };

/** Prośba o otwarcie czatu z innego miejsca (Ctrl+K, kafelek, Zamknięcie dnia). */
export interface ChatRequest {
  question?: string;
  task?: { id: number; title: string; key?: string };
  day?: string;
  /** Wyślij `question` od razu (Ctrl+K po „?”). */
  send?: boolean;
}

/** Komunikaty czatu po stronie interfejsu i symulatora. */
export const chatMessages = {
  off: "Czat jest wyłączony. Włącz „Czat” w Ustawieniach → Asystent AI.",
  mcp: "Czat czyta dane przez serwer MCP. Włącz „Serwer MCP dla agentów” w Ustawieniach → Asystent AI.",
  busy: "Asystent jeszcze odpowiada. Poczekaj albo kliknij „Zatrzymaj”.",
  empty: "Wpisz pytanie do asystenta.",
} as const;

/** Wiersz kontekstu dopisywany do wiadomości (idzie do modelu po filtrze nazwisk). */
export function contextText(context: ChatContext | null): string {
  if (!context) return "";
  if (context.kind === "day") return `Kontekst: dzień ${context.day}.`;
  const key = context.key ? `, ${context.key}` : "";
  return `Kontekst: zadanie „${context.title}” (id ${context.id}${key}).`;
}

/** Krótka etykieta kontekstu do chipa nad polem wiadomości. */
export function contextLabel(context: ChatContext): string {
  return context.kind === "day"
    ? `Dzień ${context.day}`
    : `Zadanie: ${context.title}`;
}

/** Pierwszy tekst argumentu narzędzia (id, klucz, dzień). */
function arg(input: Record<string, unknown>, ...names: string[]): string {
  for (const name of names) {
    const value = input?.[name];
    if (typeof value === "string" || typeof value === "number")
      return String(value);
  }
  return "";
}

/**
 * Wiersz „użyte narzędzia” pod odpowiedzią: „przeczytał: dzień 2026-10-20”
 * dla odczytów, „zaproponował: …” dla propozycji.
 */
export function toolLabel(
  name: string,
  input: Record<string, unknown>,
): string {
  const day = arg(input, "day");
  const task = arg(input, "id", "key", "task");
  switch (name) {
    case "tasks_list": {
      const filter = arg(input, "filter") || "open";
      const names: Record<string, string> = {
        today: "zadania na dziś",
        open: "otwarte zadania",
        blocked: "zablokowane zadania",
        project: `zadania projektu ${arg(input, "query")}`.trim(),
        search: `zadania „${arg(input, "query")}”`,
      };
      return `przeczytał: ${names[filter] ?? "zadania"}`;
    }
    case "task_get":
      return `przeczytał: zadanie ${task}`.trim();
    case "day_summary":
      return `przeczytał: dzień ${day || "dzisiejszy"}`;
    case "week_summary":
      return `przeczytał: tydzień ${arg(input, "week") || "bieżący"}`;
    case "current_measurement":
      return "przeczytał: bieżący pomiar";
    case "ts_tasks":
      return "przeczytał: zadania TS";
    case "propose_task_create":
      return `zaproponował: zadanie „${arg(input, "title")}”`;
    case "propose_note_append":
      return `zaproponował: notatkę do zadania ${task}`.trim();
    case "propose_link_folder":
      return `zaproponował: folder do zadania ${task}`.trim();
    case "propose_switch":
      return `zaproponował: przełączenie na zadanie ${task}`.trim();
    case "propose_plan":
      return `zaproponował: plan zadania ${task}`.trim();
    default:
      return `użył: ${name}`;
  }
}

/** Nagłówek karty akcji. */
export function cardTitle(kind: McpProposalKind): string {
  switch (kind) {
    case "task_create":
      return "Utwórz zadanie";
    case "note_append":
      return "Dopisz notatkę";
    case "link_folder":
      return "Przypisz folder";
    case "switch":
      return "Przełącz pomiar";
    case "plan":
      return "Zaplanuj zadanie";
  }
}

/** `\` przed znakiem specjalnym składni szybkiego dodawania na początku słowa. */
const escapeWord = (word: string) => word.replace(/^([\\#@~!/])/, "\\$1");

/** Dzień `YYYY-MM-DD` jako `@dd.mm` składni szybkiego dodawania. */
function quickDay(day: string): string {
  const [, month, date] = /^\d{4}-(\d{2})-(\d{2})$/.exec(day) ?? [];
  return month ? `@${date}.${month}` : "";
}

/** Szacunek w minutach jako `~1h30` albo `~45m`. */
function quickEstimate(minutes: number): string {
  if (minutes <= 0) return "";
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return h ? `~${h}h${m ? String(m).padStart(2, "0") : ""}` : `~${m}m`;
}

/**
 * „Popraw” dla propozycji `task_create`: tekst pola „Nowe zadanie” w składni
 * szybkiego dodawania (klucz, tytuł, szacunek, dzień, projekt).
 */
export function quickText(p: McpProposal): string {
  const key = p.key.trim();
  const title = p.title.trim().split(/\s+/).map(escapeWord).join(" ");
  const parts = [
    /^#\d+$/.test(key) || /^[A-Za-z][A-Za-z0-9]+-\d+$/.test(key) ? key : "",
    title,
    quickEstimate(p.estimate),
    quickDay(p.day),
    p.project ? `#${p.project}` : "",
  ];
  return parts.filter(Boolean).join(" ");
}
