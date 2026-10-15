# Todo Track 1.7 — plan rozwoju

Stan wyjściowy: 1.6.0 (15 października 2026), gałąź `release/1.0`. 135 testów Rust, 149 testów Playwright. Wszystko poniżej wychodzi jako **jedno wydanie 1.7.0** na końcu; etapy nie podnoszą wersji.

## Zakres

| # | Funkcja | Rozmiar | Zadanie |
|---|---------|---------|---------|
| 2 | Spotkania z kalendarza Microsoft (Graph) | L | K1, K2 |
| 1 | Pojemność dnia ze spotkaniami i „Rozłóż” | M | K2, P1 |
| 3 | Szacunki z poprawką z historii | S | P1 |
| 4 | Poranny przegląd | M | P2 |
| 5 | Cykle miesięczne i co N tygodni | S | P3 |
| 6 | „Czekam na” jako lista z przypomnieniem i statusem z Jiry | S | P3 |
| 8 | Czas agentów jako osobny tor | M | W1 |
| 9, 10 | Rozproszenie i przegląd tygodnia | M | W2 |
| 11 | Podpowiedzi z aktywnego okna (opt-in) | M | W3 |
| — | Rozszerzenie Chrome/Edge: „Pracujesz przy WMS-123?” | M | W4 |
| — | Pytanie o przesunięcie zgłoszenia w Jirze do „In Progress” przy starcie (z wyłącznikiem zapisów do Jiry) | S | J1 |
| — | Tablica Jiry według kolumn, przejścia, przypisanie, komentarze, nowe zgłoszenia | L | J2 |
| — | Osobna zakładka „Ustawienia” | S | S1 |

Odłożone: worklogi do Jiry (7) — do osobnego zaplanowania. Panel planowania dla PM — odrzucony; zamiast niego głębsza integracja z Jirą (J2).

Ręczne testy w natywnej aplikacji i po instalacji NSIS nie są częścią planu. Aplikacja trafi do ludzi w firmie i błędy będą zgłaszane na bieżąco. Zadania nadal wypisują rzeczy do ręcznego sprawdzenia, które trafiają do `NEXT_SESSION.md`.

Część rzeczy z listy pomysłów już istnieje i tylko się ją rozbudowuje: pasek pojemności na tablicy tygodnia (cel dnia), pole „Czekam na…” (zatrzymuje pomiar, Start je czyści), cykle według dni tygodnia (`recurrence.rs`), przekładanie niedokończonego planu w Zamknięciu dnia.

## Założenia przyjęte bez pytania

- Timer nigdy nie przełącza się sam. „Rozłóż”, przegląd poranny, statusy z Jiry, okna i agenci tylko proponują; zmiany planu wymagają kliknięcia „Zastosuj”.
- Kalendarz to Microsoft Graph z kontem firmowym. Aplikacja jest publicznym klientem OAuth (auth code + PKCE, przekierowanie na `http://localhost:<losowy port>`), bez sekretu. Identyfikator aplikacji (client ID) i dzierżawca (domyślnie `organizations`) są w Ustawieniach, bo rejestrację w Entra ID robi użytkownik albo administrator. README opisuje, jak ją założyć. Bez client ID funkcja jest wyłączona z czytelnym komunikatem.
- Uprawnienia: `offline_access User.Read Calendars.Read`. Refresh token w magazynie poświadczeń Windows (`keyring`, jak token Jiry); w SQLite tylko spotkania.
- Spotkanie zajmuje czas, gdy `showAs` to `busy`, `oof` albo `workingElsewhere`, nie jest anulowane i nie zostało odrzucone. `tentative` liczy się jako zajęte z osobnym wyglądem. Całodniowe `oof` = dzień wolny (pojemność 0).
- Pojemność dnia = cel dnia (z dniami roboczymi i świętami jak dziś) minus minuty spotkań, które się nie nakładają, nie mniej niż 0.
- Poprawka szacunków to mnożnik z mediany rzeczywistość/plan: projekt (≥5 ukończonych zadań z szacunkiem w 90 dniach), inaczej wszystkie (≥5), inaczej 1. Mnożnik obcięty do 0,5–3. Domyślnie tylko podpowiedź; ustawienie „Planuj z poprawką szacunków” włącza ją w pojemności, osi planu i „Rozłóż”.
- Czas agentów nie liczy się do celu dnia ani do raportów TS i eksportu. To osobna informacja.
- Tytuły okien nie trafiają do bazy ani logów. Podpowiedzi z okien są domyślnie wyłączone.
- Nowe tabele tylko przez `CREATE TABLE IF NOT EXISTS`. Nowe pola zadań w JSON `task_details` z wartościami domyślnymi przez `serde(default)`, bez migracji istniejących danych.

## Etap 1 — kalendarz

### K1 — Logowanie Microsoft i pobieranie spotkań

- `src-tauri/src/calendar.rs`: logowanie przez przeglądarkę systemową (`tauri-plugin-opener`), jednorazowy serwer na `127.0.0.1` z losowym portem przyjmujący `code`, sprawdzenie `state`, wymiana kodu z PKCE (S256) na tokeny, odświeżanie access tokenu z refresh tokenu. Limit czasu logowania 5 min. Wylogowanie usuwa token z magazynu i spotkania z bazy.
- Pobieranie `GET /me/calendarView?startDateTime=…&endDateTime=…` (od poniedziałku poprzedniego tygodnia do +21 dni, z `Prefer: outlook.timezone="UTC"` i stronicowaniem `@odata.nextLink`) co 10 minut, przy starcie i po „Odśwież”. Tabela `calendar_events(id TEXT PRIMARY KEY, start INTEGER, end INTEGER, subject TEXT, show_as TEXT, all_day INTEGER, cancelled INTEGER, response TEXT, updated_at INTEGER)`; zakres pobrania zastępuje się w jednej transakcji, więc usunięte spotkania znikają.
- Ustawienia → **Kalendarz**: client ID, dzierżawca, „Połącz konto Microsoft” / „Rozłącz”, zalogowany e-mail, czas ostatniego pobrania i błąd. Wyłącznik „Pokazuj tytuły spotkań” (wyłączony = „Spotkanie”).
- Komendy `calendar_status`, `calendar_connect`, `calendar_disconnect`, `calendar_refresh`, `calendar_events(from, to)` z odpowiednikiem w `store.ts`: podgląd ma przykładowe spotkania z `tests/fixtures/seed.ts` i symulowane połączenie.
- Kopia SQLite obejmuje `calendar_events`, ale token nie jest przenoszony (jak przy Jirze).

Testy: Rust — PKCE (długość, znaki, S256 dla wektora z RFC 7636), parsowanie odpowiedzi Graph (stronicowanie, całodniowe, anulowane, odrzucone, strefa UTC), zastąpienie zakresu, odrzucenie złego `state`. Playwright — sekcja Ustawień w podglądzie (połącz, rozłącz, błąd bez client ID).

### K2 — Spotkania w planie i pojemności

- Oś dnia (Plan i historia → Dzień): tor „Spotkania” nad torem „Plan”. Bloki planu omijają spotkania: zadanie, które trafiłoby na spotkanie, przesuwa się za nie albo dzieli na dwie części.
- Tablica tygodnia: pojemność dnia pomniejszona o spotkania. Pasek pokazuje część spotkań osobnym kolorem, a podpowiedź „Cel 8:00 · spotkania 2:30 · zostaje 5:30”. Całodniowe `oof` oznacza dzień jako wolny.
- Zadania → „Dzisiaj”: lista nadchodzących spotkań dziś (godzina, tytuł albo „Spotkanie”).
- Zamknięcie dnia: sekcja „Spotkania” z przyciskiem „Dodaj jako czas” przy każdym spotkaniu, który otwiera „Dodaj czas ręcznie” z godzinami spotkania i wyborem zadania (zapamiętany ostatni wybór). Nic nie dodaje się samo; spotkanie nakładające się na istniejący pomiar pokazuje ostrzeżenie zamiast przycisku.
- Wspólna funkcja pojemności (TS, z odpowiednikiem w Rust tam, gdzie liczy ją backend: tray, powiadomienie o celu) używana przez tablicę, oś, Dashboard i późniejsze zadania.

Testy: Rust/TS — pojemność z nakładającymi się spotkaniami, spotkaniem przez północ, całodniowym `oof`, `tentative`. Playwright — tor spotkań, pojemność na tablicy, „Dodaj jako czas” w Zamknięciu dnia. Zrzuty wzorcowe zaktualizowane świadomie.

### S1 — Zakładka „Ustawienia”

Dziś cała konfiguracja siedzi w „Historia i kopie → Ustawienia”, a 1.7 dokłada kolejne sekcje (Kalendarz, Jira, Czekam na, okna, przeglądarka). Wchodzi zaraz po K2, żeby kolejne zadania dokładały opcje już w nowym miejscu.

- Nowa pozycja **„Ustawienia”** w nawigacji głównej (ostatnia, ikona koła zębatego) i skrót **Ctrl+,**. Z „Historia i kopie” znika podzakładka Ustawienia; kopie (eksport/przywracanie SQLite) zostają w „Historia i kopie”.
- Układ: lista sekcji po lewej (przy szerokości 760 px lista rozwijana nad treścią), treść sekcji po prawej, zapamiętana ostatnia sekcja. Sekcje: Czas pracy, Kalendarz, Jira, Powiadomienia, Skupienie, Szablony, Okna, Skróty, Uruchamianie. Pole „Szukaj ustawienia” filtruje sekcje po etykietach.
- Każde miejsce, które dziś prowadzi do ustawień (linki w widokach, powiadomienia, zdarzenie `navigate`, puste stany typu „Połącz konto Jiry”), otwiera nową zakładkę na właściwej sekcji (`navigate` z polem `section`).
- Zachowanie zapisu bez zmian (backend sprawdza zakresy, zapis od razu odświeża skróty, widget i reguły powiadomień). Istniejące etykiety aria pól zostają.
- Testy e2e, które wchodzą do ustawień przez „Historia i kopie”, przechodzą na nową zakładkę (zmiana opisana w deviations); zrzuty wzorcowe Ustawień i nawigacji zaktualizowane świadomie.

Testy: e2e — nawigacja do zakładki, Ctrl+,, lista sekcji i wyszukiwanie, link z innego widoku otwiera właściwą sekcję, zapamiętana sekcja; wszystkie dotychczasowe testy ustawień zielone po zmianie ścieżki wejścia.

## Etap 2 — planowanie

### P1 — Szacunki z poprawką i „Rozłóż”

- Mnożnik według założeń, w jednym miejscu (`src/estimates.ts`, funkcja `correction(project)`), z liczbą zadań, z których pochodzi.
- Podpowiedź: chip szacunku w szybkim dodawaniu („~2h · zwykle ~2h40”), pole szacunku w rozwiniętym kafelku i karta zadania na tablicy tygodnia. Bez wystarczających danych podpowiedzi nie ma.
- Ustawienie „Planuj z poprawką szacunków” (domyślnie wyłączone) w Ustawieniach → Czas pracy.
- Tablica tygodnia: przycisk **„Rozłóż”**. Bierze zadania z „Do zaplanowania” i „Zaległe”, sortuje według terminu, priorytetu i ręcznej kolejności, i układa zachłannie na dni robocze od dziś do końca widocznego tygodnia, z pojemności z K2 (zadanie bez szacunku = 30 min). Zadanie z terminem nie ląduje po terminie; gdy się nie mieści, trafia na listę „Nie zmieściło się” z powodem. Wynik pokazuje się jako podgląd (dzień → zadania, przed/po pojemności) z „Zastosuj” i „Anuluj”. „Zastosuj” zapisuje dni w jednej transakcji. Zadania zablokowane („Czekam na”) i cykliczne są pomijane.

Testy: jednostkowe mnożnika (progi, obcięcie, projekt kontra całość) i algorytmu „Rozłóż” (termin, priorytet, przeładowanie, dzień wolny, zadanie większe niż pojemność dnia). e2e: podgląd i zastosowanie.

### P2 — Poranny przegląd

- Nowy widok „Przegląd dnia” (Plan i historia → zakładka „Rano” oraz przycisk na Dashboardzie). Kroki na jednej stronie:
  1. **Niedokończone z poprzedniego dnia roboczego**: przy każdym zadaniu „Na dziś”, „Jutro”, „Do zaplanowania”, „Ukończ”; zbiorczo „Wszystkie na dziś”.
  2. **Terminy**: dziś i zaległe.
  3. **Nowe z Jiry** od ostatniego przeglądu (import z 1.1) z „Na dziś”.
  4. **Dziś**: spotkania, plan w ręcznej kolejności i pasek pojemności (z K2 i ewentualną poprawką z P1).
- „Zaczynam dzień” zapisuje dzień przeglądu (ustawienie `morningReviewDay`).
- Gdy aplikacja działa, w pierwszy dzień roboczy po godzinie początku dnia i przy braku przeglądu przychodzi powiadomienie „Przegląd dnia” (kliknięcie otwiera widok). Powiadomienie nie przychodzi drugi raz tego dnia i czeka w kolejce trybu skupienia jak inne. Wyłącznik w Ustawieniach → Powiadomienia (domyślnie włączony).

Testy: Rust — reguła powiadomienia (dzień wolny, święto, już przejrzane). e2e na danych z `seed.ts`: przeniesienie niedokończonych, „Zaczynam dzień”.

### P3 — Cykle miesięczne i „Czekam na”

Cykle:
- Do istniejących dni tygodnia dochodzą reguły w `task_details` (`repeat_rule`, stare `repeat_days` działają bez zmian): **co N tygodni** (dni tygodnia + odstęp 1–8, liczone od tygodnia `repeat_from`), **miesięcznie w dniu X** (1–31; w krótszym miesiącu ostatni dzień), **ostatni dzień roboczy miesiąca** (dni robocze i święta z ustawień), **n-ty dzień tygodnia w miesiącu** (np. 1. poniedziałek, ostatni piątek).
- Wybór reguły w rozwiniętym kafelku obok obecnych dni tygodnia. Generowanie, uzupełnianie zaległych (do 366 dni) i wstrzymanie archiwizacją jak dziś.

„Czekam na”:
- Zapis blokady zapamiętuje `waiting_since`. W Zadaniach grupa „Czekam na” (pod „W trakcie”) z „od 3 dni” przy zadaniu i przyciskiem „Odblokuj”.
- Powiadomienie po N dniach roboczych blokady (ustawienie, domyślnie 3, 0 wyłącza): „Czekasz na: klient — X (3 dni)” z przyciskami „Odblokuj” i „Przypomnij jutro”.
- Statusy Jiry oznaczające czekanie (ustawienie, lista; domyślnie „Code review”, „Review”, „Blocked”, „Waiting for customer”, „Oczekuje”): gdy zgłoszenie powiązanego zadania wejdzie w taki status, kafelek pokazuje propozycję „Jira: Code review — oznaczyć jako czekam na?”. Gdy wyjdzie, propozycję „odblokować?”. Nic nie zmienia się samo.

Testy: Rust — każda reguła cyklu na przełomach miesięcy i lat, 31. dzień w lutym, ostatni dzień roboczy ze świętem, zgodność starych `repeat_days`. e2e: grupa „Czekam na”, propozycja ze statusu Jiry (symulowana Jira jak w testach 1.1).

## Etap 3 — wgląd w pracę

### W1 — Czas agentów jako osobny tor

- Pętla agentów zapisuje okresy pracy w `agent_sessions(id INTEGER PRIMARY KEY, agent TEXT, folder TEXT, task_id INTEGER NULL, start INTEGER, end INTEGER)`. Kolejne obserwacje tego samego agenta i folderu z przerwą poniżej 2 min łączą się. Otwarty okres zamyka się po utracie aktywności albo przy zamknięciu aplikacji. `task_id` z dopasowania z 1.5; późniejsze przypisanie folderu uzupełnia okresy z ostatnich 7 dni bez zadania.
- Oś dnia: tor „Agenci” pod „Rzeczywistością”, z kolorem zadania albo szarym dla niedopasowanych.
- Rozwinięty kafelek zadania i zakładka Miesiąc: kolumna „Agent” (czas pracy agentów nad zadaniem) i „Równolegle” (część nakładająca się na Twój pomiar tego zadania). Bez wpływu na cele, raport TS i eksport.
- Wpisy starsze niż 400 dni są usuwane przy starcie. Kopia SQLite obejmuje tabelę.

Testy: Rust — łączenie okresów, zamknięcie przy utracie aktywności, uzupełnienie `task_id` po przypisaniu, czas równoległy. e2e: tor na osi i kolumna w Miesiącu (dane z `seed.ts`).

### W2 — Rozproszenie i przegląd tygodnia

- Wskaźniki dnia (wspólna funkcja w `src/report.ts`): **przełączenia** (start pomiaru innego zadania niż poprzedni pomiar w ciągu 15 min po nim), **mediana i najdłuższy nieprzerwany pomiar**, **bloki skupienia** ukończone/rozpoczęte.
- Dashboard: karta „Rozproszenie” (dziś i średnia z 7 dni roboczych).
- Plan i historia → Tydzień → zakładka **„Przegląd”**: czas kontra cel i spotkania; plan kontra wykonanie (zaplanowane na tydzień, ukończone, przeniesione dalej); 5 największych rozjazdów szacunków; czas agentów; rozproszenie dzień po dniu; pole „Wnioski” zapisywane w `week_notes(week TEXT PRIMARY KEY, text TEXT)`.
- Powiadomienie „Przegląd tygodnia” w ostatni dzień roboczy tygodnia o ustawionej godzinie (domyślnie 15:00, wyłącznik w Ustawieniach → Powiadomienia); kliknięcie otwiera zakładkę.

Testy: jednostkowe wskaźników (przełączenia przez przerwę, przez północ, ten sam task po przerwie). Rust — reguła powiadomienia (ostatni dzień roboczy ze świętem). e2e: zakładka Przegląd i zapis wniosków.

### W3 — Podpowiedzi z aktywnego okna

- Ustawienie „Podpowiadaj zadanie na podstawie aktywnego okna” (domyślnie wyłączone) w Ustawieniach → Czas pracy, z opisem, co jest czytane.
- Co 5 s tytuł i nazwa procesu okna na pierwszym planie (`GetForegroundWindow`, `GetWindowTextW`, `QueryFullProcessImageNameW` przez `extern "system"` jak w `agents.rs`, bez nowych bibliotek). Tylko w pamięci.
- Rozpoznanie: klucz Jiry w tytule (przeglądarki, Jira, Teams, Outlook) → zadanie jak w `agents::resolve`; dla VS Code, Cursor i JetBrains nazwa folderu z tytułu → folder z `agent_bindings` albo ostatnio widziany folder agenta → gałąź git → klucz → zadanie.
- Propozycja przez istniejący mechanizm propozycji z 1.5 (baner, linia w widgecie, powiadomienie przy schowanym oknie) ze źródłem „Okno”, regułą 2 minut, pamięcią „Nie teraz” i wyciszeniem. W trybie skupienia czeka w kolejce.

Testy: Rust — rozpoznanie z przykładowych tytułów (Chrome/Edge z Jirą, VS Code, IntelliJ, brak dopasowania), reguła 2 minut wspólna z agentami. e2e: baner ze źródłem „Okno” przez zastępcze IPC.

### W4 — Rozszerzenie przeglądarki (Chrome i Edge)

Tytuł okna z W3 nie zawsze ma klucz (np. tablica Jiry z otwartym zgłoszeniem `?selectedIssue=WMS-123`), a rozszerzenie widzi adres karty.

- Katalog `browser-extension/` z rozszerzeniem Manifest V3 (bez bundlera, czysty JS). Śledzi aktywną kartę (`tabs.onActivated`, `tabs.onUpdated`, `windows.onFocusChanged`) i wysyła `{url, title, at}` tylko dla domen z listy dozwolonych (opcje rozszerzenia; domyślnie `*.atlassian.net`). Inne strony nie wychodzą z przeglądarki. Stały identyfikator rozszerzenia przez pole `key` w manifeście (klucz prywatny poza repozytorium).
- Komunikacja przez **native messaging**: mały program `todo-track-bridge.exe` (drugi plik binarny w `src-tauri`, `src/bin/bridge.rs`) czyta wiadomości ze stdin i przekazuje je do działającej aplikacji przez nazwany potok `\\.\pipe\todo-track-bridge-<użytkownik>`. Gdy aplikacja nie działa, odpowiada „niepołączone” i nic nie robi. Aplikacja przy włączonym ustawieniu rejestruje hosta w `HKCU\Software\Google\Chrome\NativeMessagingHosts` i `HKCU\Software\Microsoft\Edge\NativeMessagingHosts` (manifest hosta obok exe, `allowed_origins` ze stałym identyfikatorem), a przy wyłączeniu usuwa wpisy. Bez uprawnień administratora. Instalator NSIS dołącza `todo-track-bridge.exe`.
- Rozpoznanie adresu: `/browse/KEY`, `selectedIssue=KEY`, `/issues/KEY`, klucz w tytule. Wynik trafia do tego samego mechanizmu co W3, ze źródłem „Przeglądarka” i komunikatem „Pracujesz teraz przy WMS-123? — Przełącz”. Gdy przeglądarka jest na pierwszym planie, sygnał z rozszerzenia ma pierwszeństwo przed tytułem okna. Dane z rozszerzenia są tylko w pamięci i tracą ważność po 30 s bez odświeżenia.
- Ustawienia (zakładka z S1) → Czas pracy: „Rozszerzenie przeglądarki” (wyłącznik, stan „połączone / ostatni sygnał”, link do instrukcji). README: instalacja rozszerzenia (Tryb dewelopera → „Załaduj rozpakowane” albo zasady grupy w firmie).

Testy: Rust — rozpoznanie adresów (Jira Cloud, tablica z `selectedIssue`, filtr, inne domeny odrzucone), ramka native messaging (4 bajty długości little-endian + JSON, limit 1 MB), wygasanie sygnału, rejestracja i usunięcie wpisów (na gałęzi rejestru testowej). JS — czysta funkcja filtra domen i wyciągania danych z karty, testowana w Playwright jako moduł.

## Etap 4 — Jira

### J1 — Przesunięcie zgłoszenia przy starcie pracy

Pierwszy zapis do Jiry; worklogi (odłożone) skorzystają później z tej samej ścieżki.

- Start pomiaru zadania z kluczem Jiry, każdą drogą (przycisk, szybkie przełączanie, skrót, powiadomienie, „Przełącz” z propozycji agenta, okna lub przeglądarki), gdy zgłoszenie jest w kategorii statusu „Do zrobienia” (`statusCategory.key = new`): aplikacja pyta „Przesunąć WMS-123 do „In Progress”?” z przyciskami „Przesuń”, „Nie teraz” i „Nie pytaj dla tego zadania”. Pytanie pojawia się jako baner w głównym oknie, linia w widgecie, a przy schowanym oknie jako powiadomienie z przyciskiem „Przesuń” (w trybie skupienia czeka w kolejce). Pomiar startuje od razu, bez czekania na odpowiedź. Nic nie przesuwa się samo.
- Przejście z `GET /rest/api/3/issue/{key}/transitions`: to, którego status docelowy ma kategorię `indeterminate`. Przy kilku wygrywa nazwa z ustawienia (domyślnie „In Progress”); bez dopasowania baner pokazuje listę do wyboru. Wykonanie przez `POST …/transitions`, potem odświeżenie statusu w `jira_issues` i na kafelku. Błąd (brak uprawnień, przejście wymaga ekranu z polami) pokazuje komunikat z linkiem do zgłoszenia.
- Rozwinięty kafelek zadania z kluczem: przycisk „Przesuń w Jirze…” z listą dostępnych przejść, także przy ukończeniu zadania („Ukończone. Przesunąć WMS-123 dalej?” z listą przejść, bez domyślnego wyboru).
- Ustawienia → Jira: główny przełącznik **„Pozwól aplikacji zmieniać zgłoszenia w Jirze”** (domyślnie włączony; wyłączony blokuje wszystkie zapisy do Jiry z J1 i J2 — pytanie przy starcie, przeciąganie, „Przesuń do…”, przypisanie, komentarz, tworzenie — a backend odrzuca je niezależnie od interfejsu), „Pytaj o przesunięcie zgłoszenia przy starcie pracy” (domyślnie włączone) i nazwa docelowego statusu. Lista „Nie pytaj dla tego zadania” w `task_details` (pole z `serde(default)`), z odwróceniem w kafelku.
- Odpowiednik w `store.ts` z symulowanymi przejściami.

Testy: Rust — wybór przejścia (jedno, kilka z preferowaną nazwą, brak dopasowania), brak pytania dla statusu w toku/zrobione, „Nie pytaj”, obsługa błędu 400/403, parsowanie odpowiedzi `transitions`. e2e z symulowaną Jirą: start → baner → „Przesuń” zmienia status na kafelku; „Nie pytaj” wycisza; przesunięcie przy ukończeniu.

### J2 — Jira z poziomu Todo Track

Cel: widok Jira pokazuje, co jest w jakim statusie, a dev steruje zgłoszeniami bez otwierania przeglądarki. Wszystko działa na tokenie użytkownika, więc aplikacja może tyle, na ile pozwala Jira. Korzysta z warstwy zapisu do Jiry z J1 (przejścia, odświeżenie statusu).

- **Kolumny tablicy**: `GET /rest/agile/1.0/board/{id}/configuration` (`columnConfig.columns[].statuses[].id`) i nazwy statusów z `GET /rest/api/3/project/{klucz}/statuses` (kategoria, typy zgłoszeń). Pamięć podręczna w tabeli `jira_board_columns(board_id INTEGER, position INTEGER, name TEXT, status_ids TEXT, PRIMARY KEY(board_id, position))` i `jira_statuses(site TEXT, id TEXT, name TEXT, category TEXT, PRIMARY KEY(site, id))`, odświeżana przy „Odśwież” i raz na dobę. Status spoza kolumn tablicy trafia do kolumny według kategorii.
- **Widok Jira jako tablica**: przełącznik „Lista / Tablica” (zapamiętany). Wybór tablicy (z wybranych w Ustawieniach), kolumny jak w Jirze, liczba zgłoszeń w kolumnie. Zakres „Moje” (jak dziś) albo „Cała tablica” (otwarte, limit 300 z komunikatem przy przekroczeniu). Karta: klucz, tytuł, typ i priorytet (ikony lub etykiety), osoba przypisana (inicjały), czas zmierzony lokalnie, znacznik „mierzone teraz” i „zaplanowane na …”, przyciski Start i „Otwórz w Jirze”. Zgłoszenia bez tablicy (np. z importu przypisań) w liście jak dziś.
- **Przejście przez przeciągnięcie** karty do innej kolumny (i menu „Przesuń do…” dla klawiatury): wybór przejścia, którego status docelowy należy do kolumny (przy kilku pytanie z listą), `POST …/transitions`. Optymistyczne przesunięcie z cofnięciem i komunikatem przy błędzie. Przejście wymagające ekranu z polami (np. Resolution; `GET …/transitions?expand=transitions.fields` z polami `required`) nie wykonuje się, tylko otwiera zgłoszenie w przeglądarce z wyjaśnieniem.
- **Akcje na zgłoszeniu** (menu karty i rozwinięty kafelek zadania z kluczem): „Przypisz do mnie”, „Zmień osobę…” (wyszukiwarka `GET /rest/api/3/user/assignable/search?issueKey=…&query=…`), „Dodaj komentarz” (zwykły tekst zamieniany na ADF, akapity), „Kopiuj nazwę gałęzi” (`feature/WMS-123-krotki-tytul`: małe litery, polskie znaki bez ogonków, maks. 50 znaków; prefiks w ustawieniach), „Otwórz w Jirze”.
- **Nowe zgłoszenie**: przycisk „Nowe zgłoszenie” w widoku Jira: projekt (z tablic), typ (`GET /rest/api/3/issue/createmeta/{projekt}/issuetypes`), tytuł, opis, „Przypisz do mnie” (domyślnie), „Utwórz też zadanie w Todo Track” (domyślnie, z dniem planu opcjonalnie). Wymagane pola spoza tego zestawu → komunikat i link do tworzenia w Jirze.
- Każda akcja zapisu przechodzi przez jedną funkcję w `jira.rs` (wspólną z J1) z walidacją wejścia, limitem długości i polskim komunikatem błędu dla 400/401/403/404.
- Odpowiednik wszystkich komend w `store.ts` z symulowaną Jirą (kolumny, przejścia, użytkownicy, tworzenie).

Testy: Rust — parsowanie konfiguracji tablicy i statusów projektu, przypisanie statusu do kolumny (w tym spoza kolumn), wybór przejścia do kolumny, wykrycie wymaganych pól, ADF z komentarza, nazwa gałęzi (polskie znaki, długość), błędy 400/403. Playwright z symulowaną Jirą: tablica z kolumnami, przeciągnięcie wykonuje przejście, cofnięcie przy błędzie, przypisanie do mnie, komentarz, nowe zgłoszenie tworzy zadanie. Zrzuty wzorcowe widoku Jira (lista i tablica) zaktualizowane świadomie.

## Wydanie 1.7.0 (R17)

Wersja 1.7.0 w pięciu plikach, sekcja „Nowości 1.7” w README (z instrukcją rejestracji aplikacji w Entra ID: platforma „Mobile and desktop”, przekierowanie `http://localhost`, uprawnienia delegowane `User.Read`, `Calendars.Read`, `offline_access`), `NEXT_SESSION.md`, pełna weryfikacja, build NSIS, instalator w `releases/`, lokalny tag `v1.7.0`.

## Po każdym zadaniu

Jak w 1.3: zielone `npm run build`, Playwright, `cargo fmt --check`, `cargo test --locked`; przegląd dwiema soczewkami (zgodność z planem, poprawność), uwagi w osobnym commicie. Bez podnoszenia wersji do R17.
