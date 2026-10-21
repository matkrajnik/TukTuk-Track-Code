/**
 * Podgląd komend zadań, sesji i planu (`mutate`, `quick_add`, `save_details`,
 * …) oraz drobnych ustawień okna: autostart, widget, wnioski tygodnia,
 * rozszerzenie przeglądarki, eksport.
 */
import { read, saveSnapshot } from "./snapshot";
import {
  browserExtensionId,
  detailsOf,
  issueStatus,
  maxPinned,
  mergeTemplate,
  oneIssueError,
  pinnedTasks,
  resumeCandidate,
  TRASH_IN_GAP,
  unblocked,
  validTime,
  widgetEvent,
  type Autostart,
  type BrowserStatus,
  type ExportFormat,
  type ExportResult,
  type DayNote,
  type GapMode,
  type NewTask,
  type PlanEntry,
  type Session,
  type Snapshot,
  type TaskDetails,
  type ToggleResult,
  type WaitingChange,
  type WidgetStatus,
} from "../api/task";
import { redmineKey, validRedmineId } from "../issue";
import {
  previewRedmineIssue,
  previewRedmineSite,
  previewRedmineState,
} from "./redmine";
import { findTemplate, previewTemplates } from "./templates";
import { renderTitle, unknownTitle } from "../quickadd";
import { dayKey } from "../time";
import { previewJiraSite, previewJson, previewSettingsKey } from "./storage";
import { tsValidTaskId, tsValidTaskName } from "../api/timesheet";
import { isRecurring, normalizeRule, ruleUsesWeekdays } from "../recurrence";
import { exportCsv, exportRows } from "../export";
export async function change(
  action: string,
  id: number | null = null,
  title: string | null = null,
): Promise<Snapshot> {
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
    // Jak `task_done` w Rust: chwila ukończenia tylko dla ukończonego zadania.
    task.doneAt = task.done ? now : null;
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
  saveSnapshot(data);
  return data;
}
export async function quickAdd(task: NewTask): Promise<Snapshot> {
  const jiraKey = task.jiraKey?.trim() || null;
  if (jiraKey && !/^[A-Z0-9_]+-\d+$/.test(jiraKey))
    throw new Error("Niepoprawny klucz zgłoszenia Jiry.");
  const redmineId = task.redmineId ?? null;
  if (redmineId !== null && !validRedmineId(redmineId))
    throw new Error("Niepoprawny numer zgłoszenia Redmine.");
  if (jiraKey && redmineId !== null) throw new Error(oneIssueError);
  if (redmineId !== null && previewRedmineState().connected === false)
    throw new Error(
      `Redmine nie jest połączone, więc #${redmineId} nie zostanie powiązane. Połącz konto w Ustawieniach → Redmine albo poprzedź # znakiem \\.`,
    );
  const redmineIssue =
    redmineId !== null ? previewRedmineIssue(redmineId) : undefined;
  const name = task.template?.trim();
  const template = name ? findTemplate(previewTemplates(), name) : undefined;
  if (name && !template)
    throw new Error(
      `Nie ma szablonu „${name}”. Dodaj go w Ustawieniach → Szablony.`,
    );
  // Etykieta zgłoszenia do szablonu: klucz Jiry albo `#numer`; temat tylko z
  // symulowanego Redmine (listy Jiry w podglądzie nie ma).
  const label = jiraKey ?? (redmineId !== null ? redmineKey(redmineId) : null);
  const subject = () => redmineIssue?.subject ?? null;
  const title = template
    ? [...renderTitle(template.title, label, task.title, subject)]
        .slice(0, 240)
        .join("")
    : task.title.trim() || [...(subject() ?? "").trim()].slice(0, 240).join("");
  if (!title && label) throw new Error(unknownTitle(label));
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
          dayKey(+new Date(d + "T12:00:00")) !== d),
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
  if (redmineId !== null) {
    const existing = [...data.tasks, ...(data.trash ?? [])].find(
      (t) => t.redmineId === redmineId,
    );
    if (existing)
      throw new Error(
        `#${redmineId} ma już zadanie „${existing.title}”${data.trash?.includes(existing) ? " (w koszu)" : ""}.`,
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
    ...(redmineId !== null
      ? {
          redmineId,
          redmineUrl: `${previewRedmineSite}/issues/${redmineId}`,
          redmineStatus: redmineIssue?.status ?? "",
          redmineTracker: redmineIssue?.tracker ?? "",
          redmineDue: redmineIssue?.dueDate ?? "",
        }
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
  saveSnapshot(data);
  return data;
}
/** Niepuste przedziały spoza `skip` zachodzące na [`start`, `end`), także zadań
 * w koszu (jak db::overlapping: wracają przy przywróceniu zadania). */
const overlapping = (
  data: Snapshot,
  skip: number[],
  start: number,
  end: number,
) =>
  [...data.sessions, ...(data.trashSessions ?? [])].filter(
    (s) =>
      !skip.includes(s.id) &&
      s.startedAt < end &&
      (s.endedAt === null || s.endedAt > start) &&
      (s.endedAt === null || s.endedAt > s.startedAt),
  );
const overlaps = (data: Snapshot, skip: number[], start: number, end: number) =>
  overlapping(data, skip, start, end).length > 0;
/** db::edit_active_start: początek trwającego pomiaru, koniec bez zmian. */
async function editActiveStart(
  id: number,
  startedAt: number,
): Promise<Snapshot> {
  const now = Date.now();
  if (!Number.isSafeInteger(startedAt) || startedAt < 0 || startedAt >= now)
    throw new Error(
      "Początek trwającego pomiaru musi być wcześniejszy niż teraz.",
    );
  const data = await read();
  const session = data.sessions.find((s) => s.id === id);
  if (!session) throw new Error("Nie znaleziono przedziału.");
  if (session.endedAt !== null)
    throw new Error(
      "Pomiar został już zatrzymany. Popraw oba końce przedziału.",
    );
  if (overlaps(data, [id], startedAt, now))
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  session.startedAt = startedAt;
  saveSnapshot(data);
  return data;
}
/** db::fill_gap: przypisanie luki między pomiarami. */
export async function fillGap(
  beforeId: number,
  afterId: number,
  mode: GapMode,
): Promise<Snapshot> {
  const data = await read();
  const trashed = (id: number) =>
    data.trashSessions?.some((s) => s.id === id);
  if (trashed(beforeId) || trashed(afterId))
    throw new Error("Przedział należy do zadania w koszu.");
  const before = data.sessions.find((s) => s.id === beforeId);
  const after = data.sessions.find((s) => s.id === afterId);
  if (!before || !after) throw new Error("Nie znaleziono przedziału.");
  if (before.endedAt === null || beforeId === afterId)
    throw new Error("Lukę można przypisać tylko po zakończonym pomiarze.");
  const start = before.endedAt,
    end = after.startedAt;
  if (end <= start) throw new Error("Między tymi pomiarami nie ma luki.");
  const blocking = overlapping(data, [beforeId, afterId], start, end);
  if (blocking.some((s) => !trashed(s.id)))
    throw new Error("W tej luce leży inny pomiar. Odśwież widok.");
  if (blocking.length) throw new Error(TRASH_IN_GAP);
  const meet =
    mode === "previous"
      ? end
      : mode === "next"
        ? start
        : mode === "split"
          ? start + Math.floor((end - start) / 2)
          : NaN;
  if (Number.isNaN(meet)) throw new Error("Nieznany sposób przypisania luki.");
  before.endedAt = meet;
  after.startedAt = meet;
  saveSnapshot(data);
  return data;
}
export async function editSession(
  id: number,
  startedAt: number,
  endedAt: number | null,
): Promise<Snapshot> {
  if (endedAt === null) return editActiveStart(id, startedAt);
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
  if (overlaps(data, [id], startedAt, endedAt))
    throw new Error(
      "Ten przedział nakłada się na inny pomiar. Popraw godziny.",
    );
  session.startedAt = startedAt;
  session.endedAt = endedAt;
  saveSnapshot(data);
  return data;
}
export async function splitSession(
  id: number,
  at: number,
  targetTask: number | null = null,
): Promise<Snapshot> {
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
  saveSnapshot(data);
  return data;
}
export async function saveDetails(
  id: number,
  details: TaskDetails,
): Promise<Snapshot> {
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  if (details.plannedStart && !validTime(details.plannedStart))
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const saved: TaskDetails = JSON.parse(JSON.stringify(details));
  saved.project = details.project.trim();
  saved.waiting = details.waiting.trim();
  // Zadanie TS jak planner::validate: puste id czyści nazwę, puste pola znikają.
  const tsId = (details.tsTaskId ?? "").trim();
  const tsName = (details.tsTaskName ?? "").trim();
  delete saved.tsTaskId;
  delete saved.tsTaskName;
  if (tsId) {
    if (!tsValidTaskId(tsId) || !tsValidTaskName(tsName))
      throw new Error("Niepoprawne zadanie TS.");
    saved.tsTaskId = tsId;
    if (tsName) saved.tsTaskName = tsName;
  }
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
    if (wasWaiting) saved.waitingDismissed = issueStatus(task);
  } else {
    // Nowa blokada zaczyna bez przypomnienia (jak planner::save).
    if (!wasWaiting) saved.waitingRemindDay = "";
    if (!saved.waitingSince) saved.waitingSince = Date.now();
    if (!saved.waitingStatus) saved.waitingStatus = issueStatus(task);
  }
  if (isRecurring(saved) && !saved.repeatFrom)
    saved.repeatFrom = dayKey(Date.now());
  if (isRecurring(saved) && !saved.plannedDay)
    saved.plannedDay = saved.repeatFrom;
  task.details = saved;
  if (saved.waiting)
    data.sessions.forEach((s) => {
      if (s.taskId === id && s.endedAt === null)
        s.endedAt = Math.max(s.startedAt, Date.now());
    });
  saveSnapshot(data);
  return data;
}
export async function setWaiting(
  id: number,
  change: WaitingChange,
): Promise<Snapshot> {
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  const text = (change.text ?? "").trim();
  const status = issueStatus(task);
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
  saveSnapshot(data);
  return data;
}
export async function addSession(
  taskId: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
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
  if (overlaps(data, [], startedAt, endedAt))
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
  saveSnapshot(data);
  return data;
}
export async function setPriority(
  id: number,
  priority: number,
): Promise<Snapshot> {
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), priority };
  saveSnapshot(data);
  return data;
}
export async function reorderTasks(
  entries: { id: number; order: number; priority: number }[],
): Promise<Snapshot> {
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
  saveSnapshot(data);
  return data;
}
export async function planTask(
  id: number,
  day: string,
  order: number,
): Promise<Snapshot> {
  if (
    (day !== "" &&
      (!/^\d{4}-\d{2}-\d{2}$/.test(day) ||
        dayKey(+new Date(day + "T12:00:00")) !== day)) ||
    !Number.isSafeInteger(order) ||
    order < 0 ||
    order > 1_000_000
  )
    throw new Error("Niepoprawny dzień planu.");
  const data = await read();
  const task = data.tasks.find((t) => t.id === id);
  if (!task) throw new Error("Nie znaleziono zadania.");
  task.details = { ...detailsOf(task), plannedDay: day, order };
  saveSnapshot(data);
  return data;
}
export async function planTasks(entries: PlanEntry[]): Promise<Snapshot> {
  if (
    entries.length > 10000 ||
    entries.some(
      (e) =>
        (e.day !== "" &&
          (!/^\d{4}-\d{2}-\d{2}$/.test(e.day) ||
            dayKey(+new Date(e.day + "T12:00:00")) !== e.day)) ||
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
  saveSnapshot(data);
  return data;
}
/** Odpowiednik `planner::save_day_notes`: tylko `notes[day]`, wszystko albo nic. */
export async function saveDayNotes(
  day: string,
  entries: DayNote[],
): Promise<Snapshot> {
  if (
    !/^\d{4}-\d{2}-\d{2}$/.test(day) ||
    dayKey(+new Date(day + "T12:00:00")) !== day ||
    entries.length > 10000 ||
    entries.some((e) => [...e.note].length > 4000)
  )
    throw new Error("Niepoprawne daty lub zbyt długa treść planu/notatki.");
  const data = await read();
  const tasks = entries.map((e) => data.tasks.find((t) => t.id === e.id));
  if (tasks.some((t) => !t)) throw new Error("Nie znaleziono zadania.");
  entries.forEach((e, i) => {
    const task = tasks[i]!;
    const details = detailsOf(task);
    const notes = { ...details.notes };
    if (e.note.trim()) notes[day] = e.note;
    else delete notes[day];
    task.details = { ...details, notes };
  });
  saveSnapshot(data);
  return data;
}
export async function closeDay(
  day: string,
  closed: boolean,
): Promise<Snapshot> {
  const data = await read();
  data.closedDays = (data.closedDays ?? []).filter((d) => d !== day);
  if (closed) data.closedDays.push(day);
  saveSnapshot(data);
  return data;
}
export async function setPinned(
  id: number,
  pinned: boolean,
): Promise<Snapshot> {
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
  saveSnapshot(data);
  return data;
}
export async function toggleMeasurement(): Promise<ToggleResult> {
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
const autostartKey = "todo-track-preview-autostart-v1";
export async function readAutostart(): Promise<Autostart> {
  return {
    enabled: previewJson(autostartKey).enabled === true,
    hidden: previewJson(previewSettingsKey).autostartHidden === true,
  };
}
export async function writeAutostart(
  enabled: boolean,
  hidden: boolean,
): Promise<Autostart> {
  localStorage.setItem(autostartKey, JSON.stringify({ enabled }));
  const settings = {
    ...previewJson(previewSettingsKey),
    autostartHidden: hidden,
  };
  localStorage.setItem(previewSettingsKey, JSON.stringify(settings));
  return readAutostart();
}
const widgetDismissedKey = "todo-track-preview-widget-dismissed";
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
export async function readWidgetStatus(): Promise<WidgetStatus> {
  const settings = previewJson(previewSettingsKey);
  return {
    enabled: settings.widgetEnabled === true,
    backgroundOnly: settings.widgetBackgroundOnly === true,
    dismissed: previewWidgetDismissed(),
  };
}
export async function dismissWidget(dismissed: boolean): Promise<WidgetStatus> {
  try {
    if (dismissed) sessionStorage.setItem(widgetDismissedKey, "1");
    else sessionStorage.removeItem(widgetDismissedKey);
  } catch {}
  const status = await readWidgetStatus();
  window.dispatchEvent(new CustomEvent(widgetEvent, { detail: status }));
  return status;
}
export async function exportReport(
  format: ExportFormat,
  days: string[],
  name: string,
): Promise<ExportResult | null> {
  if (format === "xlsx")
    throw new Error("Eksport XLSX jest dostępny tylko w aplikacji Windows.");
  return { text: exportCsv(exportRows(await read(), days, Date.now())) };
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
export async function weekNote(week: string): Promise<string> {
  checkWeek(week);
  return previewWeekNotes()[week] ?? "";
}
export async function saveWeekNote(week: string, text: string): Promise<string> {
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
/**
 * Podgląd: ostatnia wiadomość rozszerzenia (`lastMessage`, `lastKey`,
 * `lastKeyAt`) w localStorage; testy ustawiają ją, żeby pokazać „połączone”.
 */
export const browserKey = "todo-track-preview-browser-v1";
export async function browserStatus(): Promise<BrowserStatus> {
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
export async function browserOpenFolder(): Promise<void> {
  throw new Error(
    "Folder rozszerzenia otwiera się w aplikacji Windows. W repozytorium to katalog browser-extension.",
  );
}
