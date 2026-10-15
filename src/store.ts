import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ref } from "vue";
import { renderTitle, type Parsed } from "./quickadd";
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
  ...task.details,
  notes: { ...task.details?.notes },
  checklist: (task.details?.checklist ?? []).map((item) => ({ ...item })),
  repeatDays: [...(task.details?.repeatDays ?? [])],
});
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
    task.details = { ...detailsOf(task), waiting: "" };
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
const trimFolder = (folder: string) => folder.trim().replace(/[\\/]+$/, "");
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
    target.details = { ...detailsOf(target), waiting: "" };
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
  if (saved.repeatDays.length && !saved.repeatFrom)
    saved.repeatFrom = localDay(Date.now());
  if (saved.repeatDays.length && !saved.plannedDay)
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
function generateRecurring(data: Snapshot): boolean {
  let changed = false;
  const today = localDay(Date.now());
  const earliest = new Date();
  earliest.setDate(earliest.getDate() - 365);
  for (const source of [...data.tasks]) {
    const rule = source.details;
    if (!rule?.repeatDays?.length || !rule.repeatFrom || source.archived)
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
        rule.repeatDays.includes(((cursor.getDay() + 6) % 7) + 1) &&
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
  /** Grupa listy zadań, do której przewinąć (np. „Nierozpoczęte”). */
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
