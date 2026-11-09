/**
 * Dziennik agentów Workflows (E8.1): czytelne polskie linie z komunikatów i
 * narzędzi runnera. To tylko prezentacja: allowlistę argumentów (bez promptów
 * i treści plików) stosuje host w `runners/activity.rs`, więc tu nie ma logiki
 * przeniesionej 1:1 z Rust i nie renderujemy żadnych kluczy spoza listy.
 */
import type { ExecutionStatus, StoredEvent } from "./workflowRuns";

/** Najdłuższa linia logu w znakach; reszta jest ucinana z wielokropkiem. */
export const MAX_LOG_LINE_CHARS = 2_000;

/** Argumenty narzędzia zapisane przez hosta (allowlista z `activity.rs`). */
export interface ToolArgs {
  file_path?: string;
  pattern?: string;
  path?: string;
  glob?: string;
  description?: string;
  subagent_type?: string;
  /** Ścieżka spoza katalogu roboczego: host zostawia tylko nazwę pliku. */
  outside?: boolean;
}

export interface ActivityLine {
  sequence: number;
  text: string;
  kind: "message" | "tool";
  /** `parent_tool_use_id` linii subagenta albo `null` dla głównego agenta. */
  parent: string | null;
}

/**
 * Dane komunikatu albo narzędzia runnera ze zdarzenia strumienia. Host zapisuje
 * w `StoredEvent.payload` cały `RunEvent` (`ports.rs`), więc `Message` i `Tool`
 * mają właściwe dane dopiero w `payload.payload` (`{"type":"message",
 * "executionId":…,"payload":{"text":…}}`). Płaski kształt (`payload.text`) to
 * zapas na dane sprzed tej poprawki; nigdy nie pochodzi z hosta.
 */
export function runnerPayload(
  event: Pick<StoredEvent, "payload">,
): Record<string, unknown> {
  const nested = event.payload.payload;
  if (nested && typeof nested === "object" && !Array.isArray(nested))
    return nested as Record<string, unknown>;
  return event.payload;
}

/**
 * Linia dziennika agentów z komunikatu runnera: tekst, a dla znacznika
 * przycięcia (`driver/messages.rs`: `truncated` z `originalBytes` albo `reason`)
 * czytelna uwaga zamiast JSON-u.
 */
export function messageLine(event: Pick<StoredEvent, "payload">): string {
  const data = runnerPayload(event);
  if (typeof data.text === "string") return data.text;
  if (data.truncated) {
    const detail =
      typeof data.originalBytes === "number"
        ? `${data.originalBytes} B`
        : String(data.reason ?? "limit");
    return `[komunikat przycięty: ${detail}]`;
  }
  return JSON.stringify(data);
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** Zawęża dowolną wartość z zapisu do znanych kluczy (string i boolean). */
function toolArgs(value: unknown): ToolArgs | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const raw = value as Record<string, unknown>;
  const args: ToolArgs = {};
  for (const key of [
    "file_path",
    "pattern",
    "path",
    "glob",
    "description",
    "subagent_type",
  ] as const) {
    const v = text(raw[key]);
    if (v) args[key] = v;
  }
  if (raw.outside === true) args.outside = true;
  return args;
}

/** Czytelny opis czynności narzędzia („Czyta src/a.ts”). */
export function describeTool(
  name: string,
  args: ToolArgs | null,
  ok?: boolean,
): string {
  const a = args ?? {};
  let line: string;
  switch (name) {
    case "Read":
      line = a.file_path ? `Czyta ${a.file_path}` : "Czyta plik";
      break;
    case "Edit":
      line = a.file_path ? `Edytuje ${a.file_path}` : "Edytuje plik";
      break;
    case "Write":
      line = a.file_path ? `Zapisuje ${a.file_path}` : "Zapisuje plik";
      break;
    case "Grep":
      line = a.pattern
        ? `Szuka „${a.pattern}”${a.path ? ` w ${a.path}` : ""}`
        : "Szuka w plikach";
      break;
    case "Glob":
      line = a.pattern
        ? `Szuka plików „${a.pattern}”${a.path ? ` w ${a.path}` : ""}`
        : "Szuka plików";
      break;
    case "Agent":
    case "Task":
      line = a.description
        ? `Uruchamia subagenta: ${a.description}`
        : "Uruchamia subagenta";
      break;
    default:
      line = `Narzędzie ${name}`;
  }
  if (a.outside) line += " (poza katalogiem roboczym)";
  if (ok === false) line += " — błąd";
  return line;
}

function clip(line: string): string {
  return line.length > MAX_LOG_LINE_CHARS
    ? `${line.slice(0, MAX_LOG_LINE_CHARS)}…`
    : line;
}

interface ToolEntry {
  sequence: number;
  name: string;
  args: ToolArgs | null;
  parent: string | null;
  executionId: string | null;
  toolUseId: string;
  failed: boolean;
  finished: boolean;
}

type Entry =
  | { kind: "message"; sequence: number; text: string; parent: string | null }
  | { kind: "tool"; tool: ToolEntry }
  | { kind: "orphan"; sequence: number; text: string; parent: string | null };

/**
 * Ostatni niezakończony start bez id o tej samej nazwie w tym samym kroku.
 * Odpowiednik stosu zapasowego runnera (`runners/mod.rs`): sparowane przez
 * hosta zakończenie niesie nazwę startu, a pusta nazwa znaczy, że host nie
 * znalazł startu, więc niczego nie paruje.
 */
function lastUnfinished(
  unnamed: readonly ToolEntry[],
  name: string,
  executionId: string | null,
): ToolEntry | undefined {
  for (let i = unnamed.length - 1; i >= 0; i--) {
    const candidate = unnamed[i];
    if (
      !candidate.finished &&
      candidate.name === name &&
      candidate.executionId === executionId
    )
      return candidate;
  }
  return undefined;
}

/**
 * Dziennik z zapisanych zdarzeń: komunikaty i narzędzia chronologicznie.
 * Koniec narzędzia nie ma własnej linii, tylko oznacza sparowany start
 * (po `toolUseId`; gdy id brak albo jest nieznane, jak w starej historii lub
 * przy starcie bez id: ostatni niezakończony start bez id tej samej nazwy
 * w tym samym kroku). Funkcja nie zmienia wejścia.
 */
export function buildActivityLog(events: readonly StoredEvent[]): ActivityLine[] {
  const entries: Entry[] = [];
  const byId = new Map<string, ToolEntry>();
  const unnamed: ToolEntry[] = [];
  const sorted = [...events].sort((a, b) => a.sequence - b.sequence);
  for (const e of sorted) {
    if (e.type === "message") {
      const data = runnerPayload(e);
      entries.push({
        kind: "message",
        sequence: e.sequence,
        text: messageLine(e),
        parent: text(data.parentToolUseId) || null,
      });
    } else if (e.type === "toolStarted") {
      const data = runnerPayload(e);
      const tool: ToolEntry = {
        sequence: e.sequence,
        name: text(data.name),
        args: toolArgs(data.args),
        parent: text(data.parentToolUseId) || null,
        executionId: e.executionId,
        toolUseId: text(data.toolUseId),
        failed: false,
        finished: false,
      };
      entries.push({ kind: "tool", tool });
      if (tool.toolUseId) byId.set(tool.toolUseId, tool);
      else unnamed.push(tool);
    } else if (e.type === "toolFinished") {
      const data = runnerPayload(e);
      const name = text(data.name);
      const id = text(data.toolUseId);
      const start =
        (id ? byId.get(id) : undefined) ??
        lastUnfinished(unnamed, name, e.executionId);
      const failed = data.ok === false;
      if (start && !start.finished) {
        start.finished = true;
        start.failed = failed;
      } else if (failed) {
        entries.push({
          kind: "orphan",
          sequence: e.sequence,
          text: name ? `Narzędzie ${name} — błąd` : "Narzędzie — błąd",
          parent: text(data.parentToolUseId) || null,
        });
      }
    }
  }
  return entries.map((entry): ActivityLine => {
    if (entry.kind === "tool") {
      const t = entry.tool;
      return {
        sequence: t.sequence,
        text: clip(describeTool(t.name, t.args, t.failed ? false : undefined)),
        kind: "tool",
        parent: t.parent,
      };
    }
    return {
      sequence: entry.sequence,
      text: clip(entry.text),
      kind: entry.kind === "message" ? "message" : "tool",
      parent: entry.parent,
    };
  });
}

// ---------------------------------------------------------------------------
// Drzewo orkiestrator → subagenci (E8.6)

/** Nazwy narzędzia delegowania: `tool_use` nosi `Agent`, a stare dane i `init` mają `Task`. */
export const SUBAGENT_TOOLS: readonly string[] = ["Agent", "Task"];

/** `interrupted`: krok skończył się, zanim tor dostał własny wynik (anulowanie, limit, zabicie). */
export type LaneState = "running" | "done" | "failed" | "interrupted";

/** Stany kroku, po których żaden tor nie może już być „w toku”. */
const FINAL_EXECUTION: Readonly<Partial<Record<ExecutionStatus, LaneState>>> = {
  completed: "done",
  failed: "failed",
  rejected: "failed",
  cancelled: "interrupted",
  interrupted: "interrupted",
  skipped: "interrupted",
};

/** Jeden tor drzewa: orkiestrator albo subagent. */
export interface Lane {
  id: string;
  label: string;
  /** Opis ostatniego narzędzia toru (z „— błąd”, gdy się nie udało) albo `null`. */
  current: string | null;
  state: LaneState;
  lines: ActivityLine[];
}

export interface ActivityTree {
  orchestrator: Lane;
  subagents: Lane[];
  /** Host zapisał znacznik limitu zdarzeń kroku: dalsze czynności nie są widoczne. */
  truncated: boolean;
}

function lastTool(lines: readonly ActivityLine[]): string | null {
  for (let i = lines.length - 1; i >= 0; i--)
    if (lines[i].kind === "tool") return lines[i].text;
  return null;
}

/**
 * Drzewo z zapisanych zdarzeń jednego kroku (`executionId`). Subagent to start
 * narzędzia Agent/Task bez rodzica; linie z `parentToolUseId` trafiają do jego toru,
 * a nieznany rodzic jest spłaszczany do orkiestratora (bez wyjątku). Koniec to
 * `toolFinished` z id subagenta: host zapisuje go z `tool_result` subagenta
 * synchronicznego albo z `system/task_notification` subagenta w tle (sam start
 * w tle, „Async agent launched”, nie jest końcem; `runners/mod.rs`). Gdy krok jest już w stanie końcowym (`status`),
 * tor bez własnego wyniku jest `interrupted`, a stan orkiestratora wynika ze
 * statusu kroku; bez statusu orkiestrator jest „w toku”. Funkcja czysta.
 */
export function buildActivityTree(
  events: readonly StoredEvent[],
  executionId: string,
  status?: ExecutionStatus,
): ActivityTree {
  const finalState = status ? FINAL_EXECUTION[status] : undefined;
  const own = events
    .filter((e) => e.executionId === executionId)
    .sort((a, b) => a.sequence - b.sequence);
  const order: string[] = [];
  const meta = new Map<string, { label: string; state: LaneState }>();
  let truncated = false;
  for (const e of own) {
    const data = runnerPayload(e);
    if (e.type === "message" && data.truncated === true && data.reason !== undefined)
      truncated = true;
    if (e.type === "toolStarted") {
      const id = text(data.toolUseId);
      const name = text(data.name);
      if (SUBAGENT_TOOLS.includes(name) && id && !text(data.parentToolUseId) && !meta.has(id)) {
        order.push(id);
        const description = toolArgs(data.args)?.description;
        meta.set(id, {
          label: description ?? `Subagent ${order.length}`,
          state: "running",
        });
      }
    } else if (e.type === "toolFinished") {
      const found = meta.get(text(data.toolUseId));
      if (found) found.state = data.ok === false ? "failed" : "done";
    }
  }
  const grouped = new Map<string, ActivityLine[]>();
  const orchestratorLines: ActivityLine[] = [];
  for (const line of buildActivityLog(own)) {
    const parent = line.parent;
    if (parent && meta.has(parent)) {
      const list = grouped.get(parent) ?? [];
      list.push(line);
      grouped.set(parent, list);
    } else orchestratorLines.push(line);
  }
  const subagents = order.map((id): Lane => {
    const lines = grouped.get(id) ?? [];
    const found = meta.get(id) ?? { label: id, state: "running" as LaneState };
    const state = found.state === "running" && finalState ? "interrupted" : found.state;
    return { id, label: found.label, current: lastTool(lines), state, lines };
  });
  return {
    orchestrator: {
      id: "orchestrator",
      label: "Orkiestrator",
      current: lastTool(
        orchestratorLines.filter((l) => !l.text.startsWith("Uruchamia subagenta")),
      ),
      state: finalState ?? "running",
      lines: orchestratorLines,
    },
    subagents,
    truncated,
  };
}
