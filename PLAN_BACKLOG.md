# Todo Track — plan poprawek, ulepszeń i integracji z agentami AI

Stan wyjściowy: 1.8.0 (wydane 20 października 2026, tag `v1.8.0`), 1.9 (Redmine, `PLAN_1.9.md`) scalone do `master` i czeka na `npm run ship -- 1.9.0`. Ten plan łączy dwa wcześniejsze dokumenty z 20 października 2026:
- **część B** (B1–B11): błędy, uwagi z użytkowania i wynik przeglądu kodu (dawny `PLAN_BACKLOG.md`);
- **część A** (A0–A6): integracja z agentami AI — serwer MCP, czat, asystent przypisań (dawny `PLAN_AI.md`, commit `e778ff6`).

Realizacja: workflow `.claude/workflows/backlog.js` (zadanie po zadaniu: implementacja, weryfikacja, przegląd, poprawki, commit) na gałęzi `backlog` w worktree `.claude/worktrees/backlog`, w kolejności z sekcji „Kolejność realizacji”. Zadania można też brać pojedynczo (`only`).

Źródła części B: zgłoszenia użytkownika z użytkowania 1.8.0, przegląd podglądu przeglądarkowego (`npm run dev`, dane z `tests/fixtures/seed.ts`), przegląd kodu `src/` i `src-tauri/src/` przez agenta z wyrywkową weryfikacją (pozycje oznaczone „zweryfikowane” sprawdzono w kodzie ręcznie), sprawdzenie hooków Claude Code 2.1.304 i Codex 0.162.1 oraz CLI Orki na tej maszynie. Części A: rozmowa z 20 października 2026 (decyzje niżej).

## Zakres

| # | Zadanie | Rozmiar | Zależy od |
|---|---------|---------|-----------|
| B1 | Czytelność przycisków, selecty z wyszukiwaniem, drobne w formularzach | M | — |
| B2 | Układ i nawigacja: przewijanie, szerokość tablic, karta „Zagrożone”, zmiana dnia po północy, „jutro”, wyszukiwanie po kluczu | M | — |
| B3 | Pomiary: akcje dla luki w zapisie, przycięcie i wznowienie z banera bezczynności, edycja startu trwającego pomiaru | M | — |
| B4 | Wysyłka do TS: nadgodziny z automatycznym podziałem powyżej normy dnia, edycja godzin, przypisanie zadania TS z podglądu, poprawne porównanie po zapisie, wysyłka wielu dni | L | B1 (`SearchSelect`) |
| B5 | Raport dnia jak kafelki, nadgodziny dnia w raportach i na Dashboardzie, oznaczenie zgłoszeń Jiry z zadaniem, stan obserwacji Jiry | M | B4 (wspólna funkcja `overtime`) |
| B6 | Wydajność: migawka, cykle, ustawienia, przeliczenia co sekundę | L | — |
| B7 | Poprawność backendu: kopie, uszkodzony cykl, `set_priority`, brakujące testy | S | — |
| B8 | Wykrywanie agentów przez hooki Claude Code i Codexa (zamiast plików sesji) | L | — |
| B9 | Worktree Orki z kafelka zadania | M | B8 (wspólna sekcja Ustawień „Agenci”) |
| B10 | Jakość kodu: wspólne formatowanie, podział `store.ts` i dużych komponentów | L | nie równolegle z innymi zadaniami |
| B11 | Drobne funkcje: skróty, godziny w „Dodaj czas ręcznie”, suma dnia w widgecie, plan dnia przy pierwszym uruchomieniu | S | — |
| A0 | Filtr danych osobowych (pseudonimizacja wszystkiego, co wychodzi do modelu) | M | — |
| A1 | Silnik AI: wykrycie CLI, wybór agenta przy pierwszym uruchomieniu, izolowane wywołania, Ustawienia → Asystent AI | M | A0, B8 (filtr `TODO_TRACK_AI` w `todo-track-hook.exe`) |
| A2 | Serwer MCP `todo-track-mcp.exe` i rejestracja w Claude Code i Codexie | L | A0 |
| A3 | Czat w aplikacji (panel boczny, karty akcji) | L | A1, A2, B10 (`src/preview/`) |
| A4 | Asystent przypisań: nieprzypisane sesje agentów, zadanie TS dla zadania | M | A1, B8, B4 (podgląd „Wyślij do TS”) |
| A5 | Zamknięcie dnia z asystentem: luki, czas nieprzypisany, opisy TS | L | A1, A4, B3 (luki) |
| A6 | Rano i przegląd tygodnia: komentarz asystenta (opcjonalnie) | S | A1 |

Rozmiary: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni z testami.

## Kolejność realizacji

Jeden implementator naraz w jednym drzewie roboczym (zadania dotykają `store.ts`, `main.rs`, `settings.rs`, `SettingsPanel.vue`, `TaskCard.vue`), więc kolejność jest ścisła:

1. **B7** — małe błędy backendu, bez zmian w interfejsie.
2. **B10** — refaktor przed resztą: wspólne `clock()` / `plural()`, symulatory podglądu w `src/preview/*.ts`, podział `JiraView` / `SettingsPanel` / `TaskCard`. Kolejne zadania zmieniają wtedy mniejsze pliki, a A3 od razu kładzie symulator w `src/preview/ai.ts`.
3. **B1** — `SearchSelect` i czytelne przyciski (potrzebne w B4).
4. **B3**, **B4**, **B5** — pomiary, wysyłka do TS z nadgodzinami, raporty z tą samą funkcją `overtime`.
5. **B2**, **B6**, **B11** — układ, wydajność, drobne.
6. **B8**, **B9** — hooki agentów i Orka.
7. **R110** — przygotowanie wydania 1.10.0 (część B).
8. **A0**, **A1**, **A2** — filtr osobowy, silnik, serwer MCP.
9. **A3**, **A4**, **A5**, **A6** — czat i funkcje asystenta.
10. **R111** — przygotowanie wydania 1.11.0 (część A).

Podział na wydania jest propozycją: część B nie wymaga AI i może wyjść wcześniej. Workflow zatrzymuje się na pierwszej blokadzie; `until: "R110"` kończy po części B.

## Założenia

- Bez migracji bazy: nowe tabele przez `CREATE TABLE IF NOT EXISTS`, nowe ustawienia z wartościami domyślnymi, nowe pola `task_details` z domyślnymi.
- Timer nigdy nie przełącza się sam; żadna poprawka nie zmienia tej zasady (B3, B8, A4).
- Nic nie trafia do TS, Jiry, Redmine ani Orki bez kliknięcia; model AI tylko proponuje (część A).
- Testy nie łączą się z żadnym serwerem (TS, Jira, Redmine) i nie uruchamiają prawdziwego `claude`, `codex` ani `orca`; nie zmieniają plików użytkownika `~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.codex/config.toml` ani rejestracji MCP (testy pracują na kopiach w katalogu tymczasowym).
- Każde zadanie kończy się: `npm run build`, `npm test`, `cargo test --locked`, `cargo fmt --check`, commit `B<n>: <tytuł>` albo `A<n>: <tytuł>`. README i `NEXT_SESSION.md` aktualizuje zadanie przygotowania wydania (R110, R111) na podstawie opisów z zadań.
- Zrzuty wzorcowe Playwright (`tests/visual.spec.ts`) odświeżane tylko w zadaniach zmieniających wygląd (B1, B2, B5, A1, A3), świadomie i z opisem.

# Część B — poprawki i ulepszenia

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

# Część A — integracja z agentami AI (Claude Code, Codex)

Temat: **agenci AI pomagają w trackowaniu, a nie tylko są wykrywani**. Dziś aplikacja wie, że Claude Code albo Codex pracuje (`agents.rs`, a po B8 hooki) i dopasowuje sesję do zadania heurystyką (`agents::resolve`: przypisanie folderu, gałąź git, klucz Jiry w gałęzi lub nazwie sesji). Po tej części:
- agent może rozmawiać z aplikacją przez **serwer MCP Todo Track**: czytać zadania i dzień, dopisywać notatki, proponować przypisania;
- aplikacja ma **czat** w panelu bocznym, oparty na tym samym MCP;
- gdy heurystyka zawodzi, aplikacja **pyta model** o dopasowanie: sesja agenta → zadanie, zadanie → zadanie TS, luka w dniu → zadanie, opis wpisu TS.

## Decyzje użytkownika dla części A (20 października 2026)

- **Silnik = narzędzia CLI z subskrypcji** (~90% użycia): `claude` (Claude Code) i `codex` (Codex CLI) uruchamiane przez aplikację, logowanie i limity z subskrypcji użytkownika. Bez klucza API; klucz API najwyżej jako późniejszy zapas, poza tym planem.
- **Dane zadań i Jiry wolno wysyłać do modelu, ale bez imion i nazwisk.** Dotyczy każdego wyjścia do modelu: wywołań aplikacji, czatu i odpowiedzi narzędzi MCP (to, co MCP zwraca sesji Claude, też trafia do modelu).
- **Domyślny agent wybiera użytkownik przy pierwszym uruchomieniu** (Claude Code / Codex / nie używaj) i wybór jest zapisany w bazie (`settings`). Zmiana w Ustawieniach → Asystent AI.

## Założenia części A (przyjęte bez pytania)

- **Model tylko proponuje.** Timer, TS i Jira zmieniają się wyłącznie po kliknięciu, jak dotąd. Narzędzia MCP i czat nie mają dostępu do wysyłki TS ani zapisów do Jiry. Zapisy lokalne z czatu i MCP przychodzą jako **karty akcji** („Zastosuj” / „Odrzuć”); bez kliknięcia nic się nie zapisuje.
- **Heurystyka przed modelem.** Model jest wołany tylko wtedy, gdy deterministyczne dopasowanie nic nie dało. Każda zaakceptowana propozycja zapisuje regułę (`agent_bindings`, `ts_project_map`), więc z czasem wywołań jest coraz mniej.
- **Wywołania tylko na zdarzenie**, nigdy w pętli co sekundę: nowa nieprzypisana sesja, rozwinięcie pola „Zadanie TS”, otwarcie Zamknięcia dnia, wiadomość w czacie. Wynik w pamięci podręcznej na czas działania aplikacji (klucz: rodzaj wywołania + skrót danych wejściowych).
- **Domyślnie wyłączone.** Bez wyboru agenta (A1) żadna funkcja AI nie działa, a serwer MCP nie jest rejestrowany.
- **Wspólny limit subskrypcji z pracą w Claude Code / Codexie.** Dwa poziomy modelu w ustawieniach: „szybki” (dopasowania, pytania o dane) i „główny” (Zamknięcie dnia, dłuższe rozmowy); puste = domyślny model CLI.
- **Tytuły okien (W3) i tytuły spotkań domyślnie nie wychodzą do modelu w ogóle**, nie tylko po filtrze; osobne przełączniki, domyślnie wyłączone.
- Nowe ustawienia w `AppSettings` (`settings.rs`) z wartościami domyślnymi; nowe tabele tylko przez `CREATE TABLE IF NOT EXISTS`. Historia czatu tylko w pamięci (zapis w SQLite odłożony).
- Żadne zadanie podczas pracy nad planem nie wywołuje prawdziwego modelu: testy Rust działają na zapisanych odpowiedziach JSON, Playwright na symulowanym asystencie w podglądzie.

Sprawdzone na tej maszynie (20 października 2026, Claude Code 2.1.304, Codex 0.162.1):
- `claude -p` ma `--output-format json|stream-json`, `--input-format stream-json`, `--json-schema`, `--model`, `--tools` (`""` wyłącza wbudowane narzędzia), `--mcp-config`, `--strict-mcp-config`, `--allowedTools`, `--permission-prompts none`, `--setting-sources`, `--append-system-prompt`, `--no-session-persistence`, `--resume`.
- `--bare` pomija hooki i CLAUDE.md, ale **wymaga `ANTHROPIC_API_KEY`**, więc przy subskrypcji odpada; izolacja przez `--setting-sources`, pusty katalog roboczy i znacznik w środowisku (A1).
- `codex exec` ma `--json` (JSONL zdarzeń), `--output-schema <plik>`, `-o/--output-last-message`, `--ephemeral`, `--skip-git-repo-check`, `-C/--cd`, `-s/--sandbox`, `-m/--model`, `-c klucz=wartość`; `codex mcp` zarządza serwerami MCP; `codex app-server` jest eksperymentalny.
- `claude` nie jest w `PATH` powłoki Git Bash (leży w `~/.local/bin/claude.exe`), więc aplikacja nie może polegać na `PATH`.

## A0 — Filtr danych osobowych (`src-tauri/src/ai_privacy.rs`)

Jedna funkcja, przez którą przechodzi każdy tekst i JSON wysyłany do modelu: `redact(input, &Dictionary) -> (String, Pseudonyms)`. Używają jej A1 (wywołania aplikacji, czat) i A2 (odpowiedzi MCP).

- **Pola osobowe podmieniane w całości** (po kształcie danych, nie po treści): przypisany i zgłaszający Jiry, pola Jiry typu osoba z `jira_issue_fields` (`jira_fields.kind`), uczestnicy i organizator spotkań (gdy kiedyś będą przechowywani), autorzy commitów z `git log`. Wartość → stały pseudonim w obrębie wywołania („Osoba 1”, „Osoba 2”…); ta sama osoba w jednym wywołaniu ma zawsze ten sam pseudonim.
- **Słownik dla zwykłego tekstu** (tytuły zadań, notatki dnia, opisy, wiadomości czatu, prompty agentów): nazwiska i imiona nie są wykrywane heurystyką, tylko słownikiem:
  - automatycznie: nazwy wyświetlane osób z Jiry (przypisani, zgłaszający, pola osobowe), rozbite na imię i nazwisko; własne imię i nazwisko użytkownika z Jiry;
  - ręcznie: Ustawienia → Asystent AI → „Ukrywaj te nazwy” (`ai_hidden_names`), jedna nazwa w wierszu;
  - dopasowanie bez rozróżniania wielkości liter, po granicy słowa, z polską odmianą przez rdzeń (Kowalski / Kowalskiego / Kowalskim / Kowalska; Anna / Anny / Annie / Anią); imiona krótsze niż 3 znaki pomijane.
- **Odwracanie**: mapa pseudonim → osoba tylko w pamięci, w obrębie wywołania lub rozmowy czatu. Odpowiedź modelu przechodzi przez `restore`, zanim trafi do UI (karta akcji pokazuje prawdziwe nazwisko). Do modelu mapa nigdy nie trafia.
- **„Pokaż, co zostanie wysłane”**: każde wywołanie z UI ma podgląd tekstu **po** filtrze (A1, A3).
- Ograniczenie zapisane w README i w Ustawieniach: nazwisko spoza słownika wpisane ręcznie w tytuł albo notatkę przejdzie.

Testy Rust: podmiana pól osobowych w JSON (zagnieżdżenia, listy), stałość pseudonimów w wywołaniu, słownik z odmianą (fixture z 20 formami), nazwisko będące zwykłym słowem (np. „Marzec”) świadomie podmieniane wszędzie (test dokumentuje), brak trafień w słowach tylko zaczynających się od nazwy (np. „Annapurna” przy „Anna”), `restore` po odpowiedzi modelu, test „szczelności”: fixture z dniem pełnym danych i asercja, że żadna nazwa ze słownika nie występuje w wyjściu.

## A1 — Silnik AI i wybór agenta (`src-tauri/src/ai.rs`)

**Wykrycie CLI** przy starcie i w Ustawieniach („Sprawdź ponownie”):
- `claude`: `%USERPROFILE%\.local\bin\claude.exe`, potem `PATH`, potem ścieżka z ustawień (`ai_claude_path`); wersja z `--version`.
- `codex`: `PATH`, potem znane miejsca instalacji (npm global, `%LOCALAPPDATA%\Programs\OpenAI\…`), potem `ai_codex_path`; wersja z `--version`.
- Wynik `{ claude: { path, version } | null, codex: … }` w pamięci; brak obu = funkcje AI niedostępne z komunikatem w Ustawieniach.

**Wybór przy pierwszym uruchomieniu**:
- Nowe ustawienie `ai_engine: String` (`""` = jeszcze nie pytano, `"claude"`, `"codex"`, `"off"`), domyślnie `""`.
- Przy starcie, gdy `ai_engine == ""` i wykryto co najmniej jedno CLI: dialog „Asystent AI” z krótkim opisem (co robi, że korzysta z subskrypcji, że nazwiska są ukrywane, że nic nie zmienia się bez kliknięcia) i trzema przyciskami: „Claude Code”, „Codex” (tylko wykryte), „Nie używaj”. Wybór zapisany w `settings`; dialog nie wraca. „Później” (zamknięcie dialogu) nie zapisuje nic i pyta przy następnym starcie.
- Gdy nie wykryto żadnego CLI: dialogu nie ma, `ai_engine` zostaje `""`; Ustawienia pokazują „Nie znaleziono Claude Code ani Codexa” z linkami do instalacji.
- Zmiana w Ustawieniach → Asystent AI (lista: Claude Code / Codex / wyłączony).

**Wywołanie jednorazowe** `ai::ask(kind, prompt, schema) -> Result<Value, String>`:
- Claude: `claude -p --output-format json --json-schema <schemat> --tools "" --strict-mcp-config --mcp-config <pusty> --setting-sources "" --permission-prompts none --no-session-persistence --model <ai_model_fast|main>`; prompt na stdin.
- Codex: `codex exec --json --output-schema <plik> --ephemeral --skip-git-repo-check -s read-only -C <katalog AI> -m <model>`; wynik z ostatniej wiadomości (`-o <plik>`).
- **Izolacja**: katalog roboczy `%APPDATA%\pl.todotrack.desktop\ai\` (pusty, bez CLAUDE.md i `.git`), zmienna `TODO_TRACK_AI=1` w środowisku procesu; `todo-track-hook.exe` (B8) dziedziczy środowisko po `claude`/`codex` i odrzuca zdarzenia z tym znacznikiem, a `agents.rs` dodatkowo ignoruje `cwd` równe katalogowi AI. Dzięki temu czat i podpowiedzi nie pokazują się w kafelku „Agenci AI” ani w torze agentów.
- Limit czasu 60 s (czat bez limitu na całą odpowiedź, ale z „Zatrzymaj”), jeden proces jednorazowy naraz na rodzaj wywołania; okno konsoli schowane (`CREATE_NO_WINDOW`).
- Błędy po polsku w jednym miejscu: brak CLI, niezalogowany („Zaloguj się: uruchom `claude` w terminalu”), limit subskrypcji, przekroczony czas, odpowiedź niezgodna ze schematem.
- Dane wejściowe zawsze po `ai_privacy::redact`, odpowiedź po `restore`.

**Ustawienia → Asystent AI** (nowa sekcja w `SettingsPanel.vue`, jako osobny komponent):
- silnik (Claude Code / Codex / wyłączony) i wykryte wersje, „Sprawdź ponownie”, ścieżki ręczne;
- model szybki i główny (pole tekstowe, puste = domyślny);
- „Ukrywaj te nazwy” (A0), przełączniki „Wysyłaj tytuły spotkań” i „Wysyłaj tytuły okien” (domyślnie wyłączone);
- przełączniki funkcji: „Serwer MCP dla agentów” (A2), „Czat” (A3), „Podpowiedzi przypisań” (A4), „Asystent w Zamknięciu dnia” (A5);
- „Ostatnie wywołania” (rodzaj, czas, długość, wynik ok/błąd, bez treści) do diagnostyki.

Nowe pola `AppSettings`: `ai_engine`, `ai_claude_path`, `ai_codex_path`, `ai_model_fast`, `ai_model_main`, `ai_hidden_names`, `ai_send_meeting_titles`, `ai_send_window_titles`, `ai_mcp`, `ai_chat`, `ai_assign_suggest`, `ai_day_close`.

Testy Rust: budowa linii poleceń dla obu silników (fixture), parsowanie odpowiedzi `claude -p --output-format json` i JSONL `codex exec --json` z zapisanych plików, walidacja ze schematem, mapowanie błędów (niezalogowany, limit), odrzucenie zdarzeń hooka ze znacznikiem `TODO_TRACK_AI`, logika dialogu pierwszego uruchomienia (`""` + wykryte CLI → pytaj; wybór zapisany → nie pytaj; brak CLI → nie pytaj). Playwright: dialog w podglądzie (symulowane wykrycie), sekcja Ustawień.

## A2 — Serwer MCP (`src-tauri/src/bin/todo-track-mcp.rs`, `src-tauri/src/mcp.rs`)

- `todo-track-mcp.exe`: mały program stdio (JSON-RPC MCP: `initialize`, `tools/list`, `tools/call`), bez własnej logiki; każde wywołanie narzędzia przekazuje nazwanym potokiem `\\.\pipe\todo-track-mcp-<użytkownik>` do głównego procesu (wzór: `todo-track-bridge.exe` i `native_msg`). Aplikacja nie działa → narzędzie zwraca błąd „Todo Track nie działa”. Dołączony do instalatora obok `todo-track-bridge.exe`.
- Główny proces (`mcp.rs`, wątek z potokiem jak `browser.rs`) wykonuje narzędzie i **przepuszcza wynik przez `ai_privacy::redact`** przed odesłaniem.
- **Narzędzia odczytu** (wykonywane od razu):
  - `tasks_list(filter: dzisiaj|otwarte|zablokowane|projekt|szukaj)`, `task_get(id | klucz)`;
  - `day_summary(dzień)`: pomiary, suma, cel, spotkania (bez tytułów, gdy wyłączone), czas agentów, notatki dnia;
  - `week_summary(tydzień)`, `current_measurement`, `ts_tasks` (lista zadań TS z pamięci podręcznej, bez logowania do TS).
- **Narzędzia zapisu tworzą propozycje, a nie zmiany**: `propose_task_create(title, project?, key?, day?, estimate?)`, `propose_note_append(task, text, day?)`, `propose_link_folder(task)` (folder z `cwd` sesji, z hooka), `propose_switch(task)`, `propose_plan(task, day)`. Propozycja trafia do kolejki `ai_proposals` (w pamięci) i pokazuje się jako baner / karta w aplikacji (w czacie A3 jako karta w rozmowie). Narzędzie zwraca „Propozycja czeka na akceptację użytkownika w Todo Track”.
- **Brak narzędzi** do wysyłki TS, zapisów w Jirze, usuwania i zmiany ustawień.
- **Rejestracja** (przełącznik `ai_mcp`): przez CLI, nie edycją plików — `claude mcp add --scope user todo-track -- <ścieżka exe>` i `codex mcp add todo-track -- <ścieżka exe>`; wyłączenie: `… mcp remove todo-track`. Przy starcie sprawdzenie (`claude mcp get todo-track`, `codex mcp list`), czy ścieżka wskazuje aktualny exe (po aktualizacji instalatorem), i ponowna rejestracja, jak przy rozszerzeniu przeglądarki.
- Opis narzędzi (`description` w `tools/list`) po angielsku, krótki, z informacją, że zapisy są propozycjami; bez instrukcji „zawsze wywołuj”, żeby nie zaśmiecać kontekstu sesji.
- README: przykład wpisu do CLAUDE.md projektu: „Po zakończeniu zadania wywołaj `propose_note_append` z jednym zdaniem podsumowania” — wtedy opisy TS powstają z pracy agenta.

Testy Rust: obsługa protokołu MCP na zapisanych wymianach (initialize, list, call, błąd), każde narzędzie odczytu na bazie w pamięci z `tests/fixtures`, filtr osobowy na wyjściu (asercja szczelności jak w A0), propozycje trafiają do kolejki i nie zmieniają bazy, budowa poleceń rejestracji i parsowanie `mcp get` / `mcp list`.

## A3 — Czat w aplikacji

- **Panel boczny po prawej** (`src/components/AiChat.vue`), skrót Ctrl+J, pozycja w lewym panelu nawigacji pod Ustawieniami; chowany jak lewy panel, szerokość zapamiętana. Widoczny tylko przy `ai_engine` ≠ `off`/`""` i włączonym `ai_chat`.
- **Kontekst z miejsca**: „Zapytaj asystenta” w rozwiniętym kafelku zadania (otwiera czat z podpiętym zadaniem), w Zamknięciu dnia (dzień), w Ctrl+K (tekst po „?” idzie do czatu).
- **Silnik**:
  - Claude: jeden długo żyjący proces na rozmowę: `claude -p --input-format stream-json --output-format stream-json --verbose --tools "" --strict-mcp-config --mcp-config <plik z todo-track-mcp> --allowedTools "mcp__todo-track__*" --permission-prompts none --setting-sources "" --no-session-persistence --append-system-prompt <rola asystenta Todo Track> --model <główny>`, w katalogu AI, ze znacznikiem `TODO_TRACK_AI=1`. Wiadomości na stdin, zdarzenia ze stdout strumieniowane do UI.
  - Codex: `codex exec --json --ephemeral … ` na każdą wiadomość z historią rozmowy w promptcie i serwerem MCP przez `-c mcp_servers.todo-track.command=…`; jeśli przy implementacji okaże się zbyt wolne, decyzja o `codex app-server` (eksperymentalny) w osobnym punkcie.
- **Karty akcji**: propozycje z MCP (A2) wyświetlane w rozmowie z „Zastosuj” / „Odrzuć” (i „Popraw” otwierającym zwykły formularz z wypełnionymi polami). Zastosowanie wywołuje istniejące komendy (`add_task`, `plan_task`, `agent_bind`, `save_day_notes`…), nie nowe ścieżki zapisu.
- **Przejrzystość**: pod odpowiedzią lista użytych narzędzi („przeczytał: dzień 20.10”); „Pokaż, co zostało wysłane” (tekst po filtrze A0); „Zatrzymaj”; „Nowa rozmowa”.
- Historia tylko w pamięci; zamknięcie aplikacji kończy rozmowy.
- **Podgląd przeglądarkowy**: symulowany asystent w `src/preview/ai.ts` (B10 przenosi symulatory z `store.ts`; do tego czasu w `store.ts`), ze skryptowanymi odpowiedziami na kilka wzorców pytań i propozycjami kart.

Testy: Rust dla parsera `stream-json` (tekst, wywołania narzędzi, koniec, błąd) na zapisanych strumieniach i dla zarządzania procesem (start, zatrzymanie, ubity proces); Playwright `tests/ai-chat.spec.ts`: otwarcie Ctrl+J, pytanie i odpowiedź z symulatora, karta „Utwórz zadanie” → „Zastosuj” tworzy zadanie, „Odrzuć” nic nie zmienia, kontekst z kafelka, panel ukryty przy wyłączonym silniku; zrzut wzorcowy panelu.

## A4 — Asystent przypisań

**Nieprzypisana sesja agenta** (zależy od B8):
- B8 dostaje dodatkowo: pierwszy `prompt` z `UserPromptSubmit` sesji (przycięty do 500 znaków, tylko w pamięci, tylko gdy `ai_assign_suggest`). Pole w Codexie do sprawdzenia przy B8.
- Gdy sesja pracuje ≥ `suggest::MIN_WORK`, `agents::resolve` nie dało dopasowania, a folder nie jest wyciszony: `ai::ask("assign", { cwd: nazwa folderu, gałąź, prompt, otwarte zadania: id, tytuł, projekt, klucz }, schemat { taskId | null, confidence, reason })`.
- `confidence ≥ 0,6` → baner w stylu istniejących propozycji (`suggest.rs`, źródło „Asystent”): „Agent w »api« pracuje nad »napraw eksport CSV« — to zadanie »Eksport CSV«? [Przypisz folder] [Przełącz timer] [Nie]”. „Przypisz folder” zapisuje `agent_bindings` (następnym razem bez modelu), „Nie” wycisza parę jak „Nie teraz”.
- Jedno wywołanie na sesję (pamięć podręczna po `session_id`).

**Zadanie TS dla zadania**:
- W polu „Zadanie TS” (1.8, `TaskCard.vue`) przycisk „Podpowiedz”, a w podglądzie „Wyślij do TS” przy wierszach bez zadania TS „Podpowiedz wszystkie”: `ai::ask("ts_task", { tytuł, projekt, klucz, notatka dnia, lista zadań TS }, schemat { tsTaskId | null, confidence, reason })`.
- Propozycja wypełnia pole jako sugestię z uzasadnieniem; zapis dopiero po wyborze. Podpowiedź „Ustaw jako domyślne dla projektu” zaznaczona, gdy projekt nie ma jeszcze mapowania.

Testy: Rust dla budowy danych wejściowych (bez nazwisk — asercja A0), progu pewności, pamięci podręcznej i braku wywołania, gdy heurystyka dopasowała; Playwright z symulowanym asystentem dla banera i pola „Zadanie TS”.

## A5 — Zamknięcie dnia z asystentem

- W Zamknięciu dnia nowa sekcja „Asystent” (przy `ai_day_close`) z przyciskiem „Przeanalizuj dzień” (nie uruchamia się sama).
- Dane wejściowe: oś dnia (pomiary, luki, czas agentów z przypisaniami, spotkania bez tytułów, gdy wyłączone), commity z dnia z folderów w `agent_bindings` (`git log --since --until --format=%H%x09%ad%x09%s`, bez autora), zmiany statusów Jiry z dnia (z obserwacji), notatki dnia.
- Wynik (schemat): lista propozycji `{ kind: gap|unassigned|note|ts_description, from, to, taskId, text, reason }`:
  - **luka** 10:15–11:00 → „Dodaj jako czas” (otwiera `ManualTime` z wypełnionymi polami);
  - **czas agenta bez zadania** → „Przypisz”;
  - **notatka dnia** z commitów i pracy agentów → „Dodaj do notatki” (edytowalna przed zapisem);
  - **opis TS** dla wierszy bez notatki → trafia do podglądu „Wyślij do TS” jako proponowany opis.
- Każda propozycja z uzasadnieniem („commit »fix eksportu« w repo api o 10:40”); „Zastosuj wszystkie” tylko dla notatek, czas zawsze pojedynczo.

Testy: Rust dla zebrania danych dnia (fixture z repo git w katalogu tymczasowym, bez autorów), schematu i mapowania propozycji; Playwright z symulatorem: sekcja, każda karta, brak zmian bez kliknięcia.

## A6 — Rano i przegląd tygodnia (opcjonalnie)

- Rano: przycisk „Komentarz asystenta” — krótka ocena planu względem pojemności i terminów (model główny), bez zmian w planie; propozycje przesunięć jako karty jak w A3.
- Przegląd tygodnia: „Szkic wniosków” wypełnia pole „Wnioski” (`week_notes`) propozycją do edycji.

Testy: Playwright z symulatorem.

## Po każdym zadaniu

Zielone `npm run build`, `npm test` (Playwright), `cargo test --locked`, `cargo fmt --check`; przegląd dwiema soczewkami (zgodność z planem, poprawność), a w części A także soczewką **prywatności** (czy każda ścieżka do modelu przechodzi przez A0 i czy nic nie wywołuje prawdziwego CLI). Commit `B<n>: <tytuł>` / `A<n>: <tytuł>`. Zadania nie podnoszą wersji.

## Wydania

- **R110 — przygotowanie 1.10.0**: zadania części B. `notes.md`, README („Nowości 1.10”, opis ustawień „Agenci”, hooki zamiast plików sesji, Orka, „Wersja w repozytorium”), `NEXT_SESSION.md` z listą ręcznych sprawdzeń B. Wersję podbija, buduje i publikuje użytkownik: scalenie `backlog` do `master` i `npm run ship -- 1.10.0`.
- **R111 — przygotowanie 1.11.0**: zadania części A. README („Nowości 1.11”, sekcja „Asystent AI”: subskrypcja zamiast klucza API, co wychodzi do modelu i jak działa filtr nazwisk z ograniczeniem słownika, serwer MCP z przykładem wpisu do CLAUDE.md, karty akcji), `notes.md`, `NEXT_SESSION.md` z listą ręcznych sprawdzeń A. Wydanie: `npm run ship -- 1.11.0` przez użytkownika.

## Do ręcznego sprawdzenia (po instalacji, przez użytkownika)

- B4: nazwa pola nadgodzin w prawdziwym TS, czy norma dnia jest tam stała (8 h) czy per użytkownik, i czy porównanie po zapisie uwzględnia nadgodziny.
- B8: sesja Claude z workflow ponad 15 minut widoczna w kafelku; sesja czekająca na zgodę nie liczy się jako praca; Codex przed i po zatwierdzeniu hooka przez `/hooks`; sesja uruchomiona z Orki; restart aplikacji w trakcie pracy agenta.
- B9: tworzenie worktree przy działającej i niedziałającej Orce.
- A1: `claude -p` uruchomione z aplikacji korzysta z subskrypcji (bez klucza API) i działa w schowanym oknie; `--setting-sources ""` naprawdę pomija hooki z `~/.claude/settings.json` (w tym wpisy Orki) — jeśli nie, potwierdzić, że filtr `TODO_TRACK_AI` w `todo-track-hook.exe` wystarcza; komunikat przy wylogowanym CLI i przy wyczerpanym limicie; `codex exec --output-schema` zwraca poprawny JSON.
- A1: dialog pierwszego uruchomienia z jednym, dwoma i zerem wykrytych CLI; wybór przetrwał restart.
- A0: przejrzeć „Pokaż, co zostanie wysłane” na prawdziwym dniu z Jirą: czy żadne nazwisko nie przeszło; uzupełnić „Ukrywaj te nazwy”.
- A2: rejestracja w Claude Code i Codexie (`claude mcp get todo-track`), narzędzia widoczne w sesji (`/mcp`), zachowanie przy zamkniętej aplikacji, ponowna rejestracja po aktualizacji instalatorem, współistnienie z wpisami Orki.
- A3: czas pierwszej odpowiedzi, zatrzymanie w trakcie, rozmowa z Codexem; czat nie pojawia się w kafelku „Agenci AI”.
- A4: sesja w nowym folderze bez klucza w gałęzi daje trafny baner; „Przypisz folder” działa następnym razem bez modelu.
- A5: analiza dnia z kilkoma lukami i pracą agentów; trafność propozycji opisów TS.
