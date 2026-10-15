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

Odłożone: worklogi do Jiry (7) — do osobnego zaplanowania. Panel PM: osobna koncepcja w `PANEL_PM.md`.

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
- Ustawienia → Czas pracy: „Rozszerzenie przeglądarki” (wyłącznik, stan „połączone / ostatni sygnał”, link do instrukcji). README: instalacja rozszerzenia (Tryb dewelopera → „Załaduj rozpakowane” albo zasady grupy w firmie).

Testy: Rust — rozpoznanie adresów (Jira Cloud, tablica z `selectedIssue`, filtr, inne domeny odrzucone), ramka native messaging (4 bajty długości little-endian + JSON, limit 1 MB), wygasanie sygnału, rejestracja i usunięcie wpisów (na gałęzi rejestru testowej). JS — czysta funkcja filtra domen i wyciągania danych z karty, testowana w Playwright jako moduł.

## Wydanie 1.7.0 (R17)

Wersja 1.7.0 w pięciu plikach, sekcja „Nowości 1.7” w README (z instrukcją rejestracji aplikacji w Entra ID: platforma „Mobile and desktop”, przekierowanie `http://localhost`, uprawnienia delegowane `User.Read`, `Calendars.Read`, `offline_access`), `NEXT_SESSION.md`, pełna weryfikacja, build NSIS, instalator w `releases/`, lokalny tag `v1.7.0`.

## Po każdym zadaniu

Jak w 1.3: zielone `npm run build`, Playwright, `cargo fmt --check`, `cargo test --locked`; przegląd dwiema soczewkami (zgodność z planem, poprawność), uwagi w osobnym commicie. Bez podnoszenia wersji do R17.
