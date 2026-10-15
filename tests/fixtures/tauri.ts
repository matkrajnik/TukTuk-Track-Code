import type { Page } from "@playwright/test";
import { defaultSettings } from "../../src/settings";
import { JIRA_SITE, NOW, seed } from "./seed";

/**
 * Zastępcze IPC Tauri dla testów funkcji, które w trybie podglądu są wyłączone
 * (baner bezczynności, kopie, obserwacja Jiry). Frontend działa jak w aplikacji
 * (`isTauri()` = true), a komendy obsługuje ten skrypt na danych z fixture.
 *
 * Logika z Rust, od której zależą testy, jest tu przeniesiona jeden do jednego
 * i opisana odwołaniem do funkcji źródłowej:
 * - `idle::Clock::observe` i `idle_resolve` (`idleTick`, `idle_resolve`),
 * - `jira::sync_assigned` z pierwszym sprawdzeniem tylko zapamiętującym (`jiraCheck`).
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
    }[];
  };
}

export async function openDesktop(page: Page, options: TauriOptions = {}) {
  await page.clock.setFixedTime(new Date(options.now ?? NOW));
  await page.addInitScript(install, {
    data: options.data ?? seed(),
    settings: { ...defaultSettings, ...options.settings },
    backup: options.backup ?? null,
    jira: options.jira ?? null,
    templates: options.templates ?? [],
    site: JIRA_SITE,
  });
  await page.goto("/");
}

/** Wywołuje funkcję zastępczego backendu w oknie strony. */
export function backend<T = unknown>(
  page: Page,
  name: "idleTick" | "jiraCheck" | "snapshot" | "calls",
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
  site: string;
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

  w.__tauriMock = {
    emit,
    idleTick,
    jiraCheck,
    snapshot: () => clone(snapshot),
    calls: () => clone(calls),
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  w.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main" },
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
        case "agent_suggestions":
          return [];
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
        case "jira_boards":
          return { values: clone(init.jira?.boards ?? []), nextStart: null };
        case "jira_issues":
          return {
            issues: (init.jira?.issues ?? []).map((i: any) => ({
              ...i,
              url: `${init.site}/browse/${i.key}`,
            })),
            warnings: [],
          };
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
          emit("data-changed");
          return clone(snapshot);
        }
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
