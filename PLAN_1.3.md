# Todo Track 1.3–1.6 — plan rozwoju

Stan wyjściowy: 1.2.0 (15 października 2026), gałąź `release/1.0`. 71 testów Rust, około 55 testów Playwright w 15 plikach. Instalator 1.2.0 w `releases/`, jeszcze nie instalowany.

## Zakres

Zaakceptowane (numery z listy pomysłów):

| # | Funkcja | Rozmiar | Etap |
|---|---------|---------|------|
| 9 | Domknięcie wydania: instalacja, aktualizacja przy aktywnym timerze, toasty, tag | S | 0 |
| 10 | Testy: zrzuty widoków, luki w pokryciu, CI na każdym etapie | M | 1 |
| 3 | Akcje w powiadomieniach Windows | M | 2 |
| 4 | Szybkie dodawanie ze składnią | M | 3 |
| 5 | Szablony zadań | S | 3 |
| 2 | Podpowiadanie zadania na podstawie pracy agentów AI | L | 4 |
| 6 | Raport miesięczny i eksport CSV/XLSX | M | 5 |
| 7 | Tryb skupienia (pomodoro) | M | 6 |

Odrzucone: worklogi do Jiry (1), automatyczne aktualizacje (8).

Rozmiary jak w PLAN_1.0.md: S do pół dnia, M jeden do dwóch dni, L dwa do trzech dni z testami. Łącznie około 10–13 dni pracy. Każdy etap od 1 wzwyż kończy się zielonymi testami i może wyjść jako osobna wersja: 1.3 (etapy 1–2), 1.4 (3), 1.5 (4), 1.6 (5–6).

Kolejność ma powód: najpierw zamknięte wydanie i siatka testów, potem akcje w powiadomieniach, bo korzystają z nich etapy 4 i 6, a szablony wchodzą razem ze składnią, bo wywołuje się je z tego samego pola.

## Założenia przyjęte bez pytania

- Aplikacja nigdy sama nie przełącza timera. Agenci, powiadomienia i tryb skupienia tylko proponują; decyzja zawsze należy do użytkownika.
- Akcje w powiadomieniach działają, gdy aplikacja jest uruchomiona. Kliknięcie starego toastu z Centrum akcji po restarcie tylko otwiera główne okno.
- Składnia szybkiego dodawania używa znaków-prefiksów (`~`, `@`, `#`, `!`, `/`), żeby zwykły tytuł typu „Spotkanie 1h” nie był rozbierany na pola.
- Eksport CSV jest pod polski Excel: UTF-8 z BOM, średnik jako separator, przecinek dziesiętny.
- Cel miesiąca liczy dni robocze z ustawień minus polskie święta ustawowe (wyliczane, także Wielkanoc i Boże Ciało). Święta można wyłączyć w ustawieniach.
- Przerwa w trybie skupienia domyślnie zatrzymuje pomiar; w ustawieniach można wybrać, żeby pomiar trwał.

Jeśli któreś założenie Ci nie pasuje, zmiana jest tania przed etapem, który go dotyczy.

## Etap 0 — domknięcie wydania (pkt 9)

Wymaga Twojej obecności przy komputerze; nie da się go w pełni zautomatyzować.

1. Ustalić, która wersja jest zainstalowana w `C:\Programy\Todo Track`. Uruchomić pomiar na dowolnym zadaniu.
2. Zainstalować `releases/Todo Track_1.2.0_x64-setup.exe` na działającą aplikację. Sprawdzić: powstała kopia `backups/before-1.2.0-*.sqlite`, aktywny pomiar został zamknięty na moment aktualizacji, aplikacja wstała sama, dane i ustawienia są na miejscu. Tym samym zamyka się niesprawdzony punkt 4 etapu 5 z PLAN_1.0.md (aktualizacja przy aktywnym timerze); test z 0.3.0 już nie ma sensu, bo nikt nie zostanie na 0.3.0.
3. Toasty po instalacji (z zarejestrowanym AUMID): termin, długi pomiar (ustawić np. 1 min), wieczór, cel, potwierdzenie skrótu przy schowanym widgecie. Zrzuty do `test-results/release-1.2/`.
4. Autostart: wylogowanie i logowanie, potem wyłączenie wpisu w Menedżerze zadań i odczyt w Ustawieniach.
5. Ustalić, co zajmuje `Ctrl+Alt+Space`. Jeśli to popularny program, zmienić domyślny skrót Start/Stop (propozycja: `Ctrl+Alt+S`).
6. Tagi `v1.2.0` na b189eec oraz `v1.0.0` i `v1.1.0` na odpowiednich commitach; push tagów, zielony workflow `check.yml` na windows-latest.
7. Aktualizacja `NEXT_SESSION.md`: usunięcie zamkniętych punktów z „Do zrobienia (etap 5)”.

Gotowe, gdy: aktualizacja przy aktywnym timerze sprawdzona, toasty obejrzane, tagi wypchnięte, CI zielone.

## Etap 1 — siatka testów (pkt 10)

Cel: kolejne zmiany interfejsu, takie jak w 1.2, nie psują niczego niezauważenie.

1. **Zrzuty wzorcowe** (`tests/visual.spec.ts`, `expect(page).toHaveScreenshot`): Zadania, Dashboard, Plan i historia (Dzień, Tablica tygodnia), Zamknięcie dnia, Jira, Ustawienia, widget; motyw ciemny i jasny, szerokość 760 i 1280. Stały zegar (`page.clock`) i stałe dane z jednego fixture, żeby zrzuty nie zależały od daty. Próg różnicy `maxDiffPixelRatio: 0.01`. Wzorce generowane na windows-latest i commitowane; lokalnie `npm run test:visual:update`.
2. **Wspólny fixture danych** `tests/fixtures/seed.ts`: tydzień pracy z projektami, szacunkami, notatkami, zadaniem z Jiry, cyklem i koszem. Dziś każdy plik testów buduje dane po swojemu.
3. **Luki w pokryciu**, które warto zamknąć:
   - import nowych zgłoszeń z Jiry w e2e: pierwsze sprawdzenie tylko zapamiętuje, drugie dodaje do „Do zaplanowania”, zgłoszenie w koszu nie jest dublowane (Rust ma test `sync_imports_only_new_assignments_and_keeps_statuses`, interfejs nie);
   - szybkie przełączanie: Ctrl+K, strzałki, Enter zatrzymuje poprzedni pomiar;
   - baner bezczynności: obie odpowiedzi i zapomnienie przerwy, gdy pracował agent;
   - przywracanie kopii w podglądzie (podsumowanie, wymagany checkbox).
4. **CI**: osobny krok dla zrzutów, artefakt z różnicami przy porażce (`playwright-report/`).
5. Skrypt `npm run check` uruchamiający lokalnie to samo co CI (build, Playwright, fmt, `cargo test --locked`), żeby nie trzeba było pamiętać pięciu komend.

Gotowe, gdy: zrzuty i nowe testy są zielone w CI dwa razy z rzędu (sprawdzenie, że nie migają).

## Etap 2 — akcje w powiadomieniach (pkt 3)

### Technika

`tauri-plugin-notification` na Windows nie przekazuje kliknięć ani przycisków do aplikacji. Dlatego `notify::show` przechodzi na bezpośrednie użycie `tauri-winrt-notification` (tej samej biblioteki, z której korzysta plugin; na początku etapu potwierdzić w kodzie pluginu 2.5.0, że kliknięcia faktycznie nie są przekazywane): `add_button`, `on_activated` z argumentem akcji. Plugin zostaje tylko jako zapas dla innych systemów.

- Nowy typ `Action { id: &'static str, label: String }` i `Message { title, body, actions, open: View }`.
- `on_activated` wysyła akcję do pętli w Rust kanałem; pętla wykonuje ją na `AppHandle` (Stop, start zadania, pokazanie okna z widokiem) i emituje `navigate` do frontendu.
- AUMID: identyfikator z instalatora. W `tauri dev` bez AUMID akcje mogą nie działać; tak samo jak dziś, miarodajny jest pakiet NSIS.

### Akcje

| Powiadomienie | Kliknięcie treści | Przyciski |
|---|---|---|
| Termin | Zadania z filtrem „Dzisiaj” | — |
| Długi pomiar | Zadania, rozwinięte bieżące zadanie | „Zatrzymaj”, „Podziel…” (otwiera edytor na zakładce Podziel) |
| Wieczór | Zamknięcie dnia | „Zatrzymaj teraz” |
| Cel osiągnięty | Zamknięcie dnia | — |
| Nowe zgłoszenia z Jiry | Zadania, sekcja „Do zaplanowania” | „Rozpocznij” (gdy jedno zgłoszenie), „Zaplanuj na dziś” |
| Potwierdzenie skrótu Start/Stop | główne okno | — |

Przyciski nie robią nic nieodwracalnego bez potwierdzenia w oknie; „Zatrzymaj” i „Rozpocznij” są odwracalne (pomiar można poprawić), więc działają od razu.

### Testy

- Rust: mapowanie reguły na `Message` z akcjami; obsługa nieznanego id akcji; „Rozpocznij” na zgłoszeniu w koszu przywraca zadanie (ta sama ścieżka co widok Jira).
- Playwright: zdarzenie `navigate` z backendu ustawia właściwy widok i rozwinięcie.
- Natywnie po instalacji: każdy przycisk z tabeli.

## Etap 3 — szybkie dodawanie ze składnią (pkt 4) i szablony (pkt 5)

### Składnia

Parser w `src/quickadd.ts`, czysta funkcja `parse(text, today, settings) → { title, fields, tokens }`.

| Zapis | Znaczenie | Przykład |
|---|---|---|
| klucz Jiry na początku | powiązanie ze zgłoszeniem; pusty tytuł bierze nazwę z `jira_issues` | `WMS-123` |
| `~30m`, `~2h`, `~1h30` | szacunek | `~45m` |
| `@dziś`, `@jutro`, `@pon`…`@nd`, `@20.10` | dzień w planie (najbliższy taki dzień) | `@pt` |
| `do:pt`, `do:20.10` | termin | `do:jutro` |
| `!` / `!niski` | priorytet wysoki / niski | `!` |
| `#nazwa` | projekt (podpowiedzi z istniejących) | `#wms` |
| `/szablon` | szablon (niżej) | `/review` |
| `\` przed znakiem | zapis dosłowny | `C\#` |

- Pod polem dodawania na żywo widać „chipy” z rozpoznanymi polami; kliknięcie chipa usuwa token. Tab uzupełnia projekt i szablon.
- To samo pole działa w szybkim przełączaniu: brak wyników → „Utwórz i rozpocznij” (Enter) albo „Utwórz” (Shift+Enter).
- Bez prefiksów zachowanie jest dokładnie takie jak dziś.

### Szablony

- Tabela `task_templates(id, name UNIQUE, json)`; json: tytuł z polami `{klucz}`, `{tytuł}`, szacunek, lista kroków, projekt, priorytet.
- „Zapisz jako szablon” w rozwiniętym kafelku; zarządzanie w Ustawieniach → Szablony (zmiana nazwy, edycja, usunięcie).
- Użycie: `/review WMS-123` tworzy „Code review WMS-123: <tytuł z Jiry>” z krokami i szacunkiem szablonu. Pola podane jawnie w linii wygrywają z szablonem.
- Kopia SQLite obejmuje szablony; przywracanie je zastępuje, jak resztę danych.

### Testy

- Jednostkowe parsera (Playwright bez UI albo `node --test`): każdy token, kolejność dowolna, dosłowne `\`, przełom tygodnia i roku, `@pon` w poniedziałek oznacza następny tydzień, niejednoznaczne `@31.02` jako błąd w chipie.
- e2e: dodanie z pełną składnią, szablon z kluczem Jiry, utworzenie i start z szybkiego przełączania.
- Rust: zapis i odczyt szablonów, unikalna nazwa.

## Etap 4 — podpowiedzi z pracy agentów (pkt 2)

### Rozpoznanie zadania

- `agents.rs` zna już `cwd` każdego pracującego agenta. Nowa funkcja `git_branch(cwd)`: idzie w górę katalogów do `.git`, obsługuje plik `.git` z `gitdir:` (worktree i submoduły), czyta `HEAD`. Bez uruchamiania procesu `git`. Wynik w cache na 30 s na katalog.
- Klucz Jiry z nazwy gałęzi (`[A-Z][A-Z0-9]+-\d+`, np. `feature/WMS-123-eksport`), a gdy go nie ma, z nazwy sesji Claude Code.
- Przypisania folderu do zadania: tabela `agent_bindings(folder, task_id)`, gdy w repo nie pracuje się na gałęziach z kluczem. Tworzone z podpowiedzi („Zapamiętaj dla tego folderu”).
- Kolejność dopasowania: przypisanie folderu → klucz z gałęzi powiązany z lokalnym zadaniem → klucz znany z `jira_issues` (wtedy zadanie powstanie przy akceptacji).
- Kafelek „Agenci AI” na Dashboardzie pokazuje gałąź i dopasowane zadanie.

### Kiedy proponować

Propozycja pojawia się, gdy agent pracuje co najmniej 2 minuty nad dopasowanym zadaniem X, a timer stoi albo mierzy inne zadanie, i ta propozycja (agent, X) nie została odrzucona. Gdy agenci pracują nad różnymi zadaniami, pokazuje listę zamiast jednego przycisku.

Miejsca: baner w głównym oknie (jak baner bezczynności), linia w widgecie, powiadomienie z przyciskiem „Przełącz” (etap 2), gdy główne okno jest schowane.

Odpowiedzi: „Przełącz” (start X, poprzedni pomiar zatrzymany), „Nie teraz” (cisza dla tej pary do końca pracy agenta), „Nie proponuj dla tego folderu” (zapis w ustawieniach, odwracalny w Ustawieniach → Czas pracy).

### Powrót z przerwy

Gdy wraca się po bezczynności, a w tym czasie agent pracował nad X innym niż mierzone zadanie, baner bezczynności ma trzecią opcję: „Przypisz przerwę do X”, czyli podział pomiaru w chwili początku przerwy z drugą częścią na X (istniejący mechanizm podziału).

### Testy

- Rust: `git_branch` dla zwykłego repo, worktree z plikiem `.git`, odłączonego HEAD, braku repo; wyciąganie klucza; reguła 2 minut i pamięć odrzuceń.
- e2e: baner z jednym i dwoma agentami, „Przełącz” zmienia timer, „Nie proponuj” zapisuje folder, trzecia opcja baneru bezczynności dzieli pomiar.

## Etap 5 — raport miesięczny i eksport (pkt 6)

- Raporty → nowa zakładka **Miesiąc**: wybór miesiąca, suma względem celu miesiąca (dni robocze minus święta, z rozbiciem „przepracowane / do celu”), tabela według projektu, według zadania z kluczem Jiry, oraz wykres dni z linią celu (wzór z Dashboardu).
- Moduł `src-tauri/src/holidays.rs`: polskie święta ustawowe na dany rok (stałe i ruchome od Wielkanocy). Ustawienie „Uwzględniaj święta w celu” (domyślnie włączone). Cel dnia, tygodnia i tablica tygodnia też przestają liczyć święto jako dzień roboczy.
- **Eksport** z zakładki Miesiąc i z „Wybranych dni”: okno zapisu (dialog plugin, jak przy kopii), formaty:
  - CSV: jeden wiersz na przedział: data, start, koniec, minuty, godziny dziesiętne, zadanie, klucz Jiry, projekt, notatka dnia;
  - XLSX (`rust_xlsxwriter`): arkusze „Przedziały” i „Podsumowanie” (dni × zadania, sumy), formaty liczb i dat ustawione, pierwszy wiersz zamrożony.
- Trwający pomiar w eksporcie liczony do chwili eksportu, z dopiskiem „trwa”.

### Testy

- Rust: święta 2024–2030 (porównanie z listą), CSV z BOM i polskimi znakami, XLSX otwierany ponownie przez `calamine` w teście i sprawdzane sumy.
- e2e: zakładka Miesiąc, cel z odjętym świętem, eksport w podglądzie zwraca tekst CSV.

## Etap 6 — tryb skupienia (pkt 7)

- Przycisk „Skupienie” na pasku bieżącego pomiaru i w widgecie; czas bloku 25 lub 50 min (ustawienie, plus dowolna liczba minut), przerwa 5 lub 10 min.
- Start bloku uruchamia pomiar wybranego zadania, jeśli nie trwa. Widget pokazuje odliczanie zamiast zegara pomiaru (pomiar nadal liczy się normalnie).
- W czasie bloku: przypomnienie w tle, propozycje agentów i powiadomienia inne niż „koniec bloku” czekają w kolejce i przychodzą po bloku jako jedno zbiorcze.
- Koniec bloku: powiadomienie z przyciskami „Przerwa” i „Kolejny blok”. Przerwa domyślnie zatrzymuje pomiar, a po przerwie przychodzi „Wracasz do X?” z „Rozpocznij”.
- Tabela `focus_blocks(id, task_id, start, end, completed)`; Dashboard: liczba ukończonych bloków dziś i w tygodniu.
- Przerwanie bloku (Stop, zmiana zadania) kończy blok jako nieukończony, bez pytań.

### Testy

- Rust: kolejka powiadomień w czasie bloku, koniec bloku po uśpieniu komputera (liczony z zegara, nie z odliczania).
- e2e z `page.clock`: start, odliczanie w widgecie, przerwa zatrzymuje pomiar, przerwanie bloku przez Stop.

## Po każdym etapie

Tak samo jak w 1.0: `npm run check` zielone, przegląd zmian, poprawki z przeglądu w osobnym commicie, README (sekcja „Nowości 1.x”) i `NEXT_SESSION.md` zaktualizowane. Przy wydaniu: wersja w pięciu plikach, build NSIS, instalator w `releases/`, tag.
