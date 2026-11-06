/**
 * API aplikacji dla widoków: każda funkcja to komenda Tauri (`desktop`) albo
 * jej odpowiednik w trybie podglądu przeglądarkowego.
 *
 * - `src/api/*.ts`: typy, stałe i reguły wspólne dla aplikacji i podglądu
 *   (`task`, `jira`, `redmine`, `timesheet`, `calendar`); tu eksportowane dalej.
 * - `src/preview/*.ts`: symulatory podglądu w localStorage (`snapshot`, `tasks`,
 *   `templates`, `agents`, `calendar`, `jira`, `jira-state`, `redmine`,
 *   `timesheet`, wspólne `storage`). Nie importują tego modułu.
 * - `calendarapi.ts`: komendy kalendarza i ręcznych spotkań, eksportowane tu dalej.
 * - Ten moduł: cienka warstwa `desktop ? invoke : podgląd`, stan współdzielony
 *   przez widoki (`templates`, `jiraConnected`, `redmineConnected`) i nasłuch
 *   zdarzeń backendu.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ref } from "vue";
import type { AgentTime } from "./agenttime";
import { dayKey } from "./time";
import {
  templateEvent,
  widgetEvent,
  type AgentBinding,
  type AgentHooksStatus,
  type ExternalApiInfo,
  type Autostart,
  type BrowserStatus,
  type ExportFormat,
  type ExportResult,
  type GapMode,
  type DayNote,
  type NewTask,
  type PlanEntry,
  type Snapshot,
  type TaskDetails,
  type TaskTemplate,
  type TemplateBody,
  type ToggleResult,
  type WaitingChange,
  type WidgetStatus,
} from "./api/task";
import {
  jiraMoveEvent,
  jiraWatchEvent,
  type JiraWatchStatus,
  type JiraAccount,
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
} from "./api/jira";
import {
  notifyTs,
  tsChangedEvent,
  type TsEntry,
  type TsFilled,
  type TsMode,
  type TsProject,
  type TsProjectMapping,
  type TsSendResult,
  type TsSent,
  type TsSettings,
} from "./api/timesheet";
import {
  notifyRedmine,
  redmineChangedEvent,
  type RedmineAccount,
  type RedmineIssue,
  type RedmineIssues,
  type RedmineProject,
  type RedmineStatus,
} from "./api/redmine";
import type { ChatRequest } from "./api/aichat";
import type { AiCall, AiStatus } from "./api/ai";
import type {
  OrcaCreated,
  OrcaCreateRequest,
  OrcaRepo,
  OrcaStatus,
  OrcaWorktree,
} from "./api/orca";
import * as snapshotSim from "./preview/snapshot";
import { snapshotKey } from "./preview/snapshot";
import * as tasksSim from "./preview/tasks";
import * as templatesSim from "./preview/templates";
import * as agentsSim from "./preview/agents";
import * as hooksSim from "./preview/hooks";
import * as orcaSim from "./preview/orca";
import * as aiSim from "./preview/ai";
import * as jiraSim from "./preview/jira";
import { previewJiraKey } from "./preview/jira-state";
import * as timesheetSim from "./preview/timesheet";
import { previewTsKey } from "./preview/timesheet";
import * as redmineSim from "./preview/redmine";
import { previewRedmineKey, previewRedmineState } from "./preview/redmine";

export * from "./api/task";
export * from "./api/calendar";
export * from "./calendarapi";
export * from "./api/jira";
export * from "./api/timesheet";
export * from "./api/redmine";
export * from "./api/orca";
export * from "./api/ai";
export { previewDataEvent } from "./preview/storage";
export {
  previewWidgetSettingsChanged,
  weekNotesKey,
  browserKey,
} from "./preview/tasks";
export {
  previewAgentsEvent,
  previewBindings,
  agentSessionsKey,
} from "./preview/agents";
export { agentHooksKey } from "./preview/hooks";
export { orcaKey } from "./preview/orca";
export { aiKey } from "./preview/ai";
export { calendarKey, previewCalendarEmail } from "./preview/calendar";
export {
  previewJiraKey,
  previewWorkflow,
  previewStatusCategory,
  previewJiraBoardKey,
  previewJiraOpenedKey,
  previewFieldCatalog,
  previewJiraMe,
} from "./preview/jira-state";
export { previewTsKey, previewTsTasks } from "./preview/timesheet";
export {
  previewRedmineSite,
  previewRedmineKey,
  previewRedmineMe,
  previewRedmineStatuses,
  previewRedmineProjects,
  previewRedmineIssues,
  previewRedmineOpenedKey,
} from "./preview/redmine";
export type { PreviewRedmineIssue } from "./preview/redmine";

export const desktop = isTauri();
/**
 * Czy Jira jest połączona: tylko wtedy szybkie dodawanie rozpoznaje klucz
 * zgłoszenia. W podglądzie Jira udaje połączoną (dane przykładowe mają zgłoszenia,
 * a `quickAdd` wiąże klucz z adresem domyślnym).
 */
export const jiraConnected = ref(!desktop);
/** Odświeża `jiraConnected` z `jira_settings` (w podglądzie nic nie robi). */
export async function refreshJiraConnected(): Promise<void> {
  if (!desktop) return;
  try {
    jiraConnected.value = (
      await invoke<{ connected: boolean }>("jira_settings")
    ).connected;
  } catch {
    jiraConnected.value = false;
  }
}
/**
 * Czy Redmine jest połączone: tylko wtedy szybkie dodawanie rozpoznaje `#1234`
 * jako zgłoszenie (bez połączenia to projekt „1234”). W podglądzie symulowane
 * Redmine jest połączone, chyba że stan w `previewRedmineKey` mówi inaczej.
 */
export const redmineConnected = ref(!desktop);
/** Odświeża `redmineConnected` z `redmine_settings` (w podglądzie ze stanu symulacji). */
export async function refreshRedmineConnected(): Promise<void> {
  if (!desktop) {
    redmineConnected.value = previewRedmineState().connected !== false;
    return;
  }
  try {
    redmineConnected.value = (
      await invoke<{ connected: boolean }>("redmine_settings")
    ).connected;
  } catch {
    redmineConnected.value = false;
  }
}
/**
 * Nasłuch zdarzenia `data-changed` w aplikacji (B6). Komenda wywołana z tego
 * okna oznacza zdarzenie etykietą okna (`source`), a okno przyjmuje migawkę
 * z jej wyniku, więc takie zdarzenie pomija; zmiany z innych okien i z
 * backendu (`source` = null) wołają `handler`.
 */
export async function onDataChanged(handler: () => void): Promise<() => void> {
  const own = getCurrentWindow().label;
  return listen<{ source?: string | null } | null>("data-changed", (e) => {
    if (e.payload?.source === own) return;
    handler();
  });
}
export async function read(): Promise<Snapshot> {
  if (desktop) return invoke("get_snapshot");
  return snapshotSim.read();
}
/**
 * Pytanie z traya „Zakończ aplikację” w trakcie kroku przebiegu Workflows
 * (`quit::QUIT_REQUEST`, ładunek: nazwa procesu); odpowiada `QuitDialog.vue`.
 */
export const QUIT_REQUEST_EVENT = "app-quit-request";
/**
 * Potwierdzone „Zakończ” (`quit::app_quit_confirmed`): zatrzymanie pomiaru i
 * wyjście jak bez kroku w toku. Podgląd nie ma traya ani procesu do zakończenia.
 */
export async function quitAppConfirmed(): Promise<void> {
  if (desktop) return invoke("app_quit_confirmed");
}
/** Oczekujące pytanie o zakończenie (nazwa procesu), gdy zdarzenie przyszło przed nasłuchem. */
export async function quitAppPending(): Promise<string | null> {
  if (desktop) return invoke("app_quit_pending");
  return null;
}
/** „Anuluj” w pytaniu o zakończenie: backend zapomina pytanie. */
export async function quitAppCancel(): Promise<void> {
  if (desktop) return invoke("app_quit_cancel");
}
/**
 * Pytanie przed startem pomiaru (B13): po „Zakończ dzień” Start na zadaniu pyta
 * „Wznowić dzień?”. Rejestruje je okno z dialogiem (`DayDialogs.vue`) albo widget
 * (pytanie w głównym oknie); `false` = Anuluj (albo pytanie w innym oknie), wtedy
 * pomiar nie startuje, a funkcja startu zwraca bieżące dane. `taskId` = zadanie
 * do startu, gdy jest znane.
 */
export type StartGuard = (taskId: number | null) => Promise<boolean>;
let startGuard: StartGuard | null = null;
export function setStartGuard(guard: StartGuard | null) {
  startGuard = guard;
}
export async function startAllowed(
  taskId: number | null = null,
): Promise<boolean> {
  return startGuard ? startGuard(taskId) : true;
}
export async function change(
  action: string,
  id: number | null = null,
  title: string | null = null,
): Promise<Snapshot> {
  if (action === "start" && !(await startAllowed(id))) return read();
  if (desktop) return invoke("mutate", { action, id, title });
  return tasksSim.change(action, id, title);
}
/**
 * Szybkie dodawanie ze składnią: zadanie z polami, kluczem Jiry albo numerem
 * Redmine i opcjonalnym startem w jednym kroku. Odpowiednik
 * `quickadd::create_linked`; w podglądzie nie ma listy zgłoszeń Jiry, więc pusty
 * tytuł z kluczem bierze tytuł z danych `jira_issues` tylko w aplikacji; temat
 * `#1234` pochodzi z symulowanego Redmine (`previewRedmineIssues`).
 * `guarded`: wywołujący już zapytał `startAllowed` (formularz w TasksView).
 */
export async function quickAdd(
  task: NewTask,
  guarded = false,
): Promise<Snapshot> {
  // Anulowane „Wznowić dzień?” tworzy zadanie bez startu pomiaru.
  if (task.start && !guarded && !(await startAllowed()))
    task = { ...task, start: false };
  if (desktop) return invoke("quick_add", { task });
  return tasksSim.quickAdd(task);
}
/** Szablony do podpowiedzi i parsera; odświeżane po każdej zmianie. */
export const templates = ref<TaskTemplate[]>([]);
/** Lista szablonów (`templates_list`); odświeża też `templates`. */
export async function loadTemplates(): Promise<TaskTemplate[]> {
  const list = desktop
    ? await invoke<TaskTemplate[]>("templates_list")
    : templatesSim.templatesList();
  templates.value = list;
  return list;
}
/** Tworzy (`id` = null) albo zmienia szablon, także nazwę (`template_save`). */
export async function saveTemplate(
  id: number | null,
  name: string,
  body: TemplateBody,
): Promise<TaskTemplate[]> {
  name = name.trim();
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_save", {
      id,
      name,
      template: body,
    });
    return templates.value;
  }
  await templatesSim.saveTemplate(id, name, body);
  return loadTemplates();
}
/** Usuwa szablon (`template_delete`). */
export async function deleteTemplate(id: number): Promise<TaskTemplate[]> {
  if (desktop) {
    templates.value = await invoke<TaskTemplate[]>("template_delete", { id });
    return templates.value;
  }
  await templatesSim.deleteTemplate(id);
  return loadTemplates();
}
/** Przypisania folderów do zadań (`agent_bindings`). */
export async function agentBindings(): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bindings");
  return agentsSim.agentBindings();
}
/** Przypisuje folder agenta do zadania (`agent_bind`); zastępuje wcześniejsze. */
export async function bindAgentFolder(
  folder: string,
  taskId: number,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_bind", { folder, taskId });
  return agentsSim.bindAgentFolder(folder, taskId);
}
/**
 * Czas agentów w `[from, to)` (`agent_time`): okresy nakładające się na zakres
 * i sumy według zadania z częścią równoległą do Twojego pomiaru.
 */
export async function agentTime(from: number, to: number): Promise<AgentTime> {
  if (desktop) return invoke("agent_time", { from, to });
  return agentsSim.agentTime(from, to);
}
/** Usuwa przypisanie folderu (`agent_unbind`). */
export async function unbindAgentFolder(
  folder: string,
): Promise<AgentBinding[]> {
  if (desktop) return invoke("agent_unbind", { folder });
  return agentsSim.unbindAgentFolder(folder);
}
/** Odświeża `templates` po zmianie w innym oknie (np. szybkie przełączanie). */
export async function onTemplatesChanged(): Promise<() => void> {
  const reload = () => void loadTemplates().catch(() => {});
  if (desktop) return listen(templateEvent, reload);
  window.addEventListener(templateEvent, reload);
  return () => window.removeEventListener(templateEvent, reload);
}
export async function editSession(
  id: number,
  startedAt: number,
  /** `null`: poprawka początku trwającego pomiaru (koniec bez zmian). */
  endedAt: number | null,
): Promise<Snapshot> {
  if (desktop) return invoke("edit_session", { id, startedAt, endedAt });
  return tasksSim.editSession(id, startedAt, endedAt);
}
/** Przypisuje lukę między pomiarami (`db::fill_gap`). */
export async function fillGap(
  beforeId: number,
  afterId: number,
  mode: GapMode,
): Promise<Snapshot> {
  if (desktop) return invoke("fill_gap", { beforeId, afterId, mode });
  return tasksSim.fillGap(beforeId, afterId, mode);
}
/** Odpowiednik db::split_session dla trybu podglądu. */
export async function splitSession(
  id: number,
  at: number,
  targetTask: number | null = null,
): Promise<Snapshot> {
  if (desktop) return invoke("split_session", { id, at, targetTask });
  return tasksSim.splitSession(id, at, targetTask);
}
export async function saveDetails(
  id: number,
  details: TaskDetails,
): Promise<Snapshot> {
  if (desktop) return invoke("save_details", { id, details });
  return tasksSim.saveDetails(id, details);
}
/**
 * Blokada z kafelka: „Odblokuj”, propozycje z Jiry („Oznacz jako czekam na”,
 * „Nie teraz”). Zmienia tylko pola blokady (`set_waiting`); odpowiednik w podglądzie.
 */
export async function setWaiting(
  id: number,
  change: WaitingChange,
): Promise<Snapshot> {
  if (desktop) return invoke("set_waiting", { id, change });
  return tasksSim.setWaiting(id, change);
}
export async function addSession(
  taskId: number,
  startedAt: number,
  endedAt: number,
): Promise<Snapshot> {
  if (desktop) return invoke("add_session", { taskId, startedAt, endedAt });
  return tasksSim.addSession(taskId, startedAt, endedAt);
}
export async function setPriority(
  id: number,
  priority: number,
): Promise<Snapshot> {
  if (desktop) return invoke("set_priority", { id, priority });
  return tasksSim.setPriority(id, priority);
}
/**
 * Odpowiednik `reorder_tasks` (`planner::reorder`): zmienia wyłącznie `order`
 * i `priority` podanych zadań, reszta szczegółów zostaje bez zmian.
 */
export async function reorderTasks(
  entries: { id: number; order: number; priority: number }[],
): Promise<Snapshot> {
  if (desktop) return invoke("reorder_tasks", { entries });
  return tasksSim.reorderTasks(entries);
}
/**
 * Odpowiednik `plan_task` (`planner::plan_task`): przeniesienie na tablicy
 * tygodnia zmienia wyłącznie `plannedDay` (pusty = „Do zaplanowania”) i `order`.
 */
export async function planTask(
  id: number,
  day: string,
  order: number,
): Promise<Snapshot> {
  if (desktop) return invoke("plan_task", { id, day, order });
  return tasksSim.planTask(id, day, order);
}
/**
 * Odpowiednik `plan_tasks` (`planner::plan_tasks`): „Zastosuj” z „Rozłóż”
 * zapisuje dni wielu zadań naraz; jak `planTask` zmienia tylko `plannedDay`
 * i `order`, a przy błędzie nie zmienia niczego.
 */
export async function planTasks(entries: PlanEntry[]): Promise<Snapshot> {
  if (desktop) return invoke("plan_tasks", { entries });
  return tasksSim.planTasks(entries);
}
/**
 * Odpowiednik `save_day_notes` (`planner::save_day_notes`, B6): notatki dnia
 * wielu zadań jednym wywołaniem; zmienia tylko `notes[day]` (pusta usuwa).
 */
export async function saveDayNotes(
  day: string,
  entries: DayNote[],
): Promise<Snapshot> {
  if (desktop) return invoke("save_day_notes", { day, entries });
  return tasksSim.saveDayNotes(day, entries);
}
export async function closeDay(
  day: string,
  closed: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("close_day", { day, closed });
  return tasksSim.closeDay(day, closed);
}
/** Przypięcie do widgetu i odpięcie (`set_pinned`); odpowiednik w podglądzie. */
export async function setPinned(
  id: number,
  pinned: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("set_pinned", { id, pinned });
  return tasksSim.setPinned(id, pinned);
}
/**
 * Start/Stop jak globalny skrót: trwający pomiar → Stop, brak pomiaru → Start
 * ostatniego zadania, brak takiego → „quick”. Ani komenda `toggle_measurement`, ani
 * podgląd nie otwierają wtedy szybkiego przełączania: robi to wywołujący (`quick_open`).
 * Potwierdzenie w aplikacji przychodzi zdarzeniem `toggle-feedback` (tekst).
 */
export async function toggleMeasurement(): Promise<ToggleResult> {
  if (desktop) return invoke("toggle_measurement");
  return tasksSim.toggleMeasurement();
}
/**
 * `autostart_get`: „włączone” pochodzi z rejestru (w podglądzie z osobnego klucza
 * localStorage), „schowane” z ustawienia `autostartHidden`.
 */
export async function readAutostart(): Promise<Autostart> {
  if (desktop) return invoke("autostart_get");
  return tasksSim.readAutostart();
}
/** `autostart_set`: zapisuje wpis Run (z `--hidden`, gdy `hidden`) i ustawienie. */
export async function writeAutostart(
  enabled: boolean,
  hidden: boolean,
): Promise<Autostart> {
  if (desktop) return invoke("autostart_set", { enabled, hidden });
  return tasksSim.writeAutostart(enabled, hidden);
}
/** `widget_status`: ustawienia widgetu i schowanie do końca sesji. */
export async function readWidgetStatus(): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_status");
  return tasksSim.readWidgetStatus();
}
/**
 * `widget_dismiss`: X na widgecie chowa go do końca sesji aplikacji (w podglądzie
 * do końca sesji karty), przełącznik w nagłówku przywraca. Ustawienie bez zmian.
 */
export async function dismissWidget(dismissed: boolean): Promise<WidgetStatus> {
  if (desktop) return invoke("widget_dismiss", { dismissed });
  return tasksSim.dismissWidget(dismissed);
}
/** Nasłuch zmian stanu widgetu (zdarzenie Tauri albo zdarzenie okna w podglądzie). */
export async function onWidgetStatus(
  handler: (status: WidgetStatus) => void,
): Promise<() => void> {
  if (desktop) {
    return listen<WidgetStatus>(widgetEvent, (e) => handler(e.payload));
  }
  const listener = (e: Event) =>
    handler((e as CustomEvent<WidgetStatus>).detail);
  window.addEventListener(widgetEvent, listener);
  return () => window.removeEventListener(widgetEvent, listener);
}
/**
 * Ładunek zdarzenia `navigate` (`notify::View` w Rust): kliknięcie powiadomienia
 * Windows lub przycisku „Podziel…”. Brak `view` = tylko główne okno.
 */
export interface NavigateRequest {
  view?: string | null;
  /** `today` = filtr „Dzisiaj: plan i praca”; `due` = ten filtr z terminami dziś i zaległymi. */
  filter?: string | null;
  /** Zadanie do rozwinięcia na liście zadań. */
  task?: number | null;
  /** Przedział do otwarcia w edytorze na zakładce Podziel. */
  split?: number | null;
  /** Przebieg Workflows do otwarcia w monitorze (`view: "workflow-runs"`). */
  run?: number | null;
  /**
   * Grupa listy zadań, do której przewinąć (np. „Nierozpoczęte”), przy
   * `view: "journal"` zakładka `morning` (Plan i historia → Rano) albo
   * `week-review` (Plan i historia → Tydzień → Przegląd), albo przy
   * `view: "settings"` sekcja Ustawień (`work`, `calendar`, `timesheet`,
   * `jira`, `redmine`, `notifications`, `focus`, `templates`, `windows`, `shortcuts`,
   * `startup`; nieznana sekcja otwiera ostatnią).
   */
  section?: string | null;
  /** Tekst pola „Nowe zadanie” (np. „Popraw” w karcie czatu); fokus w polu. */
  newTask?: string | null;
  /** Otwórz czat asystenta (A3) z kontekstem albo pytaniem; bez `view` widok się nie zmienia. */
  chat?: ChatRequest | null;
  /** Plan i historia → Zamknięcie dnia. */
  dayClose?: boolean;
}
/** Nazwa zdarzenia okna, którym podgląd (i testy) symuluje `navigate` z backendu. */
export const previewNavigateEvent = "tuktuk-track-navigate";
/** Nasłuch `navigate`: zdarzenie Tauri albo zdarzenie okna w podglądzie. */
export async function onNavigate(
  handler: (request: NavigateRequest) => void,
): Promise<() => void> {
  if (desktop)
    return listen<NavigateRequest>("navigate", (e) => handler(e.payload));
  const listener = (e: Event) =>
    handler((e as CustomEvent<NavigateRequest>).detail ?? {});
  window.addEventListener(previewNavigateEvent, listener);
  return () => window.removeEventListener(previewNavigateEvent, listener);
}
/** `widget_open_main`: klik w nazwę zadania na widgecie. W podglądzie nic nie robi. */
export async function openMainWindow(): Promise<void> {
  if (desktop) await invoke("widget_open_main");
}
/**
 * `export_report`: okno zapisu i plik CSV albo XLSX z przedziałami wybranych
 * dni; `null`, gdy użytkownik anulował. W podglądzie CSV wraca jako tekst,
 * a XLSX jest niedostępny.
 */
export async function exportReport(
  format: ExportFormat,
  days: string[],
  name: string,
): Promise<ExportResult | null> {
  if (!days.length) throw new Error("Wybierz co najmniej jeden dzień do eksportu.");
  if (desktop) {
    const path = await invoke<string | null>("export_report", {
      format,
      days,
      name,
    });
    return path ? { path } : null;
  }
  return tasksSim.exportReport(format, days, name);
}
/**
 * Przejście w głównym oknie z innego okna (np. szybkiego przełączania):
 * pokazuje główne okno i wysyła mu `navigate`. W podglądzie nic nie robi,
 * bo wszystkie widoki są w jednym oknie z zarejestrowaną obsługą.
 */
export async function navigateMain(request: NavigateRequest): Promise<void> {
  if (!desktop) return;
  await invoke("widget_open_main");
  await emitTo("main", "navigate", request);
}
/** Wnioski tygodnia zaczynającego się w poniedziałek `week` (`week_note`). */
export async function weekNote(week: string): Promise<string> {
  if (desktop) return invoke("week_note", { week });
  return tasksSim.weekNote(week);
}
/**
 * Zapis wniosków (`week_note_save`): tekst bez białych znaków na końcach, pusty
 * usuwa wpis. Zwraca zapisany tekst.
 */
export async function saveWeekNote(week: string, text: string): Promise<string> {
  if (desktop) return invoke("week_note_save", { week, text });
  return tasksSim.saveWeekNote(week, text);
}
/** `agent_hooks_status`: rejestracja hooków agentów, zaufanie Codexa i ostatnie zdarzenia. */
export async function agentHooksStatus(): Promise<AgentHooksStatus> {
  if (desktop) return invoke("agent_hooks_status");
  return hooksSim.agentHooksStatus();
}
/** `external_api_info`: ścieżka tuktuk-track-ctl.exe do Ustawień (w podglądzie brak). */
export async function externalApiInfo(): Promise<ExternalApiInfo> {
  if (desktop) return invoke("external_api_info");
  return hooksSim.externalApiInfo();
}
/** `ai_status`: wykryte CLI, wybrany silnik i czy pokazać dialog pierwszego uruchomienia (A1). */
export async function aiStatus(): Promise<AiStatus> {
  if (desktop) return invoke("ai_status");
  return aiSim.aiStatus();
}
/** `ai_detect`: „Sprawdź ponownie”, wykrywa CLI od nowa. */
export async function aiDetect(): Promise<AiStatus> {
  if (desktop) return invoke("ai_detect");
  return aiSim.aiDetect();
}
/** `ai_calls`: ostatnie wywołania asystenta bez treści, od najnowszego. */
export async function aiCalls(): Promise<AiCall[]> {
  if (desktop) return invoke("ai_calls");
  return aiSim.aiCalls();
}
/** `ai_redact_preview`: tekst po filtrze danych osobowych („Pokaż, co zostanie wysłane”); nic nie wysyła. */
export async function aiRedactPreview(text: string): Promise<string> {
  if (desktop) return invoke("ai_redact_preview", { text });
  return aiSim.aiRedactPreview(text);
}
/** `ai_test`: jedno próbne wywołanie silnika po kliknięciu „Przetestuj”. */
export async function aiTest(): Promise<string> {
  if (desktop) return invoke("ai_test");
  return aiSim.aiTest();
}
/** `orca_status`: orca.cmd, ścieżka i czy Orca działa (B9). */
export async function orcaStatus(): Promise<OrcaStatus> {
  if (desktop) return invoke("orca_status");
  return orcaSim.orcaStatus();
}
/** `orca_command`: polecenie `worktree create` do podglądu w dialogu; nic nie uruchamia. */
export async function orcaCommand(req: OrcaCreateRequest): Promise<string> {
  if (desktop) return invoke("orca_command", { req });
  return orcaSim.orcaCommand(req);
}
/** `orca_repos`: repozytoria Orki (wymaga działającej Orki). */
export async function orcaRepos(): Promise<OrcaRepo[]> {
  if (desktop) return invoke("orca_repos");
  return orcaSim.orcaRepos();
}
/** `orca_find`: worktree podpięte do zgłoszenia (`reference find`). */
export async function orcaFind(key: string): Promise<OrcaWorktree[]> {
  if (desktop) return invoke("orca_find", { key });
  return orcaSim.orcaFind(key);
}
/** `orca_create`: tworzy worktree w Orce; wołać tylko po kliknięciu w dialogu. */
export async function orcaCreate(req: OrcaCreateRequest): Promise<OrcaCreated> {
  if (desktop) return invoke("orca_create", { req });
  return orcaSim.orcaCreate(req);
}
/** `browser_status`: stan wyłącznika, rejestracji i ostatniego sygnału. */
export async function browserStatus(): Promise<BrowserStatus> {
  if (desktop) return invoke("browser_status");
  return tasksSim.browserStatus();
}
/** `browser_open_folder`: folder rozszerzenia w Eksploratorze (tylko aplikacja). */
export async function browserOpenFolder(): Promise<void> {
  if (desktop) return invoke("browser_open_folder");
  return tasksSim.browserOpenFolder();
}
/** Pytania o przesunięcie zgłoszenia (`jira_move_prompts`). */
export async function readMovePrompts(): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_prompts");
  return jiraSim.readMovePrompts();
}
/** „Nie teraz” (`jira_move_dismiss`): pytanie znika do kolejnego startu zadania. */
export async function dismissMovePrompt(
  taskId: number,
): Promise<JiraMovePrompt[]> {
  if (desktop) return invoke("jira_move_dismiss", { taskId });
  return jiraSim.dismissMovePrompt(taskId);
}
/** „Nie pytaj dla tego zadania” i „Pytaj znowu” (`jira_no_ask`). */
export async function setJiraNoAsk(
  taskId: number,
  noAsk: boolean,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_no_ask", { taskId, noAsk });
  return jiraSim.setJiraNoAsk(taskId, noAsk);
}
/** Dostępne przejścia zgłoszenia zadania (`jira_transitions`). */
export async function jiraTransitions(
  taskId: number,
): Promise<JiraTransition[]> {
  if (desktop) return invoke("jira_transitions", { taskId });
  return jiraSim.jiraTransitions(taskId);
}
/**
 * Przejście zgłoszenia (`jira_transition`): status na kafelku po odświeżeniu,
 * pytanie zadania znika. Błąd symulowanej Jiry jak `jira::write_error`.
 */
export async function jiraTransition(
  taskId: number,
  transitionId: string,
): Promise<Snapshot> {
  if (desktop) return invoke("jira_transition", { taskId, transitionId });
  return jiraSim.jiraTransition(taskId, transitionId);
}
/** Nasłuch zmian pytań (zdarzenie Rust, w podglądzie zdarzenie okna i `storage`). */
export async function onMovePromptsChanged(
  handler: () => void,
): Promise<() => void> {
  if (desktop) return listen(jiraMoveEvent, handler);
  const storage = (e: StorageEvent) => {
    if (e.key === previewJiraKey || e.key === previewRedmineKey || e.key === snapshotKey)
      handler();
  };
  window.addEventListener(jiraMoveEvent, handler);
  window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(jiraMoveEvent, handler);
    window.removeEventListener("storage", storage);
  };
}
/** `jira_watch_status` (B5): ostatnie udane sprawdzenie przypisań i ostatni błąd. */
export async function jiraWatchStatus(): Promise<JiraWatchStatus> {
  if (desktop) return invoke("jira_watch_status");
  return jiraSim.jiraWatchStatus();
}
/**
 * Nasłuch stanu obserwacji po każdym sprawdzeniu (zdarzenie Rust z nowym
 * stanem; w podglądzie zmiana stanu symulowanej Jiry z innego okna).
 */
export async function onJiraWatch(
  handler: (status: JiraWatchStatus | null) => void,
): Promise<() => void> {
  if (desktop)
    return listen<JiraWatchStatus>(jiraWatchEvent, (e) => handler(e.payload));
  const storage = (e: StorageEvent) => {
    if (e.key === previewJiraKey) handler(null);
  };
  window.addEventListener("storage", storage);
  return () => window.removeEventListener("storage", storage);
}
/** `jira_settings` (w podglądzie połączone konto przykładowe). */
export async function jiraAccount(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_settings");
  return jiraSim.jiraAccount();
}
export async function jiraConnect(
  site: string,
  email: string,
  apiToken: string,
): Promise<JiraAccount> {
  if (desktop) return invoke("jira_connect", { site, email, apiToken });
  return jiraSim.jiraConnect(site, email, apiToken);
}
export async function jiraDisconnect(): Promise<JiraAccount> {
  if (desktop) return invoke("jira_disconnect");
  return jiraSim.jiraDisconnect();
}
/** `jira_boards`: wyszukiwanie tablic. */
export async function jiraBoards(
  name: string,
  startAt: number,
): Promise<{ values: JiraBoardRef[]; nextStart: number | null }> {
  if (desktop) return invoke("jira_boards", { name, startAt });
  return jiraSim.jiraBoards(name, startAt);
}
/** `jira_fields`: lista pól Jiry do wyboru (w podglądzie stała lista). */
export async function jiraFields(refresh: boolean): Promise<JiraFieldInfo[]> {
  if (desktop) return invoke("jira_fields", { refresh });
  return jiraSim.jiraFields(refresh);
}
/**
 * `jira_refresh_fields`: status i pola zgłoszeń otwartych zadań od razu po
 * zmianie listy pól; zwraca liczbę odświeżonych zgłoszeń. W podglądzie
 * wartości biorą się z symulowanej tablicy przy odczycie.
 */
export async function jiraRefreshFields(): Promise<number> {
  if (desktop) return invoke("jira_refresh_fields");
  return jiraSim.jiraRefreshFields();
}
/** `jira_select_boards`: wybrane tablice (najwyżej 20). */
export async function jiraSelectBoards(boards: JiraBoardRef[]): Promise<void> {
  if (desktop) return invoke("jira_select_boards", { boards });
  return jiraSim.jiraSelectBoards(boards);
}
/** `jira_issues`: lista „Przypisane do mnie”. */
export async function jiraIssues(
  openOnly: boolean,
): Promise<{ issues: JiraListIssue[]; warnings: string[] }> {
  if (desktop) return invoke("jira_issues", { openOnly });
  return jiraSim.jiraIssues(openOnly);
}
/** `jira_start`: zadanie zgłoszenia (nowe albo z kosza/archiwum) i jeden pomiar. */
export async function jiraStart(issueKey: string): Promise<Snapshot> {
  if (!(await startAllowed())) return read();
  if (desktop) return invoke("jira_start", { key: issueKey });
  return jiraSim.jiraStart(issueKey);
}
/** `jira_open`: zgłoszenie w przeglądarce (w podglądzie tylko zapisany link). */
export async function jiraOpen(url: string): Promise<void> {
  if (desktop) return invoke("jira_open", { url });
  return jiraSim.jiraOpen(url);
}
/** `jira_board`: kolumny i karty tablicy. */
export async function jiraBoard(
  boardId: number,
  scope: JiraScope,
  openOnly: boolean,
  refresh: boolean,
): Promise<JiraBoardView> {
  if (desktop)
    return invoke("jira_board", { boardId, scope, openOnly, refresh });
  return jiraSim.jiraBoard(boardId, scope, openOnly, refresh);
}
/** `jira_board_move`: przeciągnięcie karty i „Przesuń do…”. */
export async function jiraBoardMove(
  boardId: number,
  issueKey: string,
  column: number,
  transitionId: string | null,
): Promise<JiraMoveOutcome> {
  if (desktop)
    return invoke("jira_board_move", {
      boardId,
      key: issueKey,
      column,
      transitionId,
    });
  return jiraSim.jiraBoardMove(boardId, issueKey, column, transitionId);
}
/** `jira_assignable`: osoby do przypisania. */
export async function jiraAssignable(
  issueKey: string,
  query: string,
): Promise<JiraPerson[]> {
  if (desktop) return invoke("jira_assignable", { key: issueKey, query });
  return jiraSim.jiraAssignable(issueKey, query);
}
/** `jira_assign`: „Przypisz do mnie” (`accountId` = null) i „Zmień osobę…”. */
export async function jiraAssign(
  issueKey: string,
  accountId: string | null,
): Promise<JiraPerson> {
  if (desktop) return invoke("jira_assign", { key: issueKey, accountId });
  return jiraSim.jiraAssign(issueKey, accountId);
}
/** `jira_comment`: komentarz zwykłym tekstem (w Jirze jako ADF). */
export async function jiraComment(
  issueKey: string,
  text: string,
): Promise<void> {
  if (desktop) return invoke("jira_comment", { key: issueKey, text });
  return jiraSim.jiraComment(issueKey, text);
}
/** `jira_branch_name`: nazwa gałęzi z prefiksem z ustawień. */
export async function jiraBranchName(
  issueKey: string,
  title: string,
): Promise<string> {
  if (desktop) return invoke("jira_branch_name", { key: issueKey, title });
  return jiraSim.jiraBranchName(issueKey, title);
}
/** `jira_projects`: projekty wybranych tablic. */
export async function jiraProjects(): Promise<JiraProject[]> {
  if (desktop) return invoke("jira_projects");
  return jiraSim.jiraProjects();
}
/** `jira_issue_types`: typy zgłoszeń projektu. */
export async function jiraIssueTypes(
  project: string,
): Promise<JiraIssueType[]> {
  if (desktop) return invoke("jira_issue_types", { project });
  return jiraSim.jiraIssueTypes(project);
}
/** `jira_issue_type_fields`: wymagane pola spoza formularza. */
export async function jiraIssueTypeFields(
  project: string,
  issueTypeId: string,
): Promise<JiraTypeFields> {
  if (desktop)
    return invoke("jira_issue_type_fields", { project, issueTypeId });
  return jiraSim.jiraIssueTypeFields(project, issueTypeId);
}
/** `jira_create`: nowe zgłoszenie i opcjonalnie zadanie w TukTuk Track. */
export async function jiraCreate(issue: JiraNewIssue): Promise<JiraCreated> {
  if (desktop) return invoke("jira_create", { issue });
  return jiraSim.jiraCreate(issue);
}
/** `ts_settings`: konfiguracja bez hasła, stan połączenia i przełącznik wysyłki. */
export async function tsSettings(): Promise<TsSettings> {
  if (desktop) return invoke("ts_settings");
  return timesheetSim.tsSettings();
}
/** `ts_connect`: logowanie, zapis hasła w magazynie i konfiguracji. */
export async function tsConnect(
  apiUrl: string,
  login: string,
  password: string,
): Promise<TsSettings> {
  tsForgetTasks();
  if (desktop)
    return notifyTsAfter(invoke("ts_connect", { apiUrl, login, password }));
  return timesheetSim.tsConnect(apiUrl, login, password);
}
/** `ts_disconnect`: usuwa hasło, zostawia login i identyfikator urządzenia. */
export async function tsDisconnect(): Promise<TsSettings> {
  tsForgetTasks();
  if (desktop) return notifyTsAfter(invoke("ts_disconnect"));
  return timesheetSim.tsDisconnect();
}
/** `ts_tasks`: projekty i zadania TS z formularza dnia. */
export async function tsTasks(date: string): Promise<TsProject[]> {
  if (desktop) return invoke("ts_tasks", { date });
  return timesheetSim.tsTasks(date);
}
/** `ts_filled`: wpisy już zapisane w TS na ten dzień. */
export async function tsFilled(date: string): Promise<TsFilled[]> {
  if (desktop) return invoke("ts_filled", { date });
  return timesheetSim.tsFilled(date);
}
/**
 * `ts_send`: zapis dnia w TS (zastępuje cały dzień; `keep` dokleja wpisy TS
 * o zadaniach spoza listy), potem odczyt dnia i znacznik wysyłki.
 */
export async function tsSend(
  date: string,
  entries: TsEntry[],
  mode: TsMode,
): Promise<TsSendResult> {
  if (desktop)
    return notifyTsAfter(invoke("ts_send", { date, entries, mode }));
  return timesheetSim.tsSend(date, entries, mode);
}
async function notifyTsAfter<T>(result: Promise<T>): Promise<T> {
  const value = await result;
  notifyTs();
  return value;
}
/** Nasłuch zmian TS; zwraca funkcję odłączającą. */
export function onTsChanged(handler: () => void): () => void {
  const storage = (e: StorageEvent) => {
    if (e.key !== previewTsKey) return;
    // Lista zadań TS mogła się zmienić w symulowanym TS.
    tsForgetTasks();
    handler();
  };
  window.addEventListener(tsChangedEvent, handler);
  if (!desktop) window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(tsChangedEvent, handler);
    window.removeEventListener("storage", storage);
  };
}
/** `ts_sent`: ostatnia wysyłka dnia albo null. */
export async function tsSent(day: string): Promise<TsSent | null> {
  if (desktop) return invoke("ts_sent", { day });
  return timesheetSim.tsSent(day);
}
/** `ts_project_map`: domyślne zadania TS projektów. */
export async function tsProjectMap(): Promise<TsProjectMapping[]> {
  if (desktop) return invoke("ts_project_map");
  return timesheetSim.tsProjectMap();
}
/**
 * `ts_set_project_map`: domyślne zadanie TS projektu (puste `tsTaskId` usuwa
 * mapowanie); zwraca całą listę.
 */
export async function tsSetProjectMap(
  project: string,
  tsTaskId: string,
  tsTaskName: string,
): Promise<TsProjectMapping[]> {
  if (desktop)
    return notifyTsAfter(
      invoke("ts_set_project_map", { project, tsTaskId, tsTaskName }),
    );
  return timesheetSim.tsSetProjectMap(project, tsTaskId, tsTaskName);
}
/**
 * Lista zadań TS do wyboru w edycji zadania: `ts_tasks(dzisiaj)` pobrane raz na
 * czas działania aplikacji (pamięć podręczna w pamięci); `refresh` pobiera
 * ponownie. Nieudane pobranie nie zostaje w pamięci.
 */
let tsTaskCache: { day: string; list: Promise<TsProject[]> } | null = null;
export function tsTodayTasks(refresh = false): Promise<TsProject[]> {
  const day = dayKey(Date.now());
  if (refresh || !tsTaskCache || tsTaskCache.day !== day) {
    const entry: { day: string; list: Promise<TsProject[]> } = {
      day,
      list: Promise.resolve([]),
    };
    entry.list = tsTasks(day).catch((e) => {
      if (tsTaskCache === entry) tsTaskCache = null;
      throw e;
    });
    tsTaskCache = entry;
  }
  return tsTaskCache.list;
}
/** Czyści pamięć podręczną listy zadań TS (połączenie, rozłączenie). */
export function tsForgetTasks() {
  tsTaskCache = null;
}
/** `redmine_issue_transitions`: dozwolone statusy zgłoszenia bez bieżącego. */
export async function redmineIssueTransitions(
  issueId: number,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_issue_transitions", { issueId });
  return redmineSim.redmineIssueTransitions(issueId);
}
/**
 * `redmine_issue_transition`: zmiana statusu (`PUT` ze `status_id`), odczyt po
 * zapisie i status na kafelkach powiązanych zadań. Symulowany Redmine może
 * odrzucić zapis (`fail`) albo przyjąć go bez zmiany (`unchanged`).
 */
export async function redmineIssueTransition(
  issueId: number,
  statusId: number,
): Promise<RedmineIssue | null> {
  if (desktop)
    return invoke("redmine_issue_transition", { issueId, statusId });
  return redmineSim.redmineIssueTransition(issueId, statusId);
}
/** `redmine_transitions`: dozwolone statusy zgłoszenia zadania bez bieżącego. */
export async function redmineTransitions(
  taskId: number,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_transitions", { taskId });
  return redmineSim.redmineTransitions(taskId);
}
/**
 * `redmine_transition`: zmiana statusu zgłoszenia zadania (baner, widget,
 * kafelek); pytanie zadania znika, a błąd zostaje przy pytaniu jak dla Jiry.
 */
export async function redmineTransition(
  taskId: number,
  statusId: number,
): Promise<Snapshot> {
  if (desktop) return invoke("redmine_transition", { taskId, statusId });
  return redmineSim.redmineTransition(taskId, statusId);
}
/** `redmine_assign_me`: `PUT` z `assigned_to_id` = moje konto. */
export async function redmineAssignMe(
  issueId: number,
): Promise<RedmineIssue | null> {
  if (desktop) return invoke("redmine_assign_me", { issueId });
  return redmineSim.redmineAssignMe(issueId);
}
/** `redmine_comment`: komentarz zwykłym tekstem w `notes` (1–4000 znaków). */
export async function redmineComment(
  issueId: number,
  text: string,
): Promise<void> {
  if (desktop) return invoke("redmine_comment", { issueId, text });
  return redmineSim.redmineComment(issueId, text);
}
/** `redmine_branch_name`: `feature/1234-krotki-tytul` z prefiksem z ustawień. */
export async function redmineBranchName(issueId: number): Promise<string> {
  if (desktop) return invoke("redmine_branch_name", { issueId });
  return redmineSim.redmineBranchName(issueId);
}
async function notifyRedmineAfter<T>(result: Promise<T>): Promise<T> {
  const value = await result;
  notifyRedmine();
  return value;
}
/** Nasłuch zmian Redmine; zwraca funkcję odłączającą. */
export function onRedmineChanged(handler: () => void): () => void {
  const storage = (e: StorageEvent) => {
    if (e.key === previewRedmineKey || e.key === null) handler();
  };
  window.addEventListener(redmineChangedEvent, handler);
  if (!desktop) window.addEventListener("storage", storage);
  return () => {
    window.removeEventListener(redmineChangedEvent, handler);
    window.removeEventListener("storage", storage);
  };
}
/** `redmine_settings` (w podglądzie konto przykładowe, chyba że rozłączone). */
export async function redmineAccount(): Promise<RedmineAccount> {
  if (desktop) return invoke("redmine_settings");
  return redmineSim.redmineAccount();
}
/**
 * `redmine_connect`: adres i klucz sprawdzone przez `/users/current.json`,
 * klucz w magazynie poświadczeń (nigdy nie wraca), reszta w konfiguracji.
 * W podglądzie klucz „zly” daje 401, a `certError` błąd certyfikatu bez
 * opcji „Akceptuj niezaufany certyfikat”. Zmiana adresu czyści śledzone
 * projekty jak w Rust.
 */
export async function redmineConnect(
  site: string,
  apiKey: string,
  acceptInvalidCert: boolean,
): Promise<RedmineAccount> {
  if (desktop)
    return notifyRedmineAfter(
      invoke("redmine_connect", { site, apiKey, acceptInvalidCert }),
    );
  await redmineSim.redmineConnect(site, apiKey, acceptInvalidCert);
  redmineConnected.value = true;
  return notifyRedmineAfter(redmineAccount());
}
/** `redmine_test`: „Połączono jako Imię Nazwisko (login), Redmine odpowiada.” */
export async function redmineTest(): Promise<string> {
  if (desktop) return invoke("redmine_test");
  return redmineSim.redmineTest();
}
/** `redmine_disconnect`: usuwa klucz; adres, projekty i powiązania zostają. */
export async function redmineDisconnect(): Promise<RedmineAccount> {
  if (desktop) return notifyRedmineAfter(invoke("redmine_disconnect"));
  await redmineSim.redmineDisconnect();
  redmineConnected.value = false;
  return notifyRedmineAfter(redmineAccount());
}
/**
 * `redmine_issues`: zgłoszenia projektu (z podprojektami, jak Redmine) albo —
 * bez `projectId` — moje ze wszystkich projektów; „Moje” = przypisane do mnie,
 * `openOnly` = bez zamkniętych. Najwyżej 500 z ostrzeżeniem.
 */
export async function redmineIssues(
  projectId: number | null,
  mine: boolean,
  openOnly: boolean,
): Promise<RedmineIssues> {
  if (desktop) return invoke("redmine_issues", { projectId, mine, openOnly });
  return redmineSim.redmineIssues(projectId, mine, openOnly);
}
/** `redmine_projects`: wszystkie projekty (wybór śledzonych w Ustawieniach, R5). */
export async function redmineProjects(): Promise<RedmineProject[]> {
  if (desktop) return invoke("redmine_projects");
  return redmineSim.redmineProjects();
}
/** `redmine_select_projects`: śledzone projekty (najwyżej 50). */
export async function redmineSelectProjects(
  projects: RedmineProject[],
): Promise<void> {
  if (desktop)
    return notifyRedmineAfter(invoke("redmine_select_projects", { projects }));
  return redmineSim.redmineSelectProjects(projects);
}
/** `redmine_statuses`: statusy w kolejności z Redmine. */
export async function redmineStatuses(
  refresh: boolean,
): Promise<RedmineStatus[]> {
  if (desktop) return invoke("redmine_statuses", { refresh });
  return redmineSim.redmineStatuses(refresh);
}
/**
 * `redmine_start`: zadanie zgłoszenia (nowe albo z kosza/archiwum, z powrotem
 * do „do zrobienia”) i jeden pomiar, jak `redmine::start_issue`.
 */
export async function redmineStart(id: number): Promise<Snapshot> {
  if (!(await startAllowed())) return read();
  if (desktop) return invoke("redmine_start", { id });
  return redmineSim.redmineStart(id);
}
/** `redmine_open`: zgłoszenie w przeglądarce (w podglądzie tylko zapisany link). */
export async function redmineOpen(url: string): Promise<void> {
  if (desktop) return invoke("redmine_open", { url });
  return redmineSim.redmineOpen(url);
}
