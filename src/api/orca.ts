/**
 * Worktree Orki z kafelka zadania (PLAN_BACKLOG.md, B9): typy komend `orca_*`
 * i reguły wspólne dla aplikacji i podglądu. Polecenie pokazane w dialogu
 * buduje backend (`orca::args_create`); funkcje tu to ich odpowiednik dla
 * podglądu przeglądarkowego (bez Rusta) oraz nazwa worktree z klucza i tytułu.
 */
import { fold } from "../searchSelect";

/** Stan Orki (`orca::Status`). */
export interface OrcaStatus {
  /** Znaleziono orca.cmd. */
  found: boolean;
  /** Używana ścieżka (pusta, gdy nie znaleziono). */
  path: string;
  /** Ścieżka wykrywana domyślnie. */
  defaultPath: string;
  running: boolean;
  /** „Uruchamiaj Orkę, gdy nie działa” jest włączone. */
  autostart: boolean;
}

/** Repozytorium Orki (`orca::Repo`). */
export interface OrcaRepo {
  id: string;
  name: string;
  path: string;
}

/** Worktree z `worktree list` / `reference find` (`orca::Worktree`). */
export interface OrcaWorktree {
  path: string;
  branch: string | null;
  name: string;
  references: string[];
}

/** Prośba o utworzenie worktree (`orca::CreateRequest`). */
export interface OrcaCreateRequest {
  name: string;
  repo: string;
  reference?: string | null;
  agent?: "claude" | "codex" | null;
  prompt?: string | null;
  baseBranch?: string | null;
  activate?: boolean;
}

/** Utworzony worktree (`orca::Created`). */
export interface OrcaCreated {
  name: string;
  path: string;
  branch: string | null;
}

/** Komunikat, gdy Orca nie działa i nie wolno jej uruchamiać (`orca::NOT_RUNNING`). */
export const orcaNotRunning =
  "Orca nie działa. Uruchom Orkę albo włącz „Uruchamiaj Orkę, gdy nie działa” w Ustawieniach → Agenci.";
/** Komunikat, gdy nie ma orca.cmd (`orca::NOT_FOUND`). */
export const orcaNotFound =
  "Nie znaleziono orca.cmd. Podaj ścieżkę w Ustawieniach → Agenci albo zainstaluj Orkę.";
/** Domyślna ścieżka pokazywana w Ustawieniach, gdy podgląd nie zna prawdziwej. */
export const orcaDefaultPath =
  "%LOCALAPPDATA%\\Programs\\orca\\resources\\bin\\orca.cmd";

/**
 * Nazwa worktree z klucza i tytułu: małe litery, bez polskich znaków, znaki
 * inne niż litery i cyfry jako myślniki, do 60 znaków. `#1234` (Redmine)
 * daje `redmine-1234-…`.
 */
export function orcaWorktreeName(key: string, title: string): string {
  const head = key.startsWith("#") ? `redmine-${key.slice(1)}` : key;
  const slug = fold(`${head} ${title}`)
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
  return slug || "worktree";
}

/** Prompt w jednej linii bez `"` i `%`, do 4000 znaków (`orca::clean_prompt`). */
export function orcaCleanPrompt(text: string): string {
  const flat = text
    .replace(/%/g, "")
    .replace(/"/g, "'")
    .split(/\s+/)
    .filter(Boolean)
    .join(" ");
  return [...flat].slice(0, 4000).join("");
}

function safe(value: string, what: string, max: number): string {
  const v = value.trim();
  if (!v) throw new Error(`${what}: pusta wartość.`);
  // eslint-disable-next-line no-control-regex
  if ([...v].length > max || /[\u0000-\u001f\u007f"%]/.test(v))
    throw new Error(`${what}: niedozwolony znak albo zbyt długa wartość.`);
  return v;
}

/** Argumenty `worktree create --json …` (`orca::args_create`). */
export function orcaCreateArgs(req: OrcaCreateRequest): string[] {
  const args = [
    "worktree",
    "create",
    "--json",
    "--name",
    safe(req.name, "Nazwa worktree", 100),
    "--repo",
    `path:${safe(req.repo, "Repozytorium", 1024)}`,
  ];
  if (req.baseBranch?.trim())
    args.push("--base-branch", safe(req.baseBranch, "Gałąź bazowa", 200));
  if (req.reference?.trim())
    args.push("--reference", safe(req.reference, "Zgłoszenie", 300));
  const agent = req.agent?.trim();
  if (agent) {
    if (agent !== "claude" && agent !== "codex")
      throw new Error("Agent musi być claude albo codex.");
    args.push("--agent", agent);
    const prompt = orcaCleanPrompt(req.prompt ?? "");
    if (prompt) args.push("--prompt", prompt);
  }
  if (req.activate) args.push("--activate");
  return args;
}

/** Argument w poleceniu: w cudzysłowie, gdy ma spację lub znak specjalny. */
function displayArg(arg: string): string {
  return /^[\p{L}\p{N}\-_.:/\\#@+=,]+$/u.test(arg)
    ? arg
    : `"${arg.replace(/"/g, '\\"')}"`;
}

/** Polecenie do podglądu (`orca::command_line`). */
export function orcaCommandLine(args: string[], program = "orca.cmd"): string {
  return [program, ...args.map(displayArg)].join(" ");
}
