/**
 * Symulowany asystent Zamknięcia dnia w podglądzie (A5, `day_assist.rs`): dane dnia
 * z podglądu (pomiary, luki, czas agentów, spotkania, notatki), commity i zmiany
 * statusów ze stanu symulatora AI (`dayData`), odpowiedź ze skryptu `day_close`
 * (`{ value: { proposals: [...] }, error?, delayMs? }`) i to samo mapowanie
 * propozycji co w Ruście (`parse`).
 *
 * `dayData` w stanie symulatora (`tuktuk-track-preview-ai-v1`):
 * `{ commits: [{ repo, at, subject, taskId? }], statusChanges: [{ system, key,
 * from, to, at, taskId? }] }` (czas w ms). Nie ma tu `git`, więc commity podaje test.
 */
import { previewJson, previewSettingsKey } from "./storage";
import { aiAsk, aiKey, aiRedactPreview } from "./ai";
import { agentTime } from "./agents";
import { calendarEvents } from "./calendar";
import { read } from "./snapshot";
import { clock, dayBounds } from "../time";
import { aiMessages } from "../api/ai";
import {
  dayAssistMessages,
  GAP_MIN_MS,
  NOTE_MAX,
  PROPOSALS_MAX,
  REASON_MAX,
  SUBJECT_MAX,
  TEXT_MAX,
  type DayAnalysis,
  type DayProposal,
} from "../api/aiday";
import { OPEN_MAX } from "../api/aiassign";

const KIND = "day_close";
const settings = () => previewJson(previewSettingsKey);

interface Span {
  sessionId: number;
  taskId: number;
  from: number;
  to: number;
}
interface Gap {
  from: number;
  to: number;
  before: Span;
  after: Span;
}
interface AgentSpan {
  agent: string;
  folder: string;
  taskId: number | null;
  from: number;
  to: number;
}
interface Commit {
  repo: string;
  at: number;
  subject: string;
  taskId?: number | null;
}
interface StatusChange {
  system: string;
  key: string;
  from: string;
  to: string;
  at: number;
  taskId?: number | null;
}
interface TaskRef {
  id: number;
  title: string;
  project: string;
  key: string;
}
interface DayInput {
  day: string;
  tasks: TaskRef[];
  measurements: Span[];
  gaps: Gap[];
  agents: AgentSpan[];
  meetings: { from: number; to: number; title?: string }[];
  commits: Commit[];
  statusChanges: StatusChange[];
  notes: { taskId: number; text: string }[];
  startedAt: number | null;
  endedAt: number | null;
}

const clip = (text: string, max: number) => [...text.trim()].slice(0, max).join("");
const folderName = (path: string) =>
  path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? "";
const normFolder = (path: string) => path.trim().replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();

/** Luki między pomiarami: od `GAP_MIN_MS`, `before` kończy się najpóźniej (`day_assist::gaps`). */
function gapsOf(spans: Span[]): Gap[] {
  const sorted = [...spans].sort((a, b) => a.from - b.from || a.sessionId - b.sessionId);
  const out: Gap[] = [];
  let last: Span | undefined;
  for (const span of sorted) {
    if (last && span.from - last.to >= GAP_MIN_MS)
      out.push({ from: last.to, to: span.from, before: last, after: span });
    if (!last || span.to >= last.to) last = span;
  }
  return out;
}

/** Dane dnia z podglądu (odpowiednik `day_assist::collect`). */
async function collect(day: string): Promise<DayInput> {
  const now = Date.now();
  const [from, to] = dayBounds(day);
  const data = await read();
  const measurements: Span[] = [];
  for (const s of [...data.sessions].sort((a, b) => a.startedAt - b.startedAt || a.id - b.id)) {
    const span = {
      sessionId: s.id,
      taskId: s.taskId,
      from: Math.max(s.startedAt, from),
      to: Math.min(s.endedAt ?? now, to),
    };
    if (span.to > span.from) measurements.push(span);
  }
  const periods = (await agentTime(from, to)).periods
    .map((p) => ({
      agent: p.agent,
      folder: p.folder,
      taskId: p.taskId,
      from: Math.max(p.start, from),
      to: Math.min(p.end, to),
    }))
    .filter((p) => p.to > p.from);
  const sendTitles = settings().aiSendMeetingTitles === true;
  const meetings = (await calendarEvents(from, to))
    .filter(
      (e) =>
        !e.allDay &&
        !e.cancelled &&
        e.response !== "declined" &&
        ["busy", "oof", "workingElsewhere", "tentative"].includes(e.showAs),
    )
    .map((e) => ({
      from: Math.max(e.start, from),
      to: Math.min(e.end, to),
      ...(sendTitles && e.subject.trim() ? { title: clip(e.subject, SUBJECT_MAX) } : {}),
    }))
    .filter((e) => e.to > e.from);
  const notes = data.tasks
    .map((t) => ({ taskId: t.id, text: clip(t.details?.notes?.[day] ?? "", NOTE_MAX) }))
    .filter((n) => n.text);
  const extra = (previewJson(aiKey).dayData ?? {}) as {
    commits?: Commit[];
    statusChanges?: StatusChange[];
  };
  const commits = (Array.isArray(extra.commits) ? extra.commits : [])
    .filter((c) => c.at >= from && c.at < to && c.subject?.trim())
    .map((c) => ({ ...c, subject: clip(c.subject, SUBJECT_MAX) }))
    .sort((a, b) => a.at - b.at);
  const statusChanges = (Array.isArray(extra.statusChanges) ? extra.statusChanges : [])
    .filter((c) => c.at >= from && c.at < to)
    .sort((a, b) => a.at - b.at);
  const mark = data.dayMarks?.find((m) => m.day === day);
  // Lista zadań: najpierw te z danych dnia, potem najnowsze otwarte.
  const wanted: number[] = [];
  const want = (id: number | null | undefined) => {
    if (id != null && !wanted.includes(id)) wanted.push(id);
  };
  measurements.forEach((s) => want(s.taskId));
  periods.forEach((p) => want(p.taskId));
  notes.forEach((n) => want(n.taskId));
  statusChanges.forEach((c) => want(c.taskId));
  commits.forEach((c) => want(c.taskId));
  const ref = (t: (typeof data.tasks)[number]): TaskRef => ({
    id: t.id,
    title: t.title,
    project: t.details?.project ?? "",
    key: t.jiraKey ?? (t.redmineId ? `#${t.redmineId}` : ""),
  });
  const tasks = wanted
    .map((id) => data.tasks.find((t) => t.id === id))
    .filter((t): t is (typeof data.tasks)[number] => !!t)
    .map(ref);
  const limit = tasks.length + OPEN_MAX;
  for (const t of [...data.tasks].sort((a, b) => b.id - a.id))
    if (tasks.length < limit && !t.done && !t.archived && !tasks.some((x) => x.id === t.id))
      tasks.push(ref(t));
  const input: DayInput = {
    day,
    tasks,
    measurements,
    gaps: gapsOf(measurements),
    agents: periods,
    meetings,
    commits,
    statusChanges,
    notes,
    startedAt: mark?.startedAt ?? null,
    endedAt: mark?.endedAt ?? null,
  };
  return input;
}

const isEmpty = (i: DayInput) =>
  !i.measurements.length &&
  !i.agents.length &&
  !i.commits.length &&
  !i.statusChanges.length &&
  !i.notes.length;

const minutes = (ms: number) => Math.floor((Math.max(0, ms) + 30_000) / 60_000);
/** Wartość bez pustych napisów (jak `put` w Ruście). */
const given = (value: string) => (value.trim() ? value.trim() : undefined);

/** Dane dla modelu (`day_assist::input_json`). */
function inputJson(i: DayInput) {
  return {
    day: i.day,
    ...(i.startedAt != null ? { dayStartedAt: clock(i.startedAt) } : {}),
    ...(i.endedAt != null ? { dayEndedAt: clock(i.endedAt) } : {}),
    tasks: i.tasks.map((t) => ({
      id: t.id,
      title: given(t.title),
      project: given(t.project),
      key: given(t.key),
    })),
    measurements: i.measurements.map((s) => ({
      taskId: s.taskId,
      from: clock(s.from),
      to: clock(s.to),
    })),
    gaps: i.gaps.map((g) => ({
      from: clock(g.from),
      to: clock(g.to),
      minutes: minutes(g.to - g.from),
      beforeTaskId: g.before.taskId,
      afterTaskId: g.after.taskId,
    })),
    agentWork: i.agents.map((a) => ({
      agent: a.agent,
      folder: given(folderName(a.folder)),
      from: clock(a.from),
      to: clock(a.to),
      taskId: a.taskId,
    })),
    meetings: i.meetings.map((e) => ({
      from: clock(e.from),
      to: clock(e.to),
      ...(e.title ? { title: e.title } : {}),
    })),
    commits: i.commits.map((c) => ({
      time: clock(c.at),
      repo: given(c.repo),
      subject: given(c.subject),
      ...(c.taskId != null ? { taskId: c.taskId } : {}),
    })),
    statusChanges: i.statusChanges.map((c) => ({
      time: clock(c.at),
      system: given(c.system),
      key: given(c.key),
      from: given(c.from),
      to: given(c.to),
      ...(c.taskId != null ? { taskId: c.taskId } : {}),
    })),
    notes: i.notes.map((n) => ({ taskId: n.taskId, note: n.text })),
  };
}
const prompt = (i: DayInput) => `Dane:\n${JSON.stringify(inputJson(i))}`;

/** Chwila z godziny HH:MM w dniu; `null` dla śmieci. */
function timeMs(day: string, text: string): number | null {
  const m = /^(\d{1,2}):(\d{2})$/.exec(text.trim());
  if (!m || Number(m[1]) > 23 || Number(m[2]) > 59) return null;
  const [start] = dayBounds(day);
  const d = new Date(start);
  d.setHours(Number(m[1]), Number(m[2]), 0, 0);
  return d.getTime();
}
/** Godzina z odpowiedzi równa początkowi albo końcowi luki (do minuty) przyjmuje jej dokładną chwilę. */
function snap(day: string, text: string, exact: number[]): number | null {
  const ms = timeMs(day, text);
  if (ms === null) return null;
  return exact.find((e) => clock(e) === clock(ms)) ?? ms;
}

/** Granice luki do pełnych sekund do środka (`day_assist::ceil_second` / `floor_second`). */
const ceilSecond = (ms: number) => Math.ceil(ms / 1000) * 1000;
const floorSecond = (ms: number) => Math.floor(ms / 1000) * 1000;

/**
 * Odpowiedź modelu jako propozycje zgodne z danymi (`day_assist::parse`): opis TS
 * tylko dla zadań bez notatki dnia, granice luki w pełnych sekundach.
 */
function parse(value: unknown, input: DayInput): DayProposal[] {
  const items = (value as { proposals?: unknown } | null)?.proposals;
  if (!Array.isArray(items)) return [];
  const out: DayProposal[] = [];
  const seen = new Set<string>();
  items.forEach((raw, index) => {
    if (out.length >= PROPOSALS_MAX || !raw || typeof raw !== "object") return;
    const item = raw as Record<string, unknown>;
    const str = (key: string) => (typeof item[key] === "string" ? (item[key] as string) : "");
    const kind = str("kind");
    const taskId = typeof item.taskId === "number" && Number.isInteger(item.taskId) ? item.taskId : null;
    const task = input.tasks.find((t) => t.id === taskId);
    if (taskId === null || !task) return;
    const p: DayProposal = {
      id: index,
      kind: kind as DayProposal["kind"],
      from: "",
      to: "",
      fromMs: null,
      toMs: null,
      taskId,
      taskTitle: task.title,
      text: "",
      reason: clip(str("reason"), REASON_MAX),
      folder: null,
      fill: null,
    };
    if (kind === "gap") {
      const rawA = snap(input.day, str("from"), input.gaps.map((g) => ceilSecond(g.from)));
      const rawB = snap(input.day, str("to"), input.gaps.map((g) => floorSecond(g.to)));
      if (rawA === null || rawB === null) return;
      const a = ceilSecond(rawA);
      const b = floorSecond(rawB);
      const gap = input.gaps.find(
        (g) => a >= ceilSecond(g.from) && b <= floorSecond(g.to) && a < b,
      );
      if (!gap || seen.has(`gap:${a}:${b}`)) return;
      seen.add(`gap:${a}:${b}`);
      Object.assign(p, { from: clock(a), to: clock(b), fromMs: a, toMs: b });
      if (a === ceilSecond(gap.from) && b === floorSecond(gap.to)) {
        const mode =
          gap.before.taskId === taskId ? "previous" : gap.after.taskId === taskId ? "next" : null;
        if (mode)
          p.fill = { beforeId: gap.before.sessionId, afterId: gap.after.sessionId, mode };
      }
    } else if (kind === "unassigned") {
      const from = timeMs(input.day, str("from"));
      if (from === null) return;
      const period = input.agents.find(
        (x) =>
          x.taskId === null &&
          ((from >= x.from && from < x.to) || clock(x.from) === clock(from)),
      );
      if (!period) return;
      const key = `unassigned:${normFolder(period.folder)}`;
      if (seen.has(key)) return;
      seen.add(key);
      Object.assign(p, {
        from: clock(period.from),
        to: clock(period.to),
        fromMs: period.from,
        toMs: period.to,
        folder: period.folder,
      });
    } else if (kind === "note" || kind === "ts_description") {
      const text = clip(str("text"), TEXT_MAX);
      if (!text) return;
      if (kind === "ts_description" && input.notes.some((n) => n.taskId === taskId)) return;
      const key = kind === "note" ? `note:${taskId}:${text}` : `ts:${taskId}`;
      if (seen.has(key)) return;
      seen.add(key);
      p.text = text;
    } else return;
    out.push(p);
  });
  return out;
}

/** Sprawdza przełącznik i silnik jak `day_assist::request`. */
function gate() {
  const s = settings();
  if (s.aiDayClose === false) throw new Error(dayAssistMessages.off);
  if (s.aiEngine !== "claude" && s.aiEngine !== "codex") throw new Error(aiMessages.off);
}

/** `ai_day_preview` w podglądzie: treść zapytania po uproszczonym filtrze nazw. */
export async function aiDayPreview(day: string): Promise<string> {
  gate();
  return aiRedactPreview(prompt(await collect(day)));
}

/** `ai_day_analyze` w podglądzie: pusty dzień nie woła modelu; odpowiedź ze skryptu `day_close`. */
export async function aiDayAnalyze(day: string): Promise<DayAnalysis> {
  gate();
  const input = await collect(day);
  if (isEmpty(input)) return { day, proposals: [], message: dayAssistMessages.empty };
  const answer = await aiAsk(KIND, prompt(input));
  const proposals = parse(answer, input);
  return {
    day,
    proposals,
    message: proposals.length ? "" : dayAssistMessages.nothing,
  };
}
