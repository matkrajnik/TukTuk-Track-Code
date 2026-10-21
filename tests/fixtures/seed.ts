import type { Page } from "@playwright/test";

/**
 * Wspólne dane testowe: tydzień pracy Pon 12.10 – Pt 16.10.2026, „dziś” to
 * czwartek 15.10.2026, 12:00 czasu polskiego (CEST, +02:00). Testy używające
 * fixture ustawiają `test.use({ timezoneId: "Europe/Warsaw" })`, żeby dni
 * i godziny w interfejsie nie zależały od strefy komputera.
 *
 * Zawartość:
 * - projekty WMS, Foundation i Zespół, szacunki, notatki dnia i lista kroków;
 * - zadanie powiązane z Jirą (WMS-12) ze statusem;
 * - zadanie cykliczne (Pon–Pt) z zapamiętanym stanem cyklu do dziś, więc podgląd
 *   nie dopisuje nowych wystąpień przy odczycie;
 * - zadanie ukończone, zarchiwizowane i jedno w koszu (z czasem i kluczem WMS-7);
 * - przedziały czasu Pon–Czw bez nakładania się, zamknięte poniedziałek i wtorek.
 *
 * Każde wywołanie `seed()` zwraca nową kopię, więc testy mogą ją zmieniać.
 */
export const TODAY = "2026-10-15";
export const NOW = "2026-10-15T12:00:00+02:00";
export const WEEK = [
  "2026-10-12",
  "2026-10-13",
  "2026-10-14",
  "2026-10-15",
  "2026-10-16",
  "2026-10-17",
  "2026-10-18",
];
export const JIRA_SITE = "https://asistonwms.atlassian.net";
/** Przykładowy adres Redmine (jak `previewRedmineSite`); żaden test się z nim nie łączy. */
export const REDMINE_SITE = "https://redmine.example.pl";
/** Stan symulowanego Redmine w podglądzie (`previewRedmineKey`). */
export const REDMINE_KEY = "todo-track-preview-redmine-v1";
export const PREVIEW_KEY = "todo-track-preview-v1";
export const SETTINGS_KEY = "todo-track-preview-settings-v1";
export const AGENTS_KEY = "todo-track-preview-agents-v1";
/** Zdarzenia hooków agentów w podglądzie (B8): `{ events, codexTrust?, claude?, codex? }`. */
export const AGENT_HOOKS_KEY = "todo-track-preview-agent-hooks-v1";
export const ORCA_KEY = "todo-track-preview-orca-v1";

/** Chwila w czasie polskim (CEST) jako znacznik ms, np. `at("2026-10-15", "11:00")`. */
export const at = (day: string, time: string) =>
  Date.parse(`${day}T${time}:00+02:00`);

/** Identyfikatory zadań z fixture, żeby testy nie wpisywały liczb na sztywno. */
export const ids = {
  analysis: 1,
  labels: 2,
  review: 3,
  standup: 4,
  docs: 5,
  archived: 6,
  trashed: 7,
} as const;

export interface SeedOptions {
  /**
   * Trwający pomiar: zadanie i godzina startu dziś (np. `{ task: 1, from: "11:00" }`).
   * Domyślnie nic nie jest mierzone.
   */
  active?: { task: number; from: string };
}

const details = (extra: Record<string, unknown>) => ({
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
  ...extra,
});

export function seed(options: SeedOptions = {}) {
  const created = at("2026-10-09", "09:00");
  const tasks = [
    {
      id: ids.analysis,
      title: "Analiza importu",
      done: false,
      createdAt: created,
      jiraKey: "WMS-12",
      jiraUrl: `${JIRA_SITE}/browse/WMS-12`,
      jiraStatus: "W toku",
      details: details({
        priority: 1,
        project: "WMS",
        plannedDay: TODAY,
        dueDay: "2026-10-16",
        estimateMinutes: 600,
        notes: {
          "2026-10-13": "Walidacja CSV",
          "2026-10-14": "Mapowanie kolumn",
        },
        checklist: [
          { text: "Przykładowe pliki", done: true },
          { text: "Raport błędów", done: false },
        ],
      }),
    },
    {
      id: ids.labels,
      title: "Wdrożenie etykiet",
      done: false,
      createdAt: created,
      details: details({
        project: "WMS",
        plannedDay: "2026-10-14",
        estimateMinutes: 480,
      }),
    },
    {
      id: ids.review,
      title: "Code review",
      done: false,
      createdAt: created,
      details: details({ project: "Foundation", estimateMinutes: 60 }),
    },
    {
      id: ids.standup,
      title: "Spotkanie zespołu",
      done: false,
      createdAt: created,
      details: details({
        project: "Zespół",
        plannedDay: "2026-10-12",
        estimateMinutes: 30,
        repeatDays: [1, 2, 3, 4, 5],
        repeatFrom: "2026-10-12",
      }),
    },
    {
      id: ids.docs,
      title: "Dokumentacja API",
      done: true,
      createdAt: created,
      details: details({
        project: "Foundation",
        plannedDay: "2026-10-12",
        estimateMinutes: 240,
        notes: { "2026-10-12": "Opis endpointów" },
      }),
    },
    {
      id: ids.archived,
      title: "Stary raport",
      done: false,
      createdAt: created,
      archived: true,
      details: details({ project: "WMS" }),
    },
  ];
  const trash = [
    {
      id: ids.trashed,
      title: "Porzucony prototyp",
      done: false,
      createdAt: created,
      jiraKey: "WMS-7",
      jiraUrl: `${JIRA_SITE}/browse/WMS-7`,
      jiraStatus: "Do zrobienia",
      details: details({ project: "WMS" }),
    },
  ];
  const ranges: [number, string, string, string][] = [
    [ids.docs, "2026-10-12", "08:00", "12:00"],
    [ids.labels, "2026-10-12", "12:30", "16:00"],
    [ids.analysis, "2026-10-13", "08:30", "11:30"],
    [ids.labels, "2026-10-13", "12:00", "16:00"],
    [ids.standup, "2026-10-14", "09:00", "09:30"],
    [ids.analysis, "2026-10-14", "09:30", "15:30"],
    [ids.standup, "2026-10-15", "08:00", "08:30"],
    [ids.analysis, "2026-10-15", "08:30", "10:30"],
  ];
  const sessions: {
    id: number;
    taskId: number;
    startedAt: number;
    endedAt: number | null;
  }[] = ranges.map(([taskId, day, from, to], i) => ({
    id: i + 1,
    taskId,
    startedAt: at(day, from),
    endedAt: at(day, to),
  }));
  if (options.active)
    sessions.push({
      id: sessions.length + 2,
      taskId: options.active.task,
      startedAt: at(TODAY, options.active.from),
      endedAt: null,
    });
  // Najnowsze przedziały pierwsze, jak w snapshot z Rust.
  sessions.sort((a, b) => b.startedAt - a.startedAt);
  return {
    tasks,
    sessions,
    trash,
    trashSessions: [
      {
        id: ranges.length + 1,
        taskId: ids.trashed,
        startedAt: at("2026-10-13", "16:00"),
        endedAt: at("2026-10-13", "17:00"),
      },
    ],
    closedDays: ["2026-10-12", "2026-10-13"],
    recurrenceState: { [ids.standup]: TODAY } as Record<string, string>,
  };
}
export type Seed = ReturnType<typeof seed>;

export const TS_KEY = "todo-track-preview-ts-v1";

/**
 * Dane do wysyłki Raportu TS (PLAN_1.8.md, T5) na bazie `seed()` (domyślny
 * `seed()` zostaje bez zmian, żeby nie ruszać innych testów i zrzutów):
 * - „Analiza importu” (notatka dnia „Import CSV”, 08:30–10:30) i „Wdrożenie
 *   etykiet” (10:30–11:00) mają jawne `tsTaskId` na to samo zadanie TS 1101
 *   „Rozwój” — scalają się w jeden wpis 2,50 h;
 * - „Spotkanie zespołu” (projekt „Zespół”, 08:00–08:30) nie ma jawnego wyboru,
 *   zadanie TS bierze z mapowania projektu (`tsFixture().projectMap`);
 * - „Code review” (projekt „Foundation”) nie ma niczego — ostrzeżenie; domyślnie
 *   trwający pomiar od 11:00 (do „teraz” 12:00 to 1,00 h).
 */
export function tsSeed(options: SeedOptions = { active: { task: ids.review, from: "11:00" } }) {
  const data = seed(options);
  const task = (id: number) => data.tasks.find((x) => x.id === id)!;
  Object.assign(task(ids.analysis).details, {
    tsTaskId: "1101",
    tsTaskName: "Rozwój",
    notes: { ...task(ids.analysis).details.notes, [TODAY]: "Import CSV" },
  });
  Object.assign(task(ids.labels).details, {
    tsTaskId: "1101",
    tsTaskName: "Rozwój",
  });
  data.sessions.push({
    id: 100,
    taskId: ids.labels,
    startedAt: at(TODAY, "10:30"),
    endedAt: at(TODAY, "11:00"),
  });
  return data;
}

/** Wpis już zapisany w symulowanym TS na dzień `TODAY` (zadanie „Administracja”). */
export const TS_EXISTING = {
  id: "2202",
  hours: "1,00",
  overtime: "0",
  description: "Rozliczenie delegacji",
};

/**
 * Symulowany TS podglądu (`openPreview({ ts })`, klucz `TS_KEY`): przykładowe
 * projekty „WMS” i „Wewnętrzne” z `store.ts`, domyślne zadanie TS projektu
 * „Zespół” → „Spotkania” i jeden wpis już zapisany w TS na `TODAY`.
 * `connected: false` — konto rozłączone (login zostaje, jak po „Rozłącz”).
 */
export function tsFixture(options: { connected?: boolean } = {}) {
  return {
    connected: options.connected ?? true,
    login: "jan",
    projectMap: [{ project: "Zespół", tsTaskId: "2201", tsTaskName: "Spotkania" }],
    filled: { [TODAY]: [{ ...TS_EXISTING }] },
  };
}

/**
 * Połączona Jira dla zastępczego IPC (`openDesktop({ jira })`): tablice
 * i zgłoszenia zgodne z zadaniami fixture. WMS-12 jest powiązane z „Analizą
 * importu”, WMS-7 z zadaniem w koszu, a WMS-31 nie ma jeszcze zadania.
 */
export function jiraFixture() {
  return {
    boards: [
      { id: 1, name: "WMS" },
      { id: 2, name: "Foundation" },
    ],
    issues: [
      {
        key: "WMS-12",
        summary: "Analiza importu",
        status: "W toku",
        boards: ["WMS"],
        issueType: "Zadanie",
        priority: "High",
        assignee: { accountId: "a-me", displayName: "Użytkownik testowy" },
      },
      {
        key: "WMS-31",
        summary: "Obsługa przyjęcia towaru",
        status: "Do zrobienia",
        boards: ["WMS", "Foundation"],
        issueType: "Historia",
        priority: "Medium",
        assignee: { accountId: "a-anna", displayName: "Anna Nowak" },
      },
      {
        key: "WMS-7",
        summary: "Porzucony prototyp",
        status: "Do zrobienia",
        boards: ["WMS"],
        issueType: "Błąd",
        priority: "Low",
        assignee: null,
      },
    ],
  };
}


/**
 * Dane fixture plus dwa zadania powiązane ze zgłoszeniami symulowanego
 * Redmine (PLAN_1.9.md, R6; domyślny `seed()` zostaje bez zmian, żeby nie
 * ruszać innych testów i zrzutów):
 * - „Import CSV zamówień” (id 20) z `#1234` w statusie „New” (status
 *   początkowy, więc start pracy pyta o przesunięcie), tracker „Funkcja”,
 *   termin 30.10, projekt „Sklep”, zaplanowane na dziś;
 * - „Błąd zapisu etykiet” (id 21) z `#1240` w „Feedback” (status „Czekam na”),
 *   tracker „Błąd”, bez terminu, zaplanowane na dziś.
 */
export function redmineSeed(options: SeedOptions = {}) {
  const data = seed(options);
  const created = at("2026-10-14", "09:00");
  const plain = (extra: Record<string, unknown>) =>
    details({ plannedDay: TODAY, ...extra });
  (data.tasks as any[]).unshift(
    {
      id: redmineIds.importCsv,
      title: "Import CSV zamówień",
      done: false,
      createdAt: created,
      redmineId: 1234,
      redmineUrl: `${REDMINE_SITE}/issues/1234`,
      redmineStatus: "New",
      redmineTracker: "Funkcja",
      redmineDue: "2026-10-30",
      details: plain({ project: "Sklep" }),
    },
    {
      id: redmineIds.labels,
      title: "Błąd zapisu etykiet",
      done: false,
      createdAt: created,
      redmineId: 1240,
      redmineUrl: `${REDMINE_SITE}/issues/1240`,
      redmineStatus: "Feedback",
      redmineTracker: "Błąd",
      redmineDue: "",
      details: plain({}),
    },
  );
  return data;
}
/** Identyfikatory zadań Redmine z `redmineSeed()`. */
export const redmineIds = { importCsv: 20, labels: 21 } as const;

/**
 * Połączony Redmine dla zastępczego IPC (`openDesktop({ redmine })`): te same
 * dane co symulowany Redmine podglądu (`previewRedmine*` w `store.ts`) —
 * statusy New / In Progress / Feedback / Resolved / Closed (dwa ostatnie
 * zamknięte), projekty Magazyn WMS (śledzony) z podprojektem „WMS – integracje”
 * i Portal klienta, siedem zgłoszeń: w Magazynie WMS #1234 (New, z terminem),
 * #1240 (Feedback), #1250 (In Progress), #1251 (New, Anny Nowak) i #1245
 * (Closed); #1260 w podprojekcie; moje #1270 w nieśledzonym Portalu klienta.
 */
export function redmineFixture() {
  const me = { id: 12, login: "jkowalski", name: "Jan Kowalski" };
  const issue = (
    id: number,
    subject: string,
    projectId: number,
    status: string,
    tracker: string,
    priority: string,
    extra: Record<string, unknown> = {},
  ) => ({
    id,
    subject,
    projectId,
    status,
    tracker,
    priority,
    assignedToId: me.id as number | null,
    assignedTo: me.name,
    dueDate: "",
    version: "",
    category: "",
    ...extra,
  });
  return {
    user: me,
    statuses: [
      { id: 1, name: "New", isClosed: false },
      { id: 2, name: "In Progress", isClosed: false },
      { id: 3, name: "Feedback", isClosed: false },
      { id: 4, name: "Resolved", isClosed: true },
      { id: 5, name: "Closed", isClosed: true },
    ],
    projects: [
      { id: 7, name: "Magazyn WMS", identifier: "wms", parentId: null as number | null },
      { id: 8, name: "WMS – integracje", identifier: "wms-integracje", parentId: 7 },
      { id: 9, name: "Portal klienta", identifier: "portal", parentId: null },
    ],
    /** Śledzone projekty (`Config.projects`). */
    tracked: [7],
    issues: [
      issue(1234, "Import CSV zamówień", 7, "New", "Funkcja", "High", {
        dueDate: "2026-10-30",
        version: "2.4",
      }),
      issue(1240, "Błąd zapisu etykiet", 7, "Feedback", "Błąd", "Normal"),
      issue(1250, "Aktualizacja dokumentacji API", 7, "In Progress", "Wsparcie", "Low"),
      issue(1251, "Raport stanów magazynowych", 7, "New", "Funkcja", "Normal", {
        assignedToId: 31,
        assignedTo: "Anna Nowak",
      }),
      issue(1245, "Eksport faktur do ERP", 7, "Closed", "Błąd", "Normal"),
      issue(1260, "Mapowanie kodów EDI", 8, "In Progress", "Funkcja", "Normal"),
      issue(1270, "Formularz reklamacji", 9, "New", "Błąd", "Urgent"),
    ],
  };
}
export type RedmineFixture = ReturnType<typeof redmineFixture>;

export const CALENDAR_KEY = "todo-track-preview-calendar-v1";
/** Identyfikator aplikacji w Entra ID używany w testach kalendarza. */
export const CALENDAR_CLIENT_ID = "0a1b2c3d-4e5f-4789-abcd-ef0123456789";

/**
 * Przykładowe spotkania z kalendarza Microsoft (jak z `calendar_events`): dziś
 * daily, przegląd z odpowiedzią „wstępnie”, anulowane i odrzucone (nie zajmują
 * czasu), jutro całodniowe „poza biurem” od lokalnej północy i daily w
 * poniedziałek tego tygodnia.
 */
export function calendarEvents() {
  const event = (
    id: string,
    day: string,
    from: string,
    to: string,
    subject: string,
    extra: Record<string, unknown> = {},
  ) => ({
    id,
    start: at(day, from),
    end: at(day, to),
    subject,
    showAs: "busy",
    allDay: false,
    cancelled: false,
    response: "accepted",
    ...extra,
  });
  return [
    event("m-daily-mon", "2026-10-12", "09:15", "09:30", "Daily WMS"),
    event("m-daily", TODAY, "09:15", "09:30", "Daily WMS"),
    event("m-review", TODAY, "13:00", "14:00", "Przegląd sprintu", {
      showAs: "tentative",
      response: "tentativelyAccepted",
    }),
    event("m-cancelled", TODAY, "15:00", "15:30", "Odwołane demo", {
      cancelled: true,
    }),
    event("m-declined", TODAY, "16:00", "17:00", "Warsztat z klientem", {
      response: "declined",
    }),
    {
      ...event("m-oof", "2026-10-16", "00:00", "00:00", "Urlop", {
        showAs: "oof",
        allDay: true,
      }),
      end: at("2026-10-17", "00:00"),
    },
  ];
}

/**
 * Kalendarz podglądu (`openPreview({ calendar })`): `connected` ma zalogowane
 * konto i pobrane spotkania, `ready` tylko zapisany client ID; w obu „chmura”
 * (`sample`) ma spotkania z `calendarEvents()`, które pobierze „Połącz” i „Odśwież”.
 */
export function calendarFixture(state: "connected" | "ready" = "connected") {
  const sample = calendarEvents();
  return {
    clientId: CALENDAR_CLIENT_ID,
    tenant: "organizations",
    email: state === "connected" ? "jan.kowalski@firma.example" : "",
    lastSync: state === "connected" ? at(TODAY, "11:50") : null,
    lastError: "",
    events: state === "connected" ? sample : [],
    sample,
  };
}

export const MEETINGS_KEY = "todo-track-preview-meetings-v1";
/**
 * Ręczne spotkania podglądu (B12, jak z `manual_meetings`): dziś telefon
 * 11:00–11:30 z zadaniem „Code review” (po pomiarze 8:00–10:30, przed teraz)
 * i spotkanie u klienta 15:00–16:00 bez zadania.
 */
export function manualMeetings() {
  return [
    {
      id: "manual:1",
      start: at(TODAY, "11:00"),
      end: at(TODAY, "11:30"),
      subject: "Telefon z magazynem",
      showAs: "busy",
      allDay: false,
      taskId: ids.review,
    },
    {
      id: "manual:2",
      start: at(TODAY, "15:00"),
      end: at(TODAY, "16:00"),
      subject: "Spotkanie u klienta",
      showAs: "busy",
      allDay: false,
      taskId: null,
    },
  ];
}

export const AGENT_SESSIONS_KEY = "todo-track-preview-agent-sessions-v1";

/**
 * Okresy pracy agentów AI (jak z `agent_time`, tabela `agent_sessions`):
 * dziś Claude Code nad „Analizą importu” 10:00–11:30 (30 min równolegle z Twoim
 * pomiarem 08:30–10:30) i Codex bez zadania 11:00–11:40; w środę Claude nad
 * „Wdrożeniem etykiet” 13:00–14:00 (bez Twojego pomiaru tego zadania), we wtorek
 * Codex nad „Code review” 14:00–14:45 (zadanie bez Twojego czasu).
 */
export function agentSessions() {
  const period = (
    id: number,
    agent: string,
    folder: string,
    taskId: number | null,
    day: string,
    from: string,
    to: string,
  ) => ({ id, agent, folder, taskId, start: at(day, from), end: at(day, to) });
  return [
    period(1, "codex", "C:\\Projekty\\api", ids.review, "2026-10-13", "14:00", "14:45"),
    period(2, "claude", "C:\\Projekty\\wms", ids.labels, "2026-10-14", "13:00", "14:00"),
    period(3, "claude", "C:\\Projekty\\wms", ids.analysis, TODAY, "10:00", "11:30"),
    period(4, "codex", "C:\\Projekty\\narzedzia", null, TODAY, "11:00", "11:40"),
  ];
}

export interface PreviewOptions extends SeedOptions {
  /** Stały czas strony; domyślnie `NOW`. */
  now?: string | number;
  /** Dane zamiast `seed(options)`. */
  data?: unknown;
  /** Ustawienia podglądu (klucz `todo-track-preview-settings-v1`). */
  settings?: Record<string, unknown>;
  /** Pracujący agenci AI w podglądzie. */
  agents?: unknown[];
  /** Zdarzenia hooków agentów (B8), np. `{ events: [...] }`; zasilają kafelek Agenci AI. */
  agentHooks?: Record<string, unknown>;
  /** Symulowana Orka (B9), np. `{ running: false }`; domyślnie działa, z dwoma repozytoriami. */
  orca?: Record<string, unknown>;
  /** Kalendarz Microsoft w podglądzie, np. `calendarFixture()`. */
  calendar?: unknown;
  /** Ręczne spotkania w podglądzie (B12), np. `manualMeetings()`. */
  meetings?: unknown[];
  /** Okresy pracy agentów w podglądzie, np. `agentSessions()`. */
  agentSessions?: unknown[];
  /** Symulowany Ideo TS w podglądzie, np. `tsFixture()`. */
  ts?: unknown;
  /**
   * Stan symulowanego Redmine (`REDMINE_KEY`), np. `{ connected: false }`;
   * domyślnie brak, czyli połączone z przykładowymi zgłoszeniami.
   */
  redmine?: Record<string, unknown>;
}

/**
 * Tryb podglądu (bez Rusta): stały zegar, dane fixture w localStorage i przeładowanie,
 * tak samo jak w istniejących testach. Zwraca zapisane dane.
 */
export async function openPreview(page: Page, options: PreviewOptions = {}) {
  const data = options.data ?? seed(options);
  await page.clock.setFixedTime(new Date(options.now ?? NOW));
  await page.goto("/");
  await page.evaluate(
    ([keys, values]) => {
      localStorage.clear();
      keys.forEach((k, i) => {
        if (values[i] !== null) localStorage.setItem(k, values[i]!);
      });
    },
    [
      [PREVIEW_KEY, SETTINGS_KEY, AGENTS_KEY, CALENDAR_KEY, AGENT_SESSIONS_KEY, TS_KEY, REDMINE_KEY, MEETINGS_KEY, AGENT_HOOKS_KEY, ORCA_KEY],
      [
        JSON.stringify(data),
        options.settings ? JSON.stringify(options.settings) : null,
        options.agents ? JSON.stringify(options.agents) : null,
        options.calendar ? JSON.stringify(options.calendar) : null,
        options.agentSessions ? JSON.stringify(options.agentSessions) : null,
        options.ts ? JSON.stringify(options.ts) : null,
        options.redmine ? JSON.stringify(options.redmine) : null,
        options.meetings ? JSON.stringify(options.meetings) : null,
        options.agentHooks ? JSON.stringify(options.agentHooks) : null,
        options.orca ? JSON.stringify(options.orca) : null,
      ],
    ] as const,
  );
  await page.reload();
  return data;
}

/** Dane podglądu zapisane w localStorage. */
export const stored = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), PREVIEW_KEY);
