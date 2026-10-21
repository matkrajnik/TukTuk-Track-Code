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
  /** Szablony zadań (`task_templates`) na starcie. */
  templates?: unknown[];
  /** Propozycje przełączenia (`agent_suggestions`) na starcie. */
  suggestions?: unknown[];
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
}

export async function openDesktop(page: Page, options: TauriOptions = {}) {
  await page.clock.setFixedTime(new Date(options.now ?? NOW));
  await page.addInitScript(install, {
    data: options.data ?? seed(),
    settings: { ...defaultSettings, ...options.settings },
    backup: options.backup ?? null,
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
    | "show",
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
  w.__tauriMock = {
    emit,
    show,
    suggest,
    idleTick,
    jiraCheck,
    redmineCheck,
    snapshot: () => clone(snapshot),
    calls: () => clone(calls),
  };
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
        case "agents_status":
          return [];
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
          else if (args.action === "start" && task) {
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
          } else throw "Nieobsługiwana akcja w teście: " + args.action;
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
        case "prepare_update":
          stopAll(now);
          emit("data-changed");
          return null;
        case "plugin:updater|install":
          if (args.updateRid !== 1 || args.bytesRid !== 2)
            throw "Instalacja bez pobranej aktualizacji w teście.";
          return null;
        case "plugin:process|restart":
          return null;
        case "inspect_backup":
          return init.backup ? clone(init.backup.preview) : null;
        case "restore_backup":
          if (!init.backup) throw "Najpierw wybierz i sprawdź kopię.";
          snapshot = clone(init.backup.snapshot);
          snapshot.trash ??= [];
          snapshot.trashSessions ??= [];
          emit("data-changed");
          return clone(snapshot);
      }
      if (cmd.startsWith("plugin:")) return null;
      throw "Nieobsługiwana komenda w teście: " + cmd;
    },
  };
}
