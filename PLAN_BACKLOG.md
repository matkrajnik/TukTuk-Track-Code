# Todo Track — plan poprawek i ulepszeń (bez przypisania do wersji)

Stan wyjściowy: 1.8.0 (wydane 20 października 2026, tag `v1.8.0`, `master` na `53caa2a`). Równolegle trwa 1.9 (Redmine, `PLAN_1.9.md`, gałąź `release-1-9`). Ten plan zbiera błędy, uwagi z użytkowania i wynik przeglądu kodu z 20 października 2026; zadania nie są przypisane do żadnego wydania i mogą wchodzić pojedynczo do 1.9.x, 1.10 albo później. Każde zadanie jest samodzielne, chyba że tabela mówi inaczej.

Źródła: zgłoszenia użytkownika z użytkowania 1.8.0, przegląd podglądu przeglądarkowego (`npm run dev`, dane z `tests/fixtures/seed.ts`), przegląd kodu `src/` i `src-tauri/src/` przez agenta z wyrywkową weryfikacją (pozycje oznaczone „zweryfikowane” sprawdzono w kodzie ręcznie), sprawdzenie hooków Claude Code 2.1.304 i Codex 0.162.1 oraz CLI Orki na tej maszynie.

## Zakres

| # | Zadanie | Rozmiar | Zależy od |
|---|---------|---------|-----------|
| B1 | Czytelność przycisków, selecty z wyszukiwaniem, drobne w formularzach | M | — |
| B2 | Układ i nawigacja: przewijanie, szerokość tablic, karta „Zagrożone”, zmiana dnia po północy, „jutro”, wyszukiwanie po kluczu | M | — |
| B3 | Pomiary: akcje dla luki w zapisie, przycięcie i wznowienie z banera bezczynności, edycja startu trwającego pomiaru | M | — |
| B4 | Wysyłka do TS: nadgodziny z automatycznym podziałem powyżej normy dnia, edycja godzin, przypisanie zadania TS z podglądu, poprawne porównanie po zapisie, wysyłka wielu dni | L | — |
| B5 | Raport dnia jak kafelki, nadgodziny dnia w raportach i na Dashboardzie, oznaczenie zgłoszeń Jiry z zadaniem, stan obserwacji Jiry | M | B4 (wspólna funkcja `overtime`) albo odwrotnie |
| B6 | Wydajność: migawka, cykle, ustawienia, przeliczenia co sekundę | L | — |
| B7 | Poprawność backendu: kopie, uszkodzony cykl, `set_priority`, brakujące testy | S | — |
| B8 | Wykrywanie agentów przez hooki Claude Code i Codexa (zamiast plików sesji) | L | — |
| B9 | Worktree Orki z kafelka zadania | M | B8 (wspólna sekcja Ustawień „Agenci”) |
| B10 | Jakość kodu: wspólne formatowanie, podział `store.ts` i dużych komponentów | L | najlepiej przed B1/B5 albo po nich, nie równolegle |
| B11 | Drobne funkcje: skróty, godziny w „Dodaj czas ręcznie”, suma dnia w widgecie, plan dnia przy pierwszym uruchomieniu | S | — |

Rozmiary jak w poprzednich planach: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni z testami.

Proponowana kolejność, gdy zadania wchodzą pojedynczo: B7 i B3 (błędy i codzienne pomiary), B4 (TS jest używany codziennie), B1 i B2 (widoczne od razu), B6, B8, B5, B11, B9, B10.

## Założenia

- Bez migracji bazy: nowe tabele przez `CREATE TABLE IF NOT EXISTS`, nowe ustawienia z wartościami domyślnymi, nowe pola `task_details` z domyślnymi.
- Timer nigdy nie przełącza się sam; żadna poprawka nie zmienia tej zasady (B3, B8).
- Nic nie trafia do TS, Jiry ani Orki bez kliknięcia.
- Każde zadanie kończy się: `npm run build`, `npm test`, `cargo test --locked`, `cargo fmt --check`, wpis w README (sekcja funkcji i „Wersja w repozytorium”), commit `B<n>: <tytuł>`.
- Zrzuty wzorcowe Playwright (`tests/visual.spec.ts`) odświeżane tylko w zadaniach zmieniających wygląd (B1, B2, B5).

## B1 — Czytelność przycisków, selecty z wyszukiwaniem, drobne w formularzach

Zgłoszenia użytkownika i obserwacje z podglądu.

1. **Przyciski są mało czytelne**: akcje w rozwiniętym kafelku (11 linków tekstowych w dwóch rzędach), „Dodaj czas ręcznie”, „Przenieś na…” nie mają tła ani ikon i nie odróżniają się od tekstu. Wprowadzić dwa poziomy: przycisk drugorzędny z tłem `var(--raised)` i ikoną `lucide` dla akcji, link tekstowy tylko dla nawigacji („Połącz konto TS w Ustawieniach”). Ujednolicić w `TaskCard`, `JournalView`, `DayClose`, `ManualTime`, `JiraView`, `SettingsPanel`.
2. **Wszystkie selecty z wyszukiwaniem**: projekt, zadanie TS, zadanie Jiry, tablica, „Wybierz zadanie” w planie dnia. Jeden komponent `SearchSelect.vue` (pole tekstowe z listą filtrowaną, klawiatura: strzałki, Enter, Escape, `aria-activedescendant`), zastępujący `<select>` tam, gdzie opcji jest więcej niż kilka; selecty z 3–5 stałymi opcjami (priorytet, cykl) zostają natywne.
3. **Pole „Cykl”** dla zadania bez cyklu pokazuje „W wybrane dni tygodnia” (wartość pusta, brak opcji „Bez cyklu”) i puste checkboxy dni. Dodać pierwszą opcję „Bez cyklu”, parametry cyklu pokazywać dopiero po wyborze (`TaskCard.vue`).
4. **„Przedziały pracy” w kafelku** powtarzają datę po obu stronach strzałki dla jednego dnia („20 paź 2026, 08:30:00 → 20 paź 2026, 10:30:00”). Dla tego samego dnia „20 paź 2026, 08:30 → 10:30”, data po prawej tylko przy przedziale przez północ, bez sekund w podglądzie (sekundy zostają w edycji).
5. **Zamknięcie dnia**: podpowiedź „Uzupełnij opis pracy z 2026-10-20” podaje datę ISO, nagłówek obok „wt., 20 paź”. Ujednolicić na format nagłówka (`DayClose.vue`).
6. **Pola liczbowe w Ustawieniach**: wyczyszczenie pola daje ogólny błąd „Niepoprawna wartość ustawienia.” (`number()` zwraca `NaN`; `SettingsPanel.vue:184-187`, `settings.rs:376-377`). Walidować w kliencie przez istniejące `applyFields` z komunikatem o zakresie pola.

Testy: Playwright dla `SearchSelect` (filtrowanie, klawiatura), dla „Bez cyklu”, dla formatu przedziałów; zrzuty wzorcowe kafelka i Ustawień.

## B2 — Układ i nawigacja

Obserwacje z podglądu (sprawdzone 20 października 2026) i przegląd kodu.

1. **Przełączenie widoku nie przewija do góry**: po przewinięciu Dashboardu Ustawienia i Jira otwierają się w połowie strony (`window.scrollY` zostaje 229–257). W `src` nie ma żadnego `scrollTo`. Dodać `window.scrollTo(0, 0)` przy zmianie widoku (`navigation.ts` / `App.vue`), z zachowaniem pozycji przy powrocie do tego samego widoku z `KeepAlive`.
2. **Tablica Jiry i tablica tygodnia nie korzystają z pełnego ekranu** (zgłoszenie użytkownika): `main { max-width: 1120px }` (`src/styles/layout.css:202-203`) obowiązuje dla wszystkich widoków, a kolumny `.jira-board` (`grid-auto-columns: minmax(220px, 1fr)`, `features.css:1996`) i 7 kolumn tygodnia rozciągają się tylko w tych 1120 px; tydzień przewija się w poziomie nawet na szerokim ekranie. Klasa `main.wide` ustawiana przez widoki tablicowe (Jira → Tablica, Plan → Tydzień → Tablica), listy i formularze zostają w 1120 px.
3. **Karta „Zagrożone teraz”** na Dashboardzie: przy szerokości okna ok. 900–1150 px (domyślne okno 1180, minimum 760) tytuł zadania łamie się po jednej literze, bo `.dashboard-count` to flex ze `strong.mono` o stałej szerokości, a `span` ma `overflow-wrap: anywhere` (`features.css:58-68`, `DashboardView.vue:307`). `flex-wrap: wrap` z tytułem nad liczbami przy braku miejsca albo przełączenie `.dashboard-columns` na jedną kolumnę poniżej ~1150 px.
4. **Dzień w „Plan i historia” i MonthReport nie przeskakują po północy**, Raporty przeskakują (`JournalView.vue:52`, `App.vue:92-94`, `MonthReport.vue:30`). Ten sam `watch(todayKey)` co w `App`.
5. **Niespójne „jutro”**: Zamknięcie dnia i Dzień przenoszą na następny dzień kalendarzowy (piątek → sobota), Rano na następny dzień roboczy (`DayClose.vue:191-192`, `JournalView.vue:185-189`, `MorningReview.vue:46`). Ujednolicić na `nextWorkDay` i pokazać datę w etykiecie.
6. **Wyszukiwanie w Zadaniach i Ctrl+K nie widzi klucza Jiry** (tylko tytuł), tablica tygodnia widzi (`TasksView.vue:155-157`, `QuickSwitch.vue:46`, `WeekBoard.vue:152`). Jedna funkcja dopasowania: tytuł, projekt, klucz.
7. **„Ukończone” / „Wszystkie”** pokazują wszystkie ukończone zadania bez ograniczenia daty (`TasksView.vue:135-159, 182`). Domyślnie ostatnie 30 dni z „Pokaż starsze”, grupowanie po dniu ukończenia.

Testy: Playwright dla przewijania, dla `main.wide`, dla zmiany dnia (zegar Playwright przez północ), dla wyszukiwania po kluczu; zrzut Dashboardu przy 1000 px.

## B3 — Pomiary: luki, bezczynność, start trwającego pomiaru

1. **Luka w zapisie bez akcji** (zgłoszenie użytkownika): między dwoma pomiarami (np. 11:58:42 → 12:02:32, 00:03:49) lista przedziałów pokazuje „Lukę w zapisie” bez możliwości jej przypisania. Trzy akcje przy luce: „Dołącz do poprzedniego” (wydłuża koniec pomiaru wyżej), „Dołącz do następnego” (cofa początek pomiaru niżej), „Podziel po pół”. Jedna komenda `fill_gap(before_id, after_id, mode)` w Rust z kontrolą nakładania jak przy edycji przedziału; w podglądzie odpowiednik w `store.ts`.
2. **Luka o zerowej długości**: po zmianie przedziału tak, że koniec jednego równa się początkowi następnego (12:02:32 → 14:58:39 i 14:58:39 → 15:02:12), lista nadal pokazuje „Lukę w zapisie” 14:58:39 → 14:58:39 o czasie 00:00:00. Luki krótsze niż 1 s nie są wyświetlane.
3. **Baner bezczynności bez „przytnij i wznów”** (`IdleBanner.vue:79-82`, `idle.rs:258-260`): „Zatrzymaj na początku przerwy” zostawia timer wyłączony, a użytkownik, który wrócił do tej samej pracy, musi szukać zadania i klikać Start. Trzecia odpowiedź: przytnij przedział do `idle_at` i wystartuj nowy od `returned_at` na tym samym zadaniu.
4. **Nie da się poprawić godziny startu trwającego pomiaru** („zacząłem o 9:00, Start o 9:20” wymaga Stop, edycji i Startu; `SessionEditor.vue:175-183`, `db.rs:257-259`). Pozwolić edytować `started_at` aktywnej sesji przy `ended_at` bez zmian, z tą samą kontrolą nakładania.
5. **Test Rust dla SQL `trim` w `idle_resolve`** (jedyna ścieżka skracająca zapisany czas; `idle.rs:258-259`), dziś niepokryty.

Testy: Rust dla `fill_gap` (trzy tryby, nakładanie, luka zerowa), dla przycięcia i wznowienia, dla edycji startu aktywnej sesji; Playwright dla akcji przy luce i trzeciej odpowiedzi banera.

## B4 — Wysyłka do TS

Zgłoszenia użytkownika z pierwszych wysyłek 1.8.0 i przegląd kodu.

1. **Pole „nadgodziny”**: w TS obok godzin jest osobne pole nadgodzin. Dodać je do każdego wpisu w podglądzie „Wyślij do TS” (format jak godziny), przekazywać w `ts_send` i uwzględniać w porównaniu po zapisie. Wymaga sprawdzenia nazwy pola w API TS (`ideo-ts-api`, symulowany TS w `store.ts`).
   - **Automatyczny podział na godziny i nadgodziny** (zgłoszenie użytkownika): czas dnia powyżej normy (8 h, czyli cel dzienny z Ustawień → Czas pracy; w dzień wolny lub święto cała praca to nadgodziny) trafia do kolumny nadgodzin. Podgląd liczy sumę dnia z pomiarów i proponuje podział: wpisy do normy w kolumnie godzin, nadwyżka w nadgodzinach, przy czym nadwyżka jest odejmowana od ostatnich chronologicznie wpisów dnia (zadanie kończące dzień ma nadgodziny). Podział jest widoczny i edytowalny przed zapisem (pkt 2 obejmuje oba pola), sumy godzin i nadgodzin osobno na dole, ostrzeżenie, gdy suma godzin przekracza normę. Wymaga sprawdzenia w prawdziwym TS, czy norma dnia jest tam stała, czy per użytkownik.
2. **Edycja godzin we wpisie**: liczba godzin w każdym wpisie edytowalna przed zapisem (pole liczbowe z dwoma miejscami, przecinek), suma na dole przeliczana; wartość z pomiarów jako domyślna z przyciskiem „Przywróć z pomiarów”.
3. **Przypisanie zadania TS z podglądu**: zadania bez zadania TS dziś blokują wpis; w podglądzie przy takim wierszu `SearchSelect` (B1) z listą zadań TS, wybór zapisuje `tsTaskId` / `tsTaskName` na zadaniu (i opcjonalnie jako domyślne dla projektu), bez wychodzenia do kafelka.
4. **Fałszywy rozjazd po zapisie** (`TsSendDialog.vue:138-142`, `timesheet.rs:994-1002`): przy „Zachowaj pozostałe wpisy TS” frontend buduje `expected` z `filled` pobranego przy otwarciu, a backend dokleja wpisy z aktualnej strony; zmiana w TS między otwarciem a zapisem daje „TS zapisał inne wartości niż wysłane”. `ts_send` zwraca listę faktycznie wysłaną i z nią porównuje.
5. **Wysyłka wielu dni**: `MultiDayReport.vue` ma listę dni, `ts_send` jest per dzień; przycisk „Wyślij dni do TS” iterujący po dniach z jednym podglądem (zakładki dni) i jednym podsumowaniem. README wymienia to jako brakujące.

Testy: Rust dla nadgodzin i porównania; Playwright na symulowanym TS dla edycji godzin, przypisania z podglądu i wysyłki wielu dni.

## B5 — Raport dnia, lista Jiry, stan obserwacji Jiry

1. **Raport dnia: lista zadań jak kafelki** (zgłoszenie użytkownika): lista w Raporcie dnia pokazuje tylko tytuł i czas; ma pokazywać to, co kafelek na liście zadań (projekt, klucz i status zgłoszenia, plan i procent, termin, notatka dnia), bez akcji Start/Ukończ. Wydzielić z `TaskCard` prezentacyjny `TaskSummary.vue` używany w obu miejscach.
2. **Lista Jiry nie odróżnia zgłoszeń z lokalnym zadaniem** (`JiraView.vue:890-925`, lista nie sprawdza `props.data.tasks` po `jiraKey`): WMS-12 z zadaniem i WMS-31 bez wyglądają tak samo, „Rozpocznij” nie mówi, czy uruchomi istniejące zadanie, czy utworzy nowe. Znacznik „W zadaniach · dziś HH:MM” i ostrzeżenie przed utworzeniem duplikatu, gdy zadanie jest w koszu albo zarchiwizowane.
3. **Nadgodziny dnia jako osobna informacja** (zgłoszenie użytkownika): gdy przepracowany czas dnia przekracza normę (cel dzienny, domyślnie 8 h; dzień wolny lub święto: cała praca), pokazać to wprost, nie tylko jako „110%”: w Raporcie dnia linia „Nadgodziny: 1:20” pod przepracowanym czasem, na Dashboardzie w karcie „Dzisiaj” i w wykresie „Czas pracy · ostatnie 7 dni” fragment słupka powyżej normy innym kolorem z sumą nadgodzin tygodnia, w Miesiącu kolumna „Nadgodziny” i suma. Jedna funkcja `overtime(day, sessions, settings)` w `report.ts`, używana też przez podział w B4.
4. **Obserwacja Jiry milczy o błędach** (zweryfikowane; `jira.rs:995-1006`): `Err` z `watch_once` (wygasły token, 401, brak sieci) jest ignorowany, statusy przestają się odświeżać bez sygnału, choć kalendarz ma `last_error`. Zapamiętać ostatni błąd i czas ostatniego udanego sprawdzenia w `JiraState`, pokazać linię „Ostatnio sprawdzono 14:05 · błąd: …” w widoku Jira i Ustawienia → Jira.

Testy: Playwright dla Raportu dnia (zrzut), dla znacznika w liście Jiry, dla linii stanu po błędzie z symulowanej Jiry.

## B6 — Wydajność

Przegląd kodu; pozycje 1–3 zweryfikowane w kodzie.

1. **Trzy migawki na akcję, `data-changed` co minutę bez zmian** (`main.rs:46-62, 408-423`, `App.vue:256`, `WidgetWindow.vue:307`, `db.rs:80-134`): `mutate` zwraca snapshot i emituje `data-changed`, na które główne okno i widget odpowiadają kolejnym `get_snapshot`; wątek cykli emituje zawsze. `recurrence::generate` zwraca `bool` i emituje tylko przy zmianie; okno, które samo wywołało komendę, pomija refresh po własnym wyniku (znacznik w zdarzeniu: etykieta okna źródłowego).
2. **`recurrence::generate` zapisuje przy każdym odczycie** (`recurrence.rs:146-150`, `main.rs:42`): upsert `recurrence_state` i `commit` także gdy `last_day` się nie zmienił. Zapis tylko gdy `last_day` rośnie; wywołanie z wątku co minutę i przy zmianie dnia, nie z `get_snapshot`.
3. **`settings::load` to ~45 osobnych `SELECT`** z serde i walidacją, wołane co 2 s (idle), co 5 s (tray), co 30 s (notify) i przy każdej akcji skupienia (`settings.rs:384-423`, `idle.rs:148-156`, `tray.rs:183`). Jeden `SELECT key, value FROM settings` i cache w `AppHandle` (`Mutex<Option<AppSettings>>`) unieważniany w `set_settings` i przy przywróceniu kopii.
4. **Lista zadań liczy O(zadania × przedziały) co sekundę**: `taskSessions(data, task.id)` w szablonie dla każdej karty przy każdym renderze, render co sekundę przez prop `now` (`TasksView.vue:524, 99-103, 152-154`). Sesje pogrupowane po `taskId` w `computed` zależnym od `data`; od `now` zależy tylko aktywny przedział.
5. **Dashboard sortuje wszystkie przedziały wielokrotnie na sekundę** (`dayDistraction`, `averageDistraction` dla 7 dni, `days`, `monthTotal`, `topTasks`; `DashboardView.vue:53-69, 80-93, 131-153`, `report.ts:328-376`). Część zamknięta zależna od `data` plus dolicznik aktywnego przedziału, albo granulacja minutowa jak przy `corrections`.
6. **„Szukaj w historii” liczy dni × zadania × przedziały co sekundę** (`LibraryView.vue:26-63`). Jedno przejście zgrupowane po (dzień, zadanie), bez zależności od `now`.
7. **Zamknięcie dnia zapisuje po jednym IPC na zadanie** z pełnym snapshotem w odpowiedzi; „Przenieś niedokończone” nadpisuje całe `details` (ryzyko cofnięcia równoległej zmiany `order` / `pinnedAt`), choć jest `planTasks` używane przez Rano (`DayClose.vue:178-198`). Komenda wsadowa `save_day_notes` i `planTasks` dla przeniesienia.

Testy: Rust dla `generate` zwracającego `bool` i braku zapisu bez zmian, dla cache ustawień; Playwright liczący wywołania `get_snapshot` po akcji (fixture `calls`).

## B7 — Poprawność backendu

1. **Przywrócenie kopii nadpisuje konfigurację TS i kalendarza** (zweryfikowane; `backup.rs:156-172`): z bieżącej bazy przenoszony jest tylko klucz `jira`; `timesheet` (login, `device_id`) i `calendar` przychodzą z kopii, więc po przywróceniu starszej kopii TS pokazuje „Nie połączono”, a inny `device_id` to dla TS nowe urządzenie. Przenosić `timesheet`, `calendar`, `jira_watch` i inne klucze połączeń tak jak `jira`; lista kluczy w jednym miejscu (`backup::KEEP_KEYS`).
2. **Jeden uszkodzony wzorzec cyklu blokuje wczytanie wszystkich danych** (`recurrence.rs:83-84`, `main.rs:42-43`): `generate` zwraca `Err("Niepoprawny początek cyklu")` dla nieparsowalnego `repeat_from`, `get_snapshot` woła go przed `snapshot`, wątek co minutę emituje `storage-error`. Pomijać taki wzorzec (`continue`) i raz zalogować.
3. **`set_priority` bez sprawdzenia istnienia zadania** (`main.rs:161-171`): dla id z kosza zwraca surowe „FOREIGN KEY constraint failed”. Przenieść do `planner` z tym samym sprawdzeniem co `plan_task` („Nie znaleziono zadania.”).
4. Testy Rust dla wszystkich trzech ścieżek.

## B8 — Wykrywanie agentów przez hooki

Zgłoszenie użytkownika: wykrywanie pracujących agentów (kafelek Agenci AI, aktywność przy bezczynności) zawodzi, zwłaszcza przy zadaniach w tle. Ustalona przyczyna (20 października 2026): plik sesji ma `status: busy` poprawnie, ale po 15 min (`STALL`) aplikacja wymaga świeżego zapisu `~/.claude/projects/<projekt>/<sessionId>.jsonl`, a przy workflow i subagentach główny transkrypt stoi i bieżące zapisy idą do `<sessionId>/subagents/**/agent-*.jsonl`. Decyzja użytkownika: zamiast łatać skan plików, przejść na hooki i stary mechanizm usunąć w całości.

Sprawdzone na tej maszynie (Claude Code 2.1.304, Codex 0.162.1):
- Claude: hooki typu `command` w `~/.claude/settings.json` dostają na stdin JSON z `session_id`, `cwd`, `transcript_path`, `hook_event_name`, `permission_mode`, w subagencie też `agent_id` i `agent_type`; hooki z pliku ustawień działają także w subagentach; `Notification` ma `notification_type` (`permission_prompt`, `idle_prompt`, `agent_needs_input`…), `SessionEnd` ma `reason`; zmiany w `settings.json` są podchwytywane bez restartu sesji; typ `http` istnieje, ale Codex go nie ma.
- Codex: pełne hooki w `~/.codex/hooks.json` (SessionStart, UserPromptSubmit, PreToolUse, PermissionRequest, PostToolUse, SubagentStart, SubagentStop, Stop, Interrupt, SessionEnd), te same pola plus `turn_id`, tylko typ `command`. Każdy nowy lub zmieniony hook musi być zaufany (`[hooks.state]` w `config.toml` z `trusted_hash`, zatwierdzenie przez `/hooks` w Codexie), więc aplikacja nie zainstaluje go po cichu.
- Orca ma już własne wpisy w obu plikach (13 zdarzeń w Claude, 9 w Codexie), więc rejestracja musi scalać JSON i usuwać tylko własne wpisy.
- Orca uruchamia zwykłe procesy `claude` / `codex` w PTY, więc sesje z Orki i z terminala obsługuje ten sam mechanizm.

Architektura:
- `todo-track-hook.exe` (mały Rust bez zależności, obok `todo-track-bridge.exe` w instalatorze): krótkotrwały program uruchamiany przez Claude / Codex przy każdym zdarzeniu, czyta JSON ze stdin, pisze do nazwanego potoku `\\.\pipe\todo-track-hooks`, kończy się; gdy aplikacja nie działa, kończy się cicho bez błędu. Bez osobnego procesu nasłuchującego: nasłuchuje główny proces Todo Track (wątek z potokiem, jak dziś potok rozszerzenia w `browser.rs`).
- Zdarzenia zasilają `agent_sessions` i stan kafelka Agenci AI: `UserPromptSubmit` / `PreToolUse` / `PostToolUse` = praca, `Notification(permission_prompt, idle_prompt)` i `PermissionRequest` = czekanie na użytkownika (nie liczy się jako praca), `Stop` / `SessionEnd` / `Interrupt` = koniec; `agent_id` odróżnia subagentów (liczeni do sesji nadrzędnej). Koniec bez zdarzenia (ubity proces) domykany timeoutem braku zdarzeń (np. 10 min).
- Przy starcie aplikacji: (1) sprawdzić wpisy Todo Track w obu plikach i czy wskazują aktualną ścieżkę exe (po aktualizacji instalatorem), brakujące lub stare dopisać, ponawiać co minutę jak rejestrację rozszerzenia; (2) odczytać z `config.toml` sekcję `hooks.state` i gdy hook Todo Track nie jest zaufany, pokazać w Ustawieniach informację o zatwierdzeniu przez `/hooks`; (3) stanu agentów nie skanować, zacząć od pustej listy i czekać na zdarzenia (`PreToolUse` przychodzi co kilka sekund, więc luka po restarcie jest krótka).
- Do usunięcia: skan `~/.claude/sessions/<pid>.json`, rolloutów Codexa i transkryptów, `process_alive`, progi `STALL` / `CODEX_FRESH` w `agents.rs` wraz z testami. Rozpoznanie zadania agenta (gałąź git z `HEAD`, klucz Jiry z gałęzi i nazwy sesji, przypisania folderów) zostaje bez zmian i dostaje `cwd` z hooka.
- Ustawienia → Czas pracy (albo nowa sekcja „Agenci”): przełącznik „Hooki agentów (Claude Code, Codex)” rejestrujący i usuwający wpisy, stan zaufania hooka Codexa, lista ostatnich zdarzeń do diagnostyki.

Testy: Rust dla parsera zdarzeń, maszyny stanów sesji, scalania JSON ustawień (z obcymi wpisami Orki) i odczytu `hooks.state`; Playwright dla kafelka na zdarzeniach z fixture; ręcznie: sesja z workflow ponad 15 min, sesja czekająca na zgodę, Codex przed i po zatwierdzeniu.

## B9 — Worktree Orki z kafelka zadania

Pytanie użytkownika, sprawdzone 20 października 2026. Orca ma CLI `%LOCALAPPDATA%\Programs\orca\resources\bin\orca.cmd` (nie w PATH) z wyjściem `--json`, rozmawiające z lokalnym runtime Orki (musi działać; `orca open --json` ją uruchamia, `orca status --json` sprawdza).

- `orca worktree create --json --name <nazwa> --repo path:<repo> [--base-branch <ref>] [--reference <klucz Jiry|URL>] [--agent claude|codex --prompt <tekst> --model <id> --effort <poziom>] [--activate]` tworzy worktree, podpina zgłoszenie i startuje agenta z promptem.
- `orca worktree list --json` (lista z gałęziami i referencjami), `orca reference find --json <klucz>` (worktree po kluczu Jiry), `orca agent launch`, `orca terminal send/read`, `orca agent-context` (schemat komend dla maszyn).

Zakres:
1. Akcja „Otwórz worktree w Orce” w rozwiniętym kafelku zadania ze zgłoszeniem (Jira, po 1.9 też Redmine): nazwa z klucza i tytułu, `--reference KEY`, repozytorium z przypisania folderu (`agent_bindings`) albo z listy repozytoriów Orki do wyboru, opcjonalny prompt z tytułu i notatki dnia, wybór agenta. Dialog z podglądem polecenia, nic nie startuje bez kliknięcia.
2. Mapowanie folder → klucz z `orca worktree list` jako dodatkowe źródło dopasowania agenta do zadania (dziś: przypisanie folderu, gałąź git, nazwa sesji).
3. Ustawienia → Agenci: ścieżka do `orca.cmd` (wykrywana domyślnie), „Uruchamiaj Orkę, gdy nie działa” (domyślnie wyłączone; gdy wyłączone, komunikat „Orca nie działa”).

Testy: Rust dla budowy polecenia i parsowania JSON z fixture; Playwright dla dialogu w podglądzie.

## B10 — Jakość kodu

1. **Powielone formatowanie** HH:MM, klucza dnia i liczby mnogiej w kilkunastu miejscach (`DayTimeline.vue:114`, `DayClose.vue:74`, `JournalView.vue:221`, `MorningReview.vue:72`, `TasksView.vue:105, 393`, `WidgetWindow.vue:175`, `useTsSend.ts:55`, `export.ts:83`, `spread.ts:68`, `JiraView.vue:210`, `FocusBanner.vue:21`; `store.ts:1042` `localDay` = `dayKey` z `time.ts`; cztery odmiany liczby mnogiej w `WidgetWindow.vue:106`, `TimesheetSettings.vue:54`, `MonthReport.vue:93`, `timesheet.ts:217`). Wspólne `clock()` / `plural()` w `time.ts`, w `store.ts` import `dayKey`.
2. **`store.ts` (3458 linii) miesza API aplikacji z symulatorami podglądu** (symulowana tablica Jiry 1977–3013, symulowany TS 3015–3458, kalendarz 1397–1575). Przenieść do `src/preview/*.ts` za tymi samymi funkcjami; `store.ts` zostaje cienką warstwą `desktop ? invoke : preview`.
3. **Bardzo duże komponenty**: `JiraView.vue` (1136 linii: połączenie, wybór tablic, nowe zgłoszenie, lista, tablica), `SettingsPanel.vue` (918, 10 sekcji), `TaskCard.vue` (588). Każda sekcja lub formularz jako osobny komponent z własnym `busy` / `error`.

Bez zmian zachowania; wszystkie istniejące testy przechodzą bez modyfikacji poza importami.

## B11 — Drobne funkcje

1. **Skróty dla najczęstszych akcji**: dziś tylko Ctrl+K i Ctrl+przecinek (`App.vue:221-229`); Ctrl+Shift+S = Start/Stop (logika `toggle_measurement`), `N` = fokus w „Nowe zadanie”, lista w Ustawienia → Skróty.
2. **„Dodaj czas ręcznie” z sensownymi godzinami**: `ManualTime.vue:25-26` zawsze proponuje 09:00–10:00; początek z `dayStart` albo koniec ostatniego przedziału dnia, koniec = teraz dla dzisiaj.
3. **Suma dnia w widgecie**: tray pokazuje „dziś 5:10 / 8:00”, widget w podlinii tylko projekt albo „Pomiar trwa” (`WidgetWindow.vue:203-215`); dane są już w oknie.
4. **Plan dnia przy pierwszym uruchomieniu w danym dniu** (zgłoszenie użytkownika): dziś Poranny przegląd (Plan i historia → Rano) otwiera się tylko ręcznie albo z powiadomienia „Przegląd dnia”. Przy pierwszym starcie aplikacji w dniu roboczym, gdy przegląd dnia nie został jeszcze zapisany („Zaczynam dzień”), główne okno otwiera się od razu na zakładce Rano zamiast na liście zadań; to samo, gdy aplikacja działała przez noc i po północy okno jest pokazywane pierwszy raz tego dnia. Nie w święto ani w całodniowy urlop z kalendarza (te same reguły co powiadomienie), nie przy starcie zminimalizowanym do traya (wtedy przy pierwszym pokazaniu okna). Po „Zaczynam dzień” albo ręcznym przejściu gdzie indziej aplikacja nie wraca do Rano tego dnia. Przełącznik „Otwieraj plan dnia przy pierwszym uruchomieniu” w Ustawieniach → Uruchamianie (domyślnie włączony), stan „pokazano dziś” w pamięci okna, nie w bazie. W podglądzie przeglądarkowym działa tak samo (`localStorage`). Test Playwright: start z zapisanym przeglądem i bez, zmiana dnia zegarem.

## Po każdym zadaniu

`npm run build`, `npm test`, `cargo test --locked`, `cargo fmt --check`; README (sekcja funkcji, „Wersja w repozytorium”); `notes.md` tylko przy wydaniu; commit `B<n>: <tytuł>`. Zadania nie podnoszą wersji; wydanie składa się z tych zadań, które weszły, i dostaje własny wpis w `RELEASE.md`.

## Do ręcznego sprawdzenia (po instalacji, przez użytkownika)

- B4: nazwa pola nadgodzin w prawdziwym TS, czy norma dnia jest tam stała (8 h) czy per użytkownik, i czy porównanie po zapisie uwzględnia nadgodziny.
- B8: sesja Claude z workflow ponad 15 minut widoczna w kafelku; sesja czekająca na zgodę nie liczy się jako praca; Codex przed i po zatwierdzeniu hooka przez `/hooks`; sesja uruchomiona z Orki; restart aplikacji w trakcie pracy agenta.
- B9: tworzenie worktree przy działającej i niedziałającej Orce.
