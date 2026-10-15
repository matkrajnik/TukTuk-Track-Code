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
export const PREVIEW_KEY = "todo-track-preview-v1";
export const SETTINGS_KEY = "todo-track-preview-settings-v1";
export const AGENTS_KEY = "todo-track-preview-agents-v1";

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
      },
      {
        key: "WMS-31",
        summary: "Obsługa przyjęcia towaru",
        status: "Do zrobienia",
        boards: ["WMS", "Foundation"],
      },
      {
        key: "WMS-7",
        summary: "Porzucony prototyp",
        status: "Do zrobienia",
        boards: ["WMS"],
      },
    ],
  };
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
      [PREVIEW_KEY, SETTINGS_KEY, AGENTS_KEY],
      [
        JSON.stringify(data),
        options.settings ? JSON.stringify(options.settings) : null,
        options.agents ? JSON.stringify(options.agents) : null,
      ],
    ] as const,
  );
  await page.reload();
  return data;
}

/** Dane podglądu zapisane w localStorage. */
export const stored = (page: Page) =>
  page.evaluate((k) => JSON.parse(localStorage.getItem(k)!), PREVIEW_KEY);
