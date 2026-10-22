# CLAUDE.md

Wskazówki dla Claude Code w repozytorium TukTuk Track (dawniej Todo Track).

## Projekt

TukTuk Track 2.0.0 to desktopowa aplikacja Windows do planowania pracy i mierzenia czasu zadań:
pomiary, plan dnia i tygodnia, raporty, integracje z Jirą, Redmine, kalendarzem Microsoft
i Ideo Timesheet (TS), asystent AI (przez Claude Code albo Codexa), serwer MCP dla agentów,
rozszerzenie przeglądarki. Dokumentacja i komunikaty są po polsku.

| Część | Gdzie | Uwagi |
|-------|-------|-------|
| Backend | `src-tauri/src/*.rs` (Tauri 2, Rust) | SQLite przez `rusqlite` (`db.rs`), HTTP przez `reqwest`, sekrety w `keyring`. Rust zapisuje początek i koniec sesji pomiaru; frontend tylko wyświetla. |
| Programy dodatkowe | `src-tauri/src/bin/` | `tuktuk-track-bridge` (host native messaging rozszerzenia), `tuktuk-track-hook` (hooki agentów), `tuktuk-track-mcp` (serwer MCP). |
| Frontend | `src/` (Vue 3, `<script setup lang="ts">`, Vite) | Bez Pinii i routera. `store.ts` to cienka warstwa `desktop ? invoke : podgląd`. |
| Typy i reguły wspólne | `src/api/*.ts` | Wspólne dla aplikacji i podglądu. |
| Tryb podglądu | `src/preview/*.ts` | Symulatory w localStorage dla `npm run dev` w przeglądarce. |
| Testy E2E | `tests/*.spec.ts` (Playwright) | Zastępcze IPC Tauri w `tests/fixtures/tauri.ts`: logika z Rust przeniesiona 1:1 do TS. Zmiana tej logiki w Rust wymaga zmiany fixture. |
| Testy Rust | moduły `#[cfg(test)]` w `src-tauri/src`, `src-tauri/tests/` | `naming.rs` pilnuje, by stara nazwa (Todo Track, `todo-track`, `todotrack`) występowała tylko w wyjątkach (`legacy*`, instalator). |
| Rozszerzenie | `browser-extension/` | Chrome i Edge, ramka native messaging w `src-tauri/src/native_msg.rs`. |

Dokumenty: `README.md` (funkcje, uruchomienie, weryfikacja), `RELEASE.md` (wydania, podpis,
updater), `NEXT_SESSION.md` (stan i lista do ręcznego sprawdzenia), `UX_AUDIT.md` (audyt WCAG),
`notes.md` (notatki bieżącego wydania).

## Komendy

```bash
npm ci                     # zależności (Node z .nvmrc: 24 LTS)
npm run dev                # podgląd w przeglądarce: http://127.0.0.1:1420 (bez traya i Tauri)
npm run tauri dev          # aplikacja desktopowa
npm run build              # vue-tsc --noEmit + vite build
npm test                   # Playwright (własny serwer Vite na porcie 1430, PLAYWRIGHT_PORT)
npm run test:visual        # tylko zrzuty wzorcowe (tests/visual.spec.ts)
npm run check              # to samo co CI bez buildu Tauri: build, Playwright, cargo fmt --check, cargo test --locked
npm run tauri build        # instalator NSIS w src-tauri/target/release/bundle/nsis/
```

Rust leży w `src-tauri/`, więc `cargo` zawsze z manifestem:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml   # nie jest częścią CI ani npm run check
```

Gdy `cargo` nie ma w PATH: `%USERPROFILE%\.cargo\bin` (`npm run check` dodaje go sam).

Wydanie (`npm run ship -- X.Y.Z`, `wydanie.cmd`) podpisuje instalator, publikuje, commituje,
taguje i pushuje. Uruchamia je tylko użytkownik (wymaga hasła klucza podpisu); szczegóły w `RELEASE.md`.

CI (`.github/workflows/check.yml`, windows-latest): `npm run build`, Playwright bez `@visual`,
zrzuty wzorcowe, `cargo fmt --check`, `cargo test --locked`, `npm run tauri build -- --no-bundle`.

## Routing: agenci, komendy i skille

Konfiguracja agentów pochodzi z ECC (opis zestawu: `.claude/ecc/README.md`). Reguły
w `.claude/rules/ecc/` ładują się same: `common/` zawsze, `rust/`, `vue/`, `typescript/`
przy plikach `.rs`, `.vue`, `.ts`.

| Sytuacja | Użyj |
|----------|------|
| Nowa funkcja, większa zmiana | `/plan` (agent `planner`), przy decyzjach architektonicznych `architect` |
| Funkcja z rozpoznaniem kodu i projektem | `/feature-dev` (`code-explorer` → `code-architect` → `code-reviewer`) |
| Zmiana w `src-tauri/**/*.rs` | `rust-reviewer` (`/rust-review`), wzorce: skill `rust-patterns` |
| Testy Rust | skill `rust-testing` |
| Błąd `cargo build` / `cargo test` (kompilacja) | `/rust-build` (`rust-build-resolver`) |
| Zmiana w `src/**/*.vue` | `vue-reviewer` (`/vue-review`), wzorce: skill `vue-patterns` |
| Zmiana w `src/**/*.ts` | `typescript-reviewer` |
| Błąd `npm run build` (`vue-tsc`, Vite) | `/build-fix` (`build-error-resolver`), konfiguracja Vite: skill `vite-patterns` |
| Testy Playwright, fixture zastępczego IPC | skill `e2e-testing` |
| Interfejs: dostępność, kontrast, fokus | `a11y-architect`, skill `accessibility` (punkt odniesienia: `UX_AUDIT.md`) |
| Interfejs: dopracowanie wyglądu | skill `make-interfaces-feel-better` |
| Sekrety, tokeny Jira/Redmine/TS, dane z API, native messaging, MCP | `security-reviewer`, skill `security-review` |
| Obsługa błędów (`Result<_, String>`, pętle w tle, HTTP) | `silent-failure-hunter` |
| Przegląd lokalnych zmian | `/code-review` (`code-reviewer`) |
| Przegląd PR | `/review-pr` (`code-reviewer`, `code-simplifier`, `comment-analyzer`, `pr-test-analyzer`, `silent-failure-hunter`, `type-design-analyzer`) |
| Uproszczenie kodu po zmianie | `code-simplifier` |
| Projekt typów (Rust i TS) | `type-design-analyzer` |
| Wydajność | `performance-optimizer` |
| Martwy kod | `/refactor-clean` (`refactor-cleaner`) |
| Dokumentacja, mapy kodu | `/update-docs`, `/update-codemaps` (`doc-updater`) |
| Ogólne zasady jakości kodu | skill `coding-standards` |
| Przerwa w pracy | `/checkpoint`, `/save-session`, `/resume-session` |

Agenci, których nie ma w zestawie (`tdd-guide`, `e2e-runner`, `react-reviewer`, reviewerzy innych
języków) i komendy `/rust-test`, `/test-coverage`, `/react-review`, `/ecc-guide` nie są dostępne.

## Hooki

`.claude/settings.json` uruchamia hooki ECC z `.claude/scripts/hooks/` (m.in. `gateguard`
żąda faktów przed pierwszą edycją pliku, `block-no-verify` blokuje `git commit --no-verify`,
na Stop formatowanie i typecheck plików JS/TS). Zależności hooków są w `.claude/node_modules/`
(poza repozytorium; instalacja: `.claude/ecc/README.md`). Profil: `ECC_HOOK_PROFILE`
(`minimal`, `standard`, `strict`), wyłączanie pojedynczych: `ECC_DISABLED_HOOKS`.

## Zasady projektu

- Frontend działa w obu trybach: desktop (`invoke`) i podgląd (`src/preview`); każda funkcja
  `store.ts` ma odpowiednik w podglądzie. Funkcje wyłączone w podglądzie testuje się przez
  zastępcze IPC w `tests/fixtures/tauri.ts`.
- Zmiany interfejsu mogą zmienić zrzuty wzorcowe (`tests/visual.spec.ts-snapshots`); wzorce
  generuje się świadomie (`npm run test:visual:update` albo workflow CI z `update_snapshots`).
- Stara nazwa Todo Track tylko w wyjątkach pilnowanych przez `src-tauri/tests/naming.rs`.
- Prace i testy nie łączą się z prawdziwymi Jirą, Redmine, TS, kalendarzem ani `claude`/`codex`/`orca`;
  testy używają fixture. Sprawdzenie na prawdziwych usługach robi użytkownik (`NEXT_SESSION.md`).
