/**
 * Serwer MCP dla agentów (PLAN_BACKLOG.md, A2): typy komend `mcp_*` i teksty
 * propozycji wspólne dla aplikacji i podglądu (symulator w `preview/mcp.ts`).
 */

/** Rodzaj propozycji (`mcp::Proposal::kind`). */
export type McpProposalKind =
  | "task_create"
  | "note_append"
  | "link_folder"
  | "switch"
  | "plan";

/** Propozycja agenta czekająca na „Zastosuj” albo „Odrzuć” (`mcp::Proposal`). */
export interface McpProposal {
  id: number;
  kind: McpProposalKind;
  /** Chwila powstania (ms). */
  at: number;
  taskId: number | null;
  /** Tytuł zadania z chwili propozycji. */
  taskTitle: string;
  title: string;
  project: string;
  key: string;
  day: string;
  /** Szacunek w minutach, 0 = brak. */
  estimate: number;
  text: string;
  folder: string;
}

/** Stan rejestracji serwera w jednym CLI (`mcp_register::Entry`). */
export interface McpEntry {
  /** `claude` albo `codex`. */
  engine: string;
  name: string;
  /** `registered`, `missing`, `error`, `unavailable` (brak CLI). */
  state: string;
  detail: string;
}

/** Stan serwera MCP do Ustawień (`mcp::Status`). */
export interface McpStatus {
  exe: string;
  exeFound: boolean;
  /** Serwer ma być zarejestrowany: przełącznik `aiMcp` i wybrany silnik. */
  enabled: boolean;
  entries: McpEntry[];
}

/** Zdanie opisujące propozycję w banerze (po polsku, z pytaniem). */
export function proposalText(p: McpProposal): string {
  const task = `„${p.taskTitle}”`;
  switch (p.kind) {
    case "task_create": {
      const parts = [`Utworzyć zadanie „${p.title}”`];
      if (p.project) parts.push(`w projekcie ${p.project}`);
      if (p.key) parts.push(`powiązane z ${p.key}`);
      if (p.day) parts.push(`zaplanowane na ${p.day}`);
      if (p.estimate) parts.push(`z szacunkiem ${p.estimate} min`);
      return `${parts.join(", ")}?`;
    }
    case "note_append":
      return `Dopisać do notatki dnia ${p.day} zadania ${task}: „${p.text}”?`;
    case "link_folder":
      return `Przypisać folder ${p.folder} do zadania ${task}?`;
    case "switch":
      return `Przełączyć pomiar na ${task}?`;
    case "plan":
      return `Zaplanować ${task} na ${p.day}?`;
  }
}

/** Krótka nazwa do etykiet przycisków. */
export function proposalName(p: McpProposal): string {
  return p.taskTitle || p.title || "propozycja";
}

/** Stan rejestracji jednym zdaniem (Ustawienia → Asystent AI). */
export function mcpEntryLine(e: McpEntry): string {
  switch (e.state) {
    case "registered":
      return `${e.name}: zarejestrowany`;
    case "missing":
      return `${e.name}: niezarejestrowany`;
    case "unavailable":
      return `${e.name}: nie znaleziono`;
    default:
      return `${e.name}: błąd rejestracji${e.detail ? ` (${e.detail})` : ""}`;
  }
}
