import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  onSettingsChanged,
  readSettings,
  writeSettings,
  type AppSettings,
} from "./settings";
import {
  change,
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
  /** Lokalne zadanie jest ukończone: bez propozycji i przypisania przerwy. */
  done?: boolean;
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
      done: bound.task.done,
    };
  if (!jiraKey) return null;
  const local = data.tasks
    .filter((t) => t.jiraKey === jiraKey)
    .sort((a, b) => Number(a.done) - Number(b.done) || b.id - a.id)[0];
  return local
    ? {
        taskId: local.id,
        title: local.title,
        key: jiraKey,
        source: "task",
        done: local.done,
      }
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

/** Agent w propozycji (`suggest::Worker`). */
export interface Worker {
  kind: Agent["kind"];
  name: string;
  cwd: string;
  since: number;
}
/** Propozycja przełączenia (`suggest::Suggestion`): zadanie i pracujący nad nim agenci. */
export interface Suggestion {
  task: AgentTask;
  agents: Worker[];
}
/** Tyle musi trwać praca agenta, zanim pojawi się propozycja (`suggest::MIN_WORK`). */
export const MIN_WORK = 2 * 60_000;
const suggestEvent = "agent-suggestions";
/** Odrzucone pary („Nie teraz”) w podglądzie. */
const dismissedKey = "todo-track-preview-agent-dismissed-v1";

/** Cel propozycji jak `suggest::target`: `t:<id>` albo `k:<klucz>`. */
export const suggestionTarget = (task: Pick<AgentTask, "taskId" | "key">) =>
  task.taskId != null ? `t:${task.taskId}` : task.key ? `k:${task.key}` : "";
const pairKey = (kind: string, cwd: string, target: string) =>
  `${kind}|${normFolder(cwd)}|${target}`;

/** Czy folder (albo folder nadrzędny) jest wyciszony (`suggest::muted`). */
export function mutedFolder(folders: string[], cwd: string): boolean {
  const folder = normFolder(cwd);
  return (
    !!folder &&
    folders
      .map(normFolder)
      .some((f) => !!f && (folder === f || folder.startsWith(`${f}/`)))
  );
}

/**
 * Reguła `suggest::Memory::pending`: agent pracuje co najmniej 2 min nad X, timer
 * stoi albo mierzy inne zadanie, folder nie jest wyciszony, para nie odrzucona.
 * Agenci nad tym samym X dają jedną propozycję.
 */
export function pendingSuggestions(
  agents: Agent[],
  activeTaskId: number | null,
  settings: Pick<AppSettings, "agentSuggest" | "agentMutedFolders">,
  dismissed: Set<string>,
  now: number,
): Suggestion[] {
  if (!settings.agentSuggest) return [];
  const out: Suggestion[] = [];
  for (const agent of agents) {
    const task = agent.task;
    if (!task) continue;
    const target = suggestionTarget(task);
    if (
      !target ||
      task.done ||
      now - agent.since < MIN_WORK ||
      (task.taskId != null && task.taskId === activeTaskId) ||
      mutedFolder(settings.agentMutedFolders, agent.cwd) ||
      dismissed.has(pairKey(agent.kind, agent.cwd, target))
    )
      continue;
    const worker = {
      kind: agent.kind,
      name: agent.name,
      cwd: agent.cwd,
      since: agent.since,
    };
    const found = out.find((s) => suggestionTarget(s.task) === target);
    if (found) found.agents.push(worker);
    else out.push({ task, agents: [worker] });
  }
  return out;
}

function previewDismissed(): Set<string> {
  try {
    const value = JSON.parse(localStorage.getItem(dismissedKey) ?? "[]");
    return new Set(Array.isArray(value) ? value : []);
  } catch {
    return new Set();
  }
}

/**
 * Propozycje przełączenia (`agent_suggestions`). W podglądzie liczone z agentów
 * z localStorage; „Nie teraz” trwa tam do wyczyszczenia danych podglądu
 * (agenci podglądu nie kończą pracy).
 */
export async function readSuggestions(): Promise<Suggestion[]> {
  if (desktop) return invoke("agent_suggestions");
  const [agents, data, settings] = await Promise.all([
    previewAgents(),
    read(),
    readSettings(),
  ]);
  const active = data.sessions.find((s) => s.endedAt === null);
  return pendingSuggestions(
    agents,
    active?.taskId ?? null,
    settings,
    previewDismissed(),
    Date.now(),
  );
}

/** „Przełącz” (`agent_switch`): start X, poprzedni pomiar staje. */
export async function switchToSuggestion(task: AgentTask): Promise<Snapshot> {
  if (desktop)
    return invoke("agent_switch", { taskId: task.taskId, key: task.key });
  if (task.taskId == null)
    throw new Error("W podglądzie nie ma listy zgłoszeń z Jiry.");
  const data = await change("start", task.taskId);
  window.dispatchEvent(new Event(suggestEvent));
  return data;
}

/** „Nie teraz” (`agent_dismiss`): cisza dla par (agent, X) do końca pracy agenta. */
export async function dismissSuggestion(
  suggestion: Suggestion,
): Promise<Suggestion[]> {
  if (desktop)
    return invoke("agent_dismiss", {
      taskId: suggestion.task.taskId,
      key: suggestion.task.key,
    });
  const target = suggestionTarget(suggestion.task);
  const dismissed = previewDismissed();
  for (const agent of await previewAgents())
    if (agent.task && suggestionTarget(agent.task) === target)
      dismissed.add(pairKey(agent.kind, agent.cwd, target));
  localStorage.setItem(dismissedKey, JSON.stringify([...dismissed]));
  window.dispatchEvent(new Event(suggestEvent));
  return readSuggestions();
}

/** „Nie proponuj dla tego folderu”: dopisuje foldery agentów do ustawień. */
export async function muteSuggestionFolders(
  suggestion: Suggestion,
): Promise<void> {
  const settings = await readSettings();
  await writeSettings({
    agentMutedFolders: [
      ...settings.agentMutedFolders,
      ...suggestion.agents.map((a) => a.cwd),
    ],
  });
}

/**
 * Nasłuch zmian propozycji: zdarzenie Rust `agent-suggestions`, a w podglądzie
 * zmiany agentów, ustawień i „Nie teraz”.
 */
export async function onSuggestionsChanged(
  handler: () => void,
): Promise<() => void> {
  const stops: (() => void)[] = [];
  if (desktop) stops.push(await listen(suggestEvent, handler));
  else {
    window.addEventListener(suggestEvent, handler);
    stops.push(() => window.removeEventListener(suggestEvent, handler));
    stops.push(await onAgentsChanged(handler));
  }
  stops.push(await onSettingsChanged(handler));
  return () => stops.forEach((stop) => stop());
}
