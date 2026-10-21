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
  subagents: Set<string>;
  firstPrompt: string | null;
}

/** Pracujący agent z hooków (jak `agents::Agent` przed rozpoznaniem zadania). */
export interface HookAgent {
  kind: "claude" | "codex";
  name: string;
  cwd: string;
  since: number;
}

/** Maszyna stanów sesji (`hooks::Sessions`) zastosowana do listy zdarzeń. */
export function foldHookEvents(
  events: PreviewHookEvent[],
  now: number,
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
        subagents: new Set(),
        firstPrompt: null,
      };
      map.set(key, s);
      s.last = ev.at;
      if (ev.cwd) s.cwd = ev.cwd;
      const work = () => {
        if (s.phase !== "working") s.since = ev.at;
        s.phase = "working";
      };
      switch (ev.event) {
        case "UserPromptSubmit":
          work();
          s.firstPrompt ??= ev.prompt?.slice(0, 500) ?? null;
          break;
        case "PreToolUse":
        case "PostToolUse":
        case "SubagentStart":
          if (ev.agentId) s.subagents.add(ev.agentId);
          work();
          break;
        case "SubagentStop":
          if (ev.agentId) s.subagents.delete(ev.agentId);
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
          if (!s.subagents.size) s.phase = "done";
          break;
        case "Interrupt":
          s.subagents.clear();
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
    if (s.phase !== "done" && (now - s.last > HOOK_SILENCE || now < s.last)) {
      s.phase = "done";
      s.subagents.clear();
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
export function previewHookAgents(now: number): HookAgent[] {
  if (!enabled()) return [];
  return foldHookEvents(events(), now)
    .sessions.filter((s) => s.phase === "working")
    .map((s) => ({
      kind: s.agent,
      name: folder(s.cwd) || "sesja",
      cwd: s.cwd,
      since: s.since,
    }))
    .sort((a, b) => a.since - b.since || a.kind.localeCompare(b.kind));
}

/** Pierwszy prompt sesji (pod A4), przycięty do 500 znaków; tylko w pamięci. */
export function previewFirstPrompt(
  agent: "claude" | "codex",
  sessionId: string,
  now: number,
): string | null {
  const found = foldHookEvents(events(), now).sessions.find(
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
