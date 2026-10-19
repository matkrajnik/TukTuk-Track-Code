# Todo Track — stan po 1.7.1

## Stan po 1.7.1 (19 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. 1.7.1 dodaje przypięte zadania („w toku”): „Przypnij do widgetu” w kafelku (najwyżej 3 otwarte, `pinnedAt` w `task_details`, komenda `set_pinned`), drugi rząd przycisków w mini-widgecie (okno 320×96, uprawnienie `core:window:allow-set-size`) i pozycje w menu traya (`tray::read_pins`, `set_pins`). Instalator `releases/Todo Track_1.7.1_x64-setup.exe` (nie instalowany), lokalny tag `v1.7.1` (niewypchnięty).

Do ręcznego sprawdzenia po instalacji:
- okno widgetu rośnie do 96 px po przypięciu i wraca do 64 px po odpięciu ostatniego; pozycja okna zostaje;
- menu traya wymienia pozycje po przypięciu, odpięciu, ukończeniu i starcie innego zadania (odświeżenie do 5 s), klik startuje zadanie i pokazuje „Start: …” w dymku i widgecie.

Gałąź `claude/hungry-pare-c0cad4` (7 commitów UX, `UX_AUDIT.md`) nie weszła do 1.7.1; scala się z `release/1.0` bez konfliktów, ale po scaleniu trzeba uruchomić pełne testy.

## Stan po 1.7.0 (16 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wydanie 1.7.0 obejmuje wszystkie zadania z PLAN_1.7.md:
- K1 (logowanie Microsoft i pobieranie spotkań): `src-tauri/src/calendar.rs` (OAuth z PKCE w przeglądarce systemowej, jednorazowy serwer na losowym porcie, limit 5 min, refresh token w magazynie poświadczeń Windows), tabela `calendar_events`, pobieranie z Graph przy starcie, co 10 min i po „Odśwież”, Ustawienia → Kalendarz (`CalendarSettings.vue`), symulowane połączenie w podglądzie.
- K2 (spotkania w planie i pojemności): `src/capacity.ts` i `src-tauri/src/capacity.rs`; tray, powiadomienie o celu, Dashboard, Zadania, Zamknięcie dnia („Dodaj jako czas”), tablica tygodnia i tor „Spotkania” na osi dnia.
- S1 (zakładka Ustawienia): osobna pozycja nawigacji, Ctrl+,, lista sekcji z wyszukiwaniem i zapamiętaną sekcją, `navigate` z polem `section`.
- P1 (szacunki z poprawką i „Rozłóż”): `src/estimates.ts`, podgląd planu na tablicy tygodnia, komenda `plan_tasks` (jedna transakcja), ustawienie `planWithCorrection`.
- P2 (poranny przegląd): Plan i historia → Rano, `morningReviewDay`/`morningReviewAt`, powiadomienie „Przegląd dnia” raz dziennie przez kolejkę trybu skupienia.
- P3 (cykle miesięczne i „Czekam na”): `repeatRule` w `task_details`, grupa „Czekam na…”, powiadomienie z „Odblokuj” i „Przypomnij jutro”, propozycje ze statusów Jiry.
- W1 (czas agentów): tabela `agent_sessions`, tor „Agenci”, kolumny „Agent” i „Równolegle”.
- W2 (rozproszenie i przegląd tygodnia): `src/report.ts`, karta „Rozproszenie”, Tydzień → Przegląd, tabela `week_notes`, powiadomienie „Przegląd tygodnia”.
- W3 (podpowiedzi z aktywnego okna): `src-tauri/src/foreground.rs`, źródło „Okno” w mechanizmie propozycji, domyślnie wyłączone.
- W4 (rozszerzenie przeglądarki): `browser-extension/` (Manifest V3), `todo-track-bridge.exe` (native messaging → nazwany potok), rejestracja hosta w HKCU dla Chrome i Edge, źródło „Przeglądarka”.
- J1 (przesunięcie zgłoszenia przy starcie pracy): baner, widget i powiadomienie z „Przesuń”, „Przesuń w Jirze…” w kafelku, propozycja przy ukończeniu, główny wyłącznik zapisów do Jiry.
- J2 (Jira z poziomu Todo Track): Lista/Tablica, kolumny z konfiguracji tablicy (tabele `jira_board_columns`, `jira_statuses`, `jira_board_meta`), przeciąganie i „Przesuń do…”, przypisanie, komentarz, nazwa gałęzi, nowe zgłoszenie, symulowana Jira w `store.ts`.

Wersja 1.7.0 w pięciu plikach, instalator `releases/Todo Track_1.7.0_x64-setup.exe` (nie instalowany), lokalny tag `v1.7.0` (niewypchnięty). README: „Nowości 1.7” z instrukcją rejestracji aplikacji w Entra ID i instalacji rozszerzenia przeglądarki, nowa zakładka Ustawienia w opisie ustawień, opis Jiry poprawiony (od 1.7 aplikacja zapisuje do Jiry po kliknięciu), wersja w „Instalacja i aktualizacje”.

Weryfikacja: `npm run build`, 253 testy Playwright (w tym zrzuty wzorcowe), `cargo fmt --check`, 234 testy Rust (`cargo test --locked`) oraz `npm run tauri build -- -- --locked` przechodzą (Git Bash, cargo dodane do PATH). Maszyna była mocno obciążona innymi procesami: pierwszy pełny przebieg Playwright (domyślna liczba procesów) dał 74 przekroczenia 30 s (czekanie na stronę, kliknięcie, `newPage`), drugi pełny przebieg z `--workers=2` 250/253, a pozostałe 3 przeszły przy `--workers=1`; każdy z 74 testów pierwszego przebiegu przeszedł po ponownym uruchomieniu. Żaden błąd nie dotyczył treści asercji poza skutkami przekroczenia czasu. Warto powtórzyć pełny przebieg na odciążonej maszynie albo w CI.

Odstępstwa od planu:
- Żadne zadanie 1.7 nie było uruchamiane w natywnej aplikacji ani po instalacji NSIS. Nic nie łączyło się z prawdziwym Microsoft Graph ani prawdziwą Jirą (e2e działają na podglądzie z symulowanym kalendarzem i Jirą, testy Rust nowych tabel używają bazy w pamięci). Zgodnie z PLAN_1.7.md ręczne testy nie są częścią planu; lista jest niżej.
- Świadomie zregenerowane lokalnie zrzuty wzorcowe, żaden nie potwierdzony na windows-latest: K1 — 4 `ustawienia-*`, K2 — 12, S1 — 28 (Ustawienia i nawigacja), W4 — 4 Ustawień, J2 — 8 widoku Jira.
- S1 przeniósł wejście do ustawień w istniejących testach e2e z „Historia i kopie → Ustawienia” na nową zakładkę Ustawienia (zgodnie z planem).
- README przy wydaniu poprawiono: sekcja „Ustawienia” opisywała jeszcze „Historia i kopie → Ustawienia”, a sekcja „Jira” mówiła, że integracja nie zmienia zgłoszeń.
- Otwarte jak po 1.6.0: etap 0 (instalacja 1.2.0, tagi `v1.0.0` i `v1.2.0`, push tagów), dwa zielone przebiegi CI, natywna walidacja przycisków w powiadomieniach (AUMID).

Do ręcznego sprawdzenia:

K1 — nic nie było uruchamiane z prawdziwym Microsoftem. Wymaga natywnej aplikacji (`tauri dev` albo instalacja NSIS) i rejestracji aplikacji w Entra ID: platforma „Mobile and desktop”, przekierowanie `http://localhost`, uprawnienia delegowane `User.Read`, `Calendars.Read` i `offline_access`.
- Logowanie: „Połącz konto Microsoft” otwiera przeglądarkę; po zalogowaniu strona pokazuje „Zalogowano…”, a Ustawienia e-mail konta i liczbę spotkań.
  - Czy Entra przyjmuje `http://localhost:<losowy port>` wobec zarejestrowanego `http://localhost`.
  - Czy przekierowanie dociera do serwera, gdy `localhost` rozwiązuje się na `::1` albo na `127.0.0.1`.
- Komunikaty błędów:
  - zły client ID albo błąd w rejestracji pokazuje komunikat `AADSTS…`;
  - zamknięcie karty przeglądarki i „Anuluj” przerywa logowanie;
  - czekanie ponad 5 minut kończy się komunikatem o limicie czasu.
- Dzierżawca: domyślne `organizations` z kontem firmowym; identyfikator dzierżawcy albo domena firmy też działają.
- Token i restart: refresh token w Menedżerze poświadczeń Windows (`<identyfikator>.calendar`); po restarcie spotkania pobierają się bez ponownego logowania; odświeżenie po wygaśnięciu access tokenu (około 1 h) działa.
- Dane z Graph:
  - spotkania zgodne z Outlookiem: godziny lokalne, całodniowe od lokalnej północy, cykliczne;
  - długi kalendarz pobiera wszystkie strony;
  - spotkanie usunięte w Outlooku znika po „Odśwież”.
- Automatyczne pobieranie co 10 minut i przy starcie, także przy starcie schowanym do traya.
- „Rozłącz” usuwa token z magazynu poświadczeń i `calendar_events` z bazy.
- Brak zgody administratora: gdy dzierżawca wymaga zgody na `Calendars.Read`, komunikat jest czytelny.
- Istniejąca baza: start na bazie użytkownika tworzy tabelę `calendar_events` i ustawienie `calendar_show_titles` bez utraty danych (testy automatyczne używały bazy w pamięci).
- Kopia: zawiera `calendar_events`; po przywróceniu token nie jest przenoszony (jak przy Jirze).
- 4 zregenerowane wzorce `ustawienia-*` potwierdzić na windows-latest w CI.

K2 — w natywnej aplikacji z połączonym kontem Microsoft:
- Tooltip traya pokazuje „dziś X / cel minus spotkania”, a w całodniowy „poza biurem” celu nie ma.
- Powiadomienie „Cel dnia osiągnięty” przychodzi przy pomniejszonym celu.
- Tor osi dnia, pojemność tablicy tygodnia i lista „Dzisiaj” odświeżają się po pobraniu co 10 minut i po „Odśwież” (zdarzenie `calendar-changed`).
- W Zamknięciu dnia „Dodaj jako czas” zapisuje spotkanie jako czas przez `add_session` w Rust, a ostrzeżenie o nakładaniu zgadza się z jego odmową.
- Rozłączenie konta usuwa tor i pomniejszenia pojemności.
- Spotkania przez północ i przez zmianę czasu (25.10) wyglądają poprawnie na osi.
- 12 zregenerowanych zrzutów potwierdzić na windows-latest.

S1:
- W natywnej aplikacji (WebView2) Ctrl+, otwiera Ustawienia i nie koliduje ze skrótem WebView. W polu skrótu w sekcji Skróty Ctrl+, ma być przechwycony jako kombinacja, a nie otwierać zakładkę.
- Kliknięcie powiadomienia Windows z `navigate {view:"settings", section}`, gdy dodaje je późniejsze zadanie (np. P2/W2).
- Zapamiętana sekcja w natywnej aplikacji po restarcie (localStorage WebView2).
- Przycisk „Otwórz ustawienia” przy błędach kalendarza („Ustawieniach → Kalendarz”) w natywnej aplikacji.
- Link „Ustawienia → Jira” w widoku Jira z połączonym prawdziwym kontem.
- Układ ustawień przy prawdziwych rozmiarach okna i DPI, w tym przejście na listę rozwijaną poniżej 850 px.
- 28 zrzutów wzorcowych potwierdzić na windows-latest w CI.

P1:
- W natywnej aplikacji „Rozłóż” → „Zastosuj” przez komendę `plan_tasks`: odświeżenie `data-changed`, widget i tray po zapisie. Logika Rust testowana tylko na bazie w pamięci.
- „Planuj z poprawką szacunków” w Ustawieniach → Czas pracy: zapis w SQLite natywnej aplikacji i dotarcie do otwartych okien przez `settings-changed`.
- „Rozłóż” z połączonym kalendarzem Microsoft: pojemność pomniejszona o spotkania, a całodniowy „poza biurem” oznaczony w podglądzie jako „wolne” (sprawdzone tylko w podglądzie bez kalendarza).
- Podpowiedź „zwykle ~…” w osobnym oknie szybkiego przełączania (Ctrl+Shift+Space) w natywnej aplikacji; liczy z danych, które samo wczytuje.
- Wygląd podglądu przy wąskim oknie i w ciemnym motywie w oknie aplikacji (oglądane tylko na zrzucie Playwright).

P2:
- Toast „Przegląd dnia” w natywnej aplikacji: czy przychodzi w dzień roboczy po godzinie początku dnia, tylko raz dziennie także po restarcie, i czy kliknięcie otwiera Plan i historia → Rano.
- Czy toast nie przychodzi po „Zaczynam dzień” zrobionym przed początkiem dnia, w święto i przy całodniowym urlopie z kalendarza Microsoft.
- Czy w czasie bloku skupienia toast czeka w kolejce i przychodzi w zbiorczym powiadomieniu po bloku. Kliknięcie zbiorczego pokazuje tylko główne okno, bez zakładki Rano.
- Zapis `morningReviewDay` i `morningReviewAt` do SQLite oraz odczyt po restarcie.
- Pasek pojemności w Rano z prawdziwymi spotkaniami i z włączonym „Planuj z poprawką szacunków”.
- Wygląd zakładki Rano przy 760 px w obu motywach (nie ma dla niej zrzutu wzorcowego).
- Po instalacji NSIS: brak AUMID w kompilacji deweloperskiej utrudnia sprawdzenie toastów.

P3 — natywnie (`tauri dev` albo po instalacji NSIS), nic nie było uruchamiane:
- Toast „Czekam na” z przyciskami „Odblokuj” i „Przypomnij jutro” (wymaga AUMID):
  - „Odblokuj” odblokowuje bez startu timera;
  - „Przypomnij jutro” przywraca toast w kolejny dzień roboczy;
  - kliknięcie treści otwiera Zadania na grupie „Czekam na…” z rozwiniętym zadaniem;
  - w czasie bloku skupienia toast czeka w kolejce.
- Istniejąca baza po aktualizacji: stare `task_details` bez `repeatRule` i pól `waiting*` czytają się poprawnie; stare cykle tygodniowe generują się jak dotąd; istniejące blokady nie pokazują „od N dni” do ponownego zapisu.
- Reguły miesięczne w prawdziwej bazie:
  - zadanie źródłowe dostaje kopie z datami po restarcie;
  - „ostatni dzień roboczy” uwzględnia `holidaysInTarget`;
  - po cofnięciu do 1.6 zadanie miesięczne nie jest traktowane jak tygodniowe.
- Propozycje z Jiry z prawdziwym kontem: po automatycznym odświeżeniu statusów (co 5 min) kafelek pokazuje propozycję; „Oznacz jako czekam na” zatrzymuje trwający pomiar; odblokowanie i „Nie teraz” nie przywracają jej dla tego samego statusu.
- Ustawienia → Jira: wpisanie i zapis statusów w polu tekstowym w WebView2; Ustawienia → Powiadomienia: wartości 0 i 31 (błąd).
- Układ przycisku „Odblokuj” w wierszu kafelka i paska propozycji w obu motywach przy 760 px (bez zrzutu wzorcowego).
- Pola wyboru „Rodzaj cyklu”, „Który w miesiącu” i „Dzień tygodnia cyklu” w rozwiniętym kafelku przy 760 px.

W1 — w natywnej aplikacji (`tauri dev`) albo po instalacji NSIS:
1. Na istniejącej bazie użytkownika tabela `agent_sessions` i jej indeksy powstają bez utraty danych, a czyszczenie przy starcie nie spowalnia startu.
2. Prawdziwy agent Claude Code / Codex daje jeden okres na agenta i folder, łączony przez kolejne 5-sekundowe cykle. Przerwa 2+ min albo zamknięcie aplikacji kończy okres. Okres po restarcie w ciągu 2 min dołącza do poprzedniego.
3. Tor „Agenci” na dzisiejszej osi odświeża się w ciągu około 60 s.
4. Przypisanie folderu w kafelku „Agenci AI” uzupełnia ostatnie okresy bez zadania, a oś i kafelek pokazują kolor zadania.
5. Sekcja „Czas agentów” w rozwiniętym kafelku i kolumny w Miesiącu pokazują sensowne wartości przy trwającym pomiarze („Równolegle” rośnie).
6. Wygląd toru „Agenci”, szarych bloków i legendy „Agent bez zadania” w obu motywach przy 760 i 1280 px (żaden zrzut wzorcowy nie obejmuje danych agentów).
7. Kopia i przywracanie przenoszą `agent_sessions` (sprawdzone tylko testem Rust).
8. Blokada bazy brana co 5 s przy pracy agentów nie spowalnia interfejsu.

W2:
- W natywnej aplikacji powiadomienie Windows „Przegląd tygodnia” przychodzi w ostatni dzień roboczy tygodnia o ustawionej godzinie (domyślnie 15:00), tylko raz w tygodniu (także po restarcie) i czeka do końca bloku skupienia. Kliknięcie otwiera Plan i historia → Tydzień → Przegląd dla bieżącego tygodnia.
- Tydzień cofa się na wcześniejszy dzień przy święcie albo całodniowym „poza biurem” w kalendarzu (pokryte testami Rust, nie oglądane w aplikacji).
- Po instalacji NSIS: tabela `week_notes` powstaje na istniejącej bazie, wnioski zapisują się i przeżywają restart, a kopia i przywracanie SQLite zachowują wnioski.
- Karta „Rozproszenie” na Dashboardzie i kolumna „Bloki skupienia” poprawnie czytają prawdziwe wiersze `focus_blocks`, w tym trwający blok.
- Sekcja czasu agentów w przeglądzie tygodnia wypełnia się z prawdziwych wierszy `agent_sessions`.
- Ustawienia → Powiadomienia: wyłącznik i godzina przeglądu tygodnia zapisują się przez backend, a pole godziny jest wyszarzone przy wyłączonym przełączniku.

W3:
- Po włączeniu ustawienia w natywnej aplikacji Chrome lub Edge z otwartym zgłoszeniem Jiry (klucz w tytule, zadanie powiązane lokalnie) daje baner „Pracujesz teraz przy innym zadaniu?” po około 2 minutach. „Przełącz” uruchamia zadanie.
- VS Code, Cursor i IntelliJ z folderem przypisanym w `agent_bindings` albo znanym z pracy agenta, na gałęzi `feature/KEY-123`: tytuł okna jest rozpoznany. Sprawdzić prawdziwe formaty tytułów, w tym en dash w JetBrains i ustawienie `window.title` w VS Code.
- Kliknięcie w baner albo widget (okno Todo Track na pierwszym planie) nie gasi propozycji.
- Przy schowanym głównym oknie przychodzi jedno powiadomienie „Pracujesz teraz przy innym zadaniu?” z „Przełącz”. W trybie skupienia trafia do kolejki.
- Po wyłączeniu ustawienia propozycja z okna znika od razu (`set_settings` → odświeżenie). Tytuły nie trafiają do bazy ani logów.
- Teams (`ms-teams.exe`) i nowy Outlook (`olk.exe`): sprawdzić, czy nazwy procesów są rozpoznawane.

W4 — nic nie było uruchamiane w Chrome ani Edge i nic nie było instalowane.

Wczytanie rozszerzenia i połączenie z zainstalowaną aplikacją:
1. Po instalacji `Todo Track_1.7.0_x64-setup.exe` włączyć Ustawienia → Czas pracy → „Rozszerzenie przeglądarki (Chrome i Edge)”.
2. Sprawdzić, że `pl.todotrack.bridge.json` pojawia się obok `Todo Track.exe` (albo w folderze danych aplikacji, gdy folder instalacji nie jest zapisywalny).
3. Sprawdzić klucze `HKCU\Software\Google\Chrome\NativeMessagingHosts\pl.todotrack.bridge` i `HKCU\Software\Microsoft\Edge\NativeMessagingHosts\pl.todotrack.bridge`; wartość domyślna ma być ścieżką manifestu.
4. Wyłączenie ustawienia ma usunąć klucze rejestru i manifest.

„Załaduj rozpakowane”:
1. W `chrome://extensions` i `edge://extensions` wczytać `<folder instalacji>\browser-extension`.
2. Identyfikator ma być `cecpcpkmmmkgfdklnkkkigofkcibnhbl` w obu przeglądarkach.
3. Opcje rozszerzenia mają pokazywać „Połączono z Todo Track”.
4. Przy zamkniętej aplikacji: „Todo Track nie działa (niepołączone)”. Przy wyłączonym ustawieniu: „Brak połączenia… Włącz…”.

Rozpoznanie i propozycje:
1. Na tablicy Jira Cloud z `?selectedIssue=WMS-123` i przeglądarką na pierwszym planie po około 2 minutach ma się pojawić baner albo powiadomienie „Pracujesz teraz przy WMS-123?” z „Przełącz”.
2. Ma mieć pierwszeństwo przed tytułem okna.
3. Po przejściu do innego programu albo po ponad 30 s bez odświeżenia sygnał ma zniknąć.
4. Stan w Ustawieniach: „Połączone · ostatni sygnał: WMS-123, …”.

Do obserwacji:
- Czy przy starcie `todo-track-bridge.exe` przez Chrome lub Edge mignie okno konsoli (program podsystemu konsoli).
- Czy service worker MV3 rzeczywiście żyje przy otwartym porcie natywnym i czy alarm co 30 s budzi go po długiej bezczynności przeglądarki.
- Czy `windows.onFocusChanged` (`WINDOW_ID_NONE`) na Windows poprawnie czyści sygnał przy przejściu do innego programu.

Inne:
- Na niedozwolonej domenie (np. github.com) most ma dostawać tylko `{type:"clear"}` (obserwacja stdin mostu albo test Rust).
- Edge: czy najpierw używa własnego klucza rejestru (powinien) i czy działa ten sam identyfikator.
- Odinstalowanie NSIS ma usunąć most, folder rozszerzenia, manifest hosta i oba klucze HKCU.
- Aktualizacja instalacji z włączonym ustawieniem: przy starcie aplikacja przepisuje manifest z bieżącą ścieżką exe.
- Start na istniejącej bazie użytkownika: klucz `browser_extension` dostaje wartość domyślną (wyłączone).
- 4 zregenerowane zrzuty Ustawień potwierdzić na windows-latest.

J1 — nic nie było uruchamiane w natywnej aplikacji; sprawdzić w `tauri dev` albo po instalacji:
- Start zadania powiązanego ze zgłoszeniem w „To Do” (przycisk, Ctrl+K szybkie przełączanie, skrót globalny, „Rozpocznij” z powiadomienia, „Przełącz” z propozycji agenta): baner pojawia się w ciągu kilku sekund, a timer już działa. „Przesuń” zmienia status w prawdziwej Jirze i na kafelku.
- Zgłoszenie z kilkoma przejściami „w toku” i nazwą niezgodną z ustawieniem: baner pokazuje listę. Przejście z ekranem wymaganych pól zwraca komunikat 400, a przycisk „Otwórz KEY w Jirze” otwiera zgłoszenie.
- Konto bez uprawnienia do przejść (403): czytelny komunikat.
- Główne okno schowane w trayu: toast „Przesunąć KEY do „…”?” z działającym przyciskiem „Przesuń” (wymaga AUMID). Bez wybranego przejścia nie ma przycisku, a kliknięcie otwiera główne okno z banerem.
- W czasie bloku skupienia toast czeka w kolejce, a baner wraca po bloku.
- Widget 320×64 w obu motywach: linia „Jira: przesunąć …” i przycisk z ikoną mieszczą się, a przesunięcie z widgetu działa.
- Ustawienia → Jira: wyłączenie „Pozwól aplikacji zmieniać zgłoszenia w Jirze” blokuje pytanie, przycisk w kafelku i propozycję przy ukończeniu. Bezpośrednie wywołanie `jira_transition` zwraca komunikat `WRITE_DISABLED`.
- Istniejąca baza użytkownika: nowe klucze `jira_write`, `jira_ask_move` i `jira_move_status` dostają wartości domyślne bez utraty danych.
- „Nie pytaj dla tego zadania” zapisuje się w `task_details` i przeżywa edycję formularza planu i restart aplikacji.
- Ukończenie zadania z kluczem: propozycja ze wszystkimi przejściami; bez kliknięcia nic się nie zmienia.

J2 — nic z tego nie było uruchamiane w natywnej aplikacji ani po instalacji NSIS. Sprawdzić z prawdziwym kontem Jira Cloud:
- `GET /rest/agile/1.0/board/{id}/configuration`: nazwy kolumn i klucz projektu zgodne z tym, co pokazuje Jira.
- `GET /rest/api/3/project/{key}/statuses`: kategorie czytane poprawnie; status spoza kolumn trafia do właściwej kolumny.
- Endpoint `/rest/software/1.0/board/{id}/issue` przyjmuje JQL z „ORDER BY Rank ASC” i zwraca pola `issuetype`, `priority` i `assignee`.
- Limit 300 i komunikat o obcięciu na dużej tablicy w „Cała tablica”.
- Przeciąganie myszą w WebView2:
  - optymistyczne przesunięcie, potem odświeżenie statusu na kafelku zadania;
  - wybór, gdy kilka przejść prowadzi do jednej kolumny;
  - przejście z Resolution albo innym wymaganym polem otwiera zgłoszenie w przeglądarce, a karta wraca.
- Błędy 400, 403 i 404 z Jiry pokazują się po polsku.
- „Przypisz do mnie” i „Zmień osobę…” działają przez wyszukiwanie `assignable/search`, łącznie z uprawnieniami.
- Komentarz trafia do Jiry jako ADF z akapitami i podziałami wierszy.
- „Kopiuj nazwę gałęzi” wstawia nazwę do schowka systemowego, z prefiksem z Ustawień.
- „Nowe zgłoszenie”:
  - createmeta zwraca `issueTypes` albo `values`;
  - typ z dodatkowym wymaganym polem pokazuje link „Utwórz w Jirze”, a link `CreateIssue!default.jspa` otwiera się poprawnie;
  - utworzone zgłoszenie jest przypisane do mnie, a powiązane zadanie ma wybrany dzień planu.
- Z wyłączonym „Pozwól aplikacji zmieniać zgłoszenia w Jirze” tablica jest tylko do odczytu, a backend odrzuca zapisy.
- Pamięć podręczna odświeża się po 24 h i po „Odśwież”. Na istniejącej bazie użytkownika tabele `jira_board_columns`, `jira_statuses` i `jira_board_meta` powstają bez utraty danych.
- Układ tablicy w obu motywach przy 760 i 1280 px z wieloma kolumnami (poziome przewijanie wewnątrz tablicy, nie całej strony).
- 8 zregenerowanych wzorców Jiry potwierdzić na windows-latest.

## Stan po 1.6.0 (15 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wydanie 1.6.0 obejmuje etapy 5 i 6 z PLAN_1.3.md:
- M1 (święta i zakładka Miesiąc): `src-tauri/src/holidays.rs` i `src/holidays.ts` (polskie święta z jednej listy wzorcowej `tests/fixtures/holidays-pl-2024-2030.json`), ustawienie `holidaysInTarget` (domyślnie włączone) w celu dnia, tygodnia, miesiąca, tablicy tygodnia, trayu i powiadomieniu o celu; zakładka Raporty → Miesiąc (`MonthReport.vue`). Po przeglądzie skrót „Dni robocze tego miesiąca” zachowuje święta w dniu roboczym tygodnia, w które zapisano czas.
- M2 (eksport): `src-tauri/src/export.rs` (komenda `export_report`, CSV pod polski Excel, XLSX przez `rust_xlsxwriter` 0.99.1, test odczytu przez `calamine` 0.36.1), `ExportButtons.vue` w zakładce Miesiąc i „Wybranych dniach”, odpowiednik CSV w podglądzie `src/export.ts`. Po przeglądzie część przedziału kończąca się o północy ma w XLSX koniec 24:00 (format `[hh]:mm`, czas jako ułamek doby), tak jak w CSV.
- F1 (tryb skupienia): `src-tauri/src/focus.rs` (tabele `focus_blocks` i `focus_queue` przez `CREATE TABLE IF NOT EXISTS`, komendy `focus_status` i `focus_action`, wątek co 2 s, kolejka powiadomień: `notify::show` odkłada w czasie bloku, `notify::display` wysyła od razu), akcje toastów `focus-break`, `focus-next`, `focus-resume`, ustawienia `focusBlockMinutes`, `focusBreakMinutes`, `focusBreakStops`; frontend `src/focus.ts` (odpowiednik w podglądzie), `FocusBanner.vue`, przycisk na pasku pomiaru i w widgecie, karta na Dashboardzie. Po przeglądzie kolejka jest też w tabeli `focus_queue`, więc odłożone powiadomienia przeżywają zamknięcie aplikacji w trakcie bloku (wracają bez przycisków).

Wersja 1.6.0 w pięciu plikach, instalator `releases/Todo Track_1.6.0_x64-setup.exe` (nie instalowany), lokalny tag `v1.6.0` (niewypchnięty).

Weryfikacja: `npm run build`, 149 testów Playwright (w tym zrzuty wzorcowe), `cargo fmt --check`, 135 testów Rust (`cargo test --locked`) oraz `npm run tauri build -- -- --locked` przechodzą (Git Bash, cargo dodane do PATH).

Odstępstwa od planu:
- Etapy 5 i 6 nie są sprawdzone w natywnej aplikacji ani po instalacji NSIS; e2e działają na podglądzie (XLSX w podglądzie niedostępny), testy Rust tabel `focus_blocks` i `focus_queue` używają tylko bazy w pamięci.
- M1 świadomie zmienił wygląd Ustawień: 4 wzorce `ustawienia-*-win32.png` zregenerowano lokalnie. F1 zregenerował lokalnie 14 wzorców (`zadania-*`, `dashboard-*`, `ustawienia-*`, `widget-*`). Żaden nie był potwierdzony na windows-latest.
- README (Nowości 1.6) przy wydaniu poprawiono: wcześniej mówił, że kolejka skupienia żyje tylko w pamięci, a po przeglądzie F1 jest zapisywana w `focus_queue`.
- Otwarte jak po 1.5.0: etap 0 (instalacja 1.2.0, tagi `v1.0.0` i `v1.2.0`, push tagów), dwa zielone przebiegi CI, natywna walidacja przycisków w powiadomieniach.

Do ręcznego sprawdzenia:

M1 — w natywnej aplikacji (`tauri dev` albo po instalacji NSIS):
- Start na istniejącej bazie użytkownika dodaje klucz `holidays_in_target` z wartością domyślną (włączone). Wyłączenie w Ustawieniach → Czas pracy zapisuje się w SQLite i przeżywa restart.
- W święto wypadające w dzień roboczy tooltip traya nie pokazuje celu dnia, a powiadomienie „Cel dnia osiągnięty” się nie pojawia. Rust sprawdza to tylko przez `day_target_minutes`; ścieżek traya i powiadomień nie testowano z datą święta.
- Zakładka Miesiąc w obu motywach przy szerokości 760 i 1280: podsumowanie, obie tabele, wykres 28–31 dni i pole miesiąca (`input type=month`) w WebView2.
- 4 wzorce Ustawień potwierdzić na windows-latest w CI.

M2 — w natywnej aplikacji (`tauri dev` albo po instalacji NSIS):
- Okno zapisu ma podpowiedzianą nazwę (`Todo-Track-RRRR-MM` albo `Todo-Track-od_do`) i filtr csv lub xlsx. Anulowanie nie pokazuje komunikatu.
- Plik CSV otwarty dwuklikiem w polskim Excelu: polskie znaki, kolumny rozdzielone średnikiem, godziny z przecinkiem.
- Plik XLSX w Excelu: arkusze „Przedziały” i „Podsumowanie”, formaty dat, godzin (w tym 24:00) i liczb, zamrożony pierwszy wiersz, sumy.
- Zapis do pliku otwartego w Excelu kończy się czytelnym błędem.
- Eksport przy trwającym pomiarze ma dopisek „trwa”, a koniec przedziału to chwila zamknięcia okna zapisu.
- Układ przycisków i komunikatu w podsumowaniu zakładki Miesiąc i pod listą „Wybrane dni” w obu motywach i przy szerokości 760 px (nie oglądane zrzutem ekranu).

F1 — nic nie było uruchamiane w natywnej aplikacji ani po instalacji NSIS. Do sprawdzenia natywnie, z zarejestrowanym AUMID:
- Toast „Koniec bloku skupienia” z przyciskami „Przerwa” i „Kolejny blok”, potem toast „Wracasz do X?” z „Rozpocznij”.
- Zbiorczy toast „Podczas skupienia: N powiadomień” po bloku, np. przy długim pomiarze ustawionym na 1 min w czasie bloku.
- Zamknięcie aplikacji w trakcie bloku z odłożonymi powiadomieniami: po starcie blok trwa, a powiadomienia przychodzą po nim.
- Uśpienie komputera w trakcie bloku: po wybudzeniu blok ma się zakończyć w ciągu około 2 s, z końcem równym planowanemu.
- Stop z traya, ze skrótu globalnego i z toastu w czasie bloku: blok przerwany, kolejka wysłana od razu.
- Baner propozycji agentów i linia „Agent: …” w widgecie znikają w czasie bloku i wracają po nim.
- Wygląd widgetu 320×64 w obu motywach z przyciskami „Przerwa” i „Kolejny blok”.
- Start na istniejącej bazie użytkownika: tabele `focus_blocks` i `focus_queue` powstają bez utraty danych.
- 14 zregenerowanych zrzutów wzorcowych do potwierdzenia na windows-latest.

## Stan po 1.5.0 (15 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wydanie 1.5.0 obejmuje etap 4 z PLAN_1.3.md: AG1 (rozpoznanie zadania agenta: `git_branch` czyta `HEAD` bez procesu `git`, także worktree, submoduł i odłączony HEAD, cache 30 s; klucz Jiry z gałęzi albo nazwy sesji Claude Code; tabela `agent_bindings` przez `CREATE TABLE IF NOT EXISTS` z komendami `agent_bindings`, `agent_bind`, `agent_unbind` i odpowiednikami w podglądzie; kafelek Agenci AI z gałęzią i zadaniem) oraz AG2 (propozycje przełączenia w `src-tauri/src/suggest.rs`: baner `AgentSuggestions.vue`, linia w widgecie, powiadomienie z „Przełącz” przy schowanym oknie, „Nie teraz”, „Nie proponuj dla tego folderu”, „Zapamiętaj dla tego folderu”; ustawienia `agentSuggest` i `agentMutedFolders` w Ustawieniach → Czas pracy; trzecia odpowiedź banera bezczynności „Przypisz przerwę do X”). Wersja 1.5.0 w pięciu plikach, instalator `releases/Todo Track_1.5.0_x64-setup.exe` (nie instalowany), lokalny tag `v1.5.0` (niewypchnięty).

Weryfikacja: `npm run build`, 137 testów Playwright (w tym zrzuty wzorcowe), `cargo fmt --check`, 116 testów Rust (`cargo test --locked`) oraz `npm run tauri build -- -- --locked` przechodzą (Git Bash, cargo dodane do PATH).

Odstępstwa od planu:
- Etap 4 nie jest sprawdzony w natywnej aplikacji ani po instalacji NSIS; e2e działają na podglądzie z symulowanymi agentami, a test Rust tabeli `agent_bindings` używa tylko bazy w pamięci.
- AG2 świadomie zmienił wygląd Ustawień (wyłącznik propozycji agentów): 4 wzorce `ustawienia-*-win32.png` zregenerowano lokalnie, nie na windows-latest.
- Po przeglądzie AG2 „Przełącz” dla zgłoszenia znanego tylko z Jiry nie wymaga już wczytanej listy: gdy listy nie pobrano od startu aplikacji, tytuł pobiera wprost z Jiry (`jira::start_by_key`). Komunikat „Odśwież listę zgłoszeń przed rozpoczęciem.” zostaje tylko w `jira_start` z widoku Jira. Kafelek Agenci AI pokazuje tytuł z Jiry nadal dopiero po wczytaniu listy w sesji.
- Otwarte jak po 1.4.0: etap 0 (instalacja 1.2.0, tagi `v1.0.0` i `v1.2.0`, push tagów), dwa zielone przebiegi CI, natywna walidacja przycisków w powiadomieniach.

Do ręcznego sprawdzenia:

AG1 — w natywnej aplikacji (`tauri dev` albo po instalacji NSIS):
- Sesja Claude Code w repo na gałęzi typu `feature/WMS-123-x`: kafelek Agenci AI pokazuje gałąź i zadanie powiązane z WMS-123.
- To samo w worktree git (plik `.git` z `gitdir:` i ścieżką bezwzględną zapisaną ukośnikami) oraz w submodule.
- Odłączony HEAD pokazuje „odłączony HEAD <hash>”. Sesja Claude Code z nazwą zawierającą klucz, na gałęzi bez klucza, bierze klucz z nazwy.
- Klucz znany tylko z listy Jiry: tytuł z Jiry pojawia się dopiero po wczytaniu listy w widoku Jira w tej sesji; inaczej widać sam klucz.
- Start na istniejącej bazie użytkownika (po aktualizacji): tabela `agent_bindings` powstaje bez utraty danych. Test automatyczny używa tylko bazy w pamięci.
- Zmiana gałęzi w repo: kafelek nadąża w ciągu około 35 s (cache 30 s plus pętla 5 s).
- Kopia i przywracanie obejmują `agent_bindings`, bo kopiowany jest cały plik SQLite; nie sprawdzono tego ręcznie.

AG2 — wszystko wymaga natywnej aplikacji (`tauri dev`) albo instalacji NSIS; nic nie było sprawdzone natywnie:
- Baner z prawdziwym agentem: prawdziwy agent Claude Code albo Codex na gałęzi typu `feature/KEY-123` lub w przypisanym folderze daje baner po około 2 minutach (pętla co 5 s). „Przełącz” uruchamia zadanie i zatrzymuje poprzednie, tray i widget się odświeżają.
- Powiadomienie przy głównym oknie schowanym w trayu: jeden toast na parę z działającym przyciskiem „Przełącz” (wymaga zainstalowanego AUMID); zbiorczy toast przy kilku zadaniach; kliknięcie treści toastu pokazuje główne okno z banerem.
- „Nie teraz” w realnym użyciu: trzyma przez kolejne tury `busy` tej samej sesji Claude i wraca po ponad 10 minutach bezczynności agenta.
- Wyciszanie i wyłącznik: „Nie proponuj dla tego folderu” zapisuje do prawdziwej bazy (klucz `agent_muted_folders`), działa dla podfolderów i da się cofnąć w Ustawieniach → Czas pracy; wyłącznik funkcji wyłącza wszystko od razu (`set_settings` przelicza propozycje).
- „Zapamiętaj dla tego folderu” w banerze i wybór zadania w kafelku Agenci AI zapisują przypisanie, a kolejna propozycja dla folderu używa go.
- Linia w widgecie: „Agent: …” i przycisk z ikoną Bot mieszczą się w oknie 320×64 w obu motywach.
- Baner bezczynności, trzecia opcja: przy prawdziwej nieobecności (10+ minut), gdy agent pracuje nad innym zadaniem, pojawia się trzecia opcja; po kliknięciu pomiar dzieli się w chwili wyjścia (`awaySince`) i trwa dalej na X.
- Zgłoszenie tylko z Jiry: przy połączonej Jirze „Przełącz” na takiej propozycji tworzy zadanie; z wczytaną listą przez `jira_start`, bez listy z tytułem pobranym wprost z Jiry (`start_by_key`). Bez połączonego konta ma się pokazać „Połącz konto Jiry, żeby rozpocząć zgłoszenie.”
- Istniejąca baza po aktualizacji: ustawienia dostają nowe klucze z wartościami domyślnymi (propozycje włączone, brak wyciszonych folderów).
- Zrzuty w CI: 4 wzorce Ustawień powstały lokalnie; potwierdzić na windows-latest.

## Stan po 1.4.0 (15 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wydanie 1.4.0 obejmuje etap 3 z PLAN_1.3.md: Q1 (składnia szybkiego dodawania: parser `src/quickadd.ts`, chipy `QuickChips.vue`, komenda `quick_add` w `src-tauri/src/quickadd.rs` z odpowiednikiem w podglądzie, „Utwórz i rozpocznij” / „Utwórz” w szybkim przełączaniu) i Q2 (szablony zadań: tabela `task_templates` przez `CREATE TABLE IF NOT EXISTS`, `templates.rs`, „Zapisz jako szablon” w kafelku, Ustawienia → Szablony z `TemplateEditor.vue`, szablony w kopii i przywracaniu). Wersja 1.4.0 w pięciu plikach, instalator `releases/Todo Track_1.4.0_x64-setup.exe` (nie instalowany), lokalny tag `v1.4.0` (niewypchnięty).

Weryfikacja: `npm run build`, 123 testy Playwright (w tym zrzuty wzorcowe), `cargo fmt --check`, 93 testy Rust (`cargo test --locked`) oraz `npm run tauri build -- -- --locked` przechodzą (Git Bash, cargo dodane do PATH).

Odstępstwa od planu:
- Etap 3 nie jest sprawdzony w natywnej aplikacji ani po instalacji NSIS; tytuł z Jiry dla samego klucza sprawdzono tylko na symulowanej liście zgłoszeń. Bez listy wczytanej w sesji (`jira::cached_summary`) zadanie z samym kluczem nie powstaje i pokazuje „Nie znam tytułu…”.
- Q2 świadomie zmienił wygląd Ustawień (sekcja Szablony): 4 wzorce `ustawienia-*-win32.png` zregenerowano lokalnie, nie na windows-latest. Test `quick-switch.spec.ts` zmieniono w Q1: wcześniej sprawdzał, że Enter bez wyników nic nie robi; teraz sprawdza propozycję „Utwórz i rozpocznij”, bo plan zmienia to zachowanie.
- Otwarte jak po 1.3.0: etap 0 (instalacja 1.2.0, tagi `v1.0.0` i `v1.2.0`, push tagów), dwa zielone przebiegi CI, natywna walidacja przycisków w powiadomieniach.

Do ręcznego sprawdzenia:

Q1 — w natywnej aplikacji (`tauri dev` albo po instalacji NSIS):
- Widok Jira, odświeżenie listy, potem w „Nowe zadanie” sam klucz (np. `WMS-123`): tytuł pochodzi ze zgłoszenia, a zadanie linkuje do właściwej witryny.
- To samo po restarcie bez odświeżonej listy: błąd ma być czytelny.
- Okno Ctrl+Shift+Space (osobne okno szybkiego przełączania): brak wyników, Enter tworzy i uruchamia, Shift+Enter tylko tworzy, okno się chowa, a główne okno się odświeża (zdarzenie `data-changed`).
- Wygląd chipów w ciemnym i jasnym motywie oraz uzupełnianie Tab przy polskim układzie klawiatury.
- Przypomnienie resetuje się po „Utwórz i rozpocznij”.

Q2 — w natywnej aplikacji albo po instalacji pakietu NSIS:
- Aktualizacja: start na istniejącej bazie użytkownika (po aktualizacji); tabela `task_templates` powstaje bez utraty danych. Test automatyczny używa tylko bazy w pamięci.
- Zapis i użycie: zapisać szablon z rozwiniętego kafelka, potem w „Nowe zadanie” `/review WMS-123` przy połączonej Jirze. Tytuł ma pochodzić z listy zgłoszeń (`jira::cached_summary`), co działa dopiero po wczytaniu listy w sesji; inaczej „Nie znam tytułu”. Automatycznie sprawdzono tylko symulowaną listę.
- Okno szybkiego przełączania (skrót globalny): Tab uzupełnia nazwę szablonu, a szablon dodany w Ustawieniach widać przy następnym otwarciu okna (ładowany przy otwarciu).
- Kopia: eksport, usunięcie lub zmiana szablonu, przywrócenie; szablony zgodne z kopią.
- Ustawienia → Szablony: zmiana nazwy, edycja, usunięcie z potwierdzeniem, próba nazwy różniącej się tylko wielkością liter (komunikat błędu).
- Zrzuty w CI: 4 wzorce Ustawień powstały lokalnie; potwierdzić na windows-latest.

## Stan po 1.3.0 (15 października 2026)

Gałąź `release/1.0`, lokalnie, bez pusha. Wydanie 1.3.0 obejmuje etapy 1 i 2 z PLAN_1.3.md: T1 (wspólny fixture `tests/fixtures/seed.ts`, zastępcze IPC `tests/fixtures/tauri.ts`, testy e2e importu z Jiry, szybkiego przełączania, banera bezczynności i przywracania kopii, `npm run check`), T2 (zrzuty wzorcowe i osobny krok CI) oraz N1 (akcje w powiadomieniach Windows przez `tauri-winrt-notification`). Wersja 1.3.0 w pięciu plikach, instalator `releases/Todo Track_1.3.0_x64-setup.exe` (nie instalowany), lokalny tag `v1.3.0` (niewypchnięty). Bez zmian w bazie.

Weryfikacja: `npm run build`, 101 testów Playwright (w tym zrzuty wzorcowe), `cargo fmt --check` i 79 testów Rust (`cargo test --locked`), uruchomione razem przez `npm run check` w Git Bash, oraz `npm run tauri build -- -- --locked` przechodzą.

Odstępstwa od planu:
- Etap 1 nie jest „gotowy” w rozumieniu planu: wzorce zrzutów powstały lokalnie na Windows 11, nie na windows-latest, a warunek „zielone w CI dwa razy z rzędu” nie jest sprawdzony (gałąź nie była pushowana; lokalnie sprawdzono trzy przebiegi). Składnię `check.yml` sprawdzono tylko na oko, bez actionlint.
- Etap 2 nie jest „gotowy” w rozumieniu planu: brak natywnej walidacji przycisków po instalacji NSIS, są tylko testy Rust i Playwright. Niepotwierdzone, czy `on_activated` z `tauri-winrt-notification` 0.8.1 dociera do kanału przy zainstalowanym AUMID, także po przeniesieniu toastu do Centrum akcji i po zakończeniu wątku, który wywołał `show()`.
- Etap 0 (instalacja 1.2.0, tagi `v1.0.0` i `v1.2.0`, push tagów; lokalnie istnieje tylko `v1.1.0`) nadal otwarty; wydanie 1.3.0 powstało bez niego.

Do ręcznego sprawdzenia:

T1 (bez sprawdzeń w aplikacji; kod aplikacji i Rust bez zmian):
- `npm run check` w zwykłym oknie cmd.exe i w terminalu, w którym `cargo` już jest w PATH (uruchamiano w PowerShell bez cargo w PATH i w Git Bash).
- Nadal niesprawdzone natywnie, jak wcześniej: prawdziwy baner bezczynności po powrocie agenta i automatyczny import z Jiry na prawdziwym koncie.

T2 (bez sprawdzeń w aplikacji ani po instalacji NSIS):
- Pierwszy przebieg `check.yml` na windows-latest. Obraz Windows Server może inaczej wygładzać czcionki (Segoe UI, Cascadia/Consolas). Jeśli krok „Zrzuty wzorcowe” przekroczy próg 1%, pobrać artefakt `playwright-report`, obejrzeć różnice i zregenerować wzorce w CI (Check ręcznie przez workflow_dispatch z `update_snapshots` = true, artefakt `visual-snapshots`, podmiana katalogu wzorców, commit) albo podnieść próg. Potem potwierdzić dwa zielone przebiegi z rzędu.

N1 — po instalacji pakietu NSIS (zarejestrowany AUMID; osobny identyfikator walidacyjny albo właściwa instalacja przy zamkniętej aplikacji), przy działającej aplikacji, ze zrzutami do `test-results/`:
- Termin (start albo 09:00): kliknięcie treści otwiera Zadania z zaznaczonym „Dzisiaj: plan i praca” i widocznymi zadaniami z terminem dziś lub zaległymi.
- Długi pomiar (ustawić np. 1 min): kliknięcie treści otwiera Zadania z rozwiniętym bieżącym zadaniem. „Zatrzymaj” zatrzymuje pomiar bez wyciągania okna, a tray i widget się odświeżają. „Podziel…” otwiera okno z edytorem „Edytuj przedział” na zakładce Podziel dla trwającego pomiaru. Po przełączeniu na inne zadanie „Zatrzymaj” na starym toaście nie może zatrzymać nowego pomiaru.
- Wieczór (godzina za minutę): kliknięcie otwiera Plan i historia → Zamknięcie dnia na dziś. „Zatrzymaj teraz” zatrzymuje pomiar.
- Cel dnia (włączony, niski cel): kliknięcie otwiera Zamknięcie dnia.
- Nowe zgłoszenia z Jiry: przy jednym zgłoszeniu przyciski „Rozpocznij” (uruchamia zadanie i zatrzymuje poprzednie; także gdy zadanie przeniesiono wcześniej do kosza, ma wrócić) i „Zaplanuj na dziś” (zadanie dostaje dziś jako dzień w planie, timer nie startuje). Przy 2+ zgłoszeniach tylko „Zaplanuj na dziś”, dla wszystkich. Kliknięcie treści otwiera Zadania przy grupie „Nierozpoczęte”.
- Potwierdzenie skrótu Start/Stop przy schowanym widgecie: kliknięcie pokazuje główne okno.
- Kliknięcia działają też z Centrum akcji po zniknięciu toastu i gdy okno było schowane w trayu. Biblioteka rejestruje `on_activated` na obiekcie `ToastNotification`, który wychodzi z zakresu po `show`; założenie planu: działa, dopóki aplikacja jest uruchomiona.
- Po restarcie aplikacji kliknięcie starego toastu z Centrum akcji tylko uruchamia aplikację albo pokazuje główne okno.
- Dźwięk toastu jak dotąd (cichy, `sound(None)`), tytuł i treść wyglądają poprawnie.

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
