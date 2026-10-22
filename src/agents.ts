import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  onSettingsChanged,
  readSettings,
  writeSettings,
  type AppSettings,
} from "./settings";
import { previewFocusHolds } from "./focus";
import { issueRef } from "./issue";
import {
  change,
  desktop,
  previewAgentsEvent,
  previewBindings,
  read,
  startAllowed,
  type Snapshot,
} from "./store";
import { dayKey } from "./time";
import { previewHookAgents } from "./preview/hooks";
import * as aiSim from "./preview/ai";
import { ASSISTANT_SOURCE } from "./api/aiassign";

/** Odpowiednik `Match` z src-tauri/src/agents.rs: zadanie, nad którym pracuje agent. */
export interface AgentTask {
  /** Lokalne zadanie; `null`, gdy klucz zna tylko lista zgłoszeń z Jiry. */
  taskId: number | null;
  /** Tytuł zadania albo zgłoszenia (pusty, gdy tytuł zgłoszenia nie jest znany). */
  title: string;
  key: string | null;
  /**
   * `folder` (przypisanie folderu), `task` (klucz lokalnego zadania), `jira`,
   * `redmine` (`#1234` z rozszerzenia przeglądarki, bez zadania; R3),
   * `assistant` (podpowiedź asystenta przypisań, A4: tylko w propozycjach).
   */
  source: "folder" | "task" | "jira" | "redmine" | "assistant";
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
  /**
   * Podpowiedź asystenta przypisań (A4) dla sesji bez dopasowania. Tylko w podglądzie:
   * Rust trzyma ją poza listą agentów i używa w propozycjach (`Agent::assist`).
   */
  assist?: AgentTask | null;
  /** Pierwszy prompt sesji (tylko w podglądzie, razem z `assist`; Rust trzyma go w pamięci sesji). */
  firstPrompt?: string | null;
}
/**
 * Agent w podglądzie (localStorage, testy): jak `Agent`, a zamiast wyniku
 * rozpoznania podaje gałąź i nazwę sesji (`session`); klucz i zadanie liczy
 * `recognize` tak jak Rust.
 */
type PreviewAgent = Agent & {
  session?: string | null;
  /** Identyfikator sesji (z hooków); bez niego klucz to rodzaj, folder i początek pracy. */
  sessionId?: string;
  /** Pierwszy prompt sesji (z hooków przy włączonym asystencie albo z testu). */
  firstPrompt?: string | null;
};
/** W podglądzie lista agentów pochodzi z localStorage (testy). */
const key = "tuktuk-track-preview-agents-v1";
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
      // Klucz tylko do wyświetlenia: z Jiry albo `#numer` z Redmine (jak `agents::bound_task`).
      key: issueRef(bound.task)?.key ?? null,
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
  const settings = await readSettings();
  // Pierwszy prompt sesji tylko dla asystenta przypisań (`assign::keep_prompts`).
  const keepPrompts =
    (settings.aiEngine === "claude" || settings.aiEngine === "codex") &&
    settings.aiAssignSuggest;
  // Zdarzenia hooków z fixture przechodzą przez tę samą maszynę stanów co w Rust.
  list = [...list, ...previewHookAgents(Date.now(), keepPrompts)];
  const data = await read();
  const bindings = previewBindings();
  return Promise.all(
    list.map(async ({ session, sessionId, firstPrompt, ...agent }) => {
      const branch = agent.branch ?? null;
      const detached = !!agent.detached;
      const jiraKey =
        (detached ? null : jiraKeyIn(branch)) ?? jiraKeyIn(session ?? null);
      const task = matchAgentTask(data, bindings, agent.cwd, jiraKey);
      const assist = task
        ? null
        : await assistFor(
            { ...agent, branch, detached },
            sessionId,
            keepPrompts || firstPrompt ? (firstPrompt ?? null) : null,
            data,
            settings,
          );
      return {
        ...agent,
        branch,
        detached,
        key: jiraKey,
        task,
        ...(assist ? { assist, firstPrompt: firstPrompt ?? null } : {}),
      };
    }),
  );
}

/**
 * Podpowiedź asystenta przypisań (`assign::apply`) dla sesji bez dopasowania
 * heurystyką: sesja pracuje co najmniej `MIN_WORK`, folder nie jest wyciszony,
 * włączone są propozycje agentów i podpowiedzi przypisań, wybrany silnik jest
 * wykryty. Jedno wywołanie na sesję (`preview/ai.ts`); zadanie z listy otwartych
 * przy pewności co najmniej 0,6.
 */
async function assistFor(
  agent: Agent,
  sessionId: string | undefined,
  firstPrompt: string | null,
  data: Snapshot,
  settings: AppSettings,
): Promise<AgentTask | null> {
  if (
    !settings.aiAssignSuggest ||
    !settings.agentSuggest ||
    (agent.kind !== "claude" && agent.kind !== "codex") ||
    Date.now() - agent.since < MIN_WORK ||
    mutedFolder(settings.agentMutedFolders, agent.cwd) ||
    !(await aiSim.aiStatus()).active
  )
    return null;
  const id = await aiSim.aiAssign(
    `${agent.kind}|${sessionId ?? `${agent.cwd}|${agent.since}`}`,
    {
      folder: agent.cwd.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "",
      branch: agent.detached ? null : (agent.branch ?? null),
      prompt: firstPrompt,
    },
    await aiSim.openAssignTasks(),
  );
  const task = id == null ? undefined : data.tasks.find((t) => t.id === id);
  return task
    ? {
        taskId: task.id,
        title: task.title,
        key: issueRef(task)?.key ?? null,
        source: ASSISTANT_SOURCE,
        done: task.done,
      }
    : null;
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

/**
 * Źródło sygnału zamiast agenta AI (`foreground::WINDOW`, `foreground::BROWSER`):
 * aktywne okno (W3) albo rozszerzenie przeglądarki (W4).
 */
export type SignalKind = "window" | "browser";
/** Czy `kind` to sygnał z okna albo przeglądarki (`foreground::is_signal`). */
export const isSignal = (kind: string): kind is SignalKind =>
  kind === "window" || kind === "browser";
/** Nazwa źródła w interfejsie (`suggest::source_label`). */
export const sourceLabel = (kind: string) =>
  kind === "window" ? "Okno" : kind === "browser" ? "Przeglądarka" : "Agent";
/**
 * Pytanie w propozycji z samego sygnału (`suggest::signal_title`): z rozszerzenia
 * przeglądarki z kluczem „Pracujesz teraz przy WMS-123?”, inaczej ogólne.
 */
export const signalTitle = (s: Suggestion) =>
  s.task.key && s.agents.some((a) => a.kind === "browser")
    ? `Pracujesz teraz przy ${s.task.key}?`
    : "Pracujesz teraz przy innym zadaniu?";
/** Propozycja tylko z sygnału okna albo przeglądarki (`suggest::signal_only`). */
export const signalOnly = (s: Suggestion) =>
  s.agents.length > 0 && s.agents.every((a) => isSignal(a.kind));

/** Początek promptu do banera (`suggest::hint_of`): białe znaki zwinięte, do 80 znaków. */
export function promptHint(prompt: string | null | undefined): string | null {
  const line = (prompt ?? "").split(/\s+/).filter(Boolean).join(" ");
  if (!line) return null;
  const chars = [...line];
  return chars.length > 80 ? `${chars.slice(0, 80).join("")}…` : line;
}

/** Agent w propozycji (`suggest::Worker`); sygnał z okna ma `kind` źródła. */
export interface Worker {
  kind: Agent["kind"] | SignalKind;
  name: string;
  cwd: string;
  since: number;
  /** Początek pierwszego promptu sesji (tylko w propozycji asystenta, A4). */
  hint?: string;
}
/** Propozycja przełączenia (`suggest::Suggestion`): zadanie i pracujący nad nim agenci. */
export interface Suggestion {
  task: AgentTask;
  agents: Worker[];
}
/** Tyle musi trwać praca agenta, zanim pojawi się propozycja (`suggest::MIN_WORK`). */
export const MIN_WORK = 2 * 60_000;
const suggestEvent = "agent-suggestions";
/** Podgląd: propozycje do przeliczenia (jak `suggest::refresh` po zmianie stanu dnia). */
export function previewSuggestionsChanged() {
  window.dispatchEvent(new Event(suggestEvent));
}
/** Odrzucone pary („Nie teraz”) w podglądzie. */
const dismissedKey = "tuktuk-track-preview-agent-dismissed-v1";

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
  settings: Pick<
    AppSettings,
    "agentSuggest" | "agentMutedFolders" | "windowSuggest" | "browserExtension"
  >,
  dismissed: Set<string>,
  now: number,
): Suggestion[] {
  const out: Suggestion[] = [];
  for (const agent of agents) {
    // Dopasowanie heurystyką, a bez niego podpowiedź asystenta (`suggest::goal_match`).
    const task = agent.task ?? agent.assist;
    if (!task) continue;
    const target = suggestionTarget(task);
    // Osobne wyłączniki agentów, okna i przeglądarki (`foreground::enabled`),
    // wspólna reguła.
    const kind: string = agent.kind;
    const enabled =
      kind === "window"
        ? settings.windowSuggest
        : kind === "browser"
          ? settings.browserExtension
          : settings.agentSuggest;
    if (
      !target ||
      !enabled ||
      task.done ||
      now - agent.since < MIN_WORK ||
      (task.taskId != null && task.taskId === activeTaskId) ||
      mutedFolder(settings.agentMutedFolders, agent.cwd) ||
      dismissed.has(pairKey(agent.kind, agent.cwd, target))
    )
      continue;
    const hint =
      task.source === ASSISTANT_SOURCE ? promptHint(agent.firstPrompt) : null;
    const worker: Worker = {
      kind: agent.kind,
      name: agent.name,
      cwd: agent.cwd,
      since: agent.since,
      ...(hint ? { hint } : {}),
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
  // W czasie bloku skupienia propozycje czekają (jak `suggest::refresh`).
  if (previewFocusHolds()) return [];
  const [agents, data, settings] = await Promise.all([
    previewAgents(),
    read(),
    readSettings(),
  ]);
  // Po „Zakończ dzień” propozycje milczą do północy (`Memory::proposals`, B13).
  const today = dayKey(Date.now());
  if (data.dayMarks?.some((m) => m.day === today && m.endedAt != null))
    return [];
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
  if (desktop) {
    // Po „Zakończ dzień” najpierw „Wznowić dzień?” (B13); podgląd pyta w `change`.
    if (!(await startAllowed(task.taskId))) return read();
    return invoke("agent_switch", { taskId: task.taskId, key: task.key });
  }
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
  for (const agent of await previewAgents()) {
    const goal = agent.task ?? agent.assist;
    if (goal && suggestionTarget(goal) === target)
      dismissed.add(pairKey(agent.kind, agent.cwd, target));
  }
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
      ...suggestion.agents.map((a) => a.cwd).filter(Boolean),
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
