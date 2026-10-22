# Przegląd kodu TukTuk Track 2.0.0

Data: 22 października 2026. Stan: `master` aa747f7, drzewo czyste. Przegląd obejmuje backend (Tauri 2, Rust, ok. 57 tys. linii z testami), frontend (Vue 3, TypeScript, ok. 39 tys. linii), testy Playwright (ok. 25 tys. linii), rozszerzenie przeglądarki, skrypty wydania, CI i dokumentację.

## 1. Metoda i zakres

Przegląd wykonało 15 wyspecjalizowanych recenzentów z wąskimi zakresami, każdy tylko do odczytu: cztery przeglądy Rust (rdzeń i baza; pulpit i Windows; integracje HTTP; AI, MCP, hooki, rozszerzenie), dwa przeglądy Vue (widoki; komponenty), TypeScript i skrypty, bezpieczeństwo, ciche błędy, projekt typów na granicy IPC, wydajność, dostępność (WCAG 2.2 AA), jakość testów, komentarze i dokumentacja oraz architektura. Najważniejsze ustalenia zweryfikowałem osobno w kodzie.

Równolegle uruchomiłem automatyczne sprawdzenia (sekcja 2). Clippy nie był zainstalowany w toolchainie; dodałem komponent `clippy` przez `rustup component add clippy` (odwracalne: `rustup component remove clippy`).

Czego przegląd nie obejmuje: zachowania na prawdziwych usługach (Jira, Redmine, TS, Microsoft Graph, Claude Code, Codex, Orca), pomiarów wydajności w działającej aplikacji, testów z czytnikiem ekranu i w trybie wysokiego kontrastu, pełnej lektury modułów testowych Rust (ok. 20 tys. linii) i części specyfikacji Playwright. Ustalenia, które są hipotezą z kodu, a nie faktem, oznaczam „do sprawdzenia”.

Skala wag: CRITICAL (luka lub utrata danych, blokuje wydanie), HIGH (błąd widoczny dla użytkownika lub istotna luka), MEDIUM (odporność, utrzymywalność, wydajność, dostępność), LOW (utwardzanie, spójność, drobiazgi).

## 2. Automatyczne sprawdzenia

| Sprawdzenie | Wynik |
|---|---|
| `npm run build` (vue-tsc + vite) | przechodzi; jeden chunk JS 780 kB (245 kB gzip), Vite ostrzega o rozmiarze |
| `cargo fmt --check` | czysto |
| `cargo test --locked` | 688 testów przechodzi (684 w bibliotece, 1 naming, 3 mcp_exe) |
| `cargo clippy --all-targets` | 16 ostrzeżeń w kodzie produkcyjnym, 41 w testach (sekcja 2.1) |
| `npm audit` | 0 podatności |
| `cargo audit` | niedostępny (brak narzędzia) |
| Playwright, pełny przebieg | 651 z 661 przeszło; 10 padło na 30-sekundowym limicie startu strony przy równoległym clippy i 15 agentach |
| Playwright, powtórka 9 plików z porażkami | 143 z 143 przeszło w 49 s |

Wniosek: zestaw testów jest zielony, a porażki były skutkiem obciążenia maszyny. Warto rozważyć `retries: 1` w CI albo mniejszą liczbę workerów.

### 2.1 Clippy, kod produkcyjny

Martwy kod (nigdy nieużywany poza testami):

- `reminders.rs:114` `to_ms`, `reminders.rs:532` `fire_due` (używane jest tylko `fire_due_tracked`)
- `timesheet.rs:484` `hours_total`, `timesheet.rs:488` `overtime_total`
- `quickadd.rs:33` `create` (używane jest `create_linked`)
- `plan_assist.rs:209` `collect_morning`
- `browser.rs:181` `fresh_key`, `hooks.rs:405` `session` i `len`

Pozostałe: `reminders.rs:651` dwie identyczne gałęzie `if` (spotkanie ręczne z pustym tematem daje pusty tytuł zamiast „spotkanie”), `jira_board.rs:295` funkcja z 8 argumentami, `browser.rs:770` i `:812` bardzo złożone typy, `settings.rs:958` przypisanie pola po `Default::default()`, 25 zbędnych `clone` w testach. Reguły projektu w `.claude/rules/ecc/rust` wymagają `cargo clippy -- -D warnings`, a clippy nie jest w CI ani w `npm run check`.

## 3. Ocena ogólna

Projekt jest w dobrej kondycji jak na rozmiar i tempo rozwoju. Mocne strony, potwierdzone przez kilku recenzentów niezależnie:

- Sekrety trafiają wyłącznie do `keyring`, komunikaty błędów są stałe i nie niosą tokenów ani adresów, w bazie nie ma sekretów, podgląd nie zapisuje tokenów w localStorage.
- Wszystkie żądania HTTP mają `redirect(Policy::none())` i limit 30 s, adresy budowane z danych użytkownika przechodzą walidację, Jira jest ograniczona do `*.atlassian.net`, OAuth kalendarza używa PKCE i `state`.
- SQL jest wszędzie parametryzowany, operacje na zadaniach i sesjach działają w transakcjach, logika dat ma testy na dni 23 i 25 godzin.
- MCP działa w trybie „tylko propozycje”, czat Claude ma `--tools ""`, procesy AI mają `kill_tree`, limit czasu i walidację odpowiedzi ze schematem.
- Granica IPC jest spójna: 153 komendy `invoke` mają handler, nazwy pól i opcjonalność zgadzają się z serde, w `src/` nie ma `any`, `tsconfig` ma `strict`.
- Warstwa komend przekazuje błędy do UI, a widoki je pokazują; composables mają liczniki żądań przeciw spóźnionym odpowiedziom.
- Testy: 688 w Rust z sensownymi asercjami i wstrzykiwanym czasem, ok. 580 w Playwright z izolacją i zamrożonym zegarem.

Największe długi, w kolejności wpływu:

1. **Odporność warstwy tła.** Aplikacja nie ma logu (stderr ginie przy `windows_subsystem="windows"`), pętle tła połykają błędy, zatruty Mutex bazy to awaria do restartu, jeden zły wiersz JSON unieruchamia migawkę.
2. **Granica zaufania wobec kopii zapasowej i AI.** Przywrócenie obcej kopii przenosi ścieżki uruchamianych programów; jednorazowe wywołania Codexa nie zamykają narzędzi; filtr prywatności cicho degraduje się.
3. **Model „cała migawka za każdym razem” i przeliczenia co sekundę.** Przy roku danych każdy klik to megabajty JSON w dwóch oknach, a widoki tygodnia i raportów liczą O(zadania × sesje) co sekundę.
4. **Logika w trzech kopiach.** Rust, symulator podglądu i fixture testów rozjeżdżają się (walidacja, idempotencja, zaokrąglanie), a `tests/` nie jest sprawdzane typami.
5. **Duplikacja między integracjami i rozmiar modułów.** Jira, Redmine, TS i kalendarz kopiują ten sam szkielet, `redmine.rs` ma 2190 linii produkcji, `main()` 357 linii.

## 4. Ustalenia do naprawy w pierwszej kolejności

Każde potwierdzone bezpośrednio w kodzie.

### 4.1 Bezpieczeństwo

**H1 · `src-tauri/src/backup.rs:32`, `settings.rs:441`, `orca.rs:681`, `ai.rs:240` · HIGH · przywrócenie kopii przenosi ścieżki programów, które aplikacja sama uruchamia.** `KEEP_KEYS` zachowuje z bieżącej bazy tylko `jira`, `timesheet`, `calendar`, `redmine`, `redmine_statuses_at`. Z obcego pliku `.sqlite` wchodzą więc `orca_path`, `ai_claude_path`, `ai_codex_path` oraz przełączniki `jira_write`, `redmine_write`, `ts_send`, `agent_hooks`, `ai_mcp` i lista `ai_hidden_names`. Walidacja ścieżki sprawdza tylko rozszerzenie i brak cudzysłowu, więc dopuszcza UNC. Po przywróceniu `orca::refresh_if_due` wykonuje `Command::new(program)` z `status --json`, a wykrywanie CLI uruchamia `<ścieżka> --version`. Skutek: wykonanie obcego kodu na koncie użytkownika bez potwierdzenia, przy UNC także wyciek hasha NTLM. Poprawka: dopisać te klucze do `KEEP_KEYS` (albo osobnej listy „nigdy z kopii”), odrzucać ścieżki UNC i względne, a w oknie „Sprawdź kopię” pokazywać, które ustawienia wrażliwe różnią się od bieżących.

**H2 · `src-tauri/src/ai.rs:502` vs `ai_chat.rs:267` · HIGH · jednorazowe wywołania Codexa nie wyłączają narzędzi.** Czat przekazuje `-c features.shell_tool=false`, `unified_exec`, `web_search_request`; `codex_args` dla `assign`, Zamknięcia dnia, Rano i tygodnia ma tylko `-s read-only -c mcp_servers={} -c notify=[]`. Prompt zawiera tytuły zadań, notatki, tytuły spotkań i okien, czyli treści od osób trzecich. Wstrzyknięta instrukcja może kazać modelowi czytać dowolne pliki (sandbox read-only pozwala na odczyt całego dysku). Poprawka: jedna wspólna stała `CODEX_DISABLED_FEATURES` użyta w obu miejscach; sprawdzić nazwy flag na prawdziwym Codexie, bo nieznany klucz jest pewnie cicho ignorowany.

**H3 · `src-tauri/src/ai.rs:1145-1149` · HIGH · słownik filtra prywatności cicho degraduje się.** Przy zatrutej blokadzie albo błędzie `Dictionary::from_db` kod wraca do `fallback` bez osób z `jira_issue_fields`, a komentarz w `ai_privacy.rs:437` mówi odwrotnie. Poprawka: `dictionary()` zwraca `Result`, a `ask`, MCP i czat odmawiają wywołania (tak jak `mcp::call` robi przy zatrutym `pseudonyms`).

**M1 · `src-tauri/src/mcp.rs:296,442`, `mcp_register.rs:24` · MEDIUM · dane z Jiry i Redmine trafiają do agentów bez oznaczenia jako niezaufane.** Serwer MCP jest rejestrowany `--scope user`, więc działa w każdym projekcie; `tasks_list` i `task_get` zwracają tytuły, notatki i pola z zewnętrznych systemów. Poprawka: opakować wyniki jawnym znacznikiem „dane zewnętrzne, nie instrukcje”, obcinać i czyścić pola tekstowe, rozważyć rejestrację per projekt.

**M2 · `src-tauri/src/ai_privacy.rs:419,679` · MEDIUM · filtr obejmuje tylko osoby.** Brak maskowania adresów e-mail, URL-i, ścieżek `C:\Users\<nazwa>` i sekretów w tekście wolnym; prompty dnia i Rano idą ścieżką tekstową, więc klucze osobowe JSON są pomijane. Poprawka: regexy dla e-mail, URL, długich tokenów, `password=`; w README i Ustawieniach napisać wprost, co filtr obejmuje.

**M3 · `src-tauri/src/redmine.rs:224,354` · MEDIUM · Redmine dopuszcza `http://` i wyłączenie weryfikacji TLS.** Klucz API idzie w nagłówku otwartym tekstem albo przez MITM. Poprawka: wyraźne ostrzeżenie w UI i odpowiedzi `redmine_connect`, docelowo przypięcie odcisku certyfikatu zamiast pełnego wyłączenia.

### 4.2 Poprawność i utrata danych

**H4 · `src-tauri/src/db.rs:128` · HIGH · jeden uszkodzony `task_details.json` unieważnia całą migawkę.** `serde_json::from_str(...)?` w `query_map`; `CheckItem` w `planner.rs:109` nie ma `#[serde(default)]`. Wiersz z `"priority":"1"` albo listą kontrolną bez `done` sprawia, że `get_snapshot`, każda mutacja i `backup::stage` zwracają `Err`, a UI nie pokazuje żadnego zadania. Poprawka: `Details::default()` z logiem przy błędzie parsowania (jak `templates::list`), `#[serde(default)]` na `CheckItem`.

**H5 · `src-tauri/src/planner.rs:261`, `src/TaskPlanning.vue:98-169`, `src/api/task.ts:64` · HIGH · zapis planu zadania podmienia cały JSON szczegółów.** `ON CONFLICT ... SET json=excluded.json` nadpisuje `pinnedAt`, `order`, `plannedDay`, `waiting*`, `jiraNoAsk` i `notes` zmienione przez inne komendy, gdy formularz był otwarty. Stare `pinnedAt>0` przypina ponownie bez sprawdzenia `MAX_PINNED=3`; stara notatka ze szkicu nadpisuje notatkę zapisaną w Zamknięciu dnia. `validate` nie sprawdza `order` ani `pinned_at`. Poprawka: `save` wczytuje bieżący wiersz i zachowuje pola backendowe, albo rozdzielić typ formularza od stanu zadania.

**H6 · `src/TaskPlanning.vue:224`, `src-tauri/src/planner.rs:15`, `src/preview/tasks.ts:189` · HIGH · puste pole szacunku wysyła `""` do `u32`.** `v-model.number` zostawia pusty napis, `saveDetails` nie normalizuje, Tauri zwraca surowy błąd serde w komunikacie; podgląd przyjmuje `""`. Poprawka: `estimateMinutes: Number(details.estimateMinutes) || 0` w `store.saveDetails`.

**H7 · `src-tauri/src/notify.rs:590-600,1114` i `:1038-1054`, `reminders.rs:570-598` · HIGH · przypomnienia są zużywane przed wyświetleniem.** `fire_due_tracked` zapisuje `claim` i zmienia tabelę (jednorazowe `enabled=0`), a potem `goal_ms_in` albo `day_total_ms_in` może zwrócić `Err` przez `?`, co wyrzuca cały wynik ticku (`unwrap_or_default` w pętli). Niezależnie od tego `let _ = toast.show()` ignoruje błąd wyświetlenia. Przy trwałym błędzie każdy tick zużywa kolejne przypomnienie bez śladu. Poprawka: reguły zwracają `Ok(out)` z błędem logowanym, `show()` z `Result` i wycofanie `claim` przy błędzie.

**H8 · `src-tauri/src/timesheet.rs:551-562,340-349,858` · HIGH · tryb „zachowaj” wysyła do TS wiersze z pustymi godzinami.** `value()` robi `unwrap_or("")`, więc gdy TS zmieni klasę inputu godzin, `parse_filled` zwraca wpisy z `hours=""`, `merge_keep` dokleja je, a `save_day` zastępuje cały dzień: wpisy TS spoza listy dostają 0 h. `confirmed` i `saved_differs` tego nie wykryją. Poprawka: w trybie keep przerwać z błędem, gdy wiersz z `id` nie ma inputu godzin; porównać sumę godzin sprzed i po zapisie.

**H9 · `src/updater.ts:136-143`, `src-tauri/src/main.rs:205-214` · HIGH · aktualizacja zatrzymuje pomiar przed instalacją i nie wznawia go po błędzie.** `prepare_update` robi `db::stop`, potem `update.install()`; gdy instalator nie ruszy (UAC, AV), `catch` ustawia tylko komunikat, a czas pracy przestaje być liczony. Poprawka: zapamiętać aktywne zadanie i wznowić je w `catch`, albo dopisać „pomiar został zatrzymany”.

**H10 · `src/QuickSwitch.vue:182-187` · HIGH · Enter w szybkim przełączaniu ignoruje cel fokusu.** Handler na całej sekcji uruchamia zadanie wybrane strzałką (domyślnie pierwsze), gdy użytkownik przeszedł Tabem na inny wynik, chip albo „Zamknij”; `preventDefault` anuluje natywną aktywację. Przy braku wyników odpala „Utwórz i rozpocznij”. Poprawka: obsługiwać Enter tylko dla `e.target === input.value`.

**H11 · `src/report.ts:13` vs `src/export.ts:89`, `src-tauri/src/export.rs:126` · HIGH · dwie reguły zaokrąglania godzin.** `decimalHours` używa `toFixed(2)` na floacie, eksport CSV/XLSX i MCP używają całkowitoliczbowego half-up. Sprawdzone: 54 000 ms daje „0,01” kontra „0,02”, 3 618 000 ms „1,00” kontra „1,01”; różnią się dla ok. 1,3% czasów. Wpisy do TS (`tsEntries` korzysta z `decimalHours`) i Raport TS mogą odbiegać od CSV tego samego dnia. Żaden test tego nie łapie. Poprawka: jedna funkcja na setnych godziny w `api/timesheet.ts`, wspólny wektor testowy czytany też w Rust.

**H12 · `tests/fixtures/tauri.ts:624-640` · HIGH · fixture `agent_switch` nie jest idempotentne jak Rust.** Zawsze zatrzymuje i wstawia nową sesję, także dla trwającego zadania; ścieżka `key` rzuca błąd. Test zielony, aplikacja robi co innego. Poprawka: wspólna funkcja `start` w fixture z zachowaniem Rust.

**H13 · `src-tauri/src/idle.rs:73-100,141-199` · HIGH (do sprawdzenia) · brak wykrywania uśpienia.** Bezczynność liczy tylko `GetLastInputInfo` co 2 s. Zamknięcie klapy przed 10 min bezczynności i powrót po 8 h najpewniej zeruje czas bezczynności, więc `candidate` nie powstaje i 8 h wchodzi do pomiaru. Poprawka: porównywać `now` z poprzednim tickiem i lukę powyżej progu traktować jako przerwę; alternatywnie `WM_POWERBROADCAST`.

**H14 · `src/components/AiChat.vue:62-73`, `AiProposals.vue:57-63` · HIGH · zastosowana propozycja wraca do stanu „otwarta”.** Po udanym `applyProposal` błąd `mcpProposalResolve` cofa kartę, nie emituje `updated`; drugie „Zastosuj” duplikuje zadanie albo notatkę. Poprawka: emitować `updated` po zapisie, błąd `resolve` traktować osobno.

### 4.3 Odporność i obserwowalność

**H15 · `src-tauri/src/main.rs:1,754` i 9 miejsc `eprintln!` · HIGH · aplikacja nie ma logu.** Przy `windows_subsystem="windows"` stderr nie ma dokąd trafić. Błąd w `setup` (migracja, `Connection::open`, `backup_before_upgrade`, `db::init`) kończy się panic bez okna; błąd serwera potoku (`hooks.rs:691`, `browser.rs:613`, `mcp.rs:949`) oznacza, że agenci i rozszerzenie milkną, a Ustawienia pokazują „połączone”. Poprawka: plik logu w `app_data_dir` (`tauri-plugin-log` albo `OpenOptions::append`), natywny dialog przy błędzie startu, błędy serwerów potoku w statusie widocznym w UI.

**H16 · `src-tauri/src/main.rs:89,102,130` i ok. 25 miejsc `lock().ok()` · HIGH · zatruty Mutex bazy to trwała awaria.** Jedna panika pod blokadą sprawia, że każda komenda zwraca „poisoned lock”, a pętle tła (idle, notify, tray, agenci, reminder) po cichu nic nie robią do restartu. `settings::Cache::slot`, `jira.rs:95` i `ai_chat.rs:556` już robią `unwrap_or_else(|e| e.into_inner())`. Poprawka: wspólny `lock_db(&state)` z `into_inner()` i logiem; `tray::quit` ma kończyć aplikację także przy błędzie `db::stop` (`tray.rs:272-293`), bo dziś „Zakończ aplikację” nie działa przy błędzie bazy.

**H17 · `src-tauri/src/redmine.rs:2096-2100,2175-2185` · HIGH · obserwacja Redmine nie zapisuje błędów.** `api_key(..).ok()?` i `if let Ok(..) = result` bez gałęzi Err; Jira ma `record_watch` i linię stanu. Wygasły klucz oznacza tygodnie bez importu i bez sygnału. Poprawka: `RedmineState::record_watch`, zdarzenie i linia stanu jak `JiraWatchLine.vue`.

**M4 · `src-tauri/src/jira.rs:1199`, `jira_board.rs:1274`, `redmine.rs:1531` · MEDIUM · błąd po wysłaniu zapisu wygląda jak błąd połączenia.** Timeout po utworzeniu zgłoszenia daje „Nie można połączyć się”, użytkownik ponawia i powstaje duplikat zgłoszenia albo komentarza. Poprawka: rozpoznawać `is_timeout()` i błędy po wysłaniu, komunikat „Nie wiadomo, czy zapisano. Sprawdź przed ponowieniem”.

**M5 · `src-tauri/src/notify.rs:316-319` · MEDIUM · stary toast z Action Center bezwarunkowo przełącza pomiar.** `AgentSwitch`, `FocusResume`, `JiraStart`, `RedmineStart` nie sprawdzają wieku ani stanu; `Stop` ma `continuation`. Poprawka: znacznik czasu w argumencie przycisku, ignorować po np. 15 min albo gdy trwa inny pomiar.

**M6 · `src-tauri/src/main.rs:428` · MEDIUM · callback single-instance woła `app.state::<Database>()` przed `manage`.** Drugi proces `--prepare-update` podczas wolnego startu daje panikę. Poprawka: `try_state`.

**M7 · `src-tauri/src/main.rs:458-466` · MEDIUM · błąd `backup_before_upgrade` przerywa `setup`.** Pełny dysk albo blokada antywirusa oznacza, że nowa wersja nie startuje i nic nie pokazuje. Poprawka: log i `storage-error` zamiast `Err` ze `setup`.

## 5. Ustalenia według obszarów

Powtórzenia między recenzentami scaliłem. Numery linii dotyczą stanu `master` aa747f7.

### 5.1 Backend: rdzeń i warstwa danych

- `recurrence.rs:105`, `main.rs:566` · MEDIUM · `generate` parsuje JSON wszystkich zadań (także ukończonych) co 60 s pod blokadą i przerywa się przy pierwszym złym wierszu; wątek emituje `storage-error` co minutę, a żaden cykl nie powstaje. Poprawka: `continue` z logiem, filtr wzorców w SQL przez `json_extract`.
- `recurrence.rs:123-131` · MEDIUM · błędny zegar systemowy (np. 2030) tworzy setki wstecznych wystąpień i ustawia `last_day` w przyszłości, co blokuje wzorzec po naprawie zegara. Poprawka: `last_day` w przyszłości traktować jako uszkodzone.
- `db.rs:59-106` · MEDIUM · brak wersjonowania schematu (`PRAGMA user_version`, `ALTER`), tylko `CREATE ... IF NOT EXISTS` rozsiane po `days.rs`, `meetings.rs`, `reminders.rs`, `templates.rs`, `week_notes.rs`. Pierwsza zmiana kolumny istniejącej tabeli nie ma ścieżki.
- `db.rs:108-168`, `assign.rs:518-527,570` · MEDIUM · każda mutacja zwraca pełną migawkę z całą historią sesji; `assign::spawn` i `ts_request` budują ją tylko po to, by wyjąć jedno zadanie, `ts_request` pod blokadą. Poprawka: zapytania celowane, okno sesji (np. 400 dni) albo delty.
- `backup.rs:144-147` · MEDIUM · `export_backup` trzyma blokadę bazy przez cały zapis na dysk docelowy (plus `sleep` w `run_to_completion`); dysk sieciowy blokuje tray, widget i komendy. Poprawka: `drop(db)` po kopii do pamięci.
- `backup.rs:82-135,214` · MEDIUM · przywracanie przenosi triggery i widoki z nieufnego pliku; `db::init` z `IF NOT EXISTS` ich nie nadpisze. Poprawka: porównać `sqlite_master` z listą oczekiwaną.
- `backup.rs:54-58` · MEDIUM · `carry_connections` robi `query_row(...).ok()`, więc błąd odczytu `jira`/`calendar`/`timesheet`/`redmine` usuwa konfigurację połączeń z kopii. Poprawka: `.optional()?`.
- `backup.rs:211-215` · MEDIUM · błąd `replace` zwraca gołe `Err` bez ścieżki kopii `before-restore-*`.
- `maintenance.rs:27`, `backup.rs:183-186,211` · MEDIUM · kopie `before-<wersja>-*`, `before-restore-*`, `restore-candidate-*` nigdy nie są usuwane; przy błędzie `db::init` po kopii każdy start dokłada pełną kopię bazy. Poprawka: retencja (np. 5) i usuwanie kandydata po przywróceniu.
- `legacy.rs:215-223` · MEDIUM · migracja kopiuje bazę, `-wal` i `-shm` zwykłym `copy`, nieatomowo względem działającego starego procesu. Poprawka: `VACUUM INTO` albo Backup API do pliku tymczasowego, potem `rename`.
- `planner.rs:216-264`, `db.rs:190-546` · MEDIUM · zapytanie „zadanie nie w koszu” skopiowane kilkanaście razy. Poprawka: helpery `task_visible` i `trashed`.
- `planner.rs:657-679` · MEDIUM · `add_session` nie sprawdza istnienia ani kosza; nieistniejące zadanie daje surowe „FOREIGN KEY constraint failed”.
- `settings.rs:594-602,689`, `capacity.rs:123,141`, `days.rs:204-210,399-405` · MEDIUM · błędy odczytu zamieniane na wartości domyślne i cache'owane do końca sesji (`jira_write=true`, `idle_enabled`, cel dnia bez spotkań, „dzień niezakończony”). Poprawka: `?` i `.optional()`, nie cache'ować po błędzie.
- `suggest.rs:433`, `jira_fields.rs:311` · MEDIUM · `settings::load` zamiast `settings::cached` w pętli co 5 s i w każdym `snapshot`.
- `db.rs:177-282`, `main.rs:106-111` · LOW · `action: String` w `mutate`, listy akcji w trzech miejscach. Poprawka: `enum Action` z serde.
- `planner.rs:119-123`, `settings.rs:291`, `week_notes.rs:16` · LOW · trzy walidatory dat o różnych regułach (`"2026-+1-+5"` przechodzi `date_valid`); w MCP `day_arg` i `propose_plan` używają różnych. Poprawka: jedna funkcja na `NaiveDate`.
- `export.rs:109` · LOW · neutralizacja formuł CSV pomija `\t` i `\r`.
- `backup.rs:142-143` · LOW · ochrona „inny plik niż bieżąca baza” porównuje ze ścieżką nowego katalogu, więc w stanie `DirState::Failed` można nadpisać otwartą bazę.
- `db.rs:296,333` · LOW · każdy błąd `query_row` raportowany jako „Nie znaleziono przedziału”.
- `main.rs:405-762` · wielkość: `main()` 357 linii, `create_linked` 170, `notify::perform` 139, `validate_fields` 119, `export::xlsx` 116, `db::change` 106.

### 5.2 Backend: pulpit, powiadomienia, pętle tła

- `notify.rs:1109-1127` · HIGH (scalone z H7) · pętla powiadomień połyka każdy błąd bazy (`unwrap_or_default`, `if let Ok`), wszystkie powiadomienia milkną bez komunikatu.
- `reminder.rs:90-94`, `idle.rs:148-156`, `tray.rs:491-499`, `focus.rs:715-717`, `shortcuts.rs:191`, `window_state.rs:104` · MEDIUM · zatrucie mutexa i błędy w pętlach bez śladu; wątki bez `catch_unwind`, więc panika w iteracji kończy wątek na zawsze. Tray przy błędzie pokazuje stary stan „mierzy”.
- `focus.rs:333-336,721-723` · MEDIUM · `focus::flush` kasuje kolejkę `focus_queue` przed wysłaniem toastu.
- `reminders.rs:623,627`, `capacity.rs:124,141` · MEDIUM · `meetings_between(...).unwrap_or_default()` i `meeting_overrides(...).unwrap_or_default()`: błąd odczytu wysyła przypomnienie o spotkaniu, które użytkownik wyłączył, a cel dnia liczy się bez spotkań.
- `main.rs` (komendy), `focus.rs:749`, `autostart.rs:110-132`, `shortcuts.rs:156-186` · MEDIUM · prawie wszystkie komendy to synchroniczne `fn` na wątku głównym (tylko `hooks.rs:783` ma `command(async)`); zajęta blokada bazy (tick notify, backup, import) zamraża UI i skrót Start/Stop. `autostart_set` trzyma blokadę przez zapis do rejestru. Poprawka: `#[tauri::command(async)]` dla komend z blokadą lub IO.
- `idle.rs:121`, `foreground.rs:517`, `browser.rs` (FFI) · MEDIUM · bloki `unsafe` bez `// SAFETY:`, deklaracje `extern "system"` zduplikowane; poprawność FFI sprawdzona (`wrapping_sub` dla `GetTickCount`, `CloseHandle` po `OpenProcess`).
- `jira_move.rs:302-311,339` · MEDIUM · `let _ = check(...)` odrzuca 401 i błędy sieci, pytanie o przesunięcie po prostu się nie pojawia.
- `focus.rs:360-379` · LOW · `Focus::start` nieatomowy: osierocony blok `completed IS NULL` nigdy się nie domyka.
- `tray.rs:337-363` · LOW · `set_pins` przy błędzie w środku pętli zostawia `count` bez aktualizacji i może kasować stałe pozycje menu.
- `reminder.rs:37-48,96-108` · LOW · sztywny rozmiar 390×230 zamiast `outer_size()`, pominięty fallback na `primary_monitor`, `hide()` co 2 s także gdy okno już schowane.
- `reminder.rs` vs `reminders.rs` · LOW · dwa moduły o prawie tej samej nazwie (brak pomiaru vs przypomnienia własne), `crate::reminder::reset` wołane z 13 miejsc. Propozycja: `measurement_nag.rs`.
- Duplikaty: `hm()` w `tray.rs` i `notify.rs`, odmiana liczebników w dwóch miejscach, „okno w tle” liczone ręcznie w `reminder.rs` mimo `widget::main_background`, SQL `task_exists` w `reminders.rs` i `meetings.rs`.

### 5.3 Backend: integracje HTTP

- `redmine.rs:2103,856-871,563-567` · MEDIUM · pusty cache statusów przy błędzie `/issue_statuses.json` plus Redmine bez `status.is_closed` importuje zamknięte zgłoszenia jako nowe zadania.
- `redmine.rs:2106-2108,1987-1995` · MEDIUM · przy ponad 500 otwartych zgłoszeniach (`MAX_ISSUE_PAGES=5`) ostrzeżenie o obcięciu jest odrzucane, `sync_assigned` zeruje `assigned` dla całej witryny, a starsze zgłoszenie po aktualizacji wraca jako „nowe”.
- `calendar.rs:526-567,707` · MEDIUM (do sprawdzenia na żywym Graph) · z `Prefer: outlook.timezone="UTC"` wydarzenie całodniowe 16.10 może przyjść jako `2026-10-15T22:00:00`, więc `date()` daje 15.10: urlop przesunięty o dzień, wpływ na pojemność i powiadomienia.
- `jira.rs:378-386`, `redmine.rs:1150`, `timesheet.rs:1024` · LOW · `let _ = entry(&app,&old)?.delete_credential()`: `?` po zapisie konfiguracji może pominąć `clear()` cache i `reset_watch`, a `let _` ukrywa stary sekret w Credential Manager.
- `jira.rs:249`, `redmine.rs:350`, `timesheet.rs:605`, `calendar.rs:390` · LOW/MEDIUM · nowy `reqwest::Client` przy każdym poleceniu i każdym obiegu obserwacji (39 wywołań): brak puli połączeń, pełny handshake TLS, ładowanie magazynu certyfikatów; brak `connect_timeout`. Poprawka: `OnceLock<Client>` per usługa.
- `jira.rs:1396`, `redmine.rs:1399` · LOW · walidowany jest `parsed`, a do `open_url` trafia surowy `url`.
- `jira_board.rs:1037-1058`, `jira_fields.rs:385` · LOW · po `jira_assign` na inną osobę kafelek pokazuje poprzedniego przypisanego do kolejnego pobrania.
- `calendar.rs:196` · LOW · `error_description` wstawiane do HTML bez escape (wymaga poprawnego `state`, praktycznie niewykonalne); `calendar.rs:207-248` `request_line` blokuje do 5 s na połączeniu bez danych.
- `calendar.rs:504,511,816` · LOW/MEDIUM · `let _ = entry.set_password(next)` przy rotacji refresh tokenu i `let _ = store(&db,&saved)` w `save_result`: błędy zapisu tokenu i statusu synchronizacji giną.
- `jira.rs:295`, `redmine.rs:497`, `timesheet.rs:803`, `calendar.rs:720` · LOW · odpowiedzi czytane bez limitu rozmiaru (adres Redmine i TS konfiguruje użytkownik).
- `jira.rs:824`, `jira_board.rs:312`, `jira_fields.rs:247` · LOW · `out.iter().any(...)` w pętli, O(n²) przy obserwacji do 20 000 wyników.
- `redmine.rs:528-546` · LOW · stronicowanie `offset` przy `sort=updated_on:desc` na zmieniających się danych gubi zgłoszenia.
- `timesheet.rs:345` · LOW · `merge_keep` wycina wpisy TS o tym samym zadaniu, więc drugi ręczny wpis znika; odczyt i zapis to dwa żądania, więc równoległa edycja w przeglądarce zostanie nadpisana.
- Duplikacja Jira/Redmine (`start_issue`, `watch_once`, `imported_message`, `start`, `entry`, `ensure_write`, `send_write`): dowód rozjazdu to `INSERT INTO task_details` z `ON CONFLICT` w `jira.rs:758` i bez w `redmine.rs:1978`.
- Rozmiar: `redmine.rs` 2190 linii produkcji w 8 tematach, `calendar::connect` 107 linii.

### 5.4 Backend: AI, MCP, hooki, rozszerzenie

- `ai_privacy.rs:679-694` · MEDIUM · `redact_with` traktuje wejście jako JSON tylko gdy zaczyna się od `{`/`[`; prompty dnia i Rano idą ścieżką tekstową, która nie ukrywa e-maili ani loginów (`jan.Osoba 1@firma.pl`).
- `mcp_wire.rs:325`, `hook_wire.rs:88`, `bin/bridge.rs:83` · MEDIUM · klient potoku czeka na odpowiedź bez limitu czasu; zawieszona aplikacja blokuje `tools/call` bezterminowo, a synchroniczny hook PreToolUse opóźnia każde narzędzie o pełne 5 s.
- `ai_chat.rs:400,408` · MEDIUM · `lines().map_while(Result::ok)` przerywa czytanie na pierwszej linii nie-UTF-8; bufor stdout się zapełnia, CLI stoi w `write`, tura wisi do „Zatrzymaj”. `ai.rs:405-413` robi to poprawnie (`read_to_end` + `from_utf8_lossy`).
- `hooks.rs:628` z `agents.rs:603` · MEDIUM · `hooks::retry` robi synchroniczne I/O na `~/.claude/settings.json` i `~/.codex/hooks.json` w jedynym wątku pętli agentów co 60 s; profil na OneDrive zawiesza listę agentów i propozycje.
- `mcp.rs:921-934,499,823` · MEDIUM · `call()` z `redact_with` działa pod globalnym mutexem bazy; `task_get` ładuje wszystkie sesje z historii dla jednego zadania.
- `browser.rs:766-795,671-824` · MEDIUM · licznik `active` zwalniany tylko na normalnej ścieżce (panika w handlerze podnosi go na stałe, po 16 potok odrzuca wszystkich), `write_frame` bez limitu czasu, pętla bez `sleep` przy trwałym błędzie `ConnectNamedPipe` (rdzeń CPU), `CreateNamedPipeW` z domyślnym DACL.
- `orca.rs:108-111` · MEDIUM · po timeoucie tylko `child.kill()` na `cmd.exe`, potomek żyje dalej; `ai.rs` ma `kill_tree`.
- `ai.rs:1090-1092`, `ai_chat.rs:850` · MEDIUM · `classify` szuka „429”, „401”, „login” w `stderr + stdout`, więc `"duration_ms":14290` daje „Limit subskrypcji wyczerpany”.
- `mcp_register.rs:264` · MEDIUM · `Step::Replace` robi `remove` i `add` nieatomowo.
- `day_assist.rs:185-211` · MEDIUM · `git config` i `git log` bez limitu czasu dla do 20 folderów; `.git/config` z `log.showSignature=true` i `gpg.program` wykona obcy program przy Zamknięciu dnia. Poprawka: `-c log.showSignature=false -c core.fsmonitor=false`, runner z timeoutem.
- `agent_time.rs:306-308` · MEDIUM · `let _ = record(...)`: czas agentów przestaje się zapisywać bez śladu.
- `mcp.rs:1039` · MEDIUM · błąd `create_dir_all` zamieniony na „nic nie rejestruj”, `run_sync` zwraca sukces z pustą listą.
- `hooks_config.rs:93-96` · LOW · `is_own` sprawdza `contains("tuktuk-track-hook")` na całym poleceniu, więc cudzy hook ze ścieżką zawierającą ten fragment zostanie usunięty.
- `hook_wire.rs:16`, `browser.rs:814` · LOW · potok hooków bez sufiksu użytkownika (RDS blokuje drugiego użytkownika; proces uruchomiony wcześniej przejmuje nazwę i dostaje początki promptów); hook wysyła `prompt` zawsze, nawet gdy `keep_prompts=false`.
- `hooks_config.rs:486` · LOW · zapis `settings.json` bez porównania mtime i bez kopii; `rename` zastępuje symlink zwykłym plikiem.
- `hooks_config.rs:103` · LOW · cudzysłowy nie chronią przed `$` i backtickiem w bashu.
- `mcp.rs:754-759` · LOW · `estimate` niebędące `i64` (np. `30.0`, `"30"`) cicho pomijane.
- `mcp.rs:257-293` · LOW · rozwiązanie zadania po tytule przez `ps.restore(title)` zawodzi przy odmienionych nazwiskach; komunikaty błędów zwracają przywrócony tekst.
- `native_msg.rs:116`, `mcp_wire.rs:38` · LOW · nazwa potoku z `USERNAME` procesu; launcher bez tej zmiennej łączy się z `...-user`.
- `mcp_register.rs:91-98` · LOW · indeks bajtowy z `to_lowercase()` użyty na oryginalnym napisie (możliwa panika przy `İ`).
- `orca.rs:691-700` · LOW · chwilowy brak Orki czyści mapę folderów; flaga `busy` może zostać `true` na zawsze.
- `ai.rs:25`, `ai_privacy.rs:28` · LOW · blokowe `#![allow(dead_code)]` bez powodu (zasadne tylko w plikach dołączanych przez `#[path]` do binarek).
- `main.rs:428` · LOW · każdy proces konta z `--prepare-update` zatrzymuje pomiar i zamyka aplikację.
- `tauri.conf.json` (CSP) · LOW · `connect-src` z hostami GitHuba jest zbędne, bo updater pobiera przez Rust; `style-src 'unsafe-inline'`. `AiSection.vue:176` ma `<a target=_blank>` bez uprawnienia `opener` (do sprawdzenia, czy linki się otwierają).
- Duplikacja: pętla klienta potoku z `PIPE_BUSY` w trzech plikach, dwa runnery procesów (`ai::Process`, `orca::Process`), helpery `clip`/`minutes`/`put`/`hm`/`key_of` w trzech modułach, to samo zapytanie o sesje dnia w `mcp.rs:332`, `day_assist.rs:269`, `plan_assist.rs:666`.
- Angielskie komunikaty narzędzi MCP (`mcp.rs:251,292,715,796,895`): „Invalid key: use a Jira key such as ABC-12”, wbrew zasadzie polskich komunikatów.

### 5.5 Ciche błędy: statystyka

| Wzorzec (kod produkcyjny) | Liczba | Uzasadnione |
|---|---|---|
| `let _ =` (Rust) | ok. 157 | ok. 140 (77 to `app.emit`, ok. 45 to sprzątanie plików, okien, procesów) |
| `.ok()` (Rust) | 145 | ok. 105 |
| `unwrap_or_default()` / `unwrap_or(false\|0\|None)` | 138 | ok. 100 |
| `eprintln!` jako jedyny raport | 9 | 1 |
| puste `catch {}` (TS/Vue bez preview) | ok. 19 | ok. 12 |
| `.catch(() => ...)` | 33 | ok. 24 |
| `void promise()` | 101 | ok. 95 |
| pętle tła `thread::spawn` z `loop` | ok. 12 (46 `thread::spawn` łącznie) | 4 z zapisem błędu |

Frontend, wartości zastępcze maskujące błąd: `useMeetings.ts:33-37` (błąd daje `connected=false, events=[]`, plan dnia pokazuje „wolny dzień”), `store.ts:166-192` (`refreshJiraConnected`/`refreshRedmineConnected` przy błędzie ustawiają `false`, więc `#1234` w szybkim dodawaniu zmienia znaczenie), `focus.ts:355-442` (`blocks=[]` daje „0 przerw”), `IdleBanner.vue:61-67` (pusty `catch`, baner przerwy nigdy się nie pokaże), `useTsSend.ts:72-79` (błąd `ts_sent` ukrywa znacznik „Wysłano do TS”, ryzyko podwójnej wysyłki), `updater.ts:88-92` (nieudane sprawdzenie blokuje kolejne na 24 h).

Skrypty wydania `scripts/ship.mjs:146,162,224-246`, `release.mjs:147`: `read()` ignoruje `.ok` (kontrola czystości drzewa pomijana, gdy `git` zawiedzie), publikacja release w GitHubie następuje przed `git commit`/`tag`/`push` (po błędzie ponowny `ship` zatrzymuje się na „release już istnieje”), każdy niezerowy status `gh release view` traktowany jako „nie istnieje”, hasło klucza podpisu trafia do `env` wszystkich kroków (także Playwright i `cargo test`), `run()` gubi `result.error` przy ENOENT.

### 5.6 Frontend: widoki i komponenty Vue

- `App.vue:663,214-220,571-575` · MEDIUM · `storage-error` i błąd Stop przy otwartym modalu są ukryte przez `!modalOpen`, a `closeModal` czyści `error`; użytkownik nie dowiaduje się, że dane się nie zapisały.
- `DayClose.vue:59-67`, `WeekReview.vue:186-205,247-263` · MEDIUM · niezapisane notatki dnia i „Wnioski” tygodnia nadpisywane bez ostrzeżenia przy zmianie dnia lub zakładki; w `WeekReview` w oknie ładowania można zapisać tekst starego tygodnia pod nowym.
- `App.vue:318-326`, `WidgetWindow.vue:442-452` · MEDIUM · `refresh()` bez numeru żądania, spóźniona odpowiedź nadpisuje nowszy stan (wzorzec z tokenem istnieje w composables).
- `ConfirmDialog.vue:21`, `TsSendDialog.vue:588`, `OrcaWorktreeDialog.vue:182` · MEDIUM · `@click.self` na overlayu zamyka dialog po zaznaczaniu tekstu przeciągnięciem i puszczeniu nad overlayem; niezapisany formularz przepada.
- `TsSendDialog.vue:352-363,564` · MEDIUM · „Podpowiedz wszystkie” nie przerywa się po zamknięciu dialogu, zużywa limit subskrypcji Claude/Codex.
- `TsSendDialog.vue:302-367,438` · MEDIUM · podpowiedzi AI kluczowane samym `task.id`, nie parą dzień+zadanie; „Użyj podpowiedzi” może zapisać zadanie TS nieobecne tego dnia.
- `TsSendDialog.vue:1113-1124`, `useModal.ts:3,18-30` · MEDIUM · po „Zapisz w TS” fokus ucieka na `body` (przycisk `disabled`, potem `v-if`); selektor pułapki pomija `select`, `textarea`, `[tabindex]`, brak `inert` na tle.
- `JiraBoard.vue:132-177` · MEDIUM · komponent mutuje obiekty z propa `board.issues`; odświeżenie z innego okna w trakcie przeciągania trafia w porzucone obiekty.
- `IdleBanner.vue:4,29-60`, `LibraryView.vue:88-203`, `issueMoves.ts:69`, `jiramove.ts:102`, `ReminderWindow.vue:31`, `App.vue:356`, `WidgetWindow.vue:468` · MEDIUM · bezpośrednie `invoke` poza `store.ts` (51 wywołań w 16 plikach), wbrew zasadzie cienkiej warstwy; `IdleBanner` bez blokady `busy` (podwójne kliknięcie wysyła dwa `idle_resolve`).
- `JiraView.vue:185`, `RedmineView.vue:188`, `JournalView.vue:195`, `DayClose.vue:193`, `MorningReview.vue:133`, `LibraryView.vue:67` · MEDIUM · wrapper `run(fn)` zaimplementowany 6–8 razy niespójnie: część bez guardu `busy`, część z `String(e)` (daje „Error: …” dla wyjątków z podglądu; 49 użyć `String(e)` w `src/`). Poprawka: `useAction()` z `busy`, `error`, `errorText`.
- `OrcaWorktreeDialog.vue:160-176` · LOW · po sukcesie „Utwórz worktree” zostaje aktywny, Esc w trakcie `busy` zamyka okno.
- `DayAssistant.vue:208-223` · LOW · `addAllNotes` bez ochrony `id === request`, karty nowej odpowiedzi oznaczane „Zastosowano”.
- `TaskPlanning.vue:327` · LOW · `v-for` z `:key="index"` na liście kontrolnej (wędrujący fokus po usunięciu kroku).
- `App.vue:552-578` · LOW · jeden `try` na pięć `await listen(...)`; błąd wcześniejszego pomija rejestrację `storage-error`; `refresh()` czeka na wszystkie.
- `main.ts:28` · LOW · brak `app.config.errorHandler` i `unhandledrejection`; okna widget i quick nie mają baneru błędu.
- `WidgetWindow.vue:514-517` · LOW · `restore()` bez `try/catch`.
- `JournalView.vue:185-190` · LOW · `plan()` zeruje wybór niezależnie od wyniku.
- `AiChat.vue:152-156` · LOW · każda zmiana strumienia przewija log na dół (brak pin-to-bottom).
- `useReminders.ts:42` · LOW · literał `"preview-data-changed"` zamiast stałej `previewDataEvent`.
- `JiraView.vue:62` · LOW · adres firmowego tenanta Jiry wpisany na sztywno jako domyślny `site`.
- `useFocus` (`focus.ts:382-399`), `agents.ts:462-474`, `AppNav.vue:177-189`, `TitleBar.vue:28`, `TsTaskField.vue:85-101` · LOW · nasłuchy rejestrowane po `await` bez flagi `alive`, listener ucieka przy wyścigu z odmontowaniem; `TsTaskField.load` bez licznika żądań.
- `aiproposals.ts:62-85` · MEDIUM · `note_append` i `plan` liczą nową wartość ze starej migawki `tasks` i nadpisują całą notatkę dnia.
- `api/aichat.ts:138-143` · LOW · „Popraw” zamienia dzień na `@dd.mm` bez roku; dzień przeszły ląduje w przyszłym roku.
- Architektura: `App.vue` 822 linii (skróty, „Rano”, listenery Tauri, pięć flag żądań), `WidgetWindow.vue` 793 (tryb malutki, propozycje), `JournalView.vue` 777 (trzy flagi `morning`/`closing`/`weekly` tworzące stany niemożliwe), `TsSendDialog.vue` 1127 (cztery odpowiedzialności, logika podpowiedzi zduplikowana z `TsTaskField.vue`). Raporty: `MonthReport` i `MultiDayReport` mają identyczne `monthOf`, `shiftMonth`, `closedTotals`, `totalOf`; „suma dnia” liczona czterema wzorami (`MonthReport`, `MultiDayReport`, `DashboardView`, `WeekReview`) plus piąty w Rust. `JiraIssueActions` i `RedmineIssueActions` prawie identyczne.

### 5.7 Frontend: TypeScript, podgląd, rozszerzenie, konfiguracja

- `browser-extension/background.js:25,66-148` · MEDIUM · `domains` startuje jako `DEFAULT_DOMAINS`, a `send()` nie czeka na `loadDomains()`; po wybudzeniu service workera wysyła adres i tytuł z domeny usuniętej przez użytkownika albo `clear` dla domeny dodanej. Poprawka: `const ready = init()` i `await ready` w `send()`.
- `background.js:104-106`, `lib.js:101-110` · LOW · `port = null` bez `disconnect()` (drugi proces hosta), pełny URL z query i fragmentem zamiast `origin + pathname`.
- `tsconfig.json:15` · MEDIUM · `include` nie obejmuje `tests/**`, `playwright.config.ts` ani `scripts/`; `tests/fixtures/tauri.ts` (1149 linii, 64 użycia `any`) nie jest sprawdzany typami. Poprawka: `tests/tsconfig.json` i krok w `npm run build`/`check`.
- `src/preview/tasks.ts:409-466,547-557,653` · MEDIUM · podgląd nie odtwarza `planner::validate` (priorytet, szacunek, lista kontrolna, daty, długości), `setPriority` bez zakresu, `closeDay` bez walidacji dnia, `change("nieznana")` cichy no-op. Poprawka: wspólna `validateDetails` w `src/api/task.ts`.
- `src/preview/snapshot.ts:12` · MEDIUM · `JSON.parse(raw)` bez `try/catch`; uszkodzony localStorage wywraca cały podgląd.
- `src/updater.ts:90,164` · MEDIUM · `lastCheckAt` ustawiane też w `catch`, nieudane sprawdzenie blokuje kolejne na dobę.
- `src/focus.ts:382-399` · LOW · `useFocus` cleanup przed rozwiązaniem `listen`.
- `playwright.config.ts` · LOW · brak `forbidOnly: !!process.env.CI`, `retries`, `trace: "on-first-retry"`; `settings.ts` ma dyrektywy `eslint-disable` bez ESLinta w repo.
- `src/store.ts` · 1197 linii, ok. 130 ręcznych powtórzeń wzorca `desktop ? invoke : podgląd`; nagłówek wymienia 5 domen `src/api`, a jest ich 14.
- Martwy kod TS: tylko 4 z 698 eksportów bez użyć (`SNOOZE_MINUTES`, `meetingReminderOptionLabel`, `noCorrection`, `previewFirstPrompt`).

### 5.8 Typy i granica IPC

Zgodność nazw, `rename_all`, opcjonalności i enumów z tagiem jest potwierdzona. Słabości to modelowanie stanów:

- `db.rs:177` `action: String` w `mutate` (osiem akcji, nielegalne kombinacje reprezentowalne, literówka przechodzi kompilację); `mcp.rs:70-85` `Proposal` z `kind: String` i polami „na wszelki wypadek”; `planner.rs:57` `RepeatRule` płaska z duplikatem normalizacji; `settings.rs:128` `aiEngine: String` mimo istniejącego `ai::Engine`; `ai.rs:885` i `hooks.rs:705` `String` zamiast istniejących enumów. Poprawka: unie z tagiem po obu stronach (`#[serde(tag="kind")]`).
- `store.ts:1188,1040-1065` · MEDIUM · identyfikatory jako goły `number`: `redmineStart(task.id)` zamiast `redmineId` kompiluje się; pary `redmineTransitions(taskId)` i `redmineIssueTransitions(issueId)` różnią się tylko nazwą. Poprawka: typy brandowane w TS, newtype w Rust.
- `api/task.ts:114-119`, `db.rs:62` · MEDIUM · `Session` nie koduje „trwa”/„zakończona”, brak `CHECK(ended_at >= started_at)`, każdy konsument filtruje inaczej; zerodługi przedział ze `stop` nie da się edytować.
- `api/jira.ts:28-48`, `api/redmine.ts:76` · MEDIUM · `JiraMovePrompt` przeciążony dla Redmine (`key="#1234"`, `transitionId` jako `String`).
- `api/calendar.ts:31,45` vs `meetings.rs:115` · MEDIUM · dwa niezależne dyskryminatory źródła spotkania (`source` vs prefiks `manual:` w `id`).
- `store.ts:494-517`, `notify.rs:48` · MEDIUM · `NavigateRequest.section` zmienia znaczenie zależnie od `view`.
- Jednostki czasu niewidoczne w typach (`total` w ms, `estimate` w minutach, setne godzin), konwersje setnych zduplikowane (`tsAmount` vs `tsHundredths`).
- `settings.rs` 67 pól w jednej strukturze, `Default` i `defaultSettings` w TS to dwie ręczne listy, trzecia w fixture; `applyFields` w TS kopiuje `validate_fields`; drobny rozjazd: `orcaPath` w TS odrzuca `\u0000-\u001f`, Rust także C1.
- `api/task.ts:138` `recurrenceState` istnieje tylko w podglądzie, ale w typie współdzielonym z Rust; pola oznaczone `?`, choć Rust zawsze je zwraca.
- Trzy walidatory dat w Rust i kilka w TS (część to tylko regex, więc podgląd przepuszcza `2026-02-30`).

### 5.9 Wydajność

Szacunki ze struktury kodu, bez pomiarów; skala roku pracy to ok. 10 tys. sesji i 2 tys. zadań.

- `App.vue:542`, `TasksView.vue:663` · HIGH · sekundnik działa także przy oknie schowanym w trayu; każdy `TaskCard` dostaje `:now`, więc 200–300 kart re-renderuje się co sekundę. Poprawka: `now` tylko dla aktywnej karty, pauza po ukryciu.
- `ReportsView.vue:55`, `report.ts:64-77`, `DayClose.vue:77`, `timesheet.ts:149` · HIGH · `dayRows` dla każdego zadania filtruje wszystkie sesje, zależne od `now`: O(T×S) co sekundę.
- `JournalView.vue:223-306`, `DayTimeline.vue:118,169`, `time.ts:10-24` · HIGH · `intervals`, `week`, `weekTasks`, `workBlocks` przeliczane co sekundę; `inDay` bez cache `dayBounds` (ok. 170 tys. wywołań na sekundę).
- `WeekBoard.vue:84-204`, `WeekReview.vue:66-177`, `EstimateAccuracy.vue:22`, `MonthReport.vue:78-95` · HIGH/MEDIUM · to samo: 7×S `inDay` i nowe obiekty (`usual`, `menuDays`) co sekundę, wszystkie `WeekCard` renderują się od nowa; `MonthReport` tworzy 62 `Intl.DateTimeFormat` przy każdej rekalkulacji.
- `RedmineView.vue:162-180`, `JiraView.vue:144-173` · MEDIUM · obiekty `local` tworzone na nowo co sekundę wymuszają re-render wszystkich kart.
- `LibraryView.vue:33-57` · MEDIUM · `history` buduje wiersze dla wszystkich par dzień × zadanie (ok. 730 tys. obiektów) przed filtrem.
- `MultiDayReport.vue:415`, `DayClose.vue:471`, `TsSendDialog.vue:64` · MEDIUM · `:now="now"` do dialogu, który używa tylko `openedAt`; 1100-liniowy szablon renderuje się co sekundę.
- `AgentTaskTime.vue:13-19`, `useAgentTime.ts:30-36`, `agent_time.rs:266-296` · MEDIUM · każdy rozwinięty kafelek co 60 s pobiera całą historię czasu agentów (`[0, 8.64e15]`), Rust liczy O(Tagent×S).
- `TasksView.vue:143`, `capacity.ts:117` · MEDIUM · `dayGoal` z `sessions.map` i sortowaniem co sekundę, nawet bez spotkań.
- `db.rs:107-131` · MEDIUM · pełna migawka (1–2,5 MB JSON) przy każdej mutacji i każdym `data-changed`, w dwóch oknach (`WidgetWindow.vue:444` także schowany); obserwacja Jiry i Redmine emituje `data-changed` co 5 min nawet bez zmian (`jira.rs:1092`, `redmine.rs:2180`).
- `main.rs:89`, `calendar.rs:848` · MEDIUM · komendy synchroniczne na wątku UI; `calendar_status` czyta Menedżer poświadczeń przy każdym montażu `useMeetings`.
- `main.ts`, `vite.config.ts` · MEDIUM · cztery webview ładują ten sam bundle 780 kB bez `import()`; symulatory podglądu (ok. 200 kB źródeł) trafiają do produkcyjnego bundla, bo `desktop` jest rozstrzygane w runtime.
- `reminder.rs:86`, `idle.rs:144`, `focus.rs:740`, `jira_move.rs:477`, `tray.rs:476`, `agents.rs:27` · MEDIUM · cztery wątki co 2 s i dwa co 5 s, każdy z odczytem bazy i synchronicznym zapytaniem do wątku UI (`is_visible` itd.), 24/7 także bez pomiaru. Poprawka: jeden planista z jednym odczytem stanu, dłuższy okres bez pomiaru, stan okna z `WindowEvent`.
- `tray.rs:469-480` · MEDIUM · pełny `refresh()` z `read_pins` i `set_tooltip` co 5 s na timeoucie.
- `db.rs:60`, `agent_time.rs:299` · MEDIUM · brak `PRAGMA synchronous=NORMAL`; przy pracy agenta fsync co 5 s.
- `stats.rs:27-33` · LOW · `day_total_ms_in` skanuje wszystkie sesje (brak indeksu na `ended_at`), ok. 20 tys. razy na dobę.
- `ai_chat.rs:917-930` · MEDIUM · proces `claude` zostaje rezydentny po turze do końca dnia (brak limitu bezczynności).
- `recurrence.rs:102` · MEDIUM · parsowanie JSON wszystkich zadań co 60 s pod blokadą (scalone z 5.1).
- `useAgentTime.ts:10` · LOW · `sessionsSignature` buduje ok. 400 kB stringa ze wszystkich sesji przy każdej zmianie `data`.
- `AgentSuggestions.vue:135` · LOW · polling `agent_suggestions` co 15 s w desktopie, choć Rust emituje zdarzenie co 5 s.
- Brak `shallowRef` w całym `src/`; `data = ref<Snapshot>` jest głęboko reaktywne, a nic nie mutuje go w miejscu.
- `focus.rs:110`, `foreground.rs:249-262` · LOW · brakujące indeksy (`focus_blocks(completed IS NULL)`, `agent_sessions(folder, end)`).

### 5.10 Dostępność (WCAG 2.2 AA)

Kontrast policzony z tokenów: wszystkie pary tekst/tło powyżej 4,5:1 w obu motywach (`muted` na `surface` 6,6/6,0; `accent` 8,1/5,9), pierścień fokusu i ramki pól powyżej 3:1. Wyjątki: zaznaczona zakładka różni się tylko tłem 1,14:1 (`tasks.css:78-86`), awatar Jiry 10,5 px (`features.css:2269`).

- `TaskCard.vue:205-224` · HIGH · 1.3.1/4.1.2 · `aria-label` przycisku rozwijania zastępuje całą treść kafelka (status, projekt, termin, czas).
- `TasksView.vue:473-513`, `TaskCard.vue:250-261`, `AppNav.vue:348-388` · HIGH · 4.1.3 · Start i Stop pomiaru nie są ogłaszane, w `App.vue` nie ma żadnego stałego `aria-live`.
- `QuickSwitch.vue:221-246` · HIGH · 4.1.2 · wybór strzałkami tylko klasą CSS, brak `combobox`/`listbox`/`aria-activedescendant`, liczba wyników nieogłaszana.
- `tauri.conf.json:28-38` · HIGH · 1.4.4 · brak `zoomHotkeysEnabled` i `setZoom`; przy tekście 12–14 px nie da się powiększyć interfejsu.
- `layout.css:87-96`, `TargetBar.vue:19`, `WeekBoard.vue:625` · HIGH · 1.4.11 · brak `@media (forced-colors: active)`; wskaźniki z samego tła (kreska wyboru, paski celu, pastylki legendy) znikają w motywie kontrastowym.
- `SessionEditor.vue:154-176`, `TsSendDialog.vue:601-624` · MEDIUM · 2.1.1 · `role="tablist"` bez strzałek i roving tabindex (wzorcowo w `SettingsView.vue:64-83`).
- `useModal.ts:3,15,20` · MEDIUM · 2.4.3 · selektor pułapki pomija `select`, `textarea`, `[tabindex]`; w `MeetingReminderDialog` fokus początkowy trafia na „Anuluj”.
- `TaskCard.vue:250`, `TasksView.vue:83`, `section.ts:17-31` · MEDIUM · 2.4.3 · `:disabled="busy"` na fokusowanym przycisku Start/Stop i polach ustawień (do sprawdzenia: utrata fokusu w WebView2). Poprawka: `aria-disabled`.
- `section.ts:17-31` · MEDIUM · 4.1.3 · zapis ustawienia bez komunikatu o powodzeniu.
- `App.vue:457-469` · MEDIUM · 2.1.4 · jednoklawiszowy skrót „N” bez możliwości wyłączenia.
- `layout.css:462-466` · MEDIUM · 2.4.11 · sticky pasek tytułu bez `scroll-padding-top`.
- `ReorderHandle.vue:20-34` · MEDIUM · 2.5.7 · zmiana kolejności tylko przeciąganiem albo Alt+↑/↓ opisanym w `title`.
- `TaskCard.vue:361`, `TasksView.vue:484`, `JiraMoveBanner.vue:155`, `JiraBoard.vue:207` · MEDIUM · 2.5.3 · nazwy dostępne nie zaczynają się od widocznego tekstu („Edytuj: …” przy widocznym „Zmień nazwę”).
- `FocusBanner.vue`, `AgentSuggestions.vue`, `JiraMoveBanner.vue`, `UpdateBanner.vue`, `AiProposals.vue` · MEDIUM · 4.1.3 · banery bez roli live (tylko `IdleBanner` ma `role="alert"`).
- `WeekBoard.vue:415`, `TsSendDialog.vue:631`, `TemplateEditor.vue:178`, `TsTaskField.vue:258` · MEDIUM · regiony live tworzone razem z treścią (`v-if` na `role="status"`), czytniki pomijają pierwszą wiadomość.
- `tauri.conf.json:52`, `WidgetWindow.vue:237-241,581-628` · MEDIUM · 2.1.1 · widget `focusable: false`, tryb malutki tylko na `@mouseenter`, `role="status"` wewnątrz przycisku z `aria-label` (komunikat nigdy nie zabrzmi), przełącznik 16×14 px z `opacity: 0`.
- `ReminderWindow.vue:40-66` · MEDIUM · okno przypomnienia bez autofokusu, Esc i `role="alertdialog"`, błąd bez `role="alert"`.
- `ReportsView.vue:373`, `TaskSessions.vue:40`, `JournalView.vue:634`, `DashboardView.vue:314` · MEDIUM · etykiety z wewnętrznym id przedziału („Edytuj przedział 4521”) i `aria-label` nadpisujący widoczny tekst.
- `JiraBoard.vue:188-249` · MEDIUM · przeniesienie karty na tablicy Jiry nieogłaszane (tablica tygodnia ma wzorzec).
- `TsSendDialog.vue:674-717` · MEDIUM · 3.3.1 · tekst błędu pola godzin bez `aria-describedby`.
- LOW: `DayTimeline.vue:314` `role="img"` z Enter otwierającym okno; `AppNav.vue:226` wąski panel bez zamknięcia na `focusout`, `<aside>` bez etykiety; `App.vue:186` zmiana widoku bez przeniesienia fokusu i bez `document.title`; `TemplateEditor.vue:148` fokus przepada po „Usuń”; `WorkSection.vue:41` opisy opcji bez `aria-describedby` (w repo tylko 3 użycia); `aria-label` na elementach bez roli w 6 miejscach; checkbox zadania jako `<button>` bez `role="checkbox"`.

### 5.11 Testy i CI

Pokrycie Rust: dobre w `db`, `settings`, `legacy`, Jira, Redmine, TS (lokalne serwery), `notify`, `reminders`, `idle`, `focus`, `export`, `recurrence`, `days`, `capacity`, `stats`, `holidays`. Słabe: `calendar.rs` (brak testów `access_token`, `graph_get`, `fetch` z nextLink; stałe adresy uniemożliwiają lokalny serwer), `backup::stage` (5 gałęzi odrzucenia, testowana 1), `main.rs` (komendy bez testów, 2 testy przeszukują tekst źródeł), błędy w `db.rs` tylko przez `is_err()` (35 vs 1 z treścią), brak testu indeksu `one_active_session`, brak DST dla przypomnień i `first_measurement`.

Frontend: ok. 580 testów Playwright, mocne asercje (ok. 3250 asercji wartości, 506 `toBeVisible`), izolacja i zamrożony zegar w 63 z 73 plików, brak `.only`/`.skip`. `ReminderWindow.vue` ma jeden test ukryty w `jira.spec.ts:190`. Moduły czyste (`order.ts`, `estimates.ts`, `review.ts`, `capacity.ts`, `time.ts`) bez testów jednostkowych na DST.

Rozjazdy fixture ↔ Rust (test zielony, aplikacja inna):

- `tauri.ts:624-640` `agent_switch` nieidempotentne (H12).
- `tauri.ts:874-894` `mutate` tylko start/stop, start nie sprawdza `done`, nie czyści „Czekam na”, nieistniejące zadanie ignoruje po cichu.
- `tauri.ts:895-928` `set_waiting` bierze tylko `jiraStatus` (Rust `COALESCE(Jira, Redmine)`), akcja `remind` rzuca, brak limitu 300 znaków.
- `tauri.ts:960-1085` `quick_add` bez odrzucenia duplikatu klucza Jiry, bez `valid_key`, bez limitu 240 znaków.
- `tauri.ts:476-519` `redmineStart` przywraca z kosza bez `trashSessions`, bez priorytetu z pasma.
- `tauri.ts:817-873` `idle_resolve` nie odrzuca zadania ukończonego ani w koszu.
- `tauri.ts:273-314` `jiraCheck` nie zapisuje priorytetu importu (Redmine tak).

CI i konfiguracja: brak clippy, `cargo audit`, pomiaru pokrycia (reguła 80% niesprawdzalna) i axe; `dtolnay/rust-toolchain@stable` bez przypiętej wersji i bez `rust-toolchain.toml`; wzorce zrzutów `-win32` generowane lokalnie, CI na `windows-latest`, `maxDiffPixelRatio: 0.01`; 24 użycia `waitForTimeout`; cztery kopie `serve_once` z `accept().unwrap()` bez limitu czasu (zawieszenie zamiast błędu); `browser.rs:1360` zapisuje do prawdziwego `HKCU` bez guarda `Drop`; `npm run check` pomija `tauri build --no-bundle`. Jedyny wspólny plik wzorcowy Rust↔TS to `holidays-pl-2024-2030.json`; komentarz w `holidays.rs:84` wskazuje nieistniejący `tests/holidays.spec.ts`.

### 5.12 Architektura i utrzymywalność

Rozmiary produkcyjne (bez testów): `redmine.rs` 2190, `jira.rs` 1406, `jira_board.rs` 1317, `ai.rs` 1317, `mcp.rs` 1140, `notify.rs` 1138, `timesheet.rs` 1131, `calendar.rs` 1099, `ai_chat.rs` 1023, `browser.rs` 838, `focus.rs` 804, `ai_privacy.rs` 801. Testy inline podwajają pliki (46–57%); wzorzec `mod tests;` już działa w `ai`, `ai_chat`, `mcp`.

- Cztery integracje kopiują `credential_user_of`, `entry`, `connected`, `client()`, `status_error`, `ensure_write`, `send_write`; Jira i Redmine kopiują obserwację i start zadania (`start_issue` 58 linii różnic na 85). Propozycja: `integrations/common.rs` i trait `Tracker` z generycznym `watch_loop`, `import_assigned`, `start_linked_task`.
- `jira_move.rs` zawiera komendy Redmine; dwie pary komend `redmine_transitions`/`redmine_issue_transitions` różnią się tylko nazwą parametru.
- `main.rs`: 357-liniowe `main()`, jedna lista 154 komend, 19 komend zadań w `main.rs`, kolejność inicjalizacji 25 usług pilnowana testem szukającym napisów w źródle. Propozycja: `commands/tasks.rs`, jawna tablica `Service { setup, start }`.
- Logika zadań w trzech wersjach: Rust, `preview/tasks.ts` (836 linii), `tests/fixtures/tauri.ts` (`install()` ok. 1025 linii, 45 z 154 komend). Propozycja: fixture deleguje do `src/preview/*`, wspólne wektory testowe JSON czytane przez Rust i Playwright.
- `Result<_, String>` wszędzie: 574 `map_err(|e| e.to_string())`, jedyny typ błędu `TsError`; frontend rozpoznaje błędy po treści (`redmine::NOT_CONNECTED` jest `pub`). Propozycja: `AppError` serializowany na granicy komendy.
- 46 `thread::spawn` z własną pętlą `sleep` i testem „czy okno main jeszcze jest”, bez wspólnego mechanizmu zatrzymania. Propozycja: `scheduler::every`.
- `Cargo.toml`: 12 pinów `=` bez uzasadnienia (powód tylko dla `tauri-winrt-notification` i `winreg`); `Cargo.lock` i `--locked` już blokują wersje.
- Nazewnictwo komend bez reguły (`mutate`, `quick_add`, `close_day` obok `jira_*`, `ts_*`; `templates_list` vs `template_save`).
- Komponenty Redmine jako monolit `RedmineSettings.vue` (647) wobec `components/jira/*`; `branchName` i `priorityBand` wspólne, ale leżą w module Jiry.

### 5.13 Komentarze i dokumentacja

- Ok. 142 linie kodu odwołują się do usuniętych planów (`PLAN_1.7.md`, `PLAN_1.8.md`, `PLAN_1.9.md`, `PLAN_BACKLOG.md`; commit 906c0ae), w tym ok. 35 nagłówków modułów Rust i 4 komentarze w `Cargo.toml`; ok. 600 oznaczeń bez nazwy pliku (W4, B8, A2, C1–C3, R20B, N1, T1) jest nie do rozszyfrowania. Propozycja: zastąpić ID krótkim powodem, historię przenieść do `docs/ids.md` albo CHANGELOG; w `tests/visual.spec.ts` zmieniać tylko komentarze, nie tytuły (nazwy zrzutów).
- `README.md:394,448` · HIGH · listy „do ręcznego sprawdzenia” odsyłają do usuniętych `PLAN_1.9.md` i `PLAN_1.8.md`.
- `README.md:625` · HIGH · instrukcja weryfikacji używa starej nazwy zmiennej środowiskowej dla sekund przypomnienia (kod czyta `TUKTUK_TRACK_REMINDER_SECONDS`, `reminder.rs:80`) i starego identyfikatora aplikacji; skrypt leży w `src-tauri/target/`, poza repozytorium.
- `README.md:56,133`, `RELEASE.md:245`, `src-tauri/tests/naming.rs:1,12`, `CLAUDE.md:20` · MEDIUM/LOW · opisy wydań przez ID zadań, odwołania do planu, niepełna lista wyjątków testu nazw.
- `src/store.ts:1-13` nagłówek wymienia 5 modułów `api/`, jest 14; `tests/fixtures/tauri.ts:11-30` mapowanie funkcji zgodne, ID zbędne.
- Angielskie komunikaty MCP (sekcja 5.4) i angielskie komentarze w testach `stats.rs`, `backup.rs`, `planner.rs`.
- Pozytywnie: brak TODO/FIXME; interwały, limity i nazwy funkcji w nagłówkach `notify.rs`, `agent_time.rs`, `calendar.rs`, `timesheet.rs`, `jira_board.rs`, `reminders.rs`, `browser.rs` zgodne z kodem; nazwy binariów, skrypty npm, tytuły okien i wersja spójne.

## 6. Proponowana kolejność prac

**Etap 1, bezpieczeństwo i utrata danych (małe zmiany, duży efekt):** H1 (`KEEP_KEYS`, UNC), H2 (wspólna stała Codex), H3 (słownik `Result`), H4 (`Details::default`), H5+H6 (merge w `save`, normalizacja szacunku), H7 (kolejność w ticku, `show()` z `Result`), H8 (keep z walidacją), H9 (wznowienie pomiaru), H10 (Enter w QuickSwitch), H11 (jedna reguła zaokrąglania z testem), H14 (stan po „Zastosuj”), CSV `\t`/`\r`, `git -c log.showSignature=false`.

**Etap 2, obserwowalność:** plik logu i dialog błędu startu (H15), `lock_db` z `into_inner` (H16), `record_watch` dla Redmine (H17), błędy po wysłaniu zapisu (M4), `storage-error` z ograniczeniem częstości w pętlach tła, `.optional()?` zamiast `unwrap_or_default` w `settings`, `backup`, `reminders`, `capacity`, `days`; status serwerów potoku w UI; `useMeetings`/`store` zachowują poprzedni stan przy błędzie.

**Etap 3, testy i kontrakt:** `tests/tsconfig.json` w `npm run build`, clippy `-D warnings` w CI i `check.mjs`, `forbidOnly`, `rust-toolchain.toml`, wspólne wektory JSON (zaokrąglanie, recurrence, `choose`), fixture delegujące do `src/preview`, test indeksu `one_active_session`, DST dla przypomnień, retencja kopii, `PRAGMA user_version` z migracjami.

**Etap 4, wydajność:** `now` tylko dla aktywnej karty i pauza po ukryciu, memoizacja `dayBounds`, podział „suma zamknięta + sesja aktywna” w Journal, WeekBoard, Reports, DayClose, `shallowRef` na migawce, widget odświeża tylko gdy widoczny, `data-changed` tylko przy zmianie, `OnceLock<Client>`, `command(async)` dla komend z blokadą, jeden planista wątków tła, `synchronous=NORMAL`, filtr cykli w SQL, `import()` dla okien pomocniczych i podglądu.

**Etap 5, dostępność:** stały `aria-live` dla Start/Stop, naprawa `aria-label` na kafelku, combobox w QuickSwitch, `zoomHotkeysEnabled`, `forced-colors`, pełny selektor w `useModal` i `inert`, role live na banerach, okno przypomnienia jako `alertdialog`.

**Etap 6, dług strukturalny (mechaniczne podziały, bezpieczne dzięki testom):** `redmine/`, `notify/`, `timesheet/`, `ai/`, `mcp/`, przeniesienie testów inline do `tests.rs`, `integrations/common.rs` i trait `Tracker`, `commands/tasks.rs` z `main.rs`, `AppError`, `useAction()`, `useDayTotals()`, podział `TsSendDialog`, `App.vue`, `JournalView.vue`, usunięcie ID planów z komentarzy, poprawa README.

## 7. Zastrzeżenia

- Oznaczone „do sprawdzenia”: całodniowe wydarzenia Graph w UTC (5.3), wykrywanie uśpienia (H13), utrata fokusu po `disabled` w WebView2 (5.10), nazwy flag `features.*` i `--permission-prompts` w obecnych wersjach CLI, `visibilityState` dla okna schowanego przez `hide()`, czy linki `target=_blank` otwierają się bez uprawnienia `opener`.
- Koszty wydajności to szacunki ze struktury kodu; warto zmierzyć na kopii prawdziwej bazy (`EXPLAIN QUERY PLAN`, DevTools Performance).
- Moduły testowe Rust i większość specyfikacji Playwright czytane były fragmentami; ocena pokrycia opiera się na nazwach testów i grepie asercji.
- Pliki `UX_AUDIT.md` i `NEXT_SESSION.md` są nieaktualne i zostały pominięte zgodnie z decyzją użytkownika; przegląd dostępności opiera się wyłącznie na kodzie.
