/**
 * Symulowana Jira podglądu: pytania o przesunięcie przy starcie pracy (J1),
 * tablica, przejścia, przypisanie, komentarze i nowe zgłoszenia (J2), z tymi
 * samymi regułami co aplikacja (`columnFor`, `decideMove`, `branchName`).
 * Błąd zapisu symuluje `fail` (400 albo 403) ze stanu J1.
 */
import { read, saveSnapshot } from "./snapshot";
import {
  bandName,
  notifyPreviewJira,
  previewBoardProject,
  previewBoards,
  previewCategories,
  previewColumns,
  previewFieldCatalog,
  previewIssueFields,
  previewIssueSeed,
  previewJiraBoardKey,
  previewJiraMe,
  previewJiraOpenedKey,
  previewJiraSettings,
  previewJiraState,
  previewMoveOptions,
  previewPeople,
  previewStatusCategory,
  previewStatusId,
  previewTransitionsFor,
  previewTypes,
  savePreviewJira,
  type PreviewBoardState,
  type PreviewIssue,
  type PreviewJira,
} from "./jira-state";
import { validRedmineId } from "../issue";
import { previewRedmineStartPrompt } from "./redmine";
import {
  branchName,
  checkKey,
  chooseTransition,
  columnFor,
  decideMove,
  jiraCommentLimit,
  jiraIssueLimit,
  jiraWriteDisabled,
  emptyJiraWatch,
  priorityBand,
  projectPattern,
  validateNewIssue,
  type JiraAccount,
  type JiraBoardIssue,
  type JiraBoardRef,
  type JiraBoardView,
  type JiraCreated,
  type JiraFieldInfo,
  type JiraIssueType,
  type JiraListIssue,
  type JiraMoveOutcome,
  type JiraMovePrompt,
  type JiraNewIssue,
  type JiraPerson,
  type JiraProject,
  type JiraScope,
  type JiraTransition,
  type JiraTypeFields,
  type JiraWatchStatus,
} from "../api/jira";
import {
  notifyPreviewData,
  previewJiraSite,
  previewJson,
  previewSettingsKey,
} from "./storage";
import { detailsOf, unblocked, type Snapshot, type Task } from "../api/task";
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
  if (task && !task.jiraKey && validRedmineId(task.redmineId)) {
    const prompt = previewRedmineStartPrompt(task, data);
    if (prompt) state.prompts = [prompt];
  }
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
      saveSnapshot(data);
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
            system: "jira",
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
export async function readMovePrompts(): Promise<JiraMovePrompt[]> {
  return (await previewObserveStart()).prompts ?? [];
}
export async function dismissMovePrompt(
  taskId: number,
): Promise<JiraMovePrompt[]> {
  const state = previewJiraState();
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return state.prompts;
}
export async function setJiraNoAsk(
  taskId: number,
  noAsk: boolean,
): Promise<Snapshot> {
  const data = await read();
  const task = data.tasks.find((t) => t.id === taskId);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), jiraNoAsk: noAsk };
  saveSnapshot(data);
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
export async function jiraTransitions(
  taskId: number,
): Promise<JiraTransition[]> {
  const { task } = await previewJiraTask(taskId);
  return previewTransitionsFor(task.jiraStatus || "Do zrobienia");
}
export async function jiraTransition(
  taskId: number,
  transitionId: string,
): Promise<Snapshot> {
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
  saveSnapshot(data);
  state.prompts = (state.prompts ?? []).filter((p) => p.taskId !== taskId);
  savePreviewJira(state);
  notifyPreviewJira();
  return data;
}
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
        priority: bandName(task.details?.priority ?? 0),
        assignee: { ...previewJiraMe },
        boards: [1],
      });
    else if (task.jiraStatus) issue.status = task.jiraStatus;
  }
  state.issues = issues;
  // Priorytet z Jiry na zadanie przy pierwszym pobraniu i każdej zmianie
  // (jak `jira_fields::save_fields`); ręczne pasmo zostaje, dopóki Jira go nie zmieni.
  const applied = (state.priorities ??= {});
  let changed = false;
  for (const task of data.tasks) {
    const issue = task.jiraKey
      ? issues.find((i) => i.key === task.jiraKey)
      : undefined;
    if (!issue || applied[issue.key] === issue.priority) continue;
    applied[issue.key] = issue.priority;
    const band = priorityBand(issue.priority);
    if (band !== null && (task.details?.priority ?? 0) !== band) {
      task.details = { ...detailsOf(task), priority: band };
      changed = true;
    }
  }
  if (changed) {
    saveSnapshot(data);
    notifyPreviewData();
  }
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
export async function jiraAccount(): Promise<JiraAccount> {
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
/** Co ile symulowana obserwacja sprawdza przypisania (jak `jira::WATCH_EVERY`). */
const previewWatchEvery = 5 * 60_000;
/**
 * `jira_watch_status` (B5): symulowana obserwacja przypisań. Przy włączonym
 * „Dodawaj nowe zgłoszenia przypisane do mnie” odczyt wykonuje zaległe
 * sprawdzenie (ostatnie starsze niż 5 min): sukces albo błąd `watchFail`,
 * zapisany jak `JiraState::record_watch` (błąd nie zmienia czasu sukcesu).
 */
export async function jiraWatchStatus(): Promise<JiraWatchStatus> {
  const state = previewJiraState();
  const watch = state.watch ?? emptyJiraWatch();
  if (previewJson(previewSettingsKey).jiraAutoImport === false) return watch;
  const now = Date.now();
  const last = Math.max(watch.lastOk ?? 0, watch.lastErrorAt ?? 0);
  if (now - last < previewWatchEvery) return watch;
  const next: JiraWatchStatus = state.watchFail
    ? { ...watch, lastError: state.watchFail, lastErrorAt: now }
    : { lastOk: now, lastError: "", lastErrorAt: null };
  savePreviewJira({ ...state, watch: next });
  return next;
}
const previewOnlyAccount =
  "W podglądzie Jira jest symulowana. Konto łączysz w aplikacji Windows.";
export async function jiraConnect(
  site: string,
  email: string,
  apiToken: string,
): Promise<JiraAccount> {
  throw new Error(previewOnlyAccount);
}
export async function jiraDisconnect(): Promise<JiraAccount> {
  throw new Error(previewOnlyAccount);
}
export async function jiraBoards(
  name: string,
  startAt: number,
): Promise<{ values: JiraBoardRef[]; nextStart: number | null }> {
  const q = name.trim().toLocaleLowerCase("pl");
  return {
    values: previewBoards.filter((b) =>
      b.name.toLocaleLowerCase("pl").includes(q),
    ),
    nextStart: null,
  };
}
export async function jiraFields(refresh: boolean): Promise<JiraFieldInfo[]> {
  return previewFieldCatalog.map((f) => ({ ...f }));
}
export async function jiraRefreshFields(): Promise<number> {
  const { state, data } = await previewBoardState();
  savePreviewBoard(state);
  notifyPreviewData();
  return data.tasks.filter((t) => t.jiraKey && !t.done).length;
}
export async function jiraSelectBoards(boards: JiraBoardRef[]): Promise<void> {
  if (boards.length > 20) throw new Error("Wybierz maksymalnie 20 tablic.");
  const state = previewJson(previewJiraBoardKey) as PreviewBoardState;
  state.boards = boards.map((b) => ({ id: b.id, name: b.name }));
  savePreviewBoard(state);
}
/**
 * Tylko testy: wywołanie symulowanej Jiry czeka, dopóki jego nazwa
 * („read”, „create”, „move”) jest na liście pod tym kluczem. Pozwala
 * sprawdzić wyścigi (np. zapis formularza w trakcie odświeżania).
 */
export const previewJiraHoldKey = "tuktuk-track-preview-jira-hold-v1";
async function previewHold(name: "read" | "create" | "move") {
  const held = () => {
    try {
      const list = JSON.parse(localStorage.getItem(previewJiraHoldKey) ?? "[]");
      return Array.isArray(list) && list.includes(name);
    } catch {
      return false;
    }
  };
  while (held()) await new Promise((resolve) => setTimeout(resolve, 20));
}
export async function jiraIssues(
  openOnly: boolean,
): Promise<{ issues: JiraListIssue[]; warnings: string[] }> {
  await previewHold("read");
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
        fields: previewIssueFields(i),
      }))
      .filter((i) => i.boards.length)
      .sort((a, b) => a.key.localeCompare(b.key, "pl", { numeric: true })),
    warnings: [],
  };
}
export async function jiraStart(issueKey: string): Promise<Snapshot> {
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
    // Jak `jira::start_issue`: nowe zadanie z pasmem priorytetu zgłoszenia
    // i polami z ustawień (w aplikacji dokłada je `db::snapshot`).
    const band = priorityBand(issue.priority);
    if (band !== null) task.details = { ...detailsOf(task), priority: band };
    task.jiraFields = previewIssueFields(issue);
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
  saveSnapshot(data);
  return data;
}
export async function jiraOpen(url: string): Promise<void> {
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
export async function jiraBoard(
  boardId: number,
  scope: JiraScope,
  openOnly: boolean,
  refresh: boolean,
): Promise<JiraBoardView> {
  await previewHold("read");
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
        fields: previewIssueFields(i),
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
  saveSnapshot(data);
}
export async function jiraBoardMove(
  boardId: number,
  issueKey: string,
  column: number,
  transitionId: string | null,
): Promise<JiraMoveOutcome> {
  await previewHold("move");
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
export async function jiraAssignable(
  issueKey: string,
  query: string,
): Promise<JiraPerson[]> {
  checkKey(issueKey);
  const q = query.trim().toLocaleLowerCase("pl");
  if ([...q].length > 100) throw new Error("Wpisz najwyżej 100 znaków.");
  return previewPeople
    .filter((p) => p.displayName.toLocaleLowerCase("pl").includes(q))
    .map((p) => ({ ...p }));
}
export async function jiraAssign(
  issueKey: string,
  accountId: string | null,
): Promise<JiraPerson> {
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
export async function jiraComment(
  issueKey: string,
  text: string,
): Promise<void> {
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
export async function jiraBranchName(
  issueKey: string,
  title: string,
): Promise<string> {
  checkKey(issueKey);
  const s = previewJson(previewSettingsKey);
  const prefix =
    typeof s.jiraBranchPrefix === "string" ? s.jiraBranchPrefix : "feature/";
  return branchName(prefix, issueKey, title);
}
export async function jiraProjects(): Promise<JiraProject[]> {
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
export async function jiraIssueTypes(
  project: string,
): Promise<JiraIssueType[]> {
  if (!projectPattern.test(project)) throw new Error("Wybierz projekt w Jirze.");
  return previewTypes.map(({ id, name }) => ({ id, name }));
}
export async function jiraIssueTypeFields(
  project: string,
  issueTypeId: string,
): Promise<JiraTypeFields> {
  const type = previewTypes.find((t) => t.id === issueTypeId);
  if (!type || !projectPattern.test(project))
    throw new Error("Wybierz projekt i typ zgłoszenia.");
  return {
    extraRequired: [...type.required],
    createUrl: `${previewJiraSite}/secure/CreateIssue!default.jspa`,
  };
}
export async function jiraCreate(issue: JiraNewIssue): Promise<JiraCreated> {
  await previewHold("create");
  if (!previewJiraSettings().write) throw new Error(jiraWriteDisabled);
  const summary = validateNewIssue(issue);
  const type = previewTypes.find((t) => t.id === issue.issueTypeId);
  if (!type) throw new Error("Wybierz typ zgłoszenia.");
  if (type.required.length)
    throw new Error(
      `Ten typ zgłoszenia wymaga pól, których TukTuk Track nie wypełnia (${type.required.join(", ")}). Utwórz zgłoszenie w Jirze.`,
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
      saveSnapshot(data);
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
    saveSnapshot(data);
    snapshot = data;
  }
  return { key: issueKey, url: previewIssueUrl(issueKey), snapshot, warning: null };
}
