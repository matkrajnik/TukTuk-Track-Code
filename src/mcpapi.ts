/**
 * Komendy serwera MCP (`mcp_*`, A2) z odpowiednikami w podglądzie
 * (`preview/mcp.ts`); osobno, żeby `store.ts` został cienką warstwą.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { McpProposal, McpStatus } from "./api/mcp";
import * as mcpSim from "./preview/mcp";

export * from "./api/mcp";

const desktop = isTauri();

/** `mcp_proposals`: propozycje agentów czekające na „Zastosuj” albo „Odrzuć”. */
export async function mcpProposals(): Promise<McpProposal[]> {
  if (desktop) return invoke("mcp_proposals");
  return mcpSim.mcpProposals();
}
/** `mcp_proposal_resolve`: zdejmuje propozycję z kolejki (po „Odrzuć” i po udanym „Zastosuj”). */
export async function mcpProposalResolve(id: number): Promise<McpProposal[]> {
  if (desktop) return invoke("mcp_proposal_resolve", { id });
  return mcpSim.mcpProposalResolve(id);
}
/** Nasłuch zmian kolejki propozycji (agent wywołał `propose_*`, decyzja w innym oknie). */
export async function onMcpProposals(handler: () => void): Promise<() => void> {
  if (desktop) return listen("ai-proposals-changed", () => handler());
  window.addEventListener(mcpSim.mcpProposalsEvent, handler);
  return () => window.removeEventListener(mcpSim.mcpProposalsEvent, handler);
}
/** `mcp_status`: rejestracja serwera MCP w Claude Code i Codexie (ostatnia synchronizacja). */
export async function mcpStatus(): Promise<McpStatus> {
  if (desktop) return invoke("mcp_status");
  return mcpSim.mcpStatus();
}
/** `mcp_sync`: „Zarejestruj ponownie”, synchronizacja po kliknięciu. */
export async function mcpSync(): Promise<McpStatus> {
  if (desktop) return invoke("mcp_sync");
  return mcpSim.mcpSync();
}
