# Todo Track

Lokalna aplikacja desktopowa: lista zadań i prosty tracker czasu. Tauri 2 + Rust + Vue 3 / TypeScript. Interfejs po polsku, dane w SQLite, bez konta i serwera.

## Funkcje

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

**Historia i kopie → Ustawienia** ma sekcje:

- **Czas pracy**: pytanie o czas po co najmniej 10 minutach bezczynności, „Praca agentów AI (Claude Code, Codex) liczy się jako aktywność”, cel dzienny w minutach (0–1440), początek dnia (`HH:MM`, dla osi planu) i dni robocze. Pod polami widać wyliczony cel dnia i tygodnia.
- **Jira**: „Dodawaj nowe zgłoszenia przypisane do mnie” (domyślnie włączone).
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

- **Termin**: przy starcie i o 09:00, gdy są nieukończone zadania z terminem dziś lub wcześniej; treść podaje liczbę i pierwsze trzy tytuły. Kliknięcie: Zadania z filtrem „Dzisiaj: plan i praca” (filtr obejmuje też terminy dziś i zaległe).
- **Długi pomiar**: aktywny przedział trwa co najmniej ustawioną liczbę minut; jedna wiadomość na przedział. Po podziale pomiaru nowa część liczy czas od momentu podziału. Kliknięcie: Zadania z rozwiniętym bieżącym zadaniem. Przyciski „Zatrzymaj” i „Podziel…” (edytor bieżącego pomiaru na zakładce Podziel).
- **Wieczór**: o ustawionej godzinie pomiar nadal trwa. Kliknięcie: Zamknięcie dnia. Przycisk „Zatrzymaj teraz”.
- **Cel**: suma dnia roboczego osiągnęła cel (domyślnie wyłączone). Kliknięcie: Zamknięcie dnia.
- **Nowe zgłoszenia z Jiry**: kliknięcie otwiera Zadania przy grupie „Nierozpoczęte” (jedno zgłoszenie: rozwinięte). Przyciski „Rozpocznij” (tylko przy jednym zgłoszeniu; ta sama ścieżka co w widoku Jira, więc zadanie wraca z kosza lub archiwum) i „Zaplanuj na dziś”.
- **Potwierdzenie skrótu Start/Stop**: kliknięcie pokazuje główne okno.

Każde powiadomienie pojawia się najwyżej raz: klucz trafia do tabeli `notifications_sent` przed wysłaniem, także po ponownym uruchomieniu. Wpisy starsze niż 7 dni są usuwane przy starcie. Przyciski działają, gdy aplikacja jest uruchomiona: „Zatrzymaj” zatrzymuje tylko ten pomiar, którego dotyczy powiadomienie, „Rozpocznij” zatrzymuje poprzedni pomiar; nic poza tym nie przełącza timera. Nieznana akcja (np. stary toast po aktualizacji) tylko pokazuje główne okno. Kliknięcie starego toastu po restarcie uruchamia aplikację. W `tauri dev` powiadomienia mogą nie pojawić się albo podpisywać się inną aplikacją (AUMID PowerShella), a przyciski mogą nie działać; miarodajny jest pakiet NSIS.

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

Wersja w repozytorium: 1.2.0 (odświeżony interfejs: niebieska paleta, ujednolicone formaty czasu i dat, uporządkowana karta zadania i dashboard; bez zmian w bazie). Build NSIS tworzy `src-tauri/target/release/bundle/nsis/Todo Track_1.2.0_x64-setup.exe`; gotowy instalator leży w repozytorium w `releases/`. Instalator 1.2.0 nie był jeszcze instalowany. Od 1.1.0 nowa tabela `jira_issues` (statusy i ostatnie przypisania) powstaje przy starcie (`IF NOT EXISTS`). Otwarte z 1.0: aktualizacja przez instalator z działającego 0.3.0 przy aktywnym timerze (PLAN_1.0.md, etap 5 pkt 4) nie jest sprawdzona i blokuje wydanie 1.0.0.

Przy pierwszym uruchomieniu 1.0.0 na bazie z 0.3.0 powstaje kopia `before-1.0.0-<czas>.sqlite` (opis niżej). Nowe indeksy i tabela `notifications_sent` są tworzone przy starcie (`IF NOT EXISTS`) bez zmiany istniejących danych.

Od 0.2.1 minimalizacja pozostawia okno na pasku zadań; krzyżyk chowa do zasobnika. Zweryfikowano natywnie stan minimized/visible, zamknięcie do traya i przywracanie na osobnej bazie testowej.

Instalator ma polski interfejs, własne grafiki i instaluje dla bieżącego użytkownika. Gdy wykryje istniejącą instalację, wyświetla potwierdzenie wersji i aktualizuje program w dotychczasowym katalogu, bez odinstalowywania. Zachowuje skróty i dane, a po zwykłej aktualizacji uruchamia aplikację ponownie. Nowszej wersji nie zastąpi starszą. Nie ma serwera ani automatycznego pobierania pakietów.

Przy aktualizacji **z wersji starszej niż 0.2.0** zakończ aplikację z menu traya. Od 0.2.0 instalator wywołuje `--prepare-update`: aplikacja zapisuje koniec bieżącej sesji i kończy proces. Jeżeli nadal działa, instalator czeka na ręczne zamknięcie lub pozwala anulować; nie wymusza ubicia procesu w trybie aktualizacji.

Przed pierwszym otwarciem istniejącej bazy przez nową wersję powstaje spójna kopia SQLite (także danych WAL) w `%APPDATA%\pl.todotrack.desktop\backups\before-<wersja>-<czas>.sqlite`. Kopie nie są automatycznie usuwane. Nowe pola są w dodatkowej tabeli `task_details`; istniejące zadania i przedziały pozostają bez zmian. Usuwanie zadania usuwa także jego plan i notatki.

Szablon instalatora pochodzi z [Tauri CLI 2.12.2](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), z lokalnymi zmianami w `src-tauri/installer/template.nsi`. Przy zmianie wersji Tauri należy porównać szablon z upstreamem. Grafiki można odtworzyć skryptem `src-tauri/installer/make-artwork.ps1`; tłumaczenia są w `Polish.nsh`.

## Jira

Otwórz zakładkę **Jira** w aplikacji desktopowej. Adres `https://asistonwms.atlassian.net` jest wstępnie uzupełniony. Podaj e-mail konta Atlassian i token API **bez zakresów** (klasyczny), a następnie wybierz „Połącz i sprawdź”. Link „Utwórz token Atlassian” otwiera stronę zarządzania tokenami w przeglądarce. Token jest przechowywany w magazynie poświadczeń Windows; nie trafia do SQLite ani localStorage. „Rozłącz konto” usuwa zapisany token.

Wybierz tablice (maksymalnie 20) i zapisz wybór. Aplikacja pobiera wszystkie strony wyników dla `assignee = currentUser()`, łączy powtarzające się zgłoszenia i pokazuje informację, jeśli którejś tablicy nie udało się pobrać. Domyślnie widoczne są tylko otwarte zgłoszenia; filtr można wyłączyć. Wyszukiwanie działa po kluczu lub nazwie w pobranych wynikach. Odświeżenie następuje przy wejściu w zakładkę, zmianie tablic/filtra oraz po kliknięciu „Odśwież”.

„Rozpocznij” tworzy lokalne zadanie powiązane z kluczem Jiry albo wykorzystuje istniejące, przywraca je do zrobienia i uruchamia timer. Poprzedni pomiar zostaje zatrzymany. Link pozostaje dostępny również w rozwiniętym kafelku lokalnego zadania. Co 5 minut aplikacja dodaje też nowe zgłoszenia przypisane do Ciebie do „Do zaplanowania” i odświeża statusy na kafelkach (zob. „Nowości 1.1”; zapytanie `/rest/api/3/search/jql` z `assignee = currentUser() AND (statusCategory != Done OR updated >= -14d)`). Integracja odczytuje Jirę; nie zmienia zgłoszeń i nie wysyła worklogów. Działa z Jira Cloud, nie z Server/Data Center.

Dokumentacja API: [tablice i stronicowanie Jira Cloud](https://developer.atlassian.com/cloud/jira/software/rest/api-group-board/), [logowanie e-mailem i tokenem](https://developer.atlassian.com/cloud/jira/platform/basic-auth-for-rest-apis/).

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
