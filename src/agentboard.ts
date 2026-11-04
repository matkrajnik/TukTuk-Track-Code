/**
 * Tablica sesji agentów (`agent_board.rs`): sesje Claude Code i Codexa, które
 * czekają na użytkownika, pracują albo skończyły się i nie są przejrzane.
 * W aplikacji lista przychodzi z Rust (`agent_board`, zdarzenie
 * `agent-board-changed`); w podglądzie liczy ją ta sama maszyna stanów hooków
 * co kafelek „Agenci AI” (`preview/hooks.ts`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { jiraKeyIn, matchAgentTask, type Agent } from "./agents";
import {
  agentHooksKey,
  desktop,
  previewAgentsEvent,
  previewBindings,
  read,
} from "./store";
import {
  agentBoardSeenKey,
  previewHookBoard,
  previewMarkBoardSeen,
  type HookPhase,
} from "./preview/hooks";

export type BoardPhase = HookPhase;

/** Odpowiednik `agent_board::Entry`: agent i stan jego sesji. */
export interface BoardEntry extends Agent {
  /** Identyfikator sesji (do „Przejrzane”). */
  id: string;
  phase: BoardPhase;
  /** Wejście w bieżącą fazę (ms). */
  phaseAt: number;
  seen: boolean;
  /** Na co czeka sesja („zgoda: Bash”, „pytanie”) albo „brak zdarzeń” przy końcu. */
  reason: string | null;
  /** Pracujący subagenci. */
  subagents: number;
  /** Koniec z braku zdarzeń (ubity proces), nie po zakończonej turze. */
  timedOut: boolean;
}

/** Nieprzejrzane czekające i zakończone (`agent_board::counts`). */
export interface BoardCounts {
  waiting: number;
  done: number;
}

/** Koniec z braku zdarzeń jest na tablicy, ale nie w licznikach (`agent_board::counts`). */
export function boardCounts(list: readonly BoardEntry[]): BoardCounts {
  return list
    .filter((e) => !e.seen && !e.timedOut)
    .reduce(
      (c, e) => ({
        waiting: c.waiting + Number(e.phase === "waiting"),
        done: c.done + Number(e.phase === "done"),
      }),
      { waiting: 0, done: 0 },
    );
}

const eventName = "agent-board-changed";
/** Podgląd: tablica do przeliczenia (po „Przejrzane”). */
const previewEvent = "tuktuk-track-preview-agent-board";

async function previewBoard(): Promise<BoardEntry[]> {
  const sessions = previewHookBoard(Date.now());
  if (!sessions.length) return [];
  const data = await read();
  const bindings = previewBindings();
  return sessions.map((s) => {
    // Podgląd nie czyta gałęzi git: klucz tylko z nazwy folderu (jak worktree).
    const key = jiraKeyIn(s.name);
    return {
      kind: s.agent,
      name: s.name,
      cwd: s.cwd,
      since: s.phaseAt,
      branch: null,
      detached: false,
      key,
      task: matchAgentTask(data, bindings, s.cwd, key),
      id: s.sessionId,
      phase: s.phase,
      phaseAt: s.phaseAt,
      seen: s.seen,
      reason: s.reason,
      subagents: s.subagents,
      timedOut: s.timedOut,
    };
  });
}

export async function readAgentBoard(): Promise<BoardEntry[]> {
  if (desktop) return invoke("agent_board");
  return previewBoard();
}

/** „Przejrzane” dla jednej sesji (czekającej albo zakończonej). */
export async function markAgentSeen(
  entry: Pick<BoardEntry, "kind" | "id">,
): Promise<BoardEntry[]> {
  if (desktop)
    return invoke("agent_board_seen", { kind: entry.kind, id: entry.id });
  if (previewMarkBoardSeen(Date.now(), { agent: entry.kind, sessionId: entry.id }))
    window.dispatchEvent(new Event(previewEvent));
  return previewBoard();
}

/** „Przejrzane” dla wszystkich czekających i zakończonych sesji. */
export async function markAllAgentsSeen(): Promise<BoardEntry[]> {
  if (desktop) return invoke("agent_board_seen_all");
  if (previewMarkBoardSeen(Date.now()))
    window.dispatchEvent(new Event(previewEvent));
  return previewBoard();
}

/** Nasłuch zmian tablicy (zdarzenie Tauri; w podglądzie localStorage i zdarzenia okna). */
export async function onAgentBoardChanged(
  handler: (list: BoardEntry[]) => void,
): Promise<() => void> {
  if (desktop)
    return listen<BoardEntry[]>(eventName, (e) => handler(e.payload));
  // Kolejne przeliczenia mogą skończyć się w innej kolejności: wygrywa ostatnie.
  let latest = 0;
  const reload = () => {
    const mine = ++latest;
    previewBoard()
      .then((list) => {
        if (mine === latest) handler(list);
      })
      .catch((e: unknown) =>
        console.warn("Nie udało się przeliczyć tablicy sesji:", e),
      );
  };
  const onStorage = (e: StorageEvent) => {
    if (e.key === agentHooksKey || e.key === agentBoardSeenKey) reload();
  };
  window.addEventListener("storage", onStorage);
  window.addEventListener(previewEvent, reload);
  window.addEventListener(previewAgentsEvent, reload);
  return () => {
    window.removeEventListener("storage", onStorage);
    window.removeEventListener(previewEvent, reload);
    window.removeEventListener(previewAgentsEvent, reload);
  };
}
