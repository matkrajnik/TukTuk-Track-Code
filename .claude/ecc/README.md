# Zestaw ECC dla to-do-track

Wybrane pliki z ECC (`ecc-universal` 2.2.3, commit `90606c83`) dla projektu
`C:\Projekty\Private\to-do-track` (Tauri 2: Rust + Vue 3 + TypeScript + Vite + Playwright).
Katalog `.claude/` ma układ docelowy i ma zastąpić dotychczasową konfigurację agentów projektu;
nie uwzględnia istniejących plików w to-do-track ani funkcji wbudowanych w Claude Code.
W plikach `.md` usunięto prefiks `ecc:`, tak jak robi to instalator `--target claude-project`.

## Zawartość (69 plików)

| Rodzaj | Pliki |
|--------|-------|
| Reguły (`.claude/rules/ecc/`) | `common/`, `rust/`, `vue/`, `typescript/` |
| Agenci językowi | `rust-reviewer`, `rust-build-resolver`, `vue-reviewer`, `typescript-reviewer` |
| Agenci ogólni | `planner`, `architect`, `code-architect`, `code-explorer`, `code-reviewer`, `code-simplifier`, `silent-failure-hunter`, `comment-analyzer`, `pr-test-analyzer`, `type-design-analyzer`, `performance-optimizer`, `a11y-architect`, `security-reviewer`, `build-error-resolver`, `refactor-cleaner`, `doc-updater` |
| Komendy językowe | `/rust-review`, `/rust-build`, `/vue-review` |
| Komendy ogólne | `/plan`, `/feature-dev`, `/code-review`, `/review-pr`, `/build-fix`, `/refactor-clean`, `/save-session`, `/resume-session`, `/checkpoint`, `/update-docs`, `/update-codemaps` |
| Skille | `rust-patterns`, `rust-testing`, `vue-patterns`, `vite-patterns`, `e2e-testing`, `accessibility`, `security-review`, `make-interfaces-feel-better`, `coding-standards` |

Zależności komend są w zestawie: `/plan` → `planner`; `/feature-dev` → `code-explorer`,
`code-architect`, `code-reviewer`; `/review-pr` → `code-reviewer`, `code-simplifier`,
`comment-analyzer`, `pr-test-analyzer`, `silent-failure-hunter`, `type-design-analyzer`;
`/update-codemaps` → `doc-updater`.

## Celowo pominięte

- `/rust-test`, `/test-coverage`: wymuszają próg pokrycia (80%, `cargo-llvm-cov`).
- C#: w projekcie nie ma kodu C#.
- Inny stack: `database-reviewer`, `database-migrations` (PostgreSQL; projekt ma SQLite),
  `frontend-a11y`, `frontend-patterns` (React/Next.js), `error-handling` (TS/Python/Go),
  `e2e-runner` (Vercel Agent Browser), `nuxt4-patterns`, `ui-to-vue`.
- `/security-scan`: wymaga zewnętrznego narzędzia AgentShield.
- Moduły ECC poza `hooks-runtime` (instalator dokłada wtedy wszystkich agentów i reguły wszystkich języków).

## Uwagi

- `rules/ecc/common/` (~19 KB) ładuje się w każdej sesji (bez `paths:`) i narzuca m.in. commity
  `feat:`/`fix:` (`git-workflow.md`) oraz 80% pokrycia i TDD (`testing.md`).
- Reguły językowe mają frontmatter `paths:` (`**/*.rs`, `**/*.vue`, `**/*.ts`), więc ładują się
  tylko przy pracy na tych plikach.
- `/code-review` z ECC ma tę samą nazwę co wbudowana komenda Claude Code i ją przesłoni.
- `/save-session` i `/resume-session` zapisują stan w `~/.claude/session-data/`, poza repozytorium.
- Agenci i komendy odsyłają w sekcjach „powiązane” do `/rust-test` i `/react-review`, których
  w zestawie nie ma. To tylko odnośniki.
- ECC nie ma skilla do Tauri.

## Hooki

Zainstalowane 2026-10-22 instalatorem (moduł `hooks-runtime`, 284 pliki):

```bash
cd /c/Projekty/Private/to-do-track && node /c/Projekty/Private/ECC/scripts/install-apply.js --target claude-project --modules hooks-runtime --enable-hooks
```

- `.claude/settings.json`: 27 hooków (PreToolUse 10, Stop 8, SessionStart 2, PostToolUse 2,
  PostToolUseFailure 2, UserPromptSubmit, PreCompact, SessionEnd) oraz `"includeCoAuthoredBy": false`.
- `.claude/scripts/` (hooki i biblioteki ECC), `.claude/hooks/`, `.claude/ecc/install-state.json`.
- Zależności runtime (`js-yaml`, `sql.js`, `ajv`, `@iarna/toml`) leżą w `.claude/node_modules/`
  (w `.gitignore`). Po świeżym klonie trzeba je doinstalować:

  ```bash
  npm install --prefix .claude --no-save --no-package-lock --ignore-scripts @iarna/toml@2.2.5 ajv@8.20.0 js-yaml@4.3.2 sql.js@1.14.2
  ```

- Komendy hooków mają wpisaną na stałe ścieżkę `C:\Projekty\Private\to-do-track\.claude`
  (base64 w `settings.json`), więc działają tylko w tym katalogu.
- Hook `pre:observe` szuka skilla `continuous-learning-v2`, którego nie ma w zestawie: kończy się
  kodem 0 i tylko loguje „observe script not found”.
- Zachowanie: `gateguard` żąda faktów (importy, API, schemat) przed pierwszą edycją pliku,
  `block-no-verify` blokuje `git commit --no-verify`, `config-protection` chroni konfigurację,
  na Stop formatowanie i typecheck edytowanych plików JS/TS, powiadomienie, koszty w `~/.claude/metrics/`.
- Wyciszanie: `ECC_HOOK_PROFILE=minimal|standard|strict` (domyślnie `standard`), `ECC_DISABLED_HOOKS`.

## Testy Rust w to-do-track (stan na 2026-10-21)

- 336 testów jednostkowych `#[test]` w modułach `src-tauri/src/*.rs` (~1 na 93 linie), brak `src-tauri/tests/`.
- Playwright testuje frontend na zastępczym IPC (`tests/fixtures/tauri.ts`), więc nie sprawdza Rusta.
- Pokrycia nie zmierzono (brak `cargo-llvm-cov`).
- Luki warte testów (czysta logika):
  - `native_msg.rs`: `encode` / `read_frame` (ramka native messaging, limit 1 MB, urwana ramka) — 0 testów.
  - `db::move_session`: przeniesienie pomiaru między zadaniami w transakcji, ścieżki błędów (kosz,
    zadanie ukończone, odarchiwizowanie) — bez testu.
  - `jira_fields::card_fields_by_task`, `agents::resolve_redmine`, `foreground::find_folder`.
- Pozostałe nieprzetestowane funkcje to głównie kod spinający z systemem i siecią: pętle w tle
  (`jira_move::tick`/`attempt`, `jira::watch_once`, `suggest::refresh`, `reminder::start`), tray,
  skróty, widżet, autostart, HTTP oraz punkty wejścia (`main.rs`, `bin/bridge.rs`).

## Wdrożenie

2026-10-22: zestaw skopiowany z katalogu przygotowawczego `todo-track-agents-prepare/.claude`
do `to-do-track/.claude`, potem zainstalowane hooki (sekcja wyżej).
