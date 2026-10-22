/**
 * Symulowany asystent Rano i przeglądu tygodnia w podglądzie (A6, `plan_assist.rs`):
 * dane z podglądu (plan dnia, spotkania, pojemność, terminy, pomiary tygodnia,
 * notatki, czas agentów), odpowiedź ze skryptu i to samo mapowanie co w Ruście.
 *
 * Skrypt w stanie symulatora AI (`todo-track-preview-ai-v1`, `script`):
 * - `morning`: `{ value: { comment, moves: [{ taskId, toDay, reason }] }, error?, delayMs? }`,
 * - `week_notes`: `{ value: { notes }, error?, delayMs? }`.
 * Bez wpisu odpowiedź jest pusta (`{ ok: true }` → brak komentarza / szkicu).
 */
import { previewJson, previewSettingsKey } from "./storage";
import { aiAsk, aiRedactPreview } from "./ai";
import { agentTime } from "./agents";
import { calendarEvents } from "./calendar";
import { read } from "./snapshot";
import { isRecurring } from "../recurrence";
import { dayTarget } from "../workday";
import { dayBounds, dayKey } from "../time";
import { meetingOccupies, type CalendarEvent } from "../api/calendar";
import { aiMessages } from "../api/ai";
import {
  COMMENT_MAX,
  DEADLINE_WINDOW,
  DEADLINES_MAX,
  DRAFT_MAX,
  HORIZON_DAYS,
  MOVES_MAX,
  NOTE_MAX,
  PLAN_MAX,
  planAssistMessages,
  REASON_MAX,
  SUBJECT_MAX,
  WEEK_TASKS_MAX,
  type EstimateCorrection,
  type MorningComment,
  type MorningMove,
  type WeekDraft,
} from "../api/aiplan";

const MORNING_KIND = "morning";
const WEEK_KIND = "week_notes";
const settings = () => previewJson(previewSettingsKey);

interface PlanTask {
  id: number;
  title: string;
  project: string;
  key: string;
  estimateMinutes: number;
  dueDay: string;
  plannedDay: string;
  priority: number;
  done: boolean;
}
interface UpcomingDay {
  day: string;
  goalMs: number;
  meetingsMs: number;
  capacityMs: number;
  plannedMinutes: number;
}
interface MorningInput {
  day: string;
  goalMs: number;
  meetingsMs: number;
  capacityMs: number;
  off: boolean;
  trackedMs: number;
  meetings: { from: number; to: number; title?: string }[];
  plan: PlanTask[];
  deadlines: PlanTask[];
  upcoming: UpcomingDay[];
  /** Szacunki po poprawce (`estimatesCorrected`). */
  corrected: boolean;
}

/** Szacunek po poprawce (`EstimateCorrection::minutes`); 0 bez szacunku. */
function correctedEstimate(
  minutesValue: number,
  project: string,
  correction: EstimateCorrection | null,
): number {
  if (!correction || minutesValue <= 0) return Math.max(0, minutesValue);
  const raw = correction.projects[project] ?? correction.all;
  const factor = Number.isFinite(raw) ? Math.min(3, Math.max(0.5, raw)) : 1;
  return Math.round(minutesValue * factor);
}

const clip = (text: string, max: number) => [...text.trim()].slice(0, max).join("");
const minutes = (ms: number) => Math.floor((Math.max(0, ms) + 30_000) / 60_000);
/** Wartość bez pustych napisów (jak `put` w Ruście). */
const given = (value: string) => (value.trim() ? value.trim() : undefined);

/** Dzień przesunięty o `amount` dni (odporne na zmianę czasu). */
function addDays(day: string, amount: number): string {
  const d = new Date(dayBounds(day)[0]);
  return dayKey(new Date(d.getFullYear(), d.getMonth(), d.getDate() + amount).getTime());
}

/** Ustawienia czasu pracy z domyślnymi (jak `AppSettings::default`). */
function workSettings() {
  const s = settings();
  return {
    targetDayMinutes: typeof s.targetDayMinutes === "number" ? s.targetDayMinutes : 480,
    workDays: Array.isArray(s.workDays) ? (s.workDays as number[]) : [1, 2, 3, 4, 5],
    holidaysInTarget: s.holidaysInTarget !== false,
  };
}

/** Złączone przedziały spotkań zajmujących czas, przycięte do doby (`capacity::meeting_spans`). */
function meetingMs(events: CalendarEvent[], day: string): number {
  const [from, to] = dayBounds(day);
  const spans = events
    .filter((e) => !e.allDay && meetingOccupies(e))
    .map((e): [number, number] => [Math.max(from, e.start), Math.min(to, e.end)])
    .filter(([a, b]) => b > a)
    .sort((a, b) => a[0] - b[0]);
  let sum = 0;
  let last: [number, number] | null = null;
  for (const span of spans) {
    if (last && span[0] <= last[1]) last[1] = Math.max(last[1], span[1]);
    else {
      if (last) sum += last[1] - last[0];
      last = [...span];
    }
  }
  return sum + (last ? last[1] - last[0] : 0);
}
/** Całodniowe „poza biurem” (`capacity::off_day`). */
const offDay = (events: CalendarEvent[], day: string) => {
  const [from, to] = dayBounds(day);
  return events.some(
    (e) =>
      e.allDay && e.showAs === "oof" && meetingOccupies(e) && e.start < to && e.end > from,
  );
};

const sessionMs = (
  sessions: { taskId: number; startedAt: number; endedAt: number | null }[],
  from: number,
  to: number,
  now: number,
) =>
  sessions.reduce(
    (sum, s) => sum + Math.max(0, Math.min(s.endedAt ?? now, to) - Math.max(s.startedAt, from)),
    0,
  );

/** Dane dnia z podglądu (odpowiednik `plan_assist::collect_morning`). */
async function collectMorning(
  day: string,
  requested: EstimateCorrection | null,
): Promise<MorningInput> {
  const now = Date.now();
  const data = await read();
  // Poprawka tylko przy „Planuj z poprawką szacunków”, jak `morning_request`.
  const correction = settings().planWithCorrection === true ? requested : null;
  const estimate = (t: (typeof data.tasks)[number]) =>
    correctedEstimate(t.details?.estimateMinutes ?? 0, t.details?.project ?? "", correction);
  const work = workSettings();
  const [from, to] = dayBounds(day);
  const events = await calendarEvents(from, to);
  const goal = dayTarget(work, day);
  const off = offDay(events, day);
  const meetings = meetingMs(events, day);
  const sendTitles = settings().aiSendMeetingTitles === true;
  const plannable = (t: (typeof data.tasks)[number]) =>
    !t.archived && !isRecurring(t.details);
  const ref = (t: (typeof data.tasks)[number]): PlanTask => ({
    id: t.id,
    title: clip(t.title, SUBJECT_MAX),
    project: clip(t.details?.project ?? "", SUBJECT_MAX),
    key: t.jiraKey ?? (t.redmineId ? `#${t.redmineId}` : ""),
    estimateMinutes: estimate(t),
    dueDay: t.details?.dueDay ?? "",
    plannedDay: t.details?.plannedDay ?? "",
    priority: t.details?.priority ?? 0,
    done: t.done,
  });
  const plan = data.tasks
    .filter((t) => plannable(t) && t.details?.plannedDay === day)
    .sort(
      (a, b) =>
        Number(a.done) - Number(b.done) ||
        (b.details?.priority ?? 0) - (a.details?.priority ?? 0) ||
        (a.details?.order ?? 0) - (b.details?.order ?? 0) ||
        b.id - a.id,
    )
    .slice(0, PLAN_MAX)
    .map(ref);
  const last = addDays(day, DEADLINE_WINDOW);
  const deadlines = data.tasks
    .filter(
      (t) =>
        plannable(t) &&
        !t.done &&
        !!t.details?.dueDay &&
        t.details.dueDay <= last &&
        t.details.plannedDay !== day,
    )
    .sort((a, b) => a.details!.dueDay.localeCompare(b.details!.dueDay) || a.id - b.id)
    .slice(0, DEADLINES_MAX)
    .map(ref);
  const upcoming: UpcomingDay[] = [];
  for (let offset = 1; offset <= 21 && upcoming.length < HORIZON_DAYS; offset++) {
    const next = addDays(day, offset);
    const target = dayTarget(work, next);
    if (target <= 0) continue;
    const [a, b] = dayBounds(next);
    const nextEvents = await calendarEvents(a, b);
    if (offDay(nextEvents, next)) continue;
    const meetingsMs = meetingMs(nextEvents, next);
    upcoming.push({
      day: next,
      goalMs: target,
      meetingsMs,
      capacityMs: Math.max(0, target - meetingsMs),
      plannedMinutes: data.tasks
        .filter((t) => plannable(t) && !t.done && t.details?.plannedDay === next)
        .reduce((sum, t) => sum + estimate(t), 0),
    });
  }
  return {
    day,
    goalMs: goal,
    meetingsMs: meetings,
    capacityMs: off ? 0 : Math.max(0, goal - meetings),
    off,
    trackedMs: sessionMs(data.sessions, from, to, now),
    meetings: events
      .filter((e) => !e.allDay && meetingOccupies(e))
      .map((e) => ({
        from: Math.max(e.start, from),
        to: Math.min(e.end, to),
        ...(sendTitles && e.subject.trim() ? { title: clip(e.subject, SUBJECT_MAX) } : {}),
      }))
      .filter((e) => e.to > e.from),
    plan,
    deadlines,
    upcoming,
    corrected: correction !== null,
  };
}

const hhmm = (ms: number) => {
  const d = new Date(ms);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};

const taskJson = (t: PlanTask, withPlanned: boolean) => ({
  taskId: t.id,
  title: given(t.title),
  project: given(t.project),
  key: given(t.key),
  ...(t.estimateMinutes > 0 ? { estimateMinutes: t.estimateMinutes } : {}),
  dueDay: given(t.dueDay),
  ...(withPlanned ? { plannedDay: given(t.plannedDay) } : {}),
  ...(t.priority !== 0 ? { priority: t.priority } : {}),
  ...(t.done ? { done: true } : {}),
});

/** Dane dla modelu (`plan_assist::morning_json`). */
function morningJson(i: MorningInput) {
  return {
    day: i.day,
    goalMinutes: minutes(i.goalMs),
    meetingsMinutes: minutes(i.meetingsMs),
    capacityMinutes: minutes(i.capacityMs),
    ...(i.off ? { off: true } : {}),
    trackedMinutes: minutes(i.trackedMs),
    ...(i.corrected ? { estimatesCorrected: true } : {}),
    meetings: i.meetings.map((e) => ({
      from: hhmm(e.from),
      to: hhmm(e.to),
      ...(e.title ? { title: e.title } : {}),
    })),
    plan: i.plan.map((t) => taskJson(t, false)),
    deadlines: i.deadlines.map((t) => taskJson(t, true)),
    nextDays: i.upcoming.map((d) => ({
      day: d.day,
      goalMinutes: minutes(d.goalMs),
      meetingsMinutes: minutes(d.meetingsMs),
      capacityMinutes: minutes(d.capacityMs),
      plannedMinutes: d.plannedMinutes,
    })),
  };
}
const morningPrompt = (i: MorningInput) =>
  `Komentarz do planu dnia.\nDane:\n${JSON.stringify(morningJson(i))}`;

/**
 * Odpowiedź modelu jako komentarz i przesunięcia zgodne z danymi
 * (`plan_assist::parse_morning`): zadanie otwarte z planu, dzień z `nextDays`
 * nie późniejszy niż termin, jedno przesunięcie na zadanie.
 */
function parseMorning(value: unknown, input: MorningInput) {
  const v = (value && typeof value === "object" ? value : {}) as {
    comment?: unknown;
    moves?: unknown;
  };
  const comment = typeof v.comment === "string" ? clip(v.comment, COMMENT_MAX) : "";
  const moves: MorningMove[] = [];
  (Array.isArray(v.moves) ? v.moves : []).forEach((raw, index) => {
    if (moves.length >= MOVES_MAX || !raw || typeof raw !== "object") return;
    const item = raw as Record<string, unknown>;
    const taskId = typeof item.taskId === "number" && Number.isInteger(item.taskId) ? item.taskId : null;
    const task = input.plan.find((t) => t.id === taskId && !t.done);
    if (taskId === null || !task) return;
    const toDay = typeof item.toDay === "string" ? item.toDay.trim() : "";
    if (!input.upcoming.some((d) => d.day === toDay)) return;
    if (task.dueDay && toDay > task.dueDay) return;
    if (moves.some((m) => m.taskId === taskId)) return;
    moves.push({
      id: index,
      taskId,
      taskTitle: task.title,
      fromDay: input.day,
      toDay,
      reason: clip(typeof item.reason === "string" ? item.reason : "", REASON_MAX),
    });
  });
  return { comment, moves };
}

/** Sprawdza przełącznik i silnik jak `plan_assist::gate`. */
function gate() {
  const s = settings();
  if (s.aiPlanAssist === false) throw new Error(planAssistMessages.off);
  if (s.aiEngine !== "claude" && s.aiEngine !== "codex") throw new Error(aiMessages.off);
}

/** `ai_morning_preview` w podglądzie: treść zapytania po uproszczonym filtrze nazw. */
export async function aiMorningPreview(
  day: string,
  correction: EstimateCorrection | null = null,
): Promise<string> {
  gate();
  return aiRedactPreview(morningPrompt(await collectMorning(day, correction)));
}

/** `ai_morning_comment` w podglądzie: bez planu i terminów model nie jest wołany. */
export async function aiMorningComment(
  day: string,
  correction: EstimateCorrection | null = null,
): Promise<MorningComment> {
  gate();
  const input = await collectMorning(day, correction);
  if (!input.plan.length && !input.deadlines.length)
    return { day, comment: "", moves: [], message: planAssistMessages.emptyMorning };
  const answer = await aiAsk(MORNING_KIND, morningPrompt(input));
  const { comment, moves } = parseMorning(answer, input);
  return {
    day,
    comment,
    moves,
    message: comment || moves.length ? "" : planAssistMessages.nothingMorning,
  };
}

// ---------------------------------------------------------------------------
// Przegląd tygodnia

interface WeekInput {
  week: string;
  days: { day: string; trackedMs: number; goalMs: number; meetingsMs: number; off: boolean }[];
  tasks: {
    id: number;
    title: string;
    project: string;
    key: string;
    trackedMs: number;
    estimateMinutes: number;
    done: boolean;
    notes: { day: string; text: string }[];
  }[];
  planned: number;
  plannedDone: number;
  plannedOpen: number;
  agentMs: number;
  agentUnmatchedMs: number;
}

/** Dane tygodnia z podglądu (odpowiednik `plan_assist::collect_week`). */
async function collectWeek(week: string): Promise<WeekInput> {
  const now = Date.now();
  const data = await read();
  const work = workSettings();
  const keys = Array.from({ length: 7 }, (_, i) => addDays(week, i));
  const days: WeekInput["days"] = [];
  for (const day of keys) {
    const [a, b] = dayBounds(day);
    const events = await calendarEvents(a, b);
    days.push({
      day,
      trackedMs: sessionMs(data.sessions, a, b, now),
      goalMs: dayTarget(work, day),
      meetingsMs: meetingMs(events, day),
      off: offDay(events, day),
    });
  }
  const from = dayBounds(keys[0])[0];
  const to = dayBounds(keys[6])[1];
  const tasks: WeekInput["tasks"] = [];
  for (const t of data.tasks) {
    const tracked = sessionMs(
      data.sessions.filter((s) => s.taskId === t.id),
      from,
      to,
      now,
    );
    const notes = keys
      .map((day) => ({ day, text: clip(t.details?.notes?.[day] ?? "", NOTE_MAX) }))
      .filter((n) => n.text);
    if (!tracked && !notes.length) continue;
    tasks.push({
      id: t.id,
      title: clip(t.title, SUBJECT_MAX),
      project: clip(t.details?.project ?? "", SUBJECT_MAX),
      key: t.jiraKey ?? (t.redmineId ? `#${t.redmineId}` : ""),
      trackedMs: tracked,
      estimateMinutes: t.details?.estimateMinutes ?? 0,
      done: t.done,
      notes,
    });
  }
  tasks.sort((a, b) => b.trackedMs - a.trackedMs || a.id - b.id);
  tasks.length = Math.min(tasks.length, WEEK_TASKS_MAX);
  const planned = data.tasks.filter(
    (t) =>
      !isRecurring(t.details) &&
      (t.done || !t.archived) &&
      (t.details?.plannedDay ?? "") >= keys[0] &&
      (t.details?.plannedDay ?? "") <= keys[6],
  );
  const agents = await agentTime(from, to);
  return {
    week,
    days,
    tasks,
    planned: planned.length,
    plannedDone: planned.filter((t) => t.done).length,
    plannedOpen: planned.filter((t) => !t.done).length,
    agentMs: agents.totals.reduce((s, t) => s + t.agentMs, 0) + agents.unmatchedMs,
    agentUnmatchedMs: agents.unmatchedMs,
  };
}

/** Dane tygodnia dla modelu (`plan_assist::week_json`). */
function weekJson(i: WeekInput) {
  return {
    week: i.week,
    days: i.days.map((d) => ({
      day: d.day,
      trackedMinutes: minutes(d.trackedMs),
      goalMinutes: minutes(d.goalMs),
      meetingsMinutes: minutes(d.meetingsMs),
      ...(d.off ? { off: true } : {}),
    })),
    plan: { planned: i.planned, done: i.plannedDone, open: i.plannedOpen },
    tasks: i.tasks.map((t) => ({
      taskId: t.id,
      title: given(t.title),
      project: given(t.project),
      key: given(t.key),
      trackedMinutes: minutes(t.trackedMs),
      ...(t.estimateMinutes > 0 ? { estimateMinutes: t.estimateMinutes } : {}),
      ...(t.done ? { done: true } : {}),
      ...(t.notes.length ? { notes: t.notes.map((n) => ({ day: n.day, note: n.text })) } : {}),
    })),
    ...(i.agentMs > 0
      ? {
          agentTime: {
            totalMinutes: minutes(i.agentMs),
            unassignedMinutes: minutes(i.agentUnmatchedMs),
          },
        }
      : {}),
  };
}
const weekPrompt = (i: WeekInput) =>
  `Szkic wniosków z tygodnia.\nDane:\n${JSON.stringify(weekJson(i))}`;

const weekEmpty = (i: WeekInput) =>
  i.days.every((d) => d.trackedMs === 0) && !i.tasks.length && !i.planned && !i.agentMs;

/** `ai_week_preview` w podglądzie: treść zapytania po uproszczonym filtrze nazw. */
export async function aiWeekPreview(week: string): Promise<string> {
  gate();
  return aiRedactPreview(weekPrompt(await collectWeek(week)));
}

/** `ai_week_draft` w podglądzie: tydzień bez danych nie woła modelu. */
export async function aiWeekDraft(week: string): Promise<WeekDraft> {
  gate();
  const input = await collectWeek(week);
  if (weekEmpty(input)) return { week, text: "", message: planAssistMessages.emptyWeek };
  const answer = (await aiAsk(WEEK_KIND, weekPrompt(input))) as { notes?: unknown } | null;
  const text = typeof answer?.notes === "string" ? clip(answer.notes, DRAFT_MAX) : "";
  return { week, text, message: text ? "" : planAssistMessages.nothingWeek };
}
