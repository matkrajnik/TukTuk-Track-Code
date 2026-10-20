export const meta = {
  name: 'release-1-8',
  description: 'Realizuje PLAN_1.8.md zadanie po zadaniu (wysyłka Raportu TS do Ideo Timesheet): implementacja, weryfikacja testami, przegląd, poprawki, commit; jedno wydanie 1.8.0 na końcu',
  whenToUse: 'Pełna delegacja prac nad Todo Track 1.8 według PLAN_1.8.md (T1–T5, R18). args: { from?: "T1", only?: ["T3"], until?: "T5", maxFixRounds?: 3 }',
  phases: [
    { title: 'Backend TS', detail: 'timesheet.rs: logowanie, sesja, formularz dnia, parser, zapis multipart' },
    { title: 'Mapowanie i wysyłka', detail: 'zadanie TS na zadaniu, domyślne per projekt, podgląd i wysyłka dnia' },
    { title: 'Ustawienia i podgląd', detail: 'sekcja Timesheet (TS), symulowany TS, testy Playwright' },
    { title: 'Wydanie 1.8.0', detail: 'wersja, README, NEXT_SESSION, NSIS, tag' },
  ],
}

// ---------------------------------------------------------------------------
// Lista zadań. Kolejność jest ścisła: jeden implementator naraz w jednym
// drzewie roboczym, bo wszystkie zadania dotykają store.ts, main.rs, settings
// i tych samych widoków (DayClose, ReportsView, SettingsPanel).
// Równolegle działają tylko agenci czytający (przegląd).
// Testów ręcznych z prawdziwym TS nie ma w planie: lista jest na końcu
// PLAN_1.8.md i trafia do NEXT_SESSION.md przy wydaniu.
// ---------------------------------------------------------------------------
const TASKS = [
  {
    id: 'T1', phase: 'Backend TS', title: 'Backend TS (timesheet.rs)',
    section: 'T1 — Backend TS (`src-tauri/src/timesheet.rs`)',
    scope: 'Port konektora z C:\\Projekty\\Private\\ideo-ts-api\\connector\\src (auth.service.ts, timesheet.service.ts, timesheet-html.parser.ts, timesheet-form.helper.ts) na Rust: Config { api_url (domyślnie https://api.ts.ideo.pl, tylko https bez ścieżki), login, device_id (losowany raz, 32 znaki hex), device_name "Todo Track" } w settings pod kluczem "timesheet" jak Jira pod "jira"; hasło w keyring (usługa "<identifier>.timesheet", użytkownik "<api_url>|<login>"); TsState w pamięci (token, refresh, expires_at, cookie) — nigdy w SQLite ani w logach. Protokół: POST /api/login (JSON login/password/deviceId/deviceName → token/refresh/expires), GET /api/user/identity z Bearer → pierwszy Set-Cookie (sam fragment nazwa=wartość), ważność tokenu z exp JWT albo expires (ms / s epoki / s od teraz) albo 30 min, odświeżenie GET /api/token/refresh?id=api-ts&name=api-ts&refresh=… (+ ponowne identity), 401 przy odświeżaniu → pełne logowanie, with_auth z jednorazową powtórką po 401; oba nagłówki Authorization i Cookie przy każdym żądaniu do /timesheet/*. GET /timesheet/wypelnij?dzien=RRRR-MM-DD i parser przez crate scraper (parse_tasks: pierwszy select w .jq-wiersz-ts .jq-zadanie, optgroup label = projekt, option value/text = zadanie; parse_filled: tr.jq-wiersz-ts, option[selected], inputy .jq-zadanie-godziny/.jq-zadanie-nadgodziny/.jq-zadanie-opis, wartości jako tekst, puste id pomijane; brak formularza → czytelny błąd). POST multipart (reqwest feature multipart): akcja=zapisz, dzien, status=0, submit="Zapisz Timesheet", zadanie[i], zadanie_godziny[i], zadanie_nadgodziny[i], zadanie_opis[i]; klient z redirect none, 2xx i 3xx po POST liczą się jako przyjęte, potwierdzeniem jest odczyt ts_filled. Komendy w main.rs: ts_settings, ts_connect(api_url, login, password), ts_disconnect, ts_tasks(date), ts_filled(date), ts_send(date, entries, mode "replace"|"keep"), ts_sent(day); walidacja wejścia jak w planie (data, id ≤ 20, godziny ^\\d+([.,]\\d{1,2})?$, opis ≤ 2000 bez znaków sterujących, 1–50 wpisów); ts_send odrzuca przy wyłączonym ts_send w settings.rs (pole domyślnie true — dodaj je już tutaj, T4 tylko pokaże przełącznik), przy "keep" dokleja wpisy TS spoza listy, po POST odczytuje ts_filled i zapisuje tabelę ts_sent_days(day, sent_at, hours). Komunikaty po polsku w jednej funkcji status_error jak w jira.rs; hasło, token, refresh i cookie nigdy w komunikatach ani logach. Zależności: reqwest + "multipart", scraper, ew. rand/getrandom — konkretne wersje, Cargo.lock zaktualizowany, --locked przechodzi. Tabele ts_sent_days i ts_project_map (T2) w db.rs przez CREATE TABLE IF NOT EXISTS; kopia SQLite je obejmuje.',
    done: 'Testy Rust bez sieci: parser na src-tauri/tests/fixtures/ts_wypelnij.html (zanonimizowany, ręcznie napisany fragment z pełną <table>, 2 projekty, 3 zadania, wiersze z „1,5” i „0.25”, wiersz bez zadania, opis z polskimi znakami i średnikiem — scraper::Html::parse_fragment gubi samotne <tr>), budowa multipartu (nazwy i kolejność pól), ważność tokenu (4 warianty), wycięcie cookie z Set-Cookie z atrybutami, walidacja daty i wpisów, scalanie "keep", normalizacja adresu, blokada przy wyłączonym ts_send, ts_sent_days na bazie w pamięci. Nic nie łączy się z prawdziwym serwerem.',
  },
  {
    id: 'T2', phase: 'Mapowanie i wysyłka', title: 'Mapowanie zadań na zadania TS',
    section: 'T2 — Mapowanie zadań na zadania TS',
    scope: 'TaskDetails w src/store.ts: tsTaskId?: string i tsTaskName?: string (backend czyta je tylko w ts_send; istniejące JSON-y bez pól wczytują się jak dotąd). Rozwinięty kafelek (TaskCard.vue): pole „Zadanie TS” — select z optgroup per projekt TS z ts_tasks(dzisiaj), opcja „— brak —”, pamięć podręczna listy w store.ts na czas działania z przyciskiem „Odśwież”; bez połączenia zapamiętana nazwa i link „Połącz konto TS w Ustawieniach” (navigate z section "timesheet"); tsTaskId spoza listy → opcja „(niedostępne) nazwa” z ostrzeżeniem. Tabela ts_project_map(project COLLATE NOCASE PRIMARY KEY, ts_task_id, ts_task_name) z komendami ts_project_map i ts_set_project_map (puste id usuwa) i odpowiednikiem w store.ts; w kafelku pole wyboru „Ustaw jako domyślne dla projektu „X””. Czysta funkcja resolveTsTask(task, map) w nowym src/timesheet.ts: jawne tsTaskId → mapowanie projektu → brak; chip „TS: nazwa (z projektu)” na kafelku. Nowe zadania nie kopiują mapowania projektu do task_details.',
    done: 'Jednostkowe resolveTsTask (jawne, z projektu, wielkość liter, brak). Rust: ts_project_map wstawianie, nadpisanie, usunięcie pustym id, NOCASE. Playwright: wybór zadania TS w kafelku i zapamiętanie, „domyślne dla projektu” — na symulowanym TS, który w tym zadaniu trzeba już dodać w store.ts w minimalnym zakresie (tsSettings, tsTasks, tsProjectMap), T5 go rozbuduje.',
  },
  {
    id: 'T3', phase: 'Mapowanie i wysyłka', title: 'Podgląd i wysyłka dnia',
    section: 'T3 — Podgląd i wysyłka dnia',
    scope: 'Nowy src/components/TsSendDialog.vue otwierany przyciskiem „Wyślij do TS” w sekcji „Raport TS” Zamknięcia dnia (DayClose.vue, obok „Kopiuj raport TS”) i Raportu dnia (ReportsView.vue, obok „Kopiuj raport”); widoczny tylko przy włączonym tsSend i połączonym koncie, bez konta tekst z linkiem do Ustawień, dzień z przyszłości blokuje. Czysta funkcja tsEntries(day, data, now, map, options) w src/timesheet.ts na bazie dayRows z report.ts: pomija zero, rozstrzyga zadanie TS przez resolveTsTask, scala wiersze o tym samym tsTaskId (suma ms → decimalHours po scaleniu, opisy „; ” bez powtórzeń, białe znaki zbite), opis = notatka dnia albo tytuł zadania, nadgodziny "0"; zwraca { entries, unmapped, totalMs }. Podgląd: tabela Zadania Todo Track → Zadanie TS · Godziny · Opis (opis edytowalny w miejscu, nie zmienia notatki), suma; ostrzeżenia: N zadań bez zadania TS (X h) z linkami do kafelków, trwający pomiar do teraz, zadanie TS spoza listy ts_tasks(day). Blok istniejących wpisów z ts_filled(day) (nazwa z ts_tasks, godziny, opis) z przełącznikiem „Zachowaj pozostałe wpisy TS” (domyślnie; wpisy o tych samych zadaniach oznaczone „zastąpi X h”) / „Zastąp wszystko” (etykieta przycisku „Zastąp i zapisz”); pusty dzień → „Dzień w TS jest pusty”. „Zapisz w TS” → ts_send; po sukcesie odczyt z TS („W TS zapisano N wpisów, razem Y h”), różnica liczby wpisów albo sumy > 0,01 h → ostrzeżenie; błąd zostawia dane w dialogu. Znacznik ts_sent(day) w obu sekcjach („Wysłano do TS 19.10 17:02 (7,50 h)”, dopisek „raport zmienił się od wysyłki” gdy suma inna). Wysyłka nie zamyka dnia, zamknięcie nie wysyła. Odpowiedniki ts_filled/ts_send/ts_sent w symulowanym TS w store.ts (zastępowanie dnia jak serwer, "keep" dokleja).',
    done: 'Jednostkowe tsEntries (scalanie dwóch zadań na jedno TS, tytuł zamiast brakującej notatki, zero pominięte, trwający do now, zaokrąglenie po scaleniu zgodne z sumą raportu, unmapped z czasem) i porównania po zapisie. Playwright: podgląd z obu miejsc, ostrzeżenia, „Zachowaj” kontra „Zastąp”, komunikat po zapisie, znacznik „Wysłano do TS”, błąd zostawia dane. Zrzuty wzorcowe Zamknięcia dnia i Raportu dnia zaktualizowane świadomie tylko, gdy nowy przycisk zmienia układ — wymienione w deviations.',
  },
  {
    id: 'T4', phase: 'Ustawienia i podgląd', title: 'Ustawienia → „Timesheet (TS)”',
    section: 'T4 — Ustawienia → „Timesheet (TS)”',
    scope: 'Nowa sekcja "timesheet" w SettingsPanel.vue i na liście sekcji w SettingsView.vue (między Jira a Kalendarz; w wyszukiwarce ustawień hasła „Timesheet”, „TS”, „Ideo”). Połączenie: Login, Hasło (type=password, wartość nigdy nie wraca z backendu; po połączeniu puste z podpisem o magazynie poświadczeń), zwinięte „Zaawansowane” z adresem API i identyfikatorem urządzenia (tylko do odczytu). Przyciski „Połącz” (ts_connect), „Testuj połączenie” (ts_tasks(dzisiaj) → „Połączono: N zadań w M projektach” albo błąd), „Rozłącz” (ConfirmDialog). Stan „Połączono jako login” / „Nie połączono”. Przełącznik „Wysyłaj raport TS” (tsSend, domyślnie włączony) z opisem, że wyłączenie chowa przyciski i blokuje zapis w backendzie. Tabela mapowań z ts_project_map (projekt → zadanie TS, „Usuń”) i dodawanie: projekt z listy projektów zadań + zadanie TS z ts_tasks(dzisiaj). Krótki opis działania pod sekcją. Wszystkie wejścia navigate z section "timesheet" otwierają tę sekcję.',
    done: 'Rust: domyślne tsSend, zapis i odczyt. Playwright: połącz (symulowane), błąd przy złym haśle, test połączenia, rozłącz, przełącznik chowa przyciski wysyłki w Zamknięciu dnia i Raporcie dnia, dodanie i usunięcie mapowania projektu. Nowy zrzut wzorcowy sekcji wymieniony w deviations.',
  },
  {
    id: 'T5', phase: 'Ustawienia i podgląd', title: 'Symulowany TS, testy e2e, seed',
    section: 'T5 — Podgląd przeglądarkowy, testy e2e, dokumentacja',
    scope: 'Dokończenie symulowanego TS w store.ts pod previewTsKey = "todo-track-preview-ts-v1" (wzór previewJira*): stan { connected, login, failLogin, tasks, filled per dzień, sent per dzień, projectMap }, projekty „WMS” (3 zadania) i „Wewnętrzne” („Spotkania”, „Administracja”), dla dnia z tests/fixtures/seed.ts jeden istniejący wpis, hasło „zle” → błąd logowania, nasłuch storage jak przy Jirze. seed.ts: dwa zadania z tsTaskId na to samo zadanie TS, jedno z projektem mapowanym tylko przez ts_project_map, jedno bez niczego. Jeden plik tests/timesheet.spec.ts zbierający scenariusze z T2–T4 (jeśli powstały w osobnych plikach, scal je tutaj bez utraty asercji) i uzupełniający brakujące: pełna ścieżka Ustawienia → mapowanie → Zamknięcie dnia → podgląd → zapis → znacznik. README NIE w tym zadaniu (robi R18), ale w summary opisz dokładnie, co trzeba w nim napisać: „Nowości 1.8”, punkt w opisie ustawień, sekcja „Timesheet (Ideo TS)” obok sekcji o Jirze z protokołem i uwagą „zapis zastępuje cały dzień”.',
    done: 'tests/timesheet.spec.ts zielony; wszystkie dotychczasowe testy zielone; zrzuty wzorcowe zgodne albo zaktualizowane świadomie i wymienione w deviations.',
  },
  {
    id: 'R18', phase: 'Wydanie 1.8.0', title: 'Wydanie 1.8.0', release: '1.8.0',
    section: 'Wydanie 1.8.0 (R18)',
    scope: 'Wydanie obejmuje wszystkie zadania z PLAN_1.8.md (T1–T5). README „Nowości 1.8” opisuje wysyłkę do TS (skąd godziny i opisy, scalanie, „Zachowaj” / „Zastąp”, hasło w magazynie poświadczeń, token tylko w pamięci, nic nie wysyła się bez kliknięcia), nowy punkt „Timesheet (TS)” w opisie ustawień i nowa sekcja „Timesheet (Ideo TS)” obok sekcji o Jirze (login, identity, refresh, wypelnij, multipart; „zapis zastępuje cały dzień”; lista rzeczy niesprawdzonych z prawdziwym TS).',
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
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_1.8.md i dlaczego' },
    manualChecks: { type: 'string', description: 'Co wymaga ręcznego sprawdzenia z prawdziwym TS, w natywnej aplikacji lub po instalacji NSIS' },
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
    noNetwork: { type: 'boolean', description: 'Żaden test ani skrypt nie łączy się z api.ts.ideo.pl' },
    playwrightCount: { type: 'number' },
    rustCount: { type: 'number' },
    failures: { type: 'string', description: 'Pełne komunikaty błędów, nazwy testów, fragmenty logu' },
    uncommitted: { type: 'boolean', description: 'git status pokazuje niezacommitowane zmiany' },
  },
  required: ['ok', 'build', 'playwright', 'cargoTest', 'cargoFmt', 'noNetwork', 'failures', 'uncommitted'],
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
Projekt: C:\\Projekty\\Private\\to-do-track\\.claude\\worktrees\\release-1-8 (worktree repozytorium to-do-track; Tauri 2 + Rust + Vue 3, interfejs po polsku), gałąź release-1-8. Pracuj WYŁĄCZNIE w tym katalogu — nie zmieniaj plików w głównym katalogu C:\\Projekty\\Private\\to-do-track poza tym worktree i nie przełączaj gałęzi.
Źródło prawdy: PLAN_1.8.md. Przeczytaj go w całości (zwłaszcza „Założenia przyjęte bez pytania” i listę ręcznych testów na końcu), a potem sekcję swojego zadania. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu w store.ts, dane użytkownika nietykalne).
Wzorzec integracji: Jira (src-tauri/src/jira.rs — konfiguracja w settings pod kluczem JSON, sekret w keyring, status_error po polsku, komendy w main.rs; src/store.ts — previewJira*; src/components/SettingsPanel.vue — sekcja jira; tests/jira*.spec.ts). Istniejący Raport TS: src/report.ts (tsText, decimalHours, dayRows), sekcje w src/DayClose.vue, src/views/ReportsView.vue, src/components/MultiDayReport.vue.
Konektor źródłowy do portu: C:\\Projekty\\Private\\ideo-ts-api\\connector\\src — TYLKO DO CZYTANIA. Nie uruchamiaj go, nie instaluj jego zależności, nie modyfikuj.

ZAKAZ BEZWZGLĘDNY: nie wykonuj żadnych żądań HTTP do https://api.ts.ideo.pl ani do żadnego innego serwera Ideo Timesheet — ani ręcznie (curl, Invoke-WebRequest, node), ani w testach, ani w skryptach build. Testy Rust działają na pliku HTML z fixtures, testy Playwright na symulowanym TS w localStorage. Adres api.ts.ideo.pl może wystąpić w kodzie tylko jako wartość domyślna konfiguracji i w dokumentacji. Hasło, token, refresh token i ciasteczko sesji nigdy nie trafiają do logów, komunikatów błędów, SQLite ani do wyników testów.

Zasady:
- Każda nowa komenda Tauri ma odpowiednik w trybie podglądu w src/store.ts (localStorage), bo testy Playwright działają bez Rusta.
- Nowe tabele SQLite tylko przez CREATE TABLE IF NOT EXISTS przy starcie; nowe pola zadań w JSON task_details z wartościami domyślnymi; bez niszczących migracji.
- Wysyłka do TS nigdy nie jest automatyczna: zawsze podgląd i kliknięcie „Zapisz w TS”. Zamknięcie dnia nie wysyła niczego samo. Timer nigdy nie przełącza się sam.
- Zapis w TS zastępuje cały dzień — każda ścieżka zapisu musi to respektować (tryb "keep" dokleja istniejące wpisy).
- Godziny do TS jak w raporcie: dziesiętne, dwa miejsca, przecinek (decimalHours), zaokrąglenie per wiersz po scaleniu; bez zaokrąglania do 0,25 h; nadgodziny "0"; status "0".
- Nowe teksty interfejsu po polsku. Zachowaj istniejące etykiety aria, bo testy ich używają.
- Nie zmieniaj ani nie usuwaj istniejących testów, chyba że sekcja planu tego wymaga; wtedy opisz to w deviations. Gdy zmiana interfejsu świadomie zmienia wygląd, zaktualizuj zrzuty wzorcowe (npm run test:visual:update) i wymień to w deviations.
- Nie dotykaj bazy użytkownika w %APPDATA%\\pl.todotrack.desktop, Menedżera poświadczeń Windows ani zainstalowanej aplikacji. Nie instaluj NSIS.
- Nie pushuj do zdalnego repozytorium, nie wypychaj tagów, nie publikuj wydania. Commity lokalne na gałęzi release-1-8, bez stopki Co-Authored-By (jak ostatnie commity na master).
- Polecenia: npm run build (vue-tsc + vite), npm test (Playwright), cargo test --manifest-path src-tauri/Cargo.toml --locked, cargo fmt --manifest-path src-tauri/Cargo.toml; npm run check uruchamia wszystko. cargo zwykle NIE jest w PATH: dodaj %USERPROFILE%\\.cargo\\bin (w Git Bash: export PATH="$HOME/.cargo/bin:$PATH").
- Nowe zależności Cargo lub npm dodawaj z konkretną wersją i zaktualizuj pliki lock; --locked musi przechodzić. Nie dodawaj zależności npm z konektora (axios, cheerio, form-data) — port jest w Rust.
`

function implementPrompt(task) {
  return `${COMMON}
ZADANIE ${task.id}: ${task.title}
Nie zmieniaj numeru wersji — wersję 1.8.0 ustawia dopiero \`npm run ship\` po scaleniu do master. README i NEXT_SESSION.md aktualizuje dopiero R18, ale opisz zmiany dokładnie w summary.
Sekcja planu: "${task.section}" w PLAN_1.8.md.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}

Wykonaj całość zakresu, dopisz testy wymienione w sekcji planu, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit z opisem "${task.id}: ${task.title}" i dwoma zdaniami treści po polsku.
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
W manualChecks wypisz to, czego nie dało się sprawdzić bez prawdziwego TS (nie powtarzaj listy z końca planu — tylko rzeczy nowe albo doprecyzowane).
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, manualChecks, blocker.`
}

function releasePrompt(task, notes) {
  return `${COMMON}
ZADANIE ${task.id}: wydanie ${task.release}. ${task.scope}
Od 1.7.4 wydanie robi użytkownik jednym poleceniem \`npm run ship -- ${task.release}\` na master (RELEASE.md, scripts/ship.mjs): ono podbija wersję, buduje i podpisuje instalator (klucz z hasłem, którego nie masz), publikuje, commituje, taguje i pushuje. Ty tylko PRZYGOTOWUJESZ wydanie na gałęzi release-1-8. NIE zmieniaj wersji w manifestach, NIE uruchamiaj npm run ship / npm run release / npm run tauri build, NIE twórz tagu, NIE ruszaj releases/. Kroki:
1. notes.md: nadpisz notatkami wydania ${task.release} (po polsku, krótko, dla użytkowników; wzoruj się na obecnym notes.md z 1.7.4).
2. README: sekcja „Nowości ${task.release.replace(/\.0$/, '')}” nad dotychczasowymi nowościami, opisująca to, co faktycznie zrobiono (git log od tagu v1.7.4), bez obietnic; punkt „Timesheet (TS)” w opisie ustawień; sekcja „Timesheet (Ideo TS)”; wiersz „Wersja w repozytorium” i blok „Weryfikacja ${task.release}” z liczbami testów. NEXT_SESSION.md: nowy wpis „Stan po ${task.release}” na górze z wynikami weryfikacji, odstępstwami i listą rzeczy do ręcznego sprawdzenia: cała sekcja „Do ręcznego sprawdzenia z prawdziwym TS” z PLAN_1.8.md plus uwagi zgłoszone przez zadania:
${notes || '(brak zgłoszonych)'}
3. Pełna weryfikacja: npm run build, npm test, cargo fmt --check, cargo test --locked.
4. Commit "${task.release}: przygotowanie wydania (README, notes.md, NEXT_SESSION)" (bez stopki Co-Authored-By). Bez tagu, bez pusha. W NEXT_SESSION.md zapisz, że wydanie wymaga scalenia release-1-8 do master i \`npm run ship -- ${task.release}\` przez użytkownika.
Zwróć wynik w schemacie IMPL.`
}

function verifyPrompt(task) {
  return `${COMMON}
Jesteś weryfikatorem po zadaniu ${task.id} (${task.title}). Nie ufaj deklaracjom implementatora. Uruchom od zera, w tej kolejności, i zapisz pełne wyjście:
1. git status --short (czy są niezacommitowane zmiany; pliki ignorowane i .claude/launch.json się nie liczą)
2. npm run build
3. npx playwright install chromium (tylko jeśli brak przeglądarki), potem npm test
4. cargo test --manifest-path src-tauri/Cargo.toml --locked
5. cargo fmt --manifest-path src-tauri/Cargo.toml --check
6. Brak sieci: grep -rn "ts.ideo.pl" po src, src-tauri/src, tests, scripts, package.json — adres może być tylko wartością domyślną konfiguracji (Config::default w timesheet.rs, previewTs w store.ts) i w tekstach interfejsu/README; żaden test, fixture ani skrypt nie może go wywoływać. Sprawdź też, że testy Rust nowego modułu nie tworzą klienta HTTP do prawdziwego adresu (szukaj reqwest w #[cfg(test)]). noNetwork=true tylko gdy to potwierdzisz.
Nie poprawiaj niczego i sam również nie łącz się z żadnym serwerem TS. Zwróć ok=true tylko gdy wszystkie kroki przeszły, noNetwork=true i nie ma niezacommitowanych zmian. W failures wklej dokładne komunikaty błędów i nazwy testów, które padły. Podaj liczbę testów Playwright i Rust.`
}

function fixPrompt(task, failures, round) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}) ma czerwoną weryfikację (runda poprawek ${round}). Wynik weryfikatora:
${failures}

Znajdź przyczynę i napraw ją w kodzie lub w nowych testach. Nie wyłączaj i nie kasuj testów. Test zrzutu wzorcowego, który pada przez świadomą zmianę wyglądu z tego zadania, możesz zaktualizować; przez przypadkową zmianę — napraw kod. Uruchom ponownie polecenia, które padły, oraz cargo fmt. Zrób commit "${task.id}: poprawki po weryfikacji (${round})".
Zwróć wynik w schemacie IMPL (status done, gdy wszystko przechodzi; blocked z opisem, gdy nie).`
}

function reviewPrompt(task, lens, impl) {
  const lenses = {
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff commitów zadania ${task.id} (git log --oneline, git show ${impl.commit || 'HEAD'} i wcześniejsze commity tego zadania) z sekcją "${task.section}" w PLAN_1.8.md i z jego założeniami. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Zawsze must: brakujący odpowiednik w store.ts dla nowej komendy, wysyłka do TS bez podglądu i kliknięcia, zapis do TS ignorujący, że zastępuje cały dzień, token/cookie/hasło zapisane w SQLite albo w logach, przełącznik tsSend nierespektowany w backendzie.`,
    bugs: `Soczewka: POPRAWNOŚĆ. Przeczytaj diff commitów zadania ${task.id} (git log --oneline, git show). Szukaj błędów: sekrety w komunikatach błędów i logach (format! z hasłem, tokenem, nagłówkiem Cookie), odświeżanie tokenu bez odnowienia ciasteczka, pętla przy 401 (powtórka więcej niż raz), brak walidacji wejścia w komendach ts_*, scalanie wpisów (suma po scaleniu kontra per wiersz, duplikaty opisów, zadania bez mapowania liczone do sumy), tryb "keep" gubiący wpisy TS albo dublujący ten sam taskId, 3xx po POST traktowane jako błąd przy redirect none, selektory parsera różne od cheerio z konektora, nadpisywanie innych pól JSON w task_details (wzorzec json_set), niespójność między Rust a store.ts, blokowanie wątku, wycieki nasłuchów Tauri w onUnmounted, dostępność (aria, klawiatura, focus w dialogu). Zgłaszaj tylko to, co umiesz uzasadnić konkretną ścieżką wykonania.`,
  }
  return `${COMMON}
Jesteś recenzentem po zadaniu ${task.id} (${task.title}). Tylko czytasz, niczego nie zmieniasz i nie uruchamiasz testów ani zapytań sieciowych.
${lenses[lens]}
Podsumowanie implementatora: ${impl.summary}
Zwróć listę findings z severity: must (błąd lub brak z planu), should (warto poprawić w tym zadaniu), nit (kosmetyka). Pusta lista, gdy nic nie znalazłeś.`
}

function applyReviewPrompt(task, findings) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}): przegląd zgłosił uwagi do wdrożenia. Wdróż wszystkie must i should; nit pomiń, chyba że są trywialne.
${JSON.stringify(findings, null, 2)}
Jeśli uważasz, że któraś uwaga jest błędna, nie wdrażaj jej i napisz dlaczego w deviations. Po zmianach uruchom npm run build, npm test, cargo test, cargo fmt. Commit "${task.id}: uwagi z przeglądu". Zwróć wynik w schemacie IMPL.`
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
    log(`${task.id}: weryfikacja czerwona (runda ${round}): build=${v.build} playwright=${v.playwright} cargo=${v.cargoTest} fmt=${v.cargoFmt} noNetwork=${v.noNetwork} uncommitted=${v.uncommitted}`)
    const extra = [
      v.uncommitted ? 'UWAGA: są niezacommitowane zmiany, dokończ commit.' : '',
      v.noNetwork === false ? 'UWAGA: weryfikator znalazł ścieżkę łączącą się z prawdziwym TS — usuń ją; testy nie mogą wychodzić do sieci.' : '',
    ].filter(Boolean).join('\n')
    const fix = await agent(fixPrompt(task, v.failures + (extra ? `\n\n${extra}` : ''), round), { label: `fix:${task.id}:${stageLabel}:${round}`, phase: task.phase, schema: IMPL })
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
// until: zatrzymaj się po tym zadaniu (np. "T3", żeby zobaczyć wysyłkę przed Ustawieniami).
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
log(`Do wykonania: ${queue.map(t => t.id).join(', ')} (rundy poprawek: ${maxFixRounds}). Żaden agent nie łączy się z api.ts.ideo.pl.`)

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
    log(`Wznowienie po naprawie: Workflow({ name: 'release-1-8', args: { from: '${task.id}' } })`)
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
