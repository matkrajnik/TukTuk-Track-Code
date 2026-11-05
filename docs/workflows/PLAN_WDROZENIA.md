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

- [ ] Dodać pozycję nawigacji, listę i edytor z Vue Flow po pozytywnej decyzji E0.
- [ ] Uporządkować menu w grupy i odsłaniać Workflows jako rozszerzenie obecnej aplikacji, bez przełącznika trybów.
- [ ] Dodać ustawienia aktywacji kluczem, trwały stan uprawnienia i kontrolę dostępu w backendzie wszystkich komend modułu.
- [ ] Zaimplementować animację odblokowania, pomijanie przez Escape, wariant reduced motion i zachowanie na małym ekranie.
- [ ] Zaimplementować pięć typów węzłów, panel parametrów i edycję krawędzi.
- [ ] Dodać zapis szkicu, publikację, informację o niezapisanych zmianach i konflikcie rewizji.
- [ ] Dodać szablon procesu oraz dostępny klawiaturą wariant listy kroków.
- [ ] Uzupełnić preview, Playwright i zrzuty obu motywów.

Odbiór: użytkownik tworzy i ponownie otwiera poprawny graf; błąd wskazuje konkretny węzeł. Przyciski uruchamiania nie udają działającego runtime przed jego wdrożeniem.

### E3 — silnik na procesach zastępczych

- [ ] Zaimplementować kolejkę, przejścia i trwałe NodeExecution.
- [ ] Dodać typowane wyniki, warunki, trzy rundy poprawek i terminalne ścieżki.
- [ ] Dodać trwałe Approval oraz idempotentny start i decyzje.
- [ ] Dodać timeouty, anulowanie, błędy i strumień zdarzeń z sequence.
- [ ] Uruchomić scenariusz end-to-end na deterministycznych runnerach testowych.

Odbiór: ten sam zapisany zestaw wyników prowadzi do tej samej ścieżki grafu; żaden test nie potrzebuje konta AI. Restart na granicy kroków nie dubluje kolejnego uruchomienia.

### E4 — runtime Windows i adaptery

- [ ] Wydzielić współdzielone elementy wykrywania/parsing bez regresji asystenta.
- [ ] Dodać preflight, profil modelu i kontrolowane środowisko dla każdego adaptera.
- [ ] Dodać workspace/worktree i rezerwacje.
- [ ] Zaimplementować streaming, wynik, identyfikator sesji i nadzór drzewa procesów.
- [ ] Egzekwować profile dostępu i blokowanie nieobsługiwanych konfiguracji.
- [ ] Przygotować checklistę testu ręcznego dla użytkownika na prawdziwych CLI.

Odbiór: procesy zastępcze potwierdzają timeout, kill i ograniczenia; użytkownik potwierdza rzeczywisty pipeline na testowym repozytorium i wspieranych wersjach obu CLI. Brak takiej walidacji pozostaje jawnie oznaczony.

### E5 — monitor, zadania i zgody

- [ ] Dodać formularz startu z karty zadania i listy definicji.
- [ ] Dodać monitor grafu, historię, filtrowanie, logi i artefakty.
- [ ] Dodać diff, wyniki review oraz decyzję związaną z hashem artefaktu.
- [ ] Połączyć przebiegi z tablicą agentów, czasem i powiadomieniami.
- [ ] Dodać odtwarzanie zdarzeń po ponownym otwarciu widoku i ograniczenie renderowania dużych logów.

Odbiór: pełny scenariusz działa z UI; nie podwaja czasu i nie zmienia automatycznie timera ani zgłoszeń. Podwójne kliknięcie zgody ma jeden skutek.

### E6 — recovery, backup i odporność

- [ ] Dodać rekonsyliację procesów i przebiegów po starcie aplikacji.
- [ ] Obsłużyć przerwanie w trakcie zapisu wyniku, oczekiwania i anulowania.
- [ ] Dodać backup/restore artefaktów, retencję i obsługę braku miejsca.
- [ ] Zablokować restore i aktualizację podczas aktywnych procesów albo przeprowadzić kontrolowane zakończenie przed operacją.
- [ ] Zweryfikować zamknięcie okna do traya versus faktyczne wyjście z aplikacji.

Odbiór: restart nie gubi historii i zgód, nie uruchamia ponownie zmian w ciemno, nie pozostawia niekontrolowanego writera. Restore nie wznawia starych poleceń.

### E7 — regresja i wydanie

- [ ] Przeprowadzić przegląd domeny, bezpieczeństwa, UI i migracji.
- [ ] Wykonać pełne CI oraz testy natywne na Windows.
- [ ] Uzupełnić README, instrukcję użytkownika, troubleshooting i release notes.
- [ ] Udostępnić moduł początkowo jako funkcję eksperymentalną odblokowywaną kluczem w ustawieniach, zachowując pełne śledzenie bez klucza.
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
