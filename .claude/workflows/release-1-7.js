export const meta = {
  name: 'release-1-7',
  description: 'Realizuje PLAN_1.7.md zadanie po zadaniu: implementacja, weryfikacja testami, przegląd, poprawki, commit; jedno wydanie 1.7.0 na końcu',
  whenToUse: 'Pełna delegacja prac nad Todo Track 1.7 według PLAN_1.7.md (K1–W4, R17). args: { from?: "K1", only?: ["P1"], until?: "W4", maxFixRounds?: 3 }',
  phases: [
    { title: 'Etap 1: Kalendarz', detail: 'logowanie Microsoft, spotkania, pojemność' },
    { title: 'Etap 2: Planowanie', detail: 'poprawka szacunków, Rozłóż, poranny przegląd, cykle, Czekam na' },
    { title: 'Etap 3: Wgląd', detail: 'czas agentów, rozproszenie, przegląd tygodnia, okna, rozszerzenie przeglądarki' },
    { title: 'Etap 4: Jira', detail: 'pytanie o przesunięcie zgłoszenia do In Progress' },
    { title: 'Wydanie 1.7.0', detail: 'wersja, README, NSIS, tag' },
  ],
}

// ---------------------------------------------------------------------------
// Lista zadań. Kolejność jest ścisła: jeden implementator naraz w jednym
// drzewie roboczym, bo funkcje dotykają store.ts, main.rs, notify.rs i ustawień.
// Równolegle działają tylko agenci czytający (przegląd).
// Testów ręcznych nie ma w planie: rzeczy do sprawdzenia trafiają do NEXT_SESSION.md.
// ---------------------------------------------------------------------------
const TASKS = [
  // --- Etap 1 ---
  {
    id: 'K1', phase: 'Etap 1: Kalendarz', title: 'Logowanie Microsoft i pobieranie spotkań',
    section: 'Etap 1 — K1',
    scope: 'src-tauri/src/calendar.rs: OAuth auth code + PKCE (S256) przez przeglądarkę systemową i jednorazowy serwer 127.0.0.1 z losowym portem, sprawdzenie state, limit 5 min, odświeżanie tokenu, refresh token w keyring (jak token Jiry). Graph /me/calendarView z UTC i stronicowaniem, od poniedziałku poprzedniego tygodnia do +21 dni, co 10 min, przy starcie i po „Odśwież”; tabela calendar_events, zakres zastępowany w jednej transakcji. Ustawienia → Kalendarz (client ID, dzierżawca domyślnie organizations, połącz/rozłącz, e-mail, ostatnie pobranie, błąd, „Pokazuj tytuły spotkań”). Komendy calendar_status/connect/disconnect/refresh/events z odpowiednikiem w store.ts i przykładowymi spotkaniami w tests/fixtures/seed.ts. Bez client ID funkcja wyłączona z komunikatem. Nowe crate’y (np. sha2, base64, rand lub getrandom) z konkretną wersją.',
    done: 'Testy Rust: PKCE (wektor z RFC 7636), parsowanie Graph (stronicowanie, całodniowe, anulowane, odrzucone), zastąpienie zakresu, zły state. Playwright: sekcja Kalendarz w podglądzie. Nie da się zalogować do prawdziwego Microsoftu — opisz w manualChecks.',
  },
  {
    id: 'K2', phase: 'Etap 1: Kalendarz', title: 'Spotkania w planie i pojemności',
    section: 'Etap 1 — K2',
    scope: 'Wspólna funkcja pojemności dnia (cel minus nienakładające się spotkania busy/oof/workingElsewhere/tentative, całodniowe oof = wolne) w TS i w Rust tam, gdzie backend liczy cel (tray, powiadomienie o celu). Tor „Spotkania” na osi dnia, bloki planu omijają spotkania (przesunięcie lub podział). Tablica tygodnia: pojemność pomniejszona o spotkania, osobny kolor w pasku i podpowiedź. Zadania → Dzisiaj: nadchodzące spotkania. Zamknięcie dnia: sekcja Spotkania z „Dodaj jako czas” otwierającym ręczne dodanie czasu z godzinami spotkania (ostrzeżenie przy nakładaniu, nic nie dodaje się samo).',
    done: 'Testy pojemności (nakładanie, przez północ, całodniowe oof, tentative). Playwright: tor spotkań, pojemność na tablicy, „Dodaj jako czas”. Zrzuty wzorcowe zaktualizowane świadomie i wymienione w deviations.',
  },

  // --- Etap 2 ---
  {
    id: 'P1', phase: 'Etap 2: Planowanie', title: 'Szacunki z poprawką i „Rozłóż”',
    section: 'Etap 2 — P1',
    scope: 'correction(project) w src/estimates.ts według założeń planu (progi 5 zadań, 90 dni, obcięcie 0,5–3). Podpowiedź „zwykle ~X” w chipie szybkiego dodawania, polu szacunku i karcie tablicy. Ustawienie „Planuj z poprawką szacunków” (domyślnie wyłączone) używane w pojemności, osi planu i „Rozłóż”. Przycisk „Rozłóż” na tablicy tygodnia: algorytm zachłanny z planu, podgląd z „Zastosuj”/„Anuluj” i listą „Nie zmieściło się”, zapis w jednej transakcji, pomija zablokowane i cykliczne.',
    done: 'Testy jednostkowe mnożnika i algorytmu (termin, priorytet, przeładowanie, dzień wolny, zadanie większe niż dzień). e2e: podgląd i zastosowanie.',
  },
  {
    id: 'P2', phase: 'Etap 2: Planowanie', title: 'Poranny przegląd',
    section: 'Etap 2 — P2',
    scope: 'Widok „Przegląd dnia” (Plan i historia → Rano, przycisk na Dashboardzie): niedokończone z poprzedniego dnia roboczego z akcjami i „Wszystkie na dziś”, terminy, nowe z Jiry od ostatniego przeglądu, dziś (spotkania, plan, pojemność). „Zaczynam dzień” zapisuje morningReviewDay. Powiadomienie „Przegląd dnia” po godzinie początku dnia w dzień roboczy bez przeglądu, raz dziennie, przez kolejkę trybu skupienia; wyłącznik w Ustawieniach → Powiadomienia.',
    done: 'Rust: reguła powiadomienia (dzień wolny, święto, już przejrzane). e2e: przeniesienie niedokończonych i „Zaczynam dzień”.',
  },
  {
    id: 'P3', phase: 'Etap 2: Planowanie', title: 'Cykle miesięczne i „Czekam na”',
    section: 'Etap 2 — P3',
    scope: 'repeat_rule w task_details (serde default; stare repeat_days bez zmian): co N tygodni, miesięcznie w dniu X, ostatni dzień roboczy miesiąca (ze świętami), n-ty dzień tygodnia w miesiącu; wybór w kafelku; generowanie w recurrence.rs z odpowiednikiem w store.ts. „Czekam na”: waiting_since, grupa „Czekam na” w Zadaniach z „od N dni” i „Odblokuj”, powiadomienie po N dniach roboczych (domyślnie 3) z „Odblokuj” i „Przypomnij jutro”, lista statusów Jiry oznaczających czekanie w ustawieniach i propozycje na kafelku (oznaczyć / odblokować), nic nie zmienia się samo.',
    done: 'Rust: reguły cykli na przełomach miesięcy i lat, 31. w lutym, ostatni dzień roboczy ze świętem, zgodność repeat_days, reguła powiadomienia. e2e: grupa „Czekam na” i propozycja ze statusu Jiry.',
  },

  // --- Etap 3 ---
  {
    id: 'W1', phase: 'Etap 3: Wgląd', title: 'Czas agentów jako osobny tor',
    section: 'Etap 3 — W1',
    scope: 'Tabela agent_sessions zapisywana przez pętlę agentów (łączenie przerw < 2 min, zamknięcie przy utracie aktywności i zamknięciu aplikacji, task_id z dopasowania, uzupełnienie po przypisaniu folderu za 7 dni wstecz, czyszczenie > 400 dni). Tor „Agenci” na osi dnia; kolumny „Agent” i „Równolegle” w rozwiniętym kafelku i w zakładce Miesiąc. Bez wpływu na cel, raport TS i eksport. Odpowiednik w store.ts z danymi w seed.ts.',
    done: 'Rust: łączenie, zamknięcie, uzupełnienie task_id, czas równoległy. e2e: tor na osi i kolumna w Miesiącu.',
  },
  {
    id: 'W2', phase: 'Etap 3: Wgląd', title: 'Rozproszenie i przegląd tygodnia',
    section: 'Etap 3 — W2',
    scope: 'Wskaźniki w src/report.ts (przełączenia, mediana i najdłuższy nieprzerwany pomiar, bloki skupienia). Karta „Rozproszenie” na Dashboardzie. Plan i historia → Tydzień → zakładka „Przegląd” (czas kontra cel i spotkania, plan kontra wykonanie, rozjazdy szacunków, czas agentów, rozproszenie dzień po dniu, „Wnioski” w week_notes). Powiadomienie „Przegląd tygodnia” w ostatni dzień roboczy o ustawionej godzinie (domyślnie 15:00), z wyłącznikiem.',
    done: 'Testy wskaźników (przerwa, północ, powrót do tego samego zadania). Rust: reguła powiadomienia ze świętem. e2e: zakładka Przegląd i zapis wniosków.',
  },
  {
    id: 'W3', phase: 'Etap 3: Wgląd', title: 'Podpowiedzi z aktywnego okna',
    section: 'Etap 3 — W3',
    scope: 'Ustawienie (domyślnie wyłączone). Co 5 s tytuł i proces okna na pierwszym planie przez extern "system" (bez nowych bibliotek), tylko w pamięci. Rozpoznanie klucza Jiry z tytułu i folderu z tytułu VS Code/Cursor/JetBrains → gałąź → klucz → zadanie (agents::resolve). Propozycje przez istniejący mechanizm z 1.5 ze źródłem „Okno” (reguła 2 minut, „Nie teraz”, wyciszenie, kolejka skupienia). Zaprojektuj rozpoznanie tak, żeby W4 mogło podać sygnał z przeglądarki do tego samego mechanizmu.',
    done: 'Rust: rozpoznanie z przykładowych tytułów i wspólna reguła 2 minut. e2e: baner ze źródłem „Okno” przez zastępcze IPC.',
  },
  {
    id: 'W4', phase: 'Etap 3: Wgląd', title: 'Rozszerzenie przeglądarki (Chrome i Edge)',
    section: 'Etap 3 — W4',
    scope: 'browser-extension/ (Manifest V3, czysty JS, stały identyfikator przez key w manifeście, klucz prywatny poza repo i w .gitignore), filtr domen (domyślnie *.atlassian.net) w opcjach. Host native messaging src-tauri/src/bin/bridge.rs → nazwany potok do działającej aplikacji; rejestracja w HKCU dla Chrome i Edge przy włączonym ustawieniu, usunięcie przy wyłączeniu; bridge dołączony do instalatora NSIS (sprawdź, że tauri build nadal przechodzi). Rozpoznanie /browse/KEY, selectedIssue=KEY, /issues/KEY, klucz w tytule; sygnał do mechanizmu z W3 ze źródłem „Przeglądarka”, pierwszeństwo przed tytułem okna, ważność 30 s. Ustawienia → Czas pracy: wyłącznik i stan połączenia. README: instalacja rozszerzenia.',
    done: 'Rust: rozpoznanie adresów, ramka native messaging (długość LE + JSON, limit 1 MB), wygasanie sygnału, rejestracja na testowej gałęzi rejestru. Playwright: czyste funkcje JS rozszerzenia. W manualChecks: załadowanie rozszerzenia i połączenie z zainstalowaną aplikacją.',
  },

  // --- Etap 4 ---
  {
    id: 'J1', phase: 'Etap 4: Jira', title: 'Przesunięcie zgłoszenia przy starcie pracy',
    section: 'Etap 4 — J1',
    scope: 'Przy starcie pomiaru zadania z kluczem Jiry (każda droga startu) i statusie w kategorii new: pytanie „Przesunąć KEY do In Progress?” (baner, linia w widgecie, powiadomienie przy schowanym oknie z przyciskiem „Przesuń”, kolejka trybu skupienia), pomiar startuje od razu, nic nie przesuwa się samo. Wybór przejścia z GET /rest/api/3/issue/{key}/transitions (kategoria indeterminate, preferowana nazwa z ustawień, inaczej lista do wyboru), POST transitions, odświeżenie statusu w jira_issues. „Przesuń w Jirze…” w rozwiniętym kafelku i propozycja przy ukończeniu zadania. Ustawienia → Jira: wyłącznik (domyślnie włączony) i nazwa statusu; „Nie pytaj dla tego zadania” w task_details. Odpowiednik w store.ts z symulowanymi przejściami. Wykorzystaj istniejący klient i token Jiry z jira.rs.',
    done: 'Rust: wybór przejścia, brak pytania dla in progress/done, „Nie pytaj”, błędy 400/403, parsowanie transitions. e2e z symulowaną Jirą: start → baner → „Przesuń” zmienia status; „Nie pytaj”; przesunięcie przy ukończeniu.',
  },

  // --- Wydanie ---
  {
    id: 'R17', phase: 'Wydanie 1.7.0', title: 'Wydanie 1.7.0', release: '1.7.0',
    section: 'Wydanie 1.7.0 (R17)',
    scope: 'Wydanie obejmuje wszystkie zadania z PLAN_1.7.md (K1–W4 i J1). README „Nowości 1.7” musi zawierać instrukcję rejestracji aplikacji w Entra ID i instalacji rozszerzenia przeglądarki.',
    done: '',
  },
]

// ---------------------------------------------------------------------------
// Schematy wyników
// ---------------------------------------------------------------------------
const IMPL = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'blocked'] },
    summary: { type: 'string' },
    commit: { type: 'string' },
    filesChanged: { type: 'array', items: { type: 'string' } },
    testsAdded: { type: 'array', items: { type: 'string' } },
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_1.7.md i dlaczego' },
    manualChecks: { type: 'string', description: 'Co wymaga ręcznego sprawdzenia w natywnej aplikacji lub po instalacji NSIS' },
    blocker: { type: 'string' },
  },
  required: ['status', 'summary', 'filesChanged'],
}
const VERIFY = {
  type: 'object',
  properties: {
    ok: { type: 'boolean' },
    build: { type: 'boolean' },
    playwright: { type: 'boolean' },
    cargoTest: { type: 'boolean' },
    cargoFmt: { type: 'boolean' },
    playwrightCount: { type: 'number' },
    rustCount: { type: 'number' },
    failures: { type: 'string', description: 'Pełne komunikaty błędów, nazwy testów, fragmenty logu' },
    uncommitted: { type: 'boolean', description: 'git status pokazuje niezacommitowane zmiany' },
  },
  required: ['ok', 'build', 'playwright', 'cargoTest', 'cargoFmt', 'failures', 'uncommitted'],
}
const REVIEW = {
  type: 'object',
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          severity: { type: 'string', enum: ['must', 'should', 'nit'] },
          file: { type: 'string' },
          line: { type: 'number' },
          summary: { type: 'string' },
          fix: { type: 'string' },
        },
        required: ['severity', 'file', 'summary', 'fix'],
      },
    },
  },
  required: ['findings'],
}

// ---------------------------------------------------------------------------
// Wspólne instrukcje
// ---------------------------------------------------------------------------
const COMMON = `
Projekt: C:\\Projekty\\Private\\to-do-track (Tauri 2 + Rust + Vue 3, interfejs po polsku).
Źródło prawdy: PLAN_1.7.md. Przeczytaj go w całości (zwłaszcza „Założenia przyjęte bez pytania”), a potem sekcję swojego zadania. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu w store.ts, dane użytkownika nietykalne).
Zasady:
- Każda nowa komenda Tauri ma odpowiednik w trybie podglądu w src/store.ts (localStorage), bo testy Playwright działają bez Rusta.
- Nowe tabele SQLite tylko przez CREATE TABLE IF NOT EXISTS przy starcie; bez niszczących migracji istniejących tabel.
- Timer nigdy nie przełącza się sam: agenci, powiadomienia i tryb skupienia tylko proponują.
- Nowe teksty interfejsu po polsku. Zachowaj istniejące etykiety aria, bo testy ich używają.
- Nie zmieniaj ani nie usuwaj istniejących testów, chyba że sekcja planu tego wymaga; wtedy opisz to w deviations. Gdy zmiana interfejsu świadomie zmienia wygląd, zaktualizuj zrzuty wzorcowe (npm run test:visual:update, gdy już istnieje) i wymień to w deviations.
- Nie dotykaj bazy użytkownika w %APPDATA%\\pl.todotrack.desktop ani zainstalowanej aplikacji w C:\\Programy\\Todo Track. Nie instaluj NSIS.
- Nie pushuj do zdalnego repozytorium i nie wypychaj tagów. Commity lokalne na bieżącej gałęzi.
- Polecenia: npm run build (vue-tsc + vite), npm test (Playwright), cargo test --manifest-path src-tauri/Cargo.toml --locked, cargo fmt --manifest-path src-tauri/Cargo.toml. cargo zwykle NIE jest w PATH: dodaj %USERPROFILE%\\.cargo\\bin (w Git Bash: export PATH="$HOME/.cargo/bin:$PATH").
- Nowe zależności Cargo lub npm dodawaj z konkretną wersją i zaktualizuj pliki lock; --locked musi przechodzić.
`

function implementPrompt(task) {
  return `${COMMON}
ZADANIE ${task.id}: ${task.title}
Nie zmieniaj numeru wersji — wersja 1.7.0 powstaje dopiero w R17. README i NEXT_SESSION.md aktualizuje dopiero R17, ale opisz zmiany dokładnie w summary.
Sekcja planu: "${task.section}" w PLAN_1.7.md.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}

Wykonaj całość zakresu, dopisz testy wymienione w sekcji planu, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit z opisem "${task.id}: ${task.title}" i dwoma zdaniami treści po polsku. Commit kończ linią "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>".
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, manualChecks, blocker.`
}

function releasePrompt(task, notes) {
  return `${COMMON}
ZADANIE ${task.id}: wydanie ${task.release}. ${task.scope}
Wzorzec: commit 4b88e04 (wydanie 1.6.0). Kroki:
1. Wersja ${task.release} w package.json, package-lock.json, src-tauri/Cargo.toml, src-tauri/Cargo.lock i src-tauri/tauri.conf.json.
2. README: sekcja „Nowości ${task.release.replace(/\.0$/, '')}” nad dotychczasowymi nowościami, opisująca to, co faktycznie zrobiono (git log od poprzedniego wydania), bez obietnic; zaktualizuj wzmiankę o wersji w „Instalacja i aktualizacje” oraz liczby testów, jeśli README je podaje. NEXT_SESSION.md: nowy wpis „Stan po ${task.release}” na górze z wynikami weryfikacji, odstępstwami i listą rzeczy do ręcznego sprawdzenia po instalacji:
${notes || '(brak zgłoszonych)'}
3. Pełna weryfikacja: npm run build, npm test, cargo fmt --check, cargo test --locked.
4. npm run tauri build -- -- --locked. Skopiuj src-tauri/target/release/bundle/nsis/Todo Track_${task.release}_x64-setup.exe do releases/. Starych instalatorów z releases/ nie usuwaj.
5. Commit "${task.release}: <krótki opis>" z linią Co-Authored-By jak w pozostałych commitach i lokalny tag v${task.release} (bez pushowania).
Zwróć wynik w schemacie IMPL; w filesChanged wymień też instalator.`
}

function verifyPrompt(task) {
  return `${COMMON}
Jesteś weryfikatorem po zadaniu ${task.id} (${task.title}). Nie ufaj deklaracjom implementatora. Uruchom od zera, w tej kolejności, i zapisz pełne wyjście:
1. git status --short (czy są niezacommitowane zmiany; pliki ignorowane i .claude/launch.json się nie liczą)
2. npm run build
3. npx playwright install chromium (tylko jeśli brak przeglądarki), potem npm test
4. cargo test --manifest-path src-tauri/Cargo.toml --locked
5. cargo fmt --manifest-path src-tauri/Cargo.toml --check
Nie poprawiaj niczego. Zwróć ok=true tylko gdy wszystkie kroki przeszły i nie ma niezacommitowanych zmian. W failures wklej dokładne komunikaty błędów i nazwy testów, które padły. Podaj liczbę testów Playwright i Rust.`
}

function fixPrompt(task, failures, round) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}) ma czerwoną weryfikację (runda poprawek ${round}). Wynik weryfikatora:
${failures}

Znajdź przyczynę i napraw ją w kodzie lub w nowych testach. Nie wyłączaj i nie kasuj testów. Test zrzutu wzorcowego, który pada przez świadomą zmianę wyglądu z tego zadania, możesz zaktualizować; przez przypadkową zmianę — napraw kod. Uruchom ponownie polecenia, które padły, oraz cargo fmt. Zrób commit "${task.id}: poprawki po weryfikacji (${round})" z linią Co-Authored-By jak w pozostałych commitach.
Zwróć wynik w schemacie IMPL (status done, gdy wszystko przechodzi; blocked z opisem, gdy nie).`
}

function reviewPrompt(task, lens, impl) {
  const lenses = {
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff commitów zadania ${task.id} (git log --oneline, git show ${impl.commit || 'HEAD'} i wcześniejsze commity tego zadania) z sekcją "${task.section}" w PLAN_1.7.md i z jego założeniami. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Brakujący odpowiednik w store.ts dla nowej komendy i jakiekolwiek samoczynne przełączenie timera to zawsze must.`,
    bugs: `Soczewka: POPRAWNOŚĆ. Przeczytaj diff commitów zadania ${task.id} (git log --oneline, git show). Szukaj błędów: warunki wyścigu przy jednym aktywnym pomiarze, przedziały przez północ i zmianę czasu, nadpisywanie innych pól JSON w task_details (wzorzec json_set), niespójność między Rust a store.ts, brak walidacji wejścia w komendach, blokowanie wątku w pętlach Rust (odczyt plików, powiadomienia), wycieki nasłuchów Tauri w onUnmounted, dostępność (aria, klawiatura). Zgłaszaj tylko to, co umiesz uzasadnić konkretną ścieżką wykonania.`,
  }
  return `${COMMON}
Jesteś recenzentem po zadaniu ${task.id} (${task.title}). Tylko czytasz, niczego nie zmieniasz i nie uruchamiasz testów.
${lenses[lens]}
Podsumowanie implementatora: ${impl.summary}
Zwróć listę findings z severity: must (błąd lub brak z planu), should (warto poprawić w tym zadaniu), nit (kosmetyka). Pusta lista, gdy nic nie znalazłeś.`
}

function applyReviewPrompt(task, findings) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}): przegląd zgłosił uwagi do wdrożenia. Wdróż wszystkie must i should; nit pomiń, chyba że są trywialne.
${JSON.stringify(findings, null, 2)}
Jeśli uważasz, że któraś uwaga jest błędna, nie wdrażaj jej i napisz dlaczego w deviations. Po zmianach uruchom npm run build, npm test, cargo test, cargo fmt. Commit "${task.id}: uwagi z przeglądu" z linią Co-Authored-By. Zwróć wynik w schemacie IMPL.`
}

// ---------------------------------------------------------------------------
// Pętla dla jednego zadania: implementacja → weryfikacja → poprawki → przegląd → wdrożenie uwag → weryfikacja
// ---------------------------------------------------------------------------
async function verifyWithFixes(task, maxRounds, stageLabel) {
  for (let round = 1; round <= maxRounds + 1; round++) {
    const v = await agent(verifyPrompt(task), { label: `verify:${task.id}:${stageLabel}:${round}`, phase: task.phase, schema: VERIFY, effort: 'low' })
    if (!v) return { ok: false, failures: 'weryfikator nie zwrócił wyniku' }
    if (v.ok) return v
    if (round > maxRounds) return v
    log(`${task.id}: weryfikacja czerwona (runda ${round}): build=${v.build} playwright=${v.playwright} cargo=${v.cargoTest} fmt=${v.cargoFmt} uncommitted=${v.uncommitted}`)
    const fix = await agent(fixPrompt(task, v.failures + (v.uncommitted ? '\n\nUWAGA: są niezacommitowane zmiany, dokończ commit.' : ''), round), { label: `fix:${task.id}:${stageLabel}:${round}`, phase: task.phase, schema: IMPL })
    if (!fix || fix.status === 'blocked') return { ok: false, failures: fix ? fix.blocker : 'agent poprawek nie zwrócił wyniku' }
  }
  return { ok: false, failures: 'wyczerpano rundy poprawek' }
}

async function runRelease(task, maxFixRounds, notes) {
  log(`▶ ${task.id}: ${task.title}`)
  const rel = await agent(releasePrompt(task, notes), { label: `release:${task.id}`, phase: task.phase, schema: IMPL })
  if (!rel) return { task: task.id, status: 'blocked', blocker: 'agent wydania nie zwrócił wyniku' }
  if (rel.status === 'blocked') return { task: task.id, status: 'blocked', blocker: rel.blocker, summary: rel.summary }
  const v = await verifyWithFixes(task, maxFixRounds, 'release')
  if (!v.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja wydania: ${v.failures}`, summary: rel.summary }
  return { task: task.id, status: 'done', summary: rel.summary, commit: rel.commit, deviations: rel.deviations || '', manualChecks: '', review: 'wydanie bez przeglądu' }
}

async function runTask(task, maxFixRounds) {
  log(`▶ ${task.id}: ${task.title}`)
  const impl = await agent(implementPrompt(task), { label: `impl:${task.id}`, phase: task.phase, schema: IMPL })
  if (!impl) return { task: task.id, status: 'blocked', blocker: 'implementator nie zwrócił wyniku' }
  if (impl.status === 'blocked') return { task: task.id, status: 'blocked', blocker: impl.blocker, summary: impl.summary }

  const v1 = await verifyWithFixes(task, maxFixRounds, 'impl')
  if (!v1.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja po implementacji: ${v1.failures}`, summary: impl.summary }

  // Przegląd dwiema soczewkami równolegle (tylko odczyt, więc bez konfliktów w drzewie roboczym).
  const reviews = (await parallel(['plan', 'bugs'].map(lens => () =>
    agent(reviewPrompt(task, lens, impl), { label: `review:${task.id}:${lens}`, phase: task.phase, schema: REVIEW })
  ))).filter(Boolean)
  const findings = reviews.flatMap(r => r.findings).filter(f => f.severity !== 'nit')
  let reviewSummary = 'przegląd bez uwag'
  if (findings.length) {
    log(`${task.id}: przegląd zgłosił ${findings.length} uwag (${findings.filter(f => f.severity === 'must').length} must)`)
    const applied = await agent(applyReviewPrompt(task, findings), { label: `apply-review:${task.id}`, phase: task.phase, schema: IMPL })
    const v2 = await verifyWithFixes(task, maxFixRounds, 'review')
    if (!v2.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja po przeglądzie: ${v2.failures}`, summary: impl.summary }
    reviewSummary = `${findings.length} uwag wdrożonych; ${applied ? applied.deviations || '' : ''}`
  }
  return {
    task: task.id, status: 'done', summary: impl.summary, commit: impl.commit,
    deviations: impl.deviations || '', manualChecks: impl.manualChecks || '', review: reviewSummary,
  }
}

// ---------------------------------------------------------------------------
// Główna pętla: ściśle sekwencyjnie, zatrzymanie na pierwszej blokadzie.
// args: { from?: id, only?: [ids], until?: id, maxFixRounds?: number }
// until: zatrzymaj się po tym zadaniu (np. "P3", żeby zatrzymać się po etapie 2).
// ---------------------------------------------------------------------------
const opts = args || {}
const maxFixRounds = opts.maxFixRounds || 3
let queue = TASKS
if (opts.only && opts.only.length) queue = TASKS.filter(t => opts.only.includes(t.id))
else if (opts.from) {
  const start = TASKS.findIndex(t => t.id === opts.from)
  if (start >= 0) queue = TASKS.slice(start)
}
if (opts.until) {
  const end = queue.findIndex(t => t.id === opts.until)
  if (end >= 0) queue = queue.slice(0, end + 1)
}
log(`Do wykonania: ${queue.map(t => t.id).join(', ')} (rundy poprawek: ${maxFixRounds})`)

const results = []
let pendingChecks = []
for (const task of queue) {
  phase(task.phase)
  const result = task.release
    ? await runRelease(task, maxFixRounds, pendingChecks.join('\n'))
    : await runTask(task, maxFixRounds)
  results.push(result)
  if (result.status === 'blocked') {
    log(`⛔ ${task.id} zablokowane: ${result.blocker}`)
    log(`Wznowienie po naprawie: Workflow({ name: 'release-1-7', args: { from: '${task.id}' } })`)
    break
  }
  if (task.release) pendingChecks = []
  else if (result.manualChecks) pendingChecks.push(`- ${task.id}: ${result.manualChecks}`)
  log(`✔ ${task.id}: ${result.summary}`)
}

return {
  completed: results.filter(r => r.status === 'done').map(r => r.task),
  blocked: results.find(r => r.status === 'blocked') || null,
  remaining: queue.slice(results.length).map(t => t.id),
  manualChecks: results.filter(r => r.manualChecks).map(r => `${r.task}: ${r.manualChecks}`),
  deviations: results.filter(r => r.deviations).map(r => `${r.task}: ${r.deviations}`),
  results,
}
