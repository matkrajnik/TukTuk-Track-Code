export const meta = {
  name: 'release-1-9',
  description: 'Realizuje PLAN_1.9.md zadanie po zadaniu (Redmine obok Jiry): implementacja, weryfikacja testami, przegląd, poprawki, commit; przygotowanie wydania 1.9.0 na końcu',
  whenToUse: 'Pełna delegacja prac nad Todo Track 1.9 według PLAN_1.9.md (R1–R6, R19). args: { from?: "R1", only?: ["R3"], until?: "R6", maxFixRounds?: 3 }',
  phases: [
    { title: 'Backend Redmine', detail: 'redmine.rs: klucz API, klient, zgłoszenia, projekty, statusy, powiązanie z zadaniem' },
    { title: 'Zgłoszenie i widok', detail: 'issueRef na kafelkach, szybkie dodawanie #1234, widok projekt → zgłoszenia, obserwacja przypisań' },
    { title: 'Zapisy i ustawienia', detail: 'przesunięcie przy starcie, Przesuń do…, komentarz, Ustawienia → Redmine' },
    { title: 'Podgląd i wydanie', detail: 'symulowany Redmine, testy e2e, README, przygotowanie 1.9.0' },
  ],
}

// ---------------------------------------------------------------------------
// Katalog i gałąź pracy. Sesja uruchamiająca workflow sprawdza, że to jej
// katalog roboczy (git rev-parse --show-toplevel) i gałąź; gdy się różnią,
// poprawia te dwie stałe przed startem.
// ---------------------------------------------------------------------------
const ROOT = 'C:\\Projekty\\Private\\to-do-track\\.claude\\worktrees\\release-1-9'
const BRANCH = 'release-1-9'

// ---------------------------------------------------------------------------
// Lista zadań. Kolejność jest ścisła: jeden implementator naraz w jednym
// drzewie roboczym, bo zadania dotykają store.ts, main.rs, db.rs, settings
// i tych samych komponentów (TaskCard, SettingsPanel, navigation).
// Równolegle działają tylko agenci czytający (przegląd).
// Testów ręcznych z prawdziwym Redmine nie ma w planie: lista jest na końcu
// PLAN_1.9.md i trafia do NEXT_SESSION.md przy przygotowaniu wydania.
// ---------------------------------------------------------------------------
const TASKS = [
  {
    id: 'R1', phase: 'Backend Redmine', title: 'Backend Redmine (redmine.rs)',
    section: 'R1 — Backend Redmine (`src-tauri/src/redmine.rs`)',
    scope: 'Nowy moduł src-tauri/src/redmine.rs na wzór jira.rs. Config { site, login, user_id, display_name, accept_invalid_cert, projects: Vec<Project{id,name,identifier,parent_id}> } w settings pod kluczem "redmine" (JSON), Default puste (brak domyślnego adresu). Klucz API w keyring (usługa "<identifier>.redmine", użytkownik "<site>|<login>"), nigdy w SQLite ani w logach. normalize_site: http albo https, host wymagany, ścieżka dozwolona (prefiks np. /redmine, bez końcowego /), zapytanie i fragment odrzucane. Klient per konfiguracja (danger_accept_invalid_certs tylko przy opcji), timeout 30 s, redirect none, nagłówki X-Redmine-API-Key i Accept: application/json. W Cargo.toml feature reqwest "rustls-tls" → "rustls-tls-native-roots" (zaufanie certyfikatom z magazynu Windows), Cargo.lock zaktualizowany, --locked przechodzi. Protokół: GET /users/current.json (id, login, firstname lastname → display_name, user_id); GET /issues.json?project_id=<id>&status_id=open&sort=updated_on:desc&limit=100&offset=N (+ assigned_to_id=me dla „Moje”), GET /issues.json?assigned_to_id=me (wszystkie projekty; z status_id=* i updated_on=>=<14 dni> dla zamkniętych niedawno), GET /issues.json?issue_id=1,2,3&status_id=* (po numerach), stronicowanie total_count/offset do 5 stron (500) z ostrzeżeniem w warnings; z każdego zgłoszenia id, subject, project.id/name, tracker.name, status.id/name/is_closed, priority.name, assigned_to.id/name, fixed_version.name, category.name, due_date, updated_on. GET /issue_statuses.json → tabela redmine_statuses(site, id, name, is_closed), odświeżana przy „Odśwież”, raz na dobę i przy pierwszym użyciu. GET /projects.json?limit=100&offset=N (do 10 stron) → lista do wyboru śledzonych projektów (pamięć w RedmineState, bez tabeli). Tabele w db.rs przez CREATE TABLE IF NOT EXISTS: redmine_tasks(task_id PK → tasks ON DELETE CASCADE, site, issue_id INTEGER, UNIQUE(site, issue_id)), redmine_issues(site, issue_id, subject, status, status_id, closed, project, tracker, priority, due_date, assigned, PRIMARY KEY(site, issue_id)), redmine_statuses; kopia SQLite je obejmuje. RedmineState(Mutex<HashMap<u64, Issue>>) i cached_subject(app, id) jak jira::cached_summary. Komendy w main.rs: redmine_settings, redmine_connect(site, api_key, accept_invalid_cert) (walidacja: adres, klucz 1–128 znaków bez białych; próba /users/current.json; zmiana adresu usuwa stare poświadczenie i czyści stan), redmine_test, redmine_disconnect (usuwa klucz, czyści stan, zostawia konfigurację i powiązania), redmine_issues(project_id: Option<u64>, mine, open_only) → { issues, warnings }, redmine_projects, redmine_select_projects(projects), redmine_statuses(refresh), redmine_start(id) → db::Snapshot (jak jira_start/start_issue: istniejące zadanie z redmine_tasks wraca z kosza/archiwum i do zrobienia, nowe z tytułem = subject, poprzedni pomiar zatrzymany, nowy wystartowany; pasmo priorytetu przez jira_fields::priority_band z dopisanymi „immediate” i „natychmiastowy”), start_by_id(app, id) publiczne, redmine_open(url) tylko pod hostem i ścieżką site. status_error po polsku jak w planie (401 klucz, 403 uprawnienia, 404 adres/API wyłączone, 422 z errors przyciętymi do 200 znaków, 429, 5xx, brak sieci, odpowiedź HTML zamiast JSON, błąd certyfikatu z osobnym komunikatem). Jira i Redmine są od siebie niezależne — żadnych zmian w jira*.rs poza priority_band.',
    done: 'Testy Rust bez sieci: parsowanie fixture src-tauri/tests/fixtures/redmine_issues.json (2 projekty, 4 zgłoszenia: z due_date i bez, zamknięte, bez assigned_to, z fixed_version, polskie znaki), redmine_projects.json (projekt z podprojektem, parent.id), redmine_statuses.json, redmine_user.json; redmine_select_projects (zapis, nadpisanie, pusta lista); stronicowanie (total_count 250 → 3 strony; limit 5 stron z ostrzeżeniem); normalize_site (http, prefiks ścieżki, odrzucenie parametrów i braku hosta); valid_issue_id; allowed_link; redmine_start na bazie w pamięci (nowe, z kosza, z archiwum, zatrzymanie poprzedniego pomiaru); zapis do redmine_issues i redmine_statuses; status_error dla 401/403/404/422 z errors/HTML. Nic nie łączy się z żadnym serwerem.',
  },
  {
    id: 'R2', phase: 'Zgłoszenie i widok', title: 'Zgłoszenie Redmine na zadaniu',
    section: 'R2 — Zgłoszenie Redmine na zadaniu',
    scope: 'db::Task: redmine_id, redmine_url (r.site || \'/issues/\' || r.issue_id), redmine_status, redmine_tracker, redmine_due przez LEFT JOIN redmine_tasks/redmine_issues w db::snapshot; te same kolumny tam, gdzie dziś dołącza się jira_tasks do listy zadań (tray.rs, agents.rs — tylko do wyświetlenia, dopasowanie po kluczu zostaje dla Jiry; planner.rs clear_waiting bierze status z Jiry ALBO Redmine przez COALESCE). Nowy src/issue.ts: issueRef(task) → { system: "jira"|"redmine", key ("WMS-12" / "#1234"), url, status, label } albo null; issueLabel. Komponenty pokazujące klucz, status lub link (TaskCard, WeekCard, QuickSwitch, WidgetWindow, MorningReview, TasksView, App, MonthReport, WeekBoard) przechodzą na issueRef; JiraFieldChips i JiraIssueActions zostają tylko dla Jiry; TaskCard dla Redmine pokazuje chipy tracker i termin DD.MM oraz „Otwórz w Redmine” (redmine_open). Klasy .task-jira-key/.task-jira-status → .task-issue-* bez zmiany wyglądu (zrzuty wzorcowe bez zmian). Pola jiraKey/jiraUrl/jiraStatus na Task bez zmian; dochodzą redmineId/redmineUrl/redmineStatus/redmineTracker/redmineDue. Szybkie dodawanie (src/quickadd.ts, quickadd.rs): „#1234” (sam # i cyfry) = zgłoszenie Redmine, gdy Redmine połączone (redmineEnabled jak jiraEnabled); „#nazwa” z literami nadal projekt; „\\#1234” projekt o nazwie 1234; sam „#1234” bez tytułu bierze temat z cached_subject, inaczej błąd „Nie znam tematu #1234. Wpisz tytuł albo odśwież listę w widoku Redmine.”; chip „Redmine: #1234”; quick_add zapisuje redmine_tasks; „WMS-12” i „#1234” w jednej linii → „Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.”; istniejące zadanie z numerem → „#1234 ma już zadanie „…””. „Czekam na”: ustawienie waiting_redmine_statuses (domyślnie ["Feedback"], walidacja jak waiting_jira_statuses) — dodaj je już tu w settings.rs, R5 pokaże pole; waitingProposal w src/review.ts bierze listę według issueRef(task).system. Eksport (src/export.ts, export.rs): kolumna jiraKey bez zmian, nowa kolumna redmine („#1234”) na końcu; testy eksportu zaktualizowane świadomie. W store.ts minimalny symulowany Redmine potrzebny do testów tego zadania (previewRedmineKey = "todo-track-preview-redmine-v1": connected, issues z tematami), R6 go rozbuduje.',
    done: 'Jednostkowe issueRef (Jira, Redmine, brak, oba pola → Jira), quickadd.ts (#1234 z połączonym Redmine, bez połączenia jako projekt, #abc jako projekt, \\#1234, oba klucze → błąd), priority_band nowe nazwy. Rust: snapshot z zadaniem Redmine, clear_waiting ze statusem Redmine, quick_add z #1234 (nowe, duplikat, oba klucze), eksport z kolumną. Playwright: chipy na kafelku, karcie tygodnia, szybkim przełączaniu i widgecie; szybkie dodawanie #1234. Wszystkie testy Jiry zielone bez zmian treści.',
  },
  {
    id: 'R3', phase: 'Zgłoszenie i widok', title: 'Widok Redmine i obserwacja przypisań',
    section: 'R3 — Widok Redmine i obserwacja przypisań',
    scope: 'Nowy src/RedmineView.vue (wzór: tryb listy JiraView.vue, ale układ „projekt → zgłoszenia”): pozycja „Redmine” w nawigacji po „Jira” (src/navigation.ts, AppNav.vue, skrót jak inne widoki), widoczna zawsze; bez konta tekst „Połącz konto Redmine w Ustawieniach” z linkiem openSettings("redmine") — formularz połączenia jest w Ustawieniach (R5), nie w widoku; nagłówek „Połączono jako …” i link do Ustawień. U góry wybór projektu spośród śledzonych (zakładki albo select; zapamiętany ostatni), zakres „Moje” (domyślnie) / „Wszystkie otwarte” (limit 500 z ostrzeżeniem), „Odśwież”; bez śledzonych projektów widok pokazuje „Moje zgłoszenia ze wszystkich projektów” i link „Wybierz projekty w Ustawieniach”. Zgłoszenia projektu pogrupowane według statusu (kolejność z redmine_statuses, status spoza listy na końcu; grupy zwijane z liczbą; zamknięte ukryte przy „Tylko otwarte” — domyślnie), w grupie sort po priorytecie i terminie; wyszukiwanie po numerze i temacie; filtr trackera (chipy z trackerów obecnych w wyniku). Karta: #1234, temat, tracker, podprojekt (gdy inny niż wybrany), status, priorytet (pasmo kolorem jak w Jirze), osoba przypisana (inicjały, gdy nie ja), wersja docelowa, termin, czas zmierzony lokalnie, „mierzone teraz” / „zaplanowane na …”, przyciski „Rozpocznij” (redmine_start), „Otwórz w Redmine” i miejsce na menu akcji (R4). Na dole sekcja „Przypisane do mnie spoza śledzonych projektów” (zapytanie bez project_id). Ostrzeżenia z warnings. Obserwacja przypisań (redmine::start, wątek jak jira::start, WATCH_TICK 20 s, interwał 5 min, kończy się bez okna): assigned_to_id=me otwarte + zamknięte zmienione w 14 dni + zgłoszenia z redmine_tasks otwartych zadań nieprzypisanych do mnie (issue_id=…, paczkami po 100); reguły jak jira::sync_assigned (pierwsze sprawdzenie tylko zapamiętuje assigned=1, nowe otwarte przypisanie → zadanie w „Do zaplanowania” bez dnia i timera + powiadomienie Windows z tymi samymi przyciskami co dla Jiry, zgłoszenie powiązane z zadaniem także w koszu/archiwum nie dubluje się, zdjęte i przypisane ponownie wraca); statusy, priorytety, tracker i termin w redmine_issues odświeżane przy każdym sprawdzeniu, pasmo priorytetu aktualizowane; wyłącznik redmine_auto_import (domyślnie true; pole w settings.rs dodaj tu, R5 pokaże przełącznik). Rozszerzenie przeglądarki: browser::known_sites dołącza adres Redmine i hosty z redmine_tasks; browser::issue_key rozpoznaje ścieżkę /issues/1234 (także pod prefiksem, np. /redmine/issues/1234) i zwraca „#1234”; suggest.rs i foreground.rs traktują klucz zaczynający się od # jako Redmine (redmine::start_by_id, tytuł z cached_subject), pozostałe jak dotąd; domyślny wzorzec domen rozszerzenia (*.atlassian.net) bez zmian. Odpowiedniki komend w symulowanym Redmine w store.ts (projekty, śledzone projekty, zgłoszenia projektu, start) i zastępcze IPC obserwacji w tests/fixtures/tauri.ts jak dla Jiry.',
    done: 'Rust: sync_assigned dla Redmine (pierwsze sprawdzenie, nowe przypisanie, duplikat w koszu, powrót po ponownym przypisaniu, odświeżenie statusu i pasma), treść powiadomienia dla jednego i wielu zgłoszeń, budowa zapytania issues.json (projekt, „Moje”, otwarte, stronicowanie), grupowanie według statusu w kolejności z redmine_statuses, issue_key dla /issues/1234, /redmine/issues/1234?x=1, odrzucenie /issues/abc i hosta spoza listy, rozdział #1234 / WMS-12 w suggest.rs. Playwright: widok (wybór projektu, Moje / Wszystkie otwarte, grupy statusów, filtr trackera, wyszukiwanie, „Rozpocznij” tworzy zadanie i startuje pomiar, link, sekcja spoza śledzonych projektów, stan bez projektów i bez konta), zastępcze IPC obserwacji. Nowy zrzut wzorcowy widoku wymieniony w deviations; zrzuty nawigacji zaktualizowane świadomie (nowa pozycja).',
  },
  {
    id: 'R4', phase: 'Zapisy i ustawienia', title: 'Zapisy do Redmine',
    section: 'R4 — Zapisy do Redmine',
    scope: 'Wspólna ścieżka zapisu redmine::write(method, path, body) z ensure_write (ustawienie redmine_write, domyślnie true — dodaj w settings.rs tu, R5 pokaże przełącznik), walidacją i status_error; wszystko tylko po kliknięciu. Przesunięcie przy starcie pracy (odpowiednik J1): jira_move.rs dostaje w Prompt pole system ("jira"|"redmine") i Linked dla obu tabel (linked czyta jira_tasks, a gdy brak — redmine_tasks); wątek, baner (JiraMoveBanner.vue → „Przesunąć #1234 do „In Progress”?”), widget i powiadomienie bez zmian poza etykietą; nazwy komend jira_move_prompts / jira_move_dismiss / jira_no_ask bez zmian, obsługują oba systemy. Dla Redmine: status bieżący odświeżony GET /issues/{id}.json?include=allowed_statuses (brak pola → wszystkie statusy z redmine_statuses); pytanie, gdy nazwa statusu jest w redmine_new_statuses (domyślnie ["New","Nowy"]) i zgłoszenie niezamknięte; cel = status o nazwie redmine_move_status (domyślnie "In Progress") wśród dozwolonych, bez dopasowania baner pokazuje listę dozwolonych niezamkniętych; wykonanie PUT /issues/{id}.json {"issue":{"status_id":N}}, potem odczyt statusu i zapis do redmine_issues i kafelka; po PUT bez zmiany statusu → „Redmine nie zmienił statusu”. Ustawienia redmine_ask_move (true), redmine_move_status, redmine_new_statuses, redmine_branch_prefix ("feature/", valid_branch_prefix) — pola w settings.rs tu, interfejs w R5; „Nie pytaj dla tego zadania” = to samo pole noAsk w task_details. Rozwinięty kafelek zadania z #1234: „Przesuń w Redmine…” (redmine_transitions(task_id) → [{id,name,is_closed}]), propozycja po ukończeniu „Ukończone. Przesunąć #1234 dalej?” (src/jiramove.ts offerAfterDone rozpoznaje system przez issueRef), „Przypisz do mnie” (PUT assigned_to_id = user_id), „Dodaj komentarz” (PUT notes, 1–4000 znaków), „Kopiuj nazwę gałęzi” (feature/1234-krotki-tytul; ta sama funkcja formatująca co jira_branch_name, prefiks z redmine_branch_prefix), „Otwórz w Redmine”. W widoku Redmine te same akcje w menu karty plus „Przesuń do…” (lista dozwolonych statusów; karta przechodzi do innej grupy, przy błędzie wraca z komunikatem). Bez „Zmień osobę…”. Komendy: redmine_transitions(task_id), redmine_transition(task_id, status_id), redmine_assign_me(issue_id), redmine_comment(issue_id, text), redmine_branch_name(issue_id); odpowiedniki w symulowanym Redmine w store.ts (dozwolone statusy, zmiana, przypisanie, komentarz; readMovePrompts symulowane dla obu systemów).',
    done: 'Rust: wybór statusu docelowego (jedno dopasowanie po nazwie, brak → lista, odfiltrowanie zamkniętych), should_ask dla Redmine (status w redmine_new_statuses, spoza listy, zamknięte, „Nie pytaj”), parsowanie allowed_statuses i brak pola (fallback), budowa ciał PUT (status, przypisanie, notatka), 422 z errors, blokada przy wyłączonym redmine_write, nazwa gałęzi z numerem. Playwright: start → baner → „Przesuń” zmienia status na kafelku; „Nie pytaj”; propozycja po ukończeniu; „Przesuń do…” w widoku; „Przypisz do mnie”; komentarz; brak akcji przy wyłączonym przełączniku. Testy jira-move i jira-board zielone bez zmian treści.',
  },
  {
    id: 'R5', phase: 'Zapisy i ustawienia', title: 'Ustawienia → „Redmine”',
    section: 'R5 — Ustawienia → „Redmine”',
    scope: 'Nowa sekcja "redmine" w SettingsPanel.vue (lista sekcji w src/navigation.ts między Jira a Powiadomienia; etykiety wyszukiwarki: „Redmine”, „klucz API”, „zgłoszenia”, „przypisane do mnie”, „statusy”, „Czekam na”, „przesunięcie”, „śledzone projekty”). Nowy RedmineSettings.vue (wzór TimesheetSettings.vue). Połączenie: „Adres Redmine” (pełny adres z ewentualnym prefiksem ścieżki; przy http ostrzeżenie „klucz API jest wysyłany bez szyfrowania”), „Klucz API” (type=password, wartość nigdy nie wraca z backendu; po połączeniu puste z podpisem o magazynie poświadczeń), link „Gdzie znaleźć klucz API” (<adres>/my/account, aktywny po wpisaniu adresu), zwinięte „Zaawansowane” z opcją „Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)” i opisem o magazynie Windows; przyciski „Połącz” (redmine_connect), „Testuj połączenie” (redmine_test → „Połączono jako Imię Nazwisko (login), Redmine odpowiada” albo błąd, osobny komunikat dla certyfikatu), „Rozłącz” (ConfirmDialog); stan „Połączono jako …” / „Nie połączono”. Śledzone projekty: lista wybranych z „Usuń”, „Dodaj projekt…” z listą z redmine_projects (podprojekty z wcięciem, wyszukiwanie po nazwie, zaznaczanie wielu), zapis redmine_select_projects, podpis „Widok Redmine pokazuje zgłoszenia tych projektów; nowe przypisania są sprawdzane we wszystkich projektach.” Przełączniki i statusy: „Pozwól aplikacji zmieniać zgłoszenia w Redmine” (redmineWrite), „Pytaj o przesunięcie zgłoszenia przy starcie pracy” (redmineAskMove, wyłączone przy wyłączonym redmineWrite), „Statusy początkowe (jeden w wierszu)” (redmineNewStatuses), „Docelowy status przy starcie pracy” (redmineMoveStatus), „Statusy oznaczające „Czekam na” (jeden w wierszu)” (waitingRedmineStatuses), „Dodawaj nowe zgłoszenia przypisane do mnie” (redmineAutoImport), „Prefiks nazwy gałęzi” (redmineBranchPrefix); przy połączonym koncie przycisk „Wstaw statusy z Redmine” (lista z redmine_statuses(true) do kliknięcia). Pod sekcją „Jak to działa” (co czyta, co zapisuje, tylko po kliknięciu, Redmine i Jira niezależne). Wszystkie wejścia navigate z section "redmine" otwierają tę sekcję. Pola w settings.rs, których R2–R4 nie dodały, dodaj tu z walidacją (redmine_move_status 1–80 znaków; listy statusów jak waiting_jira_statuses).',
    done: 'Rust: wartości domyślne, walidacja i zapis każdego pola; normalize_site dla adresu z prefiksem ścieżki. Playwright: połącz (symulowane), błąd przy złym kluczu, test połączenia, rozłącz, ostrzeżenie przy http, opcja certyfikatu zapamiętana, dodanie i usunięcie śledzonego projektu, przełączniki, listy statusów i „Wstaw statusy z Redmine”, wyszukiwarka ustawień znajduje „Redmine” i „klucz API”. Nowy zrzut wzorcowy sekcji i zrzuty ustawień (nowa pozycja na liście sekcji) wymienione w deviations.',
  },
  {
    id: 'R6', phase: 'Podgląd i wydanie', title: 'Symulowany Redmine, testy e2e, seed',
    section: 'R6 — Podgląd przeglądarkowy, testy e2e, dokumentacja',
    scope: 'Dokończenie symulowanego Redmine w store.ts pod previewRedmineKey = "todo-track-preview-redmine-v1" (wzór previewJira*): funkcje redmineAccount, redmineConnect, redmineTest, redmineDisconnect, redmineIssues(projectId, mine, openOnly), redmineProjects, redmineSelectProjects, redmineStatuses, redmineStart, redmineTransitions, redmineTransition, redmineAssignMe, redmineComment, redmineBranchName — na pulpicie invoke, w podglądzie localStorage; stan { connected, site, login, displayName, acceptInvalidCert, fail, write, statuses, projects, selectedProjects, issues }. Dane: https://redmine.example.pl, statusy New / In Progress / Feedback / Resolved / Closed (is_closed dla dwóch ostatnich), 3 projekty (jeden z podprojektem), jeden śledzony, 7 zgłoszeń (w śledzonym 5 w trzech statusach, w tym jedno przypisane do kogoś innego, jedno zamknięte, jedno z terminem; jedno w podprojekcie; jedno przypisane do mnie w projekcie nieśledzonym). Klucz API „zly” → błąd 401. Nasłuch storage jak przy Jirze. Baner przesunięcia w podglądzie obsługuje oba systemy. tests/fixtures/seed.ts: zadanie z redmineId 1234 w „New” (pytanie przy starcie) i zadanie z redmineId 1240 w „Feedback” („Czekam na”); redmineFixture() dla zastępczego IPC. Jeden plik tests/redmine.spec.ts zbierający scenariusze z R2–R5 (jeśli powstały w osobnych plikach, scal je tu bez utraty asercji) i uzupełniający brakujące: pełna ścieżka Ustawienia → projekty → widok → „Rozpocznij” → baner przesunięcia → kafelek; tests/redmine-store.spec.ts (symulacja: połączenie, błąd, zmiana statusu, przypisanie, issueRef). README NIE w tym zadaniu (robi R19), ale w summary opisz dokładnie, co trzeba w nim napisać: „Nowości 1.9”, pozycja „Redmine” w opisie ustawień, sekcja „Redmine” obok „Jira” (klucz API i gdzie go znaleźć, protokół users/current, issues, issue_statuses, PUT issues/{id}; co aplikacja zapisuje i że tylko po kliknięciu; jedno zgłoszenie na zadanie; różnice wobec Jiry: projekt zamiast tablicy, statusy bez kategorii, certyfikaty z magazynu Windows).',
    done: 'tests/redmine.spec.ts i tests/redmine-store.spec.ts zielone; wszystkie dotychczasowe testy zielone (w tym wszystkie tests/jira*.spec.ts bez zmian treści); zrzuty wzorcowe zgodne albo zaktualizowane świadomie i wymienione w deviations.',
  },
  {
    id: 'R19', phase: 'Podgląd i wydanie', title: 'Przygotowanie wydania 1.9.0', release: '1.9.0',
    section: 'Przygotowanie wydania 1.9.0 (R19)',
    scope: 'Wydanie obejmuje wszystkie zadania z PLAN_1.9.md (R1–R6). README „Nowości 1.9” opisuje integrację z Redmine (self-hosted: adres, klucz API, certyfikaty z magazynu Windows; śledzone projekty i widok projekt → zgłoszenia pogrupowane według statusu; import nowych przypisań; pytanie o przesunięcie przy starcie; „Przesuń do…”, „Przypisz do mnie”, komentarz, nazwa gałęzi; szybkie dodawanie #1234; jedno zgłoszenie na zadanie; nic nie zapisuje się bez kliknięcia; Jira i Redmine niezależne), nowy punkt „Redmine” w opisie ustawień i nowa sekcja „Redmine” obok sekcji o Jirze (protokół, różnice wobec Jiry, lista rzeczy niesprawdzonych z prawdziwym Redmine). Wzmianka o zmianie feature TLS na rustls-tls-native-roots w „Wersja w repozytorium”.',
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
    deviations: { type: 'string', description: 'Odstępstwa od PLAN_1.9.md i dlaczego' },
    manualChecks: { type: 'string', description: 'Co wymaga ręcznego sprawdzenia z prawdziwym Redmine, w natywnej aplikacji lub po instalacji NSIS' },
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
    noNetwork: { type: 'boolean', description: 'Żaden test ani skrypt nie łączy się z żadnym serwerem Redmine ani Jira' },
    jiraUntouched: { type: 'boolean', description: 'Pliki tests/jira*.spec.ts i moduły testów w src-tauri/src/jira*.rs nie zmieniły treści asercji względem gałęzi bazowej' },
    playwrightCount: { type: 'number' },
    rustCount: { type: 'number' },
    failures: { type: 'string', description: 'Pełne komunikaty błędów, nazwy testów, fragmenty logu' },
    uncommitted: { type: 'boolean', description: 'git status pokazuje niezacommitowane zmiany' },
  },
  required: ['ok', 'build', 'playwright', 'cargoTest', 'cargoFmt', 'noNetwork', 'jiraUntouched', 'failures', 'uncommitted'],
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
Projekt: ${ROOT} (worktree repozytorium to-do-track; Tauri 2 + Rust + Vue 3, interfejs po polsku), gałąź ${BRANCH}. Pracuj WYŁĄCZNIE w tym katalogu — nie zmieniaj plików w głównym katalogu C:\\Projekty\\Private\\to-do-track poza tym worktree, nie zmieniaj innych worktree i nie przełączaj gałęzi.
Źródło prawdy: PLAN_1.9.md. Przeczytaj go w całości (zwłaszcza „Założenia przyjęte bez pytania”, „Odłożone” i listę ręcznych testów na końcu), a potem sekcję swojego zadania. Przeczytaj README.md i NEXT_SESSION.md, żeby znać reguły projektu (jeden aktywny pomiar, dzielenie przez lokalną północ, tryb podglądu w store.ts, dane użytkownika nietykalne).
Wzorzec integracji: Jira (src-tauri/src/jira.rs, jira_move.rs, jira_board.rs, jira_fields.rs — konfiguracja w settings pod kluczem JSON, sekret w keyring, status_error po polsku, komendy w main.rs, wątek obserwacji; src/JiraView.vue; src/store.ts — previewJira*; src/components/SettingsPanel.vue — sekcja jira; tests/jira*.spec.ts) oraz Timesheet (src-tauri/src/timesheet.rs, src/components/TimesheetSettings.vue — formularz połączenia w Ustawieniach, „Testuj połączenie”, „Rozłącz”).
Jira i Redmine to dwa osobne, niepowiązane źródła: własne połączenie, poświadczenie, ustawienia, widok, wątek obserwacji i tabele. Wspólna jest tylko mechanika aplikacji (baner pytania o przesunięcie, issueRef, formatowanie nazwy gałęzi). Zachowanie Jiry nie może się zmienić: testy tests/jira*.spec.ts i testy w jira*.rs mają przechodzić bez zmiany treści asercji — zmiana zachowania Jiry to błąd zadania, nie powód do aktualizacji testu.

ZAKAZ BEZWZGLĘDNY: nie wykonuj żadnych żądań HTTP do żadnego serwera Redmine ani Jira — ani ręcznie (curl, Invoke-WebRequest, node), ani w testach, ani w skryptach build. Adres Redmine nie ma wartości domyślnej; w kodzie i testach mogą występować tylko adresy przykładowe (redmine.example.pl, 127.0.0.1 w testach bez sieci). Testy Rust działają na plikach JSON z fixtures, testy Playwright na symulowanym Redmine w localStorage. Klucz API nigdy nie trafia do logów, komunikatów błędów, SQLite ani do wyników testów.

Zasady:
- Każda nowa komenda Tauri ma odpowiednik w trybie podglądu w src/store.ts (localStorage), bo testy Playwright działają bez Rusta.
- Nowe tabele SQLite tylko przez CREATE TABLE IF NOT EXISTS przy starcie; nowe pola zadań w JSON task_details z wartościami domyślnymi; nowe ustawienia z wartościami domyślnymi; bez niszczących migracji. Tabele Jiry bez zmian.
- Jedno zadanie ma co najwyżej jedno zgłoszenie: z Jiry albo z Redmine. Żaden zapis do Redmine nie jest automatyczny — zawsze kliknięcie; przełącznik redmine_write blokuje zapisy także w backendzie. Timer nigdy nie przełącza się sam.
- Nowe teksty interfejsu po polsku. Zachowaj istniejące etykiety aria, bo testy ich używają.
- Nie zmieniaj ani nie usuwaj istniejących testów, chyba że sekcja planu tego wymaga; wtedy opisz to w deviations. Gdy zmiana interfejsu świadomie zmienia wygląd, zaktualizuj zrzuty wzorcowe (npm run test:visual:update) i wymień to w deviations.
- Nie dotykaj bazy użytkownika w %APPDATA%\\pl.todotrack.desktop, Menedżera poświadczeń Windows ani zainstalowanej aplikacji. Nie instaluj NSIS.
- Nie pushuj do zdalnego repozytorium, nie wypychaj tagów, nie publikuj wydania. Commity lokalne na gałęzi ${BRANCH}, bez stopki Co-Authored-By (jak ostatnie commity na master).
- Polecenia: npm run build (vue-tsc + vite), npm test (Playwright), cargo test --manifest-path src-tauri/Cargo.toml --locked, cargo fmt --manifest-path src-tauri/Cargo.toml; npm run check uruchamia wszystko. cargo zwykle NIE jest w PATH: dodaj %USERPROFILE%\\.cargo\\bin (w Git Bash: export PATH="$HOME/.cargo/bin:$PATH"). Jeśli brak node_modules, najpierw npm ci.
- Nowe zależności Cargo lub npm dodawaj z konkretną wersją i zaktualizuj pliki lock; --locked musi przechodzić. Zmiana feature reqwest na rustls-tls-native-roots (R1) jest zaplanowana; innych zmian zależności nie rób bez potrzeby.
`

function implementPrompt(task) {
  return `${COMMON}
ZADANIE ${task.id}: ${task.title}
Nie zmieniaj numeru wersji — wersję 1.9.0 ustawia dopiero \`npm run ship\` po scaleniu do master. README i NEXT_SESSION.md aktualizuje dopiero R19, ale opisz zmiany dokładnie w summary.
Sekcja planu: "${task.section}" w PLAN_1.9.md.
Zakres: ${task.scope}
Kryterium ukończenia: ${task.done}

Wykonaj całość zakresu, dopisz testy wymienione w sekcji planu, uruchom npm run build, npm test i cargo test, popraw aż przejdą, uruchom cargo fmt. Następnie zrób jeden commit z opisem "${task.id}: ${task.title}" i dwoma zdaniami treści po polsku.
Jeśli nie da się ukończyć zakresu (np. sekcja planu jest sprzeczna z kodem), nie zostawiaj połowicznych zmian: zrób commit tego, co działa i ma zielone testy, a resztę opisz w blocker ze statusem blocked.
W manualChecks wypisz to, czego nie dało się sprawdzić bez prawdziwego Redmine (nie powtarzaj listy z końca planu — tylko rzeczy nowe albo doprecyzowane).
Zwróć wynik w schemacie: status, summary, commit (hash), filesChanged, testsAdded, deviations, manualChecks, blocker.`
}

function releasePrompt(task, notes) {
  return `${COMMON}
ZADANIE ${task.id}: przygotowanie wydania ${task.release}. ${task.scope}
Od 1.7.4 wydanie robi użytkownik jednym poleceniem \`npm run ship -- ${task.release}\` na master (RELEASE.md, scripts/ship.mjs): ono podbija wersję, buduje i podpisuje instalator (klucz z hasłem, którego nie masz), publikuje, commituje, taguje i pushuje. Ty tylko PRZYGOTOWUJESZ wydanie na gałęzi ${BRANCH}. NIE zmieniaj wersji w manifestach, NIE uruchamiaj npm run ship / npm run release / npm run tauri build, NIE twórz tagu, NIE ruszaj releases/. Kroki:
1. notes.md: nadpisz notatkami wydania ${task.release} (po polsku, krótko, dla użytkowników; wzoruj się na obecnym notes.md z 1.8.0).
2. README: sekcja „Nowości ${task.release.replace(/\.0$/, '')}” nad dotychczasowymi nowościami, opisująca to, co faktycznie zrobiono (git log od commitu bazowego gałęzi: git merge-base master HEAD), bez obietnic; punkt „Redmine” w opisie ustawień; sekcja „Redmine” obok „Jira”; wiersz „Wersja w repozytorium” i blok „Weryfikacja ${task.release}” z liczbami testów. NEXT_SESSION.md: nowy wpis „Stan po ${task.release}” na górze z wynikami weryfikacji, odstępstwami i listą rzeczy do ręcznego sprawdzenia: cała sekcja „Do ręcznego sprawdzenia z prawdziwym Redmine” z PLAN_1.9.md plus uwagi zgłoszone przez zadania:
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
6. Brak sieci: grep -rn "redmine\\." i "atlassian" po src, src-tauri/src, tests, scripts, package.json — mogą występować tylko adresy przykładowe (redmine.example.pl, asistonwms.atlassian.net jako domyślna konfiguracja Jiry) i teksty interfejsu; żaden test, fixture ani skrypt nie może ich wywoływać. Sprawdź też, że testy Rust nowego modułu nie tworzą klienta HTTP do adresu spoza 127.0.0.1 (szukaj reqwest w #[cfg(test)]). noNetwork=true tylko gdy to potwierdzisz.
7. Jira nietknięta: git diff $(git merge-base master HEAD) -- tests/jira*.spec.ts oraz bloki #[cfg(test)] w src-tauri/src/jira*.rs. Dozwolone są wyłącznie zmiany mechaniczne (import, zmiana nazwy klasy CSS .task-jira-* → .task-issue-*, nowe pole w strukturze z wartością domyślną); każda zmiana oczekiwanej wartości, usunięty test albo zmieniony scenariusz to jiraUntouched=false z opisem w failures.
Nie poprawiaj niczego i sam również nie łącz się z żadnym serwerem. Zwróć ok=true tylko gdy wszystkie kroki przeszły, noNetwork=true, jiraUntouched=true i nie ma niezacommitowanych zmian. W failures wklej dokładne komunikaty błędów i nazwy testów, które padły. Podaj liczbę testów Playwright i Rust.`
}

function fixPrompt(task, failures, round) {
  return `${COMMON}
Zadanie ${task.id} (${task.title}) ma czerwoną weryfikację (runda poprawek ${round}). Wynik weryfikatora:
${failures}

Znajdź przyczynę i napraw ją w kodzie lub w nowych testach. Nie wyłączaj i nie kasuj testów. Test zrzutu wzorcowego, który pada przez świadomą zmianę wyglądu z tego zadania, możesz zaktualizować; przez przypadkową zmianę — napraw kod. Test Jiry, który pada, naprawiasz w kodzie, nigdy w teście. Uruchom ponownie polecenia, które padły, oraz cargo fmt. Zrób commit "${task.id}: poprawki po weryfikacji (${round})".
Zwróć wynik w schemacie IMPL (status done, gdy wszystko przechodzi; blocked z opisem, gdy nie).`
}

function reviewPrompt(task, lens, impl) {
  const lenses = {
    plan: `Soczewka: ZGODNOŚĆ Z PLANEM. Porównaj diff commitów zadania ${task.id} (git log --oneline, git show ${impl.commit || 'HEAD'} i wcześniejsze commity tego zadania) z sekcją "${task.section}" w PLAN_1.9.md i z jego założeniami. Wypisz każdy punkt sekcji, który nie został zrealizowany lub został zrealizowany inaczej bez uzasadnienia w deviations: "${impl.deviations || ''}". Zawsze must: brakujący odpowiednik w store.ts dla nowej komendy, zapis do Redmine bez kliknięcia, przełącznik redmine_write nierespektowany w backendcie, klucz API zapisany w SQLite albo w logach, zmiana zachowania Jiry, zadanie z dwoma zgłoszeniami naraz, widok Redmine zbudowany jako kopia tablicy Jiry zamiast układu projekt → zgłoszenia pogrupowane według statusu.`,
    bugs: `Soczewka: POPRAWNOŚĆ. Przeczytaj diff commitów zadania ${task.id} (git log --oneline, git show). Szukaj błędów: sekrety w komunikatach błędów i logach (format! z kluczem API, nagłówkiem), brak walidacji wejścia w komendach redmine_*, stronicowanie (offset, total_count, limit stron), zgłoszenia z podprojektów przypisane do złego projektu, grupowanie według statusu gubiące status spoza listy, powiadomienia o przypisaniach przy pierwszym sprawdzeniu (import backlogu), duplikaty zadań przy ponownym przypisaniu, nadpisywanie innych pól JSON w task_details (wzorzec json_set), issueRef zwracające Jirę i Redmine naraz, niespójność między Rust a store.ts, PUT bez sprawdzenia statusu po zapisie, allowed_statuses bez fallbacku, blokowanie wątku, wycieki nasłuchów Tauri w onUnmounted, dostępność (aria, klawiatura, focus w dialogu), regresje w Jirze przez wspólne komponenty (TaskCard, jira_move.rs, suggest.rs, browser.rs). Zgłaszaj tylko to, co umiesz uzasadnić konkretną ścieżką wykonania.`,
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
    log(`${task.id}: weryfikacja czerwona (runda ${round}): build=${v.build} playwright=${v.playwright} cargo=${v.cargoTest} fmt=${v.cargoFmt} noNetwork=${v.noNetwork} jira=${v.jiraUntouched} uncommitted=${v.uncommitted}`)
    const extra = [
      v.uncommitted ? 'UWAGA: są niezacommitowane zmiany, dokończ commit.' : '',
      v.noNetwork === false ? 'UWAGA: weryfikator znalazł ścieżkę łączącą się z prawdziwym serwerem — usuń ją; testy nie mogą wychodzić do sieci.' : '',
      v.jiraUntouched === false ? 'UWAGA: zmieniły się testy Jiry — przywróć je i napraw kod tak, żeby zachowanie Jiry zostało bez zmian.' : '',
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
// until: zatrzymaj się po tym zadaniu (np. "R3", żeby zobaczyć widok przed zapisami).
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
log(`Do wykonania: ${queue.map(t => t.id).join(', ')} (rundy poprawek: ${maxFixRounds}). Żaden agent nie łączy się z żadnym serwerem Redmine ani Jira.`)

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
    log(`Wznowienie po naprawie: Workflow({ scriptPath: '${ROOT.replace(/\\/g, '\\\\')}\\\\.claude\\\\workflows\\\\release-1-9.js', args: { from: '${task.id}' } })`)
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
