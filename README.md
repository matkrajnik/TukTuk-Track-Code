# Todo Track

Lokalna aplikacja desktopowa: lista zadań i prosty tracker czasu. Tauri 2 + Rust + Vue 3 / TypeScript. Interfejs po polsku, dane w SQLite, bez konta i serwera.

## Funkcje

### Nowości 1.7.2

Przebudowa interfejsu oparta na standardach (heurystyki Nielsena, WCAG 2.2 AA, Fluent 2 i wytyczne Windows) i wzorcach z Toggl Track, Clockify, Sunsama i Jiry. Każda zmiana ze źródłem jest opisana w `UX_AUDIT.md`. Funkcje i reguły się nie zmieniły: timer przełącza się tylko po kliknięciu, a zapisy do Jiry mają wyłącznik.

- **Lewy panel nawigacji** zamiast paska u góry (Windows NavigationView):
  - od 1008 px panel jest rozwinięty;
  - przy 641–1007 px ma same ikony, a nazwy są w podpowiedziach;
  - poniżej 641 px chowa się pod przyciskiem menu i otwiera nad treścią;
  - przycisk menu zwija i rozwija panel ręcznie, a wybór zostaje zapamiętany;
  - Ustawienia są na dole panelu;
  - nazwy pozycji się nie zmieniły.
- **Mierzone zadanie na każdym widoku**: w pasku nad treścią są nazwa, czas i Stop. Kliknięcie nazwy wraca do Zadań.
- **Banery w stylu Windows InfoBar**: bezczynność, agenci AI, skupienie i Jira mają ikonę i rodzaj komunikatu, a główną odpowiedź jako wyróżniony przycisk (np. „Przełącz”, „Przesuń”).
- **Czytelniejsze przyciski**: akcje zapisujące to przyciski, a nie linki, np. „Oznacz dzień jako sprawdzony”, „Zapisz wnioski”, „Zapisz notatki dnia”, „Dodaj do planu”. Start w kartach nie wygląda już na wyłączony.
- **Dostępność**:
  - kontrast tekstu co najmniej 4.5:1, ramki pól 3:1;
  - obszary klikania co najmniej 24 px;
  - widoczny fokus także w polach daty i wyszukiwaniu ustawień;
  - link „Przejdź do treści” jako pierwszy przystanek klawisza Tab;
  - nawigacja oznacza bieżący widok przez `aria-current`;
  - tablica tygodnia ogłasza czytnikowi ekranu przeniesienie karty.
- **Wygląd Windows 11**:
  - typografia według rampy Fluent (12/14/16/24/28 px, Segoe UI Variable);
  - zaokrąglenia 4/8 px;
  - pasek tytułu 32 px przyklejony u góry okna.
- **Drobniejsze**:
  - Rano → Dziś pisze, o ile plan przekracza czas na pracę;
  - opisy w Ustawieniach stoją bliżej swoich opcji;
  - nagłówki kroków w Rano i Zamknięciu dnia są mniejsze niż tytuł karty.

### Nowości 1.7.1

- **Przypięte zadania („w toku”)**: przy pracy nad kilkoma zadaniami na zmianę (np. dwa zgłoszenia z Jiry) w rozwiniętym kafelku jest „Przypnij do widgetu”. Można przypiąć najwyżej 3 otwarte zadania (pole `pinnedAt` w `task_details`, bez nowej tabeli). Mini-widget pokazuje je drugim rzędem przycisków (okno rośnie z 320×64 do 320×96), a menu ikony w trayu jako pozycje pod „Mierzone: …”. Kliknięcie zatrzymuje bieżący pomiar i startuje wybrane zadanie, więc czas nadal liczy się tylko raz, a oś dnia pokazuje kolejne odcinki. Zadanie z Jiry pokazuje klucz, mierzone jest wyróżnione (w trayu „▶”). Ukończone albo zarchiwizowane zadanie znika z listy i zwalnia miejsce; kopia zadania cyklicznego nie dziedziczy przypięcia.

### Nowości 1.7

Wydanie 1.7.0 obejmuje zadania K1, K2, S1, P1–P3, W1–W4, J1 i J2 z `PLAN_1.7.md`. Wszystko poniżej jest sprawdzone testami Rust i Playwright w trybie podglądu; nic nie było uruchamiane w natywnej aplikacji ani po instalacji NSIS, a połączenia z Microsoft Graph i prawdziwą Jirą nie były wykonywane (lista do ręcznego sprawdzenia: `NEXT_SESSION.md`). Timer nigdy nie przełącza się sam: kalendarz, agenci, okna, przeglądarka i Jira tylko proponują.

- **Kalendarz Microsoft 365** (K1): Ustawienia → Kalendarz → „Połącz konto Microsoft” otwiera logowanie w przeglądarce systemowej (OAuth z PKCE, bez sekretu, przekierowanie na `http://localhost:<losowy port>`, limit 5 minut, „Anuluj” przerywa). Refresh token jest w magazynie poświadczeń Windows, a w SQLite tylko spotkania (nowa tabela `calendar_events`). Spotkania od poniedziałku poprzedniego tygodnia do 3 tygodni naprzód pobierają się przy starcie, co 10 minut i po „Odśwież”; usunięte w Outlooku znikają. „Pokazuj tytuły spotkań” wyłączone daje „Spotkanie” zamiast tytułu. „Rozłącz” usuwa token i spotkania. Kopia SQLite obejmuje spotkania, ale nie token. Bez client ID funkcja jest wyłączona. Rejestracja aplikacji: niżej, „Rejestracja aplikacji w Entra ID”.
- **Spotkania w planie i pojemności** (K2): pojemność dnia to cel dnia minus nienakładające się spotkania zajmujące czas (zajęty, poza biurem, praca gdzie indziej; wstępnie przyjęte z osobnym wyglądem; bez anulowanych i odrzuconych), nie mniej niż 0. Całodniowe „poza biurem” zwalnia dzień; inne wydarzenia całodniowe nie zmieniają pojemności. Liczą ją tray, powiadomienie o celu, Dashboard, Zadania, Zamknięcie dnia i tablica tygodnia (pasek z częścią spotkań i podpowiedzią „Cel · spotkania · zostaje”). Oś dnia ma tor „Spotkania”, a bloki planu omijają spotkania. Zadania → „Dzisiaj” pokazują nadchodzące spotkania. Zamknięcie dnia ma sekcję „Spotkania” z „Dodaj jako czas” (otwiera ręczne dodanie czasu z godzinami spotkania; przy nakładaniu z pomiarem ostrzeżenie zamiast przycisku).
- **Zakładka Ustawienia** (S1): osobna pozycja nawigacji i skrót Ctrl+, (zob. „Ustawienia” niżej). Linki z widoków, komunikaty błędów i powiadomienia otwierają właściwą sekcję. Z „Historia i kopie” zniknęła podzakładka Ustawienia; kopie zostały na miejscu.
- **Szacunki z poprawką i „Rozłóż”** (P1): mnożnik z mediany rzeczywistość/plan z 90 dni (projekt albo wszystkie zadania, co najmniej 5 ukończonych z szacunkiem, obcięty do 0,5–3) daje podpowiedź „zwykle ~…” przy szacunku. Ustawienie „Planuj z poprawką szacunków” (domyślnie wyłączone) używa go w pojemności, osi planu i „Rozłóż”. Przycisk „Rozłóż” na tablicy tygodnia układa zadania z „Do zaplanowania” i „Zaległe” na dni robocze do końca widocznego tygodnia (termin, priorytet, ręczna kolejność; bez szacunku 30 min; bez zablokowanych i cyklicznych) i pokazuje podgląd z listą „Nie zmieściło się”. Nic nie zapisuje się bez „Zastosuj” (jedna transakcja).
- **Poranny przegląd** (P2): Plan i historia → Rano (i przycisk „Przegląd dnia” na Dashboardzie): niedokończone z poprzedniego dnia roboczego, terminy, nowe zgłoszenia z Jiry, dzisiejsze spotkania, plan i pasek pojemności. „Zaczynam dzień” zapisuje przegląd. Powiadomienie „Przegląd dnia” przychodzi raz dziennie w dzień roboczy po początku dnia, gdy przeglądu nie było (nie w święto ani w całodniowy urlop z kalendarza), czeka w kolejce trybu skupienia; wyłącznik w Ustawieniach → Powiadomienia.
- **Cykle miesięczne i „Czekam na”** (P3): poza dniami tygodnia zadania cykliczne mogą powtarzać się co N tygodni, miesięcznie w dniu X (w krótszym miesiącu ostatniego), w ostatni dzień roboczy miesiąca albo w n-ty/ostatni dzień tygodnia miesiąca; stare cykle tygodniowe działają bez zmian. Zablokowane zadania są w grupie „Czekam na…” z „od N dni” i „Odblokuj”. Po N dniach roboczych blokady (domyślnie 3, 0 wyłącza) przychodzi powiadomienie z „Odblokuj” i „Przypomnij jutro”. Statusy Jiry z listy w ustawieniach dają na kafelku tylko propozycję oznaczenia albo odblokowania.
- **Czas agentów jako osobny tor** (W1): okresy pracy Claude Code i Codex trafiają do nowej tabeli `agent_sessions` (przerwy poniżej 2 min się łączą, wpisy starsze niż 400 dni są usuwane przy starcie). Oś dnia ma tor „Agenci”, a rozwinięty kafelek i zakładka Miesiąc kolumny „Agent” i „Równolegle”. Czas agentów nie liczy się do celu, raportu TS ani eksportu.
- **Rozproszenie i przegląd tygodnia** (W2): karta „Rozproszenie” na Dashboardzie (przełączenia w ciągu 15 min, mediana i najdłuższy nieprzerwany pomiar, bloki skupienia) i Plan i historia → Tydzień → Przegląd (czas kontra cel i spotkania, plan kontra wykonanie, rozjazdy szacunków, czas agentów, „Wnioski” w nowej tabeli `week_notes`). Powiadomienie „Przegląd tygodnia” raz w tygodniu w ostatni dzień roboczy o ustawionej godzinie (domyślnie 15:00).
- **Podpowiedzi z aktywnego okna** (W3, domyślnie wyłączone): co 5 s tytuł i proces okna na pierwszym planie, tylko w pamięci. Klucz Jiry z tytułu (przeglądarka, Teams, Outlook) albo folder edytora (VS Code, Cursor, JetBrains) przez gałąź git daje propozycję „Pracujesz teraz przy innym zadaniu?” ze źródłem „Okno”, z tą samą regułą 2 minut, „Nie teraz” i wyciszeniem folderów co propozycje agentów.
- **Rozszerzenie przeglądarki Chrome i Edge** (W4): rozszerzenie z `browser-extension/` i program `todo-track-bridge.exe` (native messaging, nazwany potok) podpowiadają zgłoszenie otwarte w aktywnej karcie, także z `?selectedIssue=KEY`, z pierwszeństwem przed tytułem okna. Instalacja: niżej, „Instalacja rozszerzenia przeglądarki”.
- **Przesunięcie zgłoszenia przy starcie pracy** (J1): start pomiaru zadania ze zgłoszeniem w „Do zrobienia” (każdą drogą) pyta „Przesunąć KEY do „In Progress”?” w banerze, widgecie albo powiadomieniu, z „Przesuń”, „Nie teraz” i „Nie pytaj dla tego zadania”. Pomiar startuje od razu. Kafelek ma „Przesuń w Jirze…”, a ukończenie zadania proponuje przesunięcie dalej. Ustawienia → Jira: główny wyłącznik zapisów do Jiry, pytanie przy starcie i nazwa statusu.
- **Jira z poziomu Todo Track** (J2): widok Jira ma przełącznik Lista/Tablica. Tablica ma kolumny z konfiguracji tablicy (pamięć podręczna w nowych tabelach `jira_board_columns`, `jira_statuses` i `jira_board_meta`, odświeżana raz na dobę i przy „Odśwież”), zakres „Moje” albo „Cała tablica” (limit 300 z komunikatem) i przesuwanie kart przeciągnięciem albo „Przesuń do…” z cofnięciem przy błędzie; przejście z wymaganymi polami otwiera zgłoszenie w przeglądarce. Akcje: „Przypisz do mnie”, „Zmień osobę…”, „Dodaj komentarz”, „Kopiuj nazwę gałęzi” (prefiks w ustawieniach) i „Nowe zgłoszenie” z opcjonalnym zadaniem w Todo Track.
- Nowe tabele (`calendar_events`, `agent_sessions`, `week_notes`, `jira_board_columns`, `jira_statuses`, `jira_board_meta`) powstają przy starcie przez `CREATE TABLE IF NOT EXISTS`; nowe ustawienia dostają wartości domyślne, a nowe pola zadań (reguła cyklu, „Czekam na”, „Nie pytaj”) mają wartości domyślne w `task_details`.

#### Rejestracja aplikacji w Entra ID (kalendarz)

Robi ją użytkownik albo administrator firmy, raz. Aplikacja jest klientem publicznym, więc nie potrzebuje sekretu.

1. [Microsoft Entra admin center](https://entra.microsoft.com) → **Applications → App registrations → New registration**. Nazwa dowolna, np. „Todo Track”. Typ kont: zwykle „Accounts in this organizational directory only”.
2. **Authentication → Add a platform → Mobile and desktop applications**, własny adres przekierowania `http://localhost` (bez portu; aplikacja używa losowego portu na `localhost`).
3. **API permissions → Add a permission → Microsoft Graph → Delegated permissions**: `User.Read`, `Calendars.Read`, `offline_access`. Jeśli firma wymaga zgody administratora, administrator wybiera „Grant admin consent”.
4. Z **Overview** skopiuj „Application (client) ID” do Ustawienia → Kalendarz → „Identyfikator aplikacji (client ID)”.
5. Pole „Dzierżawca”: domyślne `organizations` działa dla rejestracji wielodzierżawczej. Przy rejestracji tylko dla własnej organizacji wpisz „Directory (tenant) ID” z Overview albo domenę firmy (np. `firma.pl`).
6. „Połącz konto Microsoft” i zaloguj się w przeglądarce. Błąd rejestracji wraca jako komunikat z kodem `AADSTS…`.

#### Instalacja rozszerzenia przeglądarki

1. W aplikacji: Ustawienia → Czas pracy → **„Rozszerzenie przeglądarki (Chrome i Edge)”** — włącz. Aplikacja zapisuje manifest hosta i wpisy w `HKCU` dla Chrome i Edge (bez uprawnień administratora); wyłączenie je usuwa.
2. W przeglądarce: `chrome://extensions` albo `edge://extensions` → „Tryb dewelopera” → „Załaduj rozpakowane” → folder `browser-extension` w folderze instalacji (przycisk „Otwórz folder rozszerzenia” w Ustawieniach). Identyfikator rozszerzenia jest stały: `cecpcpkmmmkgfdklnkkkigofkcibnhbl`; w firmie można je wdrożyć zasadami grupy (`ExtensionInstallForcelist`).
3. Opcje rozszerzenia pokazują stan połączenia i pozwalają dopisać domenę własnego serwera Jiry (domyślnie `*.atlassian.net`). Szczegóły i to, co jest wysyłane: sekcja „Rozszerzenie przeglądarki (Chrome i Edge)” niżej i [browser-extension/README.md](browser-extension/README.md).

### Nowości 1.6

- **Zakładka Miesiąc** w Raportach: wybór miesiąca, suma względem celu miesiąca (dni robocze minus święta) z rozbiciem „przepracowane / do celu”, tabele według projektu i według zadania z kluczem Jiry oraz wykres dni z linią celu.
- **Polskie święta ustawowe** (stałe i ruchome od Wielkanocy, w tym Boże Ciało) wyliczane w `src-tauri/src/holidays.rs` i `src/holidays.ts` z jednej listy wzorcowej. Ustawienie „Uwzględniaj święta w celu” (domyślnie włączone) odejmuje je od dni roboczych w celu dnia, tygodnia, miesiąca, tablicy tygodnia, trayu i powiadomieniu o celu.
- **Eksport CSV i XLSX** z zakładki Miesiąc i z „Wybranych dni” przez okno zapisu. Jeden wiersz na przedział (przedziały dzielone przez lokalną północ, część do północy kończy się o 24:00): data, start, koniec, minuty, godziny dziesiętne, zadanie, klucz Jiry, projekt, notatka dnia. CSV jest pod polski Excel (UTF-8 z BOM, średnik, przecinek dziesiętny). XLSX ma arkusze „Przedziały” i „Podsumowanie” (dni × zadania z sumami w godzinach) i zamrożony pierwszy wiersz. Trwający pomiar liczy się do chwili eksportu z dopiskiem „trwa”. Zadania w koszu nie trafiają do eksportu. XLSX działa tylko w aplikacji; w trybie podglądu eksport zwraca tekst CSV.
- **Tryb skupienia**: przycisk „Skupienie” na pasku bieżącego pomiaru i w widgecie (bez pomiaru blok startuje na ostatnio mierzonym zadaniu i uruchamia pomiar). Długość bloku (domyślnie 25 min, przyciski 25/50 albo dowolna liczba 1–240) i przerwy (5 min, przyciski 5/10 albo 1–60) w Ustawieniach → Skupienie. Widget pokazuje odliczanie zamiast zegara pomiaru, a pasek pomiaru odliczanie obok czasu. W czasie bloku powiadomienia Windows (terminy, długi pomiar, wieczór, cel, Jira) i propozycje agentów czekają w kolejce i przychodzą po bloku jako jedno zbiorcze (jedno zaległe przychodzi bez zmian, z przyciskami); przypomnienie o braku pomiaru milczy w czasie bloku i przerwy. Koniec bloku liczy się z zegara ściennego, także po uśpieniu komputera, i daje powiadomienie oraz baner z „Przerwa” i „Kolejny blok”. Przerwa domyślnie zatrzymuje pomiar (ustawienie „Przerwa zatrzymuje pomiar”), a po niej przychodzi „Wracasz do X?” z „Rozpocznij”. Stop albo pomiar innego zadania kończy blok jako nieukończony, bez pytań; „Zakończ blok” przerywa blok, a pomiar trwa. Dashboard ma kartę „Skupienie” z liczbą ukończonych bloków dziś i w tym tygodniu. Nowe tabele `focus_blocks(id, task_id, start, end, completed)` i `focus_queue(id, key, title, body)` powstają przy starcie (`CREATE TABLE IF NOT EXISTS`). Trwający blok i kolejka przeżywają restart (odłożone powiadomienia wracają bez przycisków i przychodzą po bloku); zakończony blok czekający na decyzję, przerwa i „Wracasz do X?” żyją w pamięci, więc restart aplikacji je kasuje. Timer nigdy nie przełącza się sam.
- Bez nowych tabel w M1–M2; ustawienia dostają klucz `holidaysInTarget` z wartością domyślną. Sprawdzone testami Rust (święta 2024–2030, CSV, XLSX odczytany przez `calamine`) i Playwright na podglądzie; okno zapisu i otwarcie plików w Excelu nie były sprawdzane w natywnej aplikacji.
- Tryb skupienia sprawdzono testami Rust (koniec bloku z zegara ściennego, kolejka powiadomień także po restarcie, przerwanie bloku przez Stop i pomiar innego zadania, przerwa z pytaniem o powrót, liczba ukończonych bloków) i Playwright na podglądzie; toasty, uśpienie komputera i widget w natywnej aplikacji ani po instalacji NSIS nie były sprawdzane.

### Nowości 1.5

- **Rozpoznanie zadania agenta**: dla każdego pracującego agenta (Claude Code, Codex) aplikacja czyta gałąź git wprost z pliku `HEAD`, bez uruchamiania `git` (także worktree i submoduł z plikiem `.git` zawierającym `gitdir:`; odłączony HEAD jako „odłączony HEAD <hash>”; wynik w cache na 30 s na katalog). Klucz Jiry pochodzi z nazwy gałęzi (np. `feature/WMS-123-eksport`), a gdy go tam nie ma, z nazwy sesji Claude Code. Zadanie dopasowuje się w kolejności: przypisanie folderu, lokalne zadanie z tym kluczem, zgłoszenie znane z `jira_issues`. Kafelek **Agenci AI** na Dashboardzie pokazuje gałąź i dopasowane zadanie, a dla folderu bez dopasowania pozwala wybrać zadanie.
- **Przypisania folderów**: nowa tabela `agent_bindings` powstaje przy starcie (`CREATE TABLE IF NOT EXISTS`); przypisanie tworzy „Zapamiętaj dla tego folderu” w banerze propozycji albo wybór zadania w kafelku Agenci AI. Kopia SQLite obejmuje tę tabelę, bo kopiowany jest cały plik bazy.
- **Propozycje przełączenia**: gdy agent pracuje co najmniej 2 minuty nad dopasowanym zadaniem innym niż mierzone (albo timer stoi), aplikacja proponuje przełączenie: baner w głównym oknie (lista, gdy agenci pracują nad różnymi zadaniami), linia „Agent: …” w widgecie, a przy schowanym głównym oknie powiadomienie z przyciskiem „Przełącz”. „Przełącz” uruchamia zadanie i zatrzymuje poprzedni pomiar; dla zgłoszenia bez lokalnego zadania tworzy je z tytułem z listy zgłoszeń, a gdy listy nie pobrano od startu aplikacji, z tytułem pobranym wprost z Jiry. „Nie teraz” wycisza parę do końca pracy agenta, „Nie proponuj dla tego folderu” zapisuje folder (z podfolderami) w ustawieniach. Wyłącznik funkcji i lista wyciszonych folderów („Proponuj znowu”) są w Ustawieniach → Czas pracy. Ukończone zadania nie dostają propozycji. Timer nigdy nie przełącza się sam.
- **Przerwa przypisana do pracy agenta**: gdy po co najmniej 10 minutach nieobecności okazuje się, że agent pracował nad innym zadaniem X, baner bezczynności ma trzecią odpowiedź „Przypisz przerwę do X”. Dzieli ona pomiar w chwili wyjścia istniejącym mechanizmem podziału i dalej mierzy X; gdy przerwa zaczęła się razem z pomiarem, cały pomiar przechodzi na X.
- Rozpoznanie i propozycje sprawdzono testami Rust (gałąź w zwykłym repo, worktree, submodule i przy odłączonym HEAD, cache 30 s, klucz z gałęzi i nazwy sesji, reguła 2 minut, pamięć odrzuceń, wyciszone podfoldery, „Przełącz” z powiadomienia) oraz Playwright na podglądzie; w natywnej aplikacji i po instalacji NSIS nie były jeszcze sprawdzane.

### Nowości 1.4

- **Składnia szybkiego dodawania** w polu „Nowe zadanie” (parser `src/quickadd.ts`): klucz Jiry na początku (`WMS-123`; sam klucz bierze tytuł z listy zgłoszeń wczytanej w widoku Jira, a gdy jej nie ma, pokazuje błąd „Nie znam tytułu…”), szacunek `~30m`, `~2h`, `~1h30`, dzień w planie `@dziś`, `@jutro`, `@pon`…`@nd`, `@20.10`, termin `do:pt`, `do:20.10`, priorytet `!` / `!niski`, projekt `#nazwa`, szablon `/nazwa` i `\` przed znakiem dla zapisu dosłownego (`C\#`). Pod polem widać chipy z rozpoznanymi polami; kliknięcie chipa usuwa zapis, Tab uzupełnia projekt i szablon, błędne zapisy (np. `@31.02`) blokują dodanie. Tekst bez zapisów dodaje się jak dotąd. Zadanie z polami powstaje w jednej transakcji (komenda `quick_add`).
- **To samo pole w szybkim przełączaniu**: gdy nic nie pasuje, Enter tworzy zadanie i uruchamia pomiar (poprzedni staje), Shift+Enter tylko tworzy.
- **Szablony zadań**: „Zapisz jako szablon” w rozwiniętym kafelku, zarządzanie w Ustawieniach → Szablony (zmiana nazwy, edycja, usunięcie z potwierdzeniem; nazwy unikalne bez względu na wielkość liter). Szablon ma tytuł z polami `{klucz}` i `{tytuł}`, kroki, szacunek, projekt i priorytet; `/review WMS-123` tworzy zadanie z tytułem z szablonu i zgłoszenia, a pola podane jawnie w linii wygrywają z szablonem. Kopia SQLite obejmuje szablony, przywracanie je zastępuje.
- Nowa tabela `task_templates` powstaje przy starcie (`CREATE TABLE IF NOT EXISTS`); istniejące dane się nie zmieniają.
- Składnię i szablony sprawdzono testami Playwright (parser i e2e na podglądzie z symulowaną Jirą) oraz Rust; w natywnej aplikacji i po instalacji NSIS nie były jeszcze sprawdzane.

### Nowości 1.3

- **Akcje w powiadomieniach Windows**: powiadomienia idą bezpośrednio przez `tauri-winrt-notification`, więc kliknięcie i przyciski trafiają do aplikacji. Termin otwiera Zadania z filtrem „Dzisiaj: plan i praca”, długi pomiar rozwija bieżące zadanie i ma przyciski „Zatrzymaj” i „Podziel…”, wieczór otwiera Zamknięcie dnia i ma „Zatrzymaj teraz”, cel otwiera Zamknięcie dnia, nowe zgłoszenia z Jiry mają „Rozpocznij” (przy jednym zgłoszeniu) i „Zaplanuj na dziś”. Timer zmieniają tylko przyciski, które klikniesz; nic nie przełącza się samo. Szczegóły w sekcji „Powiadomienia”. Przyciski sprawdzono testami Rust i Playwright; na zainstalowanym pakiecie NSIS nie były jeszcze klikane.
- **Siatka testów**: zrzuty wzorcowe głównych widoków, Jiry i widgetu (oba motywy, 760 i 1280 px) na stałych danych `tests/fixtures/seed.ts` i stałym zegarze; odświeżanie przez `npm run test:visual:update`. Nowe testy e2e: import nowych zgłoszeń z Jiry, szybkie przełączanie, baner bezczynności, przywracanie kopii, nawigacja z powiadomień. CI uruchamia zrzuty osobnym krokiem i zapisuje różnice jako artefakt `playwright-report`.
- **`npm run check`**: build, Playwright, `cargo fmt --check` i `cargo test --locked` jednym poleceniem.
- Bez zmian w bazie danych.

### Nowości 1.1

- **Nowe zgłoszenia z Jiry**: co 5 minut aplikacja sprawdza zgłoszenia przypisane do Ciebie (`assignee = currentUser()`, wszystkie projekty, nie tylko wybrane tablice). Każde nowe otwarte przypisanie staje się zadaniem w „Do zaplanowania” (bez dnia w planie, bez uruchamiania timera), a Windows pokazuje powiadomienie. Przy pierwszym sprawdzeniu dla konta obecne przypisania są tylko zapamiętywane, więc istniejący backlog nie jest importowany. Zgłoszenie powiązane już z lokalnym zadaniem (także w archiwum lub koszu) nie jest dublowane; zgłoszenie zdjęte z Ciebie i przypisane ponownie wraca jako nowe. Opcję wyłącza **Ustawienia → Jira → „Dodawaj nowe zgłoszenia przypisane do mnie”**.
- **Status z Jiry na kafelku**: obok klucza zgłoszenia widać jego status (np. „Code review”) w Zadaniach i na tablicy tygodnia. Status odświeża się przy każdym sprawdzeniu (otwarte i zmienione w ciągu 14 dni) oraz przy odświeżeniu zakładki Jira.
- **Agenci AI**: praca Claude Code i Codex liczy się jako aktywność przy wykrywaniu bezczynności, a Dashboard pokazuje pracujących agentów (szczegóły niżej).

### Organizacja pracy — 1.0

Czas:

- **Podział przedziału**: edytor przedziału ma zakładki „Godziny” i „Podziel”. Wybierasz moment podziału i zadanie dla drugiej części; podgląd pokazuje obie części z czasem trwania. Działa także dla trwającego pomiaru, bez Stop: pierwsza część zostaje zamknięta, druga mierzy dalej pod wybranym zadaniem. Przycisk „Podziel” jest na pasku bieżącego pomiaru w Zadaniach i przy każdym przedziale na osi dnia w Plan i historia.
- **Cel dzienny i tygodniowy**: cel dnia w minutach (domyślnie 480) i dni robocze (domyślnie Pon–Pt) w Ustawieniach; cel tygodnia to cel dnia razy liczba dni roboczych. Pasek postępu pod czasem „Dzisiaj” w Zadaniach (bursztynowy po przekroczeniu), karty „Dzisiaj” i „Ten tydzień” z linią celu na Dashboardzie, różnica względem celu przy dniach tygodnia i informacja „Do celu brakuje …” / „Cel przekroczony o …” w zamknięciu dnia, bez blokady.
- **Trafność szacunków** na Dashboardzie: ukończone zadania z szacunkiem i czasem z ostatnich 30 lub 90 dni albo z całej historii, mediana stosunku rzeczywistość/plan, odsetek w widełkach ±20% i przekroczeń, histogram, mediana według projektu i największe rozjazdy z przejściem do raportu. Karta „Zagrożone teraz” wymienia otwarte zadania ponad szacunkiem; ich kafelek pokazuje procent w kolorze ostrzegawczym.

Planowanie:

- **Ręczna kolejność**: uchwyt ⋮⋮ na kafelku. Przeciągnięcie albo Alt+↑ / Alt+↓ na uchwycie zmienia kolejność wewnątrz pasma priorytetu; przeciągnięcie do innego pasma zmienia priorytet. Aktywny timer zawsze jest pierwszy. Ta sama kolejność obowiązuje w liście zadań, szybkim przełączaniu, planie dnia i na Dashboardzie.
- **Plan kontra rzeczywistość**: zakładka Dzień w Plan i historia ma oś z torem „Plan” (zadania zaplanowane na dzień, w ręcznej kolejności od godziny początku dnia, domyślnie 08:00; długość z szacunku, bez szacunku 30 min kreskowane; zadanie z „Godziną startu w planie” jest przypięte do tej godziny) i torem „Rzeczywistość” (przedziały dnia). Kliknięcie bloku otwiera edycję przedziału albo plan zadania. Tabela pod osią porównuje plan z rzeczywistością i oznacza pracę „nieplanowane” oraz plan „niezaczęte”.
- **Tablica tygodnia** (Plan i historia → Tydzień → Tablica, domyślnie): kolumna „Do zaplanowania” z wyszukiwaniem i sekcją „Zaległe (n)” oraz siedem dni Pon–Nd z sumą szacunków względem pojemności (cel dnia albo „wolne”) i faktycznym czasem. Pasek pojemności jest zielony poniżej 90%, bursztynowy do 100%, czerwony powyżej. Przeciągnięcie karty zmienia dzień lub kolejność; bez myszy służy menu „Przenieś na…”. Dotychczasowa treść tygodnia jest w zakładce „Podsumowanie”.
- Priorytet zmienisz bez rozwijania zadania, wybierając go bezpośrednio na kafelku. Zapis i kolejność aktualizują się od razu; aktywny timer pozostaje pierwszy.
- Szybki wybór zadania: przycisk ⇄ lub Ctrl+K w aplikacji, globalnie skrót szybkiego przełączania (domyślnie Ctrl+Shift+Space). Wyszukaj zadanie, wybierz strzałkami, Enter uruchamia pomiar i zamyka poprzedni. Esc zamyka okno.
- Plan zadania zawiera szacowany czas w minutach i listę kroków. Kafelek pokazuje wykonanie względem szacunku, plan dnia sumuje szacunki.
- Cykle według dni tygodnia: osobne zadanie na każdą datę, z osobnym czasem i wyzerowaną checklistą. Codziennie = wszystkie dni, tygodniowo = jeden dzień. Generowanie następuje przy otwarciu i co minutę; po przerwie uzupełnia maksymalnie ostatnie 366 dni. Odznaczenie dni wyłącza cykl, archiwizacja wstrzymuje generowanie. Po przywróceniu następuje uzupełnienie zaległych dat.
- **Plan i historia → Zamknięcie dnia**: suma pracy, notatki każdego zadania, przerwy między wpisami, raport TS, przekładanie niedokończonego planu na następny dzień i oznaczenie dnia jako sprawdzonego. Oznaczenie nie blokuje późniejszych poprawek.
- **Historia i kopie**: wyszukiwanie po tytułach, projektach, notatkach i datach, zakres dat, archiwum, kosz oraz eksport/przywracanie kopii SQLite.
- Archiwum ukrywa zadanie z bieżącej listy i planowania, zachowując raporty. Usunięcie przenosi zadanie wraz z czasem do kosza; przywrócenie odzyskuje historię. Przedziały z kosza pozostają zarezerwowane przy sprawdzaniu konfliktów czasu.
- Po co najmniej 10 minutach bezczynności Windows, przy powrocie do aktywnego pomiaru, aplikacja pyta o zachowanie czasu lub zakończenie sesji na początku przerwy. Nie skraca czasu automatycznie. Opcję można wyłączyć w ustawieniach.
- Praca agentów AI liczy się jako aktywność. Claude Code jest wykrywany po statusie `busy` w `~/.claude/sessions/<pid>.json` i działającym procesie (gdy `busy` trwa ponad 15 min, wymagany jest też świeży zapis transkryptu), Codex po zapisie pliku `~/.codex/sessions/**/rollout-*.jsonl` w ostatnich 90 s. Przerwa liczy się od późniejszej z chwil: ostatniego wejścia z klawiatury/myszy albo końca pracy agenta. Gdy agent zacznie pracę podczas nieobecności, przerwa jest zapominana bez pytania. Pracujących agentów pokazuje kafelek **Agenci AI** na Dashboardzie.
- Eksport działa również podczas pomiaru i tworzy spójną kopię SQLite. Przywracanie pokazuje nazwę i liczby wpisów, wymaga potwierdzenia zastąpienia danych i najpierw zapisuje `backups/before-restore-*.sqlite`. Aktywna sesja z kopii jest kończona na moment eksportu (dla starszych kopii: modyfikacji pliku). Bieżąca konfiguracja Jiry pozostaje zachowana, token nie jest przenoszony.

Domyślną zakładką nadal są **Zadania**.

### Ustawienia

Od 1.7 ustawienia są osobną zakładką **Ustawienia** (ostatnia pozycja nawigacji, skrót **Ctrl+,**) z listą sekcji po lewej (przy wąskim oknie lista rozwijana nad treścią), polem „Szukaj ustawienia” i zapamiętaną ostatnią sekcją. Sekcje: Czas pracy, Kalendarz, Jira, Powiadomienia, Skupienie, Szablony, Okna, Skróty, Uruchamianie. Najważniejsze z nich:

- **Czas pracy**: pytanie o czas po co najmniej 10 minutach bezczynności, „Praca agentów AI (Claude Code, Codex) liczy się jako aktywność”, cel dzienny w minutach (0–1440), początek dnia (`HH:MM`, dla osi planu) i dni robocze. Pod polami widać wyliczony cel dnia i tygodnia. Od 1.7 także „Planuj z poprawką szacunków”, podpowiedzi z aktywnego okna i rozszerzenie przeglądarki.
- **Kalendarz** (1.7): identyfikator aplikacji (client ID), dzierżawca, połączenie konta Microsoft i „Pokazuj tytuły spotkań”.
- **Jira**: „Dodawaj nowe zgłoszenia przypisane do mnie” (domyślnie włączone); od 1.7 „Pozwól aplikacji zmieniać zgłoszenia w Jirze”, pytanie o przesunięcie przy starcie pracy z nazwą statusu, statusy oznaczające „Czekam na” i prefiks nazwy gałęzi.
- **Okna**: „Pokazuj mini-widget zawsze na wierzchu” i „Tylko gdy główne okno jest schowane lub bez fokusu”.
- **Powiadomienia**: terminy na dziś i przekroczone, długi pomiar bez Stop (minuty, domyślnie 240, 0 wyłącza), wieczorne „pomiar nadal trwa” (godzina, domyślnie 17:30, z osobnym wyłącznikiem) i osiągnięcie celu dnia (domyślnie wyłączone).
- **Skróty**: dwa pola przechwytujące kombinację klawiszy, z błędem rejestracji przy polu i przyciskiem przywrócenia domyślnego.
- **Uruchamianie**: „Uruchamiaj z Windows” i „Uruchamiaj schowane do traya”.

Ustawienia są zapisywane w bazie SQLite (w podglądzie w localStorage), a backend sprawdza zakresy. Zapis od razu przerejestrowuje skróty i odświeża widget oraz reguły powiadomień.

### Skróty klawiszowe

- **Ctrl+K** — szybkie przełączanie w aplikacji.
- **Ctrl+Shift+Space** (domyślnie, konfigurowalny) — szybkie przełączanie z dowolnego okna.
- **Ctrl+Alt+Space** (domyślnie, konfigurowalny) — Start/Stop z dowolnego okna. Trwający pomiar zostaje zatrzymany. Bez pomiaru startuje ostatnio mierzone zadanie, które nie jest ukończone ani zarchiwizowane; gdy takiego nie ma, otwiera się szybkie przełączanie. Potwierdzenie „Stop: X · 01:23:45” albo „Start: X” pojawia się w dymku ikony traya, w widgecie, a gdy widget jest schowany, także jako powiadomienie Windows.
- **Alt+↑ / Alt+↓** na uchwycie ⋮⋮ — zmiana kolejności zadania.

Przy konflikcie skrótu globalnego z innym programem pole w Ustawieniach pokazuje błąd; przycisk ⇄ i Ctrl+K pozostają dostępne.

### Mini-widget

Małe okno 320×64 zawsze na wierzchu, domyślnie wyłączone. Pokazuje nazwę bieżącego zadania i zegar, przycisk Stop albo Start (ta sama logika co skrót Start/Stop) oraz ⇄. Bez pomiaru wyświetla „Brak pomiaru”. Kliknięcie nazwy otwiera główne okno, uchwyt pozwala przesunąć widget. Okno nie powinno zabierać fokusu aplikacji, w której pracujesz (`focusable: false`). X chowa widget do końca sesji aplikacji bez zmiany ustawienia; przywraca go przełącznik „Mini-widget” w nagłówku głównego okna. W trybie „tylko w tle” widget pojawia się, gdy główne okno jest schowane lub bez fokusu (sprawdzane co 2 s i przy zmianie fokusu).

### Okna, tray i autostart

- Pozycja i rozmiar głównego okna i widgetu są zapamiętywane (`tauri-plugin-window-state`), a zapis następuje przy schowaniu okna, przy „Zakończ aplikację” z traya i przy `--prepare-update`. Okna przypomnienia i szybkiego przełączania mają stałe położenie.
- Ikona traya ma wariant z kropką, gdy pomiar trwa. Dymek co 5 s pokazuje `Zadanie · 01:23:45 · dziś 5:10 / 8:00` (do 127 znaków). Menu: nieklikalna pozycja z nazwą bieżącego zadania, „Otwórz Todo Track”, „Szybkie przełączanie”, „Zatrzymaj pomiar”, „Zakończ aplikację”.
- Autostart (`tauri-plugin-autostart`) tworzy wpis `Run` w rejestrze bieżącego użytkownika. Opcja „schowane do traya” dodaje argument `--hidden`: główne okno się nie pokazuje, widget pojawia się, jeśli jest włączony. Stan „Uruchamiaj z Windows” jest czytany z rejestru, więc wyłączenie wpisu w Menedżerze zadań widać w Ustawieniach.

### Powiadomienia

Powiadomienia Windows wysyła backend Rust bezpośrednio przez `tauri-winrt-notification` (plugin `tauri-plugin-notification` nie przekazuje na Windows kliknięć ani przycisków i zostaje tylko dla innych systemów); reguły są sprawdzane co 30 s:

- **Termin**: przy starcie i o 09:00, gdy są nieukończone zadania z terminem dziś lub wcześniej; treść podaje liczbę i pierwsze trzy tytuły. Kliknięcie: Zadania z filtrem „Dzisiaj: plan i praca” (tylko po takim kliknięciu filtr obejmuje też terminy dziś i zaległe; ręcznie włączony filtr działa jak dotąd).
- **Długi pomiar**: aktywny przedział trwa co najmniej ustawioną liczbę minut; jedna wiadomość na przedział. Po podziale pomiaru nowa część liczy czas od momentu podziału. Kliknięcie: Zadania z rozwiniętym bieżącym zadaniem. Przyciski „Zatrzymaj” i „Podziel…” (edytor bieżącego pomiaru na zakładce Podziel).
- **Wieczór**: o ustawionej godzinie pomiar nadal trwa. Kliknięcie: Zamknięcie dnia. Przycisk „Zatrzymaj teraz”.
- **Cel**: suma dnia roboczego osiągnęła cel (domyślnie wyłączone). Kliknięcie: Zamknięcie dnia.
- **Nowe zgłoszenia z Jiry**: kliknięcie otwiera Zadania przy grupie „Nierozpoczęte” (jedno zgłoszenie: rozwinięte). Przyciski „Rozpocznij” (tylko przy jednym zgłoszeniu; ta sama ścieżka co w widoku Jira, więc zadanie wraca z kosza lub archiwum) i „Zaplanuj na dziś”.
- **Potwierdzenie skrótu Start/Stop**: kliknięcie pokazuje główne okno.

Każde powiadomienie pojawia się najwyżej raz: klucz trafia do tabeli `notifications_sent` przed wysłaniem, także po ponownym uruchomieniu. Wpisy starsze niż 7 dni są usuwane przy starcie. Przyciski działają, gdy aplikacja jest uruchomiona: „Zatrzymaj” zatrzymuje tylko ten pomiar, którego dotyczy powiadomienie (także jego dalszy ciąg po podziale; gdy nie ma czego zatrzymać, pokazuje Zadania), „Podziel…” na starym toaście nie otwiera pomiaru innego zadania, kliknięcie nie zamyka otwartego okna dialogowego, „Rozpocznij” zatrzymuje poprzedni pomiar; nic poza tym nie przełącza timera. Nieznana akcja (np. stary toast po aktualizacji) tylko pokazuje główne okno. Kliknięcie starego toastu po restarcie uruchamia aplikację. W `tauri dev` powiadomienia mogą nie pojawić się albo podpisywać się inną aplikacją (AUMID PowerShella), a przyciski mogą nie działać; miarodajny jest pakiet NSIS.

### Dotychczasowe funkcje

- Dodawanie, edycja, ukończenie, przywracanie i usuwanie zadań.
- Wyszukiwanie i filtry: do zrobienia, ukończone, wszystkie.
- Start / Stop przy zadaniu; tylko jeden aktywny pomiar. Start następnego zadania automatycznie zatrzymuje poprzednie.
- Tryb ciemny (domyślny) i jasny, z zapamiętywaniem wyboru.
- Uproszczony widok: kafelek zadania pokazuje sumę czasu; po kliknięciu nazwy rozwija wszystkie przedziały z datą i godziną początku oraz końca.
- Edycja godzin zakończonych przedziałów przyciskiem ołówka. Bieżący pomiar należy najpierw zatrzymać. Zapis przelicza sumy i raporty; błędne daty, koniec w przyszłości oraz nakładanie się na inne pomiary są odrzucane.
- Raporty dzienne z wyborem dowolnej daty, nawigacją poprzedni/następny dzień, łącznym czasem pracy i rozbiciem na zadania oraz przedziały. Uwzględniają też ukończone zadania.
- Rozpoczęte, nieukończone zadania mają na kafelku status „W trakcie”, również po Stop i restarcie aplikacji. Status wynika z historii przedziałów, więc obejmuje też wcześniejsze zadania. Przywrócenie ukończonego zadania z historią przywraca ten status.
- Lista grupuje zadania: „W trakcie” na górze (bieżący timer pierwszy), potem „Do zaplanowania”, a w filtrze wszystkich również „Ukończone”. Puste grupy są ukryte; wyszukiwanie działa w grupach.
- Dashboard pokazuje czas dzisiaj, w ostatnich 7 dniach i bieżącym miesiącu, wykres dni z przejściem do raportu, liczbę zadań według statusu i 5 zadań z największym czasem dzisiaj. Domyślny widok po uruchomieniu to nadal Zadania.
- Przycisk „Raport TS” rozwija tekst do skopiowania: data, zadania, suma przedziałów każdego zadania w godzinach dziesiętnych (np. 1,50) i suma dnia. „Kopiuj raport” kopiuje tekst z tabulatorami między kolumnami. Dwa miejsca po przecinku; suma dnia zaokrąglana niezależnie z pełnego czasu. Trwający pomiar liczony do bieżącej chwili.
- Raporty dzienne → **Wybrane dni**: kalendarz miesiąca (kropka przy dniach z pracą, czas w podpowiedzi), skróty „Ten tydzień”, „Poprzedni tydzień”, „Ten miesiąc”, „Dni robocze tego miesiąca”, „Wyczyść” i lista zestawu z sumami. Raport TS w układzie „Po dniach” (Dzień, Zadanie, Czas, suma po każdym dniu i łączna) albo „Zbiorczo” (zadania zsumowane, notatki łączone średnikiem). Ostatnie zaznaczenie i układ są pamiętane lokalnie.
- Minimalizacja pozostawia aplikację na pasku zadań. Krzyżyk i przycisk „Schowaj do traya” chowają okno do zasobnika. Pomiar trwa dalej w obu przypadkach.
- Kliknięcie ikony traya przywraca okno. Menu: otwarcie, szybkie przełączanie, zatrzymanie pomiaru, zakończenie aplikacji.
- Zakończenie z menu traya zatrzymuje pomiar i zamyka proces.
- Jedna instancja aplikacji; kolejne uruchomienie przywraca istniejące okno.
- Własny pasek okna dopasowany do motywu: przeciąganie, minimalizacja, maksymalizacja/przywrócenie i zamknięcie do traya.
- Przypomnienie po 5 minutach bez aktywnego pomiaru, gdy aplikacja pozostaje w tle.
- Osobny widok Jira: kilka wybranych tablic, zgłoszenia przypisane do zalogowanego użytkownika, wyszukiwanie, link i „Rozpocznij” uruchamiające lokalny timer.

## Planowanie i historia pracy (0.2.0)

Rozwiń zadanie, aby ustawić priorytet (wysoki, normalny, niski), planowany dzień, opcjonalny termin, projekt/obszar oraz „Czekam na…”. W obrębie grup pierwszeństwo ma aktywny timer, następnie wyższy priorytet. Zapisanie blokady zatrzymuje pomiar tego zadania; Start usuwa blokadę. Filtr projektu oraz „Dzisiaj: plan i praca” pomagają ograniczyć listę.

W tym samym formularzu wybierz dzień notatki i wpisz „Co zrobiłem”. Notatki pozostają przy konkretnych dniach. Raport TS domyślnie dołącza opis z dnia raportu do nazwy zadania; można to wyłączyć.

Zakładka **Plan i historia** zawiera plan dowolnego dnia, przekładanie zadań, zaległe plany i chronologiczną oś pracy. „Dodaj czas ręcznie” uzupełnia zapomniany pomiar także w raporcie dziennym. Backend odrzuca nakładanie przedziałów i czas z przyszłości. Oś pokazuje luki wyłącznie między zapisanymi pomiarami, a edycja otwiera dotychczasowy formularz pełnego przedziału.

Widok **Tydzień** obejmuje poniedziałek–niedzielę, sumy dni i projektów oraz zadania z pracą w tym tygodniu. Statusy zadań są aktualne, nie historyczne. Niezakończone sprawy można zaplanować na następny poniedziałek. Aplikacja nadal uruchamia się na Zadaniach.

## Instalacja i aktualizacje

Wersja w repozytorium: 1.7.2 (przebudowa interfejsu według `UX_AUDIT.md`: lewy panel nawigacji, mierzone zadanie w pasku, banery, kontrast i klawiatura; bez nowych tabel i ustawień poza zapamiętanym zwinięciem panelu w pamięci WebView), wcześniej 1.7.1 (przypięte zadania w widgecie i menu traya; bez nowych tabel i ustawień) i 1.7.0 (kalendarz Microsoft i pojemność dnia, zakładka Ustawienia, „Rozłóż”, poranny przegląd, cykle miesięczne i „Czekam na”, czas agentów, przegląd tygodnia, podpowiedzi z okna i z rozszerzenia przeglądarki, przesuwanie i tablica zgłoszeń Jiry; nowe tabele `calendar_events`, `agent_sessions`, `week_notes`, `jira_board_columns`, `jira_statuses` i `jira_board_meta` tworzone przy starcie przez `IF NOT EXISTS`, nowe ustawienia dostają wartości domyślne). Instalator dołącza `todo-track-bridge.exe` i folder `browser-extension`; odinstalowanie usuwa manifest hosta rozszerzenia i oba wpisy w `HKCU`. Build NSIS tworzy `src-tauri/target/release/bundle/nsis/Todo Track_1.7.2_x64-setup.exe`; gotowe instalatory (1.2.0, 1.3.0, 1.4.0, 1.5.0, 1.6.0, 1.7.0, 1.7.1 i 1.7.2) leżą w repozytorium w `releases/`. Instalatory 1.2.0–1.7.2 nie były jeszcze instalowane, więc przyciski w powiadomieniach, szybkie dodawanie, szablony, propozycje agentów, eksport, tryb skupienia, kalendarz, rozszerzenie przeglądarki i zapisy do Jiry nie są sprawdzone natywnie. Od 1.6.0 tabele `focus_blocks` i `focus_queue` powstają przy starcie (`IF NOT EXISTS`). Od 1.5.0 tabela `agent_bindings` powstaje przy starcie (`IF NOT EXISTS`). Od 1.4.0 tabela `task_templates` powstaje przy starcie (`IF NOT EXISTS`). Od 1.1.0 nowa tabela `jira_issues` (statusy i ostatnie przypisania) powstaje przy starcie (`IF NOT EXISTS`). Otwarte z 1.0: aktualizacja przez instalator z działającego 0.3.0 przy aktywnym timerze (PLAN_1.0.md, etap 5 pkt 4) nie jest sprawdzona i blokuje wydanie 1.0.0.

Przy pierwszym uruchomieniu 1.0.0 na bazie z 0.3.0 powstaje kopia `before-1.0.0-<czas>.sqlite` (opis niżej). Nowe indeksy i tabela `notifications_sent` są tworzone przy starcie (`IF NOT EXISTS`) bez zmiany istniejących danych.

Od 0.2.1 minimalizacja pozostawia okno na pasku zadań; krzyżyk chowa do zasobnika. Zweryfikowano natywnie stan minimized/visible, zamknięcie do traya i przywracanie na osobnej bazie testowej.

Instalator ma polski interfejs, własne grafiki i instaluje dla bieżącego użytkownika. Gdy wykryje istniejącą instalację, wyświetla potwierdzenie wersji i aktualizuje program w dotychczasowym katalogu, bez odinstalowywania. Zachowuje skróty i dane, a po zwykłej aktualizacji uruchamia aplikację ponownie. Nowszej wersji nie zastąpi starszą. Nie ma serwera ani automatycznego pobierania pakietów.

Przy aktualizacji **z wersji starszej niż 0.2.0** zakończ aplikację z menu traya. Od 0.2.0 instalator wywołuje `--prepare-update`: aplikacja zapisuje koniec bieżącej sesji i kończy proces. Jeżeli nadal działa, instalator czeka na ręczne zamknięcie lub pozwala anulować; nie wymusza ubicia procesu w trybie aktualizacji.

Przed pierwszym otwarciem istniejącej bazy przez nową wersję powstaje spójna kopia SQLite (także danych WAL) w `%APPDATA%\pl.todotrack.desktop\backups\before-<wersja>-<czas>.sqlite`. Kopie nie są automatycznie usuwane. Nowe pola są w dodatkowej tabeli `task_details`; istniejące zadania i przedziały pozostają bez zmian. Usuwanie zadania usuwa także jego plan i notatki.

Szablon instalatora pochodzi z [Tauri CLI 2.12.2](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), z lokalnymi zmianami w `src-tauri/installer/template.nsi`. Przy zmianie wersji Tauri należy porównać szablon z upstreamem. Grafiki można odtworzyć skryptem `src-tauri/installer/make-artwork.ps1`; tłumaczenia są w `Polish.nsh`. Logo aplikacji (kiwi) ma źródło w `app-icon.svg`; ikony w `src-tauri/icons/` generuje `npm run tauri icon app-icon.svg`, a ikonę traya z kropką pomiaru (`icon-active.ico`) ten sam `make-artwork.ps1`. Plan i warianty logo: `LOGO_PLAN.md`, `design/logo/`.

## Jira

Otwórz zakładkę **Jira** w aplikacji desktopowej. Adres `https://asistonwms.atlassian.net` jest wstępnie uzupełniony. Podaj e-mail konta Atlassian i token API **bez zakresów** (klasyczny), a następnie wybierz „Połącz i sprawdź”. Link „Utwórz token Atlassian” otwiera stronę zarządzania tokenami w przeglądarce. Token jest przechowywany w magazynie poświadczeń Windows; nie trafia do SQLite ani localStorage. „Rozłącz konto” usuwa zapisany token.

Wybierz tablice (maksymalnie 20) i zapisz wybór. Aplikacja pobiera wszystkie strony wyników dla `assignee = currentUser()`, łączy powtarzające się zgłoszenia i pokazuje informację, jeśli którejś tablicy nie udało się pobrać. Domyślnie widoczne są tylko otwarte zgłoszenia; filtr można wyłączyć. Wyszukiwanie działa po kluczu lub nazwie w pobranych wynikach. Odświeżenie następuje przy wejściu w zakładkę, zmianie tablic/filtra oraz po kliknięciu „Odśwież”.

„Rozpocznij” tworzy lokalne zadanie powiązane z kluczem Jiry albo wykorzystuje istniejące, przywraca je do zrobienia i uruchamia timer. Poprzedni pomiar zostaje zatrzymany. Link pozostaje dostępny również w rozwiniętym kafelku lokalnego zadania. Co 5 minut aplikacja dodaje też nowe zgłoszenia przypisane do Ciebie do „Do zaplanowania” i odświeża statusy na kafelkach (zob. „Nowości 1.1”; zapytanie `/rest/api/3/search/jql` z `assignee = currentUser() AND (statusCategory != Done OR updated >= -14d)`). Do 1.6 integracja tylko odczytywała Jirę. Od 1.7 może przesuwać zgłoszenia, przypisywać je, dodawać komentarze i tworzyć nowe (zob. „Nowości 1.7”), zawsze dopiero po kliknięciu; wyłącznik „Pozwól aplikacji zmieniać zgłoszenia w Jirze” w Ustawieniach → Jira blokuje wszystkie zapisy także w backendzie. Worklogi nie są wysyłane. Działa z Jira Cloud, nie z Server/Data Center.

Dokumentacja API: [tablice i stronicowanie Jira Cloud](https://developer.atlassian.com/cloud/jira/software/rest/api-group-board/), [logowanie e-mailem i tokenem](https://developer.atlassian.com/cloud/jira/platform/basic-auth-for-rest-apis/).

## Rozszerzenie przeglądarki (Chrome i Edge)

Rozszerzenie z folderu `browser-extension/` podpowiada zadanie ze zgłoszenia Jiry otwartego w aktywnej karcie, także gdy tytuł okna nie ma klucza (np. tablica z `?selectedIssue=WMS-123`): „Pracujesz teraz przy WMS-123? — Przełącz”. Timer nigdy nie przełącza się sam. Szczegóły: [browser-extension/README.md](browser-extension/README.md).

1. **Ustawienia → Czas pracy → „Rozszerzenie przeglądarki (Chrome i Edge)”** — włącz. Aplikacja zapisuje manifest hosta `todo-track-bridge.exe` i wpisy w `HKCU\Software\Google\Chrome\NativeMessagingHosts` oraz `HKCU\Software\Microsoft\Edge\NativeMessagingHosts` (bez uprawnień administratora); wyłączenie je usuwa. Nieudana rejestracja jest ponawiana co minutę.
2. Załaduj rozszerzenie: `chrome://extensions` albo `edge://extensions` → „Tryb dewelopera” → „Załaduj rozpakowane” → folder `browser-extension` (przycisk „Otwórz folder rozszerzenia” w Ustawieniach). W firmie administrator może je wdrożyć zasadami grupy (`ExtensionInstallForcelist`) — identyfikator rozszerzenia jest stały: `cecpcpkmmmkgfdklnkkkigofkcibnhbl`.
3. W opcjach rozszerzenia można dopisać domenę własnego serwera Jiry (domyślnie `*.atlassian.net`).

Co jest wysyłane: adres i tytuł aktywnej karty tylko dla domen z listy dozwolonych, nazwa przeglądarki (Chrome, Edge…), a dla innych stron i przeglądarki w tle samo „clear”. Aplikacja trzyma w pamięci tylko rozpoznany klucz zgłoszenia przez 30 s; nic nie trafia do bazy ani logów. Przy włączonym rozszerzeniu aplikacja co 5 s sprawdza, który program jest na pierwszym planie (nazwa procesu); tytuł okna czyta tylko przy włączonych „Podpowiedziach z aktywnego okna”.

## Przypomnienie w tle

Gdy główne okno jest schowane lub nie ma fokusu, a żaden pomiar nie jest aktywny, backend odlicza 5 minut (sprawdzanie co 2 sekundy). Następnie pokazuje małe okno nad innymi aplikacjami, bez przejmowania fokusu. „Wpisz zadanie” przywraca główne okno i ustawia kursor w polu nowego zadania; można też rozpocząć istniejące zadanie lub zgłoszenie z Jiry. „Za 5 minut” oraz krzyżyk odkładają przypomnienie. Uruchomienie pomiaru albo powrót do głównego okna resetują odliczanie. Samo posiadanie zadań na liście nie wyłącza przypomnienia — istotny jest aktywny pomiar.

Przypomnienie jest własnym oknem aplikacji, więc działa także bez instalowania pakietu NSIS. Logika działa w Rust i nie zależy od ograniczania timerów ukrytego WebView.

## Uruchomienie na Windows

Wymagania: Node.js 24 LTS (wersja z `.nvmrc`) i npm, Rust stable MSVC, Microsoft C++ Build Tools z pakietem **Desktop development with C++** oraz Windows SDK, WebView2 Runtime.
Oficjalna instrukcja: https://v2.tauri.app/start/prerequisites/

```powershell
npm ci
npm run tauri dev
```

Jeśli Rust został zainstalowany bez zmiany PATH, dodaj go dla bieżącego terminala:

```powershell
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
```

Instalator Windows (NSIS):

```powershell
npm run tauri build
```

Wynik w `src-tauri/target/release/bundle/nsis/`.

## Podgląd interfejsu

```powershell
npm run dev
```

Otwórz http://127.0.0.1:1420. Podgląd używa osobnego magazynu localStorage; jego dane nie są przenoszone do aplikacji desktopowej. Tray działa wyłącznie w Tauri.

## Zapis i pomiar czasu

Od 0.3.0 użyj **Historia i kopie → Kopie → Eksportuj kopię** — aplikacja tworzy spójny plik również podczas pracy. Ręczne kopiowanie katalogu opisane poniżej pozostaje alternatywą po zamknięciu programu.

Baza znajduje się w katalogu danych aplikacji, na Windows: `%APPDATA%\pl.todotrack.desktop\todo-track.sqlite`. Operacje są transakcyjne. Do kopii zapasowej zamknij aplikację przez menu traya i skopiuj cały katalog danych (SQLite może korzystać z plików WAL).

Rust zapisuje początek i koniec sesji. Frontend tylko wyświetla różnicę znaczników czasu; throttling ukrytego okna nie wpływa na wynik. Pomiar obejmuje czas uśpienia komputera. Po awarii lub wymuszonym zamknięciu aktywna sesja pozostaje otwarta i nalicza czas do kolejnego Stop. Przed dłuższą przerwą zatrzymaj pomiar. Zmiana zegara systemowego może wpłynąć na wynik. Bezczynność Windows nie skraca pomiaru automatycznie; aplikacja tylko pyta po powrocie (patrz wyżej). Autostart jest wyłączony, dopóki nie włączysz go w Ustawieniach.

Usunięcie zadania usuwa też jego sesje (z potwierdzeniem). Ukończenie zadania zatrzymuje jego bieżący pomiar. Dzisiejszy czas jest liczony od lokalnej północy, również dla sesji rozpoczętych poprzedniego dnia.

Raporty dzielą sesję przez lokalną północ i uwzględniają zmianę czasu (doby 23- i 25-godzinne). W rozwiniętym raporcie wyświetlany jest pełny przedział, a obok czas przypadający wyłącznie na wybrany dzień. Usunięte zadania i ich sesje znikają również z raportów. Edycja przedziałów korzysta z dotychczasowej bazy; aktualizacja aplikacji nie wymaga migracji danych.

## Sprawdzenie

Weryfikacja 1.7.2 (19 października 2026, gałąź `master`):

- `npm run build`: przechodzi.
- Playwright: 257 testów z zrzutami wzorcowymi przechodzi. Nowy `tests/navigation.spec.ts` sprawdza tryby panelu, a wzorce widoków zregenerowano świadomie dla nowego wyglądu.
- `cargo fmt --check` i 236 testów Rust (`cargo test --locked`): przechodzą.
- `npm run tauri build -- -- --locked`: przechodzi.

Nie sprawdzono w natywnej aplikacji:

- przyklejony pasek tytułu i panel przy przewijaniu;
- przeciąganie okna;
- wygląd przy skalowaniu Windows.

Weryfikacja 1.7.1 (19 października 2026, gałąź `release/1.0`): `npm run build`, 254 testy Playwright (nowy: przypinanie z kafelka i przełączanie w widgecie; zrzuty wzorcowe bez zmian), `cargo fmt --check`, 236 testów Rust (nowe: limit przypięć i pozycje menu traya) i `npm run tauri build -- -- --locked` przechodzą. Zmiana rozmiaru okna widgetu i menu traya nie były sprawdzone w natywnej aplikacji.

Weryfikacja 1.7.0 (16 października 2026, gałąź `release/1.0`): `npm run build`, 253 testy Playwright (w tym zrzuty wzorcowe; kalendarz w podglądzie, zakładka Ustawienia, „Rozłóż”, Rano, cykle i „Czekam na”, tor Agenci, Przegląd tygodnia, propozycje z okna i przeglądarki, przesuwanie i tablica Jiry), `cargo fmt --check`, 234 testy Rust (`cargo test --locked`) i `npm run tauri build -- -- --locked` przechodzą. Na mocno obciążonej maszynie część testów Playwright przekraczała 30 s przy tworzeniu strony; po ponownym uruchomieniu z mniejszą liczbą procesów przeszły wszystkie. Wzorce zrzutów Ustawień, nawigacji, osi, tablicy tygodnia i Jiry odświeżono lokalnie; CI na windows-latest nadal nie był uruchomiony.

Weryfikacja 1.6.0 (15 października 2026, gałąź `release/1.0`): `npm run build`, 149 testów Playwright (w tym zrzuty wzorcowe; zakładka Miesiąc, święta w celu, eksport CSV w podglądzie, tryb skupienia), `cargo fmt --check`, 135 testów Rust (`cargo test --locked`) i `npm run tauri build -- -- --locked` przechodzą. Wzorce zrzutów Ustawień, Zadań, Dashboardu i widgetu odświeżono lokalnie; CI na windows-latest nadal nie był uruchomiony.

Weryfikacja 1.5.0 (15 października 2026, gałąź `release/1.0`): `npm run build`, 137 testów Playwright (w tym zrzuty wzorcowe; kafelek Agenci AI, baner propozycji, wyciszanie folderów, trzecia odpowiedź banera bezczynności), `cargo fmt --check`, 116 testów Rust (`cargo test --locked`) i `npm run tauri build -- -- --locked` przechodzą. Wzorce zrzutów Ustawień (wyłącznik propozycji agentów) odświeżono lokalnie; CI na windows-latest nadal nie był uruchomiony.

Weryfikacja 1.4.0 (15 października 2026, gałąź `release/1.0`): `npm run build`, 123 testy Playwright (w tym zrzuty wzorcowe; parser szybkiego dodawania, e2e składni i szablonów), `cargo fmt --check`, 93 testy Rust (`cargo test --locked`) i `npm run tauri build -- -- --locked` przechodzą. Wzorce zrzutów Ustawień (sekcja Szablony) odświeżono lokalnie; CI na windows-latest nadal nie był uruchomiony.

Weryfikacja 1.3.0 (15 października 2026, gałąź `release/1.0`): `npm run build`, 101 testów Playwright (w tym zrzuty wzorcowe), `cargo fmt --check`, 79 testów Rust (`cargo test --locked`) i `npm run tauri build -- -- --locked` przechodzą. Workflow CI z krokiem zrzutów nie był jeszcze uruchomiony na windows-latest.

Weryfikacja 1.0.0 (14 października 2026, gałąź `release/1.0`): `npm run build`, 55 testów Playwright i 63 testy Rust (`cargo test --locked`) przechodzą. Testy Playwright działają na podglądzie z localStorage (`src/store.ts`, `src/settings.ts`), bez backendu Rust; widget renderują pod adresem `#widget`. Testy Rust obejmują m.in. podział przedziału, `stats::day_total_ms` z dobą zmiany czasu, tooltip traya, wybór zadania dla Start/Stop, walidację ustawień, argumenty autostartu, logikę widoczności widgetu, reguły i deduplikację powiadomień z kontrolowanym zegarem oraz `plan_task`/`reorder` bez nadpisywania innych pól.

`cargo fmt --check` i `npm run tauri build -- -- --locked` przechodzą; instalator: `src-tauri/target/release/bundle/nsis/Todo Track_1.0.0_x64-setup.exe`.

Walidacja natywna (etap 5 pkt 4) na osobnej kompilacji `pl.todotrack.validation.v6`, bez instalacji NSIS, przez CDP WebView2 i prawdziwe wejście Windows (SendInput). Sprawdzono: globalne skróty Start/Stop i szybkiego przełączania po zmianie kombinacji oraz widoczny błąd zajętego skrótu, podział aktywnego pomiaru z zapisem do SQLite, widget (klik myszą bez kradzieży fokusu, przeciąganie, tryb „tylko w tle”, X i przywrócenie), pozycję, rozmiar i maksymalizację okien po restarcie (także po starcie z `--hidden`), wpis autostartu z `--hidden` i start tą komendą, reguły powiadomień (długi pomiar, termin przy starcie, brak powtórek po restarcie) w `notifications_sent`, `--prepare-update`, otwarcie bazy 0.3.0 z aktywnym timerem (kopia `before-1.0.0-*.sqlite`, timer trwa), tooltip traya po Start i Stop oraz tablicę tygodnia (menu „Przenieś na…” i Alt+↑ z zapisem do SQLite). Walidacja znalazła i poprawiła trzy błędy: awarię przy starcie (okna tworzone przed rejestracją stanu, teraz `"create": false` i `create_windows` w `setup`), wzrost okien bez ramki o wysokość paska tytułu przy każdym restarcie oraz tray, który po tej zmianie nie odświeżał się wcale (wątki traya, widgetu, przypomnienia, bezczynności i powiadomień startują teraz po `create_windows`). Powiadomienia Windows nie są zwalidowane: sprawdzono tylko reguły i wpisy w `notifications_sent` (długi pomiar, termin przy starcie, brak powtórek po restarcie); toastów nie było widać, bo niezainstalowany exe nie ma zarejestrowanego AUMID. Niesprawdzone, odstępstwa od etapu 5 pkt 4: instalacja NSIS i aktualizacja z działającego 0.3.0 przy aktywnym timerze (blokuje wydanie 1.0.0), toasty Windows, autostart przy prawdziwym logowaniu do Windows. Do ręcznego sprawdzenia zostają też: wygląd ikony traya z kropką i menu, odłączenie monitora, przeciąganie myszą w Zadaniach i na tablicy tygodnia. Raport, skrypty i zrzuty: `src-tauri/target/verification-v6/RAPORT.md`.

Weryfikacja 0.3.0: 18 testów Playwright i 15 testów Rust przechodzi. Osobna natywna instancja `pl.todotrack.validation.v5` potwierdziła zmianę priorytetu bez nadpisania planu, szybkie przełączanie w drugim oknie, rzeczywisty skrót globalny, pojedynczy timer, archiwum/kosz/przywracanie, zamknięcie dnia, ustawienia, minimalizację i X do traya. Eksport i import przez systemowe okna plików przywróciły dokładnie ten sam snapshot zadań/przedziałów; przed przywróceniem utworzono kopię obecnej bazy. Proces testowy zakończono przez `--prepare-update`. Skrypty i dowody: `src-tauri/target/verification-v5/`. Logika progu bezczynności jest sprawdzona testami z kontrolowanym czasem; nie czekano 10 minut na fizyczną bezczynność. Dane właściwej instalacji użytkownika nie były zmieniane. Build produkcyjny Tauri/NSIS 0.3.0 z `--locked` zakończony sukcesem. Poniżej pozostawiono opis wcześniejszych weryfikacji.

```powershell
npm run build
npx playwright install chromium
npm test
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml --check
```

Wszystkie cztery kroki naraz (to samo co CI, bez buildu Tauri): `npm run check`. Skrypt `scripts/check.mjs` zatrzymuje się na pierwszym błędzie i sam dodaje `%USERPROFILE%\.cargo\bin` do PATH, gdy `cargo` nie jest w PATH.

Nowe testy korzystają ze wspólnych danych `tests/fixtures/seed.ts` (tydzień 12–18.10.2026, „dziś” czwartek 15.10 12:00: projekty, szacunki, notatki, zadanie z Jiry, cykl, archiwum i kosz). Funkcje wyłączone w podglądzie (baner bezczynności, przywracanie kopii, obserwacja nowych zgłoszeń z Jiry) testuje się przez zastępcze IPC z `tests/fixtures/tauri.ts`, które odwzorowuje reguły `idle::Clock::observe` i `jira::sync_assigned`; same reguły sprawdzają testy Rust.

`npm test` zawsze uruchamia własny serwer Vite na porcie 1430 (nie używa serwera z `npm run dev` na 1420). Przy kilku równoległych przebiegach ustaw inny port przez zmienną `PLAYWRIGHT_PORT`.

Stan z 0.2.0 (historyczny): 14 testów Playwright sprawdza zadania, priorytety i planowanie, notatki, ręczny czas, motywy, przedziały, raporty i TS, zmianę czasu, szerokość 760 px oraz UI Jiry, własny pasek i akcje przypomnienia. Testy UI Jiry używają zastąpionego interfejsu IPC, bez prawdziwego konta. 11 testów Rust sprawdza SQLite, edycję, nowe pola i ręczny czas, kopię bazy WAL, import/wznawianie z Jiry bez duplikatów, stronicowanie HTTP na lokalnym serwerze testowym, walidację adresów oraz próg 300 sekund, reset i odroczenie przypomnienia.

Weryfikacja 0.2.0 (13 października 2026): wszystkie powyższe testy oraz build Vue/TS i Tauri/NSIS przeszły. Na osobnej instalacji `Todo Track Validation v4` sprawdzono rzeczywisty zapis danych przez WebView2 → Rust → SQLite, schowek TS, instalację oraz aktualizację testowych wersji 0.2.0 → 0.2.1 przy aktywnym timerze w tle. Timer został zamknięty, wszystkie zadania, notatki i przedziały zachowane, aplikacja wystartowała na Zadaniach, powstała kopia `before-0.2.1-…sqlite`. Testową aplikację zakończono i odinstalowano; właściwa instalacja użytkownika nie była zmieniana. Dowody i skrypty: `src-tauri/target/verification-v4/`. Numery 0.2.1 dotyczą wyłącznie testowej kompilacji; wydanie użytkownika ma wersję 0.2.0.

Do ręcznego sprawdzenia w Windows: uruchom pomiar, zminimalizuj okno, przywróć je z traya i sprawdź czas; powtórz z krzyżykiem; zatrzymaj sesję przez tray; uruchom drugą instancję; zakończ aplikację przez menu i uruchom ponownie, sprawdzając zapis.

### Wynik lokalnej weryfikacji

Weryfikacja na Windows, 12 października 2026:

- `npm ci`, `npm run build` i 9 testów Playwright — sukces. Testy uruchomiono tymczasowo na Node 22; systemowy Node nadal ma wersję 18.17.0 (wpis historyczny; obecnie projekt używa Node 24 LTS z `.nvmrc`).
- `cargo check --locked`, `cargo test --locked` (9 testów) i `cargo fmt --check`, z manifestem `src-tauri/Cargo.toml` — sukces. MSVC Build Tools i WebView2 są dostępne.
- `npm run tauri dev` — aplikacja uruchamia się z natywnym backendem SQLite.
- Testy natywne na osobnej bazie `pl.todotrack.validation`: dodawanie, edycja, anulowanie i potwierdzenie usunięcia wraz z historią, przełączanie pomiaru, ukończenie aktywnego zadania i przywracanie — sukces.
- Po dodaniu raportów i edycji przedziałów: natywny test WebView2 → Rust → SQLite na osobnej bazie `pl.todotrack.validation.v2` potwierdził zapis nowych godzin z formularza, odrzucenie nakładania pomiarów, przeliczenie raportu i trwałość zmian po restarcie procesu. Obejrzano zrzuty ciemnego i jasnego widoku, raportu i okna 760 px.
- Na bazie `pl.todotrack.validation.v3` sprawdzono natywne kontrolki paska, przeciąganie okna, formularz konfiguracji Jiry, ukrywanie przypomnienia podczas pomiaru, pokazanie go w tle, odroczenie i powrót do pola zadania. W teście debug skrócono opóźnienie do 4 sekund przez `TODO_TRACK_REMINDER_SECONDS`; kompilacja release ignoruje tę zmienną i używa 300 sekund. Zrzuty i skrypt: `src-tauri/target/verification-v3/`.
- Minimalizacja, żądanie zamknięcia i przycisk „Schowaj do traya” chowają okno bez zakończenia procesu ani sesji. Lewy klik ikony przywraca okno; Stop z menu aktualizuje interfejs i historię. Drugi proces przywraca istniejące okno i kończy się. Zakończenie z menu zamyka aktywną sesję i proces; ponowne uruchomienie zachowuje zadanie oraz zapisane czasy.
- `npm run tauri build -- -- --locked` — sukces. Instalator: `src-tauri/target/release/bundle/nsis/Todo Track_0.1.0_x64-setup.exe`. Samodzielny plik aplikacji: `src-tauri/target/release/todo-track.exe`.

Testy GUI i traya wykonano na kompilacji debug z osadzonym frontendem i osobnym identyfikatorem, aby nie naruszać danych użytkownika. Nie zweryfikowano logowania ani pobierania rzeczywistych tablic `asistonwms.atlassian.net` — wymagają e-maila i tokenu wpisanego w aplikacji. Scenariusza instalacji i odinstalowania pakietu NSIS nie wykonywano. Workflow GitHub Actions jest przygotowany, ale nie został uruchomiony z tej sesji.
