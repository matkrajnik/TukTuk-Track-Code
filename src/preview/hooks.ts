/**
 * Podgląd hooków agentów (`hooks.rs`, B8): zdarzenia z localStorage przechodzą
 * przez tę samą maszynę stanów co w Rust i zasilają kafelek „Agenci AI”.
 *
 * Klucz `tuktuk-track-preview-agent-hooks-v1`: `{ events, claude?, codex?,
 * codexTrust?, error?, exeFound? }`, gdzie `events` to zdarzenia hooków
 * (`PreviewHookEvent`) w kolejności przyjścia.
 */
import { previewJson, previewSettingsKey } from "./storage";
import type {
  AgentHooksStatus,
  ExternalApiInfo,
  HookEventRow,
} from "../api/task";

export const agentHooksKey = "tuktuk-track-preview-agent-hooks-v1";
/** Sesja bez zdarzeń tak długo kończy się sama (`hooks::SILENCE`). */
export const HOOK_SILENCE = 10 * 60_000;
/** Limit ciszy w trakcie wywołania narzędzia (`hooks::TOOL_SILENCE`). */
export const HOOK_TOOL_SILENCE = 60 * 60_000;
/** Subagent bez zdarzeń przestaje się liczyć (`hooks::SUBAGENT_SILENCE`). */
export const HOOK_SUBAGENT_SILENCE = 3 * 60_000;
/** Czekanie na użytkownika kończy się samo po takim czasie (`hooks::WAIT_SILENCE`). */
export const HOOK_WAIT_SILENCE = 2 * 3_600_000;
/** Powód końca z braku zdarzeń (`hooks::TIMEOUT_REASON`). */
export const HOOK_TIMEOUT_REASON = "brak zdarzeń";
/** Zakończona sesja jest zapominana po takim czasie (`hooks::FORGET`). */
const FORGET = 12 * 3_600_000;
const RECENT = 30;

/** Zdarzenie hooka jak w `hooks::HookEvent`, z czasem przyjścia. */
export interface PreviewHookEvent {
  at: number;
  agent: "claude" | "codex";
  event: string;
  sessionId: string;
  cwd?: string;
  agentId?: string;
  notificationType?: string;
  prompt?: string;
  /** Nazwa narzędzia (`PreToolUse`, `PermissionRequest`). */
  toolName?: string;
  /** Komunikat powiadomienia agenta (`Notification`). */
  message?: string;
  /** Źródło sesji (`hook_wire::host`): orca, claude-desktop, codex-desktop, vscode, terminal. */
  host?: string;
  /** Uchwyt terminala Orki albo `local_…` Claude Desktop. */
  hostRef?: string;
}

/** Znane źródła sesji (`hook_wire::HOSTS`). */
export const HOOK_HOSTS = [
  "orca",
  "claude-desktop",
  "codex-desktop",
  "vscode",
  "terminal",
] as const;
export type HookHost = (typeof HOOK_HOSTS)[number];
const isHost = (host: string | undefined): host is HookHost =>
  (HOOK_HOSTS as readonly string[]).includes(host ?? "");

export type HookPhase = "working" | "waiting" | "done";
type Phase = HookPhase;
interface Session {
  agent: "claude" | "codex";
  sessionId: string;
  cwd: string;
  phase: Phase;
  since: number;
  last: number;
  /** Subagenci z czasem ostatniego zdarzenia. */
  subagents: Map<string, number>;
  /** Niezakończone wywołania narzędzi według wykonawcy ("" = agent główny). */
  tools: Map<string, number>;
  /** Stop agenta głównego przy działających subagentach. */
  stopped: boolean;
  firstPrompt: string | null;
  /** Wejście w bieżącą fazę (`Session::phase_at`). */
  phaseAt: number;
  /** Czekanie albo koniec widziane przez użytkownika (`Session::seen`). */
  seen: boolean;
  /** Na co czeka sesja albo `HOOK_TIMEOUT_REASON` (`Session::reason`). */
  reason: string | null;
  /**
   * Zakończone czekania `[początek, koniec]`. W aplikacji zapisuje je pętla
   * agentów (`agent_wait.rs`, koniec = ostatnia obserwacja); tu znamy chwilę
   * wyjścia z czekania wprost ze zdarzeń.
   */
  waits: [number, number][];
  /** Źródło sesji z ostatniego zdarzenia, które je podało (`Session::host`). */
  host: HookHost | "";
  hostRef: string | null;
}

/** Czekanie sesji na użytkownika (`agent_waits`). */
export interface HookWait {
  agent: "claude" | "codex";
  sessionId: string;
  start: number;
  end: number;
}

/** `Session::enter`: nowa faza, chwila wejścia, czekanie i koniec nieprzejrzane. */
function enter(
  s: Session,
  phase: Phase,
  at: number,
  /** Koniec czekania, gdy inny niż `at` (wygaśnięcie z ciszy). */
  waitEnd = at,
) {
  if (s.phase === phase) return;
  if (s.phase === "waiting") s.waits.push([s.phaseAt, waitEnd]);
  s.phase = phase;
  s.phaseAt = at;
  s.seen = phase === "working";
  s.reason = null;
}
/** `Session::wait`: pierwszy znany powód zostaje. */
function wait(s: Session, reason: string, at: number) {
  enter(s, "waiting", at);
  s.reason ??= reason;
}
/** Narzędzia, którymi agent pyta użytkownika (`hooks::asks_user`). */
const ASKS = new Map<string, string>([
  ["AskUserQuestion", "pytanie"],
  ["ExitPlanMode", "plan do akceptacji"],
]);

const inTool = (s: Session) => [...s.tools.values()].some((n) => n > 0);

/** `Session::prune_subagents`: subagenci bez zdarzeń przestają się liczyć. */
function pruneSubagents(s: Session, now: number) {
  for (const [id, last] of s.subagents) {
    const limit =
      (s.tools.get(id) ?? 0) > 0 ? HOOK_TOOL_SILENCE : HOOK_SUBAGENT_SILENCE;
    if (now < last || now - last > limit) {
      s.subagents.delete(id);
      s.tools.delete(id);
    }
  }
}

/** Pracujący agent z hooków (jak `agents::Agent` przed rozpoznaniem zadania). */
export interface HookAgent {
  kind: "claude" | "codex";
  name: string;
  cwd: string;
  since: number;
  /** Identyfikator sesji (klucz pamięci asystenta przypisań, A4). */
  sessionId: string;
  /** Pierwszy prompt sesji, tylko przy `keepPrompts` (`ai_assign_suggest`). */
  firstPrompt: string | null;
}

/** Maszyna stanów sesji (`hooks::Sessions`) zastosowana do listy zdarzeń. */
export function foldHookEvents(
  events: PreviewHookEvent[],
  now: number,
  /** `Sessions::keep_prompts` (z `ai_assign_suggest` i wybranego silnika). */
  keepPrompts = false,
): { sessions: Session[]; recent: HookEventRow[]; waits: HookWait[] } {
  const map = new Map<string, Session>();
  const recent: HookEventRow[] = [];
  /** Czekania sesji zakończonych `SessionEnd` (sesja znika z mapy). */
  const ended: HookWait[] = [];
  const sorted = [...events].sort((a, b) => a.at - b.at);
  for (const ev of sorted) {
    const key = `${ev.agent}|${ev.sessionId}`;
    let detail = ev.notificationType ?? ev.agentId ?? "";
    if (ev.event === "SessionEnd") {
      const gone = map.get(key);
      if (gone) {
        enter(gone, "done", ev.at);
        ended.push(...waitsOf(gone, ev.at));
      }
      map.delete(key);
    } else {
      const s: Session = map.get(key) ?? {
        agent: ev.agent,
        sessionId: ev.sessionId,
        cwd: ev.cwd ?? "",
        phase: "done",
        since: ev.at,
        last: ev.at,
        subagents: new Map(),
        tools: new Map(),
        stopped: false,
        firstPrompt: null,
        phaseAt: ev.at,
        seen: true,
        reason: null,
        waits: [],
        host: "",
        hostRef: null,
      };
      map.set(key, s);
      s.last = ev.at;
      if (ev.cwd) s.cwd = ev.cwd;
      // Jak `hooks::host_of`: źródło desktopowe tylko dla „swojego” agenta.
      const foreign =
        (ev.host === "claude-desktop" && ev.agent !== "claude") ||
        (ev.host === "codex-desktop" && ev.agent !== "codex");
      if (isHost(ev.host) && !foreign) {
        s.host = ev.host;
        s.hostRef = ev.hostRef ?? null;
      }
      const work = () => {
        if (s.phase !== "working") s.since = ev.at;
        enter(s, "working", ev.at);
      };
      // Komunikat jako jedna linia (`hooks::parse`).
      const message = ev.message?.split(/\s+/).filter(Boolean).join(" ");
      const told = (fallback: string) => message || fallback;
      const actor = ev.agentId ?? "";
      if (ev.agentId && ev.event !== "SubagentStop")
        s.subagents.set(ev.agentId, ev.at);
      switch (ev.event) {
        case "UserPromptSubmit":
          work();
          s.stopped = false;
          if (keepPrompts) s.firstPrompt ??= ev.prompt?.slice(0, 500) ?? null;
          break;
        case "PreToolUse": {
          const asks = actor ? undefined : ASKS.get(ev.toolName ?? "");
          s.tools.set(actor, (s.tools.get(actor) ?? 0) + 1);
          if (asks) wait(s, asks, ev.at);
          else work();
          break;
        }
        case "PostToolUse": {
          const n = (s.tools.get(actor) ?? 0) - 1;
          if (n > 0) s.tools.set(actor, n);
          else s.tools.delete(actor);
          work();
          break;
        }
        case "SubagentStart":
          work();
          break;
        case "SubagentStop":
          if (ev.agentId) {
            s.subagents.delete(ev.agentId);
            s.tools.delete(ev.agentId);
          }
          break;
        case "PermissionRequest":
          wait(
            s,
            ev.toolName ? `zgoda: ${ev.toolName}` : "prośba o zgodę",
            ev.at,
          );
          break;
        case "Notification":
          if (ev.notificationType === "permission_prompt")
            wait(s, told("prośba o zgodę"), ev.at);
          else if (
            ev.notificationType === "agent_needs_input" ||
            ev.notificationType === "elicitation_dialog"
          )
            wait(s, told("pytanie"), ev.at);
          else if (
            ev.notificationType === "idle_prompt" &&
            s.phase === "working"
          )
            wait(s, told("czeka na odpowiedź"), ev.at);
          break;
        case "Stop":
          s.tools.delete("");
          pruneSubagents(s, ev.at);
          if (!s.subagents.size) {
            enter(s, "done", ev.at);
            s.stopped = false;
          } else s.stopped = true;
          break;
        case "Interrupt":
          s.subagents.clear();
          s.tools.clear();
          s.stopped = false;
          enter(s, "done", ev.at);
          s.seen = true;
          break;
      }
      if (!detail)
        detail =
          s.phase === "working"
            ? "pracuje"
            : s.phase === "waiting"
              ? "czeka"
              : "koniec";
    }
    recent.push({
      at: ev.at,
      agent: ev.agent,
      event: ev.event,
      session: ev.sessionId.slice(0, 8),
      cwd: ev.cwd ?? "",
      detail,
    });
  }
  // Timeout braku zdarzeń (`Sessions::expire`).
  for (const [key, s] of map) {
    if (s.phase !== "done") {
      const limit =
        s.phase === "waiting"
          ? HOOK_WAIT_SILENCE
          : inTool(s)
            ? HOOK_TOOL_SILENCE
            : HOOK_SILENCE;
      const silent = now - s.last > limit || now < s.last;
      let orphaned = false;
      if (s.stopped) {
        pruneSubagents(s, now);
        orphaned =
          !s.subagents.size && now - s.last > HOOK_SUBAGENT_SILENCE;
      }
      if (silent || orphaned) {
        // Koniec od ostatniego zdarzenia, nie od chwili wykrycia ciszy.
        // Pętla aplikacji widzi czekanie aż do wygaśnięcia (`last + limit`),
        // więc tam kończy się zapisane czekanie (`agent_waits`).
        enter(s, "done", Math.min(s.last, now), Math.min(now, s.last + limit));
        if (silent && !orphaned) s.reason = HOOK_TIMEOUT_REASON;
        s.subagents.clear();
        s.tools.clear();
        s.stopped = false;
      }
    }
    if (s.phase === "done" && (now < s.last || now - s.last > FORGET)) {
      // Zapomniana sesja: jej czekania zostają (w aplikacji leżą w bazie).
      ended.push(...waitsOf(s, now));
      map.delete(key);
    }
  }
  return {
    sessions: [...map.values()],
    recent: recent.slice(-RECENT).reverse(),
    waits: [...ended, ...[...map.values()].flatMap((s) => waitsOf(s, now))],
  };
}

/** Czekania sesji: zakończone i trwające (do `now`). */
function waitsOf(s: Session, now: number): HookWait[] {
  const spans: [number, number][] =
    s.phase === "waiting" ? [...s.waits, [s.phaseAt, now]] : s.waits;
  return spans.map(([start, end]) => ({
    agent: s.agent,
    sessionId: s.sessionId,
    start,
    end: Math.max(start, end),
  }));
}

/** Czekanie w `[from, to)` (`agent_wait::summarize`): bez liczenia dwa razy. */
export interface WaitTime {
  totalMs: number;
  count: number;
  longestMs: number;
}
export function summarizeWaits(
  waits: readonly { start: number; end: number }[],
  from: number,
  to: number,
): WaitTime {
  const clipped = waits
    .map((w) => [Math.max(w.start, from), Math.min(w.end, to)] as const)
    .filter(([a, b]) => b > a);
  const sorted = [...clipped].sort((x, y) => x[0] - y[0] || x[1] - y[1]);
  const union: [number, number][] = [];
  for (const [a, b] of sorted) {
    const last = union[union.length - 1];
    if (last && a <= last[1]) last[1] = Math.max(last[1], b);
    else union.push([a, b]);
  }
  return {
    totalMs: union.reduce((sum, [a, b]) => sum + b - a, 0),
    count: clipped.length,
    longestMs: clipped.reduce((max, [a, b]) => Math.max(max, b - a), 0),
  };
}

/**
 * `agent_wait_time` w podglądzie: czekania liczone ze zdarzeń hooków. W aplikacji
 * do bazy trafia tylko czekanie widziane, gdy aplikacja działa i hooki są
 * włączone; podgląd liczy całą historię zdarzeń.
 */
export function previewAgentWaitTime(
  from: number,
  to: number,
  now: number,
): WaitTime {
  if (!enabled()) return { totalMs: 0, count: 0, longestMs: 0 };
  return summarizeWaits(foldHookEvents(events(), now).waits, from, to);
}

function stored(): Record<string, unknown> {
  return previewJson(agentHooksKey);
}
function events(): PreviewHookEvent[] {
  const value = stored().events;
  return Array.isArray(value) ? (value as PreviewHookEvent[]) : [];
}
/** Hooki są włączone w ustawieniach podglądu (domyślnie tak). */
function enabled(): boolean {
  return previewJson(previewSettingsKey).agentHooks !== false;
}
const folder = (cwd: string) =>
  cwd
    .replace(/[\\/]+$/, "")
    .split(/[\\/]/)
    .pop() ?? "";

/** Pracujące sesje z hooków jako agenci kafelka (`hooks::Sessions::working`). */
export function previewHookAgents(now: number, keepPrompts = false): HookAgent[] {
  if (!enabled()) return [];
  return foldHookEvents(events(), now, keepPrompts)
    .sessions.filter((s) => s.phase === "working")
    .map((s) => ({
      kind: s.agent,
      name: folder(s.cwd) || "sesja",
      cwd: s.cwd,
      since: s.since,
      sessionId: s.sessionId,
      firstPrompt: s.firstPrompt,
    }))
    .sort((a, b) => a.since - b.since || a.kind.localeCompare(b.kind));
}

/**
 * „Przejrzane” w podglądzie: sesje liczone są od nowa ze zdarzeń, więc
 * zapamiętujemy klucz `agent|sesja|wejście w fazę` (kolejne czekanie albo koniec
 * jest znów nieprzejrzane, jak w `Session::enter`).
 */
export const agentBoardSeenKey = "tuktuk-track-preview-agent-board-seen-v1";
const seenKey = (s: { agent: string; sessionId: string; phaseAt: number }) =>
  `${s.agent}|${s.sessionId}|${s.phaseAt}`;
function seenMarks(): Set<string> {
  try {
    const value = JSON.parse(localStorage.getItem(agentBoardSeenKey) ?? "[]");
    return new Set(Array.isArray(value) ? value.map(String) : []);
  } catch {
    return new Set();
  }
}

/** Sesja na tablicy (`hooks::BoardSession`) przed rozpoznaniem zadania. */
export interface HookBoardSession {
  agent: "claude" | "codex";
  sessionId: string;
  cwd: string;
  name: string;
  phase: HookPhase;
  phaseAt: number;
  seen: boolean;
  reason: string | null;
  subagents: number;
  timedOut: boolean;
  host: HookHost | "";
  /** Identyfikator w źródle; tylko do „Przejdź” (w Rust nie trafia do okna). */
  hostRef: string | null;
}

/** Wszystkie sesje z oznaczeniem „przejrzane” z localStorage. */
function boardSessions(now: number): HookBoardSession[] {
  if (!enabled()) return [];
  const marks = seenMarks();
  return foldHookEvents(events(), now).sessions.map((s) => ({
    agent: s.agent,
    sessionId: s.sessionId,
    cwd: s.cwd,
    name: folder(s.cwd) || "sesja",
    phase: s.phase,
    phaseAt: s.phaseAt,
    seen: s.seen || (s.phase !== "working" && marks.has(seenKey(s))),
    reason: s.reason,
    subagents: s.subagents.size,
    timedOut: s.phase === "done" && s.reason === HOOK_TIMEOUT_REASON,
    host: s.host,
    hostRef: s.hostRef,
  }));
}

/** Tablica sesji (`Sessions::board`): bez przejrzanego końca, w kolejności Rust. */
export function previewHookBoard(now: number): HookBoardSession[] {
  const rank = { waiting: 0, working: 1, done: 2 } as const;
  // Porównanie napisów jak w Rust (bajtowo), nie według locale.
  const cmp = (a: string, b: string) => (a < b ? -1 : a > b ? 1 : 0);
  return boardSessions(now)
    .filter((s) => s.phase !== "done" || !s.seen)
    .sort(
      (a, b) =>
        rank[a.phase] - rank[b.phase] ||
        a.phaseAt - b.phaseAt ||
        cmp(a.agent, b.agent) ||
        cmp(a.sessionId, b.sessionId),
    );
}

/** „Przejrzane” (`Sessions::mark_seen`, bez sesji = `mark_all_seen`); `true`, gdy coś zmieniło. */
export function previewMarkBoardSeen(
  now: number,
  target?: { agent: string; sessionId: string },
): boolean {
  const sessions = boardSessions(now);
  // Zostają tylko znaczniki bieżących faz istniejących sesji.
  const current = new Set(sessions.map(seenKey));
  const marks = new Set([...seenMarks()].filter((k) => current.has(k)));
  const fresh = sessions.filter(
    (s) =>
      s.phase !== "working" &&
      !s.seen &&
      (!target ||
        (s.agent === target.agent && s.sessionId === target.sessionId)),
  );
  if (!fresh.length) return false;
  for (const s of fresh) marks.add(seenKey(s));
  try {
    localStorage.setItem(agentBoardSeenKey, JSON.stringify([...marks]));
  } catch {
    return false;
  }
  return true;
}

/**
 * Pierwszy prompt sesji (A4), przycięty do 500 znaków; tylko w pamięci i tylko
 * przy `keepPrompts` (z ustawienia `aiAssignSuggest` i wybranego silnika).
 */
export function previewFirstPrompt(
  agent: "claude" | "codex",
  sessionId: string,
  now: number,
  keepPrompts = false,
): string | null {
  const found = foldHookEvents(events(), now, keepPrompts).sessions.find(
    (s) => s.agent === agent && s.sessionId === sessionId,
  );
  return found?.firstPrompt ?? null;
}

/** `external_api_info` w podglądzie: nie ma programu tuktuk-track-ctl.exe. */
export async function externalApiInfo(): Promise<ExternalApiInfo> {
  return { ctlPath: null };
}

/** `agent_hooks_status` w podglądzie. */
export async function agentHooksStatus(): Promise<AgentHooksStatus> {
  const data = stored();
  const on = enabled();
  const agent = (name: "claude" | "codex") => {
    const raw = data[name] as { found?: boolean } | undefined;
    const found = raw?.found !== false;
    return {
      found,
      file:
        name === "claude"
          ? "C:\\Users\\jan\\.claude\\settings.json"
          : "C:\\Users\\jan\\.codex\\hooks.json",
      registration: found && on ? "current" : "missing",
      error: "",
    };
  };
  const codex = agent("codex");
  const trust = data.codexTrust;
  return {
    enabled: on,
    exeFound: data.exeFound !== false,
    error: typeof data.error === "string" ? data.error : "",
    claude: agent("claude"),
    codex,
    codexTrust:
      on && codex.found
        ? trust === "trusted" || trust === "partial"
          ? trust
          : "untrusted"
        : "none",
    events: foldHookEvents(events(), Date.now()).recent,
  };
}
