/**
 * Symulowany serwer MCP w podglądzie (`mcp.rs`, A2): kolejka propozycji w
 * localStorage, bez procesu i bez potoku.
 *
 * Klucz `tuktuk-track-preview-mcp-v1`: `{ proposals?, exeFound? }`.
 * - `proposals`: propozycje czekające na decyzję (`McpProposal`); test dopisuje je
 *   jak agent wywołujący `propose_*` i woła `mcpProposalsEvent`,
 * - `exeFound`: `false` symuluje brak `tuktuk-track-mcp.exe` obok aplikacji.
 * Stan rejestracji wynika z symulowanego silnika AI (`preview/ai.ts`) i ustawień.
 */
import { previewJson, previewSettingsKey } from "./storage";
import { aiStatus } from "./ai";
import type { McpEntry, McpProposal, McpStatus } from "../api/mcp";

export const mcpKey = "tuktuk-track-preview-mcp-v1";
/** Zdarzenie okna po zmianie kolejki (odpowiednik zdarzenia Rust `ai-proposals-changed`). */
export const mcpProposalsEvent = "tuktuk-track-mcp-proposals";
/** Zdarzenie okna po synchronizacji rejestracji (odpowiednik `mcp-status-changed`). */
export const mcpStatusEvent = "tuktuk-track-mcp-status";
const previewExe = "C:\\Users\\ala\\AppData\\Local\\TukTuk Track\\tuktuk-track-mcp.exe";

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
              ? "Brak programu tuktuk-track-mcp.exe obok aplikacji. Zainstaluj aplikację ponownie."
              : "",
        }));
  return { exe: exeFound ? previewExe : "", exeFound, enabled, entries };
}

/** „Zarejestruj ponownie” w podglądzie: stan jak po synchronizacji. */
export async function mcpSync(): Promise<McpStatus> {
  const status = await mcpStatus();
  window.dispatchEvent(new Event(mcpStatusEvent));
  return status;
}

/**
 * Symulowane `propose_*` (czat w podglądzie, A3): dopisuje propozycję do
 * kolejki jak serwer MCP i powiadamia baner. Niczego nie zmienia w danych.
 */
export async function mcpPropose(
  proposal: Partial<McpProposal> & Pick<McpProposal, "kind">,
): Promise<McpProposal> {
  const list = await mcpProposals();
  // Numery rosną jak w Rust (`Queue::next`): rozpatrzona propozycja nie oddaje
  // swojego numeru następnej, bo czat pamięta numery pokazanych kart.
  const id =
    Math.max(
      Number(stored().next) || 0,
      list.reduce((max, p) => Math.max(max, p.id), 0),
    ) + 1;
  const created: McpProposal = {
    id,
    at: Date.now(),
    taskId: null,
    taskTitle: "",
    title: "",
    project: "",
    key: "",
    day: "",
    estimate: 0,
    text: "",
    folder: "",
    ...proposal,
  };
  try {
    localStorage.setItem(
      mcpKey,
      JSON.stringify({ ...stored(), next: id, proposals: [...list, created] }),
    );
  } catch {
    // Bez localStorage kolejka nie zapamięta propozycji.
  }
  window.dispatchEvent(new Event(mcpProposalsEvent));
  return created;
}
