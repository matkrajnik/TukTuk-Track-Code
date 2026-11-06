# TukTuk Track — plan modułu workflow agentów

Data: 2026-11-05. Status: E0 zamknięty (wyniki: `E0_ANALIZA.md`), E1 zrealizowany; od E2 do realizacji.

Dokument obejmuje analizę stanu istniejącego, docelowe zachowanie i ekrany, architekturę, model danych, kolejność implementacji, testy oraz wydanie. Zastępuje koncepcję osobnej aplikacji AgentFlow Studio planem rozbudowy TukTuk Track. Nie jest zgodą na automatyczne rozpoczęcie implementacji.

## 1. Cel i granice projektu

Użytkownik ma tworzyć wizualny proces pracy agentów, uruchamiać go dla zadania i lokalnego repozytorium, obserwować wykonanie, przeglądać zmiany oraz podejmować decyzje. Przebiegi są trwałe i powiązane z istniejącymi zadaniami TukTuka. O kolejności kroków decyduje silnik, a nie model.

Pierwszy kompletny scenariusz:

1. Użytkownik wybiera zadanie, repozytorium i szablon procesu.
2. Claude analizuje repozytorium i zapisuje ustrukturyzowany plan.
3. Codex implementuje zadanie w wydzielonym katalogu roboczym.
4. Claude przeprowadza review bez prawa modyfikacji repozytorium.
5. Po pozytywnym review proces czeka na decyzję użytkownika.
6. Po negatywnym review Codex wykonuje poprawki i następuje kolejne review; najwyżej trzy rundy poprawek.
7. Akceptacja kończy przebieg. Odrzucenie kończy go z odpowiednim statusem; zmiany pozostają dostępne do przeglądu.

MVP obejmuje Trigger, Agent, Condition, Approval i End; ręczne uruchomienie, dwóch dostawców CLI, edytor, historię, logi, przerwanie i bezpieczne odzyskanie po restarcie. Nie obejmuje automatycznego commita, pushowania, scalania gałęzi ani publikacji.

Po MVP: OpenCode, Command, ogólny Loop, Parallel/Join, harmonogram, webhooki, narzędzia MCP/HTTP i dalsze integracje. Bez marketplace, chmury współdzielącej workflow, wielu użytkowników, pełnego BPMN i zdalnych workerów w pierwszej wersji.

„Lokalnie” oznacza lokalne sterowanie i przechowywanie danych. Dostawcy modeli mogą wysyłać kontekst do swoich usług; aplikacja nie obiecuje pracy modeli offline. Użytkownik wybiera dostawcę i model przed uruchomieniem, bez narzuconego modelu Astra.

## 2. Co potwierdzono w repozytorium

Rozpoznanie statyczne z 2026-11-05, bez uruchamiania aplikacji, CLI agentów i usług zewnętrznych. Wersja w `package.json` i `src-tauri/Cargo.toml`: 2.3.2. `CLAUDE.md` zawiera również starszy numer 2.0.0, więc manifesty są punktem odniesienia dla wersji.

| Obszar | Istniejące pliki | Wniosek dla modułu |
| --- | --- | --- |
| Aplikacja | `package.json`, `src-tauri/Cargo.toml` | Tauri 2, Vue 3, TypeScript, Vite już istnieją; bez nowego bootstrapu. |
| Dane | `src-tauri/src/db.rs` | SQLite przez rusqlite, inicjalizacja schematu w `init`; rozbudować istniejącą bazę. |
| UI | `src/App.vue`, `src/components/AppNav.vue`, `src/store.ts`, `src/navigation.ts` | Wpiąć widoki do istniejącej nawigacji; projekt nie korzysta z Pinii ani routera. |
| Podgląd | `src/api/`, `src/preview/` | Nowe API musi mieć odpowiednik demonstracyjny bez uruchamiania procesów. |
| Wywołania AI | `src-tauri/src/ai.rs` | Są wykrywanie Claude/Codex, budowa argumentów, abstrakcja Runner, timeout i kończenie drzewa procesów. |
| Streaming | `src-tauri/src/ai_chat.rs` i katalog testów | Są parsery obu dostawców, abstrakcje Spawner/Child/Sink i streaming. Historia czatu jest w pamięci. |
| Prywatność | `src-tauri/src/ai_privacy.rs` | Istnieją filtrowanie i pseudonimizacja danych; trzeba zaprojektować ich użycie dla kodu i artefaktów. |
| Sesje | `src-tauri/src/agents.rs`, `hooks.rs`, `agent_board.rs`, `src/agentboard.ts` | Istnieje monitoring, dopasowanie folderu do zadania i tablica sesji; nie zastępują trwałego wykonania grafu. |
| Czas | `src-tauri/src/agent_time.rs`, `agent_wait.rs` | Istnieją pomiary agentów i oczekiwania; potrzebna korelacja i ochrona przed podwójnym liczeniem. |
| Orca | `src-tauri/src/orca.rs`, `src/components/task/OrcaWorktreeDialog.vue` | Istnieje tworzenie worktree z opcjonalnym agentem i promptem. Nie jest to jeszcze scheduler workflow. |
| MCP | `src-tauri/src/mcp.rs`, `src-tauri/src/bin/mcp.rs` | Istnieją narzędzia TukTuka i model propozycji; nie udostępniać nowego wykonawcy automatycznie wszystkim sesjom. |
| Backup | `src-tauri/src/backup.rs` | Istnieje kopia SQLite i reguły zachowania ustawień połączeń przy przywróceniu. |
| Sekrety | Cargo: `keyring`, moduły integracji | Zachować systemowy magazyn; nie kopiować poświadczeń CLI do bazy. |
| Testy i CI | `tests/fixtures/tauri.ts`, `.github/workflows/check.yml` | Playwright, zrzuty, Rust, formatowanie i build Windows już istnieją. |

Ważne różnice wobec pierwotnego planu AgentFlow Studio:

- Zachować rusqlite; migracja całej aplikacji na SQLx nie jest częścią projektu.
- Zachować istniejący system komponentów i stylów. Nuxt UI nie jest warunkiem wdrożenia.
- Vue Flow jest kandydatem na edytor; jeszcze nie jest zależnością projektu.
- Tokio nie jest obecnie bezpośrednią zależnością w odczytanym Cargo.toml. Jawne użycie w nowym runtime wymaga dodania i sprawdzenia potrzebnych funkcji oraz współpracy z runtime Tauri.
- `ai.rs` izoluje obecnego asystenta w katalogu AI i tłumi jego hooki przez `TUKTUK_TRACK_AI`. Nie wolno skopiować tych ustawień do wykonawcy repozytorium bez rozdzielenia polityk.
- `ai_chat.rs` ogranicza dostępne narzędzia i ma inny model życia sesji. Wspólne parsery mogą być użyteczne, ale manager czatu nie powinien stać się schedulerem.
- Obecne kończenie procesów używa m.in. `taskkill /T /F`. Nowy runtime wymaga sprawdzenia zachowania przy awarii hosta i procesach potomnych.

W chwili rozpoznania istniały niezacommitowane zmiany w backendzie, nawigacji, fixture i nowych plikach powiadomień. Nie są częścią tego planu; implementacja musi najpierw ustalić aktualny stan bazowy i zachować cudzą pracę.

## 3. Etap analizy szczegółowej przed implementacją

Powyższa mapa jest punktem startowym, nie pełnym audytem bezpieczeństwa ani potwierdzeniem działania na Windows.

| ID | Zadanie analityczne | Wynik wymagany przed kodowaniem zależnej części |
| --- | --- | --- |
| A01 | Przejrzeć aktualne instrukcje projektu, git status i równoległe zmiany. | Lista zakresów, których nie należy nadpisywać; ustalona baza prac. |
| A02 | Prześledzić cykl `ai.rs` i `ai_chat.rs`: wykrycie → argumenty → proces → parser → zakończenie. | Mapa elementów do współdzielenia oraz oddzielnych polityk chat/workflow. |
| A03 | Prześledzić bazę, blokady, inicjalizację, backup i restore. | Projekt transakcyjnych migracji modułu i obsługi artefaktów. |
| A04 | Prześledzić nawigację, style, modal, ustawienia i podgląd. | Lista punktów integracji oraz makiety zgodne z UI TukTuka. |
| A05 | Prześledzić hooki, dopasowanie zadań, agent_time i agent_wait. | Jednoznaczny model korelacji i naliczania czasu. |
| A06 | Sprawdzić lokalnie zainstalowane wersje CLI oraz ich oficjalną dokumentację. | Macierz obsługiwanych wersji: streaming, sesje, model, uprawnienia, narzędzia, anulowanie. |
| A07 | Zbadać sposoby egzekwowania read-only dla każdego runnera na natywnym Windows. | Próby zapisu z procesu zastępczego i późniejszy test akceptacyjny użytkownika; brak dowodu blokuje profil review. |
| A08 | Ocenić Vue Flow: licencję, zgodność z Vue/Vite, import/eksport i obsługę klawiatury. | Krótka decyzja architektoniczna i prototyp w gałęzi implementacyjnej. |
| A09 | Zbadać workspace: Git, niezapisane zmiany, worktree, junctions, submoduły, ścieżki UNC. | Lista wspieranych przypadków MVP i jawnych blokad. |
| A10 | Uruchomić bazowe testy zgodnie z instrukcjami repozytorium. | Raport rozdzielający wcześniejsze awarie od nowych regresji. |

Nie utrwalać w planie niezweryfikowanych flag CLI jako gwarancji zgodności. Przykłady z dokumentu źródłowego są wskazówką do A06. Testy automatyczne projektu używają fixture; testy prawdziwych Claude/Codex/Orca i zewnętrznych usług wykonuje użytkownik zgodnie z `CLAUDE.md`.

## 4. Docelowa funkcjonalność i doświadczenie użytkownika

### 4.1 Miejsce w aplikacji

Jedna aplikacja z rozszerzanym zakresem funkcji: domyślnie dostępne jest obecne śledzenie i planowanie pracy. Wpisanie poprawnego klucza w ustawieniach odblokowuje dodatkowo Workflows. Nie ma dwóch osobnych trybów ani przełącznika „Śledzenie / Workflows”; odblokowanie nie usuwa i nie zastępuje dotychczasowych funkcji. Nie powstaje drugi system projektów ani zadań.

Docelowo panel boczny porządkuje funkcje w czytelne grupy:

| Grupa | Pozycje | Dostępność |
| --- | --- | --- |
| Praca | Dashboard, Zadania, obecne wejścia do planowania, Dziennik, Raporty | Zawsze; zachować istniejące akcje i ich znaczenie. |
| Agenci | Sesje agentów | Zawsze, zgodnie z dotychczasową konfiguracją. |
| Workflows | Przegląd, Definicje, Uruchomienia | Dopiero po odblokowaniu kluczem. |
| Integracje | Jira, Redmine | Zgodnie z obecnymi regułami widoczności integracji. |
| Zasoby | Biblioteka | Zawsze, zgodnie z obecną dostępnością. |
| Stała stopka panelu | Ustawienia i dotychczasowe akcje globalne | Zawsze. |

„Przegląd” Workflows pokazuje aktywne przebiegi, oczekujące akceptacje, ostatnie wyniki i przycisk utworzenia procesu. „Definicje” służą projektowaniu, „Uruchomienia” monitorowaniu i historii. Profile agentów pozostają w ustawieniach oraz panelu węzła. Szablony są dostępne z listy definicji; nie tworzyć dla nich kolejnej głównej pozycji menu. Licznik przy grupie Workflows sygnalizuje oczekujące decyzje.

Podział należy dopasować do wszystkich aktualnych wejść w `AppNav.vue`, także skrótów planowania, bez zgubienia istniejących akcji. Sekcje mają nagłówki w rozwiniętym panelu, separatory i podpowiedzi w zwiniętym; na małym ekranie zachowują tę samą kolejność. Timer, powiadomienia i dostęp do zadania pozostają widoczne również w edytorze i monitorze workflow.

Z karty zadania: „Uruchom workflow” oraz lista powiązanych przebiegów. Z sesji agenta: przejście do przebiegu, jeśli korelacja jest znana. Workflow nie przełącza automatycznie timera użytkownika, nie zamyka zadania i nie zmienia zgłoszenia w Jira/Redmine.

### 4.1.1 Odblokowanie kluczem w ustawieniach

Ustawienia → „Workflows” pokazują przed aktywacją krótki opis rozszerzenia, pole „Klucz dostępu” i przycisk „Odblokuj Workflows”. Cała obecna funkcjonalność śledzenia działa bez klucza. Przed odblokowaniem nie pokazywać pustych lub zablokowanych pozycji Workflows w głównej nawigacji ani akcji uruchomienia na kartach zadań.

Przebieg aktywacji:

1. Użytkownik wpisuje klucz i wybiera „Odblokuj Workflows”.
2. Backend sprawdza klucz; UI pokazuje stan sprawdzania i blokuje ponowne wysłanie.
3. Błędny klucz daje komunikat przy polu, bez animacji sukcesu i bez zmiany menu.
4. Poprawny klucz zapisuje trwałe uprawnienie, po czym uruchamia animację odsłonięcia nowej sekcji.
5. Ustawienia pokazują „Workflows odblokowane” i przycisk „Otwórz Workflows”. Użytkownik sam wybiera wejście do modułu; aktywacja nie uruchamia agentów.

Stan odblokowania jest zachowany po restarcie. Zapis uprawnienia musi się udać przed pokazaniem sukcesu. Klucz nie trafia do logów, analityki, komunikatów błędów ani eksportów. Weryfikacja uprawnienia jest również w backendzie dla komend modułu; ukrycie menu i flaga localStorage nie stanowią kontroli dostępu.

**Decyzja E0 (2026-11-05):** jeden stały klucz dostępu, wspólny dla wszystkich instalacji, bez serwera licencji i bez terminu ważności. W kodzie jest wyłącznie skrót SHA-256 klucza (`src-tauri/src/workflow_access.rs`, funkcja `is_valid_workflows_key`); klucz ma postać `TUKTUK-WF-XXXXX-XXXXX-XXXXX-XXXXX`, wielkość liter i otaczające spacje są ignorowane. Sam klucz zna właściciel projektu i nie zapisuje się go w repozytorium. To lokalna bramka funkcji eksperymentalnej: nie daje ochrony komercyjnej przed modyfikacją aplikacji.

Nie wprowadzać domyślnego terminu ważności ani okresowej walidacji online bez uzgodnienia. W razie późniejszego cofnięcia uprawnienia blokować nowe starty i edycję, ale zachować dostęp do historii oraz zatrzymania aktywnego procesu. Nie usuwać danych ani zostawiać działających agentów bez kontroli.

### 4.1.2 Animacja odblokowania — odsłonięcie Workflows

Cel: wyraźny, dopracowany moment rozszerzenia TukTuka o nowe możliwości. Nie jest to ekran zmiany trybu ani animacja przy każdym uruchomieniu aplikacji.

Proponowany scenariusz, około 1,2–1,6 sekundy po trwałym potwierdzeniu aktywacji:

- 0–200 ms: przycisk odblokowania przechodzi w potwierdzenie; znak TukTuka dostaje delikatny akcent świetlny.
- 200–650 ms: w lekkiej nakładce dekoracyjnej pojawia się kilka węzłów połączonych animowanymi liniami. Impuls biegnie przez graf, wizualnie zapowiadając przepływ pracy agentów.
- 650–1100 ms: graf przesuwa się i skaluje w stronę miejsca nowej sekcji menu; sekcja Workflows rozwija się płynnie, a jej trzy pozycje pojawiają się kolejno.
- 1100–1600 ms: dekoracja wygasa, sekcja dostaje krótkie podświetlenie, a komunikat „Workflows odblokowane” z akcją „Otwórz Workflows” pozostaje dostępny.

Dotychczasowy ekran, timer i dane pozostają na miejscu; tło nie znika, nie ma pełnoekranowego restartu ani udawanej migracji. Kolory i linie nawiązują do stylu aplikacji i obu motywów. Bez automatycznego dźwięku, intensywnego migania i długiego konfetti. Animacja może korzystać z SVG oraz CSS/Web Animations; nie wymaga nowego frameworka animacyjnego.

Nakładka nie blokuje Stop timera i innych istotnych akcji. Escape pomija dekorację; zakończenie lub pominięcie zawsze prowadzi do tego samego odblokowanego interfejsu. Fokus pozostaje w logicznym miejscu w ustawieniach, a czytnik ekranu otrzymuje pojedynczy komunikat sukcesu. Przy `prefers-reduced-motion` lub wyłączonych animacjach: krótkie pojawienie sekcji bez ruchu grafu. W wąskim oknie z zamkniętym menu potwierdzenie pojawia się przy przycisku menu; panel nie otwiera się samoczynnie.

Animacja uruchamia się raz po udanej aktywacji, nie po restarcie, odświeżeniu widoku ani każdym zapisie ustawień. Błąd animacji nie cofa aktywacji i nie blokuje funkcji. Podgląd demonstracyjny i testy mogą odtwarzać ją przez kontrolowany stan fixture bez produkcyjnego klucza.

### 4.2 Lista definicji

Tabela: nazwa, opis, wersja opublikowana, ostatnia zmiana, ostatni przebieg i jego status. Akcje: „Nowy”, „Z szablonu”, „Edytuj”, „Duplikuj”, „Uruchom”, „Archiwizuj”, import/eksport JSON.

Stan pusty prowadzi do szablonu „Analiza → implementacja → review”. Import tworzy nieaktywny szkic, nie uruchamia agentów i nie włącza uprawnień zapisanych na innym komputerze. Archiwizacja zachowuje historię.

### 4.3 Edytor grafu

Układ:

```text
Workflow / nazwa   Szkic • niezapisane zmiany   [Zapisz] [Opublikuj] [Uruchom]
┌────────────────┬──────────────────────────────────┬───────────────────────┐
│ Biblioteka     │                                  │ Konfiguracja węzła    │
│ Start          │     Start → Analiza → Kod        │ Nazwa / opis          │
│ Agent          │                      ↓           │ Dostawca / model      │
│ Warunek        │                    Review        │ Wejścia / wynik       │
│ Akceptacja     │                      ↓           │ Uprawnienia / timeout │
│ Koniec         │              Warunek → Akceptacja│                       │
├────────────────┴──────────────────────────────────┴───────────────────────┤
│ Walidacja: lista problemów z przyciskiem przejścia do węzła                │
└──────────────────────────────────────────────────────────────────────────┘
```

Przeciąganie i dodawanie przyciskiem; przesuwanie, zoom, dopasowanie grafu; edycja i usuwanie krawędzi; cofanie/ponawianie zmian szkicu. Autosave może dotyczyć wyłącznie szkicu i musi pokazywać błąd zapisu. Opublikowanie tworzy niezmienną wersję, a trwający przebieg zachowuje poprzednią.

Konfiguracja Agent: rola, dostawca, model, instrukcja, mapowanie wejść, schemat wyniku, profil dostępu, timeout i polityka błędów. Model nie jest zaszyty w szablonie bez możliwości zmiany. Konfiguracja Condition: typowane porównanie pola wyniku, bez eval/JavaScript i wykonywania tekstu modelu. Konfiguracja Approval: opis decyzji i lista artefaktów do przeglądu. End określa wynik końcowy.

Użytkownik może konfigurować graf także bez drag & drop: lista kroków, wybór następnego kroku i edycja połączeń klawiaturą. Statusy mają tekst i ikonę, nie tylko kolor. Zachować motyw jasny/ciemny, fokus i polskie komunikaty. Przy małej szerokości konfiguracja otwiera się w panelu; lista kroków pozostaje dostępna.

### 4.4 Formularz uruchomienia

Pokazuje wersję workflow, zadanie, opis celu, repozytorium, bazowy commit/gałąź, proponowany worktree, modele oraz uprawnienia poszczególnych kroków. Użytkownik może zmienić parametry przed startem.

Preflight sprawdza definicję, narzędzia, profile bezpieczeństwa, dostęp do folderu, Git, miejsce na dysku oraz konflikt z innym przebiegiem. Nie wykonuje zapisu do repozytorium przed kliknięciem „Uruchom”. Brak CLI kieruje do instrukcji konfiguracji; aplikacja nie instaluje go ani nie przejmuje logowania automatycznie.

### 4.5 Monitor i historia

Monitor pokazuje graf z aktualnym krokiem, oś zdarzeń, logi stdout/stderr, wyniki węzłów, listę zmienionych plików, diff i czas wykonania/oczekiwania. Każda próba i runda poprawek ma osobny wpis. Zdarzenia mają kolejność; UI może nadrobić utracone zdarzenia po ponownym otwarciu.

Akcje: „Zatrzymaj”, „Przejdź do folderu”, „Pokaż zadanie”. W stanie odzyskiwania: „Sprawdź stan”, „Ponów krok” lub „Zakończ jako przerwany”. Nie oferować pozornej pauzy procesu CLI. Approval jest trwałym oczekiwaniem pomiędzy krokami.

Historia: filtry po zadaniu, repozytorium, workflow, dacie i statusie; widok szczegółów z wersją definicji i parametrami faktycznie użytymi. Brak tokenów/kosztów u dostawcy jest oznaczony jako „brak danych”, nie zero.

### 4.6 Akceptacja człowieka

Panel pokazuje cel, podsumowanie, wyniki review/testów, diff i dokładną wersję artefaktów. „Akceptuj” kończy przebieg sukcesem; „Odrzuć” kończy odrzuceniem z komentarzem. Dodatkowa runda po odrzuceniu może być nowym uruchomieniem; MVP nie omija w ten sposób limitu trzech poprawek.

Zgoda jest przypisana do `runId`, konkretnego wykonania węzła i skrótu przeglądanego diffu. Zmiana plików po przygotowaniu review unieważnia zgodę i wymaga ponownego przeglądu. Akceptacja nie oznacza merge ani publikacji. Ewentualne zgody narzędzi CLI są odrębnym mechanizmem; niewspierane interaktywne pytanie kończy krok kontrolowanym błędem zamiast zawieszenia.

## 5. Semantyka grafu i silnika

### 5.1 Zasady MVP

- Dokładnie jeden ręczny Trigger; każdy osiągalny przebieg musi mieć drogę do End.
- Trigger ma jedno wyjście, Agent jedno zwykłe wyjście, Condition porty true/false, Approval approve/reject, End nie ma wyjść.
- Wszystkie wejścia i odwołania do wyników są typowane i walidowane także w Rust.
- Rozgałęzienie wybiera jedną drogę. Brak równoległego wykonania i ogólnych joinów w MVP; scalenie alternatywnych ścieżek jest dozwolone jako wybór jednego poprzednika.
- Brak dowolnych cykli. Jedyny dopuszczalny cykl MVP to jawnie oznaczony blok poprawek: Condition → Agent poprawek → Review → ten sam Condition, z limitem.
- Walidator wymaga wyjścia po wyczerpaniu limitu; silnik kończy wtedy przebieg `Failed` z kodem `ReviewLimitExceeded`. Po trzeciej poprawce odbywa się jeszcze review, ale nie czwarta poprawka.
- Każda iteracja tworzy nowe NodeExecution. Nie nadpisuje wcześniejszego wyniku review.
- Kolejka gotowych węzłów i trwałe przejścia stanu sterują wykonaniem; pozycje na canvasie nie mają znaczenia wykonawczego.
- Początkowy limit globalny: jeden działający krok agenta; dodatkowe przebiegi oczekują. Approval zwalnia slot procesu, ale zachowuje rezerwację workspace.

### 5.2 Stany

`WorkflowRun`: Queued → Running → WaitingForApproval → Running → Succeeded/Rejected. Dodatkowe przejścia: Running/WaitingForApproval → Cancelling → Cancelled; błędy → Failed; awaria hosta → Interrupted. Interrupted wymaga świadomego odzyskania. Przebieg Queued można anulować bez uruchamiania procesów.

`NodeExecution`: Pending → Running → Completed/Failed/Cancelled/Interrupted; Approval: Pending → Waiting → Completed/Rejected/Cancelled. Węzły niewybranej gałęzi są Skipped. Status końcowy przebiegu i status kroku nie mogą sobie przeczyć.

Odwołania wejść wskazują konkretny dostępny wynik danej iteracji; nie odczytują przypadkowo ostatniego wpisu z innego przebiegu. Brak wymaganej wartości i niezgodny JSON to błąd, a nie domyślna decyzja „review OK”.

### 5.3 Błędy i ponawianie

Odróżnić: brak CLI, brak logowania, nieobsługiwana wersja, błąd uruchomienia, błąd protokołu, niezgodny wynik, timeout, anulowanie, naruszenie uprawnień, błąd zapisu bazy i wyczerpanie limitu review.

Domyślnie brak automatycznego ponawiania kroków zmieniających pliki. Retry techniczny może działać dla operacji bez skutków ubocznych, z limitem i opóźnieniem; nie jest rundą poprawek. Nie zakładać exactly-once dla CLI. Każda próba ma osobny identyfikator i zapisywany zamiar uruchomienia.

### 5.4 Trwałość i restart

Zapisać zamiar uruchomienia przed startem procesu. Po wykonaniu w jednej transakcji utrwalić wynik kroku, przejście stanu i zdarzenie. Dopiero potem planować następny krok. Długie operacje i odczyt strumieni nie trzymają blokady SQLite.

Po restarcie odczytać wszystkie niekońcowe przebiegi. WaitingForApproval pozostaje oczekiwaniem po ponownej walidacji artefaktów. Running/Cancelling oznaczyć do rekonsyliacji; nie uruchamiać kolejnego agenta w ciemno. PID nie jest wystarczającą tożsamością procesu. Zweryfikować własność procesu i workspace, dostępność artefaktów oraz identyfikator sesji.

MVP gwarantuje historię i kontrolowane odzyskanie; nie gwarantuje kontynuacji w połowie narzędzia modelu. Wznowienie sesji CLI to zdolność adaptera, niezależna od wznowienia grafu, i wymaga ponownego narzucenia polityki dostępu.

## 6. Architektura implementacji

Silnik jako osobny crate Rust bez zależności od Tauri. Host Tauri dostarcza repozytorium danych, zegar, zdarzenia i runnerów. Proponowany układ, do potwierdzenia w analizie szczegółowej:

```text
src-tauri/
  crates/workflow-core/
    Cargo.toml
    src/{lib,model,validation,engine,policy,ports}.rs
  src/workflows/
    mod.rs
    commands.rs
    service.rs
    repository.rs
    migrations.rs
    events.rs
    recovery.rs
    workspace.rs
    runners/{mod,claude,codex,process}.rs
src/
  api/workflows.ts
  workflows.ts
  preview/workflows.ts
  views/WorkflowsView.vue
  components/workflows/
    WorkflowList.vue
    WorkflowDesigner.vue
    WorkflowNodePanel.vue
    WorkflowRunForm.vue
    WorkflowRunMonitor.vue
    WorkflowApproval.vue
    nodes/
tests/workflows.spec.ts
tests/workflow-runs.spec.ts
```

`workflow-core` zawiera domenę, walidację, przejścia i planowanie. Porty: `RunRepository`, `AgentRunner`, `EventSink`, `Clock`, `WorkspaceAccess`. Konkretne procesy Windows, SQLite i `AppHandle` zostają poza core. Core nie ma dostępu do sekretów aplikacji ani globalnego store UI.

Tokio jest proponowaną warstwą asynchronicznych procesów runtime, nie warunkiem czystej domeny. Odczyt stdout i stderr musi być równoległy, z ograniczeniami rozmiaru i buforów. Zegar i runner są zastępowalne w testach.

Najpierw adaptery nad bezpiecznymi fragmentami istniejącego kodu. Wydzielanie wspólnego kodu AI wykonywać małymi zmianami z testami regresji; nie przerabiać równocześnie całego czatu, wszystkich asystentów i workflow.

Nowy crate trzeba jawnie włączyć do testów, formatowania i CI: obecna komenda na manifest aplikacji nie gwarantuje wykonania testów zależności będącej osobnym crate. Ustalić workspace Cargo albo dodać odrębne komendy na jego manifest.

## 7. Model danych i kontrakty

### 7.1 Encje i tabele

| Encja / proponowana tabela | Najważniejsze dane i reguły |
| --- | --- |
| Workflow / `workflows` | id, nazwa, opis, szkic JSON, rewizja szkicu, archived_at. |
| WorkflowDefinition / `workflow_versions` | workflow_id, version, schema_version, JSON, hash, created_at; niezmienne po publikacji. |
| AgentProfile / `workflow_agent_profiles` | provider, model, role, policy; bez poświadczeń, profil ma rewizję. |
| WorkflowRun / `workflow_runs` | id, version_id, snapshot definicji i profili, task_id opcjonalny, snapshot tytułu zadania, repo, workspace, base_commit, status, timestamps, error. |
| NodeExecution / `workflow_node_executions` | id, run_id, node_id, iteration, attempt, status, input/output refs, provider session id, czas, kod wyjścia, error. |
| Event / `workflow_events` | run_id, sequence, node_execution_id opcjonalny, type, payload, timestamp; unikalny sequence w przebiegu. |
| Approval / `workflow_approvals` | run_id, execution_id, artifact_hash, pending/approved/rejected/invalidated, komentarz, czas decyzji. |
| Artifact / `workflow_artifacts` | id, run_id, execution_id, rodzaj, relatywna ścieżka, hash, rozmiar, utworzenie. |
| Workspace reservation / `workflow_workspaces` | kanoniczny workspace, run_id, stan rezerwacji; ochrona przed dwoma writerami. |
| Schema / `workflow_schema_migrations` | wersja migracji modułu, czas; niezależne od wersji formatu grafu. |

Identyfikatory przebiegów i wykonań są niezależne od PID i identyfikatora sesji dostawcy. Usunięcie zadania nie usuwa historii workflow: nullable powiązanie i zachowany snapshot. Indeksy: status/daty przebiegów, task_id, run_id/sequence i run_id/node_id/iteration/attempt. Klucze obce i zachowanie archiwizacji określić w migracji, nie przez przypadkowy cascade.

Szkic zapisuje się z oczekiwaną rewizją; konflikt zgłasza błąd zamiast nadpisania drugiego okna. Snapshot uruchomienia obejmuje skuteczne modele, uprawnienia, limity oraz wersje CLI. Import definicji nie importuje zaufania do lokalnych ścieżek ani profili.

### 7.2 Artefakty i backup

Duże logi i diffy w katalogu danych aplikacji `workflows/runs/<runId>/`, baza zawiera indeks i skróty. Nie zapisywać ich w repozytorium użytkownika. Zapisy plików atomowe; uszkodzony lub brakujący artefakt jest widoczny w historii.

Backup workflow wymaga rozszerzenia istniejącej kopii SQLite o manifest i artefakty lub jawnego wariantu „tylko definicje i historia bez logów”. Pełne archiwum powinno walidować skróty i ścieżki przed odtworzeniem. Przywrócenie nie uruchamia procesów i nie odtwarza aktywnych rezerwacji jako zaufanych. Poświadczenia pozostają w magazynie systemowym.

Proponowana retencja logów: 30 dni, konfigurowalna. Definicje, wyniki i decyzje pozostają do jawnego usunięcia. Nigdy nie usuwać artefaktów aktywnego przebiegu lub oczekującej zgody. Czyszczenie musi oznaczyć w historii brak pełnych logów.

### 7.3 IPC i zdarzenia

Proponowane komendy: `workflow_list`, `workflow_get`, `workflow_save_draft`, `workflow_validate`, `workflow_publish`, `workflow_archive`, `workflow_import`, `workflow_export`, `workflow_preflight`, `workflow_start`, `workflow_runs`, `workflow_run_get`, `workflow_run_events`, `workflow_cancel`, `workflow_decide`, `workflow_recover`, `workflow_artifact_read`, `workflow_profiles`.

Każda komenda mutująca waliduje dane i uprawnienia w Rust. Start i decyzja mają klucz idempotencji; podwójne kliknięcie nie tworzy dwóch procesów ani dwóch zgód. `workflow_decide` wymaga oczekiwanej rewizji stanu i hasha artefaktu. Klient nie podaje dowolnej ścieżki pliku do odczytu: podaje identyfikator artefaktu.

Zdarzenie `workflow-event`: `runId`, `sequence`, `executionId`, `type`, `timestamp`, `payload`. Typy: run/node state changed, message, tool started/finished, artifact added, approval requested/resolved, error. Snapshot + odczyt od sequence uzupełniają luki i usuwają duplikaty; event bus nie jest jedyną historią.

### 7.4 Wyniki między agentami

Analiza: summary, plan[], acceptanceCriteria[], risks[]. Implementacja: summary, changedFiles[], checks[] z wynikiem i odniesieniem do logu. Review: verdict (`approved`/`changes_requested`), findings[] z plikiem, miejscem, wagą i opisem, checkedArtifactHash.

Schemat sprawdzany po stronie hosta; wynik modelu nie jest dowodem wykonania testu. Odróżnić raport agenta od kodu wyjścia narzędzia i zachowanego logu. Przekazywać wybrane pola i referencje do artefaktów, nie cały surowy strumień.

## 8. Runnery, workspace i bezpieczeństwo

### 8.1 Kontrakt runnera

Wejście: executionId, workspace, model, instrukcja, typowane wejścia, policy, timeout, cancel token, opcjonalny sessionId. Wyjście: zdarzenia i końcowy wynik z walidacją. Możliwości adaptera są jawne: streaming, resume, structured output, read-only, ograniczenia narzędzi i sieci.

Obsłużyć różne formaty Claude i Codex, częściowe linie UTF-8/JSONL, nieznane zdarzenia, stderr, limit długości linii, brak zdarzenia końcowego, zakończenie kodem niezerowym i konflikt anulowania z zakończeniem. Zakończenie procesu kodem 0 bez poprawnego wyniku nie oznacza ukończenia kroku.

Nie sklejać promptów w komendę powłoki. Używać list argumentów i stdin; osobno zweryfikować wrappery `.cmd` i ścieżki ze spacjami/znakami specjalnymi na Windows. Środowisko ograniczyć do potrzebnego zestawu, zachowując wymagane mechanizmy istniejącego logowania; nie logować pełnego environment.

### 8.2 Workspace

Preferowane MVP: osobny worktree Git z ustalonego commita, tworzony po starcie przebiegu. Niezacommitowane zmiany w katalogu źródłowym nie są przenoszone automatycznie; użytkownik dostaje jasny komunikat i wybór uporządkowania bazy. Workspace procesu pozostaje dostępny po zakończeniu do przeglądu.

Implementacja musi działać bez Orki. Istniejąca integracja Orca pozostaje opcjonalnym sposobem otwarcia/zarządzania pracą, a nie zależnością wykonawczą. Nie uruchamiać równocześnie agenta przez Orkę i własny runner dla tego samego kroku.

Rezerwować workspace na cały przebieg, także podczas Approval. W MVP nie współdzielić zapisywalnego katalogu pomiędzy przebiegami. Normalizacja ścieżek na potrzeby UI nie zastępuje kanonikalizacji bezpieczeństwa. Uwzględnić junctions, symlinki i katalog Git współdzielony przez worktree. Ścieżki sieciowe i niezbadane układy odrzucać z jasnym komunikatem.

### 8.3 Uprawnienia

Profile: analiza/review z odczytem zatwierdzonego repozytorium, implementacja z zapisem w workspace. Brak automatycznego dostępu do innych repozytoriów, ustawień kont i MCP użytkownika. Sieć i narzędzia zewnętrzne są osobnymi możliwościami, nie wynikają z roli „reviewer”.

Read-only musi być egzekwowane mechanizmem potwierdzonym dla danego adaptera i Windows. Sam prompt, kopia katalogu, lista nazw narzędzi ani Job Object nie są sandboxem systemu plików. Jeśli dostępny runtime nie potrafi zapewnić wymaganego ograniczenia, preflight blokuje ten profil; nie deklarować bezpieczeństwa bez testu. Nie modyfikować ACL źródłowego repozytorium jako doraźnego rozwiązania.

Rozważyć Job Objects z kończeniem procesów przy zamknięciu hosta; sprawdzić dziedziczenie, escape i procesy zagnieżdżone. Uzupełnić istniejące `taskkill` kontrolą wyniku i potwierdzeniem końca. Cancelled można zapisać dopiero po ustaleniu zakończenia własnego drzewa; niepewny stan wymaga rekonsyliacji i utrzymania blokady workspace.

Treść repozytorium, wyniki modeli i MCP są danymi niezaufanymi. Nie mogą zmienić polityki, podnieść uprawnień, wyrazić zgody za użytkownika ani dopisać polecenia wykonawczego do Condition. Redagować sekrety przed zapisem logów i eksportem; nie obiecywać pełnej automatycznej anonimizacji kodu.

## 9. Integracja z zadaniami i monitoringiem

Przebieg przechowuje jawne task_id; dopasowanie po folderze jest pomocą, nie nadpisuje wyboru. Korelacja sesji: runId + executionId + providerSessionId. Własne zdarzenia runtime są źródłem stanu workflow; hooki dostarczają pomocnicze informacje i nie kończą przebiegu na podstawie samego timeoutu obserwacji.

Wybrać jeden tor naliczania czasu: dla własnych wykonawców zdarzenia runtime, dla zewnętrznych sesji istniejące hooki/obserwacja. Wspólny klucz korelacji eliminuje duplikaty. Czas pracy agenta, czas oczekiwania na człowieka i timer zadania pozostają odrębnymi metrykami. Nie dodawać czasu workflow bezpośrednio do raportu pracy człowieka.

Powiadomienie o Approval i końcu wykonania kieruje do właściwego przebiegu i jest deduplikowane. Integrację z aktualnie rozwijanym inboxem ustalić po stabilizacji jego API. Nie dodawać drugiego niezależnego centrum powiadomień.

## 10. Etapy realizacji i kryteria odbioru

Każdy etap ma własny zakres zmian i testy. Checkbox oznacza wykonanie z dowodem, nie samo istnienie kodu.

### E0 — analiza i decyzje (A01–A10)

- [x] Wykonać analizę z rozdziału 3 i zapisać jej wyniki (`E0_ANALIZA.md`).
- [x] Przygotować decyzje: podział core/host, baza i backup, workspace, uprawnienia runnerów, semantyka pętli, Vue Flow.
- [ ] Uzupełnić makiety ekranów i kryteria dostępności.
- [ ] Uzgodnić grupy menu, charakter klucza, jego wydawanie i weryfikację oraz storyboard animacji odblokowania.
- [x] Udokumentować wspierane wersje CLI i blokady techniczne.

Odbiór: znane punkty integracji i ryzyka; potwierdzona wykonalność read-only oraz nadzoru procesów albo jawny plan usunięcia blokady. Brak instalowania całego nowego stosu „na zapas”.

### E1 — domena i trwałe definicje

- [x] Utworzyć crate core, typy grafu i błędów walidacji (stany przebiegu dochodzą w E3).
- [x] Dodać walidator grafu, mapowań danych i ograniczonej pętli.
- [x] Dodać migracje, repozytorium szkiców/wersji, archiwizację i import/eksport.
- [x] Dodać API TS, komendy Tauri i preview (fixture `tests/fixtures/tauri.ts` dochodzi z UI w E2).
- [x] Włączyć testy crate do lokalnych skryptów i CI.

Odbiór: definicja zapisuje się i odczytuje; publikacja jest niezmienna; błędny graf nie może być uruchomiony także przez bezpośrednie IPC. Test migracji istniejącej bazy zachowuje zadania i czas.

### E2 — edytor i projekt funkcjonalny w aplikacji

- [x] Dodać pozycję nawigacji, listę i edytor (decyzja E2: własny canvas SVG zamiast Vue Flow, patrz dziennik).
- [x] Uporządkować menu w grupy i odsłaniać Workflows jako rozszerzenie obecnej aplikacji, bez przełącznika trybów.
- [x] Dodać ustawienia aktywacji kluczem, trwały stan uprawnienia i kontrolę dostępu w backendzie wszystkich komend modułu.
- [x] Zaimplementować animację odblokowania, pomijanie przez Escape, wariant reduced motion i zachowanie na małym ekranie.
- [x] Zaimplementować pięć typów węzłów, panel parametrów i edycję krawędzi.
- [x] Dodać zapis szkicu, publikację, informację o niezapisanych zmianach i konflikcie rewizji.
- [x] Dodać szablon procesu oraz dostępny klawiaturą wariant listy kroków.
- [ ] Uzupełnić preview, Playwright i zrzuty obu motywów (preview i Playwright są; nowych zrzutów wzorcowych nie dodano).

Odbiór: użytkownik tworzy i ponownie otwiera poprawny graf; błąd wskazuje konkretny węzeł. Przyciski uruchamiania nie udają działającego runtime przed jego wdrożeniem.

### E3 — silnik na procesach zastępczych

- [x] Zaimplementować kolejkę, przejścia i trwałe NodeExecution.
- [x] Dodać typowane wyniki, warunki, trzy rundy poprawek i terminalne ścieżki.
- [x] Dodać trwałe Approval oraz idempotentny start i decyzje.
- [x] Dodać timeouty, anulowanie, błędy i strumień zdarzeń z sequence.
- [x] Uruchomić scenariusz end-to-end na deterministycznych runnerach testowych.

Odbiór: ten sam zapisany zestaw wyników prowadzi do tej samej ścieżki grafu; żaden test nie potrzebuje konta AI. Restart na granicy kroków nie dubluje kolejnego uruchomienia.

### E4 — runtime Windows i adaptery

- [x] Wydzielić współdzielone elementy wykrywania/parsing bez regresji asystenta
  (`crates/agent-cli`; testy `ai::` i `ai_chat::` bez zmian: 140 passed).
- [x] Dodać preflight, profil modelu i kontrolowane środowisko dla każdego adaptera
  (`workflows/preflight.rs`, `runners/env.rs`, `runners/claude.rs`, `runners/codex.rs`).
- [x] Dodać workspace/worktree i rezerwacje (`workflows/workspace.rs`, migracja schematu 3).
- [x] Zaimplementować streaming, wynik, identyfikator sesji i nadzór drzewa procesów
  (`agent-cli/src/process.rs`, `tree.rs`, `runners/mod.rs`).
- [x] Egzekwować profile dostępu i blokowanie nieobsługiwanych konfiguracji
  (`runners/guard.rs`, preflight; Claude read-only to kontrola CLI, nie izolacja).
- [x] Przygotować checklistę testu ręcznego dla użytkownika na prawdziwych CLI
  (`E4_TEST_RECZNY.md`; wykonanie jej na prawdziwych CLI pozostaje otwarte).
- [x] Pełna weryfikacja: `npm run check` z kodem 0 (krok sprawdzenia etapu po rundach 1 i 2
  poprawek: build, Playwright 725 testów, `cargo fmt --check`, `cargo test --locked`; patrz dziennik).

Odbiór: procesy zastępcze potwierdzają timeout, kill i ograniczenia; użytkownik potwierdza rzeczywisty pipeline na testowym repozytorium i wspieranych wersjach obu CLI. Brak takiej walidacji pozostaje jawnie oznaczony.

### E5 — monitor, zadania i zgody

- [ ] Dodać formularz startu z karty zadania i listy definicji.
- [x] Dodać monitor grafu, historię, filtrowanie, logi i artefakty.
- [x] Dodać diff, wyniki review oraz decyzję związaną z hashem artefaktu.
- [x] Połączyć przebiegi z tablicą agentów, czasem i powiadomieniami.
- [x] Dodać odtwarzanie zdarzeń po ponownym otwarciu widoku i ograniczenie renderowania dużych logów.

Odbiór: pełny scenariusz działa z UI; nie podwaja czasu i nie zmienia automatycznie timera ani zgłoszeń. Podwójne kliknięcie zgody ma jeden skutek.

### E6 — recovery, backup i odporność

- [x] Dodać rekonsyliację procesów i przebiegów po starcie aplikacji.
- [x] Obsłużyć przerwanie w trakcie zapisu wyniku, oczekiwania i anulowania.
- [x] Dodać backup/restore artefaktów, retencję i obsługę braku miejsca.
- [x] Zablokować restore i aktualizację podczas aktywnych procesów albo przeprowadzić kontrolowane zakończenie przed operacją.
- [ ] Zweryfikować zamknięcie okna do traya versus faktyczne wyjście z aplikacji.

Odbiór: restart nie gubi historii i zgód, nie uruchamia ponownie zmian w ciemno, nie pozostawia niekontrolowanego writera. Restore nie wznawia starych poleceń.

### E7 — regresja i wydanie

- [ ] Przeprowadzić przegląd domeny, bezpieczeństwa, UI i migracji.
- [ ] Wykonać pełne CI oraz testy natywne na Windows.
- [ ] Uzupełnić README, instrukcję użytkownika, troubleshooting i release notes.
- [x] Udostępnić moduł początkowo jako funkcję eksperymentalną odblokowywaną kluczem w ustawieniach, zachowując pełne śledzenie bez klucza.
- [ ] Zebrać wynik pilotażu; poprawić blokery przed oznaczeniem MVP jako gotowego.
- [ ] Przygotować wydanie według RELEASE.md; podpisanie i publikację uruchamia użytkownik.

Odbiór: spełniona lista z rozdziału 13, znane ograniczenia opisane, sprawdzona instalacja i aktualizacja na Windows. Wyłączenie funkcji zatrzymuje nowe starty i zachowuje historię; nie usuwa bazy.

Zależności: E0 → E1 → E2/E3 → E4 → E5 → E6 → E7. E2 i E3 można organizacyjnie prowadzić niezależnie po ustaleniu kontraktów; nie jest to polecenie uruchomienia dodatkowych agentów. Nie deklarować dat zakończenia przed zamknięciem E0. Największa niewiadoma: egzekwowanie uprawnień i niezawodny lifecycle CLI na Windows.

## 11. Plan testów

| Warstwa | Scenariusze obowiązkowe |
| --- | --- |
| Graf | Brak startu/końca, zdublowane ID, osierocony węzeł, brak portu, błędne typy, nieograniczony cykl, niepoprawna pętla poprawek. |
| Silnik | Sukces, obie gałęzie warunku, maksymalnie trzy poprawki, odrzucenie, timeout, cancel, wyścig końca i cancel, retry bez podwajania skutków. |
| Baza | Migracja starej bazy, ponowne wykonanie migracji, rollback, niezmienność wersji, konflikt szkicu, duplikat requestId, archiwizacja i usunięte zadanie. |
| Parsery | Fixtures obu dostawców, nieznane eventy, uszkodzona linia, fragmenty UTF-8, duży stderr, brak finału, niezerowy exit, błędny JSON wyniku. |
| Procesy Windows | Ścieżki ze spacjami i znakami narodowymi, wrapper cmd, potomkowie, timeout, awaria hosta, blokada workspace; kontrolowane programy testowe. |
| Uprawnienia | Próba zapisu reviewera, zapis poza workspace, junction/path traversal, niedozwolone MCP, brak możliwości użycia tekstu modelu jako zgody. |
| Recovery | Awaria przed spawn, po spawn przed zapisem PID, po wyjściu przed commit, podczas Approval i cancel, ponowne użycie PID, brak artefaktu. |
| UI | Utworzenie i edycja, publikacja, formularz startu, monitor, zgoda/odrzucenie, klawiatura, oba motywy, małe okno, polskie błędy. |
| Aktywacja | Brak/błędny/poprawny klucz, błąd zapisu uprawnienia, podwójne kliknięcie, trwałość po restarcie, brak klucza w logach, odrzucenie bezpośredniego IPC bez uprawnienia. |
| Menu i animacja | Śledzenie dostępne przed i po aktywacji, brak przełącznika trybów, odsłonięcie sekcji dokładnie raz, Escape, reduced motion, fokus, oba motywy, mały ekran i awaria animacji bez utraty dostępu. |
| Integracja | Korelacja sesji, brak podwójnego czasu, brak zmiany timera, brak zapisów do Jira/Redmine/TS, chat AI zachowuje dotychczasową izolację. |
| Backup | Pełne odtworzenie, brakujące pliki, błędny hash, złośliwa ścieżka w archiwum, niezgodna wersja schematu, brak automatycznego startu. |

Zgodnie z repozytorium: fixture Tauri aktualizować wraz z kontraktami backendu, ale nie traktować kopii logiki w TypeScript jako dowodu poprawności Rust. Core i persistence wymagają własnych testów. UI testować bez prawdziwych dostawców i płatnych wywołań.

Komendy odbioru implementacji: `npm run build`, `npm test`, `npm run test:visual`, `cargo fmt --manifest-path src-tauri/Cargo.toml --check`, `cargo test --manifest-path src-tauri/Cargo.toml --locked` oraz testy nowego crate dodane w E1. Na etapie wydania także `npm run tauri build -- --no-bundle`. Skrypt `npm run check` rozszerzyć o core. To lista przyszłych kontroli, nie raport ich wykonania w trakcie przygotowania planu.

Cele wydajności do pomiaru na opisanej maszynie Windows: responsywny edytor 100 węzłów, brak blokowania UI przy strumieniu 100 zdarzeń/s, paginowana historia i wirtualizowane logi. Rozmiary promptów, wyników, logów i liczba zdarzeń muszą mieć jawne limity. Ostateczne wartości ustalić po prototypie, a przekroczenie pokazać użytkownikowi zamiast cicho obcinać dane sterujące.

## 12. Ryzyka, decyzje i prace po MVP

| Ryzyko | Działanie / warunek zamknięcia |
| --- | --- |
| Brak realnego read-only dla dostawcy na Windows | Spike E0, test negatywny, blokada profilu bez potwierdzonego mechanizmu. |
| Zmienione formaty i flagi CLI | Macierz wersji, fixtures, preflight i jawna niezgodność zamiast cichego fallbacku. |
| Powielenie zmian po restarcie | Dziennik zamiarów, stan Interrupted, ręczne odzyskanie kroku ze skutkami ubocznymi. |
| Nadpisanie pracy użytkownika | Wydzielony workspace, bazowy commit, blokada writerów, brak automatycznego reset/clean/merge. |
| Rozrost logów lub ujawnienie sekretów | Limity, redakcja, retencja, świadomy eksport i brak pełnego environment w logach. |
| Blokada istniejącej bazy przez długi proces | Krótkie transakcje, osobna obsługa strumieni, test obciążenia timera podczas workflow. |
| Konflikt z monitoringiem i hookami | Osobna tożsamość wykonania, deduplikacja i jeden tor naliczania czasu. |
| Regresja obecnego asystenta | Adaptery zamiast masowej przebudowy; kontrakty izolacji i fixture czatu. |
| Nieaktualna zgoda na zmieniony diff | Hash artefaktu i ponowna walidacja przed zakończeniem. |
| Zbyt szeroki MVP | Zamrożony zestaw pięciu typów; przyszłe integracje w osobnych etapach. |

Decyzje startowe: wspólna aplikacja i baza, osobny core, istniejący wygląd, jawny wybór modeli, ręczny Trigger, izolowany worktree, brak automatycznego merge, pojedynczy aktywny agent. Do rozstrzygnięcia w E0: dokładny mechanizm sandboxa, wspierane wersje CLI, Cargo workspace, format pełnego backupu i limity zasobów.

Po MVP, w kolejności wartości:

1. Command z osobną polityką wykonywania i wiarygodnym wynikiem testów/builda.
2. OpenCode przez ten sam kontrakt i zestaw testów zdolności.
3. Ogólny Loop oraz Parallel/Join z jawnie zdefiniowanym all/any/fail-fast, izolacją workspace i anulowaniem gałęzi.
4. Harmonogram z polityką pominiętych uruchomień, potem uwierzytelnione webhooki z deduplikacją.
5. MCP/HTTP z deklarowanymi uprawnieniami, ograniczonymi sekretami i potwierdzeniami skutków zewnętrznych.
6. Integracje istniejących Jira/Redmine i ewentualnego GitLaba; preferować ponowne użycie modułów i nie duplikować konfiguracji kont.

## 13. Definition of Done dla MVP

- [ ] Bez klucza działa dotychczasowe śledzenie; poprawny klucz w ustawieniach trwale dodaje Workflows, bez rozdzielenia aplikacji na dwa tryby.
- [ ] Menu ma czytelne grupy, a animacja odblokowania działa również w wariancie z ograniczonym ruchem i nie blokuje timera.
- [ ] Backend egzekwuje uprawnienie do modułu; historia i zatrzymywanie procesów pozostają dostępne w sytuacjach odzyskiwania dostępu.
- [ ] Użytkownik tworzy, waliduje, zapisuje i publikuje graf w TukTuk Track.
- [ ] Wybiera zadanie, repozytorium, dostawców i modele przed startem.
- [ ] Claude analizuje, Codex implementuje, Claude wykonuje egzekwowane review read-only.
- [ ] Nieudane review prowadzi do najwyżej trzech rund poprawek i ponownego review.
- [ ] Pozytywne review czeka na człowieka; zgoda dotyczy konkretnego diffu.
- [ ] Statusy, logi, wyniki i zmienione pliki są dostępne na żywo i w historii.
- [ ] Zatrzymanie kończy własne drzewo procesów i nie uruchamia następnego kroku.
- [ ] Restart zachowuje definicję, wersję, historię i oczekującą decyzję; nie dubluje pracy automatycznie.
- [ ] Repozytorium źródłowe i istniejące zmiany użytkownika są zachowane.
- [ ] Timer, raporty, czat, tablica agentów i integracje TukTuka działają bez regresji.
- [ ] Preview i testy automatyczne działają bez prawdziwych agentów.
- [ ] Backup/restore i migracja są przetestowane, a ograniczenia opisane.
- [ ] Użytkownik wykonał test akceptacyjny prawdziwych CLI na Windows.
- [ ] Dokumentacja, checklisty wydania i informacja o wspieranych wersjach są gotowe.

## 14. Jak prowadzić realizację tego planu

Przy każdym etapie dopisywać: datę, zakres, zmienione pliki, decyzje, wykonane testy z wynikami i pozostałe blokery. Nie zaznaczać odbioru na podstawie deklaracji agenta bez dowodów. Aktualizować plan po odkryciu ograniczeń CLI lub istniejącego kodu.

Kolejne etapy (E3–E7) uruchamia się zapisanym workflow `workflows-stage` (commit po każdym etapie, bez push); prompt i opis w [PROMPT_ETAPU.md](PROMPT_ETAPU.md).

Pierwsza sesja implementacyjna zaczyna od E0 i aktualnego stanu repozytorium, następnie przygotowuje kontrakty E1. Nie zaczyna od instalacji Nuxt UI/SQLx ani budowy osobnej aplikacji. Wybór modelu do pracy pozostaje po stronie użytkownika w aplikacji.

## 15. Dziennik realizacji

### 2026-11-05 — E0 i E1

- **E0:** analiza A01–A10 i decyzje w `E0_ANALIZA.md`. Otwarte: makiety i dostępność (przy E2),
  storyboard animacji (opis w 4.1.2 przyjęty), akceptacja modelu read-only dla Claude (blokuje E4).
- **E1, pliki:** `src-tauri/crates/workflow-core/` (model, walidacja, szablony, testy),
  `src-tauri/src/workflows/{mod,access,store}.rs` i `store/tests.rs`, `db.rs` (`init_tables`),
  `backup.rs` (`workflows_unlocked` w `KEEP_KEYS`), `main.rs` (12 komend `workflow_*`),
  `src-tauri/Cargo.toml` (workspace), `src/api/workflows.ts`, `src/api/workflowValidate.ts`,
  `src/api/workflowTemplates.ts`, `src/preview/workflows.ts`, `src/workflows.ts`,
  `tests/workflow-definitions.spec.ts`, `scripts/check.mjs` i CI (`cargo fmt --all`).
- **Decyzje E1:** identyfikatory INTEGER; wersje niezmienne wyzwalaczami SQLite; ponowna publikacja
  bez zmian zwraca tę samą wersję; szkic może mieć błędy walidacji, publikacja nie; import tworzy
  nieopublikowany szkic; wszystkie komendy poza `workflow_access`/`workflow_unlock` wymagają
  odblokowania w Rust.
- **Przegląd (rust-reviewer, typescript-reviewer) i poprawki:** warunek może czytać tylko wynik
  kroku leżącego na każdej drodze od Startu (dominacja, nie sama osiągalność); skrót szkicu w kolumnie
  `draft_hash` liczony z zapisanego tekstu, a wersja dostaje dokładnie ten tekst; limit szkicu 1 MB
  także dla bezpośredniego IPC; podgląd parsuje niezaufany graf jak serde (`src/api/workflowParse.ts`),
  ma kanoniczną kolejność kluczy i ponownie czyta stan przed zapisem publikacji.
- **Testy:** `workflow-core` 21 passed; `workflows::*` 20 passed (w tym migracja istniejącej bazy
  z zadaniami); Playwright `workflow-definitions.spec.ts` 17 passed.
- **Pozostaje:** E2 (UI: sekcja Ustawień z kluczem, grupy menu, animacja, lista, edytor, fixture).

### 2026-11-05 — E2 (interfejs definicji)

- **Pliki:** `src/workflowAccess.ts` (wspólny stan odblokowania i jednorazowy znacznik animacji),
  `src/components/settings/WorkflowsSection.vue`, `src/components/WorkflowReveal.vue`,
  `src/components/AppNav.vue` (grupy Praca, Agenci, Workflows, Integracje, Zasoby),
  `src/styles/layout.css`, `src/App.vue` (widoki `workflows` i `workflow-definitions`; bez klucza
  prośba o widok prowadzi do Ustawień → Workflows), `src/views/WorkflowsView.vue`,
  `src/components/workflows/{WorkflowEditor,WorkflowGraphView,WorkflowNodePanel}.vue`,
  `src/workflowEdit.ts`, `src/navigation.ts` (sekcja Ustawień), `tests/fixtures/tauri.ts`
  (`workflow_access`, `workflow_unlock` z testowym kluczem), `tests/workflows-ui.spec.ts`.
- **Decyzje E2:**
  - Canvas: własny SVG (pozycje z definicji, przeciąganie, powiększenie, dopasowanie, łuki pętli)
    zamiast Vue Flow. Powód: brak nowej zależności, pięć typów węzłów i jeden port na wyjście
    wystarczą; pełna edycja klawiaturą jest na liście kroków. Do ponownej oceny, gdy dojdą
    Parallel/Join.
  - Menu Workflows: „Przegląd” i „Definicje”. „Uruchomienia” dojdą z runtime (E3/E5), żeby nie
    udawać działania; z tego samego powodu nie ma przycisku „Uruchom”.
  - Eksport w oknie z polem tekstowym i „Kopiuj” (działa w aplikacji i podglądzie bez dialogów
    plików); import przez wklejenie JSON.
  - Cofanie/ponawianie w sesji edycji (Ctrl+Z / Ctrl+Y poza polami tekstowymi), zmiana pola
    tekstowego to jeden krok (zdarzenie `change`).
- **Testy:** `tests/workflows-ui.spec.ts` 10 passed (grupy menu, błędny i poprawny klucz, Escape,
  reduced motion, nakładka nie blokuje kliknięć, przekierowanie bez klucza, edytor: walidacja,
  cofanie, zapis, publikacja, dodanie kroku, przejście do kroku, porzucenie zmian, import i eksport).
  `tests/visual.spec.ts` bez zmian wzorców (90 passed). Sprawdzone ręcznie w podglądzie przeglądarki.
- **Pozostaje z E2:** zrzuty wzorcowe nowych widoków w obu motywach. Następny etap: E3.

### 2026-11-05 — STATUS: przerwane w trakcie poprawek po przeglądzie E2

Commit `1c8b2d3` zawiera E0+E1. Cały E2 jest **niezacommitowany**. Poprawki z przeglądu vue-reviewer
zostały przerwane w połowie, więc kod jest w stanie przejściowym i **typecheck nie przejdzie**:

- `src/components/workflows/WorkflowEditor.vue` używa `onBeforeUnmount`, ale nie importuje go z `vue`
  (brakująca edycja importu).
- W tym samym pliku `ConfirmDialog` ma nadal `@confirm="emit('close')"`; ma wołać `discard`
  (funkcja jest już dopisana).

Zrobione w poprawkach:
- Okna w `WorkflowsView` przepięte na `ConfirmDialog` (fokus, Esc, powrót fokusu); fokus w polu
  importu.
- `@open` w `App.vue` przechodzi przez `navigate()`.
- Edytor: migawka wysłanego stanu przy zapisie, `fixSelection` po cofnięciu i ponowieniu, skróty
  wyłączone przy otwartym oknie, `focusStep('')` ignorowane, `unsavedDrafts` (w `workflowEdit.ts`)
  zachowuje niezapisane zmiany przy wyjściu z widoku i przywraca je przy ponownym otwarciu.

Do zrobienia potem:
1. Dokończyć dwie edycje powyżej.
2. Ustawienia → Workflows: po odblokowaniu fokus na „Otwórz Workflows” i stały region statusu.
3. Testy przywracania niezapisanych zmian i fokusu w oknach.
4. `npm run check`.
5. Commit E2.

Ostatni pełny Playwright przed tymi poprawkami: 714 passed i 3 failed. Wszystkie 3 dotyczyły listy
sekcji w `settings-tab.spec.ts` i są już poprawione (11 passed).

### 2026-11-05 — E2 dokończony po przerwaniu

- Dokończone przerwane edycje: import `onBeforeUnmount` i `discard` w edytorze. Przy okazji
  poprawiony `@click="reload"`, który przekazywał zdarzenie jako `restore`.
- Ustawienia → Workflows: stały region statusu, po odblokowaniu fokus na „Otwórz Workflows”.
- Nowe testy: fokus w oknach (Nazwa, pole JSON importu, powrót fokusu po Esc), przywracanie
  niezapisanych zmian po przejściu do innego widoku, „Porzuć zmiany” bez przywracania, fokus po
  odblokowaniu. Uwaga dla testów: przy zamrożonym zegarze (`setFixedTime`) Vue pomija handler przodka
  z tym samym znacznikiem czasu, więc test Esc przesuwa zegar o sekundę.
- `npm run check`: zielone (Playwright 719 passed, Rust 801 + core 21 passed).
- Otwarte z E2: zrzuty wzorcowe nowych widoków w obu motywach. E2 niezacommitowany.

### 2026-11-05 — E3: decyzje po przeglądzie (runda 1)

- Odstępstwo od E0 (wiersz „Baza”: „identyfikatory INTEGER”): `workflow_runs.id` zostaje
  INTEGER, ale `workflow_node_executions.id` i `workflow_approvals.id` to deterministyczne
  identyfikatory TEXT nadawane przez silnik (`{run}-{n}`, `{exec}-a{n}`). Powód (7.1): id kroku
  i zgody musi być znane w zdarzeniu przed uruchomieniem procesu i nie może zależeć od PID ani
  od sesji dostawcy; odtworzenie stanu ze zdarzeń daje te same id.
- `Database` trzyma `Arc<Mutex<Connection>>`; `Database::shared()` zwraca tę samą blokadę dla
  `SqliteRunRepository`. W E5 repozytorium dostaje `Database::shared()`, nie drugie połączenie.
- Runner bez reakcji na anulowanie po czasie łaski: krok i przebieg kończą się `Interrupted`
  (wejście `Unresponsive`), nie `Cancelled` ani `Timeout`, a sterownik nie uruchamia kolejnego
  kroku, dopóki wątek runnera żyje (5.1, 8.3).
- Trwały błąd zapisu albo silnika w sterowniku kończy aktywny przebieg `Failed` (`StorageError`,
  `ProtocolError` albo `InvalidDefinition`, wejście `Fault`). Gdy i ten zapis się nie uda,
  sterownik zgłasza błąd przez `EventSink::fault` (w E5: zdarzenie na magistrali).

### 2026-11-05 — E3

Zmienione pliki (git status, bez `.claude/launch.json`):
- `src-tauri/Cargo.toml`, `src-tauri/crates/workflow-core/Cargo.toml` (feature `testing`),
  `crates/workflow-core/src/lib.rs`, `src-tauri/src/main.rs`, `src-tauri/src/workflows/mod.rs`,
  `workflows/store.rs`, `workflows/store/tests.rs`, `docs/workflows/E0_ANALIZA.md`.
- Nowe w workflow-core: `run.rs`, `engine.rs` + `engine/`, `driver.rs` + `driver/`, `output.rs`,
  `ports.rs`, `samples.rs`, `support.rs`, `testing.rs`; testy `tests/engine.rs`, `driver.rs`,
  `output.rs`, `run_states.rs`.
- Nowe w hoście: `src-tauri/src/workflows/runs.rs` + `runs/` (repozytorium SQLite przebiegów).

Decyzje:
- Silnik to czysta maszyna stanów: zdarzenie wejściowe daje następny krok, stan odtwarza się
  z zapisanych zdarzeń. Runner jest traitem, w testach deterministyczny; prawdziwe CLI dopiero E4.
- Limit rund wyczerpany kończy przebieg `Failed` z kodem `ReviewLimitExceeded`.
- Restart na granicy kroków nie dubluje uruchomienia; restart po zapisie zamiaru oznacza przebieg
  `Interrupted`.
- Bez komend IPC, sinka Tauri i podpięcia w `main.rs` (E5, E6). Decyzje po przeglądzie: wpis wyżej.

Wynik testów: `npm run check` zielone (exit 0, 205 s): build OK, Playwright 719 passed,
`cargo fmt --check` OK, `cargo test`: aplikacja 819 passed (2 ignored), workflow-core i pozostałe
zestawy bez błędów. Test sterownika: `cargo test -p workflow-core --features testing --test driver`
(samo `--test driver` bez featury jest pomijane przez `required-features`).

Pozostałe blokery i uwagi:
- Niezałatwione uwagi z review: brak.
- Nie zmierzono pokrycia (`cargo llvm-cov`); nie robiono osobnego przeglądu security.
- Przed E4 wymagana decyzja użytkownika o trybie read-only dla Claude (bloker 1 w E0_ANALIZA.md).
- Zrzuty wzorcowe nowych widoków z E2 nadal otwarte.

### 2026-11-05 — E4 (runtime Windows i adaptery)

Pliki:
- Nowy crate `src-tauri/crates/agent-cli` (`spec`, `failure`, `parse`, `lines`, `process`, `tree`,
  testowa binarka `agent-cli-fake`, testy `tests/parse.rs` i `tests/process.rs`).
- `workflow-core`: `RunRequest.role`, `output::json_schema(role)`, testy `output.rs` i `driver.rs`.
- Host: `workflows/workspace.rs`, `workflows/runners/{mod,env,prompt,claude,codex,process,guard}.rs`,
  `workflows/preflight.rs`, `store.rs` (migracja 3, `version_graph`, `VersionInfo.id`),
  `mod.rs` (`workflow_preflight`, `require_unlocked`), `main.rs`; `ai.rs` i `ai_chat.rs`
  re-eksportują przeniesione elementy.
- Frontend: `api/workflows.ts` (`NodeReadiness`), `workflows.ts`, `preview/workflows.ts`,
  `WorkflowReadiness.vue`, `WorkflowEditor.vue`, `WorkflowNodePanel.vue`, fixture IPC.
- Dokumenty: `E0_ANALIZA.md` (bloker 1 zaakceptowany), `E4_TEST_RECZNY.md`.

Decyzje:
- Crate `agent-cli`: testy jednostkowe hosta nie dostają `CARGO_BIN_EXE_*` (binarka bez biblioteki).
- Kończenie drzewa: Job Object (`KILL_ON_JOB_CLOSE`, `windows-sys =0.61.2`) plus `taskkill /T /F`;
  `KillResult::{Confirmed, Unconfirmed}`; przy `Unconfirmed` runner nie zwraca wyniku, więc
  sterownik zapisuje `Interrupted`. Okno wyścigu między spawn a przypisaniem do jobu łata drugie
  przejście `taskkill`.
- Stderr: tylko ogon 64 KiB plus licznik, w komunikacie błędu po redakcji tokenopodobnych ciągów.
  Linie stdout bajtowo z limitem 1 MiB.
- Claude z `workspaceWrite` blokowany (preflight i runner); parser TOML: zależność `toml =0.8.2`
  (wersja z Cargo.lock); macierz wersji: Claude 2.1.318, Codex 0.163.0, ta sama seria główna,
  nowsza z ostrzeżeniem; allowlista sandboxa `["elevated"]`, blokada także dla Codex read-only.
- Strażnik liczy skrót review przed migawką (`git add -N` zmienia indeks), więc migawki są
  porównywalne.
- Podgląd zawsze zwraca blokadę „Podgląd nie uruchamia agentów” (bez haków testowych);
  odpowiedź preflightu zadaje test przez zastępcze IPC (`options.preflight`).
- Wrapper `.cmd` przekazuje bez zmian argumenty ze spacjami, cudzysłowami i schemat JSON
  `--json-schema` (Rust escapuje argumenty plików wsadowych; test w `agent-cli/tests/process.rs`).

Wyniki testów: `cargo test --locked` (cały workspace) zielone: agent-cli 7 + 10, aplikacja 864
passed (2 ignored), workflow-core 20 driver, 24 engine, 12 output, 7 run_states, 21 validation,
naming 3; `cargo clippy -p agent-cli -p workflow-core` bez uwag po poprawkach; Playwright
`workflows-ui` i `workflow-definitions`: 32 passed; `npx vue-tsc --noEmit` czysto.

Pozostałe blokery i uwagi:
- Ręczny test na prawdziwych Claude i Codex (`E4_TEST_RECZNY.md`): niewykonany.
- Zachowanie Claude po `taskkill`/`TerminateJobObject` na prawdziwym CLI: niezweryfikowane.
- Konfiguracja sandboxa Codexa na tej maszynie: do wykonania przez użytkownika.
- Wpięcie Runtime w `main.rs`, komendy start/cancel/decide (E5); recovery i artefakty logów (E6).

Poprawki po review E4, runda 1 (uwagi 1–18; CRITICAL brak, wszystkie HIGH i MEDIUM poprawione):
- Schemat wyniku a próbki: tryb ścisły (każde pole w `required`, `logRef` typu string|null);
  `samples::implementation()` i fixture Codexa podają `"logRef": null`; test waliduje
  `workflow_core::samples` rekurencyjnie małym walidatorem (`tests/output.rs`).
- Kończenie drzewa: `Confirmed` tylko gdy proces się zakończył i job jest pusty
  (`ActiveProcesses == 0`) albo, bez jobu, gdy `taskkill /T` się udał; błąd `Job::attach`
  zapisany w `RunningProcess::job_error()`. Testy: brak jobu + nieudany taskkill = `Unconfirmed`,
  brak jobu + taskkill = `Confirmed` i martwy wnuk.
- Git w worktree (`WorktreeGit`): jawne `GIT_DIR`/`GIT_WORK_TREE` z rezerwacji (katalog
  `<git-common-dir>/worktrees/<runId>` ze źródła), `core.fsmonitor=false`, hooki wyłączone,
  `gc.auto=0`, diff z `--no-ext-diff --no-textconv`; przed każdą operacją kontrola, że `<wt>/.git`
  to plik wskazujący oczekiwany katalog, inaczej `PermissionViolation`. Test: podmieniony `.git`
  z `core.fsmonitor` (kontrola: zwykły Git uruchamia skrypt) nie uruchamia niczego.
  Konfiguracja systemowa i globalna Gita zostaje (`GIT_CONFIG_NOSYSTEM` nieustawione): agent
  nie ma do nich zapisu, a wyłączenie zmieniłoby np. `core.autocrlf` między poleceniami.
- Nieudane przygotowanie po `git worktree add` usuwa worktree (`worktree remove --force`, `prune`).
- Migawka kroku read-only obejmuje pliki ignorowane przez `.gitignore`, zagnieżdżone repozytoria
  (rekurencyjnie), dowiązania (tekst celu, bez podążania); treść czytana strumieniowo do 8 MiB,
  powyżej rozmiar i czas zmiany; limit 500 000 wpisów (przekroczenie to błąd kroku).
- Środowisko agenta: allowlista o proxy (`HTTP(S)_PROXY`, `ALL_PROXY`, `NO_PROXY`), CA
  (`NODE_EXTRA_CA_CERTS`, `SSL_CERT_FILE`, `SSL_CERT_DIR`) i `CLAUDE_CODE_GIT_BASH_PATH`; test
  `CliRunner` na prawdziwym środowisku procesu i `RealSpawner::options()` (`clear_env`).
- Prompt: każdy `<` w bloku danych zapisany jako ucieczka JSON, więc dane nie zamkną bloku.
- `GuardedRunner` nie deklaruje `read_only_enforced` (wykrywanie po fakcie to nie egzekwowanie).
- Testy argumentów porównują pełne listy argumentów Claude i Codexa.
- Preflight w UI: test aplikacji przez zastępcze IPC (`workflow_preflight`, `versionId`,
  licznik 1 → 2); test podglądu „Sprawdź ponownie” zmienia zapisaną wersję pod spodem.
- Wyścig `cancelling_a_queued_run_never_calls_the_runner`: początkowe `Wake` sterownika mogło
  wczytać przebieg z bazy i uruchomić go przed anulowaniem; test czeka na uspokojenie
  sterownika przed utworzeniem przebiegu (wada testu, nie sterownika).
- Wyścig `cancel_during_a_step_ends_the_run_cancelled` (odtworzony przy zimnym starcie, ok. 95 s):
  migawka po anulowanym kroku przekraczała czas łaski i dawała `Interrupted`; strażnik nie robi
  migawki po `Cancelled`. Runner po niepotwierdzonym `kill` czeka konfigurowalnie
  (`with_unconfirmed_max_wait`, w testach 10 s zamiast 600 s).

Wyniki testów po rundzie 1: agent-cli 7 + 14 (process); workflow-core 24 engine, 14 output,
7 run_states, 21 validation, 20 driver (`--features testing`); `--bin tuktuk-track workflows` 96,
`ai` 148; `naming` 3; `cargo fmt --check` czysto; `cargo clippy -p agent-cli -p workflow-core`
bez uwag; `npm run build` czysto; Playwright `workflows-ui` + `workflow-definitions` 33 passed,
`visual.spec.ts` 90 passed (bez zmian wzorców). `npm run check` nieuruchomiony w tym kroku.
Przeglądy agentami (rust-reviewer, security-reviewer, silent-failure-hunter, typescript-reviewer,
vue-reviewer) nie są zapisane w tym dzienniku; runda 1 to zewnętrzny przegląd etapu.

Poprawki po review E4, runda 2 (uwagi 1–9; HIGH 1–2 i MEDIUM 4–9 poprawione, uwaga 3 otwarta):
- Niepotwierdzony `kill` (HIGH 1): `Exited` z prawdziwego procesu znaczy tylko koniec
  bezpośredniego potomka. Runner nie kończy się już na `Exited`; ponawia `kill` (Job Object,
  `taskkill`, kontrola jobu) aż do `Confirmed` (`Cancelled`) albo do `unconfirmed_max_wait`
  (`Failed{Interrupted}`). `ScriptedProc` modeluje teraz prawdziwą semantykę (`Exited` zaraz po
  `kill`, koniec drzewa osobno). Testy: `real_process_whose_tree_end_is_unconfirmed_is_not_cancelled`
  (`CliRunner` na prawdziwym drzewie `cmd.exe` + `ping`, bez jobu, z nieudanym `taskkill`) i
  `agent-cli` `exited_after_an_unconfirmed_kill_does_not_mean_the_tree_ended`.
- Panel gotowości w aplikacji (HIGH 2): fixture IPC obsługuje `workflow_list`, `workflow_get`
  i `workflow_publish` dla jednej zadanej definicji (`workflows.detail`) oraz
  `preflightByVersion` i wstrzymanie wywołania (`__preflightHold`). Test aplikacji sprawdza w
  regionie „Gotowość agentów” teksty „kontrola CLI”, „nie izolacja systemu plików”, blokadę
  Codexa, licznik IPC 1 → 2 po „Sprawdź ponownie” i brak „Uruchom”.
- `workflow_preflight` (MEDIUM 4): config.toml Codexa czytany tylko, gdy graf ma węzeł Codexa
  (`preflight_reading_config`); błąd odczytu blokuje węzły Codexa zamiast całej komendy.
- `WorkflowReadiness.vue` (MEDIUM 5): licznik żądań zamiast wczesnego `return`; odpowiedź dla
  starszej wersji jest odrzucana. Test: publikacja w trakcie wstrzymanego sprawdzenia (test
  pada na starej wersji komponentu).
- Testy (MEDIUM 6–8): Codex review read-only bez sandboxa jest blokowany, z `elevated` ma
  `CodexSandbox`; ścieżka komendy (`preflight_graph`: `require_unlocked` + `version_graph`) na
  zablokowanej bazie; `LineTooLong` bez wyniku → `ProtocolError`; wynik z kodem 2 →
  `Completed{exit_code: 2}`, a w silniku → `protocolError`; zły JSON Codexa przez
  Runtime + `SqliteRunRepository` → `invalidOutput`.
- Środowisko (MEDIUM 9): proxy z danymi logowania w adresie nie przechodzi do procesu agenta;
  rozszerzona allowlista opisana w `E0_ANALIZA.md` i czeka na potwierdzenie użytkownika.
- `npm run check` (HIGH 3): nieuruchomiony w tym kroku (polecenie kroku poprawek wyklucza pełne
  `npm run check`); pole wyboru zostaje otwarte do kroku sprawdzenia etapu.

Wyniki testów po rundzie 2: agent-cli 7 (parse) + 15 (process); workflow-core 24 engine,
14 output, 7 run_states, 21 validation, 20 driver (`--features testing`); `--bin tuktuk-track
workflows` 106, `ai` 150; `naming` 3; `cargo fmt --check` czysto; `cargo clippy -p agent-cli
-p workflow-core` bez uwag; `npm run build` czysto; Playwright `workflows-ui` +
`workflow-definitions` 35 passed, `visual.spec.ts` 90 passed (bez zmian wzorców). Wcześniejszy
przebieg `workflows` przy obciążonej maszynie (każde wywołanie `git` ok. 5 s) dał przekroczenia
czasu Gita w testach `workspace` i integracyjnych; powtórzony bez obciążenia przeszedł w całości.

Weryfikacja końcowa E4 (`npm run check`, kod wyjścia 0): `npm run build` czysto; Playwright 725
testów zielonych (ok. 2,3 min); `cargo fmt --check` czysto; `cargo test --locked` bez porażek
(aplikacja 888 testów, 2 ignorowane; agent-cli, workflow-core i `naming` zielone). To zamyka
uwagę HIGH 3 z rundy 2 (kryterium „`npm run check` z kodem 0” ma teraz dowód).

Pozostałe blokery i uwagi po weryfikacji:
- Ręczny test na prawdziwych Claude i Codex według `E4_TEST_RECZNY.md` (z przykładowym
  repozytorium) niewykonany; sekcje przebiegu i testów negatywnych zależą od E5.
- Zachowanie Claude po `TerminateJobObject` i `taskkill` (bloker 3 z E0) oraz konfiguracja
  sandboxa Codexa (`[windows] sandbox = elevated`) niezweryfikowane na prawdziwych CLI.
- Przeglądy agentami (rust-reviewer, security-reviewer, silent-failure-hunter,
  typescript-reviewer, vue-reviewer) niewykonane; clippy tylko dla `agent-cli` i `workflow-core`.
- Test `cancelling_a_queued_run_never_calls_the_runner` raz zawiódł przy równoległym obciążeniu,
  potem trzykrotnie przeszedł; podejrzenie wyścigu sprzed E4, niezbadane.
- Test cudzysłowu w argumencie przez wrapper `.cmd` pominięty (Rust escapuje pliki wsadowe
  specjalnie); testowane są argumenty ze spacjami.
- Test UI preflightu w podglądzie używa haka localStorage
  (`tuktuk-track-preview-workflows-preflight`).
- `NEXT_SESSION.md` nie istnieje, więc brak odnośnika do `E4_TEST_RECZNY.md`.
- Wpięcie Runtime w `main.rs`, komendy start/cancel/decide i monitor (E5); recovery i artefakty
  logów na dysku (E6).
- Brak commita (robi go osobny krok workflow).

### E5 (dziennik)

Pliki: `workflow-core` (`run.rs`, `engine*`, `ports.rs`, `driver*`, `testing.rs`), host
`src-tauri/src/workflows/{artifacts,service,run_commands,runtime}.rs`, `runs/query.rs`, `store.rs`
(migracja 4), `workspace.rs`, `runners/{mod,prompt,env}.rs`, `hook_wire.rs`, `bin/hook.rs`,
`notify.rs` (`View.run`), `main.rs`; frontend `api/workflowRuns.ts`, `workflowRuns.ts`,
`preview/workflowRuns.ts`, `views/WorkflowRunsView.vue`, `components/workflows/WorkflowRun{Form,Monitor}.vue`,
fixture `tests/fixtures/tauri.ts`, `tests/workflow-runs.spec.ts`.

Decyzje:
- `Fault` działa też z `Queued` (Queued -> Failed) z nowym kodem `WorkspaceUnavailable`.
- Komentarz zgody (`comment`, serde default, limit 2000) w zdarzeniu, stanie i `workflow_approvals.comment`.
- Cel przebiegu (`goal`, migracja 4) trafia do promptu; limit 20 000 znaków.
- Artefakt diffu adresowany skrótem w katalogu danych aplikacji; klient czyta tylko przez (runId, skrót).
- `workflow_cancel` i historia działają bez klucza; start, decyzja i artefakt wymagają odblokowania.
- Powiadomienia reużywają `agent_notify_waiting` i `agent_notify_done`; klucze `wf-approval:`/`wf-end:`.
- Hook pomija zdarzenia z `TUKTUK_TRACK_WORKFLOW` (jeden tor czasu).
- Modele i uprawnienia kroków przy starcie tylko do odczytu.

Wyniki: workflow-core (driver, engine, run_states) zielone; `cargo test --bin tuktuk-track workflow` 151 OK;
Playwright `workflow-runs`, `workflows-ui`, `workflow-definitions`, `visual` zielone.

Niewykonane: karta zadania i tablica agentów (krok 12), `WorkflowGraphView` ze statusami, filtr
repozytorium z podpowiedzią z `agent_bindings`, testy Playwright startu z formularza, przeglądy
agentami, pełny `npm run check`, sekcje 4-5 E4_TEST_RECZNY na prawdziwych CLI.

#### E5: runda 1 poprawek po review

Przyczyny nieudanych testów `workflows` (to nie były losowe timeouty środowiska):
- `validate_source` wołał `git submodule status`. W Git z MSYS2/Git for Windows to skrypt
  powłoki: ok. 7 s na wywołanie bez obciążenia, a przy równoległych testach ponad 60 s
  (dowód: pomiar z dodatkowym logowaniem; `PROC-TIMEOUT submodule`). Zastąpione przez
  `git ls-files --stage -z` i szukanie trybu 160000 (test `gitlink_in_the_index_without_gitmodules_is_rejected`).
- Po limicie `run_with_limit` zabijał tylko proces git; wnuk trzymał potoki, a wątki odczytu
  wisiały. Teraz Git działa w Job Object, a po limicie i po wyjściu Gita ginie całe drzewo
  (test `timeout_kills_the_whole_process_tree`).
- Pomiar procesów na tej maszynie: 8 wywołań `git status` po kolei 1,3 s; 16 równoległych pętli
  po 8 wywołań 30 s, czyli przepustowość tworzenia procesów nie rośnie z równoległością
  (4-6 procesów/s na całą maszynę). Przebieg integracyjny to ok. 45 poleceń Git (ślad z
  logowaniem), a krok review kilkanaście, więc przy 16 wątkach testów krok przekraczał 30 s.
  Testy z Gitem (`runners`, `service`, `workspace`) biorą wspólny limit 3 naraz
  (`workflows::test_git::GitPermit`); w aplikacji sterownik i tak prowadzi jeden przebieg.
- Anulowanie w trakcie migawki strażnika nie było obsługiwane: polecenia Git kroku tylko
  do odczytu trwały dłużej niż czas łaski i przebieg kończył się `Interrupted` zamiast
  `Cancelled`. Strażnik przekazuje token anulowania do poleceń Git (zabicie drzewa,
  `WorkspaceError::Cancelled`) i nie uruchamia agenta po anulowaniu (testy
  `cancel_interrupts_a_running_command_quickly`, `guard_cancelled_during_snapshot_does_not_start_the_agent`).
  Testy anulowania czekają na start procesu kroku zamiast `sleep(200 ms)`.
- `queued_run_does_not_start_while_an_unresponsive_runner_is_alive` (sprzed E5) raz na ok. 35
  przebiegów pakietu `driver` kończył drugi przebieg `Timeout`: limit 300 ms obejmował też
  zdrowe kroki. Limit 1,5 s, krok uparty 4 s.
- Wyścig z E4 (`cancelling_a_queued_run_never_calls_the_runner`): regresja
  `cancel_of_queued_run_is_processed_before_dequeue` (50 iteracji). Sink blokujący bazę:
  `sink_that_locks_db_does_not_deadlock_driver`.

Poprawki uwag:
- Odrzucenie bez komentarza odrzuca też silnik (`approval_decision`) i sterownik
  (`decide_approval`); komentarz przycinany w core (`normalize_comment`).
- Nieaktualna rewizja: `DecideError::Stale`, w IPC z prefiksem unieważnienia; przebieg czeka dalej.
- Cel przebiegu w bloku danych promptu (pole `goal`, ucieczka `<`).
- Formularz startu: graf najnowszej opublikowanej wersji (`workflow_version_graph`), nie szkic
  (wcześniej brał też najstarszą wersję); nowy `requestId` po błędzie startu; wybór procesu,
  gdy formularz otwiera karta zadania.
- Karta zadania: „Uruchom proces…” tylko po odblokowaniu; po starcie monitor (`openRunRequest`).
- Tablica agentów: sekcja „Przebiegi procesów w toku” (lista `workflow_runs`, odświeżana
  `workflow-event`); przebieg nie tworzy `agent_sessions` ani `sessions`
  (`workflow_run_creates_no_agent_session_and_no_time_entry`).
- Monitor: graf tylko do odczytu ze statusem kroków tekstem i w opisie dostępności; nasłuch
  przed snapshotem z buforem, odpięcie przy odmontowaniu w trakcie rejestracji; `requestId`
  na zgodę; „Akceptuj” dopiero po wyświetleniu diffu, błąd diffu z ponowieniem.
- Diff czytany strumieniowo (skrót z całości, w pamięci tylko pokazywany fragment).
- `service::start`: pierwotna przyczyna zostaje, gdy zawiedzie też oznaczenie przebiegu.
- `run_commands`: `start_run` z kontrolą klucza, historia (`history_*`) bez klucza z testem.
- Fixture: `workflow_cancel` (stan końcowy, brak przebiegu), konflikt `requestId` z inną wersją,
  `workflow_decide` jak `driver/decide.rs` (requestId przy innej zgodzie, rewizja, trim).
- Powiadomienia: `try_claim_key` (błąd bazy zgłoszony), `should_show_toast` (klucz przed oknem).

Wyniki (2026-11-06): `workflow-core --features testing` zielone; `driver` 10 razy domyślnie i
15 razy `--test-threads=1` zielone po zmianie limitu (wcześniej 1 porażka na 35, opisana wyżej);
`--bin tuktuk-track workflows` dwa razy z rzędu 161/161 (306 s i 341 s); `hook_wire`, `agent_board`,
`ai`, `agent-cli`, `naming`, `cargo fmt --check` zielone; `--bin tuktuk-track workflows
-- --test-threads=1` 161/161 (424 s); Playwright `workflow-runs` (33), `workflows-ui`,
`workflow-definitions` zielone (68). `visual.spec.ts`: przy 16 workerach 30 z 90 testów
przekroczyło 30 s (same timeouty, bez różnic w zrzutach; maszyna była przeciążona: 13,6 min
zamiast zwykłej minuty); te 30 przy 2 workerach zielone, wzorce bez zmian.

Niewykonane: 10 przebiegów pakietu `workflows` (jeden trwa 5-7 min; wykonano dwa domyślnie
i jeden `--test-threads=1`), filtr repozytorium z podpowiedzią z `agent_bindings`,
przeglądy agentami, pełny `npm run check`, sekcje 4-5 E4_TEST_RECZNY na prawdziwych CLI.

#### E5: runda 2 poprawek po review

Rozstrzygnięcia:
- Wyścig z E4 (`cancelling_a_queued_run_never_calls_the_runner`) to wada testu, nie sterownika.
  Test tworzył przebieg już po starcie sterownika, a początkowe `Wake` mogło go wczytać
  z bazy i zdjąć z kolejki, zanim test wysłał anulowanie. Przebieg wzięty przez `Wake` jest
  `Running`, więc anulowanie zatrzymuje uruchomiony krok, a wywołanie runnera jest zgodne
  z 5.2 („Queued można anulować bez procesów” dotyczy przebiegu, który jeszcze czeka).
  Sterownik rozlicza wszystkie komendy z kanału przed każdym zdjęciem z kolejki (`main_loop`:
  `try_recv` przed `pop_front`). Nowy test `cancel_of_queued_run_is_processed_before_dequeue`
  jest deterministyczny: przebieg powstaje przed startem sterownika, repozytorium wstrzymuje
  początkowe `Wake` (`active_runs`), aż test wyśle anulowanie, więc przebieg jest w kolejce,
  a anulowanie w kanale. Dowód, że test łapie błąd: po zamianie kolejności w `main_loop`
  (najpierw `pop_front`, potem komendy) test pada (`runner.total_calls() == 1`). Poprzednia
  pętla 50 iteracji usunięta (powtarzała naprawiony test).
- Redakcja diffu (8.3), wariant A: plik `diff-<skrót>.patch` i skrót są surowe (zgoda dotyczy
  bajtów worktree), a tekst zwracany do widoku przechodzi przez `artifacts::redact_for_view`
  (ta sama reguła słowa co `runners::redact`, z zachowaniem układu linii i znaczników `+`/`-`).
  `DiffView.redacted` podaje liczbę ukrytych fragmentów, a monitor pokazuje ją jawnie.
  Testy: `diff_view_masks_secrets_but_file_and_hash_stay_raw`,
  `redaction_keeps_ordinary_code_and_paths`, Playwright „diff z ukrytymi sekretami…”.
- Odzyskiwanie dostępu (DoD 13 pkt 3), wariant A: bez klucza tablica agentów pokazuje aktywne
  przebiegi z przyciskiem „Zatrzymaj” (z potwierdzeniem), bez otwierania monitora. Bez
  aktywnych przebiegów sekcji nie ma, więc wygląd i zrzuty wzorcowe się nie zmieniają.
  W podglądzie przykładowy przebieg jest częścią demonstracji odblokowanego modułu, więc
  zablokowany podgląd nie ma historii. Do potwierdzenia przez użytkownika (wariant B:
  odzyskiwanie tylko w backendzie).

Poprawki:
- Diff przycięty (powyżej 2 MB) nie pozwala zaakceptować: `diffShown` wymaga
  `!truncated`, monitor pokazuje, że zmian nie można zaakceptować w aplikacji, a „Odrzuć”
  działa (Playwright „przycięty diff nie pozwala zaakceptować, tylko odrzucić”).
- Monitor odświeża szczegóły tylko po zdarzeniach zmieniających stan (bez `message`,
  `toolStarted`, `toolFinished`, chyba że trzeba dociągnąć lukę), a `refreshDetail` ma numer
  wywołania i pomija starszą odpowiedź. Test: 26 komunikatów bez `workflow_run_get`; odczyt
  wstrzymany hakiem fixture `__runGetHold` wraca po nowszym i nie cofa rewizji (decyzja
  idzie z `expectedRevision` 31).
- Tablica agentów odświeża listę tylko przy `runStateChanged` (test: 20 komunikatów bez
  `workflow_runs`). `TauriSink::emit` sprawdza typ zdarzenia (`may_notify`) przed odczytem
  ustawień i bazy.
- `run_goal` z błędem bazy kończy krok `StorageError` bez startu agenta i zgłasza błąd przez
  `on_error` (`goal_read_error_fails_the_step_without_starting_the_agent`); brak celu
  (`Ok(None)`) dalej dozwolony. `runtime::run_info` loguje błąd bazy (`try_run_info` zwraca
  go jawnie; test `run_info_reports_db_errors_and_missing_runs_separately`).
- Zapis artefaktu: plik tymczasowy z unikalną nazwą (pid i licznik), usuwany przy każdym
  błędzie (strażnik `Drop`); błąd `rename` przy istniejącym pliku docelowym (równoległy zapis
  tego samego skrótu) to sukces. Testy `artifact_write_is_atomic_no_partial_file_on_error`
  (wymuszony błąd `rename`: brak `.patch`, `.tmp` i wiersza) i
  `parallel_writes_of_the_same_hash_use_distinct_temp_files`.
- Filtr repozytorium: `runs::query::repo_key` (ścieżka kanoniczna, gdy istnieje, `/` jak `\`,
  bez końcowego separatora, małe litery ASCII) porównywany z tym samym kluczem w SQL; bez LIKE,
  więc `%` i `_` są dosłowne. Fixture i podgląd liczą klucz tak samo. Testy
  `repo_filter_matches_path_variants_and_treats_wildcards_literally`,
  `repo_key_canonicalizes_existing_paths`, Playwright „filtr repozytorium nie zależy…”.
- Migracja 4: test buduje bazę w stanie E4 (bez `goal`, bez `workflow_artifacts`, bez wpisu
  wersji 4) z zadaniem, wpisem czasu (`sessions`), przebiegiem i zdarzeniami, wykonuje
  `init_tables` dwa razy i porównuje pełną treść `tasks`, `sessions`, `workflow_runs`
  i `workflow_events`; sprawdza wpis wersji 4.
- Usunięta nieużywana `GitWorkspace::snapshot` (zastąpiona przez `snapshot_with`).

Kryterium 1 (stabilność), 2026-11-06: jedna zbudowana binarka testowa, uruchomiona
bezpośrednio, bez innych procesów testowych. Pakiet `--bin tuktuk-track workflows` (169 testów;
obejmuje `workspace`, `service`, `run_commands`, `runtime`, `artifacts`, `runs`, `store`):
10 razy z rzędu domyślnie, 169/169 za każdym razem (208-256 s), i 10 razy z rzędu
`--test-threads=1`, 169/169 za każdym razem (72-427 s). Pakiet `driver` 10 razy domyślnie
(ok. 4 s) i 10 razy `--test-threads=1` (ok. 11,5 s) zielony. Żadnej porażki do analizy.
Rozrzut czasu jednego trybu (72-427 s) pochodzi z tempa tworzenia procesów na tej maszynie
(zmierzone w rundzie 1), a nie z testów, które czekają na zdarzenia, a nie na stałe przerwy.

Zrzuty wzorcowe (`visual.spec.ts`): przy 4 workerach 10 z 90 testów padło na
`browserContext.newPage` albo `page.goto` po 30 s, a nie na różnicy zrzutu. Przyczynę
sprawdzono: to samo na czystym drzewie z commitu E4 (a193a20, osobny worktree): 8 z 8 testów
„widok zadania*” padło przy 4 workerach (`page.goto`/`locator.click` po 30 s), a te same 8 przy
jednym workerze przechodzi w 5-9 s każdy. Uruchamianie kilku Chromium naraz jest tu
nieproporcjonalnie wolne (to samo ograniczenie tworzenia procesów co przy Gicie), więc to
nie regresja E5. Cały `visual.spec.ts` przy jednym workerze: 90/90, wzorce bez zmian.

Wyniki pozostałe: `workflow-core --features testing` (w tym `driver` 28, `engine` 29,
`run_states` 8), `agent-cli`, `naming`, `hook_wire` 2, `agent_board` 12, `ai` 156,
`cargo fmt --check`, `vue-tsc --noEmit` zielone; Playwright `workflow-runs` 39,
`workflows-ui` + `workflow-definitions` 35 zielone.

Niewykonane: filtr repozytorium z podpowiedzią z `agent_bindings`, przeglądy agentami, pełny
`npm run check`, sekcje 4-5 E4_TEST_RECZNY na prawdziwych CLI; potwierdzenie wariantu A
odzyskiwania dostępu przez użytkownika.

### 2026-11-05 — E5

Zmienione pliki: patrz `git status` (workflow-core, host `src-tauri/src/workflows/`, `hook_wire.rs`,
`bin/hook.rs`, `notify.rs`, `main.rs`, frontend `src/api/workflowRuns.ts`, `src/workflowRuns.ts`,
`src/preview/workflowRuns.ts`, `views/WorkflowRunsView.vue`, `components/workflows/*`, fixture
`tests/fixtures/tauri.ts`, `tests/workflow-runs.spec.ts`). Poza zakresem etapu i poza commitem E5:
`.claude/launch.json`, `.claude/workflows/workflows-stage.js`.

Decyzje: jak w sekcji „E5 (dziennik)” i rundach 1-2 wyżej.

Wynik `npm run check`: zielony (exit 0, 323 s): `vue-tsc` i build Vite OK, Playwright 764 testów
OK (3,9 min), `cargo fmt --check` i wszystkie zestawy `cargo test` (workflow-core, agent-cli, aplikacja,
doc-tests) bez porażek. Zaznaczone są tylko pozycje z dowodem w testach Playwright
(`tests/workflow-runs.spec.ts`).

Pozostałe blokery:
- Pozycja „formularz startu z karty zadania i listy definicji” niezaznaczona: kryterium jednego
  przebiegu przy podwójnym kliknięciu ma test tylko dla listy definicji; test karty zadania klika raz.
- Review (HIGH): brak testu podwójnego kliknięcia dla startu z karty zadania (test klika raz);
  ścieżka listy definicji ma test (`tests/workflow-runs.spec.ts`, „podwójne kliknięcie Uruchom...”).
  Oba wejścia używają tego samego `WorkflowRunForm`, ryzyko praktyczne niskie.
- Niezrealizowane kroki planu poprawek: kroki 2-10 (m.in. testy kontroli dostępu IPC, powiadomień
  i hooka `TUKTUK_TRACK_WORKFLOW`, testy artefaktu diffu), start z karty zadania, sekcja aktywnych
  przebiegów na tablicy agentów, graf ze statusami w monitorze; `WorkflowRunForm` nadal pokazuje
  kroki ze szkicu zamiast z opublikowanej wersji (krok 0b).
- Testy regresji z planu kroku 1 bez potwierdzenia: `sink_that_locks_db_does_not_deadlock_driver`.
- Pakiet `workflows` przy domyślnej równoległości nie przechodzi stabilnie przy obecnym narzucie
  startu procesów (0,4-2 s na git): kryterium 10 razy z rzędu niespełnione w tym przebiegu.
  Potrzebne ograniczenie liczby spawnów git w snapshotach albo budżet kroku od startu agenta.
- Brak potwierdzenia RED testu wnuka trzymającego potoki na starym kodzie; niezbadany wyścig `Wake`
  w `cancel_of_queued_run_is_processed_before_dequeue`; `concurrent_start_same_request_id_creates_one_run`
  i `failed_fail_queued_run_is_reported_not_swallowed` bez potwierdzenia.
- Uzupełnienie `E4_TEST_RECZNY.md` i sekcje 4-5 na prawdziwych CLI (robi użytkownik).

### 2026-11-06 — E6 (poprawki po review, runda 1)

Zakres: rejestr procesów (`workflows/processes.rs`, `CliRunner::with_ledger`), rekonsyliacja przed
`Runtime::start` i `workflow_recover` (`workflows/recovery.rs`, `RunDetail.recovery`, zwolnienie
rezerwacji w transakcji zdarzenia `recovery`), tożsamość procesu z trzymanym uchwytem
(`agent-cli/src/identity.rs`: `Confirmed` tylko po udanym `taskkill /T` i potwierdzonym końcu
procesu głównego), `ShutdownReport.timed_out`, kontrolowane zatrzymanie przy wyjściu, `--prepare-update`
i `prepare_update` (`workflows::runtime::shutdown`, `EXIT_GRACE`), blokada aktualizacji i restore przy
przebiegu `queued`/`running`/`cancelling`, neutralizacja przywróconej kopii (`Input::Abandon`, rezerwacje
`restored`), odrzucenie nowszego schematu, diffy w kopii z weryfikacją (`workflows/backup.rs`),
retencja (`workflows/retention.rs`, `workflowRetentionDays`, `ArtifactError::Purged`), kontrola
miejsca (`workflows/disk.rs`, `ArtifactError::DiskFull`), TS: `workflow_recover`, monitor
(„Odzyskaj przebieg”), fixture IPC, `tests/workflow-recovery.spec.ts`. Checklista ręczna:
`E6_TEST_RECZNY.md`; A03 w `E0_ANALIZA.md` zaktualizowane.

Testy ukierunkowane (zielone): `cargo test --bin tuktuk-track tests::` 1009, `workflows::recovery` 15,
`backup` 26, workflow-core (driver, engine, run_states), agent-cli 22, `naming`, `cargo fmt --check`,
`cargo clippy -p agent-cli -p workflow-core` bez uwag, `vue-tsc --noEmit`; Playwright:
`workflow-recovery` 9, `workflow-runs`/`workflows-ui`/`workflow-definitions`/`backup`/`updater`/
`settings-tab` 92, `visual` 90. Pełne `npm run check` wykonuje krok sprawdzenia etapu.

### 2026-11-06 — E6 (poprawki po review, runda 2)

Poprawki:
- Przywracanie kopii odrzuca każdy nieskończony przebieg, także `waitingForApproval`
  (`workflows::backup::restore_blocker`, użyte w `restore_locked`); aktualizację nadal blokują tylko
  `queued`/`running`/`cancelling` (`active_work`). Odwołuje to decyzję z rundy 1 („przebieg czekający
  na zgodę nie blokuje”), która była sprzeczna z kryterium. Podgląd kopii (`inspect_backup`,
  `Preview.workflowBlocker`) ostrzega o tym z góry. Fixture: osobna `workflowRestoreBlocker`.
- Diffy z kopii: `prepare_restore` tylko weryfikuje wpisy (`take_artifacts`) i usuwa tabelę;
  pliki zapisuje `PreparedRestore::write_artifacts` po udanym `restore_locked`, bez blokady bazy.
  Odrzucone przywrócenie nie zostawia plików z kopii.
- Eksport zwraca `ExportResult { path, skippedArtifacts, overLimitArtifacts }`; `LibraryView.vue`
  podaje liczby pominiętych diffów.
- `workflow_recover`: PID procesów bez potwierdzonego końca w notatce zdarzenia
  (`recovery::unconfirmed_note`), w `RecoverOutcome.unconfirmedPids` i `RecoveryView.unconfirmedPids`;
  monitor pokazuje komunikat z PID jako `role=alert`. Przycisk „Odzyskaj przebieg” otwiera
  `ConfirmDialog` („Sprawdź procesy i zwolnij katalog”). Podgląd ma przykładowy przerwany przebieg (id 2).
- `prepare_update` nie zatrzymuje już sterownika (nieudana instalacja zostawiała martwy sterownik);
  zatrzymanie robią tylko `RunEvent::Exit` i `--prepare-update`. Dodatkowo `RuntimeHandle::is_stopped`
  i `runtime_handle` odrzuca nowe przebiegi przy zatrzymanym sterowniku (`ERR_RUNTIME_STOPPED`).
- Start aplikacji: `recovery::remove_orphan_tmp` usuwa `diff-*.tmp` w `workflows/runs/*`;
  `recovery::startup_faults` zgłasza procesy bez potwierdzonego końca (z PID), przebiegi zakończone
  `workspaceUnavailable` i błędy przez `FAULT_EVENT`. Frontend nie słucha jeszcze `workflow-fault`
  (zgłoszenie trafia też do logu).
- `workflow_run_get` sprawdza pliki diffów po odczycie z bazy (`artifacts::with_file_availability`);
  monitor mówi o brakującym pliku (`artifact-unavailable`).
- Retencja diffów: zakres 7–365 dni (Rust, TS, pole w Ustawieniach).

Nowe albo zmienione testy: `restore_is_refused_while_any_run_is_unfinished`,
`restored_active_runs_become_interrupted_and_runtime_never_calls_runner` (zgoda → `invalidated`,
krok w toku → `interrupted`, `runner.total_calls() == 0`), `restore_writes_verified_artifacts_only_after_replace`,
`refused_restore_writes_no_artifact_files`, `recover_keeps_reservation_when_process_still_alive` (PID 4242),
`orphan_tmp_artifact_is_removed_and_final_files_kept`, `startup_faults_cover_failed_runs_and_errors_but_not_routine_cleanup`,
`missing_artifact_file_shows_artifact_unavailable_in_detail`, `unconfirmed_kill_marks_unknown`,
`spawn_is_recorded_before_any_output_is_read`, `ledger_rows_survive_restart_of_repository`,
`stopped_runtime_rejects_new_work_with_message`, driver `graceful_shutdown_when_idle_returns_immediately`
(`is_stopped`), test źródła `prepare_update` (bez `shutdown`), retencja 1 i 6 odrzucone;
Playwright: potwierdzenie odzyskania, podwójne kliknięcie (1 wywołanie), Esc z przesuniętym zegarem,
alert z PID, podgląd odzyskania, przebieg zakończony bez sekcji, restore przy `waitingForApproval`,
brak pliku diffu, eksport z pominiętymi diffami, ostrzeżenie w podglądzie kopii.

Wyniki (2026-11-06): `cargo test --manifest-path src-tauri/Cargo.toml --locked` (cały workspace)
exit 0: aplikacja 1017 (2 ignorowane), agent-cli 7 + 22, workflow-core driver 38, engine 38,
run_states 8 i pozostałe zestawy bez porażek. `-p workflow-core --features testing --test driver`
10 razy z rzędu: 10 × 38/38 (ok. 5 s każde). `cargo fmt --check` zielone, `cargo clippy -p agent-cli
-p workflow-core` bez uwag, `naming` 3, `workflows` 227, `backup` 27, `vue-tsc --noEmit` zielone.
Playwright: `workflow-recovery` + `backup` 21/21, `workflow-runs`/`workflows-ui`/`workflow-definitions`/
`updater`/`settings-tab` 90/90, `visual` (1 worker) 90/90.

Niewykonane: pełne `npm run check` (zgodnie z poleceniem rundy wykonuje je krok sprawdzenia etapu;
wynik do dopisania tam), test ręczny `E6_TEST_RECZNY.md` (użytkownik).

### 2026-11-06 — E6

Wynik etapu po dwóch rundach poprawek po review (szczegóły wyżej). Pozostałe uwagi review: brak.

Zmienione pliki (git status, bez commitu): crate `agent-cli` (`identity.rs`, `fs_space.rs` nowe; `lib.rs`,
`process.rs`, `tree.rs`, `Cargo.toml`, test `process.rs`), `workflow-core` (`driver.rs`, `engine*.rs`, `lib.rs`,
`run.rs`, testy driver/engine/run_states), host (`workflows/backup.rs`, `disk.rs`, `processes.rs`, `recovery.rs`,
`retention.rs`, `test_db.rs` nowe; `artifacts`, `mod`, `run_commands`, `runners`, `runs`, `runtime`, `service`,
`store`, `workspace` zmienione; `backup.rs`, `main.rs`, `settings.rs`), frontend (`LibraryView.vue`,
`SettingsPanel.vue`, `settings/WorkflowsSection.vue`, `workflows/WorkflowRunMonitor.vue`, `api/workflowRuns.ts`,
`preview/workflowRuns.ts`, `settings.ts`, `workflowRuns.ts`), testy (`tests/workflow-recovery.spec.ts` nowy,
`tests/backup.spec.ts`, `tests/fixtures/tauri.ts`), dokumenty (`E6_TEST_RECZNY.md` nowy, `E0_ANALIZA.md`,
ten plik) oraz `.claude/launch.json` i `.claude/workflows/workflows-stage.js`.

Decyzje: rejestr procesów zapisywany przez runner przed odczytem strumieni; rekonsyliacja przed startem
runtime, bez wznawiania starych poleceń; przywrócona kopia neutralizuje przebiegi (zgody `invalidated`, kroki
`interrupted`); restore odrzuca każdy nieskończony przebieg, aktualizację blokują `queued`/`running`/`cancelling`;
artefakty diffów w kopii jako BLOB z weryfikacją, retencja `workflowRetentionDays` (7–365), brak miejsca
(kody 112 i 39, `MIN_FREE_BYTES`) zgłaszany jako `DiskFull`; kontrolowane zatrzymanie przy wyjściu i
`--prepare-update`.

Wynik `npm run check`: zielone (370 s, exit 0): build OK, Playwright 783 passed, `cargo fmt --check` OK,
`cargo test --locked` bez porażek (aplikacja 1017 passed / 2 ignored, ok. 156 w pozostałych zestawach).

Checkboxy E6 w rozdz. 10: zaznaczone cztery z dowodem w testach (rekonsyliacja, przerwanie w trakcie
zapisu/oczekiwania/anulowania, backup/retencja/brak miejsca, blokada restore i aktualizacji). Niezaznaczone:
zamknięcie okna do traya versus faktyczne wyjście (wymaga ręcznego sprawdzenia).

Pozostałe blokery (poza zakresem automatycznych testów): ręczny test `E6_TEST_RECZNY.md` na prawdziwych
Claude i Codex, zamknięcie do traya a „Zakończ”, aktualizacja przez instalator, zabicie procesu w Menedżerze
zadań; decyzje użytkownika (Running na granicy kroków, WaitingForApproval a restore, okno potwierdzenia przy
„Zakończ”). Frontend nie słucha jeszcze zdarzenia `workflow-fault`.

### 2026-11-06 — E7

Zakres: regresja i przygotowanie wydania (dokumentacja, wyłączenie modułu, fixture, checklista ręczna). Wersji w
plikach nie podniesiono (wydanie 2.5.0 wymaga potwierdzenia użytkownika).

Zmienione pliki (git status, bez commitu): host (`src-tauri/src/main.rs`, `workflows/mod.rs`, `run_commands.rs`,
`runtime.rs`, `store.rs` oraz ich testy, `backup/tests.rs`; nowy `src-tauri/tests/workflow_commands.rs`), frontend
(`src/workflows.ts`, `workflowRuns.ts`, `workflowAccess.ts`, `api/workflowRuns.ts`, `preview/workflows.ts`,
`preview/workflowRuns.ts`, `views/AgentsView.vue`, `views/WorkflowRunsView.vue`, `components/settings/WorkflowsSection.vue`,
nowy `components/workflows/WorkflowFaults.vue`), testy (`tests/fixtures/tauri.ts`, `workflow-recovery.spec.ts`,
`workflow-runs.spec.ts`, `workflows-ui.spec.ts`), dokumenty (`README.md`, `RELEASE.md`, `notes.md`,
`E4_TEST_RECZNY.md`, `E6_TEST_RECZNY.md`, nowe `E7_TEST_RECZNY.md` i `INSTRUKCJA.md`) oraz `.claude/launch.json`.

Decyzje: odmowa `workflow_lock` przy aktywnych przebiegach zamiast ich anulowania (do potwierdzenia przez
użytkownika); klucz dostępu nie występuje w żadnym pliku (sprawdzone grepem, w kodzie tylko SHA-256); logika
zmieniona w Rust jest odzwierciedlona w fixture `tests/fixtures/tauri.ts`.

Wynik `npm run check`: zielone (exit 0, 290 s): build i vue-tsc OK, Playwright 796 passed (3,6 min),
`cargo fmt --check` OK, `cargo test --locked` OK (20 zestawów, 0 porażek).

Checkboxy E7 w rozdz. 10: zaznaczony jeden z dowodem w testach (funkcja eksperymentalna odblokowywana kluczem,
testy `workflows-ui.spec.ts`: menu bez klucza, błędny i poprawny klucz, wyłączenie zachowuje definicje).
Niezaznaczone: przegląd (nie wykonany, patrz niżej), pełne CI wraz z testami natywnymi (natywnych brak),
dokumentacja i release notes (napisane, bez dowodu w testach), pilotaż, przygotowanie wydania.

Znane ograniczenia: Claude read-only to kontrola CLI, nie izolacja; jeden aktywny agent; brak merge.

Pozostałe blokery: przegląd całego modułu nie został wykonany (limit 3 poprawek i `ReviewLimitExceeded`, spójność
stanów, redakcja diffu i stderr, allowlista środowiska, reduced motion; bez agentów rust-reviewer,
security-reviewer, vue-reviewer; sprawdzono tylko clippy i grep klucza). Testy natywne na Windows (instalator,
aktualizacja z 2.4.0, tray a „Zakończ”, zabicie procesu), testy na prawdziwych Claude i Codex oraz pilotaż:
checklista w `E7_TEST_RECZNY.md`, wykonuje ją użytkownik. Do potwierdzenia: numer 2.5.0. Poza zakresem E7: nowe
zrzuty wzorcowe widoków Workflows, filtr repozytorium z `agent_bindings`, pomiar `cargo llvm-cov`.
