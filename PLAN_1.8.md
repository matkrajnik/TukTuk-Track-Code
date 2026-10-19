# Todo Track 1.8 — plan rozwoju

Stan wyjściowy: 1.7.3 (19 października 2026), gałąź `master`, tag `v1.7.3` wypchnięty. 244 testy Rust, 260 testów Playwright. Wszystko poniżej wychodzi jako **jedno wydanie 1.8.0** na końcu; zadania nie podnoszą wersji.

Temat wydania: **wysyłka Raportu TS do Ideo Timesheet**. Dziś „Raport TS” (`src/report.ts`, funkcje `tsText` i `decimalHours`; sekcje w `src/DayClose.vue`, `src/views/ReportsView.vue` i `src/components/MultiDayReport.vue`) to tylko tekst do skopiowania i ręcznego przepisania do formularza TS. Po 1.8 ten sam raport dnia da się wysłać jednym kliknięciem, z podglądem i potwierdzeniem.

Źródło wiedzy o TS: osobny projekt `C:\Projekty\Private\ideo-ts-api` (Node/TS, katalog `connector/`), który loguje się do `https://api.ts.ideo.pl`, parsuje HTML formularza dnia (cheerio) i zapisuje dzień multipartem. Plan portuje ten konektor do Rusta; konektor Node nie staje się zależnością aplikacji.

## Zakres

| # | Funkcja | Rozmiar | Zadanie |
|---|---------|---------|---------|
| 1 | Backend TS w Rust: logowanie, sesja, formularz dnia, parser, zapis | L | T1 |
| 2 | Mapowanie zadań Todo Track na zadania TS (pole zadania, domyślne per projekt) | M | T2 |
| 3 | Podgląd i wysyłka dnia z Zamknięcia dnia i Raportu dnia | M | T3 |
| 4 | Ustawienia → sekcja „Timesheet (TS)” | S | T4 |
| 5 | Symulowany TS w podglądzie, testy Playwright, README, wersja | M | T5, R18 |

Rozmiary jak w poprzednich planach: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni z testami.

Odłożone: wysyłka wielu dni naraz (z `MultiDayReport.vue`) — po sprawdzeniu zachowania serwera na jednym dniu; worklogi do Jiry — bez zmian, osobne planowanie; podgląd miesiąca z TS (`GET /timesheet/mojtimesheet?vue=1&miesiac=`) — nie w tym wydaniu.

Ręczne testy z prawdziwym TS nie są częścią zadań (jak w 1.7); lista rzeczy do sprawdzenia jest na końcu planu i trafia do `NEXT_SESSION.md`.

## Założenia przyjęte bez pytania

- **Nic nie łączy się z `https://api.ts.ideo.pl` podczas pracy nad wydaniem.** Testy Rust działają na zapisanym, zanonimizowanym fragmencie HTML; testy Playwright na symulowanym TS w `localStorage`; `cargo test` i `npm run build` nie wykonują żadnych zapytań sieciowych. Pierwsze prawdziwe połączenie wykonuje użytkownik ręcznie, po instalacji, według listy na końcu planu.
- Wysyłka nigdy nie jest automatyczna. Przycisk „Wyślij do TS” otwiera podgląd, a dopiero „Zapisz w TS” w podglądzie wykonuje zapis. Zamknięcie dnia nie wysyła niczego samo.
- Zapis w TS **zastępuje cały dzień** (tak działa formularz `wypelnij`: wpisy pominięte w POST znikają). Dlatego podgląd zawsze pokazuje wpisy już istniejące w TS na ten dzień i daje wybór „Zastąp wszystko” / „Zachowaj pozostałe wpisy TS” (domyślnie to drugie). Przy „Zachowaj” wpisy TS, których zadania nie ma w raporcie, są dołączane do POST bez zmian.
- Godziny idą do TS tak jak w raporcie: dziesiętne, dwa miejsca, przecinek (`decimalHours`), zaokrąglenie per wiersz po scaleniu. Dopóki ręczny test nie powie inaczej, nie ma zaokrąglania do 0,25 h. Nadgodziny zawsze `0`. Pole `status` zawsze `0` (jak w konektorze; znaczenie do sprawdzenia).
- Opis wpisu TS to **notatka dnia** zadania (`details.notes[dzień]`), a gdy jej nie ma — tytuł zadania. Kilka zadań Todo Track na to samo zadanie TS scala się w jeden wpis: suma czasu, opisy łączone „; ” (bez powtórzeń), jak w układzie `summary` raportu.
- Konfiguracja (adres API, login, `deviceId`, nazwa urządzenia) leży w tabeli `settings` pod kluczem `timesheet` jako JSON, jak Jira pod `jira`. Hasło w magazynie poświadczeń Windows (`keyring`, usługa `<identifier>.timesheet`, użytkownik `<adres API>|<login>`), jak token Jiry. **Token, refresh token i ciasteczko sesji tylko w pamięci** (`TsState`), nigdy w SQLite ani w logach; po restarcie aplikacja loguje się ponownie hasłem z magazynu przy pierwszym użyciu.
- Adres API jest edytowalny (domyślnie `https://api.ts.ideo.pl`), tylko `https`, bez ścieżki i parametrów — jak `normalize_site` dla Jiry. Pozwala wskazać środowisko testowe, gdyby firma takie udostępniła.
- Lista zadań TS pochodzi z formularza dnia (`GET /timesheet/wypelnij?dzien=`), więc teoretycznie może zależeć od dnia. Aplikacja pobiera ją dla wysyłanego dnia przy podglądzie wysyłki, a do mapowania w edycji zadania — dla dnia dzisiejszego, z pamięcią podręczną w pamięci na czas działania aplikacji i przyciskiem „Odśwież”.
- Główny przełącznik „Wysyłaj raport TS” (`ts_send` w `settings.rs`, domyślnie włączony) chowa przyciski wysyłki i blokuje `ts_send` także w backendzie, jak „Pozwól aplikacji zmieniać zgłoszenia w Jirze”.
- Nowe tabele tylko przez `CREATE TABLE IF NOT EXISTS`. Nowe pola zadań w JSON `task_details` z wartościami domyślnymi (`serde(default)` tam, gdzie backend je czyta), bez migracji.
- Kopia SQLite obejmuje nowe tabele (`ts_project_map`, `ts_sent_days`); hasło nie jest przenoszone (jak przy Jirze i kalendarzu).

## T1 — Backend TS (`src-tauri/src/timesheet.rs`)

Port konektora z `ideo-ts-api/connector/src` (`services/auth.service.ts`, `services/timesheet.service.ts`, `parsers/timesheet-html.parser.ts`, `helpers/timesheet-form.helper.ts`) na Rust z `reqwest`. Jeden moduł, bez nowych wątków w tle.

Zależności w `src-tauri/Cargo.toml`: `reqwest` dostaje feature `multipart` (obok `json`, `rustls-tls`); nowy crate `scraper` do parsowania HTML (html5ever + selektory CSS, bez JS). Oba zablokowane w `Cargo.lock` (`--locked` jak dotąd).

Konfiguracja i stan:
- `Config { api_url, login, device_id, device_name }` z `Default` (`https://api.ts.ideo.pl`, pusty login, pusty `device_id`, `"Todo Track"`). `device_id` losowany raz przy pierwszym połączeniu (32 znaki szesnastkowe z `rand`, już w zależnościach przez tauri; jeśli nie — z `uuid`/`getrandom`) i zapamiętany w konfiguracji, żeby TS widział jedno urządzenie.
- `TsState(Mutex<Session>)` w pamięci: `token`, `refresh`, `expires_at` (ms), `cookie` (sam fragment `nazwa=wartość` z pierwszego `Set-Cookie`, bez atrybutów). `setup(app)` rejestruje stan jak `jira::setup`.
- `entry(app, &Config)` i `password(app, &Config)` jak `jira::entry`/`token`: brak hasła → „Połącz konto TS: podaj login i hasło.”.

Protokół (ścieżki względem `api_url`):
- **Logowanie**: `POST /api/login`, JSON `{login, password, deviceId, deviceName}` → `{token, refresh, expires}`. Potem `GET /api/user/identity` z `Authorization: Bearer <token>`; z odpowiedzi brany pierwszy nagłówek `Set-Cookie` → `cookie`. Brak ciasteczka → „Serwer TS nie zwrócił ciasteczka sesji po zalogowaniu.”.
- **Ważność tokenu** jak w `AuthService.tokenExpiry`: `exp` z ładunku JWT (base64url, bez weryfikacji podpisu), inaczej `expires` interpretowane jako ms, sekundy epoki albo sekundy od teraz, inaczej 30 min. Odświeżenie, gdy do wygaśnięcia zostaje mniej niż 60 s.
- **Odświeżenie**: `GET /api/token/refresh?id=api-ts&name=api-ts&refresh=<refresh>` → nowe `{token, refresh, expires}`, potem znowu `GET /api/user/identity` po nowe ciasteczko. 401 przy odświeżaniu → pełne logowanie hasłem z magazynu. Każde żądanie do stron timesheetu idzie z **oboma** nagłówkami: `Authorization` i `Cookie`.
- **`with_auth(op)`**: `ensure_fresh`, wykonanie, a przy 401 jednorazowe odświeżenie i powtórka (odpowiednik `withAuthentication`; bez równoległych odświeżeń dzięki `Mutex`).
- **Formularz dnia**: `GET /timesheet/wypelnij?dzien=RRRR-MM-DD` → HTML. Walidacja daty jak `isIsoDate` (`RRRR-MM-DD`, istniejący dzień).
- **Zapis**: `POST /timesheet/wypelnij?dzien=RRRR-MM-DD`, `multipart/form-data` (`reqwest::multipart::Form`) z polami `akcja=zapisz`, `dzien`, `status=0`, `submit=Zapisz Timesheet` oraz dla każdego wpisu `i` od 0: `zadanie[i]`, `zadanie_godziny[i]`, `zadanie_nadgodziny[i]`, `zadanie_opis[i]`. Klient ma `redirect::Policy::none()` (jak Jira), więc odpowiedź 2xx **albo 3xx** po zapisie liczy się jako przyjęta; potwierdzeniem jest i tak ponowny odczyt formularza (`ts_filled`) wykonywany przez komendę `ts_send` zaraz po POST i zwracany do interfejsu.

Parser (`scraper`, selektory 1:1 z cheerio):
- `parse_tasks(html) -> Vec<Project { name, tasks: Vec<TsTask { id, name }> }>`: pierwszy `select` w `.jq-wiersz-ts .jq-zadanie`, każdy `optgroup` to projekt (`label`), każdy `option` z niepustym `value` to zadanie (`value` = id, tekst = nazwa, przycięte).
- `parse_filled(html) -> Vec<Filled { id, hours, overtime, description }>`: każdy `tr.jq-wiersz-ts`; `id` z `.jq-zadanie select option[selected]` (`value`), `hours` z `.jq-zadanie-godziny input` (`value`), `overtime` z `.jq-zadanie-nadgodziny input`, `description` z `.jq-zadanie-opis input`; wiersze z pustym `id` pomijane. Wartości zwracane jako tekst, bez przeliczania (TS może oddawać „1,5” albo „1.5”).
- Dokument bez `select` w `.jq-wiersz-ts` → błąd „Nie rozpoznano formularza timesheetu. Strona TS mogła się zmienić albo sesja wygasła.” (również gdy zamiast formularza przyszła strona logowania).

Komendy (w `main.rs` `invoke_handler`, nazwy z prefiksem `ts_`):
- `ts_settings` → `{ config (bez hasła), connected, sendEnabled }`; `connected` = hasło w magazynie.
- `ts_connect(api_url, login, password)` → walidacja (login 1–100 znaków, hasło 1–1024, adres `https` bez ścieżki), próba logowania + identity, zapis hasła do magazynu i konfiguracji; zmiana loginu lub adresu usuwa stare poświadczenie i czyści `TsState`. Zwraca `ts_settings`.
- `ts_disconnect` → usuwa hasło z magazynu, czyści `TsState`, zostawia konfigurację (login, `device_id`) i mapowania.
- `ts_tasks(date)` → `Vec<Project>` z formularza dnia.
- `ts_filled(date)` → `Vec<Filled>`.
- `ts_send(date, entries: Vec<{ taskId, hours, overtime, description }>, mode: "replace" | "keep")` → gdy `ts_send` w ustawieniach wyłączony: „Wysyłka do TS jest wyłączona w Ustawieniach → Timesheet.”. Walidacja: `taskId` niepuste i ≤ 20 znaków, `hours` i `overtime` pasują do `^\d+([.,]\d{1,2})?$`, opis ≤ 2000 znaków bez znaków sterujących, lista 1–50 wpisów, data poprawna. Przy `keep` komenda sama pobiera `ts_filled(date)` i dokleja wpisy, których `taskId` nie ma w `entries` (w kolejności z TS, za naszymi). Po POST odczytuje `ts_filled(date)`, zapisuje `ts_sent_days(day TEXT PRIMARY KEY, sent_at INTEGER NOT NULL, hours TEXT NOT NULL DEFAULT '')` (suma godzin z odczytu) i zwraca `{ filled, sentAt }`.
- `ts_sent(day)` → ostatni zapis z `ts_sent_days` albo `null`.

Komunikaty błędów po polsku, w jednej funkcji `status_error` jak w `jira.rs`: 401 → „TS odrzucił logowanie. Sprawdź login i hasło.”, 403 → „Brak uprawnień do timesheetu.”, 404 → „Nie znaleziono strony timesheetu. Sprawdź adres API.”, 429 → „TS ograniczył liczbę zapytań. Spróbuj ponownie za chwilę.”, 5xx → „TS zwrócił błąd HTTP …”, brak sieci / limit 30 s → „Nie udało się połączyć z TS. Sprawdź połączenie z siecią firmową.”. Hasło, token, refresh i ciasteczko nigdy nie trafiają do `format!` komunikatu ani do logów.

Testy Rust (bez sieci): parser na pliku `src-tauri/tests/fixtures/ts_wypelnij.html` (`include_str!`), zanonimizowanym fragmencie strony formularza przygotowanym ręcznie na podstawie struktury z konektora — pełna tabela z nagłówkiem, 2 projekty w `optgroup`, 3 zadania, 2 wypełnione wiersze (jeden z przecinkiem „1,5”, jeden z kropką „0.25”), 1 wiersz bez wybranego zadania (pomijany), opis z polskimi znakami i średnikiem. Uwaga: `scraper::Html::parse_fragment` nie zachowuje samotnych `<tr>` (kontekst `body`), więc fixture ma pełny `<table>`. Dalej: budowa multipartu (nazwy pól i kolejność indeksów, `status=0`, `submit`), wyliczenie ważności tokenu (JWT z `exp`, `expires` w ms / s / s od teraz, brak → 30 min), wycięcie `nazwa=wartość` z `Set-Cookie` z atrybutami, walidacja daty i wpisów (odrzucenie „1,375”, „abc”, pustego id, 51 wpisów), scalanie `keep` (nasze wpisy pierwsze, TS-owe spoza listy za nimi, duplikaty id z TS zastąpione), normalizacja adresu API, blokada przy wyłączonym `ts_send`, zapis i odczyt `ts_sent_days` na bazie w pamięci.

## T2 — Mapowanie zadań na zadania TS

- `TaskDetails` (`src/store.ts`) dostaje `tsTaskId?: string` i `tsTaskName?: string` (nazwa do pokazania bez połączenia; odświeżana przy każdym wyborze). Backend nie czyta tych pól poza `ts_send`-em (mapowanie liczy frontend), więc bez zmian w strukturach Rust poza testem, że istniejące JSON-y bez pól wczytują się jak dotąd.
- **Edycja zadania** (rozwinięty kafelek, `src/components/TaskCard.vue`): pole „Zadanie TS” — `select` z `optgroup` per projekt TS z `ts_tasks(dzisiaj)` i opcją „— brak —”. Lista w pamięci podręcznej `store.ts` na czas działania (jedno pobranie na sesję, „Odśwież” obok pola). Bez połączenia pole pokazuje zapamiętaną nazwę i link „Połącz konto TS w Ustawieniach” (`navigate` z `section: "timesheet"`). Gdy zapamiętane `tsTaskId` nie występuje na liście, opcja „(niedostępne) nazwa” zostaje wybrana z ostrzeżeniem.
- **Domyślne mapowanie per projekt Todo Track**: tabela `ts_project_map(project TEXT PRIMARY KEY COLLATE NOCASE, ts_task_id TEXT NOT NULL, ts_task_name TEXT NOT NULL DEFAULT '')` (wzór: `agent_bindings`). Komendy `ts_project_map` (lista), `ts_set_project_map(project, ts_task_id, ts_task_name)` (puste `ts_task_id` usuwa). W edycji zadania pod polem „Zadanie TS” pole wyboru „Ustaw jako domyślne dla projektu „X”” (widoczne, gdy zadanie ma projekt). W Ustawieniach → Timesheet lista mapowań z usuwaniem (T4).
- Reguła rozstrzygania przy wysyłce (czysta funkcja `resolveTsTask(task, map)` w `src/timesheet.ts`): jawne `tsTaskId` zadania → mapowanie projektu → brak (ostrzeżenie). Nowe zadania nie dostają kopii mapowania projektu do `task_details` (zmiana mapowania projektu działa od razu na wszystkie zadania bez jawnego wyboru); kafelek pokazuje chip „TS: nazwa (z projektu)”.

Testy: jednostkowe `resolveTsTask` (jawne, z projektu, wielkość liter projektu, brak). Rust — `ts_project_map` wstawianie, nadpisanie, usunięcie pustym id, `COLLATE NOCASE`. Playwright (T5) — wybór zadania TS w kafelku i zapamiętanie, pole „domyślne dla projektu”.

## T3 — Podgląd i wysyłka dnia

- Nowy komponent `src/components/TsSendDialog.vue` otwierany przyciskiem **„Wyślij do TS”** w dwóch miejscach: sekcja „Raport TS” w Zamknięciu dnia (`DayClose.vue`, obok „Kopiuj raport TS”) i sekcja „Raport TS” w Raporcie dnia (`ReportsView.vue`, obok „Kopiuj raport”). Przycisk widoczny tylko przy włączonym `tsSend` i połączonym koncie; bez konta — tekst „Połącz konto TS w Ustawieniach” z linkiem. Dzień z przyszłości blokuje przycisk.
- Wiersze buduje czysta funkcja `tsEntries(day, data, now, map, options)` w `src/timesheet.ts` na bazie `dayRows` z `report.ts`: pomija wiersze z zerowym czasem, rozstrzyga zadanie TS (`resolveTsTask`), scala wiersze o tym samym `tsTaskId` (suma ms → `decimalHours`, opisy „; ” bez powtórzeń, przycięte, białe znaki zbite), nadgodziny `"0"`. Zwraca `{ entries, unmapped: Task[], totalMs }`. Zadania bez mapowania są wyłączone z `entries`, ale ich czas jest widoczny w ostrzeżeniu.
- Podgląd: tabela „Zadania Todo Track → Zadanie TS · Godziny · Opis” (opis edytowalny w miejscu przed wysyłką; edycja nie zmienia notatki w Todo Track), suma godzin; blok ostrzeżeń: „N zadań bez zadania TS (X h) nie zostanie wysłanych” z linkiem do kafelka każdego, „Trwający pomiar liczony do teraz”, „Zadanie TS „…” nie występuje na liście tego dnia” (porównanie z `ts_tasks(day)` pobranym przy otwarciu).
- Blok „W TS na ten dzień jest już N wpisów (razem Y h)” z listą z `ts_filled(day)` (nazwa z `ts_tasks`, godziny, opis) i przełącznikiem: **„Zachowaj pozostałe wpisy TS”** (domyślnie; wpisy o zadaniach spoza raportu zostaną dopisane bez zmian, wpisy o tych samych zadaniach zostaną zastąpione — każdy taki oznaczony „zastąpi 2,00 h”) / **„Zastąp wszystko”** (znikną wszystkie dotychczasowe wpisy; przycisk zapisu zmienia etykietę na „Zastąp i zapisz”). Gdy w TS nic nie ma, blok mówi „Dzień w TS jest pusty”.
- „Zapisz w TS” → `ts_send(day, entries, mode)`; po sukcesie dialog pokazuje odczyt z TS („W TS zapisano N wpisów, razem Y h”) i porównuje z wysłanym: różnica w liczbie wpisów albo sumie > 0,01 h → ostrzeżenie „TS zapisał inne wartości niż wysłane — sprawdź w przeglądarce”. Błąd → komunikat z backendu, dane w dialogu zostają.
- Znacznik wysyłki: `ts_sent(day)` pokazuje w obu sekcjach „Wysłano do TS 19.10 17:02 (7,50 h)”; po kolejnych zmianach pomiarów dnia (suma różni się od zapisanej) dopisek „raport zmienił się od wysyłki”.
- Zamknięcie dnia: wysyłka nie zamyka dnia i zamknięcie nie wysyła; kolejność kroków w sekcji: Raport TS → „Wyślij do TS” → „Zamknij dzień”.

Testy: jednostkowe `tsEntries` (scalanie dwóch zadań na jedno TS z sumą i opisami, zadanie bez notatki używa tytułu, zero czasu pominięte, trwający pomiar do `now`, zaokrąglenie po scaleniu zgodne z sumą raportu, `unmapped` z czasem) i porównania „zapisano inne wartości”. Playwright (T5) — otwarcie podglądu z obu miejsc, ostrzeżenia, „Zachowaj” kontra „Zastąp”, komunikat po zapisie, znacznik „Wysłano do TS”, brak przycisku przy wyłączonym przełączniku.

## T4 — Ustawienia → „Timesheet (TS)”

- Nowa sekcja `timesheet` w `src/components/SettingsPanel.vue` (lista sekcji w `SettingsView.vue`, między Jira a Kalendarz; wpis w wyszukiwarce ustawień: „Timesheet”, „TS”, „Ideo”).
- **Połączenie**: pola Login i Hasło (`type="password"`, wartość nigdy nie wraca z backendu; po połączeniu pole puste z podpisem „zapisane w magazynie poświadczeń Windows”), zwinięte „Zaawansowane”: adres API (domyślny `https://api.ts.ideo.pl`) i identyfikator urządzenia (tylko do odczytu, generowany przy pierwszym połączeniu). Przyciski „Połącz” (`ts_connect`), „Testuj połączenie” (`ts_tasks(dzisiaj)` → „Połączono: N zadań w M projektach” albo błąd), „Rozłącz” (`ts_disconnect`, z `ConfirmDialog`). Stan: „Połączono jako login” / „Nie połączono”.
- **Przełącznik „Wysyłaj raport TS”** (`tsSend`, domyślnie włączony) z opisem, że wysyłka zawsze wymaga potwierdzenia w podglądzie i że wyłączenie chowa przyciski i blokuje zapis w backendzie.
- **Domyślne zadania TS dla projektów**: tabela mapowań z `ts_project_map` (projekt → zadanie TS, „Usuń”), dodawanie: projekt z listy projektów zadań (`projects` jak w `TaskCard.vue`) + wybór zadania TS z `ts_tasks(dzisiaj)`.
- Pod sekcją krótki opis działania: skąd biorą się godziny i opisy, co znaczy „Zastąp wszystko”.
- `settings.rs`: pole `ts_send: bool` z domyślną wartością `true`, odczyt w `ts_send` (T1).

Testy: Rust — domyślne `ts_send`, zapis i odczyt. Playwright (T5) — połącz (symulowane), błąd przy złym haśle, test połączenia, rozłącz, przełącznik chowa przyciski wysyłki, dodanie i usunięcie mapowania projektu.

## T5 — Podgląd przeglądarkowy, testy e2e, dokumentacja

- `store.ts`: funkcje `tsSettings`, `tsConnect`, `tsDisconnect`, `tsTasks(date)`, `tsFilled(date)`, `tsSend(date, entries, mode)`, `tsSent(day)`, `tsProjectMap`, `tsSetProjectMap` — na pulpicie `invoke`, w podglądzie symulacja w `localStorage` pod `previewTsKey = "todo-track-preview-ts-v1"` (wzór `previewJira*`): stan `{ connected, login, failLogin, tasks: Project[], filled: Record<day, Filled[]>, sent: Record<day, {sentAt, hours}>, projectMap }`. Przykładowe projekty TS „WMS” (3 zadania) i „Wewnętrzne” (2 zadania: „Spotkania”, „Administracja”); dla dnia z `tests/fixtures/seed.ts` jeden wpis już istniejący. `tsSend` w podglądzie zastępuje dzień tak jak serwer (przy `keep` dokleja pozostałe), hasło „zle” zwraca komunikat błędu logowania; `window.storage` nasłuch jak przy Jirze, żeby testy mogły zmieniać stan.
- `tests/fixtures/seed.ts`: dwa zadania z `tsTaskId` na to samo zadanie TS (do scalania), jedno z projektem bez jawnego mapowania (z mapowaniem projektu), jedno bez niczego (ostrzeżenie).
- Nowy `tests/timesheet.spec.ts`: sekcja Ustawień (połącz, błąd, test, rozłącz, przełącznik), pole „Zadanie TS” w kafelku i „domyślne dla projektu”, podgląd wysyłki z Zamknięcia dnia i z Raportu dnia (wiersze scalone, ostrzeżenie o braku mapowania, lista istniejących wpisów, „Zachowaj” dokleja, „Zastąp” usuwa, komunikat po zapisie, znacznik „Wysłano do TS”), błąd `ts_send` zostawia dane w dialogu. Zrzuty wzorcowe (`tests/visual.spec.ts`) Zamknięcia dnia i Raportu dnia zaktualizowane świadomie, gdy nowy przycisk zmienia układ; nowy zrzut sekcji Ustawień.
- README: sekcja **„Nowości 1.8”** (co robi wysyłka, skąd godziny i opisy, scalanie, „Zachowaj” / „Zastąp”, gdzie jest hasło, że token jest tylko w pamięci, że nic nie wysyła się bez kliknięcia), nowy punkt w opisie ustawień („Timesheet (TS)”), nowa sekcja „Timesheet (Ideo TS)” obok sekcji o Jirze z opisem protokołu (login, identity, refresh, `wypelnij`, multipart), uwaga „zapis zastępuje cały dzień” i lista rzeczy niesprawdzonych z prawdziwym TS. Wiersz „Wersja w repozytorium” i „Weryfikacja 1.8.0”.

## Wydanie 1.8.0 (R18)

Wersja 1.8.0 w pięciu plikach (`package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`), README jak w T5, `NEXT_SESSION.md` ze stanem i listą ręcznych testów poniżej, pełna weryfikacja (`npm run build`, Playwright, `cargo fmt --check`, `cargo test --locked`, `npm run tauri build -- -- --locked`), instalator w `releases/` (tylko najnowszy, starszy usunięty jak przy 1.7.3), tag `v1.8.0`, publikacja według `RELEASE.md`.

## Po każdym zadaniu

Jak w 1.7: zielone `npm run build`, Playwright, `cargo fmt --check`, `cargo test --locked`; przegląd dwiema soczewkami (zgodność z planem, poprawność), uwagi w osobnym commicie. Bez podnoszenia wersji do R18. Żadne zadanie nie wykonuje zapytań do `api.ts.ideo.pl`.

## Do ręcznego sprawdzenia z prawdziwym TS (po instalacji, przez użytkownika)

Wymaga natywnej aplikacji i konta w Ideo TS. Najlepiej na dniu testowym (np. miniony dzień bez wpisów albo dzień, który i tak trzeba poprawić), bo zapis zastępuje cały dzień.

Format godzin:
- Czy serwer przyjmuje godziny **z przecinkiem** („1,37”) i czy po zapisie `ts_filled` oddaje tę samą wartość, czy przelicza (np. na „1.37”, „1,25” albo odrzuca). Jeśli przyjmuje tylko kropkę, `ts_send` ma zamieniać przecinek na kropkę przed POST (jedna linia, bez zmiany raportu).
- Czy akceptuje **dowolną liczbę miejsc po przecinku** (dwa, jak w raporcie), czy wymaga kroku **0,25 h** (albo 0,5 h). Jeśli wymaga kroku, dochodzi ustawienie „Zaokrąglaj godziny do 0,25 h” w sekcji Timesheet z ostrzeżeniem w podglądzie, że suma różni się od raportu.
- Czy przyjmuje wartość „0” w `zadanie_godziny` (wpis z samymi nadgodzinami) i jak reaguje na wiersz z zerem — czy taki wpis trzeba pominąć.
- Jaki jest limit długości `zadanie_opis` i czy średnik oraz polskie znaki przechodzą bez zmian.

Pole `status`:
- Co oznacza `status=0` w POST (prawdopodobnie „roboczy / niezatwierdzony”) i czy po zapisie formularz da się dalej edytować. Czy istnieje inna wartość (np. `1` = „wyślij do akceptacji”) i czy dzień zatwierdzony przez przełożonego da się nadpisać z aplikacji, czy serwer odpowiada błędem (jakim kodem i treścią) — na tej podstawie komunikat w `status_error`.

Lista zadań:
- Czy lista zadań w `GET /timesheet/wypelnij?dzien=` **różni się między dniami** (np. zadania zamknięte w połowie miesiąca znikają z późniejszych dni, a są w starszych). Jeśli tak, pamięć podręczna listy w edycji zadania (dla dzisiaj) jest tylko podpowiedzią, a ostrzeżenie „nie występuje na liście tego dnia” w podglądzie ma znaczenie; jeśli nie, można pobierać listę rzadziej.
- Czy identyfikatory zadań (`option value`) są stabilne w czasie (to one są zapisane w `tsTaskId`).

Zastępowanie dnia:
- Czy zapis z pominięciem istniejących wpisów **faktycznie je usuwa** (po POST z jednym wpisem na dniu z dwoma, `ts_filled` zwraca jeden). Jeśli serwer scala zamiast zastępować, „Zachowaj pozostałe wpisy TS” jest zbędne i może zostać ukryte, a „Zastąp wszystko” wymaga innego mechanizmu (np. wysłania pustych wierszy).
- Czy POST bez żadnego wpisu czyści dzień (potrzebne do ewentualnego „Wyczyść dzień w TS”; nie jest w planie).
- Czy odpowiedź na POST to 3xx (przekierowanie do formularza) czy 200 z HTML, i czy przy błędzie walidacji serwer oddaje 200 ze stroną z komunikatem (wtedy `ts_send` musi wykrywać błąd po odczycie `ts_filled`, nie po kodzie HTTP).

Sesja:
- Czy logowanie `POST /api/login` z nowym `deviceId` wymaga dodatkowego potwierdzenia (np. e-mail) i czy po restarcie aplikacji logowanie hasłem z magazynu przechodzi bez pytań.
- Czy ciasteczko z `GET /api/user/identity` wystarcza do stron `/timesheet/*` i czy po `token/refresh` trzeba je odnawiać (konektor odnawia; sprawdzić, czy stare nie przestaje działać).
- Rzeczywisty czas życia tokenu (`exp` w JWT czy `expires`), żeby potwierdzić wyliczenie z T1; zachowanie po dłuższej przerwie (godzina, noc) — czy pierwsza wysyłka po przerwie przechodzi bez błędu.
- Hasło po „Rozłącz” znika z Menedżera poświadczeń Windows (`pl.todotrack.desktop.timesheet`).
