export const meta = {
  name: 'backlog',
  description: 'Realizuje PLAN_BACKLOG.md zadanie po zadaniu (TukTuk Track 2.0: poprawki, agenci AI, nowa nazwa z migracją): implementacja, weryfikacja testami, przegląd, poprawki, commit; przygotowanie wydania 2.0.0',
  whenToUse: 'Pełna delegacja prac nad TukTuk Track 2.0 (dawniej Todo Track) według PLAN_BACKLOG.md (B7, B10, B1, B3, B4, B5, B12, B2, B6, B11, B8, B9, A0–A6, N1, R20). args: { from?: "B7", only?: ["B3"], until?: "B9", maxFixRounds?: 3, model?: "opus" }',
  phases: [
    { title: 'Porządek', detail: 'B7 poprawność backendu, B10 podział store.ts i dużych komponentów' },
    { title: 'Codzienna praca', detail: 'B1 przyciski i SearchSelect, B3 pomiary, B4 wysyłka do TS z nadgodzinami, B5 raporty, B12 ręczne spotkania' },
    { title: 'Układ i wydajność', detail: 'B2 układ i nawigacja, B6 wydajność, B11 drobne funkcje' },
    { title: 'Agenci', detail: 'B8 hooki Claude Code i Codexa, B9 worktree Orki' },
    { title: 'AI: fundamenty', detail: 'A0 filtr danych osobowych, A1 silnik CLI, A2 serwer MCP' },
    { title: 'AI: funkcje', detail: 'A3 czat, A4 asystent przypisań, A5 Zamknięcie dnia, A6 Rano i tydzień' },
    { title: 'Nazwa i wydanie 2.0', detail: 'N1 TukTuk Track z migracją z Todo Track, R20 przygotowanie wydania 2.0.0' },
  ],
}

// ---------------------------------------------------------------------------
// Katalog i gałąź pracy. Przed pierwszym uruchomieniem (po `npm run ship -- 1.9.0`,
// żeby gałąź startowała z wersji 1.9.0):
//   git worktree add .claude/worktrees/backlog -b backlog master
// Sesja uruchamiająca workflow sprawdza, że to jej katalog roboczy
// (git rev-parse --show-toplevel) i gałąź; gdy się różnią, poprawia te dwie stałe.
// ---------------------------------------------------------------------------
const ROOT = 'C:\\Projekty\\Private\\to-do-track\\.claude\\worktrees\\backlog'
const BRANCH = 'backlog'
const PLAN = 'PLAN_BACKLOG.md'

// Opcjonalny model agentów: args.model (np. 'opus'). Bez niego opcje agentów są bez zmian (cache wznowienia działa).
const MODEL = (typeof args !== 'undefined' && args && args.model) || null
function runAgent(prompt, o) { return agent(prompt, MODEL ? { ...o, model: MODEL } : o) }

// ---------------------------------------------------------------------------
// Lista zadań w kolejności z sekcji „Kolejność realizacji” planu. Kolejność jest
// ścisła: jeden implementator naraz w jednym drzewie roboczym, bo zadania dotykają
// store.ts, main.rs, settings.rs, SettingsPanel.vue i TaskCard.vue. Równolegle
// działają tylko agenci czytający (przegląd).
// Pola: ai — zadanie części A (dodatkowa soczewka prywatności); testChanges — jakie
// zmiany istniejących testów są dozwolone (reszta musi przejść bez zmiany asercji).
// ---------------------------------------------------------------------------
const TASKS = [
  {
    id: 'B7', phase: 'Porządek', title: 'Poprawność backendu',
    section: 'B7 — Poprawność backendu',
    scope: 'Wszystkie punkty sekcji: backup::KEEP_KEYS (jira, timesheet, calendar, jira_watch, redmine i pozostałe klucze połączeń — sprawdź w settings.rs, co jest konfiguracją połączenia) przenoszone z bieżącej bazy przy przywróceniu kopii; recurrence::generate pomija nieparsowalny wzorzec (continue + jednorazowy log) zamiast zwracać Err; set_priority przeniesione do planner ze sprawdzeniem istnienia zadania („Nie znaleziono zadania.”).',
    done: 'Testy Rust dla trzech ścieżek: przywrócenie starszej kopii zachowuje timesheet/calendar/redmine z bieżącej bazy; uszkodzony repeat_from nie blokuje get_snapshot ani pozostałych cykli; set_priority dla zadania z kosza i nieistniejącego id zwraca polski komunikat.',
    testChanges: 'żadne',
  },
  {
    id: 'B10', phase: 'Porządek', title: 'Jakość kodu: formatowanie, podział store.ts i dużych komponentów',
    section: 'B10 — Jakość kodu',
    scope: 'Bez zmian zachowania. (1) clock() / plural() w src/time.ts zamiast powielonych formatowań HH:MM i odmian liczby mnogiej we wszystkich miejscach z sekcji (sprawdź aktualne numery linii — kod się zmienił od przeglądu); store.ts używa dayKey z time.ts zamiast localDay. (2) Symulatory podglądu wyniesione ze store.ts do src/preview/*.ts (jira.ts, timesheet.ts, calendar.ts, redmine.ts — Redmine doszedł w 1.9 — i wspólne pomocnicze), za tymi samymi eksportami; store.ts zostaje cienką warstwą desktop ? invoke : preview. (3) JiraView.vue, SettingsPanel.vue (każda sekcja jako komponent z własnym busy/error) i TaskCard.vue podzielone na mniejsze komponenty. Nazwy plików i podział opisz w summary — kolejne zadania na nich polegają.',
    done: 'Wszystkie istniejące testy Playwright i Rust przechodzą; zrzuty wzorcowe bez zmian (podział nie zmienia wyglądu). store.ts wyraźnie mniejszy (podaj liczbę linii przed i po).',
    testChanges: 'tylko importy w testach (ścieżki modułów); żadnych zmian asercji ani zrzutów wzorcowych',
  },
  {
    id: 'B1', phase: 'Codzienna praca', title: 'Czytelność przycisków, selecty z wyszukiwaniem, drobne w formularzach',
    section: 'B1 — Czytelność przycisków, selecty z wyszukiwaniem, drobne w formularzach',
    scope: 'Wszystkie 6 punktów sekcji: przycisk drugorzędny z tłem var(--raised) i ikoną lucide dla akcji (TaskCard, JournalView, DayClose, ManualTime, JiraView, RedmineView, SettingsPanel i komponenty wydzielone w B10), link tekstowy tylko dla nawigacji; src/components/SearchSelect.vue (filtrowanie, strzałki, Enter, Escape, aria-activedescendant, role combobox/listbox) zamiast <select> dla projektu, zadania TS, zadania Jiry, tablicy, projektu Redmine, „Wybierz zadanie”; „Bez cyklu” jako pierwsza opcja i parametry cyklu dopiero po wyborze; format przedziałów pracy bez powtórzonej daty i bez sekund w podglądzie; data w podpowiedzi Zamknięcia dnia w formacie nagłówka; walidacja pól liczbowych Ustawień w kliencie z komunikatem o zakresie.',
    done: 'Playwright: SearchSelect (filtrowanie, klawiatura, Escape), „Bez cyklu”, format przedziałów (ten sam dzień i przez północ), puste pole liczbowe w Ustawieniach. Zrzuty wzorcowe kafelka i Ustawień zaktualizowane świadomie.',
    testChanges: 'selektory w testach używających zamienionych <select> (selectOption → SearchSelect) i zrzuty wzorcowe zmienione przez nowy wygląd przycisków; scenariusze i oczekiwane wartości bez zmian',
  },
  {
    id: 'B3', phase: 'Codzienna praca', title: 'Pomiary: luki, bezczynność, start trwającego pomiaru',
    section: 'B3 — Pomiary: luki, bezczynność, start trwającego pomiaru',
    scope: 'Wszystkie 5 punktów sekcji: komenda fill_gap(before_id, after_id, mode: previous|next|split) z kontrolą nakładania i odpowiednikiem w podglądzie; luki krótsze niż 1 s niewyświetlane; trzecia odpowiedź banera bezczynności „Przytnij i wznów” (przycięcie do idle_at, nowy przedział od returned_at na tym samym zadaniu); edycja started_at aktywnej sesji z kontrolą nakładania; test Rust dla SQL trim w idle_resolve.',
    done: 'Rust: fill_gap (trzy tryby, nakładanie, luka zerowa), przycięcie i wznowienie, edycja startu aktywnej sesji, trim w idle_resolve. Playwright: akcje przy luce, trzecia odpowiedź banera, edycja startu trwającego pomiaru.',
    testChanges: 'żadne',
  },
  {
    id: 'B4', phase: 'Codzienna praca', title: 'Wysyłka do TS: nadgodziny, edycja godzin, przypisanie, porównanie, wiele dni',
    section: 'B4 — Wysyłka do TS',
    scope: 'Wszystkie 5 punktów sekcji. Nazwę pola nadgodzin w API TS ustal TYLKO czytając kod w C:\\Projekty\\Private\\ideo-ts-api (tylko odczyt, niczego tam nie zmieniaj, niczego nie uruchamiaj) i obecny timesheet.rs; jeśli nie da się jej ustalić, przyjmij nazwę z kodu klienta, oznacz jako niepewną w manualChecks i trzymaj ją w jednej stałej. Funkcja overtime(day, sessions, settings) w src/report.ts (norma = cel dzienny z Ustawień → Czas pracy; dzień wolny lub święto: cała praca to nadgodziny) — B5 użyje tej samej. Podział w podglądzie „Wyślij do TS”: nadwyżka odejmowana od ostatnich chronologicznie wpisów; godziny i nadgodziny edytowalne (dwa miejsca, przecinek, „Przywróć z pomiarów”), sumy osobno, ostrzeżenie przy przekroczeniu normy. Przypisanie zadania TS z podglądu przez SearchSelect (B1) z opcją „domyślne dla projektu”. ts_send zwraca listę faktycznie wysłaną i z nią porównuje (koniec fałszywego rozjazdu). „Wyślij dni do TS” w MultiDayReport z jednym podglądem (zakładki dni) i jednym podsumowaniem. Symulowany TS w podglądzie obsługuje nadgodziny.',
    done: 'Rust: nadgodziny w ts_send, porównanie z listą faktycznie wysłaną (zmiana w TS między otwarciem a zapisem nie daje rozjazdu). Jednostkowe overtime() i podziału (norma, dzień wolny, nadwyżka z ostatnich wpisów). Playwright na symulowanym TS: podział, edycja godzin i nadgodzin, przypisanie z podglądu, wysyłka wielu dni.',
    testChanges: 'oczekiwane ciała wysyłki TS w istniejących testach mogą dostać pole nadgodzin = 0; poza tym bez zmian',
  },
  {
    id: 'B5', phase: 'Codzienna praca', title: 'Raport dnia jak kafelki, nadgodziny, lista Jiry, stan obserwacji Jiry',
    section: 'B5 — Raport dnia, lista Jiry, stan obserwacji Jiry',
    scope: 'Wszystkie 4 punkty sekcji: prezentacyjny TaskSummary.vue wydzielony z TaskCard i użyty w Raporcie dnia (bez akcji Start/Ukończ; Redmine przez issueRef jak w kafelku); znacznik „W zadaniach · dziś HH:MM” i ostrzeżenie o duplikacie w liście Jiry (i analogicznie w widoku Redmine, jeśli ma ten sam problem); nadgodziny dnia z funkcji overtime z B4 w Raporcie dnia, karcie „Dzisiaj”, wykresie 7 dni (fragment słupka innym kolorem + suma tygodnia) i w Miesiącu (kolumna + suma); JiraState z ostatnim błędem i czasem ostatniego udanego sprawdzenia, linia stanu w widoku Jira i Ustawienia → Jira.',
    done: 'Playwright: Raport dnia (zrzut), znacznik w liście Jiry, linia stanu po błędzie z symulowanej Jiry, nadgodziny na Dashboardzie i w Miesiącu. Rust: zapis błędu obserwacji w JiraState. Zrzuty zaktualizowane świadomie.',
    testChanges: 'zrzuty wzorcowe Raportu dnia, Dashboardu i Miesiąca; asercje treści Raportu dnia, jeśli lista zmieniła strukturę (opisz)',
  },
  {
    id: 'B12', phase: 'Codzienna praca', title: 'Ręczne spotkania',
    section: 'B12 — Ręczne spotkania',
    scope: 'Wszystkie 6 punktów sekcji: tabela manual_meetings (CREATE TABLE IF NOT EXISTS, id z prefiksem manual:, task_id ON DELETE SET NULL, w kopii SQLite) osobno od calendar_events, żeby replace_range i calendar_disconnect jej nie kasowały; calendar::meetings_between łącząca Outlook (tylko przy połączonym koncie) i ręczne (zawsze), CalendarEvent z polami source ("outlook" domyślnie / "manual") i taskId; przepięcie calendar_events, capacity.rs, notify.rs, przeglądu tygodnia i frontendu (DayTimeline, capacity.ts, Rano, Zadania → Dzisiaj, Zamknięcie dnia, tablica i przegląd tygodnia) na wspólne źródło, tak żeby ręczne spotkania działały bez połączonego kalendarza; komendy meeting_add / meeting_update / meeting_delete z walidacją z sekcji i odpowiednikami w podglądzie; MeetingEditor.vue (temat, dzień, od–do albo Cały dzień, typ, opcjonalne zadanie przez SearchSelect) z wejściami z toru Spotkania osi dnia, Plan i historia → Dzień, Zadania → Dzisiaj i tablicy tygodnia; edycja i Usuń tylko dla ręcznych (Outlook tylko do odczytu), znacznik „ręczne”; „Dodaj jako czas” w Zamknięciu dnia wypełnia zadanie ręcznego spotkania; „Pokazuj tytuły spotkań” tylko dla Outlooka. Bez cykli.',
    done: 'Rust: walidacja komend, meetings_between (oba źródła, brak konta, kolejność), ręczne spotkania nietknięte po replace_range i calendar_disconnect, pojemność z ręcznym spotkaniem bez połączonego kalendarza, ON DELETE SET NULL. Playwright: dodanie z osi dnia, edycja i usunięcie, pojemność i plan omijający spotkanie, Rano i Zamknięcie dnia z „Dodaj jako czas” z zadaniem, tor Spotkania bez połączonego kalendarza; nowy zrzut osi dnia z ręcznym spotkaniem.',
    testChanges: 'mechaniczne dopisanie pól source/taskId w literałach CalendarEvent w testach Rust (notify.rs, capacity.rs, calendar.rs) i zrzuty osi dnia z przyciskiem „Dodaj spotkanie”; oczekiwane wartości pojemności i spotkań z Outlooka bez zmian',
  },
  {
    id: 'B2', phase: 'Układ i wydajność', title: 'Układ i nawigacja',
    section: 'B2 — Układ i nawigacja',
    scope: 'Wszystkie 7 punktów sekcji: przewinięcie do góry przy zmianie widoku (z zachowaniem pozycji przy powrocie z KeepAlive); main.wide dla widoków tablicowych (Jira → Tablica, Plan → Tydzień → Tablica, widok Redmine, jeśli ma szeroki układ); karta „Zagrożone teraz” bez łamania po literze przy 900–1150 px; watch(todayKey) w JournalView i MonthReport; „jutro” = nextWorkDay z datą w etykiecie w Zamknięciu dnia, Dniu i Rano; jedna funkcja dopasowania wyszukiwania (tytuł, projekt, klucz Jiry, #numer Redmine) w Zadaniach, Ctrl+K i tablicy tygodnia; „Ukończone”/„Wszystkie” domyślnie 30 dni z „Pokaż starsze” i grupowaniem po dniu ukończenia.',
    done: 'Playwright: przewijanie, main.wide, zmiana dnia zegarem przez północ, wyszukiwanie po kluczu, „jutro” w piątek, limit 30 dni; zrzut Dashboardu przy 1000 px.',
    testChanges: 'testy oczekujące przeniesienia na następny dzień kalendarzowy (piątek → sobota) zmienione na dzień roboczy; zrzuty widoków tablicowych',
  },
  {
    id: 'B6', phase: 'Układ i wydajność', title: 'Wydajność',
    section: 'B6 — Wydajność',
    scope: 'Wszystkie 7 punktów sekcji: data-changed z etykietą okna źródłowego i bez refreshu we własnym oknie, emisja z wątku cykli tylko przy zmianie (generate → bool); generate zapisuje tylko gdy last_day rośnie i nie jest wołane z get_snapshot; settings::load jednym SELECT z cache w AppHandle unieważnianym w set_settings i przy przywróceniu kopii; sesje pogrupowane po taskId w computed (od now zależy tylko aktywny przedział) w TasksView; Dashboard i „Szukaj w historii” bez pełnych przeliczeń co sekundę; komenda wsadowa save_day_notes i planTasks dla „Przenieś niedokończone”.',
    done: 'Rust: generate zwraca bool i nie zapisuje bez zmian, cache ustawień (unieważnienie po set_settings i po przywróceniu kopii), save_day_notes. Playwright: liczba wywołań get_snapshot po akcji (fixture calls) i brak regresji widoków.',
    testChanges: 'testy liczące wywołania IPC mogą oczekiwać mniej wywołań; poza tym bez zmian',
  },
  {
    id: 'B11', phase: 'Układ i wydajność', title: 'Drobne funkcje',
    section: 'B11 — Drobne funkcje',
    scope: 'Wszystkie 4 punkty sekcji: Ctrl+Shift+S Start/Stop i N = fokus „Nowe zadanie” z listą w Ustawienia → Skróty; „Dodaj czas ręcznie” z początkiem z dayStart albo końca ostatniego przedziału i końcem = teraz dla dzisiaj; suma dnia „dziś 5:10 / 8:00” w podlinii widgetu; plan dnia (Rano) przy pierwszym uruchomieniu/pokazaniu okna w dniu roboczym bez zapisanego przeglądu, z regułami świąt/urlopu/startu do traya i przełącznikiem w Ustawienia → Uruchamianie (domyślnie włączony), stan „pokazano dziś” w pamięci okna (podgląd: localStorage).',
    done: 'Playwright: skróty, domyślne godziny w „Dodaj czas ręcznie”, suma w widgecie, start z zapisanym przeglądem i bez, zmiana dnia zegarem, wyłączony przełącznik.',
    testChanges: 'testy startujące aplikację, które oczekują listy zadań jako pierwszego widoku: fixture z zapisanym przeglądem dnia albo wyłączonym przełącznikiem (opisz, które)',
  },
  {
    id: 'B8', phase: 'Agenci', title: 'Wykrywanie agentów przez hooki',
    section: 'B8 — Wykrywanie agentów przez hooki',
    scope: 'Całość sekcji: src-tauri/src/bin/todo-track-hook.rs (stdin JSON → potok \\\\.\\pipe\\todo-track-hooks, cicho kończy się bez aplikacji; od razu odrzuca zdarzenia, gdy w środowisku jest TODO_TRACK_AI=1 — A1 na tym polega) dołączony do instalatora obok todo-track-bridge.exe; wątek potoku w głównym procesie; maszyna stanów sesji (praca / czekanie na użytkownika / koniec, subagenci liczeni do sesji nadrzędnej, timeout braku zdarzeń); rejestracja przez scalanie JSON w ~/.claude/settings.json i ~/.codex/hooks.json usuwająca tylko własne wpisy (obce wpisy Orki zostają) i z aktualizacją ścieżki exe; odczyt hooks.state z ~/.codex/config.toml i informacja o zatwierdzeniu przez /hooks; usunięcie starego skanu (sessions/<pid>.json, rollouty Codexa, transkrypty, process_alive, STALL, CODEX_FRESH) wraz z jego testami; rozpoznanie zadania (agents::resolve) bez zmian, z cwd z hooka; pierwszy prompt z UserPromptSubmit przycięty do 500 znaków trzymany w pamięci sesji (pod A4); Ustawienia → „Agenci” z przełącznikiem, stanem zaufania Codexa i listą ostatnich zdarzeń. Ścieżki plików konfiguracyjnych wstrzykiwane (parametr / funkcja), żeby testy pracowały w katalogu tymczasowym. Podgląd przeglądarkowy: kafelek Agenci AI zasilany zdarzeniami z fixture.',
    done: 'Rust: parser zdarzeń Claude i Codexa (z fixture JSON), maszyna stanów (praca, czekanie na zgodę, koniec, subagent, timeout), scalanie JSON z obcymi wpisami Orki (dodanie, aktualizacja ścieżki, usunięcie tylko własnych), odczyt hooks.state, odrzucenie zdarzenia z TODO_TRACK_AI=1. Playwright: kafelek Agenci AI na zdarzeniach z fixture, sekcja Ustawień „Agenci”. Wszystkie testy w katalogu tymczasowym.',
    testChanges: 'usunięcie testów starego skanu plików w agents.rs (wymień je w deviations); testy agents::resolve i kafelka bez zmian oczekiwanych wartości',
  },
  {
    id: 'B9', phase: 'Agenci', title: 'Worktree Orki z kafelka zadania',
    section: 'B9 — Worktree Orki z kafelka zadania',
    scope: 'Całość sekcji: moduł orca.rs budujący polecenia orca worktree create/list i reference find (--json) i parsujący wyjście; akcja „Otwórz worktree w Orce” w rozwiniętym kafelku zadania ze zgłoszeniem Jiry lub Redmine (nazwa z klucza i tytułu, --reference, repozytorium z agent_bindings albo wybór z listy Orki, opcjonalny prompt, wybór agenta), dialog z podglądem polecenia, uruchomienie tylko po kliknięciu; mapowanie folder → klucz z orca worktree list jako dodatkowe źródło dopasowania agenta; w sekcji Ustawień „Agenci” (z B8) ścieżka orca.cmd (wykrywana: %LOCALAPPDATA%\\Programs\\orca\\resources\\bin\\orca.cmd) i „Uruchamiaj Orkę, gdy nie działa” (domyślnie wyłączone). Symulowana Orka w podglądzie.',
    done: 'Rust: budowa poleceń, parsowanie JSON z fixture (worktree list, reference find, status), mapowanie folder → klucz. Playwright: dialog w podglądzie (podgląd polecenia, wybór repo i agenta, komunikat „Orca nie działa”). Żaden test nie uruchamia prawdziwego orca.cmd.',
    testChanges: 'żadne',
  },
  {
    id: 'A0', phase: 'AI: fundamenty', title: 'Filtr danych osobowych (ai_privacy.rs)', ai: true,
    section: 'A0 — Filtr danych osobowych (`src-tauri/src/ai_privacy.rs`)',
    scope: 'Całość sekcji: redact(input, &Dictionary) -> (String, Pseudonyms) dla tekstu i JSON; pola osobowe po kształcie danych (przypisany/zgłaszający Jiry, pola osobowe z jira_fields.kind, przypisany w Redmine, autorzy commitów) → „Osoba N” stałe w obrębie wywołania; słownik z nazw osób z Jiry/Redmine (imię i nazwisko osobno), własnego imienia i nazwiska użytkownika oraz ustawienia ai_hidden_names (pole w settings.rs tu, interfejs w A1); dopasowanie bez wielkości liter, po granicy słowa, z polską odmianą przez rdzeń, pomijanie imion < 3 znaków; restore odwracające pseudonimy w odpowiedzi; mapa tylko w pamięci. Moduł bez interfejsu — korzystają z niego A1–A5.',
    done: 'Testy Rust z sekcji: zagnieżdżony JSON i listy, stałość pseudonimów, fixture z 20 formami odmiany, „Marzec” podmieniany wszędzie (test dokumentuje), brak trafienia w „Annapurna”, restore, test szczelności na fixture pełnego dnia (żadna nazwa ze słownika w wyjściu).',
    testChanges: 'żadne',
  },
  {
    id: 'A1', phase: 'AI: fundamenty', title: 'Silnik AI i wybór agenta (ai.rs)', ai: true,
    section: 'A1 — Silnik AI i wybór agenta (`src-tauri/src/ai.rs`)',
    scope: 'Całość sekcji: wykrycie claude i codex (kolejność miejsc z planu, wersja z --version) z możliwością wstrzyknięcia ścieżek w testach; ustawienia ai_* z listy w sekcji z wartościami domyślnymi (ai_engine = ""); dialog pierwszego uruchomienia (Claude Code / Codex tylko wykryte / Nie używaj; zamknięcie = pytaj ponownie; brak CLI = brak dialogu); ai::ask(kind, prompt, schema) z liniami poleceń dokładnie jak w planie, katalogiem %APPDATA%\\pl.todotrack.desktop\\ai\\, TODO_TRACK_AI=1, CREATE_NO_WINDOW, limitem 60 s, jednym procesem na rodzaj, błędami po polsku, wejściem przez ai_privacy::redact i wyjściem przez restore, pamięcią podręczną (rodzaj + skrót wejścia); agents.rs ignoruje cwd równe katalogowi AI; komponent Ustawienia → Asystent AI (silnik, wersje, „Sprawdź ponownie”, ścieżki, modele, „Ukrywaj te nazwy”, przełączniki spotkań i okien, przełączniki funkcji, „Ostatnie wywołania” bez treści); „Pokaż, co zostanie wysłane” jako wspólny komponent dla A3–A6. Uruchamianie procesu za cechą (trait) tak, żeby testy podstawiały zapisane odpowiedzi. Podgląd: symulowany silnik w src/preview/ai.ts (wykrycie, odpowiedzi ze skryptu).',
    done: 'Rust: linie poleceń obu silników (fixture), parsowanie claude -p --output-format json i JSONL codex exec --json z zapisanych plików, walidacja schematu, mapowanie błędów (niezalogowany, limit, czas), logika dialogu pierwszego uruchomienia, ignorowanie katalogu AI w agents.rs. Playwright: dialog (0, 1, 2 wykryte CLI), wybór zapisany po przeładowaniu, sekcja Ustawień. Żaden test nie uruchamia prawdziwego claude ani codex.',
    testChanges: 'testy startujące aplikację: fixture z ai_engine ustawionym na "off", żeby dialog nie zasłaniał widoków (opisz); zrzuty Ustawień (nowa sekcja)',
  },
  {
    id: 'A2', phase: 'AI: fundamenty', title: 'Serwer MCP (todo-track-mcp.exe)', ai: true,
    section: 'A2 — Serwer MCP (`src-tauri/src/bin/todo-track-mcp.rs`, `src-tauri/src/mcp.rs`)',
    scope: 'Całość sekcji: todo-track-mcp.exe (stdio JSON-RPC MCP: initialize, tools/list, tools/call) przekazujący wywołania potokiem \\\\.\\pipe\\todo-track-mcp-<użytkownik> do głównego procesu, błąd „Todo Track nie działa” bez aplikacji, dołączony do instalatora; mcp.rs w głównym procesie z narzędziami odczytu (tasks_list, task_get, day_summary, week_summary, current_measurement, ts_tasks) i narzędziami propose_* tworzącymi wpisy w kolejce ai_proposals (w pamięci) zamiast zmian; wynik każdego narzędzia przez ai_privacy::redact; tytuły spotkań i okien tylko przy włączonych przełącznikach; brak narzędzi TS, Jiry, Redmine, usuwania i ustawień; baner propozycji w aplikacji z „Zastosuj” / „Odrzuć” wywołującym istniejące komendy; rejestracja przez claude mcp add/remove/get i codex mcp add/remove/list za przełącznikiem ai_mcp, ze sprawdzeniem ścieżki exe przy starcie — budowa poleceń w funkcjach testowalnych, uruchamianie za cechą (trait), żeby testy nie wywoływały prawdziwego CLI; opisy narzędzi po angielsku, krótkie.',
    done: 'Rust: wymiany MCP z zapisanych plików (initialize, list, call, błąd, aplikacja nie działa), każde narzędzie odczytu na bazie w pamięci, test szczelności wyjścia (jak A0), propose_* nie zmienia bazy i trafia do kolejki, budowa poleceń rejestracji i parsowanie mcp get / mcp list. Playwright: baner propozycji (Zastosuj tworzy zadanie, Odrzuć nic nie zmienia).',
    testChanges: 'żadne',
  },
  {
    id: 'A3', phase: 'AI: funkcje', title: 'Czat w aplikacji', ai: true,
    section: 'A3 — Czat w aplikacji',
    scope: 'Całość sekcji: AiChat.vue jako panel po prawej (Ctrl+J, pozycja w nawigacji, chowany, szerokość zapamiętana; widoczny tylko przy wybranym silniku i ai_chat); kontekst z kafelka, Zamknięcia dnia i Ctrl+K („?”); silnik Claude jako długo żyjący proces stream-json z --mcp-config wskazującym todo-track-mcp i --allowedTools "mcp__todo-track__*" (linia poleceń jak w planie), Codex przez codex exec na wiadomość z historią; strumieniowanie do UI; karty akcji z propozycji A2 (Zastosuj / Odrzuć / Popraw otwierający zwykły formularz) wywołujące istniejące komendy; lista użytych narzędzi, „Pokaż, co zostało wysłane”, „Zatrzymaj”, „Nowa rozmowa”; historia tylko w pamięci; każda wiadomość użytkownika przez redact, odpowiedzi przez restore. Symulowany asystent w src/preview/ai.ts ze skryptowanymi odpowiedziami i kartami.',
    done: 'Rust: parser stream-json (tekst, wywołanie narzędzia, koniec, błąd) na zapisanych strumieniach, zarządzanie procesem (start, zatrzymanie, ubity proces) na procesie zastępczym. Playwright tests/ai-chat.spec.ts: Ctrl+J, pytanie i odpowiedź z symulatora, karta „Utwórz zadanie” → Zastosuj tworzy zadanie, Odrzuć nic nie zmienia, kontekst z kafelka, panel ukryty przy wyłączonym silniku; nowy zrzut wzorcowy panelu.',
    testChanges: 'zrzuty nawigacji (nowa pozycja „Asystent”)',
  },
  {
    id: 'A4', phase: 'AI: funkcje', title: 'Asystent przypisań', ai: true,
    section: 'A4 — Asystent przypisań',
    scope: 'Całość sekcji: dla sesji agenta pracującej ≥ suggest::MIN_WORK bez dopasowania z agents::resolve i bez wyciszenia — ai::ask("assign", …) z nazwą folderu, gałęzią, pierwszym promptem z B8 i listą otwartych zadań (id, tytuł, projekt, klucz), schemat { taskId | null, confidence, reason }; confidence ≥ 0,6 → baner w suggest.rs ze źródłem „Asystent” (Przypisz folder zapisuje agent_bindings, Przełącz timer, Nie wycisza parę); jedno wywołanie na session_id; brak wywołania, gdy heurystyka dopasowała. „Podpowiedz” w polu Zadanie TS kafelka i „Podpowiedz wszystkie” w podglądzie „Wyślij do TS” (ai::ask("ts_task", …)) wypełniające sugestię z uzasadnieniem, zapis dopiero po wyborze, „Ustaw jako domyślne dla projektu” zaznaczone, gdy projekt nie ma mapowania. Za przełącznikiem ai_assign_suggest.',
    done: 'Rust: dane wejściowe bez nazwisk (asercja A0), próg pewności, pamięć podręczna po session_id, brak wywołania przy dopasowaniu heurystyką lub wyłączonym przełączniku. Playwright z symulatorem: baner (Przypisz folder, Nie), Podpowiedz i Podpowiedz wszystkie w TS.',
    testChanges: 'żadne',
  },
  {
    id: 'A5', phase: 'AI: funkcje', title: 'Zamknięcie dnia z asystentem', ai: true,
    section: 'A5 — Zamknięcie dnia z asystentem',
    scope: 'Całość sekcji: sekcja „Asystent” w Zamknięciu dnia (przy ai_day_close) z przyciskiem „Przeanalizuj dzień” i „Pokaż, co zostanie wysłane”; zebranie danych dnia (oś dnia, luki, czas agentów z przypisaniami, spotkania bez tytułów przy wyłączonym przełączniku, commity z folderów agent_bindings przez git log --since --until --format=%H%x09%ad%x09%s bez autora, zmiany statusów Jiry/Redmine z obserwacji, notatki dnia) przez redact; wynik według schematu propozycji { kind: gap|unassigned|note|ts_description, from, to, taskId, text, reason }; karty: luka → ManualTime wypełniony (albo fill_gap z B3, gdy luka jest między pomiarami), czas agenta → Przypisz, notatka → Dodaj do notatki (edytowalna), opis TS → proponowany opis w podglądzie „Wyślij do TS”; „Zastosuj wszystkie” tylko dla notatek.',
    done: 'Rust: zebranie danych dnia (repo git w katalogu tymczasowym, bez autorów w wyjściu), schemat i mapowanie propozycji. Playwright z symulatorem: sekcja, każda karta, brak zmian bez kliknięcia, opis TS w podglądzie wysyłki.',
    testChanges: 'żadne',
  },
  {
    id: 'A6', phase: 'AI: funkcje', title: 'Rano i przegląd tygodnia z asystentem', ai: true,
    section: 'A6 — Rano i przegląd tygodnia (opcjonalnie)',
    scope: 'Całość sekcji: w Rano przycisk „Komentarz asystenta” (ocena planu względem pojemności i terminów, model główny, bez zmian w planie; propozycje przesunięć jako karty jak w A3); w przeglądzie tygodnia „Szkic wniosków” wypełniający pole Wnioski (week_notes) propozycją do edycji. Wejście przez redact.',
    done: 'Playwright z symulatorem: komentarz w Rano z kartą przesunięcia (Zastosuj / Odrzuć), szkic wniosków edytowalny przed zapisem.',
    testChanges: 'żadne',
  },
  {
    id: 'N1', phase: 'Nazwa i wydanie 2.0', title: 'TukTuk Track: nazwa, identyfikator i migracja z Todo Track', migration: true,
    section: 'N1 — TukTuk Track: nazwa, identyfikator i migracja z Todo Track',
    scope: 'Wszystkie 6 punktów sekcji. Nazwa aplikacji „TukTuk Track” (ze spacją) wszędzie, nazwa kodowa tuktuk-track dla nazw technicznych, identyfikatory pl.tuktuktrack.* bez myślnika; (productName, okna, tray, powiadomienia, interfejs, instalator z grafikami z make-artwork.ps1, rozszerzenie, README/RELEASE.md/browser-extension/README.md, scripts/ship.mjs i release.mjs z nazwą pliku TukTuk Track_<wersja>_x64-setup.exe). Nazwy techniczne: pakiet i binaria tuktuk-track / tuktuk-track-bridge / tuktuk-track-hook / tuktuk-track-mcp, potoki \\\\.\\pipe\\tuktuk-track-*, TUKTUK_TRACK_AI, serwer MCP tuktuk-track (mcp__tuktuk-track__*), klucze localStorage tuktuk-track-*; identyfikator pl.tuktuktrack.desktop, host pl.tuktuktrack.bridge (klucz key rozszerzenia bez zmian). BEZ ZMIAN (decyzja użytkownika): repozytoria GitHub matkrajnik/todo-track-private (kod) i matkrajnik/todo-track (wydania), lokalny folder to-do-track, adres latest.json, pubkey i plik ~/.tauri/todotrack.key — 1.9 musi znaleźć aktualizację. Moduł legacy.rs przed otwarciem bazy: kopia %APPDATA%\\pl.todotrack.desktop → %APPDATA%\\pl.tuktuktrack.desktop tylko gdy nowy nie ma bazy (stary zostaje z PRZENIESIONO-DO-TUKTUK-TRACK.txt, oba z bazą = nic nie nadpisuje, informacja w Ustawieniach), najlepszym wysiłkiem %LOCALAPPDATA%\\pl.todotrack.desktop; poświadczenia jira/calendar/timesheet/redmine przenoszone z odczytem kontrolnym, stary wpis usuwany dopiero po udanym przeniesieniu, błąd jednego nie blokuje reszty; nowy host rozszerzenia zarejestrowany, stare klucze i manifest usunięte; autostart Run\\Todo Track → Run\\TukTuk Track z tymi samymi argumentami; stare wpisy hooków (todo-track-hook.exe) i serwera MCP todo-track usuwane przy rejestracji; znacznik legacy_migrated_at; rejestr, magazyn poświadczeń i katalogi za warstwą testowalną. Instalator (template.nsi): wykrycie Uninstall\\Todo Track, pytanie o aktualizację do TukTuk Track 2.0.0, zamknięcie starej aplikacji, ciche odinstalowanie bez usuwania danych (sprawdź w szablonie, że /S nie kasuje %APPDATA%), usunięcie starych skrótów, instalacja do %LOCALAPPDATA%\\TukTuk Track i uruchomienie. Kontrola kompletności w npm run check albo teście Rust (dozwolone wyjątki z sekcji). Nie buduj instalatora (bez npm run tauri build) — szablon sprawdź czytaniem i, jeśli się da, makensis w trybie samej kompilacji składni, bez instalowania.',
    done: 'Rust: migracja katalogu (brak starego, stary bez nowego, oba z bazą, ponowne uruchomienie), poświadczenia na zastępczym magazynie (sukces, brak wpisu, błąd zapisu, odczyt kontrolny przed usunięciem), budowa kluczy rejestru hosta i autostartu, usuwanie starych wpisów hooków i MCP, kontrola kompletności. Playwright: TukTuk Track w tytule, nawigacji i Ustawieniach, nowe klucze localStorage. Wszystkie dotychczasowe testy zielone po mechanicznej zmianie nazw; żaden test nie pisze do prawdziwego rejestru HKCU ani Menedżera poświadczeń.',
    testChanges: 'mechaniczna zmiana nazw w testach i fixture (Todo Track → TukTuk Track, todo-track → tuktuk-track, pl.todotrack → pl.tuktuktrack, klucze localStorage) i zrzuty wzorcowe z nową nazwą; żadnych zmian scenariuszy ani innych oczekiwanych wartości',
  },
  {
    id: 'R20', phase: 'Nazwa i wydanie 2.0', title: 'Przygotowanie wydania 2.0.0', release: '2.0.0', part: 'B, A i N',
    section: 'Wydanie 2.0.0 (R20)',
    scope: 'Jedno wydanie z całym planem. notes.md: najpierw nowa nazwa TukTuk Track i to, że dane, ustawienia i logowania przechodzą same z Todo Track, potem najważniejsze nowości. README: tytuł i opis TukTuk Track, „Nowości 2.0” (zmiana nazwy i migracja; pomiary i luki; wysyłka TS z nadgodzinami i wieloma dniami; raporty z nadgodzinami; ręczne spotkania także bez kalendarza Microsoft; SearchSelect i czytelne przyciski; plan dnia rano; hooki agentów zamiast plików sesji z zatwierdzeniem w Codexie przez /hooks; worktree Orki; Asystent AI: subskrypcja Claude Code / Codex bez klucza API, wybór przy pierwszym uruchomieniu, co wychodzi do modelu i filtr nazwisk z ograniczeniem słownika, serwer MCP z rejestracją i przykładem wpisu do CLAUDE.md z propose_note_append, czat i karty akcji, model tylko proponuje), opisy sekcji ustawień „Agenci”, „Asystent AI”, „Skróty”, ręczne spotkania w opisie kalendarza, sekcja „Przejście z Todo Track” (co się przenosi, gdzie zostaje stary katalog, rozszerzenie do przeładowania), „Wersja w repozytorium”.',
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
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_BACKLOG.md i dlaczego, w tym każda zmiana istniejącego testu' },
    readme: { type: 'string', description: 'Co trzeba napisać w README przy wydaniu (funkcja, ustawienia, ograniczenia)' },
    manualChecks: { type: 'string', description: 'Co wymaga ręcznego sprawdzenia w natywnej aplikacji, z prawdziwym TS/Jirą/Redmine/Orką/CLI albo po instalacji NSIS' },
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
    noNetwork: { type: 'boolean', description: 'Żaden test ani skrypt nie łączy się z prawdziwym TS, Jirą ani Redmine' },
    noRealTools: { type: 'boolean', description: 'Żaden test nie uruchamia prawdziwego claude, codex ani orca i nie zapisuje do ~/.claude, ~/.codex, rejestru HKCU, Menedżera poświadczeń ani katalogów danych aplikacji' },
    testsIntact: { type: 'boolean', description: 'Istniejące testy zmieniły się wyłącznie w zakresie dozwolonym dla zadania' },
    playwrightCount: { type: 'number' },
    rustCount: { type: 'number' },
    failures: { type: 'string', description: 'Pełne komunikaty błędów, nazwy testów, fragmenty logu' },
    uncommitted: { type: 'boolean', description: 'git status pokazuje niezacommitowane zmiany' },
  },
  required: ['ok', 'build', 'playwright', 'cargoTest', 'cargoFmt', 'noNetwork', 'noRealTools', 'testsIntact', 'failures', 'uncommitted'],
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
Projekt: ${ROOT} (worktree repozytorium to-do-track; Tauri 2 + Rust + Vue 3, interfejs po polsku), gałąź ${BRANCH}. Pracuj WYŁĄCZNIE w tym katalogu — nie zmieniaj plików w głównym katalogu C:\\Projekty\\Private\\to-do-track poza tym worktree, nie zmieniaj innych worktree i nie przełączaj gałęzi. Repozytorium C:\\Projekty\\Private\\ideo-ts-api wolno tylko czytać.
Źródło prawdy: ${PLAN}. Przeczytaj go w całości (zwłaszcza „Kolejność realizacji”, „Założenia”, a przy zadaniach A także „Decyzje użytkownika dla części A” i „Założenia części A”), potem sekcję swojego zadania. Numery linii w planie pochodzą z przeglądu 20 października 2026 — kod mógł się zmienić (1.9 dodało Redmine, wcześniejsze zadania tego planu przenoszą kod), więc lokalizuj miejsca po nazwach, nie po numerach. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu, dane użytkownika nietykalne). Przejrzyj git log --oneline $(git merge-base master HEAD)..HEAD, żeby wiedzieć, co zrobiły wcześniejsze zadania (np. B10 przeniosło symulatory do src/preview/).

ZAKAZY BEZWZGLĘDNE:
- Żadnych żądań HTTP do prawdziwego TS, Jiry ani Redmine — ani ręcznie, ani w testach, ani w skryptach. Testy Rust na plikach z fixtures, Playwright na symulatorach podglądu.
- Nie uruchamiaj prawdziwego claude, codex ani orca (także --version), ani w testach, ani ręcznie. Kod uruchamiający procesy ma być za cechą (trait) albo parametrem, a testy podstawiają zapisane odpowiedzi.
- Nie czytaj i nie zmieniaj ~/.claude/settings.json, ~/.claude.json, ~/.codex/* ani rejestracji MCP; nie wywołuj claude mcp / codex mcp. Testy hooków i rejestracji działają na kopiach w katalogu tymczasowym. Ta sesja sama działa w Claude Code — zmiana jego ustawień zepsułaby workflow.
- Nie uruchamiaj aplikacji desktopowej (npm run tauri dev, todo-track.exe): przy starcie rejestruje hooki i MCP w plikach użytkownika. Wystarcza podgląd przeglądarkowy i testy.
- Nie dotykaj bazy użytkownika w %APPDATA%\\pl.todotrack.desktop (ani %APPDATA%\\pl.tuktuktrack.desktop), Menedżera poświadczeń Windows, rejestru HKCU ani zainstalowanej aplikacji. Nie instaluj NSIS i nie uruchamiaj instalatora ani deinstalatora.
- Aplikacja zmienia w N1 nazwę z Todo Track na TukTuk Track. Zadania przed N1 używają obecnych nazw (todo-track-*, pl.todotrack.*, TODO_TRACK_AI, serwer MCP todo-track) tak, jak opisuje je plan; nowe nazwy wprowadza dopiero N1.

Zasady:
- Każda nowa komenda Tauri ma odpowiednik w trybie podglądu (store.ts / src/preview/*.ts), bo testy Playwright działają bez Rusta.
- Nowe tabele SQLite tylko przez CREATE TABLE IF NOT EXISTS; nowe pola zadań w task_details z domyślnymi; nowe ustawienia w AppSettings z domyślnymi i walidacją; bez niszczących migracji.
- Nic nie trafia do TS, Jiry, Redmine ani Orki bez kliknięcia; timer nigdy nie przełącza się sam; model AI tylko proponuje (karty „Zastosuj” / „Odrzuć”), a każda ścieżka do modelu przechodzi przez ai_privacy::redact.
- Nowe teksty interfejsu po polsku. Zachowaj istniejące etykiety aria, bo testy ich używają.
- Istniejące testy zmieniaj tylko w zakresie dozwolonym dla zadania i opisz każdą zmianę w deviations. Świadomą zmianę wyglądu potwierdź aktualizacją zrzutów (npm run test:visual:update) i wymień w deviations.
- Nie pushuj, nie twórz tagów, nie publikuj wydania. Commity lokalne na gałęzi ${BRANCH}, bez stopki Co-Authored-By (jak ostatnie commity na master).
- Polecenia: npm run build (vue-tsc + vite), npm test (Playwright), cargo test --manifest-path src-tauri/Cargo.toml --locked, cargo fmt --manifest-path src-tauri/Cargo.toml; npm run check uruchamia wszystko. cargo zwykle NIE jest w PATH: dodaj %USERPROFILE%\\.cargo\\bin (w Git Bash: export PATH="$HOME/.cargo/bin:$PATH"). Jeśli brak node_modules, najpierw npm ci.
- Nowe zależności Cargo lub npm tylko gdy konieczne, z konkretną wersją i zaktualizowanymi plikami lock; --locked musi przechodzić.
`

function implementPrompt(task) {
  return `${COMMON}
ZADANIE ${task.id}: ${task.title}
Nie zmieniaj numeru wersji. README i NEXT_SESSION.md aktualizuje dopiero zadanie przygotowania wydania — w polu readme opisz dokładnie, co trzeba w nich napisać.
Sekcja planu: "${task.section}" w ${PLAN}.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}
Dozwolone zmiany istniejących testów: ${task.testChanges}.

Wykonaj całość zakresu, dopisz testy, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit "${task.id}: ${task.title}" z dwoma zdaniami treści po polsku.
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
W manualChecks wypisz tylko rzeczy nowe albo doprecyzowane względem listy „Do ręcznego sprawdzenia” w planie.
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, readme, manualChecks, blocker.`
}

function releasePrompt(task, notes) {
  return `${COMMON}
ZADANIE ${task.id}: przygotowanie wydania ${task.release}. ${task.scope}
Wydanie robi użytkownik jednym poleceniem \`npm run ship -- ${task.release}\` na master (RELEASE.md, scripts/ship.mjs): ono podbija wersję, buduje i podpisuje instalator, publikuje, commituje, taguje i pushuje. Ty tylko PRZYGOTOWUJESZ wydanie na gałęzi ${BRANCH}. NIE zmieniaj wersji w manifestach, NIE uruchamiaj npm run ship / npm run release / npm run tauri build, NIE twórz tagu, NIE ruszaj releases/. Kroki:
1. notes.md: nadpisz notatkami wydania ${task.release} (po polsku, krótko, dla użytkowników; wzoruj się na obecnym notes.md).
2. README: sekcja „Nowości ${task.release.replace(/\.0$/, '')}” nad dotychczasowymi nowościami, opisująca to, co faktycznie zrobiono (git log zadań części ${task.part}), bez obietnic; opisy ustawień i sekcje funkcji według poniższych wskazówek z zadań; wiersz „Wersja w repozytorium” i blok „Weryfikacja ${task.release}” z liczbami testów. NEXT_SESSION.md: nowy wpis „Stan po ${task.release}” na górze z wynikami weryfikacji, odstępstwami i listą rzeczy do ręcznego sprawdzenia: pozycje części ${task.part} z sekcji „Do ręcznego sprawdzenia” w ${PLAN} plus uwagi zgłoszone przez zadania.
Wskazówki do README, odstępstwa i uwagi z zadań:
${notes || '(brak zgłoszonych)'}
3. Pełna weryfikacja: npm run build, npm test, cargo fmt --check, cargo test --locked.
4. Commit "${task.release}: przygotowanie wydania (README, notes.md, NEXT_SESSION)" (bez stopki Co-Authored-By). Bez tagu, bez pusha. W NEXT_SESSION.md zapisz, że wydanie wymaga scalenia ${BRANCH} do master i \`npm run ship -- ${task.release}\` przez użytkownika.
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
6. Brak sieci: przejrzyj diff gałęzi (git diff $(git merge-base master HEAD) -- src src-tauri tests scripts package.json) — w testach i skryptach mogą występować tylko adresy przykładowe i 127.0.0.1; żaden test nie tworzy klienta HTTP do prawdziwego serwera. noNetwork=true tylko gdy to potwierdzisz.
7. Brak prawdziwych narzędzi: w diffie żaden test (#[cfg(test)], tests/*.spec.ts) nie uruchamia procesu claude, codex ani orca (Command::new z tymi nazwami lub ścieżkami bez podstawienia) i nie używa prawdziwych ścieżek ~/.claude, ~/.codex, %APPDATA%/%LOCALAPPDATA% aplikacji (zamiast katalogu tymczasowego) ani prawdziwego rejestru HKCU i Menedżera poświadczeń (keyring::Entry, winreg bez warstwy zastępczej). noRealTools=true tylko gdy to potwierdzisz.
8. Testy nietknięte: git diff $(git merge-base master HEAD) -- tests src-tauri/src (bloki #[cfg(test)]) dla testów istniejących przed gałęzią. Dla zadań wcześniejszych tej gałęzi zmiany już są — oceniaj tylko commity zadania ${task.id} (git log --oneline --grep "^${task.id}:"). Dozwolone dla ${task.id}: ${task.testChanges || 'aktualizacja liczb testów w README'}. Każda inna zmiana oczekiwanej wartości, usunięty test albo zmieniony scenariusz to testsIntact=false z opisem w failures.
Nie poprawiaj niczego. Zwróć ok=true tylko gdy kroki 1–5 przeszły, noNetwork=true, noRealTools=true, testsIntact=true i nie ma niezacommitowanych zmian. W failures wklej dokładne komunikaty błędów i nazwy testów, które padły. Podaj liczbę testów Playwright i Rust.`
}

function fixPrompt(task, failures, round) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}) ma czerwoną weryfikację (runda poprawek ${round}). Wynik weryfikatora:
${failures}

Znajdź przyczynę i napraw ją w kodzie lub w nowych testach. Nie wyłączaj i nie kasuj testów. Zrzut wzorcowy, który pada przez świadomą zmianę wyglądu z tego zadania, możesz zaktualizować; przez przypadkową zmianę — napraw kod. Istniejący test spoza dozwolonego zakresu (${task.testChanges || '—'}) naprawiasz w kodzie, nigdy w teście. Uruchom ponownie polecenia, które padły, oraz cargo fmt. Zrób commit "${task.id}: poprawki po weryfikacji (${round})".
Zwróć wynik w schemacie IMPL (status done, gdy wszystko przechodzi; blocked z opisem, gdy nie).`
}

function reviewPrompt(task, lens, impl) {
  const lenses = {
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff commitów zadania ${task.id} (git log --oneline --grep "^${task.id}:", git show) z sekcją "${task.section}" w ${PLAN} i z jego założeniami. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Zawsze must: brakujący odpowiednik w podglądzie dla nowej komendy, zapis do TS/Jiry/Redmine/Orki bez kliknięcia, timer przełączany automatycznie, nowe ustawienie bez wartości domyślnej, niszcząca migracja, zmiana istniejącego testu poza dozwolonym zakresem (${task.testChanges || '—'}).`,
    bugs: `Soczewka: POPRAWNOŚĆ. Przeczytaj diff commitów zadania ${task.id} (git log --oneline --grep "^${task.id}:", git show). Szukaj błędów: dzielenie przez lokalną północ i strefy czasu, nakładanie przedziałów, aktywny pomiar, nadpisywanie innych pól JSON w task_details (wzorzec json_set), niespójność między Rust a podglądem, blokowanie wątku głównego lub wątku potoku, wycieki nasłuchów Tauri w onUnmounted, wyścigi przy równoległych akcjach, liczby i przecinki (godziny TS), dostępność (aria, klawiatura, focus w dialogu), regresje we wspólnych komponentach. Zgłaszaj tylko to, co umiesz uzasadnić konkretną ścieżką wykonania.`,
    migration: `Soczewka: MIGRACJA Z TODO TRACK. Przeczytaj diff commitów zadania ${task.id}. Prześledź pierwszy start 2.0 po instalacji 1.9 i po aktualizacji przez updater: czy baza (z -wal/-shm), kopie i stan okien trafiają do nowego katalogu przed otwarciem bazy; czy nic nie kasuje ani nie nadpisuje danych, gdy oba katalogi istnieją; czy stary katalog zostaje; czy stary wpis poświadczenia jest usuwany dopiero po zapisie i odczycie kontrolnym nowego; czy migracja jest idempotentna i nie powtarza się przy każdym starcie; czy host rozszerzenia, autostart, hooki i MCP po migracji wskazują nowe nazwy i ścieżki, a stare wpisy znikają; czy instalator przy cichym odinstalowaniu Todo Track nie usuwa %APPDATA% i nie zostawia dwóch instalacji; czy updater 1.9 nadal znajdzie 2.0 (endpoint, pubkey, repozytorium wydań bez zmian); czy w kodzie nie zostały stare nazwy poza dozwolonymi wyjątkami. Każde ryzyko utraty danych lub logowań to must.`,
    privacy: `Soczewka: PRYWATNOŚĆ I IZOLACJA AI. Przeczytaj diff commitów zadania ${task.id}. Prześledź każdą ścieżkę, którą dane trafiają do modelu (ai::ask, czat, odpowiedzi MCP, prompty): czy każda przechodzi przez ai_privacy::redact, czy odpowiedź przechodzi przez restore przed UI, czy mapa pseudonimów nie trafia do modelu ani na dysk, czy tytuły spotkań i okien wychodzą tylko przy włączonych przełącznikach, czy autorzy commitów są usunięci. Izolacja: TODO_TRACK_AI=1 i katalog AI przy każdym uruchomieniu CLI, brak narzędzi MCP do TS/Jiry/Redmine/usuwania/ustawień, propozycje bez zapisu do bazy przed kliknięciem, funkcje AI nieaktywne przy ai_engine "" / "off", żaden test nie uruchamia prawdziwego CLI. Każde naruszenie to must.`,
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
    const v = await runAgent(verifyPrompt(task), { label: `verify:${task.id}:${stageLabel}:${round}`, phase: task.phase, schema: VERIFY, effort: 'low' })
    if (!v) return { ok: false, failures: 'weryfikator nie zwrócił wyniku' }
    if (v.ok) return v
    if (round > maxRounds) return v
    log(`${task.id}: weryfikacja czerwona (runda ${round}): build=${v.build} playwright=${v.playwright} cargo=${v.cargoTest} fmt=${v.cargoFmt} noNetwork=${v.noNetwork} noRealTools=${v.noRealTools} tests=${v.testsIntact} uncommitted=${v.uncommitted}`)
    const extra = [
      v.uncommitted ? 'UWAGA: są niezacommitowane zmiany, dokończ commit.' : '',
      v.noNetwork === false ? 'UWAGA: weryfikator znalazł ścieżkę łączącą się z prawdziwym serwerem — usuń ją; testy nie mogą wychodzić do sieci.' : '',
      v.noRealTools === false ? 'UWAGA: test uruchamia prawdziwe claude/codex/orca albo używa prawdziwych katalogów użytkownika — podstaw proces zastępczy i katalog tymczasowy.' : '',
      v.testsIntact === false ? 'UWAGA: istniejące testy zmieniły się poza dozwolonym zakresem — przywróć je i napraw kod.' : '',
    ].filter(Boolean).join('\n')
    const fix = await runAgent(fixPrompt(task, v.failures + (extra ? `\n\n${extra}` : ''), round), { label: `fix:${task.id}:${stageLabel}:${round}`, phase: task.phase, schema: IMPL })
    if (!fix || fix.status === 'blocked') return { ok: false, failures: fix ? fix.blocker : 'agent poprawek nie zwrócił wyniku' }
  }
  return { ok: false, failures: 'wyczerpano rundy poprawek' }
}

async function runRelease(task, maxFixRounds, notes) {
  log(`▶ ${task.id}: ${task.title}`)
  const rel = await runAgent(releasePrompt(task, notes), { label: `release:${task.id}`, phase: task.phase, schema: IMPL })
  if (!rel) return { task: task.id, status: 'blocked', blocker: 'agent wydania nie zwrócił wyniku' }
  if (rel.status === 'blocked') return { task: task.id, status: 'blocked', blocker: rel.blocker, summary: rel.summary }
  const v = await verifyWithFixes(task, maxFixRounds, 'release')
  if (!v.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja wydania: ${v.failures}`, summary: rel.summary }
  return { task: task.id, status: 'done', summary: rel.summary, commit: rel.commit, deviations: rel.deviations || '', manualChecks: '', readme: '', review: 'wydanie bez przeglądu' }
}

async function runTask(task, maxFixRounds) {
  log(`▶ ${task.id}: ${task.title}`)
  const impl = await runAgent(implementPrompt(task), { label: `impl:${task.id}`, phase: task.phase, schema: IMPL })
  if (!impl) return { task: task.id, status: 'blocked', blocker: 'implementator nie zwrócił wyniku' }
  if (impl.status === 'blocked') return { task: task.id, status: 'blocked', blocker: impl.blocker, summary: impl.summary }

  const v1 = await verifyWithFixes(task, maxFixRounds, 'impl')
  if (!v1.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja po implementacji: ${v1.failures}`, summary: impl.summary }

  // Przegląd soczewkami równolegle (tylko odczyt, więc bez konfliktów w drzewie roboczym).
  const lenses = ['plan', 'bugs'].concat(task.ai ? ['privacy'] : [], task.migration ? ['migration'] : [])
  const reviews = (await parallel(lenses.map(lens => () =>
    runAgent(reviewPrompt(task, lens, impl), { label: `review:${task.id}:${lens}`, phase: task.phase, schema: REVIEW })
  ))).filter(Boolean)
  const findings = reviews.flatMap(r => r.findings).filter(f => f.severity !== 'nit')
  let reviewSummary = 'przegląd bez uwag'
  if (findings.length) {
    log(`${task.id}: przegląd zgłosił ${findings.length} uwag (${findings.filter(f => f.severity === 'must').length} must)`)
    const applied = await runAgent(applyReviewPrompt(task, findings), { label: `apply-review:${task.id}`, phase: task.phase, schema: IMPL })
    const v2 = await verifyWithFixes(task, maxFixRounds, 'review')
    if (!v2.ok) return { task: task.id, status: 'blocked', blocker: `weryfikacja po przeglądzie: ${v2.failures}`, summary: impl.summary }
    reviewSummary = `${findings.length} uwag wdrożonych; ${applied ? applied.deviations || '' : ''}`
  }
  return {
    task: task.id, status: 'done', summary: impl.summary, commit: impl.commit,
    deviations: impl.deviations || '', manualChecks: impl.manualChecks || '', readme: impl.readme || '', review: reviewSummary,
  }
}

// ---------------------------------------------------------------------------
// Główna pętla: ściśle sekwencyjnie, zatrzymanie na pierwszej blokadzie.
// args: { from?: id, only?: [ids], until?: id, maxFixRounds?: number, model?: string, carryNotes?: [string] }
// until: zatrzymaj się po tym zadaniu (np. "B9" — tylko część B, bez wydania).
// carryNotes: wskazówki z wcześniejszego uruchomienia przy wznowieniu od środka części.
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
log(`Do wykonania: ${queue.map(t => t.id).join(', ')} (rundy poprawek: ${maxFixRounds}). Żaden agent nie łączy się z TS, Jirą ani Redmine i nie uruchamia prawdziwego claude, codex ani orca.`)

const results = []
let pendingNotes = (opts.carryNotes || []).slice()
for (const task of queue) {
  phase(task.phase)
  const result = task.release
    ? await runRelease(task, maxFixRounds, pendingNotes.join('\n'))
    : await runTask(task, maxFixRounds)
  results.push(result)
  if (result.status === 'blocked') {
    log(`⛔ ${task.id} zablokowane: ${result.blocker}`)
    log(`Wznowienie po naprawie: Workflow({ scriptPath: '${ROOT.replace(/\\/g, '\\\\')}\\\\.claude\\\\workflows\\\\backlog.js', args: { from: '${task.id}' } })`)
    break
  }
  if (task.release) pendingNotes = []
  else {
    const note = [
      result.readme ? `README: ${result.readme}` : '',
      result.deviations ? `odstępstwa: ${result.deviations}` : '',
      result.manualChecks ? `do ręcznego sprawdzenia: ${result.manualChecks}` : '',
    ].filter(Boolean).join(' | ')
    if (note) pendingNotes.push(`- ${task.id}: ${note}`)
  }
  log(`✔ ${task.id}: ${result.summary}`)
}

return {
  completed: results.filter(r => r.status === 'done').map(r => r.task),
  blocked: results.find(r => r.status === 'blocked') || null,
  remaining: queue.slice(results.length).map(t => t.id),
  pendingNotes,
  manualChecks: results.filter(r => r.manualChecks).map(r => `${r.task}: ${r.manualChecks}`),
  deviations: results.filter(r => r.deviations).map(r => `${r.task}: ${r.deviations}`),
  results,
}
