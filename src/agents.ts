import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { desktop } from "./store";

/** Odpowiednik `Agent` z src-tauri/src/agents.rs: agent AI, który teraz pracuje. */
export interface Agent {
  kind: "claude" | "codex";
  name: string;
  cwd: string;
  /** Od kiedy pracuje (ms). */
  since: number;
}
/** W podglądzie lista agentów pochodzi z localStorage (testy). */
const key = "todo-track-preview-agents-v1";
const eventName = "agents-changed";

function previewAgents(): Agent[] {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
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
  const listener = (e: StorageEvent) => {
    if (e.key === key) handler(previewAgents());
  };
  window.addEventListener("storage", listener);
  return () => window.removeEventListener("storage", listener);
}
