# Workflows — wyniki etapu E0 (analiza i decyzje)

Data: 2026-11-05. Dotyczy: `PLAN_WDROZENIA.md`, rozdział 3 (A01–A10) i 10 (E0).
Rozpoznanie statyczne kodu i pomoc lokalnych CLI; bez wywołań modeli i usług zewnętrznych.

## A01 — stan bazowy

- Gałąź `master`, commit `e4a9b77` (wydanie 2.4.0). Historia powiadomień z dzwoneczkiem
  jest już zacommitowana; plan wspominał ją jako niezacommitowaną.
- Nieśledzone na starcie: `docs/` (plan) i `src-tauri/src/workflow_access.rs` (klucz).
- Wersja w manifestach: 2.4.0 (`package.json`, `src-tauri/Cargo.toml`).

## A10 — testy bazowe

`npm run check` przed zmianami E1: zielone w 364 s. Playwright 690 passed, `cargo test --locked`
784 passed / 2 ignored plus testy binarek. Każda późniejsza awaria to regresja modułu.

## A02 — cykl procesów AI (`ai.rs`, `ai_chat.rs`)

| Element | Gdzie | Dla workflow |
| --- | --- | --- |
| Wykrywanie CLI | `ai.rs`: `Engine`, `Cli`, `Detected`, `detect`, listy kandydatów | Użyć bez zmian. |
| Kontrakt procesu | `ai.rs`: `Spec { program, args, stdin, cwd, env }` | Użyć; `cwd` = worktree przebiegu. |
| Proces jednorazowy | `ai.rs`: `Runner`, `Process::run`, `TIMEOUT` 60 s | Nie rozszerzać: sztywny limit 60 s i odczyt całego wyjścia. |
| Strumień | `ai_chat.rs`: `Spawner`, `Child`, `Stream`, `ProcessSpawner` | Wzorzec dla runnera; konfiguracja `Command` jest zdublowana z `Process::run` — w E4 wydzielić wspólny helper. |
| Parsery | `ai_chat.rs`: `parse_claude_line`, `parse_codex_line`, `Parsed`; `ai.rs`: `classify`, `Failure` | Użyć; `short_tool_name` obcina prefiks MCP TukTuka — dla workflow bez tego. |
| Zabijanie drzewa | `ai.rs`: `kill_tree` (`taskkill /T /F`, wynik ignorowany) | Użyć, ale w E4 sprawdzać wynik i potwierdzać koniec procesu. |

Polityka asystenta, której nie przenosić: `TUKTUK_TRACK_AI=1` (hook pomija sesję), katalog AI jako
`cwd`, `--tools ""`, `--setting-sources ""`, puste MCP, `--no-session-persistence`/`--ephemeral`,
pseudonimizacja `ai_privacy`.

**Decyzja:** runtime na wątkach std i `mpsc`, jak w istniejącym kodzie. Tokio nie jest dodawane.

## A03 — baza, backup, restore

- Jedno połączenie `Database(Mutex<Connection>)`; komendy blokują je na czas krótkiej operacji.
  Długie procesy i odczyt strumieni nie mogą trzymać tej blokady (zgodnie z 5.4 planu).
- Schemat bez wersjonowania: `db::init` to idempotentne `CREATE ... IF NOT EXISTS` i `ensure_column`;
  `init` działa też na przywracanej kopii.
- **Decyzja:** moduł ma `workflows::store::init_tables`, wołane na końcu `db::init`. Własna tabela
  `workflow_schema_migrations` zapisuje wersję schematu modułu. Klucze obce bez kaskad na historię.
- Backup kopiuje całą bazę (online backup API), więc tabele workflow są w kopii. Restore zastępuje
  wszystko poza `KEEP_KEYS`. **Decyzja:** znacznik odblokowania `workflows_unlocked` dopisany do
  `KEEP_KEYS` (przywrócenie starej kopii nie blokuje modułu). Artefakty na dysku (logi, diffy)
  nie są w kopii — do rozwiązania w E6; restore musi oznaczyć aktywne przebiegi jako przerwane.
- **E6 (zamknięte):** eksport osadza diffy (skrót SHA-256 sprawdzony, limit 256 MiB) w tabeli
  `workflow_backup_artifacts` samej kopii; baza robocza tej tabeli nie ma; wynik eksportu podaje
  liczbę pominiętych diffów. Restore przyjmuje tylko diffy z dokładną ścieżką
  `workflows/runs/<run>/diff-<hash>.patch`, zgodnym skrótem i wierszem indeksu, usuwa tabelę z kopii
  i zapisuje pliki dopiero po udanej zamianie bazy. Każdy nieskończony przebieg z kopii przechodzi przez silnik
  (`Input::Abandon`) w `Interrupted`, aktywne rezerwacje są zwalniane jako `restored`. Restore jest
  odrzucany przy każdym nieskończonym przebiegu, także `waitingForApproval` (`restore_blocker`,
  sprawdzenie pod blokadą zamiany; podgląd kopii ostrzega z góry); aktualizację blokują tylko
  `queued`/`running`/`cancelling` (`active_work`). Kopia
  z nowszym schematem modułu jest odrzucana przed migracją (`workflows/backup.rs`).

## A04 — punkty integracji UI

- Nawigacja: płaska lista w `AppNav.vue`, typ `View` w tym pliku, lista `views` w `App.vue`
  (bez wpisu widok jest ignorowany przez `navigate`). Grupowanie menu to nowy kod i style.
- Ustawienia: rejestr `settingsSections` w `src/navigation.ts` (etykiety zasilają wyszukiwarkę),
  sekcje w `src/components/settings/*Section.vue`, przełącznik w `SettingsPanel.vue`.
- Wzorzec modułu: `src/inbox.ts` (`desktop ? invoke : podgląd`, localStorage `…-v1`, zdarzenie okna).
- Komponenty i tokeny: `.primary-button`, `.action-button`, `.empty-state`, `ConfirmDialog`, `useModal`,
  zmienne `--surface`, `--line`, `--accent` itd.; motyw przez `data-theme`.

## A05 — korelacja sesji i czasu

Sesje agentów wiążą się z zadaniem po folderze (`agent_bindings`) albo kluczu z gałęzi Git;
`agent_time` liczy czas sesji. Nie ma powiązania sesja → przebieg.
**Decyzja (do E5):** runner workflow ustawia własną zmienną `TUKTUK_TRACK_WORKFLOW=<runId>:<executionId>`;
hook rozpoznaje ją i nie nalicza czasu drugi raz. Czas workflow pochodzi ze zdarzeń runtime.

## A06/A07 — CLI i read-only

Lokalnie: Claude Code 2.1.318, codex-cli 0.163.0. Źródła: `--help` i dokumentacja; bez wywołań modeli.

| Zdolność | Claude Code | Codex |
| --- | --- | --- |
| Tryb wsadowy | `-p` | `codex exec` (prompt z stdin `-`) |
| Strumień JSON | `--output-format stream-json --verbose` | `--json` (JSONL) |
| Wynik ustrukturyzowany | `--json-schema '<json>'` → pole `structured_output` | `--output-schema <plik>`, `-o <plik>` |
| Sesja | `--session-id <uuid>` z góry, `--resume` | `thread.started.thread_id`, `exec resume` (bez `-s`/`-C`!) |
| Model | `--model` | `-m` |
| Katalog | `cwd` procesu | `-C <dir>` |
| Uprawnienia | `--permission-mode`, `--tools`, `--allowedTools`/`--disallowedTools`, `--permission-prompts none` | `-s read-only\|workspace-write` |
| Read-only | **polityka narzędzi CLI, nie sandbox systemu** | **sandbox systemu na Windows** (tu: `[windows] sandbox = "elevated"`; jednorazowa konfiguracja administratora niezweryfikowana) |
| Wyłączenie MCP/ustawień | `--strict-mcp-config`, `--setting-sources`; `--bare` wymaga klucza API (odpada przy subskrypcji) | `--ignore-user-config` (wpływ na MCP i hooki niezweryfikowany) |

**Decyzja read-only dla Claude (do akceptacji użytkownika przed E4):** profil review uruchamia
`--tools Read,Grep,Glob` (bez Bash/PowerShell/Edit/Write), `--permission-mode dontAsk`,
`--permission-prompts none`, `--strict-mcp-config`. Dodatkowo host przed i po kroku liczy skrót stanu
worktree (`git status --porcelain` + diff); zmiana plików kończy krok błędem „naruszenie uprawnień”
i unieważnia review. To kontrola CLI plus wykrywanie po fakcie, nie izolacja systemu plików —
zgodnie z 8.3 planu UI musi to tak nazywać. Codex w roli review dostaje `-s read-only`; preflight
blokuje profil, gdy sandbox Windows nie jest skonfigurowany.

**Środowisko procesu agenta (E4, do potwierdzenia użytkownika):** czyste środowisko plus biała
lista z `runners/env.rs`. Poza zmiennymi systemowymi i katalogami konfiguracji CLI (lista z
planu, krok 5) review rundy 1 dodało proxy (`HTTP(S)_PROXY`, `ALL_PROXY`, `NO_PROXY`), certyfikaty
CA (`NODE_EXTRA_CA_CERTS`, `SSL_CERT_FILE`, `SSL_CERT_DIR`), `CLAUDE_CODE_GIT_BASH_PATH` i
`SystemDrive`, potrzebne na maszynach firmowych. Adres proxy z danymi logowania
(`http://user:hasło@proxy`) nie przechodzi wcale: proxy z uwierzytelnianiem trzeba skonfigurować
w samym CLI. Rozszerzona lista czeka na potwierdzenie użytkownika.

## A08 — edytor grafu

**Decyzja:** Vue Flow (`@vue-flow/core`, licencja MIT, Vue 3, bez zależności od Nuxt) jako kandydat
na canvas w E2. Wersję przypiąć przy instalacji (`=x.y.z`, jak inne zależności). Lista kroków
z edycją klawiaturą powstaje niezależnie od canvasu i jest pełnoprawnym edytorem.

## A09 — workspace

**Decyzja MVP:** `git worktree add --detach <app-data>/workflows/worktrees/<runId> <baseCommit>`
z kanonicznej ścieżki repozytorium. Odrzucane z komunikatem: ścieżki UNC i sieciowe, katalog bez Git,
repozytorium z submodułami (MVP), niezacommitowane zmiany w źródle (komunikat, bez automatycznego
przenoszenia). Kanonikalizacja przez `std::fs::canonicalize` przed porównaniem rezerwacji.

## Decyzje zbiorcze

| Temat | Decyzja |
| --- | --- |
| Podział | crate `src-tauri/crates/workflow-core` (graf, walidacja, szablon; później silnik) + host `src-tauri/src/workflows/`. |
| Cargo | workspace w `src-tauri/Cargo.toml` z `default-members`, więc istniejące `cargo test`/CI testują core; `cargo fmt` z `--all`. |
| Baza | tabele modułu przez `init_tables` z `db::init`; identyfikatory INTEGER (wyjątek z E3: kroki i zgody przebiegu mają deterministyczne id TEXT z silnika, zob. PLAN_WDROZENIA.md, rozdz. 15); JSON grafu w TEXT. |
| Klucz | jeden stały klucz, w kodzie skrót SHA-256 (`workflow_access.rs`); znacznik w `settings`. |
| Pętla | jedyny cykl: warunek z `repairLimit` 1–3 + kroki agentów; po limicie przebieg `Failed`/`ReviewLimitExceeded`. |
| Async | wątki std + `mpsc`, bez tokio. |

## Otwarte blokery

1. ~~Akceptacja przez użytkownika modelu read-only dla Claude~~ **zaakceptowane 2026-11-05.**
   Zestaw: Claude `--tools Read,Grep,Glob`, `--permission-mode dontAsk`, `--permission-prompts none`,
   `--strict-mcp-config` (pusty `--mcp-config`); wykrywanie zmian worktree przed krokiem i po nim;
   w UI nazwa „kontrola CLI”, nie izolacja systemu plików; Codex review `-s read-only`, preflight
   blokuje profil Codexa bez skonfigurowanego sandboxa.
   Decyzje projektowe E4 (obowiązują, dopóki użytkownik ich nie zmieni):
   (a) nowy crate `crates/agent-cli` jako wspólna warstwa procesu i parserów, wyjątek od układu
   z rozdz. 6; (b) Claude z profilem `workspaceWrite` jest w MVP blokowany w preflighcie, bo nie ma
   mechanizmu ograniczenia zapisu do workspace (8.3); implementację wykonuje Codex.
   **Zmiana (b), decyzja użytkownika 2026-11-06:** Claude może działać w krokach z zapisem przez
   `--restricted` (Claude Code 2.1.319+, zweryfikowane lokalnie w `claude --help`): bez narzędzi
   uruchamiających kod i WebFetch, bez ustawień user/project/local, narzędzia plików tylko w
   katalogu roboczym (cwd, bez `--add-dir`), bez `bypassPermissions`; zestaw `--tools
   Read,Grep,Glob,Edit,Write` (świadomie bez Bash), `--permission-mode acceptEdits`,
   `--permission-prompts none`. Starsza wersja jest blokowana w preflighcie. Read-only bez zmian.
2. Weryfikacja konfiguracji sandboksa Codexa na tej maszynie (test ręczny w E4).
3. Zachowanie Claude Code na Windows po `taskkill /T /F` (test procesów zastępczych w E4).
