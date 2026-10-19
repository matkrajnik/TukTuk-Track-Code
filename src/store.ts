import { invoke, isTauri } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { ref } from "vue";
import { renderTitle, type Parsed } from "./quickadd";
import { exportCsv, exportRows } from "./export";
import { agentTotals, type AgentPeriod, type AgentTime } from "./agenttime";
import {
  emptyRule,
  isRecurring,
  normalizeRule,
  occurs,
  ruleUsesWeekdays,
  type RepeatRule,
} from "./recurrence";
export interface Task {
  id: number;
  title: string;
  done: boolean;
  createdAt: number;
  jiraKey?: string | null;
  jiraUrl?: string | null;
  /** Ostatni znany status zgłoszenia w Jirze. */
  jiraStatus?: string | null;
  details?: TaskDetails;
  archived?: boolean;
}
export interface TaskDetails {
  priority: number;
  plannedDay: string;
  dueDay: string;
  project: string;
  waiting: string;
  notes: Record<string, string>;
  estimateMinutes: number;
  checklist: { text: string; done: boolean }[];
  repeatDays: number[];
  repeatFrom: string;
  /** Ręczna kolejność w paśmie priorytetu (etap 3); brak w starych danych = 0. */
  order: number;
  /** Własna godzina startu bloku planu: pusta albo „HH:MM”. */
  plannedStart: string;
  /** Reguła cyklu (P3); brak albo pusty rodzaj = co tydzień w `repeatDays`. */
  repeatRule?: RepeatRule;
  /** Chwila zapisania blokady „Czekam na” (ms, 0 = brak albo stary zapis). */
  waitingSince?: number;
  /** Status Jiry w chwili blokady (propozycja „odblokować?”). */
  waitingStatus?: string;
  /** Status Jiry, dla którego propozycja została odrzucona albo obsłużona. */
  waitingDismissed?: string;
  /** „Przypomnij jutro” z powiadomienia: dzień kolejnego przypomnienia. */
  waitingRemindDay?: string;
  /** „Nie pytaj dla tego zadania” o przesunięcie zgłoszenia w Jirze (J1). */
  jiraNoAsk?: boolean;
  /** Przypięcie do widgetu („w toku”): chwila przypięcia (ms), 0 = nieprzypięte. */
  pinnedAt?: number;
}
const validTime = (value: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
export const detailsOf = (task: Task): TaskDetails => ({
  priority: 0,
  plannedDay: "",
  dueDay: "",
  project: "",
  waiting: "",
  estimateMinutes: 0,
  repeatFrom: "",
  order: 0,
  plannedStart: "",
  waitingSince: 0,
  waitingStatus: "",
  waitingDismissed: "",
  waitingRemindDay: "",
  ...task.details,
  notes: { ...task.details?.notes },
  checklist: (task.details?.checklist ?? []).map((item) => ({ ...item })),
  repeatDays: [...(task.details?.repeatDays ?? [])],
  repeatRule: { ...emptyRule(), ...task.details?.repeatRule },
});
/**
 * Szczegóły po zdjęciu blokady przy starcie pomiaru (`planner::clear_waiting`):
 * bieżący status Jiry trafia do `waitingDismissed`.
 */
function unblocked(task: Task): TaskDetails {
  const d = detailsOf(task);
  if (!d.waiting) return d;
  return {
    ...d,
    waiting: "",
    waitingSince: 0,
    waitingStatus: "",
    waitingRemindDay: "",
    waitingDismissed: task.jiraStatus ?? "",
  };
}
export interface Session {
  id: number;
  taskId: number;
  startedAt: number;
  endedAt: number | null;
}
export interface Snapshot {
  tasks: Task[];
  sessions: Session[];
  trash?: Task[];
  trashSessions?: Session[];
  closedDays?: string[];
  recurrenceState?: Record<string, string>;
}
export const desktop = isTauri();
/**
 * Czy Jira jest połączona: tylko wtedy szybkie dodawanie rozpoznaje klucz
 * zgłoszenia. W podglądzie Jira udaje połączoną (dane przykładowe mają zgłoszenia,
 * a `quickAdd` wiąże klucz z adresem domyślnym).
 */
export const jiraConnected = ref(!desktop);
/** Odświeża `jiraConnected` z `jira_settings` (w podglądzie nic nie robi). */
export async function refreshJiraConnected(): Promise<void> {
  if (!desktop) return;
  try {
    jiraConnected.value = (
      await invoke<{ connected: boolean }>("jira_settings")
    ).connected;
  } catch {
    jiraConnected.value = false;
  }
}
const key = "todo-track-preview-v1";
export async function read(): Promise<Snapshot> {
  if (desktop) return invoke("get_snapshot");
  const raw = localStorage.getItem(key);
  const data: Snapshot = raw ? JSON.parse(raw) : { tasks: [], sessions: [] };
  if (generateRecurring(data)) localStorage.setItem(key, JSON.stringify(data));
  return data;
}
export async function change(
  action: string,
  id: number | null = null,
  title: string | null = null,
): Promise<Snapshot> {
  if (desktop) return invoke("mutate", { action, id, title });
  const data = await read();
  const now = Date.now();
  const task = data.tasks.find((t) => t.id === id);
  const stop = () =>
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
  if (["rename", "toggle", "delete", "start"].includes(action) && !task)
    throw new Error("Nie znaleziono zadania.");
  if (action === "add" || action === "rename") {
    title = title?.trim() ?? "";
    if (!title || [...title].length > 240)
      throw new Error("Tytuł musi mieć od 1 do 240 znaków.");
    if (task) task.title = title;
    else
      data.tasks.unshift({
        id:
          Math.max(
            0,
            ...data.tasks.map((t) => t.id),
            ...(data.trash ?? []).map((t) => t.id),
          ) + 1,
        title,
        done: false,
        createdAt: now,
      });
  } else if (action === "start" && task) {
    if (task.done) throw new Error("Najpierw przywróć zadanie do zrobienia.");
    task.details = unblocked(task);
    task.archived = false;
    if (!data.sessions.some((s) => s.taskId === id && s.endedAt === null)) {
      stop();
      data.sessions.unshift({
        id:
          Math.max(
            0,
            ...data.sessions.map((s) => s.id),
            ...(data.trashSessions ?? []).map((s) => s.id),
          ) + 1,
        taskId: task.id,
        startedAt: now,
        endedAt: null,
      });
    }
  } else if (action === "stop") stop();
  else if (action === "toggle" && task) {
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(now, s.startedAt);
    });
    task.done = !task.done;
  } else if (action === "delete") {
    if (task) {
      data.sessions.forEach((s) => {
        if (s.taskId === id && s.endedAt === null)
          s.endedAt = Math.max(s.startedAt, now);
      });
      (data.trash ??= []).push(task);
      (data.trashSessions ??= []).push(
        ...data.sessions.filter((s) => s.taskId === id),
      );
    }
    data.tasks = data.tasks.filter((t) => t.id !== id);
    data.sessions = data.sessions.filter((s) => s.taskId !== id);
  } else if (action === "restore") {
    const restored = data.trash?.find((t) => t.id === id);
    if (!restored) throw Error("Nie znaleziono zadania w koszu.");
    data.tasks.unshift(restored);
    data.trash = data.trash?.filter((t) => t.id !== id);
    data.sessions.push(
      ...(data.trashSessions ?? []).filter((s) => s.taskId === id),
    );
    data.trashSessions = data.trashSessions?.filter((s) => s.taskId !== id);
  } else if ((action === "archive" || action === "unarchive") && task) {
    task.archived = action === "archive";
    if (task.archived)
      data.sessions.forEach((s) => {
        if (s.taskId === id && s.endedAt === null)
          s.endedAt = Math.max(now, s.startedAt);
      });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Ładunek `quick_add` (`quickadd::NewTask` w Rust). */
export interface NewTask {
  title: string;
  jiraKey?: string | null;
  details?: TaskDetails | null;
  /** Utwórz i rozpocznij: poprzedni pomiar staje. */
  start?: boolean;
  /** `/nazwa`: szablon; jawne pola z linii wygrywają z szablonem. */
  template?: string | null;
}
/** Ładunek `quick_add` z wyniku parsera: szczegóły tylko, gdy podano pola planu. */
export function quickTask(parsed: Parsed, start = false): NewTask {
  const f = parsed.fields;
  const planned =
    f.estimateMinutes != null ||
    !!f.plannedDay ||
    !!f.dueDay ||
    f.priority != null ||
    !!f.project;
  return {
    title: parsed.title,
    jiraKey: f.jiraKey ?? null,
    details: planned
      ? {
          ...detailsOf({ id: 0, title: "", done: false, createdAt: 0 }),
          estimateMinutes: f.estimateMinutes ?? 0,
          plannedDay: f.plannedDay ?? "",
          dueDay: f.dueDay ?? "",
          priority: f.priority ?? 0,
          project: f.project ?? "",
        }
      : null,
    start,
    template: f.template ?? null,
  };
}
/** Adres Jiry w podglądzie (domyślny z `jira::Config`). */
const previewJiraSite = "https://asistonwms.atlassian.net";
/**
 * Szybkie dodawanie ze składnią: zadanie z polami, kluczem Jiry i opcjonalnym
 * startem w jednym kroku. Odpowiednik `quickadd::create`; w podglądzie nie ma listy
 * zgłoszeń, więc pusty tytuł z kluczem bierze tytuł z danych `jira_issues` tylko
 * w aplikacji.
 */
export async function quickAdd(task: NewTask): Promise<Snapshot> {
  if (desktop) return invoke("quick_add", { task });
  const jiraKey = task.jiraKey?.trim() || null;
  if (jiraKey && !/^[A-Z0-9_]+-\d+$/.test(jiraKey))
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
  const name = task.template?.trim();
  const template = name ? findTemplate(previewTemplates(), name) : undefined;
  if (name && !template)
    throw new Error(
      `Nie ma szablonu „${name}”. Dodaj go w Ustawieniach → Szablony.`,
    );
  // W podglądzie nie ma listy zgłoszeń, więc {tytuł} z samym kluczem daje błąd.
  const title = template
    ? [...renderTitle(template.title, jiraKey, task.title, () => null)]
        .slice(0, 240)
        .join("")
    : task.title.trim();
  if (!title && jiraKey)
    throw new Error(
      `Nie znam tytułu ${jiraKey}. Wpisz tytuł albo odśwież listę zgłoszeń w widoku Jira.`,
    );
  if (!title || [...title].length > 240)
    throw new Error("Tytuł musi mieć od 1 do 240 znaków.");
  const lineDetails = template
    ? mergeTemplate(task.details ?? null, template)
    : task.details;
  const details = lineDetails
    ? {
        ...JSON.parse(JSON.stringify(lineDetails)),
        project: lineDetails.project.trim(),
        waiting: lineDetails.waiting.trim(),
      }
    : undefined;
  if (
    details &&
    ([details.plannedDay, details.dueDay].some(
      (d: string) =>
        d !== "" &&
        (!/^\d{4}-\d{2}-\d{2}$/.test(d) ||
          localDay(+new Date(d + "T12:00:00")) !== d),
    ) ||
      ![-1, 0, 1].includes(details.priority) ||
      details.estimateMinutes < 0 ||
      details.estimateMinutes > 600_000 ||
      [...details.project].length > 80)
  )
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const data = await read();
  if (jiraKey) {
    const existing = [...data.tasks, ...(data.trash ?? [])].find(
      (t) => t.jiraKey === jiraKey,
    );
    if (existing)
      throw new Error(
        `${jiraKey} ma już zadanie „${existing.title}”${data.trash?.includes(existing) ? " (w koszu)" : ""}.`,
      );
  }
  const now = Date.now();
  const id =
    Math.max(
      0,
      ...data.tasks.map((t) => t.id),
      ...(data.trash ?? []).map((t) => t.id),
    ) + 1;
  data.tasks.unshift({
    id,
    title,
    done: false,
    createdAt: now,
    ...(jiraKey
      ? { jiraKey: jiraKey, jiraUrl: `${previewJiraSite}/browse/${jiraKey}` }
      : {}),
    ...(details ? { details } : {}),
  });
  if (task.start) {
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
    data.sessions.unshift({
      id:
        Math.max(
          0,
          ...data.sessions.map((s) => s.id),
          ...(data.trashSessions ?? []).map((s) => s.id),
        ) + 1,
      taskId: id,
      startedAt: now,
      endedAt: null,
    });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Szablon zadania (`templates::Template` w Rust). */
export interface TaskTemplate {
  id: number;
  /** `/nazwa` w szybkim dodawaniu: litery, cyfry, `-` i `_`, bez spacji. */
  name: string;
  /** Tytuł z polami `{klucz}` i `{tytuł}`. */
  title: string;
  estimateMinutes: number;
  steps: string[];
  project: string;
  priority: number;
}
export type TemplateBody = Omit<TaskTemplate, "id" | "name">;
/** Szablony do podpowiedzi i parsera; odświeżane po każdej zmianie. */
export const templates = ref<TaskTemplate[]>([]);
const templatesKey = "todo-track-preview-templates-v1";
const templateEvent = "templates-changed";
const TEMPLATE_NAME = /^[\p{L}\p{N}_-]{1,60}$/u;
const sortTemplates = (list: TaskTemplate[]) =>
  [...list].sort((a, b) =>
    a.name
      .toLocaleLowerCase("pl")
      .localeCompare(b.name.toLocaleLowerCase("pl"), "pl"),
  );
function previewTemplates(): TaskTemplate[] {
  try {
    return JSON.parse(localStorage.getItem(templatesKey) ?? "[]");
  } catch {
    return [];
  }
}
const findTemplate = (list: TaskTemplate[], name: string) =>
  list.find(
    (t) =>
      t.name.toLocaleLowerCase("pl") === name.trim().toLocaleLowerCase("pl"),
  );
/**
 * Pola szablonu pod pola z linii (`templates::merge_details`): niezerowe pola
 * z linii wygrywają, kroki szablonu stają się listą kontrolną.
 */
export function mergeTemplate(
  line: TaskDetails | null,
  template: TemplateBody,
): TaskDetails | null {
  const has =
    template.estimateMinutes > 0 ||
    !!template.project ||
    template.priority !== 0 ||
    template.steps.length > 0;
  if (!line && !has) return null;
  const details =
    line ?? detailsOf({ id: 0, title: "", done: false, createdAt: 0 });
  return {
    ...details,
    estimateMinutes: details.estimateMinutes || template.estimateMinutes,
    project: details.project.trim() || template.project,
    priority: details.priority || template.priority,
    checklist: details.checklist.length
      ? details.checklist
      : template.steps.map((text) => ({ text, done: false })),
  };
}
/** Szablon z zadania („Zapisz jako szablon” w rozwiniętym kafelku). */
export function templateFromTask(task: Task): TemplateBody {
  const d = detailsOf(task);
  return {
    // Klucz w tytule zadania z Jiry zamienia się w pole {klucz}.
    title: task.jiraKey
      ? task.title.replaceAll(task.jiraKey, "{klucz}")
      : task.title,
    estimateMinutes: d.estimateMinutes,
    steps: d.checklist.map((item) => item.text),
    project: d.project,
    priority: d.priority,
  };
}
/** Walidacja jak `templates::validate`; zwraca oczyszczoną treść. */
function validTemplate(name: string, body: TemplateBody): TemplateBody {
  if (!TEMPLATE_NAME.test(name))
    throw new Error(
      "Nazwa szablonu może mieć od 1 do 60 liter, cyfr, „-” lub „_” (bez spacji).",
    );
  const clean = {
    title: body.title.trim(),
    estimateMinutes: body.estimateMinutes,
    steps: body.steps.map((s) => s.trim()).filter(Boolean),
    project: body.project.trim(),
    priority: body.priority,
  };
  if ([...clean.title].length > 240)
    throw new Error("Tytuł szablonu może mieć najwyżej 240 znaków.");
  if ([...clean.project].length > 80)
    throw new Error("Nazwa projektu może mieć najwyżej 80 znaków.");
  if (
    ![-1, 0, 1].includes(clean.priority) ||
    !Number.isInteger(clean.estimateMinutes) ||
    clean.estimateMinutes < 0 ||
    clean.estimateMinutes > 600_000
  )
    throw new Error("Niepoprawny priorytet lub szacunek szablonu.");
  if (clean.steps.length > 100 || clean.steps.some((s) => [...s].length > 240))
    throw new Error("Szablon może mieć najwyżej 100 kroków po 240 znaków.");
  return clean;
}
function previewTemplatesChanged(list: TaskTemplate[]) {
  localStorage.setItem(templatesKey, JSON.stringify(list));
  window.dispatchEvent(new CustomEvent(templateEvent));
}
/** Lista szablonów (`templates_list`); odświeża też `templates`. */
export async function loadTemplates(): Promise<TaskTemplate[]> {
  const list = desktop
    ? await invoke<TaskTemplate[]>("templates_list")
    : sortTemplates(previewTemplates());
  templates.value = list;
  return list;
}
/** Tworzy (`id` = null) albo zmienia szablon, także nazwę (`template_save`). */
export async function saveTemplate(
  id: number | null,
  name: string,
  body: TemplateBody,
): Promise<TaskTemplate[]> {
  name = name.trim();
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_save", {
      id,
      name,
      template: body,
    });
    return templates.value;
  }
  const clean = validTemplate(name, body);
  const list = previewTemplates();
  const other = findTemplate(list, name);
  if (other && other.id !== id)
    throw new Error(`Szablon „${other.name}” już istnieje.`);
  if (id !== null) {
    const current = list.find((t) => t.id === id);
    if (!current) throw new Error("Nie znaleziono szablonu.");
    Object.assign(current, clean, { name });
  } else
    list.push({
      id: Math.max(0, ...list.map((t) => t.id)) + 1,
      name,
      ...clean,
    });
  previewTemplatesChanged(list);
  return loadTemplates();
}
/** Usuwa szablon (`template_delete`). */
export async function deleteTemplate(id: number): Promise<TaskTemplate[]> {
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_delete", { id });
    return templates.value;
  }
  const list = previewTemplates();
  if (!list.some((t) => t.id === id))
    throw new Error("Nie znaleziono szablonu.");
  previewTemplatesChanged(list.filter((t) => t.id !== id));
  return loadTemplates();
}
/** Folder przypisany do zadania (`agents::Binding`, tabela `agent_bindings`). */
export interface AgentBinding {
  folder: string;
  taskId: number;
  title: string;
}
const bindingsKey = "todo-track-preview-agent-bindings-v1";
/** Zdarzenie podglądu: przypisania folderów zmieniły się, dopasuj agentów od nowa. */
export const previewAgentsEvent = "todo-track-preview-agents";
/** Przypisania w podglądzie: `{ folder, taskId }` z localStorage. */
export function previewBindings(): { folder: string; taskId: number }[] {
  try {
    const value = JSON.parse(localStorage.getItem(bindingsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/** Postać zapisywana (jak `agents::canon_folder` w Windows): separatory `\`, bez końcowego. */
const trimFolder = (folder: string) =>
  folder.trim().replace(/[\\/]/g, "\\").replace(/\\+$/, "");
const sameFolder = (a: string, b: string) =>
  trimFolder(a).toLowerCase() === trimFolder(b).toLowerCase();
async function previewBindingList(): Promise<AgentBinding[]> {
  const data = await read();
  return previewBindings()
    .flatMap((b) => {
      const task = [...data.tasks, ...(data.trash ?? [])].find(
        (t) => t.id === b.taskId,
      );
      return task
        ? [{ folder: b.folder, taskId: b.taskId, title: task.title }]
        : [];
    })
    .sort((a, b) =>
      a.folder.toLowerCase().localeCompare(b.folder.toLowerCase()),
    );
}
function previewBindingsChanged(list: { folder: string; taskId: number }[]) {
  localStorage.setItem(bindingsKey, JSON.stringify(list));
  // Jak `agents::bind`: przypisanie uzupełnia okresy agentów bez zadania.
  previewAgentBackfill(Date.now());
  window.dispatchEvent(new Event(previewAgentsEvent));
}
/** Przypisania folderów do zadań (`agent_bindings`). */
export async function agentBindings(): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bindings");
  return previewBindingList();
}
/** Przypisuje folder agenta do zadania (`agent_bind`); zastępuje wcześniejsze. */
export async function bindAgentFolder(
  folder: string,
  taskId: number,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bind", { folder, taskId });
  const clean = trimFolder(folder);
  if (!clean || [...clean].length > 1024)
    throw new Error("Niepoprawny folder.");
  if (!(await read()).tasks.some((t) => t.id === taskId))
    throw new Error("Nie znaleziono zadania.");
  previewBindingsChanged([
    ...previewBindings().filter((b) => !sameFolder(b.folder, clean)),
    { folder: clean, taskId },
  ]);
  return previewBindingList();
}
/** Okresy pracy agentów w podglądzie (`agent_sessions`), dane z `seed.ts`. */
export const agentSessionsKey = "todo-track-preview-agent-sessions-v1";
function previewAgentPeriods(): AgentPeriod[] {
  try {
    const value = JSON.parse(localStorage.getItem(agentSessionsKey) ?? "[]");
    return Array.isArray(value) ? value : [];
  } catch {
    return [];
  }
}
/**
 * Jak `agent_time::backfill`: okresy z ostatnich 7 dni bez zadania dostają
 * zadanie z przypisania folderu (także podfolderu, wygrywa najdłuższe).
 */
function previewAgentBackfill(now: number) {
  const norm = (f: string) => trimFolder(f).toLowerCase();
  const bindings = previewBindings().map((b) => ({ ...b, norm: norm(b.folder) }));
  let changed = false;
  const periods = previewAgentPeriods().map((p) => {
    if (p.taskId !== null || p.end < now - 7 * 86_400_000) return p;
    const folder = norm(p.folder);
    const found = bindings
      .filter((b) => b.norm && (folder === b.norm || folder.startsWith(b.norm + "\\")))
      .sort((a, b) => b.norm.length - a.norm.length)[0];
    if (!found) return p;
    changed = true;
    return { ...p, taskId: found.taskId };
  });
  if (changed) localStorage.setItem(agentSessionsKey, JSON.stringify(periods));
}
/**
 * Czas agentów w `[from, to)` (`agent_time`): okresy nakładające się na zakres
 * i sumy według zadania z częścią równoległą do Twojego pomiaru.
 */
export async function agentTime(from: number, to: number): Promise<AgentTime> {
  if (desktop) return invoke("agent_time", { from, to });
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const periods = previewAgentPeriods()
    .filter((p) => p.start < to && p.end > from)
    .sort((a, b) => a.start - b.start || a.id - b.id);
  const now = Date.now();
  const { sessions, trashSessions } = await read();
  return {
    periods,
    ...agentTotals(periods, [...sessions, ...(trashSessions ?? [])], from, to, now),
  };
}
/** Usuwa przypisanie folderu (`agent_unbind`). */
export async function unbindAgentFolder(
  folder: string,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_unbind", { folder });
  previewBindingsChanged(
    previewBindings().filter((b) => !sameFolder(b.folder, folder)),
  );
  return previewBindingList();
}
/** Odświeża `templates` po zmianie w innym oknie (np. szybkie przełączanie). */
export async function onTemplatesChanged(): Promise<() => void> {
  const reload = () => void loadTemplates().catch(() => {});
  if (desktop) return listen(templateEvent, reload);
  window.addEventListener(templateEvent, reload);
  return () => window.removeEventListener(templateEvent, reload);
}
export async function editSession(
  id: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
  if (desktop) return invoke("edit_session", { id, startedAt, endedAt });
  if (
    !Number.isSafeInteger(startedAt) ||
    !Number.isSafeInteger(endedAt) ||
    startedAt < 0 ||
    endedAt <= startedAt ||
    endedAt > Date.now()
  )
    throw new Error(
      "Koniec musi być późniejszy niż początek i nie może być w przyszłości.",
    );
  const data = await read();
  const session = data.sessions.find((s) => s.id === id);
  if (!session) throw new Error("Nie znaleziono przedziału.");
  if (session.endedAt === null)
    throw new Error("Zatrzymaj pomiar przed edycją przedziału.");
  if (
    data.sessions.some(
      (s) =>
        s.id !== id &&
        s.startedAt < endedAt &&
        (s.endedAt === null || s.endedAt > startedAt) &&
        (s.endedAt === null || s.endedAt > s.startedAt),
    )
  )
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  session.startedAt = startedAt;
  session.endedAt = endedAt;
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Odpowiednik db::split_session dla trybu podglądu. */
export async function splitSession(
  id: number,
  at: number,
  targetTask: number | null = null,
): Promise<Snapshot> {
  if (desktop) return invoke("split_session", { id, at, targetTask });
  const data = await read();
  const now = Date.now();
  if (data.trashSessions?.some((s) => s.id === id))
    throw new Error("Przedział należy do zadania w koszu.");
  const session = data.sessions.find((s) => s.id === id);
  if (!session) throw new Error("Nie znaleziono przedziału.");
  const end = session.endedAt ?? now;
  if (!Number.isSafeInteger(at) || !(session.startedAt < at && at < end))
    throw new Error("Moment podziału musi leżeć wewnątrz przedziału.");
  const targetId = targetTask ?? session.taskId;
  if (data.trash?.some((t) => t.id === targetId))
    throw new Error("Zadanie docelowe jest w koszu. Najpierw je przywróć.");
  const target = data.tasks.find((t) => t.id === targetId);
  if (!target) throw new Error("Nie znaleziono zadania docelowego.");
  if (session.endedAt === null) {
    if (target.done)
      throw new Error("Najpierw przywróć zadanie docelowe do zrobienia.");
    target.archived = false;
    target.details = unblocked(target);
  }
  const second: Session = {
    id:
      Math.max(
        0,
        ...data.sessions.map((s) => s.id),
        ...(data.trashSessions ?? []).map((s) => s.id),
      ) + 1,
    taskId: targetId,
    startedAt: at,
    endedAt: session.endedAt,
  };
  session.endedAt = at;
  data.sessions.unshift(second);
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
export function duration(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return [
    Math.floor(seconds / 3600),
    Math.floor(seconds / 60) % 60,
    seconds % 60,
  ]
    .map((v) => String(v).padStart(2, "0"))
    .join(":");
}

export async function saveDetails(
  id: number,
  details: TaskDetails,
): Promise<Snapshot> {
  if (desktop) return invoke("save_details", { id, details });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  if (details.plannedStart && !validTime(details.plannedStart))
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const saved: TaskDetails = JSON.parse(JSON.stringify(details));
  saved.project = details.project.trim();
  saved.waiting = details.waiting.trim();
  // Jak planner::validate: reguła cyklu i dane blokady.
  const rule = normalizeRule(saved.repeatRule);
  if (!rule) throw new Error("Niepoprawna reguła cyklu.");
  saved.repeatRule = rule;
  if (!ruleUsesWeekdays(rule.kind)) saved.repeatDays = [];
  if (
    saved.waitingRemindDay &&
    !/^\d{4}-\d{2}-\d{2}$/.test(saved.waitingRemindDay)
  )
    throw new Error("Niepoprawne dane blokady „Czekam na”.");
  const wasWaiting = !!task.details?.waiting;
  if (!saved.waiting) {
    saved.waitingSince = 0;
    saved.waitingStatus = "";
    saved.waitingRemindDay = "";
    if (wasWaiting) saved.waitingDismissed = task.jiraStatus ?? "";
  } else {
    // Nowa blokada zaczyna bez przypomnienia (jak planner::save).
    if (!wasWaiting) saved.waitingRemindDay = "";
    if (!saved.waitingSince) saved.waitingSince = Date.now();
    if (!saved.waitingStatus) saved.waitingStatus = task.jiraStatus ?? "";
  }
  if (isRecurring(saved) && !saved.repeatFrom)
    saved.repeatFrom = localDay(Date.now());
  if (isRecurring(saved) && !saved.plannedDay)
    saved.plannedDay = saved.repeatFrom;
  task.details = saved;
  if (saved.waiting)
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(s.startedAt, Date.now());
    });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

/** Zmiana blokady „Czekam na” (`planner::WaitingChange`). */
export interface WaitingChange {
  action: "block" | "unblock" | "dismiss" | "remind";
  text?: string;
}
/**
 * Blokada z kafelka: „Odblokuj”, propozycje z Jiry („Oznacz jako czekam na”,
 * „Nie teraz”). Zmienia tylko pola blokady (`set_waiting`); odpowiednik w podglądzie.
 */
export async function setWaiting(
  id: number,
  change: WaitingChange,
): Promise<Snapshot> {
  if (desktop) return invoke("set_waiting", { id, change });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  const text = (change.text ?? "").trim();
  const status = task.jiraStatus ?? "";
  const now = Date.now();
  const d = detailsOf(task);
  if (change.action === "block") {
    if (!text || [...text].length > 300)
      throw new Error("Opis blokady musi mieć od 1 do 300 znaków.");
    Object.assign(d, {
      waiting: text,
      waitingSince: now,
      waitingStatus: status,
      waitingRemindDay: "",
      waitingDismissed: "",
    });
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(s.startedAt, now);
    });
  } else if (change.action === "unblock")
    Object.assign(d, {
      waiting: "",
      waitingSince: 0,
      waitingStatus: "",
      waitingRemindDay: "",
      waitingDismissed: status,
    });
  else if (change.action === "dismiss") d.waitingDismissed = status;
  else if (change.action === "remind") {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(text))
      throw new Error("Niepoprawny dzień przypomnienia.");
    // Stary toast po odblokowaniu: bez blokady nic nie zapisujemy.
    if (d.waiting) d.waitingRemindDay = text;
  } else throw new Error("Nieznana zmiana blokady.");
  task.details = d;
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

export async function addSession(
  taskId: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
  if (desktop) return invoke("add_session", { taskId, startedAt, endedAt });
  if (
    !Number.isSafeInteger(startedAt) ||
    !Number.isSafeInteger(endedAt) ||
    startedAt < 0 ||
    endedAt <= startedAt ||
    endedAt > Date.now()
  )
    throw new Error(
      "Koniec musi być późniejszy niż początek i nie może być w przyszłości.",
    );
  const data = await read();
  if (!data.tasks.some((t) => t.id === taskId))
    throw new Error("Nie znaleziono zadania.");
  if (
    data.sessions.some(
      (s) =>
        s.startedAt < endedAt &&
        (s.endedAt === null || s.endedAt > startedAt) &&
        (s.endedAt === null || s.endedAt > s.startedAt),
    )
  )
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  data.sessions.unshift({
    id:
      Math.max(
        0,
        ...data.sessions.map((s) => s.id),
        ...(data.trashSessions ?? []).map((s) => s.id),
      ) + 1,
    taskId,
    startedAt,
    endedAt,
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

export async function setPriority(
  id: number,
  priority: number,
): Promise<Snapshot> {
  if (desktop) return invoke("set_priority", { id, priority });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), priority };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/**
 * Odpowiednik `reorder_tasks` (`planner::reorder`): zmienia wyłącznie `order`
 * i `priority` podanych zadań, reszta szczegółów zostaje bez zmian.
 */
export async function reorderTasks(
  entries: { id: number; order: number; priority: number }[],
): Promise<Snapshot> {
  if (desktop) return invoke("reorder_tasks", { entries });
  const data = await read();
  if (
    entries.length > 10000 ||
    entries.some(
      (e) =>
        !Number.isSafeInteger(e.order) ||
        e.order < 0 ||
        e.order > 1_000_000 ||
        ![-1, 0, 1].includes(e.priority),
    )
  )
    throw new Error("Niepoprawna kolejność zadań.");
  const tasks = entries.map((e) => data.tasks.find((t) => t.id === e.id));
  if (tasks.some((t) => !t)) throw new Error("Nie znaleziono zadania.");
  entries.forEach((e, i) => {
    const task = tasks[i]!;
    task.details = { ...detailsOf(task), order: e.order, priority: e.priority };
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/**
 * Odpowiednik `plan_task` (`planner::plan_task`): przeniesienie na tablicy
 * tygodnia zmienia wyłącznie `plannedDay` (pusty = „Do zaplanowania”) i `order`.
 */
export async function planTask(
  id: number,
  day: string,
  order: number,
): Promise<Snapshot> {
  if (desktop) return invoke("plan_task", { id, day, order });
  if (
    (day !== "" &&
      (!/^\d{4}-\d{2}-\d{2}$/.test(day) ||
        localDay(+new Date(day + "T12:00:00")) !== day)) ||
    !Number.isSafeInteger(order) ||
    order < 0 ||
    order > 1_000_000
  )
    throw new Error("Niepoprawny dzień planu.");
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), plannedDay: day, order };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** Wpis `plan_tasks`: dzień planu i kolejność jednego zadania. */
export interface PlanEntry {
  id: number;
  day: string;
  order: number;
}
/**
 * Odpowiednik `plan_tasks` (`planner::plan_tasks`): „Zastosuj” z „Rozłóż”
 * zapisuje dni wielu zadań naraz; jak `planTask` zmienia tylko `plannedDay`
 * i `order`, a przy błędzie nie zmienia niczego.
 */
export async function planTasks(entries: PlanEntry[]): Promise<Snapshot> {
  if (desktop) return invoke("plan_tasks", { entries });
  if (
    entries.length > 10000 ||
    entries.some(
      (e) =>
        (e.day !== "" &&
          (!/^\d{4}-\d{2}-\d{2}$/.test(e.day) ||
            localDay(+new Date(e.day + "T12:00:00")) !== e.day)) ||
        !Number.isSafeInteger(e.order) ||
        e.order < 0 ||
        e.order > 1_000_000,
    )
  )
    throw new Error("Niepoprawny dzień planu.");
  const data = await read();
  const tasks = entries.map((e) => data.tasks.find((t) => t.id === e.id));
  if (tasks.some((t) => !t)) throw new Error("Nie znaleziono zadania.");
  entries.forEach((e, i) => {
    const task = tasks[i]!;
    task.details = { ...detailsOf(task), plannedDay: e.day, order: e.order };
  });
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
export async function closeDay(
  day: string,
  closed: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("close_day", { day, closed });
  const data = await read();
  data.closedDays = (data.closedDays ?? []).filter((d) => d !== day);
  if (closed) data.closedDays.push(day);
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
function localDay(at: number) {
  const d = new Date(at);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}
/**
 * Dni robocze i święta z ustawień podglądu (klucz jak w `settings.ts`, bez
 * importu, który zamknąłby cykl modułów) dla reguły „ostatni dzień roboczy”.
 */
function previewWorkSettings(): {
  workDays: number[];
  holidaysInTarget: boolean;
} {
  let stored: Record<string, unknown> = {};
  try {
    stored =
      JSON.parse(
        localStorage.getItem("todo-track-preview-settings-v1") ?? "{}",
      ) ?? {};
  } catch {
    stored = {};
  }
  const days = stored.workDays;
  return {
    workDays:
      Array.isArray(days) && days.every((d) => Number.isInteger(d))
        ? (days as number[])
        : [1, 2, 3, 4, 5],
    holidaysInTarget: stored.holidaysInTarget !== false,
  };
}
function generateRecurring(data: Snapshot): boolean {
  let changed = false;
  const today = localDay(Date.now());
  const work = previewWorkSettings();
  const earliest = new Date();
  earliest.setDate(earliest.getDate() - 365);
  for (const source of [...data.tasks]) {
    const rule = source.details;
    if (!rule || !isRecurring(rule) || !rule.repeatFrom || source.archived)
      continue;
    const last = data.recurrenceState?.[source.id];
    const cursor = new Date((last || rule.repeatFrom) + "T12:00:00");
    if (last) cursor.setDate(cursor.getDate() + 1);
    if (localDay(+cursor) < rule.repeatFrom)
      cursor.setTime(+new Date(rule.repeatFrom + "T12:00:00"));
    if (+cursor < +earliest) cursor.setTime(+earliest);
    while (Number.isFinite(+cursor) && localDay(+cursor) <= today) {
      const day = localDay(+cursor);
      if (
        occurs(rule, rule.repeatFrom, day, work) &&
        rule.plannedDay !== day
      ) {
        data.tasks.unshift({
          id:
            Math.max(
              0,
              ...data.tasks.map((t) => t.id),
              ...(data.trash ?? []).map((t) => t.id),
            ) + 1,
          title: source.title,
          done: false,
          createdAt: Date.now(),
          details: {
            ...detailsOf(source),
            notes: {},
            waiting: "",
            plannedDay: day,
            dueDay: "",
            repeatDays: [],
            repeatFrom: "",
            repeatRule: emptyRule(),
            waitingSince: 0,
            waitingStatus: "",
            waitingDismissed: "",
            waitingRemindDay: "",
            pinnedAt: 0,
            checklist: (rule.checklist ?? []).map((i) => ({
              ...i,
              done: false,
            })),
          },
        });
      }
      (data.recurrenceState ??= {})[source.id] = day;
      changed = true;
      cursor.setDate(cursor.getDate() + 1);
    }
  }
  return changed;
}

/** Wynik Start/Stop, odpowiednik `shortcuts::Toggle` w Rust. */
export type ToggleResult =
  | { action: "stopped"; taskId: number; title: string; elapsedMs: number }
  | { action: "started"; taskId: number; title: string }
  | { action: "quick" };

/** Najwięcej przypiętych otwartych zadań (`planner::MAX_PINNED`). */
export const maxPinned = 3;
/** Przypięte otwarte zadania w kolejności przypięcia (przyciski w widgecie). */
export function pinnedTasks(data: Snapshot): Task[] {
  const at = (t: Task) => t.details?.pinnedAt ?? 0;
  return data.tasks
    .filter((t) => !t.done && !t.archived && at(t) > 0)
    .sort((a, b) => at(a) - at(b) || a.id - b.id);
}
/** Przypięcie do widgetu i odpięcie (`set_pinned`); odpowiednik w podglądzie. */
export async function setPinned(
  id: number,
  pinned: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("set_pinned", { id, pinned });
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  if (pinned) {
    if (task.done || task.archived)
      throw new Error("Przypiąć można tylko otwarte zadanie.");
    if (pinnedTasks(data).filter((t) => t.id !== id).length >= maxPinned)
      throw new Error(
        `Przypiąć można najwyżej ${maxPinned} zadania. Odepnij jedno z nich.`,
      );
  }
  task.details = { ...detailsOf(task), pinnedAt: pinned ? Date.now() : 0 };
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}

/** Zadanie do wznowienia: ostatnio mierzone, nieukończone, niezarchiwizowane, nie w koszu. */
export function resumeCandidate(data: Snapshot): Task | undefined {
  const open = new Map(
    data.tasks.filter((t) => !t.done && !t.archived).map((t) => [t.id, t]),
  );
  const last = data.sessions
    .filter((s) => open.has(s.taskId))
    .sort(
      (a, b) =>
        (b.endedAt ?? b.startedAt) - (a.endedAt ?? a.startedAt) ||
        b.startedAt - a.startedAt ||
        b.id - a.id,
    )[0];
  return last && open.get(last.taskId);
}

/**
 * Start/Stop jak globalny skrót: trwający pomiar → Stop, brak pomiaru → Start
 * ostatniego zadania, brak takiego → „quick”. Ani komenda `toggle_measurement`, ani
 * podgląd nie otwierają wtedy szybkiego przełączania: robi to wywołujący (`quick_open`).
 * Potwierdzenie w aplikacji przychodzi zdarzeniem `toggle-feedback` (tekst).
 */
export async function toggleMeasurement(): Promise<ToggleResult> {
  if (desktop) return invoke("toggle_measurement");
  const data = await read();
  const active = data.sessions.find((s) => s.endedAt === null);
  if (active) {
    const task = data.tasks.find((t) => t.id === active.taskId);
    const now = Date.now();
    await change("stop");
    return {
      action: "stopped",
      taskId: active.taskId,
      title: task?.title ?? "",
      elapsedMs: Math.max(0, now - active.startedAt),
    };
  }
  const task = resumeCandidate(data);
  if (!task) return { action: "quick" };
  await change("start", task.id);
  return { action: "started", taskId: task.id, title: task.title };
}

/** Uruchamianie z Windows: stan wpisu w rejestrze i opcja „schowane do traya”. */
export interface Autostart {
  enabled: boolean;
  hidden: boolean;
}
const autostartKey = "todo-track-preview-autostart-v1";
const previewSettingsKey = "todo-track-preview-settings-v1";
function previewJson(name: string): Record<string, unknown> {
  try {
    const value = JSON.parse(localStorage.getItem(name) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value)
      ? value
      : {};
  } catch {
    return {};
  }
}

/**
 * `autostart_get`: „włączone” pochodzi z rejestru (w podglądzie z osobnego klucza
 * localStorage), „schowane” z ustawienia `autostartHidden`.
 */
export async function readAutostart(): Promise<Autostart> {
  if (desktop) return invoke("autostart_get");
  return {
    enabled: previewJson(autostartKey).enabled === true,
    hidden: previewJson(previewSettingsKey).autostartHidden === true,
  };
}

/** `autostart_set`: zapisuje wpis Run (z `--hidden`, gdy `hidden`) i ustawienie. */
export async function writeAutostart(
  enabled: boolean,
  hidden: boolean,
): Promise<Autostart> {
  if (desktop) return invoke("autostart_set", { enabled, hidden });
  localStorage.setItem(autostartKey, JSON.stringify({ enabled }));
  const settings = {
    ...previewJson(previewSettingsKey),
    autostartHidden: hidden,
  };
  localStorage.setItem(previewSettingsKey, JSON.stringify(settings));
  return readAutostart();
}

/** Stan mini-widgetu (`widget::Status` w Rust). */
export interface WidgetStatus {
  enabled: boolean;
  backgroundOnly: boolean;
  /** Schowany przyciskiem X do końca sesji aplikacji; ustawienie się nie zmienia. */
  dismissed: boolean;
}
const widgetDismissedKey = "todo-track-preview-widget-dismissed";
const widgetEvent = "widget-state";
function previewWidgetDismissed(): boolean {
  try {
    return sessionStorage.getItem(widgetDismissedKey) === "1";
  } catch {
    return false;
  }
}

/**
 * Podgląd `Status::after_settings_change`: schowanie X kasujemy już przy
 * wyłączeniu widgetu (wyłączony i tak jest niewidoczny), więc ponowne włączenie
 * w ustawieniach go pokazuje.
 */
export function previewWidgetSettingsChanged(widgetEnabled: boolean) {
  if (widgetEnabled) return;
  try {
    sessionStorage.removeItem(widgetDismissedKey);
  } catch {}
}

/** `widget_status`: ustawienia widgetu i schowanie do końca sesji. */
export async function readWidgetStatus(): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_status");
  const settings = previewJson(previewSettingsKey);
  return {
    enabled: settings.widgetEnabled === true,
    backgroundOnly: settings.widgetBackgroundOnly === true,
    dismissed: previewWidgetDismissed(),
  };
}

/**
 * `widget_dismiss`: X na widgecie chowa go do końca sesji aplikacji (w podglądzie
 * do końca sesji karty), przełącznik w nagłówku przywraca. Ustawienie bez zmian.
 */
export async function dismissWidget(dismissed: boolean): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_dismiss", { dismissed });
  try {
    if (dismissed) sessionStorage.setItem(widgetDismissedKey, "1");
    else sessionStorage.removeItem(widgetDismissedKey);
  } catch {}
  const status = await readWidgetStatus();
  window.dispatchEvent(new CustomEvent(widgetEvent, { detail: status }));
  return status;
}

/** Nasłuch zmian stanu widgetu (zdarzenie Tauri albo zdarzenie okna w podglądzie). */
export async function onWidgetStatus(
  handler: (status: WidgetStatus) => void,
): Promise<() => void> {
  if (desktop) {
    return listen<WidgetStatus>(widgetEvent, (e) => handler(e.payload));
  }
  const listener = (e: Event) =>
    handler((e as CustomEvent<WidgetStatus>).detail);
  window.addEventListener(widgetEvent, listener);
  return () => window.removeEventListener(widgetEvent, listener);
}

/**
 * Ładunek zdarzenia `navigate` (`notify::View` w Rust): kliknięcie powiadomienia
 * Windows lub przycisku „Podziel…”. Brak `view` = tylko główne okno.
 */
export interface NavigateRequest {
  view?: string | null;
  /** `today` = filtr „Dzisiaj: plan i praca”; `due` = ten filtr z terminami dziś i zaległymi. */
  filter?: string | null;
  /** Zadanie do rozwinięcia na liście zadań. */
  task?: number | null;
  /** Przedział do otwarcia w edytorze na zakładce Podziel. */
  split?: number | null;
  /**
   * Grupa listy zadań, do której przewinąć (np. „Nierozpoczęte”), przy
   * `view: "journal"` zakładka `morning` (Plan i historia → Rano) albo
   * `week-review` (Plan i historia → Tydzień → Przegląd), albo przy
   * `view: "settings"` sekcja Ustawień (`work`, `calendar`, `jira`,
   * `notifications`, `focus`, `templates`, `windows`, `shortcuts`, `startup`).
   */
  section?: string | null;
  /** Plan i historia → Zamknięcie dnia. */
  dayClose?: boolean;
}
/** Nazwa zdarzenia okna, którym podgląd (i testy) symuluje `navigate` z backendu. */
export const previewNavigateEvent = "todo-track-navigate";

/** Nasłuch `navigate`: zdarzenie Tauri albo zdarzenie okna w podglądzie. */
export async function onNavigate(
  handler: (request: NavigateRequest) => void,
): Promise<() => void> {
  if (desktop)
    return listen<NavigateRequest>("navigate", (e) => handler(e.payload));
  const listener = (e: Event) =>
    handler((e as CustomEvent<NavigateRequest>).detail ?? {});
  window.addEventListener(previewNavigateEvent, listener);
  return () => window.removeEventListener(previewNavigateEvent, listener);
}

/** `widget_open_main`: klik w nazwę zadania na widgecie. W podglądzie nic nie robi. */
export async function openMainWindow(): Promise<void> {
  if (desktop) await invoke("widget_open_main");
}

export type ExportFormat = "csv" | "xlsx";
/** Wynik eksportu: ścieżka zapisanego pliku albo (w podglądzie) tekst CSV. */
export type ExportResult = { path: string } | { text: string };
/**
 * `export_report`: okno zapisu i plik CSV albo XLSX z przedziałami wybranych
 * dni; `null`, gdy użytkownik anulował. W podglądzie CSV wraca jako tekst,
 * a XLSX jest niedostępny.
 */
export async function exportReport(
  format: ExportFormat,
  days: string[],
  name: string,
): Promise<ExportResult | null> {
  if (!days.length) throw new Error("Wybierz co najmniej jeden dzień do eksportu.");
  if (desktop) {
    const path = await invoke<string | null>("export_report", {
      format,
      days,
      name,
    });
    return path ? { path } : null;
  }
  if (format === "xlsx")
    throw new Error("Eksport XLSX jest dostępny tylko w aplikacji Windows.");
  return { text: exportCsv(exportRows(await read(), days, Date.now())) };
}

/** Stan kalendarza Microsoft (`calendar_status`). */
export interface CalendarStatus {
  clientId: string;
  tenant: string;
  /** Zalogowane konto; pusty, gdy niepołączone. */
  email: string;
  connected: boolean;
  /** Trwa logowanie w przeglądarce. */
  connecting: boolean;
  lastSync: number | null;
  lastError: string;
}
/** Spotkanie (`calendar_events`): czasy w ms, całodniowe od lokalnej północy. */
export interface CalendarEvent {
  id: string;
  start: number;
  end: number;
  subject: string;
  /** `free`, `tentative`, `busy`, `oof`, `workingElsewhere` albo `unknown`. */
  showAs: string;
  allDay: boolean;
  cancelled: boolean;
  /** Odpowiedź na zaproszenie, np. `accepted`, `declined`, `organizer`. */
  response: string;
}
/** Dane podglądu: ustawienia, pobrane spotkania i spotkania „w chmurze” (`sample`). */
interface PreviewCalendar {
  clientId: string;
  tenant: string;
  email: string;
  lastSync: number | null;
  lastError: string;
  events: CalendarEvent[];
  sample: CalendarEvent[];
}
export const calendarKey = "todo-track-preview-calendar-v1";
const calendarEvent = "calendar-changed";
/** Konto „zalogowane” w podglądzie. */
export const previewCalendarEmail = "jan.kowalski@firma.example";
const guid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
/** Jak `calendar::valid_tenant`: organizations/common/consumers, GUID albo domena. */
const validTenant = (t: string) =>
  ["organizations", "common", "consumers"].includes(t) ||
  guid.test(t) ||
  (t.length <= 253 &&
    t.includes(".") &&
    t.split(".").every((l) => /^[a-z0-9-]+$/i.test(l)));
export const calendarClientIdMessage =
  "Podaj identyfikator aplikacji (client ID) z rejestracji w Entra ID, np. 00000000-0000-0000-0000-000000000000.";
function previewCalendar(): PreviewCalendar {
  const empty: PreviewCalendar = {
    clientId: "",
    tenant: "organizations",
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
    sample: [],
  };
  try {
    const value = JSON.parse(localStorage.getItem(calendarKey) ?? "{}");
    return value && typeof value === "object" ? { ...empty, ...value } : empty;
  } catch {
    return empty;
  }
}
function savePreviewCalendar(data: PreviewCalendar) {
  localStorage.setItem(calendarKey, JSON.stringify(data));
  window.dispatchEvent(new Event(calendarEvent));
}
const previewStatus = (c: PreviewCalendar): CalendarStatus => ({
  clientId: c.clientId,
  tenant: c.tenant || "organizations",
  email: c.email,
  connected: !!c.email && !!c.clientId,
  connecting: false,
  lastSync: c.lastSync,
  lastError: c.lastError,
});
export async function calendarStatus(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_status");
  return previewStatus(previewCalendar());
}
/**
 * „Połącz konto Microsoft”: logowanie w przeglądarce i pierwsze pobranie.
 * Podgląd symuluje udane logowanie i „pobiera” spotkania z `sample`.
 */
export async function calendarConnect(
  clientId: string,
  tenant: string,
): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_connect", { clientId, tenant });
  const id = clientId.trim().toLowerCase();
  const t = tenant.trim().toLowerCase() || "organizations";
  if (!guid.test(id)) throw new Error(calendarClientIdMessage);
  if (!validTenant(t))
    throw new Error(
      "Podaj dzierżawcę: organizations, identyfikator albo domenę firmy.",
    );
  const c = previewCalendar();
  const next: PreviewCalendar = {
    ...c,
    clientId: id,
    tenant: t,
    email: previewCalendarEmail,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** „Rozłącz”: usuwa token i spotkania; client ID i dzierżawca zostają. */
export async function calendarDisconnect(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_disconnect");
  const next = {
    ...previewCalendar(),
    email: "",
    lastSync: null,
    lastError: "",
    events: [],
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** „Odśwież”: pobiera spotkania teraz. */
export async function calendarRefresh(): Promise<CalendarStatus> {
  if (desktop) return invoke("calendar_refresh");
  const c = previewCalendar();
  if (!c.clientId)
    throw new Error(
      "Kalendarz jest wyłączony: podaj client ID w Ustawieniach → Kalendarz.",
    );
  if (!c.email)
    throw new Error("Połącz konto Microsoft w Ustawieniach → Kalendarz.");
  const next = {
    ...c,
    lastSync: Date.now(),
    lastError: "",
    events: c.sample.map((e) => ({ ...e })),
  };
  savePreviewCalendar(next);
  return previewStatus(next);
}
/** Spotkania nakładające się na `[from, to)`, według początku. */
export async function calendarEvents(
  from: number,
  to: number,
): Promise<CalendarEvent[]> {
  if (desktop) return invoke("calendar_events", { from, to });
  if (to < from) throw new Error("Niepoprawny zakres dat.");
  const c = previewCalendar();
  // Jak `calendar_events`: bez połączonego konta żadne spotkania nie obowiązują.
  if (!c.email) return [];
  return c.events
    .filter(
      (e) => e.start < to && (e.end > from || (e.end === e.start && e.start >= from)),
    )
    .sort((a, b) => a.start - b.start || a.end - b.end || a.id.localeCompare(b.id));
}
/** Nasłuch zmian kalendarza (pobranie, połączenie, rozłączenie). */
export async function onCalendarChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(calendarEvent, () => handler());
  window.addEventListener(calendarEvent, handler);
  return () => window.removeEventListener(calendarEvent, handler);
}
/**
 * Czy spotkanie zajmuje czas (`calendar::occupies`): busy, oof, workingElsewhere
 * albo tentative, nieanulowane i nieodrzucone.
 */
export const meetingOccupies = (e: CalendarEvent) =>
  ["busy", "oof", "workingElsewhere", "tentative"].includes(e.showAs) &&
  !e.cancelled &&
  e.response !== "declined";
/** Tytuł spotkania w interfejsie: „Spotkanie”, gdy tytuły są wyłączone albo puste. */
export const meetingTitle = (e: CalendarEvent, showTitles: boolean) =>
  (showTitles && e.subject.trim()) || "Spotkanie";

/**
 * Przejście w głównym oknie z innego okna (np. szybkiego przełączania):
 * pokazuje główne okno i wysyła mu `navigate`. W podglądzie nic nie robi,
 * bo wszystkie widoki są w jednym oknie z zarejestrowaną obsługą.
 */
export async function navigateMain(request: NavigateRequest): Promise<void> {
  if (!desktop) return;
  await invoke("widget_open_main");
  await emitTo("main", "navigate", request);
}

/** „Wnioski” z przeglądu tygodnia w podglądzie (`week_notes`): tydzień → tekst. */
export const weekNotesKey = "todo-track-preview-week-notes-v1";
const WEEK_NOTE_MAX = 10_000;
function previewWeekNotes(): Record<string, string> {
  const value = previewJson(weekNotesKey);
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, string] => typeof entry[1] === "string",
    ),
  );
}
/** Jak `week_notes::valid_week`: istniejący poniedziałek RRRR-MM-DD. */
function checkWeek(week: string) {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(week);
  const d = m ? new Date(Date.UTC(+m[1], +m[2] - 1, +m[3])) : null;
  if (
    !m ||
    !d ||
    d.getUTCFullYear() !== +m[1] ||
    d.getUTCMonth() !== +m[2] - 1 ||
    d.getUTCDate() !== +m[3] ||
    d.getUTCDay() !== 1
  )
    throw new Error("Tydzień musi być poniedziałkiem w formacie RRRR-MM-DD.");
}
/** Wnioski tygodnia zaczynającego się w poniedziałek `week` (`week_note`). */
export async function weekNote(week: string): Promise<string> {
  if (desktop) return invoke("week_note", { week });
  checkWeek(week);
  return previewWeekNotes()[week] ?? "";
}
/**
 * Zapis wniosków (`week_note_save`): tekst bez białych znaków na końcach, pusty
 * usuwa wpis. Zwraca zapisany tekst.
 */
export async function saveWeekNote(week: string, text: string): Promise<string> {
  if (desktop) return invoke("week_note_save", { week, text });
  checkWeek(week);
  const value = text.trim();
  if ([...value].length > WEEK_NOTE_MAX)
    throw new Error(`Wnioski mogą mieć najwyżej ${WEEK_NOTE_MAX} znaków.`);
  const notes = previewWeekNotes();
  if (value) notes[week] = value;
  else delete notes[week];
  localStorage.setItem(weekNotesKey, JSON.stringify(notes));
  return value;
}

/** Stan rozszerzenia przeglądarki (`browser::Status`, W4). */
export interface BrowserStatus {
  enabled: boolean;
  /** Wpisy hosta native messaging są w rejestrze Chrome i Edge. */
  registered: boolean;
  /** Rozszerzenie odezwało się w ostatnich 45 s. */
  connected: boolean;
  lastMessage: number | null;
  /** Ostatni rozpoznany klucz zgłoszenia i kiedy przyszedł. */
  lastKey: string | null;
  lastKeyAt: number | null;
  /** Błąd rejestracji (np. brak todo-track-bridge.exe). */
  error: string;
  extensionId: string;
  /** Folder do „Załaduj rozpakowane”; `null`, gdy nie znaleziono. */
  extensionFolder: string | null;
}
/** Stały identyfikator rozszerzenia (`browser::EXTENSION_ID`). */
export const browserExtensionId = "cecpcpkmmmkgfdklnkkkigofkcibnhbl";
/**
 * Podgląd: ostatnia wiadomość rozszerzenia (`lastMessage`, `lastKey`,
 * `lastKeyAt`) w localStorage; testy ustawiają ją, żeby pokazać „połączone”.
 */
export const browserKey = "todo-track-preview-browser-v1";
/** `browser_status`: stan wyłącznika, rejestracji i ostatniego sygnału. */
export async function browserStatus(): Promise<BrowserStatus> {
  if (desktop) return invoke("browser_status");
  const enabled = previewJson(previewSettingsKey).browserExtension === true;
  const data = previewJson(browserKey);
  const num = (v: unknown) => (typeof v === "number" ? v : null);
  const lastMessage = num(data.lastMessage);
  return {
    enabled,
    registered: enabled,
    connected:
      enabled && lastMessage != null && Date.now() - lastMessage <= 45_000,
    lastMessage,
    lastKey: typeof data.lastKey === "string" ? data.lastKey : null,
    lastKeyAt: num(data.lastKeyAt),
    error: "",
    extensionId: browserExtensionId,
    extensionFolder: "browser-extension",
  };
}
/** `browser_open_folder`: folder rozszerzenia w Eksploratorze (tylko aplikacja). */
export async function browserOpenFolder(): Promise<void> {
  if (desktop) return invoke("browser_open_folder");
  throw new Error(
    "Folder rozszerzenia otwiera się w aplikacji Windows. W repozytorium to katalog browser-extension.",
  );
}

/** Przejście zgłoszenia Jiry (`jira::Transition`). */
export interface JiraTransition {
  id: string;
  name: string;
  toStatus: string;
  /** Kategoria statusu docelowego: `new`, `indeterminate`, `done`. */
  toCategory: string;
}
/** Pytanie o przesunięcie zgłoszenia przy starcie pracy (`jira_move::Prompt`). */
export interface JiraMovePrompt {
  taskId: number;
  key: string;
  title: string;
  status: string;
  url: string;
  /** Status z ustawień (gdy przejście nie jest wybrane). */
  target: string;
  /** Przejścia do statusu „w toku”. */
  transitions: JiraTransition[];
  /** Wybrane przejście; `null` = lista do wyboru. */
  transitionId: string | null;
  /** Błąd ostatniej próby przesunięcia (baner z linkiem do zgłoszenia). */
  failure?: string;
}
/** Nazwa statusu docelowego w pytaniu (`Prompt::target_name`). */
export const moveTargetName = (p: JiraMovePrompt) =>
  p.transitions.find((t) => t.id === p.transitionId)?.toStatus || p.target;
/**
 * Wybór przejścia jak `jira_move::choose`: przejścia do kategorii
 * `indeterminate`; jedno wygrywa samo, przy kilku to z nazwą z ustawień (status
 * docelowy, potem nazwa przejścia), bez dopasowania `null` (lista do wyboru).
 */
export function chooseTransition(
  all: JiraTransition[],
  preferred: string,
): { transitions: JiraTransition[]; transitionId: string | null } {
  const transitions = all.filter((t) => t.toCategory === "indeterminate");
  const wanted = preferred.trim().toLowerCase();
  const by = (pick: (t: JiraTransition) => string) =>
    (wanted &&
      transitions.find((t) => pick(t).trim().toLowerCase() === wanted)?.id) ||
    null;
  const transitionId =
    transitions.length === 1
      ? transitions[0].id
      : transitions.length
        ? (by((t) => t.toStatus) ?? by((t) => t.name))
        : null;
  return { transitions, transitionId };
}
/** Komunikat przy wyłączonych zapisach (`jira::WRITE_DISABLED`). */
export const jiraWriteDisabled =
  "Zmiany w Jirze są wyłączone. Włączysz je w Ustawienia → Jira („Pozwól aplikacji zmieniać zgłoszenia w Jirze”).";
export const jiraMoveEvent = "jira-move-prompts";
/**
 * Symulowana Jira podglądu: ostatnio widziany pomiar i pytania; status zgłoszenia
 * to status z kafelka (pusty = „Do zrobienia”). `fail` (400 albo 403) symuluje
 * odrzucenie przejścia przez Jirę.
 */
export const previewJiraKey = "todo-track-preview-jira-v1";
interface PreviewJira {
  ready?: boolean;
  seen?: [number, number] | null;
  prompts?: JiraMovePrompt[];
  fail?: number;
}
/** Przepływ symulowanej Jiry: z każdego statusu do pozostałych. */
export const previewWorkflow: readonly JiraTransition[] = Object.freeze([
  {
    id: "11",
    name: "Do zrobienia",
    toStatus: "Do zrobienia",
    toCategory: "new",
  },
  {
    id: "21",
    name: "Rozpocznij",
    toStatus: "In Progress",
    toCategory: "indeterminate",
  },
  {
    id: "31",
    name: "Do code review",
    toStatus: "Code review",
    toCategory: "indeterminate",
  },
  { id: "41", name: "Zakończ", toStatus: "Gotowe", toCategory: "done" },
]);
/** Kategoria statusu w symulowanej Jirze. */
export function previewStatusCategory(status: string): string {
  const s = status.trim().toLowerCase();
  if (["", "do zrobienia", "to do", "open", "backlog", "nowe"].includes(s))
    return "new";
  if (["gotowe", "done", "zamknięte", "closed", "resolved"].includes(s))
    return "done";
  return "indeterminate";
}
const previewTransitionsFor = (status: string) =>
  previewWorkflow
    .filter((t) => t.toStatus.toLowerCase() !== status.trim().toLowerCase())
    .map((t) => ({ ...t }));
function previewJiraState(): PreviewJira {
  return previewJson(previewJiraKey) as PreviewJira;
}
function savePreviewJira(state: PreviewJira) {
  // Bez zmian nie zapisujemy: inne okna podglądu (widget) dostają `storage`.
  const raw = JSON.stringify(state);
  if (localStorage.getItem(previewJiraKey) !== raw)
    localStorage.setItem(previewJiraKey, raw);
}
/** Ustawienia zapisu do Jiry w podglądzie (wartości domyślne jak w Rust). */
function previewJiraSettings() {
  const s = previewJson(previewSettingsKey);
  return {
    write: s.jiraWrite !== false,
    ask: s.jiraAskMove !== false,
    status:
      typeof s.jiraMoveStatus === "string" && s.jiraMoveStatus.trim()
        ? s.jiraMoveStatus.trim()
        : "In Progress",
  };
}
/**
 * Odpowiednik wątku `jira_move::tick` i `check`: nowy pomiar innego zadania niż
 * poprzednio widziany, zadanie z kluczem w kategorii „Do zrobienia” → pytanie.
 * Pomiar trwający przy pierwszym odczycie nie jest nowym startem.
 */
async function previewObserveStart(): Promise<PreviewJira> {
  const state = previewJiraState();
  const data = await read();
  const active = data.sessions.find((s) => s.endedAt === null);
  const current: [number, number] | null = active
    ? [active.id, active.taskId]
    : null;
  const previous = state.seen ?? null;
  let fresh: number | null = null;
  if (!state.ready) state.ready = true;
  else if (
    current &&
    !(previous && (previous[0] === current[0] || previous[1] === current[1]))
  )
    fresh = current[1];
  state.seen = current;
  // Pytania tylko dla zadania aktywnego pomiaru (`jira_move::keep_current`).
  state.prompts = (state.prompts ?? []).filter(
    (p) => current !== null && p.taskId === current[1],
  );
  const task = data.tasks.find((t) => t.id === fresh);
  const settings = previewJiraSettings();
  if (
    task?.jiraKey &&
    settings.write &&
    settings.ask &&
    !task.details?.jiraNoAsk
  ) {
    // Status odczytany z Jiry trafia na kafelek (`store_status`).
    const status = task.jiraStatus || "Do zrobienia";
    if (task.jiraStatus !== status) {
      task.jiraStatus = status;
      localStorage.setItem(key, JSON.stringify(data));
    }
    if (previewStatusCategory(status) === "new") {
      const choice = chooseTransition(
        previewTransitionsFor(status),
        settings.status,
      );
      if (choice.transitions.length)
        state.prompts = [
          {
            taskId: task.id,
            key: task.jiraKey,
            title: task.title,
            status,
            url: task.jiraUrl ?? `${previewJiraSite}/browse/${task.jiraKey}`,
            target: settings.status,
            ...choice,
          },
        ];
    }
  }
  savePreviewJira(state);
  return state;
}
function notifyPreviewJira() {
  window.dispatchEvent(new Event(jiraMoveEvent));
}
/** Pytania o przesunięcie zgłoszenia (`jira_move_prompts`). */
export async function readMovePrompts(): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_prompts");
  return (await previewObserveStart()).prompts ?? [];
}
/** „Nie teraz” (`jira_move_dismiss`): pytanie znika do kolejnego startu zadania. */
export async function dismissMovePrompt(
  taskId: number,
): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_dismiss", { taskId });
  const state = previewJiraState();
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return state.prompts;
}
/** „Nie pytaj dla tego zadania” i „Pytaj znowu” (`jira_no_ask`). */
export async function setJiraNoAsk(
  taskId: number,
  noAsk: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_no_ask", { taskId, noAsk });
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), jiraNoAsk: noAsk };
  localStorage.setItem(key, JSON.stringify(data));
  if (noAsk) await dismissMovePrompt(taskId);
  return data;
}
/** Zadanie z kluczem dla zapisu w podglądzie (`jira_move::target`). */
async function previewJiraTask(taskId: number) {
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  if (!task?.jiraKey)
    throw new Error("To zadanie nie jest powiązane ze zgłoszeniem Jiry.");
  return { data, task };
}
/** Dostępne przejścia zgłoszenia zadania (`jira_transitions`). */
export async function jiraTransitions(
  taskId: number,
): Promise<JiraTransition[]> {
  if (desktop) return invoke("jira_transitions", { taskId });
  const { task } = await previewJiraTask(taskId);
  return previewTransitionsFor(task.jiraStatus || "Do zrobienia");
}
/**
 * Przejście zgłoszenia (`jira_transition`): status na kafelku po odświeżeniu,
 * pytanie zadania znika. Błąd symulowanej Jiry jak `jira::write_error`.
 */
export async function jiraTransition(
  taskId: number,
  transitionId: string,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_transition", { taskId, transitionId });
  const { data, task } = await previewJiraTask(taskId);
  const state = previewJiraState();
  // Błąd zostaje przy pytaniu zadania (`jira_move::mark_failure`).
  const fail = (message: string) => {
    if ((state.prompts ?? []).some((p) => p.taskId === taskId)) {
      state.prompts = (state.prompts ?? []).map((p) =>
        p.taskId === taskId ? { ...p, failure: message } : p,
      );
      savePreviewJira(state);
      notifyPreviewJira();
    }
    return new Error(message);
  };
  if (state.fail === 403)
    throw fail("Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
  if (state.fail === 400)
    throw fail(
      "Jira odrzuciła zmianę. Przejście może wymagać wypełnienia pól na ekranie w Jirze — zrób je w przeglądarce.",
    );
  const transition = previewTransitionsFor(
    task.jiraStatus || "Do zrobienia",
  ).find((t) => t.id === transitionId);
  if (!transition)
    throw fail("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  task.jiraStatus = transition.toStatus;
  localStorage.setItem(key, JSON.stringify(data));
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return data;
}
/** Nasłuch zmian pytań (zdarzenie Rust, w podglądzie zdarzenie okna i `storage`). */
export async function onMovePromptsChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(jiraMoveEvent, handler);
  const storage = (e: StorageEvent) => {
    if (e.key === previewJiraKey || e.key === key) handler();
  };
  window.addEventListener(jiraMoveEvent, handler);
  window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(jiraMoveEvent, handler);
    window.removeEventListener("storage", storage);
  };
}

// ---------------------------------------------------------------------------
// J2: Jira z poziomu Todo Track (tablica, przejścia, przypisanie, komentarze,
// nazwa gałęzi, nowe zgłoszenia). W aplikacji komendy z `jira.rs` i
// `jira_board.rs`; w podglądzie symulowana Jira w localStorage
// (`previewJiraBoardKey`) z tymi samymi regułami (`columnFor`, `decideMove`,
// `branchName`). Błąd zapisu symuluje `fail` (400 albo 403) ze stanu J1.
// ---------------------------------------------------------------------------

export interface JiraBoardRef {
  id: number;
  name: string;
}
/** `jira::Settings`: konto i wybrane tablice. */
export interface JiraAccount {
  config: {
    site: string;
    email: string;
    displayName: string;
    boards: JiraBoardRef[];
  };
  connected: boolean;
}
/** Zgłoszenie listy „Moje” (`jira::Issue`). */
export interface JiraListIssue {
  key: string;
  summary: string;
  status: string;
  boards: string[];
  url: string;
}
export interface JiraColumn {
  name: string;
  statusIds: string[];
}
export interface JiraPerson {
  accountId: string;
  displayName: string;
}
/** Karta tablicy (`jira_board::BoardIssue`). */
export interface JiraBoardIssue {
  key: string;
  summary: string;
  status: string;
  statusId: string;
  category: string;
  issueType: string;
  priority: string;
  assignee: JiraPerson | null;
  url: string;
  column: number;
}
export interface JiraBoardView {
  boardId: number;
  project: string;
  columns: JiraColumn[];
  issues: JiraBoardIssue[];
  truncated: boolean;
  limit: number;
  layoutAt: number;
}
/** Przejście z polami ekranu (`jira_board::MoveOption`). */
export interface JiraMoveOption {
  id: string;
  name: string;
  toStatus: string;
  toStatusId: string;
  toCategory: string;
  required: string[];
}
export type JiraMoveOutcome =
  | {
      result: "moved";
      status: string;
      statusId: string;
      category: string;
      column: number;
    }
  | { result: "choose"; options: JiraMoveOption[] }
  | { result: "screen"; url: string; fields: string[]; message: string };
export interface JiraIssueType {
  id: string;
  name: string;
}
export interface JiraProject {
  key: string;
  boards: string[];
}
export interface JiraTypeFields {
  extraRequired: string[];
  createUrl: string;
}
export interface JiraNewIssue {
  project: string;
  issueTypeId: string;
  summary: string;
  description: string;
  assignMe: boolean;
  createTask: boolean;
  plannedDay: string;
}
export interface JiraCreated {
  key: string;
  url: string;
  snapshot: Snapshot | null;
  /** Zgłoszenie utworzone, ale zadanie w Todo Track nie powstało. */
  warning?: string | null;
}
export type JiraScope = "mine" | "all";

/** Najwięcej kart na tablicy (`jira_board::ISSUE_LIMIT`). */
export const jiraIssueLimit = 300;
/** Najdłuższy komentarz (`jira_board::COMMENT_LIMIT`). */
export const jiraCommentLimit = 5000;

/**
 * Kolumna statusu jak `jira_board::column_for`: kolumna ze statusem; inaczej
 * pierwsza kolumna ze statusem tej samej kategorii; inaczej „Do zrobienia” →
 * pierwsza, „Gotowe” → ostatnia, „W toku” → druga (przy 3+ kolumnach).
 */
export function columnFor(
  statusId: string,
  category: string,
  columns: JiraColumn[],
  categories: Record<string, string>,
): number {
  if (!columns.length) return 0;
  const direct = columns.findIndex((c) => c.statusIds.includes(statusId));
  if (direct >= 0) return direct;
  if (category) {
    const same = columns.findIndex((c) =>
      c.statusIds.some((s) => categories[s] === category),
    );
    if (same >= 0) return same;
  }
  if (category === "new") return 0;
  if (category === "done") return columns.length - 1;
  return columns.length >= 3 ? 1 : 0;
}
export type JiraMoveDecision =
  | { kind: "perform" | "screen"; option: JiraMoveOption }
  | { kind: "choose"; options: JiraMoveOption[] }
  | { kind: "none" };
/** Wybór przejścia do kolumny jak `jira_board::decide`. */
export function decideMove(
  options: JiraMoveOption[],
  target: number,
  columns: JiraColumn[],
  categories: Record<string, string>,
  chosen: string | null,
): JiraMoveDecision {
  const candidates = options.filter(
    (o) => columnFor(o.toStatusId, o.toCategory, columns, categories) === target,
  );
  let pick: JiraMoveOption | undefined;
  if (chosen !== null) {
    pick = candidates.find((o) => o.id === chosen);
    if (!pick) throw new Error("To przejście nie prowadzi do wybranej kolumny.");
  } else if (!candidates.length) return { kind: "none" };
  else if (candidates.length > 1) return { kind: "choose", options: candidates };
  else pick = candidates[0];
  return { kind: pick.required.length ? "screen" : "perform", option: pick };
}
const plainLetters: Record<string, string> = {
  ą: "a",
  á: "a",
  ä: "a",
  à: "a",
  â: "a",
  ć: "c",
  č: "c",
  ç: "c",
  ę: "e",
  é: "e",
  ě: "e",
  è: "e",
  ë: "e",
  ê: "e",
  ł: "l",
  ń: "n",
  ň: "n",
  ó: "o",
  ö: "o",
  ô: "o",
  ò: "o",
  ś: "s",
  š: "s",
  ź: "z",
  ż: "z",
  ž: "z",
  í: "i",
  ì: "i",
  î: "i",
  ï: "i",
  ú: "u",
  ü: "u",
  ů: "u",
  ù: "u",
  û: "u",
  ř: "r",
  ý: "y",
  ť: "t",
  ď: "d",
  ß: "ss",
};
/** Nazwa gałęzi jak `jira_board::branch_name` (część bez prefiksu do 50 znaków). */
export function branchName(prefix: string, key: string, title: string): string {
  let slug = "";
  for (const c of title.toLowerCase()) {
    if (/^[a-z0-9]$/.test(c)) slug += c;
    else if (plainLetters[c]) slug += plainLetters[c];
    else if (!slug.endsWith("-")) slug += "-";
  }
  slug = slug.replace(/^-+|-+$/g, "");
  const full = slug ? `${key}-${slug}` : key;
  return prefix + full.slice(0, 50).replace(/-+$/, "");
}
/** Inicjały osoby na karcie. */
export const initials = (name: string) =>
  name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toLocaleUpperCase("pl") ?? "")
    .join("") || "?";

const jiraKeyPattern = /^[A-Z0-9_]+-\d+$/;
function checkKey(issueKey: string) {
  if (!jiraKeyPattern.test(issueKey) || issueKey.length > 100)
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
}

/** Symulowana Jira podglądu (J2). */
export const previewJiraBoardKey = "todo-track-preview-jira-board-v1";
/** Linki otwarte „w przeglądarce” w podglądzie (do sprawdzenia w testach). */
export const previewJiraOpenedKey = "todo-track-preview-jira-opened-v1";
interface PreviewIssue {
  key: string;
  summary: string;
  status: string;
  issueType: string;
  priority: string;
  assignee: JiraPerson | null;
  boards: number[];
}
interface PreviewBoardState {
  issues?: PreviewIssue[];
  comments?: Record<string, string[]>;
  next?: number;
  boards?: JiraBoardRef[];
}
export const previewJiraMe: Readonly<JiraPerson> = Object.freeze({
  accountId: "preview-me",
  displayName: "Jan Kowalski",
});
const previewPeople: JiraPerson[] = [
  { ...previewJiraMe },
  { accountId: "preview-anna", displayName: "Anna Nowak" },
  { accountId: "preview-piotr", displayName: "Piotr Wiśniewski" },
];
const previewBoards: JiraBoardRef[] = [
  { id: 1, name: "WMS" },
  { id: 2, name: "Foundation" },
];
const previewBoardProject: Record<number, string> = { 1: "WMS", 2: "FND" };
/** Statusy symulowanej Jiry; „Testy” nie ma kolumny (trafia według kategorii). */
const previewStatuses = [
  { id: "1", name: "Do zrobienia", category: "new" },
  { id: "3", name: "In Progress", category: "indeterminate" },
  { id: "4", name: "Code review", category: "indeterminate" },
  { id: "5", name: "Gotowe", category: "done" },
  { id: "6", name: "Testy", category: "indeterminate" },
];
const previewColumns: JiraColumn[] = [
  { name: "Do zrobienia", statusIds: ["1"] },
  { name: "W toku", statusIds: ["3"] },
  { name: "Code review", statusIds: ["4"] },
  { name: "Gotowe", statusIds: ["5"] },
];
const previewCategories: Record<string, string> = Object.fromEntries(
  previewStatuses.map((s) => [s.id, s.category]),
);
function previewStatusId(name: string): string {
  const n = name.trim().toLowerCase();
  return previewStatuses.find((s) => s.name.toLowerCase() === n)?.id ?? "";
}
/** Przejścia tablicy: przepływ z J1, „Do testów” i „Zakończ” z polem Rozwiązanie. */
function previewMoveOptions(status: string): JiraMoveOption[] {
  const all: JiraMoveOption[] = [
    ...previewWorkflow.map((t) => ({
      ...t,
      toStatusId: previewStatusId(t.toStatus),
      required: t.id === "41" ? ["Rozwiązanie"] : [],
    })),
    {
      id: "51",
      name: "Do testów",
      toStatus: "Testy",
      toStatusId: "6",
      toCategory: "indeterminate",
      required: [],
    },
  ];
  return all.filter(
    (t) => t.toStatus.toLowerCase() !== status.trim().toLowerCase(),
  );
}
const previewTypes: (JiraIssueType & { required: string[] })[] = [
  { id: "10001", name: "Zadanie", required: [] },
  { id: "10002", name: "Błąd", required: ["Środowisko"] },
  { id: "10004", name: "Historia", required: [] },
];
const previewIssueSeed = (): PreviewIssue[] => [
  {
    key: "WMS-31",
    summary: "Obsługa przyjęcia towaru",
    status: "Do zrobienia",
    issueType: "Historia",
    priority: "High",
    assignee: { ...previewJiraMe },
    boards: [1, 2],
  },
  {
    key: "WMS-40",
    summary: "Raport rozbieżności magazynowych",
    status: "Code review",
    issueType: "Błąd",
    priority: "High",
    assignee: { ...previewPeople[1] },
    boards: [1],
  },
  {
    key: "WMS-41",
    summary: "Etykiety dla kurierów",
    status: "Do zrobienia",
    issueType: "Zadanie",
    priority: "Low",
    assignee: null,
    boards: [1],
  },
  {
    key: "WMS-44",
    summary: "Inwentaryzacja cząstkowa",
    status: "Testy",
    issueType: "Zadanie",
    priority: "Medium",
    assignee: { ...previewPeople[2] },
    boards: [1],
  },
  {
    key: "FND-3",
    summary: "Aktualizacja bibliotek",
    status: "In Progress",
    issueType: "Zadanie",
    priority: "Medium",
    assignee: { ...previewJiraMe },
    boards: [2],
  },
];
/**
 * Stan symulowanej tablicy: zgłoszenia z przykładu i z zadań z kluczem
 * (status z kafelka wygrywa, bo zmienia go też J1).
 */
async function previewBoardState(): Promise<{
  state: PreviewBoardState;
  data: Snapshot;
  issues: PreviewIssue[];
}> {
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  const data = await read();
  const issues = Array.isArray(state.issues) ? state.issues : previewIssueSeed();
  for (const task of [...data.tasks, ...(data.trash ?? [])]) {
    if (!task.jiraKey) continue;
    const issue = issues.find((i) => i.key === task.jiraKey);
    if (!issue)
      issues.push({
        key: task.jiraKey,
        summary: task.title,
        status: task.jiraStatus || "Do zrobienia",
        issueType: "Zadanie",
        priority: "Medium",
        assignee: { ...previewJiraMe },
        boards: [1],
      });
    else if (task.jiraStatus) issue.status = task.jiraStatus;
  }
  state.issues = issues;
  return { state, data, issues };
}
function savePreviewBoard(state: PreviewBoardState) {
  localStorage.setItem(previewJiraBoardKey, JSON.stringify(state));
}
const previewSelectedBoards = (state: PreviewBoardState) =>
  Array.isArray(state.boards)
    ? state.boards
    : previewBoards.map((b) => ({ ...b }));
/** Zapis do symulowanej Jiry: główny przełącznik i błąd `fail` (jak `jira::write`). */
function previewWrite() {
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const fail = previewJiraState().fail;
  if (fail === 403)
    throw new Error("Brak uprawnień do zmiany tego zgłoszenia w Jirze.");
  if (fail === 400)
    throw new Error(
      "Jira odrzuciła zmianę. Przejście może wymagać wypełnienia pól na ekranie w Jirze — zrób je w przeglądarce.",
    );
}
const previewIssueUrl = (issueKey: string) =>
  `${previewJiraSite}/browse/${issueKey}`;
function previewCategory(status: string) {
  const id = previewStatusId(status);
  return id ? previewCategories[id] : previewStatusCategory(status);
}

/** `jira_settings` (w podglądzie połączone konto przykładowe). */
export async function jiraAccount(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_settings");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  return {
    config: {
      site: previewJiraSite,
      email: "jan.kowalski@firma.example",
      displayName: previewJiraMe.displayName,
      boards: previewSelectedBoards(state),
    },
    connected: true,
  };
}
const previewOnlyAccount =
  "W podglądzie Jira jest symulowana. Konto łączysz w aplikacji Windows.";
export async function jiraConnect(
  site: string,
  email: string,
  apiToken: string,
): Promise<JiraAccount> {
  if (desktop) return invoke("jira_connect", { site, email, apiToken });
  throw new Error(previewOnlyAccount);
}
export async function jiraDisconnect(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_disconnect");
  throw new Error(previewOnlyAccount);
}
/** `jira_boards`: wyszukiwanie tablic. */
export async function jiraBoards(
  name: string,
  startAt: number,
): Promise<{ values: JiraBoardRef[]; nextStart: number | null }> {
  if (desktop) return invoke("jira_boards", { name, startAt });
  const q = name.trim().toLocaleLowerCase("pl");
  return {
    values: previewBoards.filter((b) =>
      b.name.toLocaleLowerCase("pl").includes(q),
    ),
    nextStart: null,
  };
}
/** `jira_select_boards`: wybrane tablice (najwyżej 20). */
export async function jiraSelectBoards(boards: JiraBoardRef[]): Promise<void> {
  if (desktop) return invoke("jira_select_boards", { boards });
  if (boards.length > 20) throw new Error("Wybierz maksymalnie 20 tablic.");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  state.boards = boards.map((b) => ({ id: b.id, name: b.name }));
  savePreviewBoard(state);
}
/** `jira_issues`: lista „Przypisane do mnie”. */
export async function jiraIssues(
  openOnly: boolean,
): Promise<{ issues: JiraListIssue[]; warnings: string[] }> {
  if (desktop) return invoke("jira_issues", { openOnly });
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  const boards = previewSelectedBoards(state);
  if (!boards.length) throw new Error("Najpierw wybierz tablice.");
  return {
    issues: issues
      .filter((i) => i.assignee?.accountId === previewJiraMe.accountId)
      .filter((i) => !openOnly || previewCategory(i.status) !== "done")
      .map((i) => ({
        key: i.key,
        summary: i.summary,
        status: i.status,
        boards: boards
          .filter((b) => i.boards.includes(b.id))
          .map((b) => b.name),
        url: previewIssueUrl(i.key),
      }))
      .filter((i) => i.boards.length)
      .sort((a, b) => a.key.localeCompare(b.key, "pl", { numeric: true })),
    warnings: [],
  };
}
/** `jira_start`: zadanie zgłoszenia (nowe albo z kosza/archiwum) i jeden pomiar. */
export async function jiraStart(issueKey: string): Promise<Snapshot> {
  if (desktop) return invoke("jira_start", { key: issueKey });
  checkKey(issueKey);
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue) throw new Error("Odśwież listę zgłoszeń przed rozpoczęciem.");
  const data = await read();
  const now = Date.now();
  let task = data.tasks.find((t) => t.jiraKey === issueKey);
  const trashed = data.trash?.find((t) => t.jiraKey === issueKey);
  if (!task && trashed) {
    data.trash = data.trash?.filter((t) => t !== trashed);
    data.sessions.push(
      ...(data.trashSessions ?? []).filter((s) => s.taskId === trashed.id),
    );
    data.trashSessions = data.trashSessions?.filter(
      (s) => s.taskId !== trashed.id,
    );
    data.tasks.unshift(trashed);
    task = trashed;
  }
  if (!task) {
    task = {
      id:
        Math.max(
          0,
          ...data.tasks.map((t) => t.id),
          ...(data.trash ?? []).map((t) => t.id),
        ) + 1,
      title: [...issue.summary].slice(0, 240).join(""),
      done: false,
      createdAt: now,
      jiraKey: issueKey,
      jiraUrl: previewIssueUrl(issueKey),
      jiraStatus: issue.status,
    };
    data.tasks.unshift(task);
  }
  const started = task;
  started.done = false;
  started.archived = false;
  started.details = unblocked(started);
  if (
    !data.sessions.some((s) => s.taskId === started.id && s.endedAt === null)
  ) {
    data.sessions.forEach((s) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });
    data.sessions.unshift({
      id:
        Math.max(
          0,
          ...data.sessions.map((s) => s.id),
          ...(data.trashSessions ?? []).map((s) => s.id),
        ) + 1,
      taskId: started.id,
      startedAt: now,
      endedAt: null,
    });
  }
  localStorage.setItem(key, JSON.stringify(data));
  return data;
}
/** `jira_open`: zgłoszenie w przeglądarce (w podglądzie tylko zapisany link). */
export async function jiraOpen(url: string): Promise<void> {
  if (desktop) return invoke("jira_open", { url });
  try {
    const opened = JSON.parse(
      localStorage.getItem(previewJiraOpenedKey) ?? "[]",
    );
    localStorage.setItem(
      previewJiraOpenedKey,
      JSON.stringify(
        [...(Array.isArray(opened) ? opened : []), url].slice(-20),
      ),
    );
  } catch {
    /* tylko ślad dla testów */
  }
}
/** `jira_board`: kolumny i karty tablicy. */
export async function jiraBoard(
  boardId: number,
  scope: JiraScope,
  openOnly: boolean,
  refresh: boolean,
): Promise<JiraBoardView> {
  if (desktop)
    return invoke("jira_board", { boardId, scope, openOnly, refresh });
  const { state, issues } = await previewBoardState();
  savePreviewBoard(state);
  if (!previewSelectedBoards(state).some((b) => b.id === boardId))
    throw new Error(
      "Tej tablicy nie ma wśród wybranych. Wybierz ją w widoku Jira.",
    );
  const cards = issues
    .filter((i) => i.boards.includes(boardId))
    .filter((i) =>
      scope === "mine"
        ? i.assignee?.accountId === previewJiraMe.accountId &&
          (!openOnly || previewCategory(i.status) !== "done")
        : previewCategory(i.status) !== "done",
    )
    .map((i): JiraBoardIssue => {
      const statusId = previewStatusId(i.status);
      const category = previewCategory(i.status);
      return {
        key: i.key,
        summary: i.summary,
        status: i.status,
        statusId,
        category,
        issueType: i.issueType,
        priority: i.priority,
        assignee: i.assignee ? { ...i.assignee } : null,
        url: previewIssueUrl(i.key),
        column: columnFor(
          statusId,
          category,
          previewColumns,
          previewCategories,
        ),
      };
    });
  return {
    boardId,
    project: previewBoardProject[boardId] ?? "",
    columns: previewColumns.map((c) => ({
      ...c,
      statusIds: [...c.statusIds],
    })),
    issues: cards.slice(0, jiraIssueLimit),
    truncated: cards.length > jiraIssueLimit,
    limit: jiraIssueLimit,
    layoutAt: Date.now(),
  };
}
/** Status zgłoszenia w symulowanej Jirze i na kafelkach zadań z tym kluczem. */
async function previewSetStatus(issueKey: string, status: string) {
  const { state, data, issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (issue) issue.status = status;
  for (const task of [...data.tasks, ...(data.trash ?? [])])
    if (task.jiraKey === issueKey) task.jiraStatus = status;
  savePreviewBoard(state);
  localStorage.setItem(key, JSON.stringify(data));
}
/** `jira_board_move`: przeciągnięcie karty i „Przesuń do…”. */
export async function jiraBoardMove(
  boardId: number,
  issueKey: string,
  column: number,
  transitionId: string | null,
): Promise<JiraMoveOutcome> {
  if (desktop)
    return invoke("jira_board_move", {
      boardId,
      key: issueKey,
      column,
      transitionId,
    });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const { issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue)
    throw new Error("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  const target = previewColumns[column];
  if (!target) throw new Error("Nie ma takiej kolumny na tablicy.");
  const decision = decideMove(
    previewMoveOptions(issue.status),
    column,
    previewColumns,
    previewCategories,
    transitionId,
  );
  if (decision.kind === "none")
    throw new Error(
      `Jira nie ma przejścia ${issueKey} do kolumny „${target.name}”. Zgłoszenie zostaje w poprzedniej kolumnie.`,
    );
  if (decision.kind === "choose")
    return { result: "choose", options: decision.options };
  const o = decision.option;
  if (decision.kind === "screen")
    return {
      result: "screen",
      url: previewIssueUrl(issueKey),
      fields: o.required,
      message: `Przejście „${o.name}” wymaga wypełnienia pól w Jirze (${o.required.join(", ")}). Otwieram ${issueKey} w przeglądarce — dokończ zmianę tam.`,
    };
  previewWrite();
  await previewSetStatus(issueKey, o.toStatus);
  return {
    result: "moved",
    status: o.toStatus,
    statusId: o.toStatusId,
    category: o.toCategory,
    column: columnFor(
      o.toStatusId,
      o.toCategory,
      previewColumns,
      previewCategories,
    ),
  };
}
/** `jira_assignable`: osoby do przypisania. */
export async function jiraAssignable(
  issueKey: string,
  query: string,
): Promise<JiraPerson[]> {
  if (desktop) return invoke("jira_assignable", { key: issueKey, query });
  checkKey(issueKey);
  const q = query.trim().toLocaleLowerCase("pl");
  if ([...q].length > 100) throw new Error("Wpisz najwyżej 100 znaków.");
  return previewPeople
    .filter((p) => p.displayName.toLocaleLowerCase("pl").includes(q))
    .map((p) => ({ ...p }));
}
/** `jira_assign`: „Przypisz do mnie” (`accountId` = null) i „Zmień osobę…”. */
export async function jiraAssign(
  issueKey: string,
  accountId: string | null,
): Promise<JiraPerson> {
  if (desktop) return invoke("jira_assign", { key: issueKey, accountId });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const who =
    accountId === null
      ? previewJiraMe
      : previewPeople.find((p) => p.accountId === accountId);
  if (!who) throw new Error("Niepoprawna osoba w Jirze.");
  previewWrite();
  const { state, issues } = await previewBoardState();
  const issue = issues.find((i) => i.key === issueKey);
  if (!issue)
    throw new Error("Nie znaleziono zgłoszenia albo przejścia w Jirze.");
  issue.assignee = { ...who };
  savePreviewBoard(state);
  return { ...who };
}
/** `jira_comment`: komentarz zwykłym tekstem (w Jirze jako ADF). */
export async function jiraComment(
  issueKey: string,
  text: string,
): Promise<void> {
  if (desktop) return invoke("jira_comment", { key: issueKey, text });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  checkKey(issueKey);
  const body = text.replace(/\r\n?/g, "\n").trim();
  if (!body) throw new Error("Wpisz treść komentarza.");
  if ([...body].length > jiraCommentLimit)
    throw new Error(
      `Komentarz może mieć najwyżej ${jiraCommentLimit} znaków.`,
    );
  previewWrite();
  const { state } = await previewBoardState();
  const comments = (state.comments ??= {});
  comments[issueKey] = [...(comments[issueKey] ?? []), body];
  savePreviewBoard(state);
}
/** `jira_branch_name`: nazwa gałęzi z prefiksem z ustawień. */
export async function jiraBranchName(
  issueKey: string,
  title: string,
): Promise<string> {
  if (desktop) return invoke("jira_branch_name", { key: issueKey, title });
  checkKey(issueKey);
  const s = previewJson(previewSettingsKey);
  const prefix =
    typeof s.jiraBranchPrefix === "string" ? s.jiraBranchPrefix : "feature/";
  return branchName(prefix, issueKey, title);
}
/** `jira_projects`: projekty wybranych tablic. */
export async function jiraProjects(): Promise<JiraProject[]> {
  if (desktop) return invoke("jira_projects");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  const out: JiraProject[] = [];
  for (const b of previewSelectedBoards(state)) {
    const project = previewBoardProject[b.id];
    if (!project) continue;
    const known = out.find((p) => p.key === project);
    if (known) known.boards.push(b.name);
    else out.push({ key: project, boards: [b.name] });
  }
  if (!out.length)
    throw new Error("Wybrane tablice nie należą do projektu Jiry.");
  return out;
}
const projectPattern = /^[A-Z][A-Z0-9_]{0,19}$/;
/** `jira_issue_types`: typy zgłoszeń projektu. */
export async function jiraIssueTypes(
  project: string,
): Promise<JiraIssueType[]> {
  if (desktop) return invoke("jira_issue_types", { project });
  if (!projectPattern.test(project)) throw new Error("Wybierz projekt w Jirze.");
  return previewTypes.map(({ id, name }) => ({ id, name }));
}
/** `jira_issue_type_fields`: wymagane pola spoza formularza. */
export async function jiraIssueTypeFields(
  project: string,
  issueTypeId: string,
): Promise<JiraTypeFields> {
  if (desktop)
    return invoke("jira_issue_type_fields", { project, issueTypeId });
  const type = previewTypes.find((t) => t.id === issueTypeId);
  if (!type || !projectPattern.test(project))
    throw new Error("Wybierz projekt i typ zgłoszenia.");
  return {
    extraRequired: [...type.required],
    createUrl: `${previewJiraSite}/secure/CreateIssue!default.jspa`,
  };
}
/** Walidacja formularza jak `jira_board::validate_new_issue`; zwraca tytuł. */
export function validateNewIssue(n: JiraNewIssue): string {
  if (!projectPattern.test(n.project))
    throw new Error("Wybierz projekt w Jirze.");
  if (!/^\d{1,20}$/.test(n.issueTypeId))
    throw new Error("Wybierz typ zgłoszenia.");
  const summary = n.summary.trim();
  if (!summary || [...summary].length > 255 || /[\r\n]/.test(summary))
    throw new Error(
      "Tytuł zgłoszenia musi mieć od 1 do 255 znaków w jednej linii.",
    );
  if ([...n.description].length > jiraCommentLimit)
    throw new Error(`Opis może mieć najwyżej ${jiraCommentLimit} znaków.`);
  if (n.plannedDay && !/^\d{4}-\d{2}-\d{2}$/.test(n.plannedDay))
    throw new Error("Dzień planu musi mieć format RRRR-MM-DD.");
  return summary;
}
/** `jira_create`: nowe zgłoszenie i opcjonalnie zadanie w Todo Track. */
export async function jiraCreate(issue: JiraNewIssue): Promise<JiraCreated> {
  if (desktop) return invoke("jira_create", { issue });
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const summary = validateNewIssue(issue);
  const type = previewTypes.find((t) => t.id === issue.issueTypeId);
  if (!type) throw new Error("Wybierz typ zgłoszenia.");
  if (type.required.length)
    throw new Error(
      `Ten typ zgłoszenia wymaga pól, których Todo Track nie wypełnia (${type.required.join(", ")}). Utwórz zgłoszenie w Jirze.`,
    );
  previewWrite();
  const { state, issues } = await previewBoardState();
  const number = state.next ?? 100;
  state.next = number + 1;
  const issueKey = `${issue.project}-${number}`;
  issues.push({
    key: issueKey,
    summary,
    status: "Do zrobienia",
    issueType: type.name,
    priority: "Medium",
    assignee: issue.assignMe ? { ...previewJiraMe } : null,
    boards: Object.entries(previewBoardProject)
      .filter(([, p]) => p === issue.project)
      .map(([id]) => Number(id)),
  });
  savePreviewBoard(state);
  let snapshot: Snapshot | null = null;
  if (issue.createTask) {
    const data = await read();
    const linked = data.tasks.find((t) => t.jiraKey === issueKey);
    if (linked) {
      // Jak w Ruście: zadanie zaimportowane wcześniej, bez drugiej kopii.
      if (issue.plannedDay)
        linked.details = { ...detailsOf(linked), plannedDay: issue.plannedDay };
      localStorage.setItem(key, JSON.stringify(data));
      return { key: issueKey, url: previewIssueUrl(issueKey), snapshot: data, warning: null };
    }
    const created: Task = {
      id:
        Math.max(
          0,
          ...data.tasks.map((t) => t.id),
          ...(data.trash ?? []).map((t) => t.id),
        ) + 1,
      title: [...summary].slice(0, 240).join(""),
      done: false,
      createdAt: Date.now(),
      jiraKey: issueKey,
      jiraUrl: previewIssueUrl(issueKey),
    };
    if (issue.plannedDay)
      created.details = { ...detailsOf(created), plannedDay: issue.plannedDay };
    data.tasks.unshift(created);
    localStorage.setItem(key, JSON.stringify(data));
    snapshot = data;
  }
  return { key: issueKey, url: previewIssueUrl(issueKey), snapshot, warning: null };
}
