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
  orcaKey,
  previewAgentsEvent,
  previewBindings,
  read,
} from "./store";
import {
  agentBoardSeenKey,
  previewAgentWaitTime,
  previewHookBoard,
  previewMarkBoardSeen,
  type HookPhase,
  type WaitTime,
} from "./preview/hooks";
import { previewJson } from "./preview/storage";

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
  /** Folder sesji to worktree Orki: „Przejdź” może wyciągnąć Orkę na wierzch. */
  orca: boolean;
}

/** Dokąd prowadzi „Przejdź” (`agent_board::Target`). */
export type OpenTarget = "orca" | "code" | "folder";

/** Czekanie sesji na użytkownika w zakresie (`agent_wait::WaitTime`). */
export type { WaitTime } from "./preview/hooks";

/**
 * Podgląd: ostatnie „Przejdź” (nic nie uruchamia; testy sprawdzają, co poszłoby
 * do aplikacji).
 */
export const agentOpenKey = "tuktuk-track-preview-agent-open-v1";

const norm = (folder: string) =>
  folder.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
/** Worktree z podglądu Orki (`orca::in_worktrees`). */
function previewOrcaFolders(): string[] {
  const worktrees = previewJson(orcaKey).worktrees;
  return Array.isArray(worktrees)
    ? worktrees
        .map((w) => norm(String((w as { path?: unknown })?.path ?? "")))
        .filter(Boolean)
    : [];
}
const inWorktrees = (folders: string[], cwd: string) => {
  const folder = norm(cwd);
  return (
    !!folder && folders.some((w) => folder === w || folder.startsWith(`${w}/`))
  );
};

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
  const worktrees = previewOrcaFolders();
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
      orca: inWorktrees(worktrees, s.cwd),
    };
  });
}

/**
 * „Przejdź”: folder sesji w Orce, VS Code albo Eksploratorze; potem sesja jest
 * przejrzana. W podglądzie nic nie uruchamia, tylko zapisuje żądanie.
 */
export async function openAgentSession(
  entry: Pick<BoardEntry, "kind" | "id">,
  target: OpenTarget,
): Promise<BoardEntry[]> {
  if (desktop)
    return invoke("agent_board_open", {
      kind: entry.kind,
      id: entry.id,
      target,
    });
  const found = (await previewBoard()).find(
    (e) => e.kind === entry.kind && e.id === entry.id,
  );
  if (!found) throw new Error("Tej sesji nie ma już na tablicy.");
  if (target === "orca" && !found.orca)
    throw new Error("Folder sesji nie jest worktree Orki.");
  try {
    localStorage.setItem(
      agentOpenKey,
      JSON.stringify({ kind: entry.kind, id: entry.id, target, cwd: found.cwd }),
    );
  } catch {
    // Zapis żądania służy tylko testom; brak miejsca niczego nie psuje.
  }
  return markAgentSeen(entry);
}

/** Czas czekania sesji na użytkownika w `[from, to)` (`agent_wait_time`). */
export async function readAgentWaitTime(
  from: number,
  to: number,
): Promise<WaitTime> {
  if (desktop) return invoke("agent_wait_time", { from, to });
  return previewAgentWaitTime(from, to, Date.now());
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
