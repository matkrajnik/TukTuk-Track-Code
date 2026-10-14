# Todo Track 1.0 — plan implementacji

Stan wyjściowy: 0.3.0 (14 października 2026). Tauri 2 + Rust + Vue 3, SQLite w `%APPDATA%\pl.todotrack.desktop`. 18 testów Playwright, 15 testów Rust. Kod nieskomitowany.

## Zakres 1.0

Zaakceptowane:

| # | Funkcja | Rozmiar |
|---|---------|---------|
| A | Podział przedziału w wybranym momencie | M |
| B | Mini-widget zawsze na wierzchu, włączany w ustawieniach | M |
| C | Cel dzienny i tygodniowy, tooltip i ikona traya | M |
| D | Raport TS z dowolnie wybranych dni | M |
| E | Plan kontra rzeczywistość na osi dnia | L |
| F | Ręczna kolejność zadań | M |
| G | Tablica tygodnia Pon–Nd z przeciąganiem | L |
| H | Statystyka trafności szacunków | S |
| I | Autostart z Windows | S |
| J | Powiadomienia systemowe | M |
| K | Zapamiętywanie pozycji i rozmiaru okna | S |
| L | Globalny skrót Start/Stop | S |

Poza zakresem 1.0: worklogi Jiry, autoupdater, eksport CSV. Zgłoszenia Jiry w szybkim przełączaniu nie zostały rozstrzygnięte; nie wchodzą do planu, dopóki nie zdecydujesz.

Rozmiary: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni pracy z testami. Łącznie około 15–20 dni roboczych, w sześciu etapach. Każdy etap kończy się zielonymi testami i działającą aplikacją, więc można go wydać osobno jako 0.4, 0.5 itd., a 1.0.0 nadać po ostatnim.

## Założenia przyjęte bez pytania

- Cel tygodniowy wynika z celu dziennego i wybranych dni roboczych (domyślnie 8 h, Pon–Pt). Jedno pole mniej do pilnowania.
- Domyślny skrót Start/Stop: `Ctrl+Alt+Space`. Skrót szybkiego przełączania zostaje `Ctrl+Shift+Space`. Oba stają się konfigurowalne.
- Widget domyślnie wyłączony. Po włączeniu ma dodatkową opcję „tylko gdy główne okno jest schowane lub bez fokusu”.
- Ręczna kolejność działa wewnątrz pasma priorytetu. Przeciągnięcie zadania do innego pasma zmienia jego priorytet. Aktywny timer zawsze pierwszy, jak dotąd.
- Bloki planu na osi dnia układają się kolejno od godziny startu dnia (ustawienie, domyślnie 08:00) w ręcznej kolejności; zadanie może mieć też własną godzinę startu.
- Powiadomienia Windows: termin dzisiaj lub przekroczony, przedział dłuższy niż 4 h bez Stop, wieczorne „pomiar nadal trwa” (domyślnie 17:30). Każde z osobnym wyłącznikiem.

Jeśli któreś założenie Ci nie pasuje, zmiana jest tania przed etapem, który go dotyczy.

## Etap 0 — fundamenty (bez nowych funkcji)

Cel: bezpieczny kod, miejsce na rozbudowę, wspólne mechanizmy, z których korzysta większość funkcji.

### 0.1 Git i CI

- `git add -A`, commit „0.3.0 baseline”, push na `main`. Sprawdzić, że `.gitignore` wyklucza `src-tauri/target`, `dist`, `node_modules`, `test-results`.
- Uruchomić istniejący workflow `.github/workflows` i doprowadzić do zieleni (Node 22, Rust stable, `npm test`, `cargo test --locked`, `tauri build --no-bundle`).
- Dodać `.nvmrc` z `22`, bo systemowy Node to 18.

### 0.2 Podział `App.vue`

Wyciąć z [src/App.vue](src/App.vue) bez zmiany zachowania. Wzorzec pozostaje: widoki dostają `data` i `now` przez props, zwracają snapshot przez `updated`.

| Nowy plik | Zawartość |
|-----------|-----------|
| `src/components/TitleBar.vue` | własny pasek okna, kontrolki, stan maximized |
| `src/components/AppNav.vue` | nagłówek z nawigacją, motyw, ⇄, schowaj |
| `src/views/TasksView.vue` | pole dodawania, filtry, grupy, lista |
| `src/components/TaskCard.vue` | kafelek: wiersz, priorytet inline, rozwinięcie z przedziałami i `TaskPlanning` |
| `src/views/ReportsView.vue` | raport dzienny, wybór dnia, TS |
| `src/components/SessionEditor.vue` | modal edycji przedziału (w etapie 1 dostanie zakładkę „Podziel”) |
| `src/components/ConfirmDialog.vue` | usuwanie i inne potwierdzenia |
| `src/composables/useModal.ts` | focus trap, przywracanie fokusu |
| `src/report.ts` | `dayRows(data, day, now)`, `decimalHours`, `tsText(...)`; wspólne dla ReportsView i DayClose, które dziś liczą to dwa razy |
| `src/order.ts` | jeden komparator sortowania zadań (timer, priorytet, kolejność, id), używany przez listę, QuickSwitch, Journal i tablicę tygodnia |

`App.vue` zostaje jako powłoka: snapshot, zegar, nasłuch zdarzeń Tauri, baner bezczynności, routing widoków. Cel: poniżej 300 linii.

`style.css` podzielić analogicznie: `base.css` (tokeny, motywy, typografia), `layout.css` (pasek, nawigacja, modal), `tasks.css`, `reports.css`, `features.css` zostaje dla reszty.

Weryfikacja: wszystkie 18 testów Playwright przechodzi bez zmian w testach. To jest kryterium, że refaktor niczego nie zmienił.

### 0.3 Ustawienia jako jeden mechanizm

Dziś: `app_settings` zwraca ręcznie składany JSON, `set_idle_enabled` to osobna komenda. Osiem nowych funkcji potrzebuje ustawień, więc:

- Nowy moduł `src-tauri/src/settings.rs` ze strukturą `AppSettings` (serde, `#[serde(default)]`), zapisywaną klucz po kluczu w istniejącej tabeli `settings`. Klucze i domyślne wartości:

| Klucz | Domyślnie | Funkcja |
|-------|-----------|---------|
| `idle_enabled` | true | istniejące |
| `target_day_minutes` | 480 | C |
| `work_days` | `[1,2,3,4,5]` | C, G |
| `day_start` | `08:00` | E |
| `widget_enabled` | false | B |
| `widget_background_only` | false | B |
| `autostart_hidden` | false | I |
| `notify_due` | true | J |
| `notify_long_minutes` | 240 | J |
| `notify_evening` | `17:30` | J |
| `shortcut_quick` | `Control+Shift+Space` | L |
| `shortcut_toggle` | `Control+Alt+Space` | L |

- Komendy: `get_settings() -> AppSettings`, `set_settings(patch: serde_json::Value) -> AppSettings`. Walidacja zakresów w Rust (minuty 0–1440, godzina `HH:MM`, dni 1–7). Po zapisie emit `settings-changed`; moduły B, J, L reagują na zdarzenie.
- Frontend: `src/settings.ts` z `readSettings()` i `writeSettings(patch)`; w trybie podglądu localStorage, tak jak store.
- `LibraryView` dzieli zakładkę „Kopie i ustawienia” na „Kopie” i „Ustawienia”. Ustawienia dostają sekcje: Czas pracy, Okna, Powiadomienia, Skróty, Uruchamianie.
- Usunąć `app_settings` i `set_idle_enabled` po przepięciu.

### 0.4 Baza

- Indeksy: `CREATE INDEX IF NOT EXISTS sessions_task ON sessions(task_id)` i `sessions_started ON sessions(started_at)`. Idempotentne, bez migracji danych.
- `planner::Details` dostaje pola `order: i64` (domyślnie 0) i `planned_start: String` (puste lub `HH:MM`). Stare rekordy JSON czytają się przez `serde(default)`.
- Nowa tabela `notifications_sent(key TEXT PRIMARY KEY, at INTEGER)` dla etapu 2.
- Moduł `src-tauri/src/stats.rs`: `day_total_ms(conn, date: NaiveDate, now)` dzielący przedziały przez lokalną północ z chrono `Local` (odpowiednik `inDay` z [src/time.ts](src/time.ts)). Potrzebny traya i powiadomieniom. Test z dobą zmiany czasu (25 października 2026).

## Etap 1 — czas: podział, cel, Start/Stop

### A. Podział przedziału

Backend, [src-tauri/src/db.rs](src-tauri/src/db.rs):

```
split_session(id, at, target_task: Option<i64>, now) -> Snapshot
```

- Walidacja: przedział istnieje i nie jest w koszu; `started_at < at < COALESCE(ended_at, now)`; zadanie docelowe istnieje, nie jest w koszu.
- Przedział zakończony: `UPDATE sessions SET ended_at=at WHERE id`, `INSERT` drugiej części `(target, at, old_end)`. Brak konfliktów z definicji, bo zakres łączny się nie zmienia.
- Przedział aktywny: pierwsza część zamknięta na `at`, druga część wstawiona jako aktywna `(target, at, NULL)`. Unikalny indeks `one_active_session` wymusza kolejność UPDATE, potem INSERT, w jednej transakcji. Dla aktywnej części obowiązują reguły Startu: zadanie docelowe nie może być ukończone, `clear_waiting`, zdjęcie archiwum.
- Komenda `split_session` w `main.rs`, `reminder::reset`, emit `data-changed`.
- Odpowiednik w [src/store.ts](src/store.ts) dla trybu podglądu.

Frontend:

- `SessionEditor.vue` dostaje zakładki „Godziny” i „Podziel”. Zakładka Podziel: pole `datetime-local` z domyślnym środkiem przedziału, lista zadań (domyślnie to samo), podgląd obu części z czasem trwania. Dostępna także dla aktywnego przedziału, bez wymogu Stop.
- Pasek „Bieżący pomiar” w Zadaniach dostaje przycisk „Podziel”, który otwiera modal dla aktywnej sesji. To główny przypadek: „od 11:15 robiłem już coś innego”.
- Oś dnia w Plan i historia: przycisk „Podziel” obok „Edytuj”.

Testy: Rust (zakończony, aktywny, `at` poza zakresem, zadanie ukończone jako cel aktywnej części, cel w koszu). Playwright: podział aktywnego pomiaru z paska i sprawdzenie, że pierwsza część trafiła do raportu pod starym zadaniem, a timer biegnie pod nowym.

### C. Cel dzienny i tygodniowy, tray

- Ustawienia: `target_day_minutes`, `work_days`. Cel tygodnia = cel dnia × liczba dni roboczych.
- Zadania: pod „Dzisiaj 03:12” pasek postępu do celu i tekst „03:12 / 08:00”. Po przekroczeniu pasek zmienia kolor na bursztynowy.
- Dashboard: karta „Dzisiaj” i „Ten tydzień” z paskami względem celu; w wykresie 7 dni cienka linia celu dla dni roboczych.
- Tydzień w Plan i historia: przy każdym dniu różnica względem celu.
- Zamknięcie dnia: komunikat „Do celu brakuje 1:20” lub „Cel przekroczony o 0:40”, informacyjnie, bez blokady.
- Tray, nowy moduł `src-tauri/src/tray.rs` (przeniesienie kodu z `main.rs`):
  - `TrayIconBuilder::with_id("main")`, wątek odświeżający co 5 s tooltip: `Zadanie X · 01:23:45 · dziś 5:10 / 8:00` (limit 127 znaków, tytuł przycinany).
  - Druga ikona `icons/icon-active.ico` z kropką, generowana w `installer/make-artwork.ps1`; `set_icon` przy zmianie stanu aktywny/nieaktywny.
  - Menu: pozycja „Szybkie przełączanie” oraz nieklikalna pozycja z nazwą bieżącego zadania (`MenuItem::set_text`).

Testy: Rust dla `stats::day_total_ms` i składania tooltipu; Playwright dla paska postępu i komunikatu w zamknięciu dnia.

### L. Globalny skrót Start/Stop

- Nowy moduł `src-tauri/src/shortcuts.rs`: rejestracja obu skrótów z ustawień, ponowna rejestracja po `settings-changed`, mapa błędów `{quick: "", toggle: "zajęty"}` zamiast dzisiejszego `ShortcutStatus(String)`.
- Zachowanie `toggle`: aktywny pomiar → Stop. Brak pomiaru → Start ostatnio mierzonego, nieukończonego i niezarchiwizowanego zadania. Brak takiego → otwórz szybkie przełączanie.
- Potwierdzenie: jeśli widget włączony, pokazuje zmianę; jeśli nie, krótkie powiadomienie systemowe (po etapie 2) z treścią „Stop: X · 01:23:45” lub „Start: X”.
- Ustawienia: dwa pola przechwytujące kombinację klawiszy (komponent `ShortcutInput.vue`: nasłuch `keydown`, zapis w formacie pluginu `Control+Alt+Space`), błąd rejestracji przy polu, przycisk „Przywróć domyślny”.

Testy: Rust dla logiki wyboru zadania do wznowienia; Playwright dla komponentu wejścia skrótu (zapis i odczyt z localStorage w podglądzie).

## Etap 2 — okna i system

### K. Pozycja i rozmiar okna

- `tauri-plugin-window-state` z `with_denylist(&["reminder", "quick"])`. Okna `main` i `widget` zapamiętywane.
- Nasze okna są chowane, nie zamykane, więc plugin sam nie zapisze stanu. Wywołać `app.save_window_state(StateFlags::all())` w `CloseRequested`, przy „Zakończ aplikację” z traya i przy `--prepare-update`.
- Sprawdzić natywnie: odłączony monitor (plugin ma sprowadzać okno na dostępny ekran), stan zmaksymalizowany z własnym paskiem.

### I. Autostart

- `tauri-plugin-autostart`, sterowany z Rust: komendy `autostart_get`, `autostart_set(enabled, hidden)`. Przy `hidden` wpis Run dostaje argument `--hidden`.
- `main` w `tauri.conf.json` dostaje `"visible": false`; w `setup` pokazujemy je, chyba że jest `--hidden`. Wtedy aplikacja startuje do traya, a widget (jeśli włączony) się pokazuje.
- Ustawienia: „Uruchamiaj z Windows” i „Uruchamiaj schowane do traya”. Stan czytany z rejestru przez plugin, nie z bazy, żeby nie rozjechać się po ręcznej zmianie w Menedżerze zadań.
- Uwaga: wpis wskazuje ścieżkę exe; instalator NSIS aktualizuje w miejscu, więc ścieżka się nie zmienia.

### B. Mini-widget

Okno `widget` w `tauri.conf.json`: 320×64, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: false`, `visible: false`, `focusable: false` (klik nie zabiera fokusu aplikacji, w której pracujesz). Dodać do `capabilities/default.json`.

Frontend `src/WidgetWindow.vue`, montowany w [src/main.ts](src/main.ts) po etykiecie okna, tak jak reminder i quick:

- Pasek przeciągania (`data-tauri-drag-region`), nazwa zadania z wielokropkiem, zegar mono, przycisk Stop lub Start (ostatnie zadanie, ta sama logika co skrót `toggle`), przycisk ⇄ (`quick_open`), klik w nazwę pokazuje główne okno.
- Bez pomiaru: „Brak pomiaru” i ⇄.
- Stan: `get_snapshot` przy `data-changed`, tyknięcie co 1 s, motyw z localStorage jak w reminder.

Backend `src-tauri/src/widget.rs`:

- `apply(app)`: czyta `widget_enabled` i `widget_background_only`, pokazuje lub chowa okno. Wołany w `setup`, po `settings-changed` i co 2 s z istniejącej pętli w `reminder.rs`, która już sprawdza widoczność i fokus głównego okna (tryb „tylko w tle”).
- Przycisk X na widgecie chowa go do końca sesji aplikacji, nie zmienia ustawienia; główne okno ma w nagłówku przełącznik widgetu, żeby go przywrócić.

Testy: Playwright renderuje `WidgetWindow` w podglądzie (route `#widget`) i sprawdza Stop/Start; natywnie: brak kradzieży fokusu, przeciąganie, pamiętanie pozycji po restarcie (K).

### J. Powiadomienia systemowe

- `tauri-plugin-notification`, wysyłka z Rust. Nowy moduł `src-tauri/src/notify.rs`, wątek co 30 s.
- Reguły, każda z osobnym kluczem dedupe w `notifications_sent`:

| Reguła | Klucz | Warunek |
|--------|-------|---------|
| Termin | `due:<dzień>` | przy starcie i o 09:00: są nieukończone zadania z `dueDay <= dziś`; treść z liczbą i pierwszymi trzema tytułami |
| Długi przedział | `long:<id sesji>` | aktywna sesja trwa ≥ `notify_long_minutes`; jedna wiadomość na sesję |
| Wieczór | `evening:<dzień>` | o `notify_evening` aktywny pomiar nadal trwa |
| Cel | `target:<dzień>` | suma dnia osiągnęła cel (opcjonalne, wyłączone domyślnie) |

- Logika w strukturze `NotifyClock` z kontrolowanym czasem, jak `idle::Clock` i `ReminderClock`, z testami Rust. Wpisy starsze niż 7 dni usuwane przy starcie.
- Ryzyko: w `tauri dev` powiadomienia Windows mogą podpisywać się jako PowerShell lub nie pojawić bez zarejestrowanego AUMID; weryfikacja na instalacji NSIS z osobnym identyfikatorem. Klik w powiadomienie nie jest obsługiwany, nie opieramy na nim żadnej funkcji.

## Etap 3 — planowanie

Kolejność ma znaczenie: F daje porządek, z którego korzystają E i G.

### F. Ręczna kolejność zadań

- Komparator w `src/order.ts`: aktywny timer, priorytet malejąco, `order` rosnąco, `id` malejąco. Zastępuje sortowanie w TasksView, QuickSwitch, Journal (lista planu) i Dashboard.
- Backend `planner::reorder(entries: Vec<{id, order, priority}>)`: w transakcji `json_set` obu pól dla każdego wpisu, wzorem `set_priority`, bez dotykania reszty JSON (nie nadpisuje niezapisanego formularza planu). Komenda `reorder_tasks`.
- Frontend w `TaskCard`/`TasksView`: natywne HTML5 drag and drop (`draggable`, `dragstart`, `dragover`, `drop`), bez biblioteki. Uchwyt ⋮⋮ po lewej. Upuszczenie liczy nową listę pasma (priorytet sąsiadów) i numeruje `order` od 1 krokiem 1, potem wysyła całe pasmo.
- Klawiatura: `Alt+↑` / `Alt+↓` na uchwycie przesuwa o jedną pozycję. Daje dostępność i stabilne testy Playwright, bo drag w Playwright bywa chwiejny.
- W Plan i historia lista planu dnia ma te same uchwyty; kolejność w planie to kolejność bloków w E.

### E. Plan kontra rzeczywistość na osi dnia

Komponent `src/components/DayTimeline.vue` (SVG), osadzony w zakładce Dzień w [src/JournalView.vue](src/JournalView.vue) nad dzisiejszą listą przedziałów, która zostaje (ma przyciski Edytuj i Podziel).

- Oś pozioma: od min(`day_start`, pierwszy pomiar) do max(17:00, ostatni pomiar, koniec ostatniego bloku planu), zaokrąglone do pełnej godziny.
- Tor „Plan”: zadania z `plannedDay == dzień`, w ręcznej kolejności, układane kolejno od `day_start`; długość z `estimateMinutes` (bez szacunku 30 min, rysowane kreskowane). Zadanie z własnym `plannedStart` jest przypięte do tej godziny, a kolejne układają się po nim.
- Tor „Rzeczywistość”: przedziały z `inDay`, kolor według zadania. Trwający pomiar z animowaną krawędzią.
- Kolory: deterministyczny hash `id` do palety 8 odcieni zdefiniowanych jako tokeny CSS w obu motywach. Legenda pod osią.
- Interakcje: hover pokazuje tytuł i godziny; klik w blok rzeczywisty otwiera `SessionEditor`; klik w blok planu rozwija `TaskPlanning`.
- Tabela pod osią: zadanie, plan, rzeczywistość, różnica (kolor przy przekroczeniu), „nieplanowane” dla pracy spoza planu i „niezaczęte” dla planu bez pracy.
- `TaskPlanning` dostaje pole „Godzina startu w planie” (`plannedStart`, opcjonalne).

Testy: Playwright z ustalonym zegarem (`page.clock`): dwa zadania z szacunkami, jeden pomiar, sprawdzenie liczby bloków i tekstów tabeli. Zrzut 760 px.

### G. Tablica tygodnia Pon–Nd

Zakładka Tydzień w JournalView dzieli się na „Tablica” (nowa, domyślna) i „Podsumowanie” (dotychczasowa treść). Nowy komponent `src/components/WeekBoard.vue`.

- Kolumna „Do zaplanowania” po lewej: otwarte zadania bez `plannedDay` w tym tygodniu, z polem szukania, w ręcznej kolejności. Zaległe plany sprzed tygodnia jako sekcja „Zaległe (n)” w tej kolumnie.
- Siedem kolumn: nagłówek z datą i dniem tygodnia, suma szacunków względem pojemności (cel dzienny dla dni z `work_days`, „wolne” dla pozostałych), faktyczny czas dnia. Pasek pojemności: zielony poniżej 90%, bursztynowy 90–100%, czerwony powyżej.
- Karta zadania: tytuł, projekt, szacunek, procent wykonania, znacznik Jiry, status ukończone/czekam.
- Przeciąganie między kolumnami zmienia `plannedDay`, w obrębie kolumny zmienia `order`. Backend: `plan_task(id, day, order)` w planner.rs, atomowy `json_set`, nie rusza reszty JSON.
- Alternatywa bez myszy: menu „Przenieś na…” z listą dni na karcie. Używane też w testach.
- Nawigacja ← → po tygodniach, „Dzisiaj”. Kopie cykliczne pojawiają się same, bo mają `plannedDay`.
- „Zaplanuj na następny poniedziałek” z podsumowania przenosi się na kartę jako opcja w menu dni.

Testy: Playwright: przeniesienie zadania do środy przez menu, suma szacunków w nagłówku, przekroczenie pojemności. Rust: `plan_task` nie nadpisuje innych pól.

## Etap 4 — raporty

### D. Raport TS z wybranych dni

W `ReportsView` nowy tryb „Wybrane dni” obok „Jeden dzień”.

- Kalendarz miesiąca jako siatka przycisków; klik przełącza zaznaczenie dnia; dni z pracą mają kropkę i czas w tooltipie. Nawigacja po miesiącach. Skróty: „Ten tydzień”, „Poprzedni tydzień”, „Ten miesiąc”, „Dni robocze tego miesiąca”, „Wyczyść”. Ostatnie zaznaczenie pamiętane w localStorage.
- Lista zaznaczonych dni z sumami i przyciskiem usunięcia dnia z zestawu.
- Układ raportu, przełącznik:
  - „Po dniach”: kolumny `Dzień`, `Zadanie`, `Czas (h)`; po każdym dniu wiersz `Razem <dzień>`; na końcu `Razem`.
  - „Zbiorczo”: zadania zsumowane ze wszystkich dni, kolumny `Zadanie`, `Czas (h)`; notatki z kilku dni łączone średnikiem.
- Notatki dołączane jak dziś (checkbox). Zaokrąglenie per wiersz, sumy z pełnego czasu, zgodnie z obecną zasadą.
- Cała logika w `src/report.ts` (`tsText(days, data, now, options)`), żeby DayClose, raport dzienny i tryb wielodniowy miały jedno źródło. Kopiowanie przez istniejący mechanizm z awaryjnym zaznaczeniem.

Testy: Playwright: zaznaczenie dwóch dni, oba układy, zawartość schowka, przełączenie miesiąca; test dnia zmiany czasu w zestawie.

### H. Trafność szacunków

Nowa karta na Dashboardzie „Trafność szacunków”, bez backendu.

- Zbiór: zadania ukończone (również zarchiwizowane) z `estimateMinutes > 0` i czasem > 0. Okres: 30 dni, 90 dni, wszystko (po dacie ostatniego przedziału).
- Wskaźniki: liczba zadań, mediana stosunku rzeczywistość/plan, odsetek w widełkach ±20%, odsetek przekroczeń. Histogram sześciu koszyków (<50%, 50–80, 80–120, 120–150, 150–200, >200) rysowany jak obecny wykres 7 dni.
- Mediana według projektu (tabela) i dziesięć największych rozjazdów z linkiem do raportu z dnia ostatniej pracy.
- „Zagrożone teraz”: otwarte zadania, których czas przekroczył szacunek; na kafelku w Zadaniach procent powyżej 100 w kolorze ostrzegawczym.

Testy: funkcje czyste w `src/estimates.ts` z testem Playwright na przygotowanym localStorage.

## Etap 5 — wydanie 1.0.0

1. Wersje: `package.json`, `package-lock.json`, `Cargo.toml`, `Cargo.lock`, `tauri.conf.json` na 1.0.0. `maintenance.rs` sam zrobi `before-1.0.0-*.sqlite` przy pierwszym otwarciu starej bazy.
2. README: sekcja „Organizacja pracy 1.0” zamiast listy 0.3.0, aktualizacja opisu ustawień, skrótów, widgetu, autostartu i powiadomień. NEXT_SESSION.md: stan po 1.0.
3. Pełna weryfikacja: `npm run build`, `npm test`, `cargo test --locked`, `cargo fmt --check`, `npm run tauri build -- -- --locked`.
4. Natywna instancja `pl.todotrack.validation.v6` z instalacją NSIS (osobny identyfikator): autostart z `--hidden`, widget bez kradzieży fokusu, pozycja okien po restarcie, powiadomienia Windows, globalne skróty, podział aktywnego pomiaru, tablica tygodnia, aktualizacja z 0.3.0 przy aktywnym timerze. Skrypty i zrzuty w `src-tauri/target/verification-v6/`.
5. Tag `v1.0.0`, instalator `Todo Track_1.0.0_x64-setup.exe`.

## Zależności i kolejność

```
0.1 git ─┐
0.2 refaktor ─┬─ 0.3 ustawienia ─┬─ C cel/tray ─┬─ J powiadomienia
0.4 baza ─────┘                  ├─ L skrót     │
                                 ├─ K okna ─────┼─ B widget
                                 └─ I autostart ┘
A podział (po 0.2)
F kolejność ─┬─ E oś dnia (po C: day_start)
             └─ G tablica (po C: work_days)
D TS wielodniowy (po 0.2: report.ts)
H trafność (po 0.2)
```

A, D i H są niezależne od reszty i mogą wypełniać luki między etapami.

## Ryzyka

- **Dwie implementacje domeny.** Każda nowa komenda (split, reorder, plan_task, ustawienia) musi mieć odpowiednik w [src/store.ts](src/store.ts), bo testy Playwright chodzą na localStorage. To stały koszt około 20% każdej funkcji; wliczony w rozmiary. Alternatywa (testy na prawdziwym backendzie przez `tauri-driver`) to osobny temat, nie na 1.0.
- **Drag and drop** w WebView2 i Playwright: dlatego każde przeciąganie ma równoważnik klawiaturowy lub menu, a testy używają jego.
- **Okno `focusable: false`** może na niektórych konfiguracjach Windows nie przyjmować kliknięć w przyciski; sprawdzić w pierwszym dniu etapu 2, awaryjnie `focus: false` jak w reminder i przywracanie fokusu poprzedniemu oknu.
- **Powiadomienia w dev** mogą nie działać; weryfikacja wyłącznie na pakiecie NSIS.
- **Skróty globalne** kolidują z innymi programami; stąd konfigurowalność i widoczny błąd rejestracji.
- **Rozmiar snapshotu.** Widget i tray czytają dane co sekundę lub co 5 s; przy kilku tysiącach przedziałów `get_snapshot` nadal mieści się w milisekundach, ale tray liczy sumę dnia w SQL (`stats.rs`), nie przez snapshot.

## Co dostajesz po każdym etapie

| Etap | Co działa |
|------|-----------|
| 0 | Kod w gicie, CI zielone, ta sama aplikacja w mniejszych plikach, nowa zakładka Ustawienia |
| 1 | Podział pomiaru, cel dnia na liście i w trayu, ikona z kropką, skrót Start/Stop |
| 2 | Widget, pamięć okien, autostart, powiadomienia |
| 3 | Przeciąganie zadań, tablica tygodnia, oś planu i rzeczywistości |
| 4 | TS z wybranych dni, trafność szacunków |
| 5 | Instalator 1.0.0 i zweryfikowana aktualizacja z 0.3.0 |
