import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";
import { readSettings } from "./settings";
import { change, desktop, read, type Snapshot } from "./store";

/** Blok skupienia (`focus::Block` w Rust). */
export interface FocusBlock {
  id: number;
  taskId: number;
  title: string;
  start: number;
  /** Planowany koniec bloku (ms). */
  end: number;
}
/** Przerwa po bloku (`focus::Pause`); `stops`: przerwa zatrzymała pomiar. */
export interface FocusPause {
  taskId: number;
  title: string;
  start: number;
  until: number;
  stops: boolean;
}
/** „Wracasz do X?” po przerwie (`focus::Return`). */
export interface FocusReturn {
  taskId: number;
  title: string;
}
/** Stan trybu skupienia (`focus::Status`). */
export interface FocusStatus {
  block: FocusBlock | null;
  /** Ukończony blok czekający na „Przerwa” albo „Kolejny blok”. */
  ended: FocusBlock | null;
  pause: FocusPause | null;
  resume: FocusReturn | null;
  /** Powiadomienia czekające na koniec bloku (tylko aplikacja). */
  queued: number;
  /** Ukończone bloki dziś i w tym tygodniu. */
  today: number;
  week: number;
}
export type FocusAction =
  | "start"
  | "stop"
  | "break"
  | "next"
  | "end-break"
  | "resume"
  | "dismiss";

export const emptyFocus = (): FocusStatus => ({
  block: null,
  ended: null,
  pause: null,
  resume: null,
  queued: 0,
  today: 0,
  week: 0,
});

/** Pozostały czas „mm:ss” (od godziny „g:mm:ss”), zaokrąglony w górę do sekundy. */
export function countdown(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor(total / 60) % 60;
  const s = String(total % 60).padStart(2, "0");
  return h
    ? `${h}:${String(m).padStart(2, "0")}:${s}`
    : `${String(m).padStart(2, "0")}:${s}`;
}

// --- Podgląd (localStorage), odpowiednik `focus::Focus` w Rust ---

const key = "todo-track-preview-focus-v1";
/** Zdarzenie okna w podglądzie: stan skupienia się zmienił. */
const previewEvent = "todo-track-preview-focus";
const MIN = 60_000;

interface PreviewRow {
  id: number;
  taskId: number;
  start: number;
  end: number;
  /** null = blok trwa. */
  completed: boolean | null;
}
interface PreviewState {
  blocks: PreviewRow[];
  /** Id ukończonego bloku czekającego na decyzję. */
  ended: number | null;
  pause: Omit<FocusPause, "title"> | null;
  /** Zadanie z pytania „Wracasz do X?”. */
  resume: number | null;
}

function load(): PreviewState {
  try {
    const value = JSON.parse(localStorage.getItem(key) ?? "null");
    if (value && Array.isArray(value.blocks)) return value;
  } catch {}
  return { blocks: [], ended: null, pause: null, resume: null };
}
function save(state: PreviewState) {
  localStorage.setItem(key, JSON.stringify(state));
  window.dispatchEvent(new Event(previewEvent));
  // Propozycje agentów czekają w czasie bloku (`agents.ts`).
  window.dispatchEvent(new Event("agent-suggestions"));
}

/** Czy w podglądzie trwa blok (propozycje agentów czekają). */
export function previewFocusHolds(): boolean {
  return !desktop && load().blocks.some((b) => b.completed === null);
}

const activeOf = (data: Snapshot) =>
  data.sessions.find((s) => s.endedAt === null);

/** Jedno sprawdzenie jak `Focus::tick` (bez powiadomień). */
function tick(state: PreviewState, data: Snapshot, now: number): boolean {
  let changed = false;
  const current = activeOf(data);
  const block = state.blocks.find((b) => b.completed === null);
  if (block) {
    let stoppedAt: number | null = null;
    if (current && current.taskId !== block.taskId) stoppedAt = current.startedAt;
    else if (!current) {
      // Stop: chwila zatrzymania pomiaru zadania.
      const ends = data.sessions
        .filter(
          (s) =>
            s.taskId === block.taskId &&
            s.endedAt !== null &&
            s.endedAt >= block.start,
        )
        .map((s) => s.endedAt as number);
      stoppedAt = ends.length ? Math.max(...ends) : now;
    }
    const completed =
      stoppedAt === null
        ? now >= block.end
          ? true
          : null
        : Math.min(Math.max(stoppedAt, block.start), now) >= block.end;
    if (completed !== null) {
      if (!completed)
        block.end = Math.min(
          Math.max(Math.min(stoppedAt ?? now, now), block.start),
          block.end,
        );
      block.completed = completed;
      if (completed) state.ended = block.id;
      changed = true;
    }
  }
  const ended = state.blocks.find((b) => b.id === state.ended);
  if (ended && current && current.taskId !== ended.taskId) {
    state.ended = null;
    changed = true;
  }
  const pause = state.pause;
  if (pause) {
    if (pause.stops && current) {
      state.pause = null;
      changed = true;
    } else if (now >= pause.until) {
      state.pause = null;
      changed = true;
      if (current?.taskId !== pause.taskId) state.resume = pause.taskId;
    }
  }
  if (state.resume !== null && current) {
    state.resume = null;
    changed = true;
  }
  return changed;
}

function previewStatus(state: PreviewState, data: Snapshot, now: number) {
  const title = (id: number) =>
    [...data.tasks, ...(data.trash ?? [])].find((t) => t.id === id)?.title ??
    "";
  const asBlock = (b: PreviewRow): FocusBlock => ({
    id: b.id,
    taskId: b.taskId,
    title: title(b.taskId),
    start: b.start,
    end: b.end,
  });
  const day = new Date(now);
  day.setHours(0, 0, 0, 0);
  const monday = new Date(day);
  monday.setDate(monday.getDate() - ((monday.getDay() + 6) % 7));
  const running = state.blocks.find((b) => b.completed === null);
  const ended = state.blocks.find((b) => b.id === state.ended);
  const done = state.blocks.filter((b) => b.completed === true);
  return {
    block: running ? asBlock(running) : null,
    ended: ended ? asBlock(ended) : null,
    pause: state.pause
      ? { ...state.pause, title: title(state.pause.taskId) }
      : null,
    resume:
      state.resume !== null
        ? { taskId: state.resume, title: title(state.resume) }
        : null,
    queued: 0,
    today: done.filter((b) => b.start >= +day).length,
    week: done.filter((b) => b.start >= +monday).length,
  };
}

async function previewAction(
  action: FocusAction,
  taskId: number | null,
): Promise<void> {
  const settings = await readSettings();
  const state = load();
  let data = await read();
  tick(state, data, Date.now());
  const now = Date.now();
  const current = activeOf(data);
  const running = state.blocks.find((b) => b.completed === null);
  const ended = state.blocks.find((b) => b.id === state.ended);
  const close = (b: PreviewRow) => {
    b.end = Math.min(Math.max(now, b.start), b.end);
    b.completed = false;
  };
  const start = async (task: number | null) => {
    const id = task ?? current?.taskId ?? null;
    if (id === null)
      throw new Error(
        "Uruchom pomiar albo wybierz zadanie, żeby zacząć blok skupienia.",
      );
    if (running) {
      if (running.taskId === id && current?.taskId === id) return;
      close(running);
    }
    if (current?.taskId !== id) data = await change("start", id);
    state.blocks.push({
      id: Math.max(0, ...state.blocks.map((b) => b.id)) + 1,
      taskId: id,
      start: now,
      end: now + Math.max(1, settings.focusBlockMinutes) * MIN,
      completed: null,
    });
    state.ended = null;
    state.pause = null;
    state.resume = null;
  };
  switch (action) {
    case "start":
      await start(taskId);
      break;
    case "stop":
      if (running) close(running);
      break;
    case "break":
      if (ended) {
        if (settings.focusBreakStops) data = await change("stop");
        state.pause = {
          taskId: ended.taskId,
          start: now,
          until: now + Math.max(1, settings.focusBreakMinutes) * MIN,
          stops: settings.focusBreakStops,
        };
        state.ended = null;
        state.resume = null;
      }
      break;
    case "next":
      await start(
        taskId ?? ended?.taskId ?? state.resume ?? state.pause?.taskId ?? null,
      );
      break;
    case "end-break":
      if (state.pause) {
        if (activeOf(data)?.taskId !== state.pause.taskId)
          state.resume = state.pause.taskId;
        state.pause = null;
      }
      break;
    case "resume": {
      const id = taskId ?? state.resume;
      if (id === null) throw new Error("Nie ma zadania do wznowienia.");
      await change("start", id);
      state.resume = null;
      state.pause = null;
      break;
    }
    case "dismiss":
      state.ended = null;
      state.resume = null;
      break;
    default:
      throw new Error("Nieznana akcja trybu skupienia.");
  }
  save(state);
}

/** Stan trybu skupienia (`focus_status`); sprawdza koniec bloku i przerwy. */
export async function readFocus(): Promise<FocusStatus> {
  if (desktop) return invoke("focus_status");
  const state = load();
  const data = await read();
  const now = Date.now();
  if (tick(state, data, now)) save(state);
  return previewStatus(state, data, now);
}

/**
 * Akcja trybu skupienia (`focus_action`): `start` (zadanie `taskId` albo
 * mierzone), `stop` (przerwij blok), `break`, `next`, `end-break`, `resume`,
 * `dismiss`. Pomiar zmienia się tylko po takim kliknięciu.
 */
export async function focusAction(
  action: FocusAction,
  taskId: number | null = null,
): Promise<FocusStatus> {
  if (desktop) return invoke("focus_action", { action, taskId });
  await previewAction(action, taskId);
  return readFocus();
}

/** Nasłuch zmian stanu (`focus-changed`; w podglądzie zdarzenie okna). */
export async function onFocusChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen("focus-changed", handler);
  const listener = (e: StorageEvent) => {
    if (e.key === key) handler();
  };
  window.addEventListener(previewEvent, handler);
  window.addEventListener("storage", listener);
  return () => {
    window.removeEventListener(previewEvent, handler);
    window.removeEventListener("storage", listener);
  };
}

/**
 * Wspólny stan skupienia okna. Odświeża się po zdarzeniu, przy upływie końca
 * bloku albo przerwy (zegar ścienny) i po `refreshFocus()` (np. zmiana danych).
 */
export const focus = ref<FocusStatus>(emptyFocus());
let loading: Promise<void> | null = null;
let again = false;
export async function refreshFocus(): Promise<void> {
  if (loading) {
    again = true;
    return loading;
  }
  loading = (async () => {
    do {
      again = false;
      try {
        focus.value = await readFocus();
      } catch {
        // Bez stanu skupienia reszta okna działa; spróbujemy przy kolejnej zmianie.
      }
    } while (again);
  })();
  try {
    await loading;
  } finally {
    loading = null;
  }
}

let users = 0;
let stop: (() => void) | undefined;
let timer: ReturnType<typeof setInterval> | undefined;
/** Termin, po którym trzeba zapytać o stan (koniec bloku albo przerwy). */
function due(now: number) {
  const s = focus.value;
  return (
    (!!s.block && now >= s.block.end) || (!!s.pause && now >= s.pause.until)
  );
}

/** Stan skupienia w komponencie; pierwszy użytkownik włącza nasłuch. */
export function useFocus() {
  onMounted(async () => {
    if (users++ === 0) {
      timer = setInterval(() => {
        if (due(Date.now())) void refreshFocus();
      }, 1000);
      try {
        stop = await onFocusChanged(() => void refreshFocus());
      } catch {}
      await refreshFocus();
    }
  });
  onUnmounted(() => {
    if (--users === 0) {
      clearInterval(timer);
      stop?.();
      stop = undefined;
    }
  });
  return focus;
}
