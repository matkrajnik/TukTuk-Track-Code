export const meta = {
  name: 'release-1-0',
  description: 'Realizuje PLAN_1.0.md etap po etapie: implementacja, weryfikacja testami, przegląd, poprawki, commit',
  whenToUse: 'Pełna delegacja prac nad Todo Track 1.0 według PLAN_1.0.md. args: { from?: "A", only?: ["A","C"], maxFixRounds?: 3 }',
  phases: [
    { title: 'Etap 0: Fundamenty', detail: 'git, refaktor App.vue, ustawienia, baza' },
    { title: 'Etap 1: Czas', detail: 'podział przedziału, cel dzienny i tray, skrót Start/Stop' },
    { title: 'Etap 2: Okna i system', detail: 'pamięć okien, autostart, widget, powiadomienia' },
    { title: 'Etap 3: Planowanie', detail: 'kolejność ręczna, oś dnia, tablica tygodnia' },
    { title: 'Etap 4: Raporty', detail: 'TS z wybranych dni, trafność szacunków' },
    { title: 'Etap 5: Wydanie', detail: 'wersje, dokumentacja, pełna weryfikacja, instalator' },
  ],
}

// ---------------------------------------------------------------------------
// Lista zadań. Kolejność jest ścisła: jeden implementator naraz w jednym
// drzewie roboczym, bo każda funkcja dotyka store.ts, main.rs i ustawień.
// Równolegle działają tylko agenci czytający (przegląd).
// ---------------------------------------------------------------------------
const TASKS = [
  {
    id: '0.1', phase: 'Etap 0: Fundamenty', title: 'Git, CI, .nvmrc',
    section: '0.1 Git i CI',
    scope: 'Sprawdź .gitignore, utwórz gałąź release/1.0 z bieżącej, zrób commit "0.3.0 baseline" ze wszystkimi plikami. Dodaj .nvmrc z 22. Przejrzyj .github/workflows i popraw, żeby kroki odpowiadały README (Node 22, cargo test --locked, tauri build --no-bundle). NIE pushuj.',
    done: 'Wszystkie pliki w commicie na gałęzi release/1.0, .nvmrc istnieje, workflow CI poprawny składniowo.',
  },
  {
    id: '0.2', phase: 'Etap 0: Fundamenty', title: 'Podział App.vue i style.css',
    section: '0.2 Podział App.vue',
    scope: 'Wytnij z src/App.vue komponenty i widoki wymienione w tabeli sekcji 0.2 oraz moduły src/report.ts i src/order.ts. Podziel style.css według sekcji. Zero zmian zachowania i zero zmian w istniejących testach.',
    done: 'App.vue poniżej 300 linii; wszystkie 18 testów Playwright przechodzi bez modyfikacji plików w tests/; vue-tsc bez błędów.',
  },
  {
    id: '0.3', phase: 'Etap 0: Fundamenty', title: 'Ustawienia: settings.rs i zakładka Ustawienia',
    section: '0.3 Ustawienia jako jeden mechanizm',
    scope: 'Moduł src-tauri/src/settings.rs z AppSettings i wszystkimi kluczami z tabeli, komendy get_settings/set_settings, zdarzenie settings-changed, src/settings.ts z trybem podglądu, podział zakładki w LibraryView na Kopie i Ustawienia z sekcjami. Usuń app_settings i set_idle_enabled po przepięciu.',
    done: 'Przełącznik bezczynności działa przez nowy mechanizm; test Rust walidacji zakresów; test Playwright zakładki Ustawienia w podglądzie.',
  },
  {
    id: '0.4', phase: 'Etap 0: Fundamenty', title: 'Baza: indeksy, pola Details, notifications_sent, stats.rs',
    section: '0.4 Baza',
    scope: 'Indeksy sessions_task i sessions_started, pola order i plannedStart w planner::Details i store.ts (detailsOf), tabela notifications_sent, moduł stats.rs z day_total_ms liczonym przez chrono Local.',
    done: 'Test Rust day_total_ms z dobą zmiany czasu 25 października 2026 i przedziałem przez północ; stare JSON-y Details czytają się bez błędu.',
  },
  {
    id: 'A', phase: 'Etap 1: Czas', title: 'Podział przedziału',
    section: 'A. Podział przedziału',
    scope: 'db::split_session dla przedziału zakończonego i aktywnego, komenda split_session, odpowiednik w store.ts, zakładka Podziel w SessionEditor, przycisk Podziel na pasku bieżącego pomiaru i na osi dnia.',
    done: 'Testy Rust z sekcji (5 przypadków) i test Playwright podziału aktywnego pomiaru z paska.',
  },
  {
    id: 'C', phase: 'Etap 1: Czas', title: 'Cel dzienny i tygodniowy, tray',
    section: 'C. Cel dzienny i tygodniowy, tray',
    scope: 'Ustawienia target_day_minutes i work_days w UI, paski postępu w Zadaniach, Dashboardzie i Tygodniu, komunikat w Zamknięciu dnia, moduł tray.rs z tooltipem co 5 s, ikona icon-active.ico generowana w make-artwork.ps1, pozycje menu traya.',
    done: 'Testy Rust składania tooltipu; Playwright dla paska postępu i komunikatu w zamknięciu dnia.',
  },
  {
    id: 'L', phase: 'Etap 1: Czas', title: 'Globalny skrót Start/Stop i konfigurowalne skróty',
    section: 'L. Globalny skrót Start/Stop',
    scope: 'Moduł shortcuts.rs, rejestracja z ustawień i ponowna po settings-changed, logika toggle (Stop / Start ostatniego / szybkie przełączanie), komponent ShortcutInput.vue, błędy rejestracji przy polach.',
    done: 'Test Rust wyboru zadania do wznowienia; Playwright dla ShortcutInput.',
  },
  {
    id: 'K', phase: 'Etap 2: Okna i system', title: 'Pamięć pozycji i rozmiaru okna',
    section: 'K. Pozycja i rozmiar okna',
    scope: 'tauri-plugin-window-state z denylist reminder i quick, jawne save_window_state przy CloseRequested, quit z traya i --prepare-update.',
    done: 'cargo build przechodzi; opis ręcznej weryfikacji w notatkach wyniku.',
  },
  {
    id: 'I', phase: 'Etap 2: Okna i system', title: 'Autostart z Windows',
    section: 'I. Autostart',
    scope: 'tauri-plugin-autostart sterowany z Rust, komendy autostart_get/autostart_set z argumentem --hidden, main visible:false i pokazanie w setup bez --hidden, dwa przełączniki w Ustawieniach.',
    done: 'Build przechodzi; Playwright: przełączniki w podglądzie zapisują się do localStorage.',
  },
  {
    id: 'B', phase: 'Etap 2: Okna i system', title: 'Mini-widget',
    section: 'B. Mini-widget',
    scope: 'Okno widget w tauri.conf.json i capabilities, WidgetWindow.vue montowany po etykiecie, widget.rs z apply() wołanym z setup, settings-changed i pętli reminder.rs, przełącznik w nagłówku głównego okna, ustawienia widget_enabled i widget_background_only.',
    done: 'Playwright renderuje #widget w podglądzie i sprawdza Stop/Start; build Tauri przechodzi.',
  },
  {
    id: 'J', phase: 'Etap 2: Okna i system', title: 'Powiadomienia systemowe',
    section: 'J. Powiadomienia systemowe',
    scope: 'tauri-plugin-notification z Rust, notify.rs z NotifyClock, cztery reguły z tabeli, dedupe w notifications_sent, czyszczenie wpisów starszych niż 7 dni, przełączniki w Ustawieniach.',
    done: 'Testy Rust NotifyClock z kontrolowanym czasem dla każdej reguły i dedupe po restarcie.',
  },
  {
    id: 'F', phase: 'Etap 3: Planowanie', title: 'Ręczna kolejność zadań',
    section: 'F. Ręczna kolejność zadań',
    scope: 'Komparator w src/order.ts użyty wszędzie, planner::reorder i komenda reorder_tasks, odpowiednik w store.ts, natywny drag and drop z uchwytem, Alt+↑/↓, uchwyty także w liście planu dnia.',
    done: 'Rust: reorder nie nadpisuje innych pól JSON; Playwright: przesunięcie klawiaturą zmienia kolejność i przetrwa reload; zmiana pasma zmienia priorytet.',
  },
  {
    id: 'E', phase: 'Etap 3: Planowanie', title: 'Plan kontra rzeczywistość na osi dnia',
    section: 'E. Plan kontra rzeczywistość na osi dnia',
    scope: 'DayTimeline.vue (SVG) z torami Plan i Rzeczywistość, paleta 8 kolorów jako tokeny CSS w obu motywach, tabela różnic, pole plannedStart w TaskPlanning, interakcje hover i klik.',
    done: 'Playwright z ustalonym zegarem: liczba bloków, treść tabeli, zrzut 760 px.',
  },
  {
    id: 'G', phase: 'Etap 3: Planowanie', title: 'Tablica tygodnia Pon–Nd',
    section: 'G. Tablica tygodnia Pon–Nd',
    scope: 'WeekBoard.vue z kolumną Do zaplanowania i siedmioma dniami, pojemność z celu dziennego i work_days, planner::plan_task i komenda, odpowiednik w store.ts, drag and drop plus menu Przenieś na…, podział zakładki Tydzień na Tablica i Podsumowanie.',
    done: 'Playwright: przeniesienie przez menu, suma szacunków, przekroczenie pojemności; Rust: plan_task nie nadpisuje innych pól.',
  },
  {
    id: 'D', phase: 'Etap 4: Raporty', title: 'Raport TS z wybranych dni',
    section: 'D. Raport TS z wybranych dni',
    scope: 'Tryb Wybrane dni w ReportsView z kalendarzem miesiąca i skrótami, dwa układy raportu, tsText w src/report.ts jako jedyne źródło dla DayClose, raportu dziennego i trybu wielodniowego, pamięć zaznaczenia w localStorage.',
    done: 'Playwright: dwa dni, oba układy, schowek, zmiana miesiąca, dzień zmiany czasu w zestawie.',
  },
  {
    id: 'H', phase: 'Etap 4: Raporty', title: 'Trafność szacunków',
    section: 'H. Trafność szacunków',
    scope: 'src/estimates.ts z funkcjami czystymi, karta na Dashboardzie z okresem, wskaźnikami, histogramem, medianą per projekt i listą rozjazdów, sekcja Zagrożone teraz, kolor procentu powyżej 100 na kafelku.',
    done: 'Playwright na przygotowanym localStorage sprawdza wskaźniki i koszyki histogramu.',
  },
  {
    id: 'R1', phase: 'Etap 5: Wydanie', title: 'Wersje 1.0.0 i dokumentacja',
    section: 'Etap 5 — wydanie 1.0.0, punkty 1 i 2',
    scope: 'Podnieś wersje w package.json, package-lock.json, Cargo.toml, Cargo.lock, tauri.conf.json na 1.0.0. Zaktualizuj README (sekcja Organizacja pracy 1.0, ustawienia, skróty, widget, autostart, powiadomienia, liczby testów) i NEXT_SESSION.md (stan po 1.0, co nie zostało zweryfikowane natywnie).',
    done: 'Wszystkie pięć plików ma 1.0.0; README i NEXT_SESSION opisują stan faktyczny, bez obietnic.',
  },
  {
    id: 'R2', phase: 'Etap 5: Wydanie', title: 'Build produkcyjny i walidacja natywna',
    section: 'Etap 5 — wydanie 1.0.0, punkty 3 i 4',
    scope: 'Uruchom pełną weryfikację i npm run tauri build -- -- --locked. Następnie, w miarę możliwości bez interakcji z użytkownikiem, natywna walidacja na osobnym identyfikatorze pl.todotrack.validation.v6 (osobna kompilacja debug, nie instalacja na danych użytkownika): widget, skróty, podział pomiaru, pamięć okien, autostart z --hidden, powiadomienia. Czego nie da się sprawdzić automatycznie, wypisz jako do ręcznego sprawdzenia. Nie dotykaj bazy %APPDATA%\\pl.todotrack.desktop. Nie instaluj NSIS na właściwej instalacji użytkownika.',
    done: 'Instalator Todo Track_1.0.0_x64-setup.exe istnieje; raport walidacji z listą sprawdzonych i niesprawdzonych punktów w src-tauri/target/verification-v6/.',
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
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_1.0.md i dlaczego' },
    manualChecks: { type: 'string', description: 'Co wymaga ręcznego sprawdzenia w natywnej aplikacji' },
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
Źródło prawdy: PLAN_1.0.md. Przeczytaj go w całości, a potem sekcję swojego zadania. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu w store.ts, dane użytkownika nietykalne).
Zasady:
- Każda nowa komenda Tauri ma odpowiednik w trybie podglądu w src/store.ts (localStorage), bo testy Playwright działają bez Rusta.
- Nowe teksty interfejsu po polsku. Zachowaj istniejące etykiety aria, bo testy ich używają.
- Nie zmieniaj ani nie usuwaj istniejących testów, chyba że sekcja planu tego wymaga; wtedy opisz to w deviations.
- Nie dotykaj bazy użytkownika w %APPDATA%\\pl.todotrack.desktop ani zainstalowanej aplikacji w C:\\Programy\\Todo Track.
- Nie pushuj do zdalnego repozytorium. Commity lokalne na gałęzi release/1.0.
- Polecenia: npm run build (vue-tsc + vite), npm test (Playwright, wymaga npx playwright install chromium raz), cargo test --manifest-path src-tauri/Cargo.toml --locked, cargo fmt --manifest-path src-tauri/Cargo.toml. Jeśli cargo nie jest w PATH, dodaj %USERPROFILE%\\.cargo\\bin.
- Nowe zależności Cargo lub npm dodawaj z konkretną wersją i zaktualizuj pliki lock; --locked musi przechodzić.
`

function implementPrompt(task) {
  return `${COMMON}
ZADANIE ${task.id}: ${task.title}
Sekcja planu: "${task.section}" w PLAN_1.0.md.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}

Wykonaj całość zakresu, dopisz testy wymienione w sekcji planu, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit na gałęzi release/1.0 z opisem "${task.id}: ${task.title}" i dwoma zdaniami treści. Commit kończ linią "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>".
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, manualChecks, blocker.`
}

function verifyPrompt(task) {
  return `${COMMON}
Jesteś weryfikatorem po zadaniu ${task.id} (${task.title}). Nie ufaj deklaracjom implementatora. Uruchom od zera, w tej kolejności, i zapisz pełne wyjście:
1. git status --short (czy są niezacommitowane zmiany)
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

Znajdź przyczynę i napraw ją w kodzie lub w nowych testach. Nie wyłączaj i nie kasuj testów. Uruchom ponownie polecenia, które padły, oraz cargo fmt. Zrób commit "${task.id}: poprawki po weryfikacji (${round})" z linią Co-Authored-By jak w pozostałych commitach.
Zwróć wynik w schemacie IMPL (status done, gdy wszystko przechodzi; blocked z opisem, gdy nie).`
}

function reviewPrompt(task, lens, impl) {
  const lenses = {
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff ostatniego commita zadania (git show ${impl.commit || 'HEAD'} oraz poprzednie commity zadania ${task.id}, jeśli są) z sekcją "${task.section}" w PLAN_1.0.md. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Brakujący odpowiednik w store.ts dla nowej komendy to zawsze must.`,
    bugs: `Soczewka: POPRAWNOŚĆ. Przeczytaj diff commitów zadania ${task.id} (git log --oneline, git show). Szukaj błędów: warunki wyścigu przy jednym aktywnym pomiarze, przedziały przez północ i zmianę czasu, nadpisywanie niezapisanego formularza planu (set_priority wzorzec json_set), niespójność między Rust a store.ts, brak walidacji wejścia w komendach, wycieki nasłuchów Tauri w onUnmounted, dostępność (aria, klawiatura). Zgłaszaj tylko to, co umiesz uzasadnić konkretną ścieżką wykonania.`,
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
// args: { from?: id, only?: [ids], maxFixRounds?: number }
// ---------------------------------------------------------------------------
const opts = args || {}
const maxFixRounds = opts.maxFixRounds || 3
let queue = TASKS
if (opts.only && opts.only.length) queue = TASKS.filter(t => opts.only.includes(t.id))
else if (opts.from) {
  const start = TASKS.findIndex(t => t.id === opts.from)
  if (start >= 0) queue = TASKS.slice(start)
}
log(`Do wykonania: ${queue.map(t => t.id).join(', ')} (rundy poprawek: ${maxFixRounds})`)

const results = []
for (const task of queue) {
  phase(task.phase)
  const result = await runTask(task, maxFixRounds)
  results.push(result)
  if (result.status === 'blocked') {
    log(`⛔ ${task.id} zablokowane: ${result.blocker}`)
    log(`Wznowienie po naprawie: Workflow({ name: 'release-1-0', args: { from: '${task.id}' } })`)
    break
  }
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
