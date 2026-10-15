import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  desktop,
  previewAgentsEvent,
  previewBindings,
  read,
  type Snapshot,
} from "./store";

/** Odpowiednik `Match` z src-tauri/src/agents.rs: zadanie, nad którym pracuje agent. */
export interface AgentTask {
  /** Lokalne zadanie; `null`, gdy klucz zna tylko lista zgłoszeń z Jiry. */
  taskId: number | null;
  /** Tytuł zadania albo zgłoszenia (pusty, gdy tytuł zgłoszenia nie jest znany). */
  title: string;
  key: string | null;
  /** `folder` (przypisanie folderu), `task` (klucz lokalnego zadania), `jira`. */
  source: "folder" | "task" | "jira";
}

/** Odpowiednik `Agent` z src-tauri/src/agents.rs: agent AI, który teraz pracuje. */
export interface Agent {
  kind: "claude" | "codex";
  name: string;
  cwd: string;
  /** Od kiedy pracuje (ms). */
  since: number;
  /** Gałąź git folderu roboczego, a przy odłączonym HEAD skrót commita. */
  branch?: string | null;
  detached?: boolean;
  /** Klucz Jiry z gałęzi, a bez niego z nazwy sesji Claude Code. */
  key?: string | null;
  task?: AgentTask | null;
}
/**
 * Agent w podglądzie (localStorage, testy): jak `Agent`, a zamiast wyniku
 * rozpoznania podaje gałąź i nazwę sesji (`session`); klucz i zadanie liczy
 * `recognize` tak jak Rust.
 */
type PreviewAgent = Agent & { session?: string | null };
/** W podglądzie lista agentów pochodzi z localStorage (testy). */
const key = "todo-track-preview-agents-v1";
const eventName = "agents-changed";

/** Pierwszy klucz Jiry (`[A-Z][A-Z0-9]+-\d+`) jak `agents::jira_key`. */
export function jiraKeyIn(text: string | null | undefined): string | null {
  return text?.match(/(?<![A-Za-z0-9])[A-Z][A-Z0-9]+-\d+/)?.[0] ?? null;
}

const normFolder = (folder: string) =>
  folder.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/**
 * Dopasowanie jak `agents::resolve`: przypisanie folderu (także nadrzędnego,
 * najbliższe wygrywa) → klucz powiązany z lokalnym zadaniem. W podglądzie nie ma
 * listy zgłoszeń z Jiry (`jira_issues`), więc trzeci krok nie występuje.
 */
export function matchAgentTask(
  data: Snapshot,
  bindings: { folder: string; taskId: number }[],
  cwd: string,
  jiraKey: string | null,
): AgentTask | null {
  const folder = normFolder(cwd);
  const bound = folder
    ? bindings
        .map((b) => ({ ...b, norm: normFolder(b.folder) }))
        .filter(
          (b) =>
            b.norm && (folder === b.norm || folder.startsWith(`${b.norm}/`)),
        )
        .map((b) => ({ ...b, task: data.tasks.find((t) => t.id === b.taskId) }))
        .filter((b) => b.task)
        .sort((a, b) => b.norm.length - a.norm.length)[0]
    : undefined;
  if (bound?.task)
    return {
      taskId: bound.task.id,
      title: bound.task.title,
      key: bound.task.jiraKey ?? null,
      source: "folder",
    };
  if (!jiraKey) return null;
  const local = data.tasks
    .filter((t) => t.jiraKey === jiraKey)
    .sort((a, b) => Number(a.done) - Number(b.done) || b.id - a.id)[0];
  return local
    ? { taskId: local.id, title: local.title, key: jiraKey, source: "task" }
    : null;
}

async function previewAgents(): Promise<Agent[]> {
  let list: PreviewAgent[];
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    list = Array.isArray(value) ? value : [];
  } catch {
    list = [];
  }
  const data = await read();
  const bindings = previewBindings();
  return list.map(({ session, ...agent }) => {
    const branch = agent.branch ?? null;
    const detached = !!agent.detached;
    const jiraKey =
      (detached ? null : jiraKeyIn(branch)) ?? jiraKeyIn(session ?? null);
    return {
      ...agent,
      branch,
      detached,
      key: jiraKey,
      task: matchAgentTask(data, bindings, agent.cwd, jiraKey),
    };
  });
}

export async function readAgents(): Promise<Agent[]> {
  if (desktop) return invoke("agents_status");
  return previewAgents();
}

/** Nasłuch zmian listy agentów (zdarzenie Tauri; w podglądzie zdarzenie `storage`). */
export async function onAgentsChanged(
  handler: (agents: Agent[]) => void,
): Promise<() => void> {
  if (desktop) return listen<Agent[]>(eventName, (e) => handler(e.payload));
  const reload = () => void previewAgents().then(handler);
  const listener = (e: StorageEvent) => {
    if (e.key === key) reload();
  };
  window.addEventListener("storage", listener);
  window.addEventListener(previewAgentsEvent, reload);
  return () => {
    window.removeEventListener("storage", listener);
    window.removeEventListener(previewAgentsEvent, reload);
  };
}
