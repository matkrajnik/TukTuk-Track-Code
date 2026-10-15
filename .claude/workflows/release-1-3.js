export const meta = {
  name: 'release-1-3',
  description: 'Realizuje PLAN_1.3.md etap po etapie: implementacja, weryfikacja testami, przegląd, poprawki, commit, wydania 1.3–1.6',
  whenToUse: 'Pełna delegacja prac nad Todo Track 1.3–1.6 według PLAN_1.3.md (etapy 1–6; etap 0 robi użytkownik ręcznie). args: { from?: "T1", only?: ["Q1","Q2"], until?: "R13", maxFixRounds?: 3 }',
  phases: [
    { title: 'Etap 1: Testy', detail: 'wspólne dane testowe, luki w pokryciu, zrzuty wzorcowe, npm run check' },
    { title: 'Etap 2: Powiadomienia', detail: 'przyciski i kliknięcia w toastach, wydanie 1.3.0' },
    { title: 'Etap 3: Szybkie dodawanie', detail: 'składnia, szablony zadań, wydanie 1.4.0' },
    { title: 'Etap 4: Agenci', detail: 'gałąź git i klucz Jiry, propozycje przełączenia, wydanie 1.5.0' },
    { title: 'Etap 5: Raport miesięczny', detail: 'święta, zakładka Miesiąc, eksport CSV/XLSX' },
    { title: 'Etap 6: Skupienie', detail: 'tryb skupienia, wydanie 1.6.0' },
  ],
}

// ---------------------------------------------------------------------------
// Lista zadań. Kolejność jest ścisła: jeden implementator naraz w jednym
// drzewie roboczym, bo funkcje dotykają store.ts, main.rs, notify.rs i ustawień.
// Równolegle działają tylko agenci czytający (przegląd).
// Etap 0 z planu (instalacja 1.2.0, toasty, autostart, tagi) wymaga człowieka
// przy komputerze, więc go tu nie ma.
// ---------------------------------------------------------------------------
const TASKS = [
  // --- Etap 1 ---
  {
    id: 'T1', phase: 'Etap 1: Testy', title: 'Wspólne dane testowe, luki w pokryciu, npm run check',
    section: 'Etap 1 — siatka testów, punkty 2, 3 i 5',
    scope: 'Fixture tests/fixtures/seed.ts (tydzień pracy z projektami, szacunkami, notatkami, zadaniem z Jiry, cyklem i koszem) do użycia w nowych testach; istniejących testów nie przepisuj. Nowe testy e2e: import nowych zgłoszeń z Jiry (pierwsze sprawdzenie tylko zapamiętuje, drugie dodaje do „Do zaplanowania”, zgłoszenie w koszu nie jest dublowane), szybkie przełączanie (Ctrl+K, strzałki, Enter zatrzymuje poprzedni pomiar), baner bezczynności (obie odpowiedzi i zapomnienie przerwy przy pracy agenta), przywracanie kopii w podglądzie. Skrypt npm run check uruchamiający build, Playwright, cargo fmt --check i cargo test --locked (działający w Windows, z cargo z %USERPROFILE%\\.cargo\\bin, gdy nie ma go w PATH).',
    done: 'Nowe testy przechodzą; npm run check przechodzi lokalnie; żaden istniejący test nie został zmieniony.',
  },
  {
    id: 'T2', phase: 'Etap 1: Testy', title: 'Zrzuty wzorcowe widoków i CI',
    section: 'Etap 1 — siatka testów, punkty 1 i 4',
    scope: 'tests/visual.spec.ts z toHaveScreenshot: Zadania, Dashboard, Plan i historia (Dzień, Tablica tygodnia), Zamknięcie dnia, Jira, Ustawienia, widget; motyw ciemny i jasny; szerokość 760 i 1280. Stały zegar page.clock i dane z tests/fixtures/seed.ts. maxDiffPixelRatio 0.01. Skrypt npm run test:visual:update. Wzorce wygeneruj lokalnie i zacommituj. W .github/workflows/check.yml artefakt playwright-report przy porażce. Uruchom zestaw zrzutów trzy razy z rzędu, żeby wykluczyć miganie; elementy niedeterministyczne (kursor, animacje) wyłącz w teście.',
    done: 'visual.spec.ts przechodzi trzy razy z rzędu bez aktualizacji wzorców; workflow CI poprawny składniowo.',
  },

  // --- Etap 2 ---
  {
    id: 'N1', phase: 'Etap 2: Powiadomienia', title: 'Akcje w powiadomieniach Windows',
    section: 'Etap 2 — akcje w powiadomieniach',
    scope: 'Najpierw potwierdź w źródłach tauri-plugin-notification 2.5.0 (~/.cargo/registry), że na Windows kliknięcia i przyciski nie trafiają do aplikacji; jeśli jednak trafiają, użyj pluginu i opisz to w deviations. Inaczej: notify::show przez tauri-winrt-notification (wersja zgodna z tą w Cargo.lock), typy Action i Message z polem open, on_activated → kanał → wykonanie akcji na AppHandle (Stop, start zadania, pokazanie okna) i zdarzenie navigate do frontendu. Akcje i kliknięcia według tabeli w sekcji planu, w tym powiadomienie o nowych zgłoszeniach z Jiry z „Rozpocznij” (gdy jedno) i „Zaplanuj na dziś”. Obsługa navigate w App.vue (widok, filtr, rozwinięcie zadania, edytor na zakładce Podziel) z odpowiednikiem w trybie podglądu.',
    done: 'Testy Rust: mapowanie reguł na Message z akcjami, nieznane id akcji, „Rozpocznij” na zgłoszeniu w koszu. Playwright: zdarzenie navigate ustawia widok i rozwinięcie. W manualChecks lista przycisków do sprawdzenia po instalacji NSIS.',
  },
  {
    id: 'R13', phase: 'Etap 2: Powiadomienia', title: 'Wydanie 1.3.0', release: '1.3.0',
    section: 'Po każdym etapie',
    scope: 'Wydanie obejmuje etapy 1 i 2 planu.',
    done: '',
  },

  // --- Etap 3 ---
  {
    id: 'Q1', phase: 'Etap 3: Szybkie dodawanie', title: 'Składnia szybkiego dodawania',
    section: 'Etap 3 — Składnia',
    scope: 'Czysta funkcja parse(text, today, settings) w src/quickadd.ts dla wszystkich zapisów z tabeli (klucz Jiry, ~szacunek, @dzień, do:termin, !priorytet, #projekt, /szablon jako token rozpoznany, ale jeszcze bez działania, \\ dosłownie). Chipy pod polem dodawania z usuwaniem tokenu kliknięciem, Tab uzupełnia projekt. Pusty tytuł przy kluczu Jiry bierze nazwę z jira_issues. To samo pole w szybkim przełączaniu: brak wyników → „Utwórz i rozpocznij” (Enter) i „Utwórz” (Shift+Enter). Bez prefiksów zachowanie identyczne jak dziś.',
    done: 'Testy jednostkowe parsera z przypadkami z sekcji (kolejność dowolna, \\, przełom tygodnia i roku, @pon w poniedziałek, @31.02 jako błąd); e2e dodania z pełną składnią i utworzenia z szybkiego przełączania; wszystkie stare testy dodawania zadań zielone.',
  },
  {
    id: 'Q2', phase: 'Etap 3: Szybkie dodawanie', title: 'Szablony zadań',
    section: 'Etap 3 — Szablony',
    scope: 'Tabela task_templates (IF NOT EXISTS) i komendy CRUD w Rust z odpowiednikiem w store.ts. „Zapisz jako szablon” w rozwiniętym kafelku, sekcja Ustawienia → Szablony. Token /nazwa w parserze z Q1 działa: pola {klucz} i {tytuł}, kroki, szacunek, projekt, priorytet; jawne pola z linii wygrywają. Tab uzupełnia nazwę szablonu. Kopia SQLite i przywracanie obejmują szablony.',
    done: 'Rust: zapis i odczyt, unikalna nazwa, szablony w kopii. e2e: zapis szablonu z kafelka i użycie /review WMS-123 z tytułem z Jiry.',
  },
  {
    id: 'R14', phase: 'Etap 3: Szybkie dodawanie', title: 'Wydanie 1.4.0', release: '1.4.0',
    section: 'Po każdym etapie',
    scope: 'Wydanie obejmuje etap 3 planu.',
    done: '',
  },

  // --- Etap 4 ---
  {
    id: 'AG1', phase: 'Etap 4: Agenci', title: 'Rozpoznanie zadania agenta (gałąź git, klucz Jiry)',
    section: 'Etap 4 — Rozpoznanie zadania',
    scope: 'git_branch(cwd) w agents.rs: w górę katalogów do .git, plik .git z gitdir: (worktree, submoduł), odczyt HEAD, odłączony HEAD; bez uruchamiania procesu git; cache 30 s na katalog. Klucz Jiry z gałęzi, a bez niego z nazwy sesji Claude Code. Tabela agent_bindings(folder, task_id) i komendy z odpowiednikiem w store.ts. Kolejność dopasowania z planu. Agent w odpowiedzi agents_status ma branch, key i dopasowane zadanie; kafelek Agenci AI na Dashboardzie je pokazuje.',
    done: 'Testy Rust git_branch (zwykłe repo, worktree, odłączony HEAD, brak repo), wyciąganie klucza, kolejność dopasowania. Playwright: kafelek pokazuje gałąź i zadanie.',
  },
  {
    id: 'AG2', phase: 'Etap 4: Agenci', title: 'Propozycje przełączenia i przypisanie przerwy',
    section: 'Etap 4 — Kiedy proponować oraz Powrót z przerwy',
    scope: 'Reguła w Rust: agent pracuje co najmniej 2 min nad dopasowanym X, timer stoi lub mierzy inne zadanie, para (agent, X) nie odrzucona. Kilka agentów nad różnymi zadaniami → lista. Baner w głównym oknie (wzór IdleBanner), linia w widgecie, powiadomienie z przyciskiem „Przełącz” (mechanizm z N1), gdy główne okno schowane. Odpowiedzi „Przełącz”, „Nie teraz”, „Nie proponuj dla tego folderu” (ustawienie odwracalne w Ustawieniach → Czas pracy, plus wyłącznik całej funkcji). Timer nigdy nie przełącza się sam. Baner bezczynności: trzecia opcja „Przypisz przerwę do X” przez istniejący podział pomiaru.',
    done: 'Rust: reguła 2 minut i pamięć odrzuceń. e2e: baner z jednym i dwoma agentami, „Przełącz” zmienia timer, „Nie proponuj” zapisuje folder, trzecia opcja baneru bezczynności dzieli pomiar.',
  },
  {
    id: 'R15', phase: 'Etap 4: Agenci', title: 'Wydanie 1.5.0', release: '1.5.0',
    section: 'Po każdym etapie',
    scope: 'Wydanie obejmuje etap 4 planu.',
    done: '',
  },

  // --- Etap 5 ---
  {
    id: 'M1', phase: 'Etap 5: Raport miesięczny', title: 'Święta i zakładka Miesiąc',
    section: 'Etap 5 — raport miesięczny (bez eksportu)',
    scope: 'src-tauri/src/holidays.rs: polskie święta ustawowe (stałe i ruchome od Wielkanocy) z odpowiednikiem w TypeScript dla interfejsu i podglądu. Ustawienie „Uwzględniaj święta w celu” (domyślnie włączone); cel dnia, tygodnia i pojemność tablicy tygodnia nie liczą święta jako dnia roboczego. Raporty → zakładka Miesiąc: wybór miesiąca, suma względem celu miesiąca, tabele według projektu i zadania z kluczem Jiry, wykres dni z linią celu.',
    done: 'Rust: święta 2024–2030 zgodne z listą (Rust i TS dają to samo). e2e: zakładka Miesiąc, cel pomniejszony o święto, tablica tygodnia oznacza święto jako wolne.',
  },
  {
    id: 'M2', phase: 'Etap 5: Raport miesięczny', title: 'Eksport CSV i XLSX',
    section: 'Etap 5 — Eksport',
    scope: 'Eksport z zakładki Miesiąc i z „Wybranych dni” przez okno zapisu (dialog plugin jak przy kopii). CSV: UTF-8 z BOM, średnik, przecinek dziesiętny, jeden wiersz na przedział z kolumnami z planu. XLSX przez rust_xlsxwriter: arkusze Przedziały i Podsumowanie, formaty liczb i dat, zamrożony pierwszy wiersz. Trwający pomiar liczony do chwili eksportu z dopiskiem „trwa”. W podglądzie eksport CSV zwraca tekst, XLSX jest niedostępny z komunikatem.',
    done: 'Rust: CSV z BOM i polskimi znakami; XLSX odczytany ponownie przez calamine (dev-dependency) ze zgodnymi sumami. e2e: eksport CSV w podglądzie.',
  },

  // --- Etap 6 ---
  {
    id: 'F1', phase: 'Etap 6: Skupienie', title: 'Tryb skupienia',
    section: 'Etap 6 — tryb skupienia',
    scope: 'Przycisk „Skupienie” na pasku pomiaru i w widgecie, długości bloku i przerwy w ustawieniach. Start bloku uruchamia pomiar, jeśli nie trwa; widget pokazuje odliczanie. W czasie bloku przypomnienie w tle, propozycje agentów i powiadomienia czekają w kolejce i przychodzą po bloku jako jedno zbiorcze. Koniec bloku: powiadomienie z „Przerwa” i „Kolejny blok” (mechanizm z N1). Przerwa domyślnie zatrzymuje pomiar (ustawienie), po niej „Wracasz do X?” z „Rozpocznij”. Tabela focus_blocks, liczba bloków na Dashboardzie. Stop lub zmiana zadania kończy blok jako nieukończony. Koniec bloku liczony z zegara ściennego, więc działa po uśpieniu komputera.',
    done: 'Rust: kolejka powiadomień w bloku, koniec bloku po uśpieniu. e2e z page.clock: start, odliczanie w widgecie, przerwa zatrzymuje pomiar, przerwanie przez Stop.',
  },
  {
    id: 'R16', phase: 'Etap 6: Skupienie', title: 'Wydanie 1.6.0', release: '1.6.0',
    section: 'Po każdym etapie',
    scope: 'Wydanie obejmuje etapy 5 i 6 planu.',
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
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_1.3.md i dlaczego' },
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
Źródło prawdy: PLAN_1.3.md. Przeczytaj go w całości (zwłaszcza „Założenia przyjęte bez pytania”), a potem sekcję swojego zadania. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu w store.ts, dane użytkownika nietykalne).
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
Sekcja planu: "${task.section}" w PLAN_1.3.md.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}

Wykonaj całość zakresu, dopisz testy wymienione w sekcji planu, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit z opisem "${task.id}: ${task.title}" i dwoma zdaniami treści po polsku. Commit kończ linią "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>".
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, manualChecks, blocker.`
}

function releasePrompt(task, notes) {
  return `${COMMON}
ZADANIE ${task.id}: wydanie ${task.release}. ${task.scope}
Wzorzec: commit b189eec (wydanie 1.2.0). Kroki:
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
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff commitów zadania ${task.id} (git log --oneline, git show ${impl.commit || 'HEAD'} i wcześniejsze commity tego zadania) z sekcją "${task.section}" w PLAN_1.3.md i z jego założeniami. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Brakujący odpowiednik w store.ts dla nowej komendy i jakiekolwiek samoczynne przełączenie timera to zawsze must.`,
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
// until: zatrzymaj się po tym zadaniu (np. "R13", żeby wydać tylko 1.3.0).
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
    log(`Wznowienie po naprawie: Workflow({ name: 'release-1-3', args: { from: '${task.id}' } })`)
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
