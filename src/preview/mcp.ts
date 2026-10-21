/**
 * Symulowany serwer MCP w podglądzie (`mcp.rs`, A2): kolejka propozycji w
 * localStorage, bez procesu i bez potoku.
 *
 * Klucz `todo-track-preview-mcp-v1`: `{ proposals?, exeFound? }`.
 * - `proposals`: propozycje czekające na decyzję (`McpProposal`); test dopisuje je
 *   jak agent wywołujący `propose_*` i woła `mcpProposalsEvent`,
 * - `exeFound`: `false` symuluje brak `todo-track-mcp.exe` obok aplikacji.
 * Stan rejestracji wynika z symulowanego silnika AI (`preview/ai.ts`) i ustawień.
 */
import { previewJson, previewSettingsKey } from "./storage";
import { aiStatus } from "./ai";
import type { McpEntry, McpProposal, McpStatus } from "../api/mcp";

export const mcpKey = "todo-track-preview-mcp-v1";
/** Zdarzenie okna po zmianie kolejki (odpowiednik zdarzenia Rust `ai-proposals-changed`). */
export const mcpProposalsEvent = "todo-track-mcp-proposals";
const previewExe = "C:\\Users\\ala\\AppData\\Local\\Todo Track\\todo-track-mcp.exe";

function stored(): Record<string, unknown> {
  return previewJson(mcpKey);
}

/** Propozycje czekające na decyzję. */
export async function mcpProposals(): Promise<McpProposal[]> {
  const list = stored().proposals;
  return Array.isArray(list) ? (list as McpProposal[]) : [];
}

/** Zdejmuje propozycję z kolejki (`mcp_proposal_resolve`); zwraca pozostałe. */
export async function mcpProposalResolve(id: number): Promise<McpProposal[]> {
  const left = (await mcpProposals()).filter((p) => p.id !== id);
  try {
    localStorage.setItem(mcpKey, JSON.stringify({ ...stored(), proposals: left }));
  } catch {
    // Bez localStorage kolejka nie zapamięta decyzji.
  }
  window.dispatchEvent(new Event(mcpProposalsEvent));
  return left;
}

/** `mcp_status` w podglądzie: rejestracja zgodna z ustawieniami, jak po synchronizacji. */
export async function mcpStatus(): Promise<McpStatus> {
  const ai = await aiStatus();
  const settings = previewJson(previewSettingsKey);
  const engine = ai.engine;
  const enabled =
    (engine === "claude" || engine === "codex") && settings.aiMcp !== false;
  const exeFound = stored().exeFound !== false;
  const entries: McpEntry[] =
    engine === ""
      ? []
      : (
          [
            ["claude", "Claude Code", ai.detected.claude],
            ["codex", "Codex", ai.detected.codex],
          ] as const
        ).map(([id, name, cli]) => ({
          engine: id,
          name,
          state: !cli
            ? "unavailable"
            : enabled && exeFound
              ? "registered"
              : enabled
                ? "error"
                : "missing",
          detail:
            cli && enabled && !exeFound
              ? "Brak programu todo-track-mcp.exe obok aplikacji. Zainstaluj aplikację ponownie."
              : "",
        }));
  return { exe: exeFound ? previewExe : "", exeFound, enabled, entries };
}

/** „Zarejestruj ponownie” w podglądzie: stan jak po synchronizacji. */
export async function mcpSync(): Promise<McpStatus> {
  return mcpStatus();
}
