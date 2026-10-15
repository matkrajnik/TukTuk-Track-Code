# Todo Track — stan po 1.0

## 1.3 w toku: zrzuty wzorcowe (T2)

Otwarty punkt: wzorce w `tests/visual.spec.ts-snapshots` powstały lokalnie na Windows 11, a plan (Etap 1, pkt 1) każe generować je na windows-latest. Gałąź nie była pushowana, więc warunek „zrzuty zielone w CI dwa razy z rzędu” nie jest sprawdzony. Po pierwszym pushu: jeśli krok „Zrzuty wzorcowe” w workflow Check padnie na różnicach renderowania, uruchomić Check ręcznie (workflow_dispatch) z `update_snapshots` = true, pobrać artefakt `visual-snapshots`, podmienić nim katalog wzorców, zacommitować i potwierdzić dwa kolejne zielone przebiegi.

## 1.3 w toku: akcje w powiadomieniach (N1) — etap niezamknięty

Plan (Etap 2, Testy) wymaga sprawdzenia natywnie po instalacji każdego przycisku z tabeli; tego nie zrobiono, są tylko testy Rust i Playwright. Do czasu tej walidacji N1 nie jest „gotowe” w rozumieniu planu. Niepotwierdzone: czy `on_activated` z `tauri-winrt-notification` 0.8.1 dociera do kanału przy zainstalowanym AUMID, także po przeniesieniu toastu do Centrum akcji i po zakończeniu wątku, który wywołał `show()`.

Do sprawdzenia na zainstalowanym pakiecie NSIS (osobny identyfikator walidacyjny, nie instalacja użytkownika), ze zrzutami w `test-results/`:
- „Zatrzymaj” i „Podziel…” na toaście długiego pomiaru, także z Centrum akcji i po podziale pomiaru (Zatrzymaj ma zatrzymać dalszy ciąg; Podziel… otwiera bieżący wiersz);
- „Zatrzymaj teraz” na toaście wieczornym;
- „Rozpocznij” i „Zaplanuj na dziś” na toaście nowych zgłoszeń Jiry;
- kliknięcie treści każdego powiadomienia (termin → Zadania z filtrem „Dzisiaj” + terminy, długi pomiar, wieczór, cel, Jira, skrót Start/Stop);
- kliknięcie starego toastu po restarcie aplikacji (tylko główne okno).

## Stan po 1.0 (14 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wszystkie funkcje z PLAN_1.0.md (0.1–0.4 i A–L) mają commity implementacji na tej gałęzi; większość ma też osobny commit „uwagi z przeglądu”. Commit R1 podnosi wersję w `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock` i `src-tauri/tauri.conf.json` do 1.0.0 i aktualizuje README (Organizacja pracy 1.0, Ustawienia, Skróty, Mini-widget, Okna/tray/autostart, Powiadomienia, Sprawdzenie).

Weryfikacja po R1: `npm run build`, 55 testów Playwright i 59 testów Rust (`cargo test --locked`) przechodzi, `cargo fmt` nie wprowadza zmian.

R2 (build produkcyjny i walidacja natywna) z uwagami z przeglądu: `npm run build`, 55 testów Playwright, 63 testy Rust, `cargo fmt --check` i `npm run tauri build -- -- --locked` przechodzą. Instalator: `src-tauri/target/release/bundle/nsis/Todo Track_1.0.0_x64-setup.exe` (nie instalowany). Walidacja natywna na osobnej kompilacji debug `pl.todotrack.validation.v6` (bez NSIS); raport ze sprawdzonymi i niesprawdzonymi punktami, skrypty i zrzuty: `src-tauri/target/verification-v6/RAPORT.md`.

Walidacja znalazła i R2 poprawił:
- Awaria przy każdym starcie: Tauri tworzył okna z konfiguracji przed `setup`, a okno wywoływało `get_settings` przed `manage(Database)` (panika „state() called before manage()”). Okna mają teraz `"create": false` i powstają w `create_windows` (`main.rs`) po zarejestrowaniu stanu; pętla powiadomień startuje po oknach.
- Okna `main` i `widget` rosły o wysokość paska tytułu (37 px przy 125%) przy każdym restarcie: pierwszy `set_size` pluginu window-state w świeżym oknie bez ramki. `window_state::reapply_saved_sizes` ustawia zapisany rozmiar drugi raz zaraz po utworzeniu okien.
- (uwagi z przeglądu) Po przeniesieniu tworzenia okien do `create_windows` wątek traya startował przed oknem `main`, od razu się kończył i tray nie odświeżał się przez całą sesję (tooltip „Todo Track”, bez ikony z kropką i nazwy zadania w menu). Wątki traya, widgetu, przypomnienia, bezczynności i powiadomień startują teraz po `create_windows` (`tray::start`, `widget::start`, `reminder::start`, `idle::start`, `notify::start`); rejestracja stanu (`setup`) zostaje przed oknami. Test `tests::background_threads_start_after_windows`; natywnie tooltip po Start i Stop (`phase3.json`), stara kompilacja pokazywała tylko „Todo Track”.

Sprawdzone natywnie: tooltip traya po Start i Stop, tablica tygodnia (render, menu „Przenieś na…” i Alt+↑ z zapisem `plannedDay`/`order` do SQLite bez zmiany innych pól, `week-board.png`), skróty globalne (zmiana kombinacji, Start/Stop i szybkie przełączanie przez SendInput, błąd zajętego skrótu), podział aktywnego pomiaru w WebView2 z zapisem do SQLite, widget (klik myszą bez kradzieży fokusu, przeciąganie, tylko w tle, X i przywrócenie), pozycja, rozmiar i maksymalizacja okien po restarcie i po starcie z `--hidden`, wpis `Run` z `--hidden` i start tą komendą (bez prawdziwego logowania), reguły powiadomień w `notifications_sent` (długi pomiar, termin przy starcie, bez powtórek; to nie jest walidacja samych powiadomień Windows), `--prepare-update`, otwarcie kopii bazy 0.3.0 z aktywnym timerem (`before-1.0.0-*.sqlite`, timer trwa).

Odstępstwa od PLAN_1.0.md, etap 5 pkt 4:
- Brak instalacji NSIS (osobny identyfikator `pl.todotrack.validation.v6`, ale zwykła kompilacja). Szablon NSIS rozpoznaje działający `todo-track.exe` po nazwie procesu, a podczas walidacji działała instalacja użytkownika.
- Aktualizacja z 0.3.0 przy aktywnym timerze przez instalator nie została wykonana. Plan jej wymaga (pkt 4, a tabela etapów obiecuje „zweryfikowaną aktualizację z 0.3.0”). Sprawdzenie na poziomie danych (kopia bazy v5 z aktywnym pomiarem otwarta przez 1.0.0) i osobne `--prepare-update` są zastępstwem, nie wykonaniem tego punktu. Punkt pozostaje otwarty i blokuje wydanie 1.0.0.
- Powiadomienia Windows nie są zwalidowane: toastów nie widać na zrzutach (`toast-long.png`, `toast-due.png`), bo niezainstalowany exe nie ma zarejestrowanego AUMID. Sprawdzono tylko reguły i deduplikację w `notifications_sent`.
- Autostart sprawdzono wpisem `Run` i ręcznym uruchomieniem tej komendy, bez prawdziwego startu po zalogowaniu do Windows i bez wyłączenia wpisu w Menedżerze zadań.

Do zrobienia (etap 5):
- Blokuje wydanie 1.0.0: instalacja NSIS 1.0.0 jako aktualizacja działającego 0.3.0 z aktywnym timerem (kopia `before-1.0.0-*.sqlite`, zamknięty pomiar, start po aktualizacji).
- Wygląd toastów Windows po instalacji NSIS (termin przy starcie i o 09:00, długi pomiar, wieczór, cel, potwierdzenie skrótu przy schowanym widgecie). Szablon NSIS sprawdza działający `todo-track.exe` po nazwie procesu, więc instalację testową robić, gdy właściwa aplikacja jest zamknięta, albo od razu na właściwej instalacji.
- Uruchomienie z Windows po zalogowaniu i odczyt stanu po wyłączeniu wpisu w Menedżerze zadań.
- C: wygląd ikony traya z kropką i menu z nazwą zadania (tooltip sprawdzony przez UI Automation).
- K: odłączenie monitora. F i G: przeciąganie myszą w WebView2.
- Domyślny `Ctrl+Alt+Space` jest na tym komputerze zajęty przez inny program; ustalić, który, i ewentualnie zmienić domyślny skrót.
- Tag `v1.0.0`. Workflow `.github/workflows/check.yml` nie był uruchamiany na windows-latest, bo gałąź nie była pushowana.
- Proces instalacji użytkownika (`C:ProgramyTodo Track`, PID 39824) działał podczas walidacji, a około 21:16 już go nie było; brak wpisu o awarii, przyczyna nieustalona. Uruchomić aplikację ponownie.

Decyzje nadal otwarte:
- 0.1: `.nvmrc` i CI używają Node 24 zamiast Node 22 z planu, bo lokalny nvm nie ma Node 22. Zostać przy 24 albo wykonać `nvm install 22`.
- J: po podziale pomiaru powiadomienie o długiej sesji liczy czas nowej części od momentu podziału, więc przychodzi później. Zachowanie świadome, pokryte testem.
- Playwright używa stałego portu 1430 (`PLAYWRIGHT_PORT`).
- Poza zakresem 1.0: worklogi Jiry, autoupdater, eksport CSV, zgłoszenia Jiry w szybkim przełączaniu.

Baza `%APPDATA%\pl.todotrack.desktop` i instalacja `C:\Programy\Todo Track` nie były zmieniane.

## Stan 0.3.0 (historyczny, 13 października 2026)

Wpis historyczny. Aktualny stan opisuje sekcja „Stan po 1.0” powyżej.

Zaimplementowano zaakceptowane dziewięć rozszerzeń oraz późniejszą prośbę o priorytet bez rozwijania:

- Inline select priorytetu na każdym kafelku, atomowy `set_priority` zmienia tylko to pole JSON; natychmiastowe sortowanie, zgodność z niezapisanym formularzem planu.
- QuickSwitch: osobne okno Tauri `quick`, globalny Ctrl+Shift+Space (plugin global-shortcut), lokalny Ctrl+K i przycisk ⇄. Wyszukiwanie, strzałki, Enter uruchamia jeden timer, Esc chowa okno. Błąd zajętego skrótu jest widoczny w ustawieniach.
- DayClose w Plan i historia: notatki wielu zadań, luki, TS, przeniesienie niezakończonego planu na jutro, znacznik sprawdzenia dnia bez blokady edycji. Niezapisane notatki nie są kasowane przez zdarzenia odświeżania danych.
- Szacunki w minutach, wykonanie procentowe na kafelku, suma szacunków planu; checklisty do 100 kroków.
- Cykle według dni tygodnia, osobne kopie na datę, wyzerowane checklisty, bez kopiowania notatek i terminów; generowanie przy get_snapshot i co 60 s, maks. 366 dni zaległości. Archiwizacja wstrzymuje cykl, przywrócenie uzupełnia zaległości. Tabele recurrence_state i recurrence_occurrences zabezpieczają przed duplikatami.
- Windows GetLastInputInfo: po powrocie z minimum 10 minut bezczynności użytkownik wybiera zachowanie czasu lub stop na początku przerwy. Żadnego automatycznego obcinania. Można wyłączyć w ustawieniach. Dotychczasowe przypomnienie po 5 minutach bez timera pozostaje osobnym mechanizmem.
- LibraryView: wyszukiwanie historii po tytule/projekcie/notatce/dacie i zakresie dat, archiwum, kosz, kopie i ustawienia.
- task_lifecycle: archiwum zachowuje raporty, delete przenosi zadanie i czas do kosza, restore odzyskuje historię. Snapshot rozszerzony o trash/trashSessions/closedDays, live sessions pomijają kosz. Jira start przywraca powiązane zadanie z kosza/archiwum. Przedziały w koszu nadal zajmują czas przy walidacji konfliktów.
- backup.rs: native dialog plugin, rusqlite backup API; eksport spójny również z WAL i aktywnym timerem. Import waliduje bazę, tworzy stabilny plik kandydata, wyświetla podsumowanie i wymaga checkboxa zastąpienia. Przed restore VACUUM INTO before-restore, zachowana konfiguracja Jiry bieżącej instalacji. Token pozostaje w Windows Credential Manager. Timer kopii kończony na znaczniku eksportu lub modyfikacji starszego pliku.

Walidacja: 18 Playwright + 15 Rust przeszło, Vue/TS build przeszedł. Natywna debug instancja `pl.todotrack.validation.v5`: zmiana priorytetu i zachowanie innych pól, timer i quick window, globalny skrót wywołany przez Windows, archiwum/kosz/restore, ustawienia, zamknięcie dnia, minimalizacja na pasek/X do traya. Eksport oraz wybór pliku przez systemowe dialogi; restore po dopisaniu dodatkowego zadania przywrócił snapshot identyczny z eksportem, kopia bezpieczeństwa istnieje. Testową aplikację zakończono przez --prepare-update. Skrypty/zrzuty/pliki testowe w ignorowanym src-tauri/target/verification-v5. Próg 10 minut bezczynności sprawdzono z kontrolowanym czasem w Rust, nie przez realne 10 minut czekania. Prawdziwe konto Jiry nadal nieprzetestowane bez tokenu.

Wydanie 0.3.0 miało instalator `src-tauri/target/release/bundle/nsis/Todo Track_0.3.0_x64-setup.exe` (obecnie wersje pakietów to 1.0.0). Własny NSIS i mechanizm aktualizacji pozostają z poprzedniej wersji; 0.3.0 dodało migracje tabel przy uruchomieniu i korzysta z dotychczasowej kopii przed zmianą wersji. Baza i działająca instalacja użytkownika nie były zmieniane. Poniższe wpisy są historią poprzednich etapów.

## Historia wcześniejszych etapów

Build produkcyjny Tauri/NSIS 0.3.0 z `--locked` zakończył się sukcesem. Instalator istnieje w powyższej ścieżce (3,82 MiB). Nie instalowano go na danych użytkownika; mechanizm aktualizacji NSIS został sprawdzony wcześniej na osobnej instalacji v4.

Weryfikacja zakończona 12 października 2026 w `C:\Projekty\Private\to-do-track`.

## Gotowe pliki

- Instalator NSIS: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.0_x64-setup.exe`.
- Aplikacja bez instalowania: `src-tauri/target/release/todo-track.exe`.

## Wyniki

- `npm ci`, build Vue / TypeScript i oba testy Playwright: sukces.
- `cargo check --locked`, `cargo test --locked` (2 testy), `cargo fmt --check`: sukces.
- `npm run tauri dev`: natywna aplikacja uruchomiona.
- `npm run tauri build -- -- --locked`: build produkcyjny oraz instalator NSIS utworzone.
- W osobnej kompilacji debug z osadzonym frontendem oraz identyfikatorem `pl.todotrack.validation` sprawdzono natywny backend i GUI: dodawanie, przełączanie jednej aktywnej sesji, ukończenie i przywrócenie zadania, edycję, anulowanie usunięcia, usunięcie z historią.
- Sprawdzono minimalizację, żądanie zamknięcia i przycisk chowania, przywracanie lewym kliknięciem ikony traya, Stop z menu oraz aktualizację historii. Proces i aktywna sesja pozostają po schowaniu okna.
- Drugi proces przywraca pierwsze okno i kończy się. Zakończenie z traya zatrzymuje sesję i kończy proces; restart zachowuje dane i czasy.
- Wersję testową zamknięto przez menu traya. Dowody i pomocnicze skrypty testowe są w ignorowanym `test-results/`; osobna kompilacja jest w `src-tauri/target/native-validation/`.
- Poprawiono obserwowanie plików w Vite: zmiany w `src-tauri/` nie przeładowują frontendu podczas kompilacji Rust.
- Szczegółowe wyniki zapisano w README.

## Środowisko i ograniczenia

MSVC Build Tools, Windows SDK, Rust 1.99.0 i WebView2 są dostępne. Systemowy Node nadal ma wersję 18.17.0; testy uruchomiono tymczasowo na Node 22 przez `npx --yes --package=node@22 --call "npm test"`. (Wpis historyczny.) Obecnie `.nvmrc` istnieje i wskazuje Node 24 LTS; CI czyta wersję z tego pliku.

Nie wykonywano instalacji/odinstalowania NSIS ani uruchomienia GitHub Actions. Testy interakcji natywnych dotyczą kompilacji debug z osobnym identyfikatorem, nie zainstalowanego pakietu release.

## Dane użytkownika

Baza właściwej aplikacji: `%APPDATA%\pl.todotrack.desktop\todo-track.sqlite`. Podczas sesji użytkownik dodał zadanie „AI - Rozwój” i uruchomił pomiar; dalsze testy przeniesiono do osobnej bazy `%APPDATA%\pl.todotrack.validation\todo-track.sqlite`. Nie usuwaj ani nie zastępuj danych użytkownika.

Timer obejmuje uśpienie komputera. Po awarii aktywna sesja pozostaje otwarta do Stop. Brak autostartu i wykrywania bezczynności jest zgodny z zakresem.

Pliki projektu pozostają niecommitowane; zachowaj wszystkie zmiany użytkownika. Brak dalszych zadań implementacyjnych z pierwotnej notatki.

## Aktualizacja: ciemny motyw, uproszczony widok, przedziały i raporty

Wykonano kolejne zlecenie użytkownika:
- Motyw ciemny domyślnie; przełącznik jasny/ciemny zapamiętuje wybór lokalnie.
- Usunięto ozdobne treści, boczny panel i duże karty statystyk. Dwie zakładki: Zadania / Raporty dzienne.
- Kafelek zadania pokazuje sumę czasu; rozwinięcie pokazuje daty i godziny wszystkich sesji, czas każdej oraz edycję zadania/usuwanie.
- Edycja początku i końca zakończonej sesji przez formularz datetime-local. Backend Rust transakcyjnie odrzuca brak sesji, sesję aktywną, niepoprawny zakres, przyszłość i nakładanie się z innymi zadaniami. Aktywną sesję najpierw należy zatrzymać.
- Raporty z wyborem daty, poprzednim/następnym dniem i przyciskiem Dzisiaj; suma dnia, sumy zadań, rozwijane przedziały i edycja. Sesje są dzielone przez lokalną północ, z poprawnymi granicami dni zmiany czasu.
- Bez zmiany schematu SQLite i bez modyfikowania danych użytkownika.

Aktualne testy: 6 Playwright + 3 Rust, build TypeScript/Vue, cargo check i fmt — sukces. Test natywny edycji, raportu, odrzucenia konfliktu i trwałości po restarcie wykonano przez WebView2 na osobnej bazie `pl.todotrack.validation.v2`. Zrzuty i skrypt w `test-results/` (Playwright czyści ten katalog przy kolejnym uruchomieniu).

Główne zmiany: `src/App.vue`, `src/style.css`, `src/time.ts`, `src/store.ts`, `src-tauri/src/db.rs`, `src-tauri/src/main.rs`, `tests/app.spec.ts`. Zaktualizowano README. Nowy instalator nadpisuje plik w dotychczasowej lokalizacji, wersja pakietu nadal 0.1.0.

## Aktualizacja: Jira, własny pasek i przypomnienie w tle

Wykonano kolejne zlecenie użytkownika. Jira Cloud: `https://asistonwms.atlassian.net`.

- Nowa zakładka Jira: konfiguracja e-mail/token API bez zakresów, wybór kilku tablic, zgłoszenia przypisane do aktualnego użytkownika, filtr otwartych, wyszukiwanie po kluczu/nazwie, link do zgłoszenia i przycisk Rozpocznij. Start importuje lub wznawia lokalne zadanie bez duplikatów i zatrzymuje poprzedni timer. Nie wysyła worklogów ani nie zmienia Jiry.
- Backend HTTPS używa nowego endpointu Cloud `/rest/software/1.0/board/{id}/issue` i stronicowania nextPageToken. Token w Windows Credential Manager (keyring), nie w SQLite/localStorage. Publiczna konfiguracja w tabeli settings; powiązania w jira_tasks. Istniejące zadania/sesje pozostają zachowane.
- Zastąpiono natywny pasek okna własnym, zgodnym z motywem. Przeciąganie, maksymalizacja/przywrócenie, minimalizacja i zamknięcie do traya działają.
- Backend Rust pokazuje osobne małe okno reminder po 300 sekundach ciągłego braku aktywnego pomiaru, gdy główne okno jest schowane lub bez fokusu. Odroczenie 5 min, przycisk otwierający pole nowego zadania. Pomiar lub powrót do głównego okna resetują odliczanie. Popup nie przejmuje fokusu i działa również bez instalacji NSIS.

Główne nowe pliki: `src/JiraView.vue`, `src/ReminderWindow.vue`, `src-tauri/src/jira.rs`, `src-tauri/src/reminder.rs`, `tests/jira.spec.ts`. Zmiany też w App.vue, main.ts, CSS, bazie, konfiguracji okien/capabilities i zależnościach Cargo.

Weryfikacja: build Vue/TS, 9 Playwright, 9 Rust, cargo check/fmt, produkcyjny build i NSIS — sukces. Test Jiry używa lokalnego serwera HTTP i zastąpionego IPC; obejmuje paginację, currentUser, deduplikację oraz uruchamianie pojedynczego timera. Rzeczywiste logowanie i tablice nie były sprawdzone bez tokenu użytkownika. Dane logowania należy podać w aplikacji.

Natywne testy na osobnym identyfikatorze `pl.todotrack.validation.v3`: kontrolki paska, faktyczne przeciąganie okna, formularz Jiry, brak popupu podczas pracy, popup w tle, snooze i powrót z fokusem do pola zadania. Debug miał `TODO_TRACK_REMINDER_SECONDS=4`; release ignoruje zmienną i używa 300 s (próg zweryfikowany testem jednostkowym). Kopia skryptu i zrzutów: `src-tauri/target/verification-v3/`. Testowy proces zamknięto po zatrzymaniu pomiaru.

Gotowy zaktualizowany instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.0_x64-setup.exe` (~3,58 MiB); exe bez instalacji: `src-tauri/target/release/todo-track.exe`. README zaktualizowane. Wersja nadal 0.1.0, stare artefakty nadpisane. Nie instalowano NSIS i nie uruchamiano CI. Zmiany nadal bez commita.

## Aktualizacja: Raport TS (0.1.1)

W raporcie dziennym dodano przycisk „Raport TS”. Rozwijany panel zawiera datę, nazwy zadań i ich sumaryczny czas z wybranego dnia w godzinach dziesiętnych (przecinek, dwa miejsca), a także sumę dnia. Uwzględnia zakończone zadania, wiele przedziałów, lokalną północ i trwający pomiar do bieżącej chwili. Suma jest zaokrąglana z pełnego czasu niezależnie od wierszy. Tekst z tabulatorami można kopiować przyciskiem lub ręcznie z pola tekstowego. Przy odmowie schowka próbuje kopiowania zaznaczenia; ostatecznie zaznacza tekst i podaje Ctrl+C.

Zmiany: App.vue, CSS, test raportu w tests/app.spec.ts, README. Wersje package.json, package-lock.json, Cargo.toml, Cargo.lock i tauri.conf.json podniesione do 0.1.1. Baza i identyfikator aplikacji bez zmian.

Weryfikacja: 10 testów Playwright przeszło (w tym rzeczywisty schowek Chromium, ręczne kopiowanie awaryjne, sumowanie przedziałów i raport pustego dnia); build Vue/TS oraz produkcyjny Tauri/NSIS z --locked zakończone powodzeniem. Obejrzano zrzut test-results/report-ts.png przy szerokości 760 px. Nie powtarzano testów natywnych WebView2 ani instalacji NSIS.

Najnowszy instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.1_x64-setup.exe`; exe: `src-tauri/target/release/todo-track.exe`.

## Aktualizacja: status „W trakcie” (0.1.2)

Każde nieukończone zadanie z co najmniej jedną sesją ma na kafelku status „W trakcie”. Obejmuje także wcześniejsze zadania oraz pauzę/Stop i restart aplikacji. Ukończenie usuwa etykietę, a przywrócenie zadania z historią ją odtwarza. Status wyliczany z zapisanych sesji, bez migracji SQLite. Filtry pozostają dotychczasowe; „Do zrobienia” obejmuje wszystkie nieukończone zadania.

Zmiany: App.vue, CSS, test cyklu statusu, README oraz wersje pakietów podniesione do 0.1.2. 11 testów Playwright i produkcyjny build Tauri/NSIS z --locked zakończone powodzeniem. Obejrzano widok 760 px na zrzucie tasks-compact.png. Nie instalowano NSIS ani nie powtarzano testów natywnych.

Najnowszy instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.2_x64-setup.exe`; exe: `src-tauri/target/release/todo-track.exe`.

## Aktualizacja: grupy zadań i Dashboard (0.1.3)

Lista ma oddzielone nagłówkami, licznikami i linią sekcje „W trakcie”, „Do zaplanowania” i (w odpowiednim filtrze) „Ukończone”. Rozpoczęte są na górze, a aktywnie mierzone zadanie pierwsze w grupie. Wyszukiwanie/filtry działają przed grupowaniem; puste grupy ukryte.

Dodano src/DashboardView.vue: podsumowania dzisiaj, ostatnich 7 dni i miesiąca do bieżącej chwili, wykres 7 lokalnych dni z przyciskami otwierającymi właściwy raport, liczniki statusów i 5 zadań z największym czasem dzisiaj. Dane z tego samego snapshotu i zegara; raporty dzielone przez lokalną północ. Domyślny widok po uruchomieniu nadal Zadania. Nie zmieniono bazy.

Wersja 0.1.3. 12 testów Playwright oraz build Vue/TS i Tauri/NSIS z --locked przeszły. Nowy test obejmuje kolejność grup, aktywny timer na górze, wyszukiwanie, sumy i liczniki dashboardu, przejście do raportu i start na Zadaniach po reload. Obejrzano zrzuty dashboard-dark.png i dashboard-compact.png (760 px). Nie wykonywano instalacji NSIS ani ponownych testów natywnych.

Najnowszy instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.3_x64-setup.exe`; exe: `src-tauri/target/release/todo-track.exe`.

## Aktualizacja: organizacja pracy i instalator (0.2.0)

Wdrożono zaakceptowany plan: priorytety (sortowanie wewnątrz grup, timer pierwszy), planowany dzień i opcjonalny termin, projekty/obszary i filtr, dzienne notatki opcjonalnie dołączane do TS, ręczne przedziały z kontrolą konfliktów/przyszłości, blokada „Czekam na…” z osobną sekcją, oś dnia z lukami i edycją oraz podsumowanie tygodnia (poniedziałek–niedziela), sumy projektów i przekładanie spraw na kolejny tydzień. Zakładka startowa nadal Zadania. Plan i notatki są po rozwinięciu zadania. Osobna zakładka Plan i historia, a w Zadaniach filtr „Dzisiaj: plan i praca”. Statusy w podsumowaniu tygodnia są bieżące, nie historyczne.

Nowe komponenty: TaskPlanning.vue, ManualTime.vue, JournalView.vue. Nowy backend planner.rs; tabela task_details(task_id FK cascade,json) przechowuje priority (-1/0/1), plannedDay, dueDay, project, waiting oraz mapę notes[YYYY-MM-DD]. Brak niszczącej migracji. Snapshot zawiera details z wartościami domyślnymi dla wcześniejszych zadań. Komendy save_details/add_session walidują dane; Start lokalny i z Jiry kasuje waiting, zapis waiting zamyka sesję tego zadania.

Instalator w języku polskim, własna grafika/ikony i teksty. Szablon skopiowano z tagu tauri-cli-v2.12.2 (adres w README); należy porównać przy aktualizowaniu Tauri. Istniejąca instalacja: jedno potwierdzenie, update w miejscu w poprzednim katalogu, bez deinstalacji; pasywny postęp, zachowanie skrótów i automatyczne uruchomienie po zwykłej aktualizacji. Downgrade zablokowany. Od 0.2.0 obsługa --prepare-update przez single-instance/setup zapisuje koniec sesji i kończy aplikację. Ze starszej wersji trzeba ją zakończyć przez tray; instalator pokazuje instrukcję. W trybie aktualizacji nie wymusza ubicia procesu: Restart Manager sprawdza zamknięcie, w razie potrzeby Retry/Cancel. Brak serwera/autoupdatera.

maintenance.rs: przed otwarciem istniejącej bazy przez nową wersję wykonywane jest VACUUM INTO do app_data_dir/backups/before-VERSION-TIMESTAMP.sqlite (uwzględnia WAL); po udanej inicjalizacji zapis settings.app_version. Wersja pochodzi z config, fallback Cargo. Kopie pozostają na dysku. Nowe pliki instalatora: template.nsi, Polish.nsh, header.bmp, sidebar.bmp, make-artwork.ps1. Publisher nadal todotrack, identifier pl.todotrack.desktop, więc zachowane rozpoznawanie poprzedniej instalacji.

Weryfikacja: 14 Playwright + 11 Rust przeszło, build Vue/TS i produkcyjny Tauri/NSIS z --locked. Natywna instalacja testowa pl.todotrack.validation.v4 / Todo Track Validation v4: rzeczywisty zapis pól/notatek, ręczny czas i odmowa konfliktu, trwałość po reload, oś dnia, TS i schowek. Zainstalowano debug 0.2.0, uruchomiono timer w tle, wykonano aktualizację przez nowy instalator do testowej 0.2.1: stare dane identyczne, aktywna sesja zakończona, restart na Zadaniach, kopia bazy utworzona. Instalator obejrzano w UI przez computer-use; dowody i skrypty w src-tauri/target/verification-v4. Testową aplikację zakończono przez --prepare-update i odinstalowano /S; testowa baza/backup pozostają. Nie zmieniano działającej instalacji użytkownika C:/Programy/Todo Track/todo-track.exe (podczas testów PID 20000).

Najnowszy instalator wydania: `src-tauri/target/release/bundle/nsis/Todo Track_0.2.0_x64-setup.exe`; exe: `src-tauri/target/release/todo-track.exe`. Testowy numer 0.2.1 nie jest wydaniem. Wszystkie wersje pakietu ustawiono na 0.2.0. README zaktualizowane. Nadal brak commitów i testu prawdziwej Jiry bez tokenu.

## Aktualizacja: minimalizacja na pasek zadań (wydanie 0.2.1)

Usunięto obsługę WindowEvent::Resized chowającą zminimalizowane okno. Przycisk minimalizacji normalnie minimalizuje na pasek zadań, X i osobny przycisk ukrywania nadal chowają do traya. Tooltip zaktualizowany. Wersje pakietów 0.2.1; wcześniejsze wzmianki o 0.2.1 jako wyłącznie testowej wersji dotyczą poprzedniej sesji.

Build Vue/TS i produkcyjny Tauri/NSIS przeszedł. Na osobnym identyfikatorze validation.v4 sprawdzono natywnie: po kliknięciu minimalizacji is_minimized=true i is_visible=true także po opóźnieniu, przywrócenie usuwa minimalizację, X ustawia is_visible=false, powrót z traya działa. Skrypt: src-tauri/target/verification-v4/native-minimize.cjs. Proces testowy zakończony przez --prepare-update; właściwa instalacja użytkownika bez zmian.

Najnowszy instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.2.1_x64-setup.exe`.
