/**
 * Podgląd hooków agentów (`hooks.rs`, B8): zdarzenia z localStorage przechodzą
 * przez tę samą maszynę stanów co w Rust i zasilają kafelek „Agenci AI”.
 *
 * Klucz `todo-track-preview-agent-hooks-v1`: `{ events, claude?, codex?,
 * codexTrust?, error?, exeFound? }`, gdzie `events` to zdarzenia hooków
 * (`PreviewHookEvent`) w kolejności przyjścia.
 */
import { previewJson, previewSettingsKey } from "./storage";
import type { AgentHooksStatus, HookEventRow } from "../api/task";

export const agentHooksKey = "todo-track-preview-agent-hooks-v1";
/** Sesja bez zdarzeń tak długo kończy się sama (`hooks::SILENCE`). */
export const HOOK_SILENCE = 10 * 60_000;
/** Limit ciszy w trakcie wywołania narzędzia (`hooks::TOOL_SILENCE`). */
export const HOOK_TOOL_SILENCE = 60 * 60_000;
/** Subagent bez zdarzeń przestaje się liczyć (`hooks::SUBAGENT_SILENCE`). */
export const HOOK_SUBAGENT_SILENCE = 3 * 60_000;
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
}

type Phase = "working" | "waiting" | "done";
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
}

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
): { sessions: Session[]; recent: HookEventRow[] } {
  const map = new Map<string, Session>();
  const recent: HookEventRow[] = [];
  const sorted = [...events].sort((a, b) => a.at - b.at);
  for (const ev of sorted) {
    const key = `${ev.agent}|${ev.sessionId}`;
    let detail = ev.notificationType ?? ev.agentId ?? "";
    if (ev.event === "SessionEnd") map.delete(key);
    else {
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
      };
      map.set(key, s);
      s.last = ev.at;
      if (ev.cwd) s.cwd = ev.cwd;
      const work = () => {
        if (s.phase !== "working") s.since = ev.at;
        s.phase = "working";
      };
      const actor = ev.agentId ?? "";
      if (ev.agentId && ev.event !== "SubagentStop")
        s.subagents.set(ev.agentId, ev.at);
      switch (ev.event) {
        case "UserPromptSubmit":
          work();
          s.stopped = false;
          if (keepPrompts) s.firstPrompt ??= ev.prompt?.slice(0, 500) ?? null;
          break;
        case "PreToolUse":
          s.tools.set(actor, (s.tools.get(actor) ?? 0) + 1);
          work();
          break;
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
          s.phase = "waiting";
          break;
        case "Notification":
          if (
            [
              "permission_prompt",
              "agent_needs_input",
              "elicitation_dialog",
            ].includes(ev.notificationType ?? "")
          )
            s.phase = "waiting";
          else if (
            ev.notificationType === "idle_prompt" &&
            s.phase === "working"
          )
            s.phase = "waiting";
          break;
        case "Stop":
          s.tools.delete("");
          pruneSubagents(s, ev.at);
          if (!s.subagents.size) {
            s.phase = "done";
            s.stopped = false;
          } else s.stopped = true;
          break;
        case "Interrupt":
          s.subagents.clear();
          s.tools.clear();
          s.stopped = false;
          s.phase = "done";
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
      const limit = inTool(s) ? HOOK_TOOL_SILENCE : HOOK_SILENCE;
      const silent = now - s.last > limit || now < s.last;
      let orphaned = false;
      if (s.stopped) {
        pruneSubagents(s, now);
        orphaned =
          !s.subagents.size && now - s.last > HOOK_SUBAGENT_SILENCE;
      }
      if (silent || orphaned) {
        s.phase = "done";
        s.subagents.clear();
        s.tools.clear();
        s.stopped = false;
      }
    }
    if (s.phase === "done" && (now < s.last || now - s.last > FORGET))
      map.delete(key);
  }
  return {
    sessions: [...map.values()],
    recent: recent.slice(-RECENT).reverse(),
  };
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
