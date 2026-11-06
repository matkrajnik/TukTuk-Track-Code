# E4: checklista testu ręcznego na prawdziwych CLI

Dokument dla użytkownika. Agent go nie wykonuje: testy automatyczne używają wyłącznie procesów
zastępczych. Do czasu wykonania tej checklisty odbiór E4 jest **niezweryfikowany na prawdziwych
CLI** (`PLAN_WDROZENIA.md`, E4).

## 1. Przykładowe repozytorium testowe

```powershell
mkdir "C:\tmp\tuktuk wf test\src"
cd "C:\tmp\tuktuk wf test"
git init
```

`src\kalkulator.js`:

```js
// Błąd celowy: powinno być a + b.
function dodaj(a, b) {
  return a - b;
}
module.exports = { dodaj };
```

`README.md`: jedna linia opisu. Potem jeden commit, bez submodułów:

```powershell
git add -A
git commit -m "start"
git status --porcelain   # musi być puste
```

Aplikacja odrzuca repozytorium z niezacommitowanymi zmianami, z submodułami, ścieżkę sieciową
(UNC) i katalog nie będący korzeniem repozytorium.

## 2. Wersje CLI

```powershell
claude --version
codex --version
```

Wpisz wyniki do tabeli w punkcie 6. Przetestowane wersje (E0): Claude Code 2.1.318,
codex-cli 0.163.0. Starsza wersja albo inna seria główna daje `UnsupportedVersion`; nowsza w tej
samej serii jest dozwolona z ostrzeżeniem „niezweryfikowana wersja”.

## 3. Sandbox Codexa i panel „Gotowość agentów”

1. Jednorazowo (administrator) skonfiguruj sandbox Windows w `%USERPROFILE%\.codex\config.toml`
   (albo `%CODEX_HOME%\config.toml`):

   ```toml
   [windows]
   sandbox = "elevated"
   ```

2. W aplikacji: Workflows → Definicje → opublikuj szablon „Analiza → implementacja → review” →
   panel „Gotowość agentów”.
3. Sprawdź: dla Claude (review, analiza) „kontrola CLI (narzędzia tylko do odczytu + wykrywanie
   zmian), nie izolacja systemu plików”; dla Codexa „sandbox Codexa”. Bez konfiguracji powinien
   pojawić się polski powód blokady. Zmień config, kliknij „Sprawdź ponownie”.

Preflight nie sprawdza zalogowania (wymagałoby wywołania modelu); `NotLoggedIn` wychodzi przy
pierwszym kroku.

## 4. Przykładowy przebieg szablonu (aktywna od E5)

Uruchomienie: Workflows → Definicje → „Uruchom” (opublikowana wersja), repozytorium z punktu 1,
monitor w „Uruchomieniach”. Automatyczny odpowiednik na procesach zastępczych:
`sample_repo_full_run_reaches_approval_and_succeeds` (src-tauri/src/workflows/runners/tests.rs).
Uruchom szablon na repozytorium z punktu 1 z instrukcją
„Napraw funkcję dodaj i dodaj test”. Zapisz:

- powstały worktree pod `%APPDATA%\<katalog aplikacji>\workflows\worktrees\<runId>`,
- brak jakichkolwiek zmian w `C:\tmp\tuktuk wf test` (`git status` czysty),
- strumień zdarzeń kroków,
- `session_id` każdego kroku,
- skrót zgody równy skrótowi diffu worktree,
- czy tablica agentów pozostaje pusta dla kroków workflow (brak podwójnego czasu),
- czy powiadomienie o zgodzie prowadzi po kliknięciu do monitora tego przebiegu,
- odrzucenie z komentarzem (przebieg „Odrzucony”, komentarz widoczny przy zgodzie),
- anulowanie w trakcie kroku (kolejny krok nie startuje).

## 5. Testy negatywne (aktywne od E5)

- Review z instrukcją „utwórz plik notatka.txt”: krok kończy się `PermissionViolation`, a review
  jest unieważnione.
- Anulowanie w trakcie kroku Codexa: w Menedżerze zadań nie zostają procesy `codex`, `node`
  ani `cmd` z drzewa kroku.
- Zamknięcie aplikacji w trakcie kroku: Job Object kończy drzewo procesów.
- Zachowanie Claude Code po `TerminateJobObject` i `taskkill /T /F` na prawdziwym CLI (otwarty
  bloker 3 z E0).

## 6. Tabela wyników

| Data | Pozycja | Wersja CLI | Wynik (OK / BŁĄD) | Uwagi |
|------|---------|------------|-------------------|-------|
|      | Preflight Claude |   |   |   |
|      | Preflight Codex (z sandboxem) |   |   |   |
|      | Preflight Codex (bez sandboxa, blokada) |   |   |   |
|      | Przebieg szablonu |   |   |   |
|      | Źródło nietknięte |   |   |   |
|      | Brak podwójnego czasu (tablica agentów pusta dla kroków workflow) |   |   |   |
|      | Powiadomienie o zgodzie prowadzi do przebiegu |   |   |   |
|      | Review zapisuje plik: PermissionViolation |   |   |   |
|      | Anulowanie kroku Codexa |   |   |   |
|      | Zamknięcie aplikacji w trakcie kroku |   |   |   |
