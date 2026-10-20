# Todo Track — integracja z agentami AI (Claude Code, Codex)

Stan wyjściowy: 1.8.0 (wydane 20 października 2026), równolegle 1.9 (Redmine, `PLAN_1.9.md`) i zadania bez wersji w `PLAN_BACKLOG.md`. Plan nie jest przypisany do wydania; zadania A0–A6 mogą wejść do 1.10 albo później. Analiza i decyzje: rozmowa z 20 października 2026.

Temat: **agenci AI pomagają w trackowaniu, a nie tylko są wykrywani**. Dziś aplikacja wie, że Claude Code albo Codex pracuje (`agents.rs`, a po B8 hooki) i dopasowuje sesję do zadania heurystyką (`agents::resolve`: przypisanie folderu, gałąź git, klucz Jiry w gałęzi lub nazwie sesji). Po tym planie:
- agent może rozmawiać z aplikacją przez **serwer MCP Todo Track**: czytać zadania i dzień, dopisywać notatki, proponować przypisania;
- aplikacja ma **czat** w panelu bocznym, oparty na tym samym MCP;
- gdy heurystyka zawodzi, aplikacja **pyta model** o dopasowanie: sesja agenta → zadanie, zadanie → zadanie TS, luka w dniu → zadanie, opis wpisu TS.

## Zakres

| # | Zadanie | Rozmiar | Zależy od |
|---|---------|---------|-----------|
| A0 | Filtr danych osobowych (pseudonimizacja wszystkiego, co wychodzi do modelu) | M | — |
| A1 | Silnik AI: wykrycie CLI, wybór agenta przy pierwszym uruchomieniu, izolowane wywołania, Ustawienia → Asystent AI | M | A0 |
| A2 | Serwer MCP `todo-track-mcp.exe` i rejestracja w Claude Code i Codexie | L | A0 |
| A3 | Czat w aplikacji (panel boczny, karty akcji) | L | A1, A2 |
| A4 | Asystent przypisań: nieprzypisane sesje agentów, zadanie TS dla zadania | M | A1, B8 |
| A5 | Zamknięcie dnia z asystentem: luki, czas nieprzypisany, opisy TS | L | A1, A4 |
| A6 | Rano i przegląd tygodnia: komentarz asystenta (opcjonalnie) | S | A1 |

Rozmiary jak w poprzednich planach: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni z testami. A0 i A2 mogą iść równolegle; A3 jest pierwszym zadaniem widocznym dla użytkownika w aplikacji.

## Decyzje użytkownika (20 października 2026)

- **Silnik = narzędzia CLI z subskrypcji** (~90% użycia): `claude` (Claude Code) i `codex` (Codex CLI) uruchamiane przez aplikację, logowanie i limity z subskrypcji użytkownika. Bez klucza API; klucz API najwyżej jako późniejszy zapas, poza tym planem.
- **Dane zadań i Jiry wolno wysyłać do modelu, ale bez imion i nazwisk.** Dotyczy każdego wyjścia do modelu: wywołań aplikacji, czatu i odpowiedzi narzędzi MCP (to, co MCP zwraca sesji Claude, też trafia do modelu).
- **Domyślny agent wybiera użytkownik przy pierwszym uruchomieniu** (Claude Code / Codex / nie używaj) i wybór jest zapisany w bazie (`settings`). Zmiana w Ustawieniach → Asystent AI.

## Założenia przyjęte bez pytania

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

Jak w 1.8: zielone `npm run build`, Playwright, `cargo fmt --check`, `cargo test --locked`; przegląd dwiema soczewkami (zgodność z planem, poprawność) z dodatkową soczewką **prywatności** (czy każda ścieżka do modelu przechodzi przez A0); README (sekcja funkcji, „Asystent AI”, „Wersja w repozytorium”); commit `A<n>: <tytuł>`. Zadania nie podnoszą wersji. Żadne zadanie nie wywołuje prawdziwego `claude` ani `codex` w testach.

## Do ręcznego sprawdzenia (po instalacji, przez użytkownika)

- A1: `claude -p` uruchomione z aplikacji korzysta z subskrypcji (bez klucza API) i działa w schowanym oknie; `--setting-sources ""` naprawdę pomija hooki z `~/.claude/settings.json` (w tym wpisy Orki) — jeśli nie, potwierdzić, że filtr `TODO_TRACK_AI` w `todo-track-hook.exe` wystarcza; komunikat przy wylogowanym CLI i przy wyczerpanym limicie; `codex exec --output-schema` zwraca poprawny JSON.
- A1: dialog pierwszego uruchomienia z jednym, dwoma i zerem wykrytych CLI; wybór przetrwał restart.
- A0: przejrzeć „Pokaż, co zostanie wysłane” na prawdziwym dniu z Jirą: czy żadne nazwisko nie przeszło; uzupełnić „Ukrywaj te nazwy”.
- A2: rejestracja w Claude Code i Codexie (`claude mcp get todo-track`), narzędzia widoczne w sesji (`/mcp`), zachowanie przy zamkniętej aplikacji, ponowna rejestracja po aktualizacji instalatorem, współistnienie z wpisami Orki.
- A3: czas pierwszej odpowiedzi, zatrzymanie w trakcie, rozmowa z Codexem; czat nie pojawia się w kafelku „Agenci AI”.
- A4: sesja w nowym folderze bez klucza w gałęzi daje trafny baner; „Przypisz folder” działa następnym razem bez modelu.
- A5: analiza dnia z kilkoma lukami i pracą agentów; trafność propozycji opisów TS.
