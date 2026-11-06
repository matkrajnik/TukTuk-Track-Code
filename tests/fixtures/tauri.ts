import type { Page } from "@playwright/test";
import { defaultSettings } from "../../src/settings";
import {
  JIRA_SITE,
  NOW,
  REDMINE_SITE,
  seed,
  type RedmineFixture,
} from "./seed";

/**
 * Zastępcze IPC Tauri dla testów funkcji, które w trybie podglądu są wyłączone
 * (baner bezczynności, kopie, obserwacja Jiry). Frontend działa jak w aplikacji
 * (`isTauri()` = true), a komendy obsługuje ten skrypt na danych z fixture.
 *
 * Logika z Rust, od której zależą testy, jest tu przeniesiona jeden do jednego
 * i opisana odwołaniem do funkcji źródłowej:
 * - `idle::Clock::observe` i `idle_resolve` (`idleTick`, `idle_resolve`, z
 *   „Przytnij i wznów” jak `idle::trim_and_resume`, B3),
 * - `gap::recover` przy starcie (opcja `heartbeat`) i `gap::resolve`
 *   (`gap_notice`, `gap_resolve`),
 * - `jira::sync_assigned` z pierwszym sprawdzeniem tylko zapamiętującym (`jiraCheck`),
 * - `redmine::sync_assigned` tak samo, osobno od Jiry (`redmineCheck`, R3),
 * - odczyty Redmine i `redmine::start_issue` na danych `redmineFixture()`
 *   (`redmine_settings`, `redmine_issues` z `group_by_status`,
 *   `redmine_projects`, `redmine_statuses`, `redmine_start`, szybkie
 *   dodawanie `#1234`, R6),
 * - `planner::set_waiting` (`set_waiting`: blokada „Czekam na” z kafelka),
 * - `suggest::refresh` jako gotowa lista propozycji (`suggest`: zdarzenie
 *   `agent-suggestions`), `agent_switch` i `agent_dismiss`.
 * Same reguły są pokryte testami Rust; tutaj sprawdzamy, że interfejs reaguje na
 * zdarzenia backendu i pokazuje skutki.
 *
 * Stan żyje w oknie, więc przeładowanie strony zaczyna od danych początkowych.
 */
export interface TauriOptions {
  data?: unknown;
  settings?: Record<string, unknown>;
  now?: string | number;
  /** Wynik `inspect_backup` (null = anulowany wybór pliku) i dane po przywróceniu. */
  backup?: { preview: unknown; snapshot: unknown } | null;
  /**
   * Wynik `export_backup` (`backup::ExportResult`): ścieżka i liczby diffów
   * Workflows pominiętych w kopii. Domyślnie anulowany wybór pliku (null).
   */
  backupExport?: {
    path: string;
    skippedArtifacts: number;
    overLimitArtifacts: number;
  } | null;
  /** Szablony zadań (`task_templates`) na starcie. */
  templates?: unknown[];
  /** Propozycje przełączenia (`agent_suggestions`) na starcie. */
  suggestions?: unknown[];
  /**
   * Ostatni puls aplikacji (`app_heartbeat`, gap.rs): przy starcie trwający
   * pomiar po co najmniej 10 min przerwy kończy się na pulsie i pojawia się
   * baner „Pomiar zatrzymany po przerwie”. Domyślnie brak pulsu.
   */
  heartbeat?: number;
  /** Etykieta okna Tauri (`main`, `widget`); wybiera widok jak w `main.ts`. */
  window?: string;
  /**
   * Połączona Jira (widok Jira): tablice i zgłoszenia zwracane przez
   * `jira_boards` i `jira_issues`. Domyślnie Jira nie jest połączona.
   */
  jira?: {
    boards: { id: number; name: string }[];
    issues: {
      key: string;
      summary: string;
      status: string;
      boards: string[];
      /** Wartości pól z ustawień (`jira_fields::FieldValue`). */
      fields?: { id: string; name: string; value: string }[];
    }[];
    /** Lista pól do wyboru w Ustawieniach (`jira_fields`). */
    fields?: { id: string; name: string; custom: boolean; kind: string }[];
  };
  /**
   * Plugin updater (`plugin:updater|check`): nowa wersja z notatkami, `"offline"`
   * (sprawdzenie kończy się błędem sieci) albo brak aktualizacji (domyślnie).
   */
  update?: { version: string; notes?: string; date?: string } | "offline" | null;
  /**
   * Połączony Redmine (`redmineFixture()`): konto, statusy, projekty i
   * zgłoszenia dla komend `redmine_*`. Domyślnie Redmine nie jest połączony.
   */
  redmine?: RedmineFixture;
  /**
   * Stan obserwacji przypisań Jiry (`jira_watch_status`, B5); domyślnie
   * jeszcze nic nie sprawdzono.
   */
  jiraWatch?: { lastOk: number | null; lastError: string; lastErrorAt: number | null };
  /**
   * Główne okno schowane do traya (start z `--hidden`, B11): `isVisible()`
   * zwraca false do `__tauriMock.show()`, które zgłasza też `tauri://focus`.
   */
  hidden?: boolean;
  /** Historia powiadomień (`notification_log`, inbox.rs) na starcie, od najnowszych. */
  notifications?: unknown[];
  /**
   * Moduł Workflows (`workflows/mod.rs`): stan odblokowania na starcie i klucz,
   * który `workflow_unlock` przyjmuje (testowy, nie prawdziwy klucz aplikacji).
   * Definicje i edytor testuje się w podglądzie (`src/preview/workflows.ts`);
   * `detail` to jedna gotowa (np. opublikowana) definicja dla `workflow_list`
   * i `workflow_get`, żeby panel gotowości działał na zastępczym IPC.
   */
  workflows?: {
    unlocked?: boolean;
    key?: string;
    detail?: unknown;
    /** Grafy opublikowanych wersji (`workflow_version_graph`); domyślnie szkic `detail`. */
    versionGraphs?: Record<number, unknown>;
  };
  /**
   * Przebiegi Workflows (E5, `workflows/run_commands.rs`): lista i szczegóły zadane
   * przez test, zdarzenia i diff po `runId`, wynik `workflow_start` oraz błąd startu.
   * `workflow_decide` wzorowany na `driver/decide.rs`: ten sam `requestId` zwraca
   * zapisany wynik, inna decyzja albo nieaktualna rewizja to konflikt, a
   * `invalidate` zwraca błąd unieważnienia. Zmiana logiki w Rust wymaga zmiany tu.
   */
  workflowRuns?: {
    runs?: any[];
    /** Odpowiedź `workflow_faults` (problemy zgłoszone przez silnik po starcie). */
    faults?: any[];
    details?: Record<number, any>;
    events?: Record<number, any[]>;
    diffs?: Record<string, any>;
    startResult?: { runId: number; created: boolean };
    startError?: string;
    invalidate?: boolean;
    /** Po decyzji przebieg prosi o kolejną zgodę (drugi węzeł zgody w tym samym przebiegu). */
    followUpApproval?: boolean;
    /**
     * `workflow_recover` nie potwierdza końca procesu (rezerwacja zostaje); PID
     * procesu to `recoverUnconfirmedPid` (domyślnie 4242).
     */
    recoverUnconfirmed?: boolean;
    recoverUnconfirmedPid?: number;
    /** `workflow_resume` rzuca ten błąd (np. sterownik jeszcze się uruchamia). */
    resumeError?: string;
    /** `workflow_resume` znajduje naruszony worktree: przebieg kończy się `failed`. */
    resumeWorkspaceBroken?: boolean;
  };
  /**
   * Pytanie o zakończenie aplikacji (`quit.rs`): `pending` to nazwa procesu, którą
   * zwraca `app_quit_pending` (pytanie wysłane, zanim okno zaczęło nasłuchiwać),
   * a `error` to błąd `app_quit_confirmed` (np. nieudane zatrzymanie pomiaru).
   */
  quit?: { pending?: string | null; error?: string };
  /** Odpowiedź `workflow_preflight` (gotowość agentów zadana przez test). */
  preflight?: unknown[];
  /** Odpowiedź `workflow_preflight` dla konkretnych `versionId` (pierwszeństwo przed `preflight`). */
  preflightByVersion?: Record<number, unknown[]>;
}

/** Klucz Workflows w testach aplikacji (fixture), inny niż klucz produkcyjny. */
export const TEST_WORKFLOWS_KEY = "TUKTUK-WF-TESTY-TESTY-TESTY-TESTY";

export async function openDesktop(page: Page, options: TauriOptions = {}) {
  await page.clock.setFixedTime(new Date(options.now ?? NOW));
  await page.addInitScript(install, {
    data: options.data ?? seed(),
    settings: { ...defaultSettings, ...options.settings },
    backup: options.backup ?? null,
    backupExport: options.backupExport ?? null,
    jira: options.jira ?? null,
    templates: options.templates ?? [],
    suggestions: options.suggestions ?? [],
    window: options.window ?? "main",
    site: JIRA_SITE,
    redmineSite: REDMINE_SITE,
    update: options.update ?? null,
    redmine: options.redmine ?? null,
    jiraWatch: options.jiraWatch ?? null,
    hidden: options.hidden ?? false,
    heartbeat: options.heartbeat ?? null,
    notifications: options.notifications ?? [],
    preflight: options.preflight ?? [],
    preflightByVersion: options.preflightByVersion ?? {},
    workflowRuns: options.workflowRuns ?? {},
    quit: { pending: options.quit?.pending ?? null, error: options.quit?.error ?? "" },
    workflows: {
      unlocked: options.workflows?.unlocked ?? false,
      key: options.workflows?.key ?? TEST_WORKFLOWS_KEY,
      detail: options.workflows?.detail ?? null,
      versionGraphs: options.workflows?.versionGraphs ?? {},
    },
  });
  await page.goto("/");
}

/** Wywołuje funkcję zastępczego backendu w oknie strony. */
export function backend<T = unknown>(
  page: Page,
  name:
    | "idleTick"
    | "jiraCheck"
    | "redmineCheck"
    | "snapshot"
    | "calls"
    | "suggest"
    | "show"
    | "notify",
  arg?: unknown,
) {
  return page.evaluate(([n, a]) => (window as any).__tauriMock[n](a), [
    name,
    arg,
  ] as const) as Promise<T>;
}

function install(init: {
  data: any;
  settings: any;
  backup: { preview: any; snapshot: any } | null;
  backupExport: any;
  jira: { boards: any[]; issues: any[] } | null;
  templates: any[];
  suggestions: any[];
  window: string;
  site: string;
  redmineSite: string;
  update: { version: string; notes?: string; date?: string } | "offline" | null;
  redmine: RedmineFixture | null;
  jiraWatch: { lastOk: number | null; lastError: string; lastErrorAt: number | null } | null;
  hidden: boolean;
  heartbeat: number | null;
  notifications: any[];
  workflows: {
    unlocked: boolean;
    key: string;
    detail: any;
    versionGraphs: Record<number, any>;
  };
  preflight: unknown[];
  preflightByVersion: Record<number, unknown[]>;
  workflowRuns: any;
  quit: { pending: string | null; error: string };
}) {
  const w = window as any;
  w.isTauri = true;
  let snapshot = structuredClone(init.data);
  snapshot.trash ??= [];
  snapshot.trashSessions ??= [];
  const settings = structuredClone(init.settings);
  const calls: { cmd: string; args: any }[] = [];
  // templates::list (kolejność po nazwie) i find (bez względu na wielkość liter).
  let templates: any[] = structuredClone(init.templates);
  const sortedTemplates = () =>
    [...templates].sort((a, b) =>
      a.name.toLowerCase().localeCompare(b.name.toLowerCase(), "pl"),
    );
  const findTemplate = (name: string) =>
    templates.find((t) => t.name.toLowerCase() === name.trim().toLowerCase());
  const clone = <T>(v: T): T => structuredClone(v);
  /** Klucz repozytorium jak `runs::query::repo_key` (bez kanonikalizacji dysku). */
  const repoKey = (path: string): string =>
    path.trim().replace(/\//g, "\\").replace(/\\+$/, "").toLowerCase();

  // Zdarzenia: `listen` rejestruje callback przez transformCallback.
  const callbacks = new Map<number, (payload: any) => void>();
  const listeners: { event: string; id: number; handler: number }[] = [];
  let nextId = 1;
  function emit(event: string, payload: unknown = null) {
    for (const l of listeners.filter((x) => x.event === event))
      callbacks.get(l.handler)?.({ event, id: l.id, payload });
  }
  /**
   * `data-changed` po komendzie z `main.rs` zwracającej migawkę (B6): etykieta
   * okna, które ją wywołało (`DataChanged::source`); zmiany z backendu mają `null`.
   */
  const ownChange = { source: init.window };

  const nextSessionId = () =>
    Math.max(
      0,
      ...snapshot.sessions.map((s: any) => s.id),
      ...snapshot.trashSessions.map((s: any) => s.id),
    ) + 1;
  const nextTaskId = () =>
    Math.max(
      0,
      ...snapshot.tasks.map((t: any) => t.id),
      ...snapshot.trash.map((t: any) => t.id),
    ) + 1;
  const stopAll = (now: number) =>
    snapshot.sessions.forEach((s: any) => {
      if (s.endedAt === null) s.endedAt = Math.max(now, s.startedAt);
    });

  /** db::change("start"): poprzedni pomiar staje, chyba że to samo zadanie już trwa. */
  function startTask(taskId: number, now: number) {
    if (snapshot.sessions.some((s: any) => s.taskId === taskId && s.endedAt === null)) return;
    stopAll(now);
    snapshot.sessions.unshift({ id: nextSessionId(), taskId, startedAt: now, endedAt: null });
  }

  // idle::Clock (z `see` i `agent_tasks` dla „Przypisz przerwę do X”)
  const idle: {
    candidate: [number, number, number] | null;
    pending: any;
    awaySince: number | null;
    seen: { at: number; task: any }[];
  } = { candidate: null, pending: null, awaySince: null, seen: [] };
  const MIN10 = 600_000;
  function idleTick({
    userIdle,
    agentIdle = Number.MAX_SAFE_INTEGER,
    working = [],
  }: {
    userIdle: number;
    agentIdle?: number;
    /** Zadania pracujących agentów (`agents::Match`), jak `Clock::see`. */
    working?: any[];
  }) {
    if (!settings.idleEnabled) {
      idle.candidate = null;
      idle.pending = null;
      idle.awaySince = null;
      idle.seen = [];
      return false;
    }
    const now = Date.now();
    const agent = settings.idleAgents ? agentIdle : Number.MAX_SAFE_INTEGER;
    const active = snapshot.sessions.find((s: any) => s.endedAt === null);
    idle.seen = idle.seen.filter((x) => now - x.at <= 24 * 3_600_000);
    for (const task of working) {
      if (task.taskId == null || task.done || task.taskId === active?.taskId)
        continue;
      idle.seen = idle.seen.filter((x) => x.task.taskId !== task.taskId);
      idle.seen.push({ at: now, task: clone(task) });
    }
    if (!active) {
      idle.candidate = null;
      return false;
    }
    const idleMs = Math.min(userIdle, agent);
    let due = false;
    if (idleMs >= MIN10) {
      if (idle.candidate?.[0] !== active.id) {
        idle.candidate = [
          active.id,
          active.taskId,
          Math.max(now - idleMs, active.startedAt),
        ];
        idle.awaySince = Math.max(now - userIdle, active.startedAt);
      }
    } else if (userIdle >= MIN10) {
      idle.candidate = null;
    } else if (idle.candidate) {
      const [id, task, at] = idle.candidate;
      idle.candidate = null;
      if (id === active.id && !idle.pending && now - at >= MIN10) {
        const since = Math.min(idle.awaySince ?? at, at);
        idle.pending = {
          sessionId: id,
          taskId: task,
          idleAt: at,
          awaySince: since,
          returnedAt: now,
          agentTasks: idle.seen
            .filter((x) => x.at >= since && x.task.taskId !== task)
            .sort((a, b) => b.at - a.at)
            .map((x) => clone(x.task)),
        };
        due = true;
      }
    }
    if (due) emit("idle-notice");
    return due;
  }

  // gap::recover przy starcie: trwający pomiar po ≥ 10 min od pulsu kończy się
  // na pulsie (nie wcześniej niż jego start).
  let gap: any = null;
  {
    const active = snapshot.sessions.find((s: any) => s.endedAt === null);
    const started = Date.now();
    if (active && init.heartbeat !== null && started - init.heartbeat >= MIN10) {
      active.endedAt = Math.max(active.startedAt, init.heartbeat);
      gap = { sessionId: active.id, taskId: active.taskId, stoppedAt: active.endedAt, restartedAt: started };
    }
  }

  // jira::sync_assigned z ustaleniem pierwszego sprawdzenia jak w watch_once.
  const jira = {
    watched: false,
    issues: new Map<string, { status: string; assigned: boolean }>(),
  };
  for (const t of [...snapshot.tasks, ...snapshot.trash])
    if (t.jiraKey)
      jira.issues.set(t.jiraKey, {
        status: t.jiraStatus ?? "",
        assigned: false,
      });
  function jiraCheck(
    rows: { key: string; summary: string; status: string; open?: boolean }[],
  ) {
    if (!settings.jiraAutoImport) return [];
    const baseline = !jira.watched;
    const linked = new Set(
      [...snapshot.tasks, ...snapshot.trash]
        .map((t: any) => t.jiraKey)
        .filter(Boolean),
    );
    for (const r of rows) {
      const issue = jira.issues.get(r.key);
      if (issue) issue.status = r.status;
      else jira.issues.set(r.key, { status: r.status, assigned: false });
    }
    const imported: string[] = [];
    const now = Date.now();
    for (const r of rows.filter((x) => x.open !== false)) {
      const known = jira.issues.get(r.key)!.assigned || linked.has(r.key);
      if (baseline || known) continue;
      snapshot.tasks.unshift({
        id: nextTaskId(),
        title: [...r.summary].slice(0, 240).join(""),
        done: false,
        createdAt: now,
        jiraKey: r.key,
        jiraUrl: `${init.site}/browse/${r.key}`,
        jiraStatus: r.status,
      });
      imported.push(r.key);
    }
    jira.issues.forEach((issue) => (issue.assigned = false));
    for (const r of rows.filter((x) => x.open !== false))
      jira.issues.get(r.key)!.assigned = true;
    // Snapshot w Rust bierze status z jira_issues.
    for (const t of [...snapshot.tasks, ...snapshot.trash])
      if (t.jiraKey && jira.issues.has(t.jiraKey))
        t.jiraStatus = jira.issues.get(t.jiraKey)!.status;
    jira.watched = true;
    emit("data-changed");
    return imported;
  }

  // redmine::sync_assigned z ustaleniem pierwszego sprawdzenia jak w
  // redmine::watch_once; osobny stan i osobny wyłącznik (`redmineAutoImport`).
  const redmine = {
    watched: false,
    issues: new Map<
      number,
      { status: string; tracker: string; assigned: boolean }
    >(),
  };
  for (const t of [...snapshot.tasks, ...snapshot.trash])
    if (t.redmineId)
      redmine.issues.set(t.redmineId, {
        status: t.redmineStatus ?? "",
        tracker: t.redmineTracker ?? "",
        assigned: false,
      });
  // jira_fields::priority_band dla nazw priorytetów Redmine.
  const band = (priority = "") => {
    const n = priority.trim().toLowerCase();
    if (["immediate", "urgent", "high", "natychmiastowy", "pilny", "wysoki"].includes(n))
      return 1;
    if (["low", "niski"].includes(n)) return -1;
    return null;
  };
  function redmineCheck(
    rows: {
      id: number;
      subject: string;
      status: string;
      tracker?: string;
      priority?: string;
      /** Zgłoszenie zamknięte (`is_closed`): bez importu. */
      closed?: boolean;
    }[],
  ) {
    if (!settings.redmineAutoImport) return [];
    const baseline = !redmine.watched;
    const linked = new Set(
      [...snapshot.tasks, ...snapshot.trash]
        .map((t: any) => t.redmineId)
        .filter(Boolean),
    );
    const known = new Map(
      [...redmine.issues].map(([id, issue]) => [id, issue.assigned]),
    );
    const imported: number[] = [];
    const now = Date.now();
    for (const r of rows.filter((x) => !x.closed)) {
      if (baseline || known.get(r.id) || linked.has(r.id)) continue;
      const priority = band(r.priority);
      snapshot.tasks.unshift({
        id: nextTaskId(),
        title: [...r.subject].slice(0, 240).join(""),
        done: false,
        createdAt: now,
        redmineId: r.id,
        redmineUrl: `${init.redmineSite}/issues/${r.id}`,
        redmineStatus: r.status,
        redmineTracker: r.tracker ?? "",
        redmineDue: "",
        ...(priority !== null
          ? {
              details: {
                priority,
                plannedDay: "",
                dueDay: "",
                project: "",
                waiting: "",
                notes: {},
                estimateMinutes: 0,
                checklist: [],
                repeatDays: [],
                repeatFrom: "",
                order: 0,
                plannedStart: "",
              },
            }
          : {}),
      });
      imported.push(r.id);
    }
    for (const r of rows)
      redmine.issues.set(r.id, {
        status: r.status,
        tracker: r.tracker ?? redmine.issues.get(r.id)?.tracker ?? "",
        assigned: false,
      });
    redmine.issues.forEach((issue) => (issue.assigned = false));
    for (const r of rows.filter((x) => !x.closed))
      redmine.issues.get(r.id)!.assigned = true;
    // Snapshot w Rust bierze status i tracker z redmine_issues.
    for (const t of [...snapshot.tasks, ...snapshot.trash])
      if (t.redmineId && redmine.issues.has(t.redmineId)) {
        t.redmineStatus = redmine.issues.get(t.redmineId)!.status;
        t.redmineTracker = redmine.issues.get(t.redmineId)!.tracker;
      }
    redmine.watched = true;
    emit("data-changed");
    return imported;
  }

  // Odczyty Redmine na danych redmineFixture() (R6): redmine::parse_issue,
  // tracked_project_ids (projekt z podprojektami) i group_by_status (kolejność
  // statusów z /issue_statuses.json; w grupie pasmo priorytetu, potem termin).
  const rm = init.redmine;
  const rmIssue = (i: any) => {
    const status = rm?.statuses.find((s: any) => s.name === i.status);
    return {
      id: i.id,
      subject: i.subject,
      projectId: i.projectId,
      project: rm?.projects.find((p: any) => p.id === i.projectId)?.name ?? "",
      tracker: i.tracker,
      status: i.status,
      statusId: status?.id ?? 0,
      closed: status?.isClosed ?? false,
      priority: i.priority,
      assignedToId: i.assignedToId,
      assignedTo: i.assignedTo,
      version: i.version,
      category: i.category,
      dueDate: i.dueDate,
      updatedOn: "",
      url: `${init.redmineSite}/issues/${i.id}`,
    };
  };
  const rmWithChildren = (ids: number[]) => {
    const out = [...ids];
    for (let before = -1; before !== out.length; ) {
      before = out.length;
      for (const p of rm?.projects ?? [])
        if (
          !out.includes(p.id) &&
          p.parentId !== null &&
          out.includes(p.parentId)
        )
          out.push(p.id);
    }
    return out;
  };
  function rmGroups(issues: any[]) {
    return (rm?.statuses ?? [])
      .map((s: any) => ({
        status: s.name,
        closed: s.isClosed,
        ids: issues
          .map((i, index) => ({ i, index }))
          .filter(({ i }) => i.statusId === s.id)
          .sort(
            (a, b) =>
              (band(b.i.priority) ?? 0) - (band(a.i.priority) ?? 0) ||
              Number(!a.i.dueDate) - Number(!b.i.dueDate) ||
              a.i.dueDate.localeCompare(b.i.dueDate) ||
              a.index - b.index,
          )
          .map(({ i }) => i.id),
      }))
      .filter((g: any) => g.ids.length);
  }
  // redmine::start_issue: zadanie zgłoszenia (nowe albo z kosza) i jeden pomiar.
  function redmineStart(id: number) {
    const source = rm?.issues.find((i: any) => i.id === id);
    if (!source) throw "Odśwież listę zgłoszeń przed rozpoczęciem.";
    const issue = rmIssue(source);
    const now = Date.now();
    let task = snapshot.tasks.find((t: any) => t.redmineId === id);
    const trashed = snapshot.trash.find((t: any) => t.redmineId === id);
    if (!task && trashed) {
      snapshot.trash = snapshot.trash.filter((t: any) => t !== trashed);
      snapshot.tasks.unshift(trashed);
      task = trashed;
    }
    if (!task) {
      task = {
        id: nextTaskId(),
        title: [...issue.subject].slice(0, 240).join(""),
        done: false,
        createdAt: now,
        redmineId: id,
        redmineUrl: issue.url,
        redmineStatus: issue.status,
        redmineTracker: issue.tracker,
        redmineDue: issue.dueDate,
      };
      snapshot.tasks.unshift(task);
    }
    task.done = false;
    task.archived = false;
    if (
      !snapshot.sessions.some(
        (s: any) => s.taskId === task.id && s.endedAt === null,
      )
    ) {
      stopAll(now);
      snapshot.sessions.unshift({
        id: nextSessionId(),
        taskId: task.id,
        startedAt: now,
        endedAt: null,
      });
    }
    emit("data-changed");
    return clone(snapshot);
  }
  const rmNotConnected = "Połącz konto Redmine: podaj adres i klucz API.";

  // suggest::refresh: lista propozycji ustawiana z testu; zmiana wysyła zdarzenie.
  let suggestions: any[] = structuredClone(init.suggestions);
  const target = (t: any) =>
    t.taskId != null ? `t:${t.taskId}` : t.key ? `k:${t.key}` : "";
  function suggest(list: any[]) {
    suggestions = clone(list);
    emit("agent-suggestions", clone(suggestions));
  }

  /** Okno widoczne (`plugin:window|is_visible`); `show()` jak pokazanie z traya. */
  let visible = !init.hidden;
  function show() {
    visible = true;
    emit("tauri://focus");
  }
  // Historia powiadomień (`inbox.rs`): `notify` jak `notify::show` w Rust.
  let inbox: any[] = structuredClone(init.notifications);
  let workflowsUnlocked = init.workflows.unlocked;
  function workflowRunsIn(statuses: string[]): number {
    const byId = new Map<number, string>();
    for (const r of init.workflowRuns.runs ?? []) byId.set(r.id, r.status);
    for (const [id, d] of Object.entries(init.workflowRuns.details ?? {}))
      byId.set(Number(id), (d as any).summary.status);
    return [...byId.values()].filter((s) => statuses.includes(s)).length;
  }
  /** Jak `workflows::backup::active_work`: przebiegi z procesem w toku albo null. */
  function workflowActiveWork(): string | null {
    const count = workflowRunsIn(["queued", "running", "cancelling"]);
    return count > 0
      ? `Przebiegi Workflows w toku: ${count}. Poczekaj na ich koniec albo je zatrzymaj (przebieg czekający na zgodę nie blokuje).`
      : null;
  }
  /**
   * Jak `workflows::backup::restore_blocker`: każdy nieskończony przebieg, także
   * czekający na zgodę i wstrzymany po restarcie.
   */
  function workflowRestoreBlocker(): string | null {
    const count = workflowRunsIn([
      "queued",
      "running",
      "cancelling",
      "waitingForApproval",
      "paused",
    ]);
    return count > 0
      ? `Nieskończone przebiegi Workflows: ${count}. Poczekaj na ich koniec, zatwierdź albo odrzuć zgody, wznów wstrzymane przebiegi lub zatrzymaj przebiegi.`
      : null;
  }
  const inboxChanged = () => {
    emit("notification-log-changed", clone(inbox));
    return clone(inbox);
  };
  function notify(entry: { title: string; body?: string; open?: unknown }) {
    const id = Math.max(0, ...inbox.map((e) => e.id)) + 1;
    inbox.unshift({
      id,
      at: Date.now(),
      title: entry.title,
      body: entry.body ?? "",
      open: entry.open ?? { view: null },
      read: false,
    });
    return inboxChanged();
  }
  w.__tauriMock = {
    emit,
    show,
    notify,
    suggest,
    idleTick,
    jiraCheck,
    redmineCheck,
    snapshot: () => clone(snapshot),
    calls: () => clone(calls),
  };
  /** Stan przebiegów fixture (test może zmienić status przed zdarzeniem `workflow-event`). */
  w.__workflowRunsState = init.workflowRuns;
  /** Pytanie o zakończenie (`quit.rs`); test może zmienić `error` w trakcie. */
  w.__quitState = init.quit;
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  w.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: init.window },
      currentWebview: { label: init.window },
    },
    transformCallback: (cb: (payload: any) => void) => {
      const id = nextId++;
      callbacks.set(id, cb);
      return id;
    },
    unregisterCallback: (id: number) => callbacks.delete(id),
    invoke: async (cmd: string, args: any = {}) => {
      calls.push({ cmd, args: clone(args) });
      const now = Date.now();
      switch (cmd) {
        case "plugin:event|listen": {
          const id = nextId++;
          listeners.push({ event: args.event, id, handler: args.handler });
          return id;
        }
        case "plugin:event|unlisten": {
          const i = listeners.findIndex((l) => l.id === args.eventId);
          if (i >= 0) listeners.splice(i, 1);
          return;
        }
        case "plugin:window|is_maximized":
          return false;
        case "plugin:window|is_visible":
          return visible;
        case "get_snapshot":
          return clone(snapshot);
        case "get_settings":
          return clone(settings);
        case "set_settings":
          Object.assign(settings, args.patch);
          emit("settings-changed", clone(settings));
          return clone(settings);
        case "app_info":
          return {
            version: "test",
            shortcutErrors: { quick: "", toggle: "" },
          };
        case "autostart_get":
          return { enabled: false, hidden: settings.autostartHidden };
        case "widget_status":
          return {
            enabled: settings.widgetEnabled,
            backgroundOnly: settings.widgetBackgroundOnly,
            dismissed: false,
          };
        case "widget_set_layout":
          return args.compact && args.expanded;
        case "widget_can_collapse":
          return !(globalThis as any).__widgetQuickOpen;
        case "agents_status":
          return [];
        // Tablica sesji (`agent_board.rs`): logika w e2e na podglądzie
        // (`preview/hooks.ts`); tu bez zdarzeń hooków, więc pusta.
        case "agent_board":
        case "agent_board_seen":
        case "agent_board_seen_all":
          return [];
        // Jak `workflows::workflow_access` / `workflow_unlock`: klucz po
        // przycięciu i wielkich literach; trwały znacznik (tu: do przeładowania).
        case "workflow_access":
          return { unlocked: workflowsUnlocked };
        case "workflow_unlock":
          if (
            String(args.key ?? "").trim().toUpperCase() !==
            init.workflows.key.toUpperCase()
          )
            throw "Nieprawidłowy klucz dostępu.";
          workflowsUnlocked = true;
          return { unlocked: true };
        // Jak `workflows::workflow_lock`: odmowa przy przebiegach queued, running albo
        // cancelling (`backup::active_work`; waitingForApproval nie blokuje), inaczej
        // usuwa tylko znacznik odblokowania. `window.__workflowLockHold` (obietnica)
        // wstrzymuje jedno, najbliższe wywołanie (okno zostaje otwarte i zajęte).
        case "workflow_lock": {
          w.__workflowLocks = (w.__workflowLocks ?? 0) + 1;
          const hold = w.__workflowLockHold;
          w.__workflowLockHold = null;
          if (hold) await hold;
          const all = [
            ...(init.workflowRuns.runs ?? []),
            ...Object.values<any>(init.workflowRuns.details ?? {}).map((d) => d.summary),
          ];
          const active = new Set(
            all
              .filter((r: any) => ["queued", "running", "cancelling"].includes(r.status))
              .map((r: any) => r.id),
          );
          if (active.size > 0)
            throw `Wyłączenie wstrzymane. Przebiegi Workflows w toku: ${active.size}. Poczekaj na ich koniec albo je zatrzymaj (przebieg czekający na zgodę nie blokuje).`;
          workflowsUnlocked = false;
          return { unlocked: false };
        }
        // Definicje (`workflows::store`): logika w Rust i podglądzie; tu tylko
        // jedna zadana przez test definicja (`workflows.detail`), bez zapisu.
        case "workflow_list": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          const detail = init.workflows.detail;
          if (!detail) return [];
          const { draft: _d, issues: _i, versions: _v, ...summary } = detail;
          return [clone(summary)];
        }
        case "workflow_get":
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          if (!init.workflows.detail || init.workflows.detail.id !== args.id)
            throw "Nie ma takiego procesu.";
          return clone(init.workflows.detail);
        // Publikacja: nowa wersja z kolejnym id na początku listy (jak `store::publish`).
        case "workflow_publish": {
          const detail = init.workflows.detail;
          if (!detail || detail.id !== args.id) throw "Nie ma takiego procesu.";
          if (detail.draftRevision !== args.expectedRevision)
            throw "Definicja zmieniła się w międzyczasie.";
          const version = (detail.publishedVersion ?? 0) + 1;
          const id = Math.max(0, ...detail.versions.map((v: any) => v.id)) + 1;
          detail.versions = [
            { id, version, hash: `h${version}`, createdAt: detail.updatedAt },
            ...detail.versions,
          ];
          detail.publishedVersion = version;
          detail.unpublishedChanges = false;
          return clone(detail);
        }
        // Gotowość agentów (`workflows::preflight`): logika w Rust i jej testach;
        // fixture zwraca dane zadane przez test (`options.preflight`) i liczy
        // wywołania w `window.__preflightCalls` (argumenty: `__tauriMock.calls()`).
        // `window.__preflightHold` (obietnica) wstrzymuje jedno, najbliższe wywołanie.
        case "workflow_preflight": {
          w.__preflightCalls = (w.__preflightCalls ?? 0) + 1;
          const hold = w.__preflightHold;
          w.__preflightHold = null;
          const result = clone(
            init.preflightByVersion?.[args.versionId] ?? init.preflight,
          );
          if (hold) await hold;
          return result;
        }
        // Jak `workflows::workflow_version_graph`: tylko po odblokowaniu, wersja z `detail`.
        case "workflow_version_graph": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          const detail = init.workflows.detail;
          const known = detail?.versions?.some((v: any) => v.id === args.versionId);
          if (!known) throw "Nie znaleziono wersji procesu.";
          return clone(init.workflows.versionGraphs?.[args.versionId] ?? detail.draft);
        }
        // Przebiegi (`workflows::run_commands`): historia i anulowanie bez klucza,
        // start, decyzja i artefakt po odblokowaniu. Żadna z tych komend nie rusza timera.
        case "workflow_runs": {
          const f = args.filter ?? {};
          let list = clone(init.workflowRuns.runs ?? []);
          if (f.status?.length) list = list.filter((r: any) => f.status.includes(r.status));
          if (f.workflowId != null) list = list.filter((r: any) => r.workflowId === f.workflowId);
          if (f.taskId != null) list = list.filter((r: any) => r.taskId === f.taskId);
          // Jak `runs::query::repo_key`: bez wielkości liter, oba separatory
          // równoważne, bez końcowego separatora; porównanie dosłowne (bez wzorców).
          if (f.repo?.trim()) {
            const key = repoKey(f.repo);
            list = list.filter((r: any) => r.repo != null && repoKey(r.repo) === key);
          }
          if (f.from != null) list = list.filter((r: any) => r.createdAt >= f.from);
          if (f.to != null) list = list.filter((r: any) => r.createdAt <= f.to);
          if (f.beforeId != null) list = list.filter((r: any) => r.id < f.beforeId);
          list.sort((a: any, b: any) => b.id - a.id);
          return list.slice(0, Math.min(f.limit ?? 100, 100));
        }
        case "workflow_run_get": {
          const detail = init.workflowRuns.details?.[args.runId];
          if (!detail) throw "Nie znaleziono przebiegu.";
          // Hak testu: odpowiedź ze stanu z chwili wywołania, oddana po `__runGetHold`
          // (odtwarza wyprzedzenie starszej odpowiedzi przez nowszą).
          const snapshot = clone(detail);
          const hold = w.__runGetHold;
          if (hold) await hold;
          return snapshot;
        }
        case "workflow_run_events": {
          const all = init.workflowRuns.events?.[args.runId] ?? [];
          return clone(
            all.filter((e: any) => e.sequence > args.after).slice(0, args.limit ?? 500),
          );
        }
        // Jak `run_commands::cancel`: brak przebiegu i stan końcowy (lista z
        // `RunStatus::is_terminal`) to błąd, `cancelling` to cichy sukces.
        case "workflow_cancel": {
          const detail = init.workflowRuns.details?.[args.runId];
          const status =
            detail?.summary.status ??
            init.workflowRuns.runs?.find((r: any) => r.id === args.runId)?.status;
          if (!status) throw "Nie znaleziono przebiegu.";
          if (["succeeded", "rejected", "failed", "cancelled", "interrupted"].includes(status))
            throw "Przebieg już się zakończył.";
          w.__workflowCancels = (w.__workflowCancels ?? 0) + 1;
          return null;
        }
        // Jak `run_commands::resume_run` i `driver/resume.rs`: klucz wymagany, ten
        // sam requestId zwraca zapisany stan bez drugiego wznowienia, poza `paused`
        // błąd; naruszony worktree kończy przebieg `failed`/`workspaceUnavailable`.
        case "workflow_resume": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          if (!String(args.requestId ?? "").trim()) throw "Brak identyfikatora żądania.";
          // `window.__workflowResumeHold` (obietnica) wstrzymuje najbliższą odpowiedź.
          const hold = w.__workflowResumeHold;
          w.__workflowResumeHold = null;
          if (hold) await hold;
          if (init.workflowRuns.resumeError) throw init.workflowRuns.resumeError;
          const detail = init.workflowRuns.details?.[args.runId];
          const listed = init.workflowRuns.runs?.find((r: any) => r.id === args.runId);
          if (!detail && !listed) throw "Nie znaleziono przebiegu.";
          const status = detail?.summary.status ?? listed.status;
          // Jak `RunState::resume_requests`: powtórka w tej samej pauzie zwraca
          // bieżący stan bez drugiego wznowienia.
          const saved: Record<string, boolean> = (w.__workflowResumeRequests ??= {});
          const key = `${args.runId}|${args.requestId}`;
          if (saved[key]) return status;
          if (status !== "paused") throw "Przebieg nie jest wstrzymany.";
          const broken = !!init.workflowRuns.resumeWorkspaceBroken;
          const next = broken ? "failed" : "running";
          saved[key] = true;
          w.__workflowResumes = (w.__workflowResumes ?? 0) + 1;
          for (const summary of [detail?.summary, listed].filter(Boolean)) {
            summary.status = next;
            if (broken) summary.errorCode = "workspaceUnavailable";
          }
          if (detail) {
            detail.revision += 2;
            const event = {
              runId: args.runId,
              sequence: detail.revision,
              executionId: null,
              type: "runStateChanged",
              timestamp: now,
              payload: {
                type: "runStateChanged",
                from: "paused",
                to: next,
                code: broken ? "workspaceUnavailable" : null,
              },
            };
            const requested = {
              ...event,
              sequence: detail.revision - 1,
              payload: { type: "resumeRequested", requestId: args.requestId },
            };
            (init.workflowRuns.events ??= {})[args.runId] = (
              init.workflowRuns.events[args.runId] ?? []
            ).concat(requested, event);
            emit("workflow-event", requested);
            emit("workflow-event", event);
          }
          return next;
        }
        // Jak `quit.rs`: oczekujące pytanie, anulowanie i potwierdzenie (błąd
        // zatrzymania pomiaru wraca do okna; po sukcesie pytanie znika).
        case "app_quit_pending":
          return init.quit.pending;
        case "app_quit_cancel":
          init.quit.pending = null;
          return null;
        case "app_quit_confirmed":
          if (init.quit.error) throw init.quit.error;
          init.quit.pending = null;
          w.__appQuits = (w.__appQuits ?? 0) + 1;
          return null;
        // Jak `run_commands::workflow_faults`: bez klucza, kopia dziennika problemów.
        case "workflow_faults":
          return clone(init.workflowRuns.faults ?? []);
        case "workflow_start": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          if (init.workflowRuns.startError) throw init.workflowRuns.startError;
          w.__workflowStarts = (w.__workflowStarts ?? []).concat(clone(args.request));
          // Jak `service::read_phase`: ten sam requestId z inną wersją to konflikt.
          const seen: Record<string, number> = (w.__workflowRequestIds ??= {});
          const previous = seen[args.request.requestId];
          if (previous !== undefined && previous !== args.request.versionId)
            throw "Ten identyfikator żądania został użyty przy innym procesie.";
          const created = previous === undefined;
          if (created) seen[args.request.requestId] = args.request.versionId;
          const base = init.workflowRuns.startResult ?? { runId: 1, created: true };
          return { runId: base.runId, created };
        }
        // Jak `recovery::recover` (E6): bez klucza, tylko przerwany przebieg; po
        // potwierdzonym końcu procesów zwalnia katalog i dopisuje zdarzenie `recovery`.
        // `recoverUnconfirmed` odtwarza proces, którego końca nie potwierdzono.
        case "workflow_recover": {
          const detail = init.workflowRuns.details?.[args.runId];
          if (!detail) throw "Nie znaleziono przebiegu.";
          if (detail.summary.status !== "interrupted")
            throw "Odzyskać można tylko przerwany przebieg.";
          const view = (detail.recovery ??= {
            workspaceReserved: true,
            processesUnconfirmed: 0,
            unconfirmedPids: [],
            attempts: 0,
          });
          view.unconfirmedPids ??= [];
          w.__workflowRecovers = (w.__workflowRecovers ?? 0) + 1;
          if (!view.workspaceReserved && view.processesUnconfirmed === 0)
            return {
              processesConfirmed: true,
              workspaceReleased: true,
              alreadyRecovered: true,
              unconfirmedPids: [],
            };
          const confirmed = !init.workflowRuns.recoverUnconfirmed;
          const pids: number[] = confirmed
            ? []
            : [init.workflowRuns.recoverUnconfirmedPid ?? 4242];
          const hadReservation = view.workspaceReserved;
          view.attempts += 1;
          if (confirmed) {
            view.workspaceReserved = false;
            view.processesUnconfirmed = 0;
            view.unconfirmedPids = [];
          } else {
            view.processesUnconfirmed = pids.length;
            view.unconfirmedPids = pids;
          }
          detail.revision += 1;
          // Jak `recovery::unconfirmed_note`.
          const note = `Nie potwierdzono końca procesu agenta (PID ${pids.join(", ")}); katalog roboczy pozostaje zablokowany. Zakończ proces ręcznie i spróbuj ponownie.`;
          const event = {
            runId: args.runId,
            sequence: detail.revision,
            executionId: null,
            type: "recovery",
            timestamp: now,
            payload: {
              type: "reconciled",
              processesConfirmed: confirmed,
              workspaceReleased: confirmed && hadReservation,
              ...(confirmed ? {} : { note }),
            },
          };
          (init.workflowRuns.events ??= {})[args.runId] = (
            init.workflowRuns.events[args.runId] ?? []
          ).concat(event);
          emit("workflow-event", event);
          return {
            processesConfirmed: confirmed,
            workspaceReleased: confirmed || !hadReservation,
            alreadyRecovered: false,
            unconfirmedPids: pids,
          };
        }
        case "workflow_artifact_read": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          const diff = init.workflowRuns.diffs?.[args.artifactHash];
          if (!diff) throw "Nie znaleziono artefaktu tego przebiegu.";
          return { redacted: 0, ...clone(diff) };
        }
        // Jak `driver/decide.rs`: komentarz przycięty, odrzucenie bez niego to błąd,
        // requestId użyty przy innej zgodzie to konflikt, powtórka zwraca zapisany
        // wynik, a nieaktualna rewizja to błąd z prefiksem unieważnienia.
        case "workflow_decide": {
          if (!workflowsUnlocked) throw "Moduł Workflows nie jest odblokowany.";
          const c = args.command;
          const comment = (c.comment ?? "").trim() || null;
          if (c.decision === "reject" && !comment) throw "Odrzucenie wymaga komentarza.";
          const detail = init.workflowRuns.details?.[c.runId];
          const approval = detail?.approvals.find((a: any) => a.id === c.approvalId);
          if (!approval) throw "Nie znaleziono zgody.";
          const conflict = "Zgoda została już rozstrzygnięta albo przebieg się zmienił.";
          const saved = (w.__workflowDecisions ??= {});
          const known = saved[c.requestId];
          if (known && known.approvalId !== c.approvalId) throw conflict;
          if (approval.status !== "pending") {
            if (!known) throw conflict;
            return clone(known.result);
          }
          if (c.expectedRevision !== detail.revision)
            throw "Zgoda unieważniona: Przebieg zmienił się od ostatniego odczytu. Przejrzyj zgodę jeszcze raz.";
          if (init.workflowRuns.invalidate)
            throw "Zgoda unieważniona: Zmiany w katalogu roboczym unieważniły zgodę. Przejrzyj je jeszcze raz.";
          const approvalStatus = c.decision === "approve" ? "approved" : "rejected";
          const followUp = init.workflowRuns.followUpApproval && c.decision === "approve";
          const result = {
            approvalStatus,
            runStatus: followUp
              ? "waitingForApproval"
              : c.decision === "approve"
                ? "succeeded"
                : "rejected",
          };
          saved[c.requestId] = { approvalId: c.approvalId, result };
          w.__workflowDecideCount = (w.__workflowDecideCount ?? 0) + 1;
          w.__workflowDecideRequests = (w.__workflowDecideRequests ?? []).concat(c.requestId);
          approval.status = approvalStatus;
          approval.comment = comment;
          detail.revision += 2;
          if (followUp) {
            init.workflowRuns.followUpApproval = false;
            detail.approvals.push({
              ...approval,
              id: `${approval.id}-next`,
              status: "pending",
              comment: null,
            });
          } else {
            detail.summary.status = result.runStatus;
            detail.summary.waitingApproval = false;
          }
          return clone(result);
        }
        case "notification_log":
          return clone(inbox);
        case "notification_log_read":
          inbox = inbox.map((e) =>
            args.id == null || e.id === args.id ? { ...e, read: true } : e,
          );
          return inboxChanged();
        case "notification_log_clear":
          inbox = [];
          return inboxChanged();
        // „Przejdź” wymaga sesji z tablicy, a tablica w fixture jest pusta.
        case "agent_board_open":
          throw "Tej sesji nie ma już na tablicy.";
        // Czas czekania (`agent_wait.rs`): bez zdarzeń hooków zero.
        case "agent_wait_time":
          return { totalMs: 0, count: 0, longestMs: 0 };
        // Asystent AI (A1): logika w `ai.rs`, e2e na podglądzie; tu bez wykrytych CLI.
        case "ai_status":
          return {
            detected: { claude: null, codex: null },
            engine: settings.aiEngine,
            ask: false,
            active: false,
            dir: "",
          };
        // Pytania o przesunięcie w Jirze (J1) pokrywa podgląd ze store.ts.
        case "jira_move_prompts":
          return [];
        case "agent_suggestions":
          return clone(suggestions);
        case "agent_dismiss": {
          // suggest::agent_dismiss: „Nie teraz” wycisza cel do końca pracy.
          const goal = target(args);
          suggestions = suggestions.filter((s: any) => target(s.task) !== goal);
          return clone(suggestions);
        }
        case "agent_switch": {
          // suggest::agent_switch: start X (poprzedni pomiar staje).
          const task = snapshot.tasks.find((t: any) => t.id === args.taskId);
          if (!task) throw "Brak zadania do przełączenia.";
          stopAll(now);
          snapshot.sessions.unshift({
            id: nextSessionId(),
            taskId: task.id,
            startedAt: now,
            endedAt: null,
          });
          suggestions = suggestions.filter(
            (s: any) => target(s.task) !== target(args),
          );
          emit("data-changed");
          return clone(snapshot);
        }
        // Czas agentów (W1): bez okresów; e2e toru i kolumn działa na podglądzie.
        // B13: start po zakończeniu dnia (`days::day_start_request` /
        // `day_start_answer`); testy sprawdzają wywołania, start wykonuje Rust.
        case "day_start_request":
          return null;
        case "day_start_answer":
          return clone(snapshot);
        case "agent_time":
          return { periods: [], totals: [], unmatchedMs: 0 };
        // Tryb skupienia: bez bloku (logika w `focus.rs`, e2e na podglądzie).
        case "focus_status":
          return {
            block: null,
            ended: null,
            pause: null,
            resume: null,
            queued: 0,
            today: 0,
            week: 0,
          };
        // Kalendarz niepołączony (logika w `calendar.rs`, e2e na podglądzie).
        case "calendar_status":
          return {
            clientId: "",
            tenant: "organizations",
            email: "",
            connected: false,
            connecting: false,
            lastSync: null,
            lastError: "",
          };
        case "calendar_events":
          return [];
        // Zewnętrzne sterowanie: program obok aplikacji; logika potoku w `api.rs`.
        case "external_api_info":
          return {
            ctlPath: "C:\\Program Files\\TukTuk Track\\tuktuk-track-ctl.exe",
          };
        // Rozszerzenie przeglądarki (W4) bez połączenia; logika w `browser.rs`.
        case "browser_status":
          return {
            enabled: settings.browserExtension === true,
            registered: settings.browserExtension === true,
            connected: false,
            lastMessage: null,
            lastKey: null,
            lastKeyAt: null,
            error: "",
            extensionId: "cecpcpkmmmkgfdklnkkkigofkcibnhbl",
            extensionFolder: null,
          };
        case "jira_settings":
          return {
            config: {
              site: init.site,
              email: init.jira ? "test@example.invalid" : "",
              displayName: init.jira ? "Użytkownik testowy" : "",
              boards: init.jira ? clone(init.jira.boards) : [],
            },
            connected: !!init.jira,
          };
        // Redmine (PLAN_1.9.md): bez `redmine` w opcjach niepołączone, więc
        // `#1234` w szybkim dodawaniu jest projektem; z `redmineFixture()`
        // połączone kontem i danymi fixture (R6).
        case "redmine_settings":
          return {
            config: {
              site: rm ? init.redmineSite : "",
              login: rm?.user.login ?? "",
              userId: rm?.user.id ?? 0,
              displayName: rm?.user.name ?? "",
              acceptInvalidCert: false,
              projects: rm
                ? clone(
                    rm.projects.filter((p: any) => rm.tracked.includes(p.id)),
                  )
                : [],
            },
            connected: !!rm,
          };
        case "redmine_issues": {
          if (!rm) throw rmNotConnected;
          const scope =
            args.projectId == null ? null : rmWithChildren([args.projectId]);
          const issues = rm.issues
            .map(rmIssue)
            .filter((i: any) => scope === null || scope.includes(i.projectId))
            .filter(
              (i: any) =>
                !(args.mine || args.projectId == null) ||
                i.assignedToId === rm.user.id,
            )
            .filter((i: any) => !args.openOnly || !i.closed);
          return {
            issues,
            warnings: [],
            groups: rmGroups(issues),
            tracked: rmWithChildren(rm.tracked),
          };
        }
        case "redmine_projects":
          if (!rm) throw rmNotConnected;
          return clone(rm.projects);
        case "redmine_statuses":
          if (!rm) throw rmNotConnected;
          return clone(rm.statuses);
        case "redmine_start":
          if (!rm) throw rmNotConnected;
          return redmineStart(args.id);
        case "redmine_open":
          return null;
        case "jira_watch_status":
          return clone(
            init.jiraWatch ?? { lastOk: null, lastError: "", lastErrorAt: null },
          );
        case "jira_boards":
          return { values: clone(init.jira?.boards ?? []), nextStart: null };
        case "jira_issues":
          return {
            issues: (init.jira?.issues ?? []).map((i: any) => ({
              ...i,
              url: `${init.site}/browse/${i.key}`,
              fields: i.fields ?? [],
            })),
            warnings: [],
          };
        // Lista pól Jiry (`jira_fields`) i odświeżenie wartości po zmianie
        // ustawień (`jira_refresh_fields`): w fixture stała lista, bez odświeżania.
        case "jira_fields":
          return clone(init.jira?.fields ?? []);
        case "jira_refresh_fields":
          return 0;
        // Tablica Jiry (J2) na danych fixture: kolumny jak w Jirze, karta
        // w kolumnie swojego statusu (przypisanie spoza kolumn testuje Rust
        // `jira_board::column_for` i e2e na podglądzie).
        case "jira_board": {
          const columns = [
            { name: "Do zrobienia", statusIds: ["1"] },
            { name: "W toku", statusIds: ["3"] },
            { name: "Gotowe", statusIds: ["5"] },
          ];
          const byName: Record<string, [string, string, number]> = {
            "Do zrobienia": ["1", "new", 0],
            "W toku": ["3", "indeterminate", 1],
            Gotowe: ["5", "done", 2],
          };
          const board = init.jira?.boards.find((b: any) => b.id === args.boardId);
          const issues = (init.jira?.issues ?? [])
            .filter((i: any) => board && i.boards.includes(board.name))
            .map((i: any) => {
              const [statusId, category, column] = byName[i.status] ?? [
                "",
                "indeterminate",
                1,
              ];
              return {
                key: i.key,
                summary: i.summary,
                status: i.status,
                statusId,
                category,
                issueType: i.issueType ?? "Zadanie",
                priority: i.priority ?? "Medium",
                assignee: i.assignee ?? null,
                url: `${init.site}/browse/${i.key}`,
                column,
                fields: i.fields ?? [],
              };
            });
          return {
            boardId: args.boardId,
            project: "WMS",
            columns,
            issues,
            truncated: false,
            limit: 300,
            layoutAt: Date.now(),
          };
        }
        case "gap_notice":
          return clone(gap);
        case "gap_resolve": {
          // gap::resolve: keep / restore / resume.
          if (!gap) throw "Brak zgłoszonej przerwy w pracy aplikacji.";
          if (args.action === "restore") {
            const s = snapshot.sessions.find((x: any) => x.id === gap.sessionId);
            const task = snapshot.tasks.find((t: any) => t.id === gap.taskId);
            if (!s && !snapshot.trashSessions.some((x: any) => x.id === gap.sessionId))
              throw "Nie znaleziono przedziału.";
            if (!s || !task) throw "Przedział należy do zadania w koszu.";
            if (task.done) throw "Najpierw przywróć zadanie do zrobienia.";
            if (s.endedAt !== gap.stoppedAt)
              throw "Przedział zmieniono po zatrzymaniu. Popraw go na osi dnia.";
            if (snapshot.sessions.some((x: any) => x.endedAt === null))
              throw "Trwa inny pomiar. Zatrzymaj go, aby przywrócić cały czas.";
            if ([...snapshot.sessions, ...snapshot.trashSessions].some(
                (x: any) => x.id !== s.id && x.endedAt > gap.stoppedAt))
              throw "Po zatrzymaniu zapisano inny pomiar. Przywrócenie nałożyłoby czas.";
            s.endedAt = null;
            task.archived = false;
            // planner::clear_waiting jak start w db::change.
            if (task.details?.waiting)
              Object.assign(task.details, {
                waiting: "",
                waitingSince: 0,
                waitingStatus: "",
                waitingRemindDay: "",
                waitingDismissed: task.jiraStatus ?? "",
              });
          } else if (args.action === "resume") {
            const task = snapshot.tasks.find((t: any) => t.id === gap.taskId);
            if (!task) throw "Najpierw przywróć zadanie z kosza.";
            if (task.done) throw "Najpierw przywróć zadanie do zrobienia.";
            startTask(task.id, now);
          } else if (args.action !== "keep") throw "Nieznana odpowiedź.";
          gap = null;
          emit("data-changed");
          emit("gap-notice");
          return clone(snapshot);
        }
        case "idle_notice":
          return clone(idle.pending);
        case "idle_resolve": {
          const notice = idle.pending;
          if (!notice) throw "Brak zgłoszonej bezczynności.";
          if (args.assign != null) {
            // idle::assign_break → db::split_session w chwili wyjścia użytkownika
            // (`awaySince`), a gdy to start pomiaru, db::move_session.
            if (!notice.agentTasks.some((t: any) => t.taskId === args.assign))
              throw "To zadanie nie jest pracą agenta z tej przerwy.";
            const s = snapshot.sessions.find(
              (x: any) => x.id === notice.sessionId,
            );
            if (!s) throw "Nie znaleziono przedziału.";
            const split = notice.awaySince;
            const end = s.endedAt ?? now;
            if (split <= s.startedAt) s.taskId = args.assign;
            else {
              if (!(split < end))
                throw "Moment podziału musi leżeć wewnątrz przedziału.";
              snapshot.sessions.unshift({
                id: nextSessionId(),
                taskId: args.assign,
                startedAt: split,
                endedAt: s.endedAt,
              });
              s.endedAt = split;
            }
          } else if (args.trim && args.resume) {
            // idle::trim_and_resume: z pomiaru wypada tylko przerwa.
            const s = snapshot.sessions.find(
              (x: any) => x.id === notice.sessionId,
            );
            if (!s) throw "Nie znaleziono przedziału.";
            if (s.endedAt !== null && s.endedAt <= notice.returnedAt) {
              if (s.endedAt > notice.idleAt)
                s.endedAt = Math.max(s.startedAt, notice.idleAt);
            } else if (notice.idleAt <= s.startedAt)
              s.startedAt = notice.returnedAt;
            else {
              snapshot.sessions.unshift({
                id: nextSessionId(),
                taskId: s.taskId,
                startedAt: notice.returnedAt,
                endedAt: s.endedAt,
              });
              s.endedAt = notice.idleAt;
            }
          } else if (args.trim) {
            const s = snapshot.sessions.find(
              (x: any) => x.id === notice.sessionId,
            );
            if (s && (s.endedAt === null || s.endedAt > notice.idleAt))
              s.endedAt = Math.max(s.startedAt, notice.idleAt);
          }
          idle.pending = null;
          emit("data-changed");
          return clone(snapshot);
        }
        case "mutate": {
          const task = snapshot.tasks.find((t: any) => t.id === args.id);
          if (args.action === "stop") stopAll(now);
          else if (args.action === "start" && task) startTask(task.id, now);
          else throw "Nieobsługiwana akcja w teście: " + args.action;
          emit("data-changed", ownChange);
          return clone(snapshot);
        }
        case "set_waiting": {
          // planner::set_waiting: tylko pola blokady; status Jiry z jira_issues.
          const task = snapshot.tasks.find((t: any) => t.id === args.id);
          if (!task) throw "Nie znaleziono zadania.";
          const status = task.jiraStatus ?? "";
          const text = String(args.change.text ?? "").trim();
          const d = (task.details ??= {});
          if (args.change.action === "block") {
            if (!text) throw "Opis blokady musi mieć od 1 do 300 znaków.";
            Object.assign(d, {
              waiting: text,
              waitingSince: now,
              waitingStatus: status,
              waitingRemindDay: "",
              waitingDismissed: "",
            });
            snapshot.sessions.forEach((s: any) => {
              if (s.taskId === task.id && s.endedAt === null)
                s.endedAt = Math.max(s.startedAt, now);
            });
          } else if (args.change.action === "unblock")
            Object.assign(d, {
              waiting: "",
              waitingSince: 0,
              waitingStatus: "",
              waitingRemindDay: "",
              waitingDismissed: status,
            });
          else if (args.change.action === "dismiss")
            d.waitingDismissed = status;
          else throw "Nieobsługiwana zmiana blokady w teście.";
          emit("data-changed", ownChange);
          return clone(snapshot);
        }
        case "templates_list":
          return clone(sortedTemplates());
        case "template_save": {
          // templates::save (walidację nazwy i pól sprawdzają testy Rust).
          const name = String(args.name).trim();
          const other = findTemplate(name);
          if (other && other.id !== args.id)
            throw `Szablon „${other.name}” już istnieje.`;
          const body = {
            ...args.template,
            steps: args.template.steps
              .map((s: string) => s.trim())
              .filter(Boolean),
          };
          if (args.id != null) {
            const t = templates.find((x: any) => x.id === args.id);
            if (!t) throw "Nie znaleziono szablonu.";
            Object.assign(t, body, { name });
          } else
            templates.push({
              id: Math.max(0, ...templates.map((t: any) => t.id)) + 1,
              name,
              ...body,
            });
          emit("templates-changed");
          return clone(sortedTemplates());
        }
        case "template_delete":
          templates = templates.filter((t: any) => t.id !== args.id);
          emit("templates-changed");
          return clone(sortedTemplates());
        case "quick_add": {
          // quickadd::create z szablonem: templates::render_title i merge_details,
          // tytuł zgłoszenia z listy Jiry (jira::cached_summary).
          const task = args.task;
          const key = task.jiraKey || null;
          if (key && !init.jira)
            throw `Jira nie jest połączona, więc ${key} nie zostanie powiązane.`;
          // Redmine (quickadd::create z `#1234`): temat z listy
          // (redmine::cached_subject), jedno zgłoszenie na zadanie, bez duplikatu.
          const redmineId: number | null = task.redmineId ?? null;
          if (redmineId !== null) {
            if (key) throw "Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.";
            if (!rm)
              throw `Redmine nie jest połączone, więc #${redmineId} nie zostanie powiązane.`;
            const existing = [...snapshot.tasks, ...snapshot.trash].find(
              (t: any) => t.redmineId === redmineId,
            );
            if (existing)
              throw `#${redmineId} ma już zadanie „${existing.title}”.`;
            const source = rm.issues.find((i: any) => i.id === redmineId);
            const title = String(task.title).trim() || source?.subject || "";
            if (!title)
              throw `Nie znam tematu #${redmineId}. Wpisz tytuł albo odśwież listę w widoku Redmine.`;
            const issue = source ? rmIssue(source) : null;
            const id = nextTaskId();
            snapshot.tasks.unshift({
              id,
              title,
              done: false,
              createdAt: now,
              redmineId,
              redmineUrl: `${init.redmineSite}/issues/${redmineId}`,
              redmineStatus: issue?.status ?? "",
              redmineTracker: issue?.tracker ?? "",
              redmineDue: issue?.dueDate ?? "",
              ...(task.details ? { details: clone(task.details) } : {}),
            });
            if (task.start) {
              stopAll(now);
              snapshot.sessions.unshift({
                id: nextSessionId(),
                taskId: id,
                startedAt: now,
                endedAt: null,
              });
            }
            emit("data-changed", ownChange);
            return clone(snapshot);
          }
          const summary = (k: string) =>
            init.jira?.issues.find((i: any) => i.key === k)?.summary ?? null;
          const template = task.template ? findTemplate(task.template) : null;
          if (task.template && !template)
            throw `Nie ma szablonu „${task.template}”. Dodaj go w Ustawieniach → Szablony.`;
          let title = String(task.title).trim();
          if (template) {
            const wants = /\{tytu[łl]\}/.test(template.title);
            let value = title;
            if (!title && wants && key) {
              value = summary(key) ?? "";
              if (!value) throw `Nie znam tytułu ${key}.`;
            }
            title = template.title
              .replaceAll("{klucz}", key ?? "")
              .replace(/\{tytu[łl]\}/g, value);
            if (template.title.trim() && !wants && value)
              title = `${title} ${value}`;
            if (!template.title.trim()) title = value;
            title = title
              .split(/\s+/)
              .filter(Boolean)
              .join(" ")
              .replace(/ ([:,.])/g, "$1")
              .replace(/^[:,\-–— ]+|[:,\-–— ]+$/g, "");
          }
          if (!title && key) title = summary(key) ?? "";
          if (!title) throw "Tytuł musi mieć od 1 do 240 znaków.";
          let details = task.details ? clone(task.details) : null;
          if (template) {
            const d = details ?? {
              priority: 0,
              plannedDay: "",
              dueDay: "",
              project: "",
              waiting: "",
              notes: {},
              estimateMinutes: 0,
              checklist: [],
              repeatDays: [],
              repeatFrom: "",
              order: 0,
              plannedStart: "",
            };
            d.estimateMinutes ||= template.estimateMinutes;
            d.project ||= template.project;
            d.priority ||= template.priority;
            if (!d.checklist.length)
              d.checklist = template.steps.map((text: string) => ({
                text,
                done: false,
              }));
            details = d;
          }
          const id = nextTaskId();
          snapshot.tasks.unshift({
            id,
            title,
            done: false,
            createdAt: now,
            ...(key
              ? { jiraKey: key, jiraUrl: `${init.site}/browse/${key}` }
              : {}),
            ...(details ? { details } : {}),
          });
          if (task.start) {
            stopAll(now);
            snapshot.sessions.unshift({
              id: nextSessionId(),
              taskId: id,
              startedAt: now,
              endedAt: null,
            });
          }
          emit("data-changed", ownChange);
          return clone(snapshot);
        }
        // Plugin updater: `check()` zwraca metadane z identyfikatorem zasobu
        // (albo null, gdy nie ma nowszej wersji), `download()` zgłasza postęp
        // kanałem (`Channel` z @tauri-apps/api: `__CHANNEL__:<id>`) i zwraca
        // identyfikator pobranych bajtów, `prepare_update` (main.rs) kończy
        // pomiar, a `install()` w Windows kończy proces po starcie instalatora;
        // tutaj kończy się na wywołaniu `plugin:process|restart` z `relaunch()`.
        case "plugin:updater|check": {
          if (init.update === "offline")
            throw "Could not fetch a valid release JSON from the remote";
          if (!init.update) return null;
          return {
            rid: 1,
            currentVersion: "test",
            version: init.update.version,
            date: init.update.date ?? null,
            body: init.update.notes ?? null,
            rawJson: {},
          };
        }
        case "plugin:updater|download": {
          if (args.rid !== 1) throw "Nieznany zasób aktualizacji w teście.";
          const channel = args.onEvent;
          const id =
            typeof channel === "string"
              ? Number(channel.split(":")[1])
              : Number(channel?.id);
          const deliver = (index: number, message: unknown) =>
            callbacks.get(id)?.({ index, message });
          const wait = (ms: number) =>
            new Promise((resolve) => setTimeout(resolve, ms));
          deliver(0, { event: "Started", data: { contentLength: 4_194_304 } });
          await wait(60);
          deliver(1, { event: "Progress", data: { chunkLength: 2_097_152 } });
          await wait(60);
          deliver(2, { event: "Progress", data: { chunkLength: 2_097_152 } });
          await wait(60);
          deliver(3, { event: "Finished" });
          return 2;
        }
        // Jak `workflows::runtime::update_blocker`: przebieg z procesem w toku
        // (queued, running, cancelling) blokuje; sam waitingForApproval nie.
        case "prepare_update": {
          const blocker = workflowActiveWork();
          if (blocker) throw `Aktualizacja wstrzymana. ${blocker}`;
          stopAll(now);
          emit("data-changed");
          return null;
        }
        case "plugin:updater|install":
          if (args.updateRid !== 1 || args.bytesRid !== 2)
            throw "Instalacja bez pobranej aktualizacji w teście.";
          return null;
        case "plugin:process|restart":
          return null;
        // Jak `backup::inspect_backup`: ostrzeżenie z `restore_blocker` z góry.
        case "inspect_backup":
          return init.backup
            ? { ...clone(init.backup.preview), workflowBlocker: workflowRestoreBlocker() }
            : null;
        case "export_backup":
          return init.backupExport ? clone(init.backupExport) : null;
        case "restore_backup": {
          if (!init.backup) throw "Najpierw wybierz i sprawdź kopię.";
          // Jak `backup::restore_locked`: odmowa przy każdym nieskończonym przebiegu.
          const blocker = workflowRestoreBlocker();
          if (blocker) throw `Nie można teraz przywrócić kopii. ${blocker}`;
          snapshot = clone(init.backup.snapshot);
          snapshot.trash ??= [];
          snapshot.trashSessions ??= [];
          emit("data-changed");
          return clone(snapshot);
        }
      }
      if (cmd.startsWith("plugin:")) return null;
      throw "Nieobsługiwana komenda w teście: " + cmd;
    },
  };
}
