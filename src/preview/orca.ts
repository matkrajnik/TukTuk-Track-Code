/**
 * Symulowana Orka w podglądzie (`orca.rs`, B9): stan w localStorage, bez procesu.
 *
 * Klucz `todo-track-preview-orca-v1`: `{ found?, running?, repos?, worktrees?,
 * created? }`. Domyślnie `orca.cmd` jest znaleziony, Orca działa, a repozytoria
 * to dwa przykładowe. `orca_create` dopisuje worktree do `worktrees` i `created`
 * (testy sprawdzają, co poszło do „Orki”).
 */
import { previewJson, previewSettingsKey } from "./storage";
import {
  orcaCommandLine,
  orcaCreateArgs,
  orcaDefaultPath,
  orcaNotFound,
  orcaNotRunning,
  type OrcaCreateRequest,
  type OrcaCreated,
  type OrcaRepo,
  type OrcaStatus,
  type OrcaWorktree,
} from "../api/orca";

export const orcaKey = "todo-track-preview-orca-v1";

const defaultRepos: OrcaRepo[] = [
  { id: "repo-wms", name: "wms", path: "C:\\Projekty\\wms" },
  { id: "repo-api", name: "api", path: "C:\\Projekty\\api" },
];

function stored(): Record<string, unknown> {
  return previewJson(orcaKey);
}
function save(patch: Record<string, unknown>) {
  try {
    localStorage.setItem(orcaKey, JSON.stringify({ ...stored(), ...patch }));
  } catch {
    // Bez localStorage podgląd działa bez zapamiętanych zmian.
  }
}
function list<T>(value: unknown, fallback: T[] = []): T[] {
  return Array.isArray(value) ? (value as T[]) : fallback;
}
const settings = () => previewJson(previewSettingsKey);
const autostart = () => settings().orcaAutostart === true;
const customPath = () =>
  typeof settings().orcaPath === "string" ? (settings().orcaPath as string) : "";
const found = () => stored().found !== false;
const running = () => stored().running !== false;

/** `orca_status` w podglądzie. */
export async function orcaStatus(): Promise<OrcaStatus> {
  const ok = found();
  return {
    found: ok,
    path: ok ? customPath().trim() || orcaDefaultPath : "",
    defaultPath: orcaDefaultPath,
    running: ok && running(),
    autostart: autostart(),
  };
}

/** Jak `orca::ensure_running`: Orka działa albo (z autostartem) „startuje”. */
function ensureRunning() {
  if (!found()) throw new Error(orcaNotFound);
  if (running()) return;
  if (!autostart()) throw new Error(orcaNotRunning);
  save({ running: true });
}

/** `orca_command`: polecenie z podglądu dialogu. */
export async function orcaCommand(req: OrcaCreateRequest): Promise<string> {
  return orcaCommandLine(orcaCreateArgs(req));
}

/** `orca_repos` w podglądzie. */
export async function orcaRepos(): Promise<OrcaRepo[]> {
  ensureRunning();
  return list<OrcaRepo>(stored().repos, defaultRepos);
}

/** `orca_find`: worktree podpięte do klucza albo adresu zgłoszenia. */
export async function orcaFind(key: string): Promise<OrcaWorktree[]> {
  ensureRunning();
  return list<OrcaWorktree>(stored().worktrees).filter((w) =>
    w.references.includes(key),
  );
}

/** `orca_create`: worktree „powstaje” tylko w stanie podglądu. */
export async function orcaCreate(
  req: OrcaCreateRequest,
): Promise<OrcaCreated> {
  // Jak w Rust: najpierw argumenty, potem stan Orki.
  const args = orcaCreateArgs(req);
  ensureRunning();
  const repo = req.repo.replace(/[\\/]+$/, "");
  const created: OrcaCreated = {
    name: req.name,
    path: `${repo}\\..\\orca\\${req.name}`,
    branch: req.name,
  };
  const worktree: OrcaWorktree = {
    ...created,
    references: req.reference ? [req.reference] : [],
  };
  save({
    worktrees: [...list<OrcaWorktree>(stored().worktrees), worktree],
    created: [...list(stored().created), { args, ...created }],
  });
  return created;
}
