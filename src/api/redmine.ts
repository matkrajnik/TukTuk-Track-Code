/**
 * Redmine (PLAN_1.9.md): typy komend, komunikaty i reguły z `redmine.rs`,
 * wspólne dla aplikacji i podglądu.
 */
import { priorityBand, type JiraTransition } from "./jira";
// --- Redmine: widok „projekt → zgłoszenia” (PLAN_1.9.md, R3) ---------------

/** Śledzony albo dostępny projekt Redmine (`redmine::Project`). */
export interface RedmineProject {
  id: number;
  name: string;
  identifier: string;
  parentId: number | null;
}
/** Konfiguracja połączenia bez klucza API (`redmine::Config`). */
export interface RedmineConfig {
  site: string;
  login: string;
  userId: number;
  displayName: string;
  acceptInvalidCert: boolean;
  projects: RedmineProject[];
}
/** `redmine_settings`: konfiguracja i czy klucz API jest w magazynie. */
export interface RedmineAccount {
  config: RedmineConfig;
  connected: boolean;
}
/** Status z `/issue_statuses.json` (`redmine::Status`). */
export interface RedmineStatus {
  id: number;
  name: string;
  isClosed: boolean;
}
/** Zgłoszenie z `/issues.json` (`redmine::Issue`). */
export interface RedmineIssue {
  id: number;
  subject: string;
  projectId: number;
  project: string;
  tracker: string;
  status: string;
  statusId: number;
  closed: boolean;
  priority: string;
  assignedToId: number | null;
  assignedTo: string;
  /** Wersja docelowa (`fixed_version.name`). */
  version: string;
  category: string;
  dueDate: string;
  updatedOn: string;
  url: string;
}
/** Grupa statusu: numery zgłoszeń w kolejności wyświetlania (`redmine::StatusGroup`). */
export interface RedmineStatusGroup {
  status: string;
  closed: boolean;
  ids: number[];
}
/** `redmine_issues`: lista, ostrzeżenia, grupy statusów i śledzone projekty z podprojektami. */
export interface RedmineIssues {
  issues: RedmineIssue[];
  warnings: string[];
  groups: RedmineStatusGroup[];
  tracked: number[];
}
// --- Redmine: zapisy (PLAN_1.9.md, R4) -------------------------------------

/** Komunikat przy wyłączonych zapisach (`redmine::WRITE_DISABLED`). */
export const redmineWriteDisabled =
  "Zmiany w Redmine są wyłączone. Włączysz je w Ustawienia → Redmine („Pozwól aplikacji zmieniać zgłoszenia w Redmine”).";
/** Najdłuższy komentarz (`redmine::NOTES_LIMIT`). */
export const redmineNotesLimit = 4000;
/** Status Redmine jako przejście pytania (`jira_move::redmine_transitions_of`). */
export const redmineTransitionOf = (s: RedmineStatus): JiraTransition => ({
  id: String(s.id),
  name: s.name,
  toStatus: s.name,
  toCategory: s.isClosed ? "done" : "indeterminate",
});
/**
 * Cel pytania przy starcie pracy jak `redmine::choose_status`: dozwolone
 * statusy niezamknięte bez bieżącego; wybrany status o nazwie z ustawień (bez
 * względu na wielkość liter), bez dopasowania `null` — lista do wyboru.
 */
export function chooseRedmineStatus(
  allowed: readonly RedmineStatus[],
  current: number,
  preferred: string,
): { statuses: RedmineStatus[]; statusId: number | null } {
  const statuses = allowed.filter((s) => s.id !== current && !s.isClosed);
  const wanted = preferred.trim().toLowerCase();
  const statusId =
    (wanted &&
      statuses.find((s) => s.name.trim().toLowerCase() === wanted)?.id) ||
    null;
  return { statuses, statusId };
}
/** Komunikat bez połączenia (`redmine::NOT_CONNECTED`). */
export const redmineNotConnected =
  "Połącz konto Redmine: podaj adres i klucz API.";
/** Najwięcej zgłoszeń na liście (5 stron po 100, `redmine::MAX_ISSUE_PAGES`). */
export const redmineIssueLimit = 500;
/**
 * Grupy statusów jak `redmine::group_by_status`: kolejność statusów z listy
 * (`/issue_statuses.json`), status spoza listy na końcu, puste grupy pominięte;
 * w grupie pasmo priorytetu malejąco, potem termin (bez terminu na końcu),
 * potem kolejność z serwera.
 */
export function redmineGroups(
  issues: readonly RedmineIssue[],
  statuses: readonly RedmineStatus[],
): RedmineStatusGroup[] {
  const groups = statuses.map((s) => ({
    status: s.name,
    closed: s.isClosed,
    id: s.id as number | null,
    members: [] as number[],
  }));
  issues.forEach((issue, index) => {
    let slot = groups.findIndex(
      (g) => issue.statusId !== 0 && g.id === issue.statusId,
    );
    if (slot < 0)
      slot = groups.findIndex(
        (g) =>
          g.status === issue.status && (g.id === null || issue.statusId === 0),
      );
    if (slot < 0)
      groups.push({
        status: issue.status,
        closed: issue.closed,
        id: null,
        members: [index],
      });
    else groups[slot].members.push(index);
  });
  return groups
    .filter((g) => g.members.length)
    .map((g) => {
      const sorted = [...g.members].sort((a, b) => {
        const x = issues[a],
          y = issues[b];
        const band =
          (priorityBand(y.priority) ?? 0) - (priorityBand(x.priority) ?? 0);
        if (band) return band;
        if (!x.dueDate !== !y.dueDate) return x.dueDate ? -1 : 1;
        if (x.dueDate !== y.dueDate) return x.dueDate < y.dueDate ? -1 : 1;
        return a - b;
      });
      return {
        status: g.status,
        closed: g.closed,
        ids: sorted.map((i) => issues[i].id),
      };
    });
}
/** Śledzone projekty z podprojektami jak `redmine::tracked_project_ids`. */
export function trackedProjectIds(
  tracked: readonly RedmineProject[],
  known: readonly RedmineProject[],
): number[] {
  const ids: number[] = [];
  for (const p of tracked) if (!ids.includes(p.id)) ids.push(p.id);
  for (let before = -1; before !== ids.length; ) {
    before = ids.length;
    for (const p of known)
      if (
        !ids.includes(p.id) &&
        p.parentId !== null &&
        ids.includes(p.parentId)
      )
        ids.push(p.id);
  }
  return ids;
}
/**
 * Zmiana konta albo śledzonych projektów Redmine: zdarzenie okna po „Połącz”,
 * „Rozłącz” i wyborze projektów, a w podglądzie także zdarzenie `storage` dla
 * `previewRedmineKey` (inne okno albo test zmienia symulowany Redmine), jak
 * przy Jirze i TS.
 */
export const redmineChangedEvent = "tuktuk-track-redmine-changed";
export function notifyRedmine() {
  window.dispatchEvent(new Event(redmineChangedEvent));
}
// --- Redmine: połączenie w Ustawieniach (PLAN_1.9.md, R5) ------------------

/** `redmine::SITE_ERROR`. */
export const redmineSiteError =
  "Podaj adres Redmine, np. https://redmine.firma.pl albo https://firma.pl/redmine (bez parametrów).";
/** `redmine::KEY_ERROR`. */
export const redmineKeyError =
  "Podaj klucz API Redmine (1–128 znaków, bez spacji).";
/** `redmine::status_error` dla 401. */
export const redmineKeyRejected =
  "Redmine odrzucił klucz API. Sprawdź klucz w „Moje konto” w Redmine.";
/** `redmine::CERT_ERROR`: certyfikat spoza magazynu Windows. */
export const redmineCertError =
  "Certyfikat serwera nie jest zaufany. Zainstaluj certyfikat firmowego urzędu w Windows albo włącz opcję w „Zaawansowane”.";
/**
 * Adres Redmine jak `redmine::normalize_site`: `http` albo `https`, host
 * wymagany, prefiks ścieżki dozwolony (bez końcowego `/`), bez parametrów,
 * fragmentu i danych logowania.
 */
export function redmineNormalizeSite(site: string): string {
  const raw = site.trim();
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    throw new Error(redmineSiteError);
  }
  if (
    !["http:", "https:"].includes(url.protocol) ||
    !url.hostname ||
    url.search ||
    url.hash ||
    raw.includes("?") ||
    raw.includes("#") ||
    url.username ||
    url.password
  )
    throw new Error(redmineSiteError);
  return url.href.replace(/\/+$/, "");
}
/** Klucz API jak `redmine::valid_api_key`: 1–128 znaków ASCII bez spacji. */
export const redmineValidApiKey = (key: string) =>
  key.length >= 1 && key.length <= 128 && /^[\x21-\x7e]+$/.test(key);
/** Link „Gdzie znaleźć klucz API”: `<adres>/my/account` albo null. */
export function redmineAccountUrl(site: string): string | null {
  try {
    return `${redmineNormalizeSite(site)}/my/account`;
  } catch {
    return null;
  }
}
