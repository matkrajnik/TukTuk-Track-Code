# Todo Track

Lokalna aplikacja desktopowa: lista zadań i prosty tracker czasu. Tauri 2 + Rust + Vue 3 / TypeScript. Interfejs po polsku, dane w SQLite, bez konta i serwera.

## Funkcje

### Nowości 1.9

Wydanie 1.9.0 obejmuje zadania R1–R6 z `PLAN_1.9.md`: **Redmine obok Jiry**. Wszystko poniżej sprawdzono testami Rust (fragmenty JSON z `src-tauri/tests/fixtures/redmine_*.json` i lokalny serwer na `127.0.0.1`) oraz Playwright na symulowanym Redmine w podglądzie. Nic nie łączyło się z prawdziwym Redmine ani nie było uruchamiane w natywnej aplikacji (lista do ręcznego sprawdzenia: sekcja „Redmine” niżej i `NEXT_SESSION.md`).

- **Połączenie z własnym Redmine** (self-hosted): Ustawienia → Redmine. Podajesz adres (`http` albo `https`, także z prefiksem ścieżki, np. `https://firma.pl/redmine`; bez wartości domyślnej, przy `http` ostrzeżenie o kluczu wysyłanym bez szyfrowania) i klucz API z „Moje konto” w Redmine. Klucz trafia do Menedżera poświadczeń Windows (usługa `pl.todotrack.desktop.redmine`), nie do SQLite ani localStorage. Są też przyciski „Testuj połączenie” („Połączono jako Imię Nazwisko (login), Redmine odpowiada”) i „Rozłącz”. Klient Redmine ufa **certyfikatom z magazynu Windows**, więc firmowy urząd certyfikacji zainstalowany w systemie wystarcza. W „Zaawansowane” jest wyłączona domyślnie opcja „Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)”, a błąd certyfikatu ma osobny komunikat.
- **Śledzone projekty i widok Redmine**: w Ustawieniach wybierasz śledzone projekty (lista z `/projects.json`, podprojekty z wcięciem, wyszukiwanie, do 50 projektów). Nowa pozycja nawigacji „Redmine” (po „Jira”) pokazuje zgłoszenia wybranego projektu **pogrupowane według statusu**: kolejność z listy statusów Redmine, grupy zwijane, liczba w nagłówku. Zakres „Moje” (domyślnie) albo „Wszystkie otwarte” (do 500 zgłoszeń, z ostrzeżeniem). Do tego „Tylko otwarte”, filtr trackera, wyszukiwanie po numerze i temacie oraz „Odśwież”. Karta pokazuje numer, temat, tracker, podprojekt, status, priorytet, osobę przypisaną, wersję docelową, termin, czas zmierzony lokalnie i znacznik „mierzone teraz”, a do tego przyciski „Rozpocznij” i „Otwórz w Redmine”. Na dole jest sekcja „Przypisane do mnie spoza śledzonych projektów”. Bez śledzonych projektów widok pokazuje moje zgłoszenia ze wszystkich projektów, a bez konta link do Ustawień.
- **Import nowych przypisań**: co 5 minut aplikacja sprawdza zgłoszenia przypisane do Ciebie we wszystkich projektach (otwarte i zamknięte w ostatnich 14 dniach) oraz zgłoszenia otwartych zadań przekazane innym osobom. Reguły są takie same jak w Jirze:
  - pierwsze sprawdzenie dla konta tylko zapamiętuje obecne przypisania;
  - każde nowe przypisanie staje się zadaniem w „Do zaplanowania” i daje powiadomienie z „Rozpocznij” (przy jednym zgłoszeniu) i „Zaplanuj na dziś”;
  - zgłoszenie powiązane już z zadaniem (także w koszu i archiwum) nie jest dublowane.

  Statusy, tracker, termin i priorytet na kafelkach odświeżają się przy tym samym sprawdzeniu. Wyłącznik: „Dodawaj nowe zgłoszenia przypisane do mnie”.
- **Zgłoszenie na zadaniu**: kafelek, karta tygodnia, szybkie przełączanie, widget, Rano, raport miesiąca i tablica tygodnia pokazują `#1234` ze statusem (wspólna funkcja `issueRef` w `src/issue.ts`). Kafelek ma też chipy trackera i terminu oraz „Otwórz w Redmine”. Priorytet z Redmine (także „Immediate” / „Natychmiastowy”) ustawia pasmo priorytetu zadania jak w Jirze. „Czekam na” proponuje się dla statusów z listy „Statusy oznaczające „Czekam na”” (domyślnie „Feedback”). Eksport CSV i XLSX ma nową ostatnią kolumnę „Redmine”, a kolumna „Klucz Jiry” się nie zmieniła. Menu traya pokazuje przypięte zadanie jako `#1234 · tytuł`.
- **Szybkie dodawanie `#1234`**: przy połączonym Redmine sam `#` z cyframi oznacza zgłoszenie Redmine (chip „Redmine: #1234”). `#nazwa` z literami to nadal projekt, a `\#1234` to projekt „1234”. Bez tytułu temat pochodzi z listy pobranej w widoku Redmine; bez niej pojawia się błąd „Nie znam tematu #1234…”. **Jedno zadanie ma co najwyżej jedno zgłoszenie**: klucz Jiry i `#1234` w jednej linii dają błąd „Zadanie może mieć jedno zgłoszenie: Jira albo Redmine.”, a numer z istniejącym zadaniem daje „#1234 ma już zadanie „…””.
- **Pytanie o przesunięcie przy starcie**: start pomiaru zadania ze zgłoszeniem w statusie z listy „Statusy początkowe” (domyślnie „New”, „Nowy”) pyta „Przesunąć #1234 do „In Progress”?”. Pytanie pojawia się w tym samym banerze, widgecie i powiadomieniu co dla Jiry, z „Przesuń”, „Nie teraz” i „Nie pytaj dla tego zadania”. Cel to status o nazwie z „Docelowy status przy starcie pracy” wśród statusów dozwolonych dla zgłoszenia; bez dopasowania baner pokazuje listę. Pomiar startuje od razu.
- **Zapisy do Redmine**: rozwinięty kafelek ma „Przesuń w Redmine…”. Ukończenie zadania proponuje „Ukończone. Przesunąć #1234 dalej?”. Kafelek i karta w widoku mają też „Przypisz do mnie”, „Dodaj komentarz” (zwykły tekst, do 4000 znaków) i „Kopiuj nazwę gałęzi” (`feature/1234-krotki-tytul`, prefiks z osobnego ustawienia). Karta w widoku Redmine ma „Przesuń do…”: po zmianie statusu przechodzi do innej grupy, a przy błędzie wraca z komunikatem. Przeciągania nie ma, bo Redmine nie ma tablic. Po zapisie aplikacja odczytuje status ponownie; gdy Redmine go nie zmienił, pokazuje „Redmine nie zmienił statusu”.
- **Nic nie zapisuje się bez kliknięcia**: każdy zapis wymaga kliknięcia, a przełącznik „Pozwól aplikacji zmieniać zgłoszenia w Redmine” blokuje wszystkie zapisy także w backendzie.
- **Jira i Redmine są niezależne**: każde ma własne połączenie, poświadczenie, ustawienia, widok, wątek obserwacji i tabele. Można połączyć jedno, oba albo żadne. Wspólna jest tylko mechanika aplikacji (baner pytania o przesunięcie, `issueRef`, formatowanie nazwy gałęzi). Zachowanie Jiry się nie zmieniło.
- **Rozszerzenie przeglądarki** rozpoznaje stronę zgłoszenia `<adres>/issues/1234`, także pod prefiksem ścieżki, i proponuje „Przełącz na #1234”. Adres Redmine z konfiguracji i hosty powiązanych zgłoszeń dochodzą do listy znanych domen tak samo jak hosty z zadań Jiry.
- Nowe tabele `redmine_tasks`, `redmine_issues` i `redmine_statuses` powstają przy starcie przez `CREATE TABLE IF NOT EXISTS`, a tabele Jiry się nie zmieniły. Nowe ustawienia mają wartości domyślne. Konfiguracja leży w `settings` pod kluczem `redmine`. Kopia SQLite obejmuje nowe tabele, ale nie klucz API; przywrócenie kopii zachowuje bieżącą konfigurację Redmine (jak przy Jirze).

### Nowości 1.8.0

- **Wysyłka Raportu TS do Ideo Timesheet**: przycisk „Wyślij do TS” w sekcji „Raport TS” Zamknięcia dnia (obok „Kopiuj raport TS”) i Raportu dnia (obok „Kopiuj raport”) otwiera podgląd dnia. Przycisk widać tylko przy połączonym koncie i włączonym „Wysyłaj raport TS”; bez konta jest link „Połącz konto TS w Ustawieniach”, a dzień z przyszłości blokuje wysyłkę.
- **Skąd godziny i opisy**: wiersze powstają z pomiarów dnia jak w Raporcie TS (`dayRows` i `decimalHours` z `src/report.ts`, funkcja `tsEntries` w `src/timesheet.ts`): godziny dziesiętne z dwoma miejscami i przecinkiem, zaokrąglone per wiersz po scaleniu, bez zaokrąglania do 0,25 h; nadgodziny zawsze „0”, pole `status` zawsze „0”. Opis to notatka dnia zadania, a bez niej tytuł zadania; w podglądzie opis można poprawić przed wysyłką (notatka w Todo Track się nie zmienia). Wiersze z zerowym czasem są pomijane, a trwający pomiar liczy się do teraz (z ostrzeżeniem).
- **Scalanie**: kilka zadań Todo Track przypisanych do tego samego zadania TS daje jeden wpis z sumą czasu i opisami połączonymi „; ” bez powtórzeń. Zadania bez zadania TS nie są wysyłane; podgląd pokazuje ich liczbę i czas oraz link do kafelka.
- **Zadanie TS zadania**: rozwinięty kafelek ma pole „Zadanie TS” (lista z formularza TS na dziś, pobierana raz na czas działania aplikacji, przycisk „Odśwież”), pole „Ustaw jako domyślne dla projektu „X”” i chip „TS: nazwa” na kafelku. Kolejność: jawny wybór w zadaniu, potem domyślne zadanie TS projektu (nowa tabela `ts_project_map`, nazwa projektu bez rozróżniania wielkości liter). Wybór leży w `task_details` (`tsTaskId`, `tsTaskName`), więc istniejące zadania wczytują się bez zmian.
- **„Zachowaj” / „Zastąp”**: zapis w TS zastępuje cały dzień, dlatego podgląd zawsze pokazuje wpisy, które już są w TS na ten dzień. „Zachowaj pozostałe wpisy TS” (domyślnie) dopisuje bez zmian wpisy TS o zadaniach spoza raportu, a wpisy o tych samych zadaniach zastępuje (oznaczone „zostanie zastąpiony”). „Zastąp wszystko” usuwa wszystkie dotychczasowe wpisy dnia, a przycisk zmienia się na „Zastąp i zapisz”.
- **Nic nie wysyła się bez kliknięcia**: zapis wykonuje dopiero „Zapisz w TS” w podglądzie. Zamknięcie dnia niczego nie wysyła, a wysyłka nie zamyka dnia. Po zapisie aplikacja odczytuje dzień z TS i porównuje go z wysłanymi wpisami; różnica w liczbie wpisów albo sumie powyżej 0,01 h daje ostrzeżenie „TS zapisał inne wartości niż wysłane”. Obie sekcje pokazują znacznik „Wysłano do TS dd.MM HH:mm (Y h)” (nowa tabela `ts_sent_days`), a po zmianie pomiarów dnia dopisek „raport zmienił się od wysyłki”.
- **Hasło i sesja**: hasło leży w Menedżerze poświadczeń Windows (usługa `pl.todotrack.desktop.timesheet`), nie w SQLite ani localStorage. Token, refresh token i ciasteczko sesji są tylko w pamięci; po restarcie aplikacja loguje się ponownie hasłem z magazynu przy pierwszym użyciu. Konfiguracja (adres API, login, identyfikator urządzenia) jest w tabeli `settings` pod kluczem `timesheet`.
- **Ustawienia → Timesheet (TS)**: połączenie konta, „Testuj połączenie”, „Rozłącz”, przełącznik „Wysyłaj raport TS” i domyślne zadania TS dla projektów (opis w „Ustawienia” i „Timesheet (Ideo TS)” niżej).
- Nie było jeszcze połączenia z prawdziwym TS: wszystko sprawdzono na symulowanym TS w podglądzie i na zapisanym fragmencie HTML formularza. Pierwszą wysyłkę najlepiej zrobić na dniu testowym. Wysyłka wielu dni naraz nie jest w tym wydaniu.

### Nowości 1.7.4

- **Automatyczne aktualizacje**: przy starcie (i raz dziennie) aplikacja pyta publiczne repozytorium wydań `matkrajnik/todo-track` o `latest.json`. Nowa wersja pokazuje baner z numerem i notatkami z wydania oraz przyciskami „Zainstaluj i uruchom ponownie” (pasek postępu pobierania, podpisany instalator NSIS w trybie pasywnym, restart aplikacji) i „Później”. Brak sieci jest ignorowany bez komunikatu. Ustawienia → Uruchamianie → Aktualizacje: „Sprawdzaj aktualizacje automatycznie” (nowe ustawienie `autoUpdateCheck`, domyślnie włączone) i „Sprawdź teraz” z wynikiem. Wydanie publikuje `npm run release` (`RELEASE.md`). Ścieżka aktualizacji nie była jeszcze sprawdzona natywnie z własnym szablonem NSIS (lista w `RELEASE.md`).

### Nowości 1.7.3

- **Pola Jiry na kafelku**: Ustawienia → Jira → „Pola Jiry na kafelku” pozwala wybrać do 6 pól zgłoszenia (systemowych i niestandardowych, z listy pobieranej z `GET /rest/api/3/field`) i ustalić ich kolejność. Domyślnie etykiety i przypisany. Wartości widać jako chipy w zwiniętym kafelku zadania, na karcie tygodnia i na kartach listy oraz tablicy w widoku Jira (etykiety jako osobne chipy, inne pola jako „Nazwa: wartość”, pełna wartość w dymku). Wartości są zapisywane w tabeli `jira_issue_fields` przy każdym pobraniu listy i tablicy oraz co 5 minut przez obserwację przypisań; po zmianie listy pól odświeżają się od razu (`jira_refresh_fields`). Formatowanie zależy od kształtu wartości: tekst i liczby wprost, daty jako `DD.MM.RRRR`, osoby po nazwie wyświetlanej, opcje, priorytety, wersje, komponenty i sprinty po nazwie, listy po przecinku; treść sformatowana (ADF) nie jest pokazywana.
- **Zgłoszenia przekazane komuś innemu**: obserwacja co 5 minut pobiera też zgłoszenia powiązane z otwartymi zadaniami wprost po kluczach (`POST /rest/api/3/issue/bulkfetch`), więc status i „Przypisany” na kafelku są aktualne także wtedy, gdy zgłoszenie nie jest już przypisane do Ciebie.
- **Priorytet z Jiry na zadaniu**: priorytet zgłoszenia ustawia pasmo priorytetu zadania (Highest, High, Critical, Blocker → wysoki; Medium → normalny; Low, Lowest, Minor, Trivial → niski; także polskie nazwy) przy imporcie, przy „Rozpocznij” z listy i przy każdej zmianie priorytetu w Jirze. Ręczna zmiana pasma zostaje, dopóki Jira nie zmieni priorytetu. Nieznana nazwa priorytetu nie zmienia zadania.

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

Od 1.7 ustawienia są osobną zakładką **Ustawienia** (ostatnia pozycja nawigacji, skrót **Ctrl+,**) z listą sekcji po lewej (przy wąskim oknie lista rozwijana nad treścią), polem „Szukaj ustawienia” i zapamiętaną ostatnią sekcją. Sekcje: Czas pracy, Kalendarz, Timesheet (TS), Jira, Redmine, Powiadomienia, Skupienie, Szablony, Okna, Skróty, Uruchamianie. Najważniejsze z nich:

- **Czas pracy**: pytanie o czas po co najmniej 10 minutach bezczynności, „Praca agentów AI (Claude Code, Codex) liczy się jako aktywność”, cel dzienny w minutach (0–1440), początek dnia (`HH:MM`, dla osi planu) i dni robocze. Pod polami widać wyliczony cel dnia i tygodnia. Od 1.7 także „Planuj z poprawką szacunków”, podpowiedzi z aktywnego okna i rozszerzenie przeglądarki.
- **Kalendarz** (1.7): identyfikator aplikacji (client ID), dzierżawca, połączenie konta Microsoft i „Pokazuj tytuły spotkań”.
- **Timesheet (TS)** (1.8): login i hasło konta Ideo TS („Połącz”; hasło nigdy nie wraca do formularza, po połączeniu pole jest puste z podpisem o magazynie poświadczeń Windows), zwinięte „Zaawansowane” z adresem API (domyślnie `https://api.ts.ideo.pl`, tylko `https` bez ścieżki) i identyfikatorem urządzenia (tylko do odczytu, generowany przy pierwszym połączeniu), „Testuj połączenie” (liczba zadań i projektów z dzisiejszego formularza), „Rozłącz” z potwierdzeniem (usuwa hasło, zostawia login, identyfikator i mapowania), przełącznik „Wysyłaj raport TS” (`ts_send`, domyślnie włączony; wyłączenie chowa przyciski wysyłki i blokuje zapis także w backendzie) oraz „Domyślne zadania TS dla projektów” z dodawaniem i usuwaniem. Pod sekcją krótki opis „Jak to działa”.
- **Jira**: „Dodawaj nowe zgłoszenia przypisane do mnie” (domyślnie włączone); od 1.7 „Pozwól aplikacji zmieniać zgłoszenia w Jirze”, pytanie o przesunięcie przy starcie pracy z nazwą statusu, statusy oznaczające „Czekam na” i prefiks nazwy gałęzi; od 1.7.3 „Pola Jiry na kafelku” (wybór i kolejność pól, „Dodaj pole…”, „Pobierz listę pól z Jiry”).
- **Redmine** (1.9):
  - **Połączenie**: „Adres Redmine” (pełny adres z ewentualnym prefiksem ścieżki, bez wartości domyślnej; przy `http` ostrzeżenie, że klucz API idzie bez szyfrowania) i „Klucz API” (pole hasła; klucz nigdy nie wraca do formularza, po połączeniu pole jest puste z podpisem o magazynie poświadczeń Windows). Link „Gdzie znaleźć klucz API” otwiera `<adres>/my/account`. Zwinięte „Zaawansowane” mają opcję „Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)”, która działa po „Połącz”. Przyciski: „Połącz”, „Testuj połączenie”, „Rozłącz” z potwierdzeniem (usuwa klucz, zostawia adres, projekty i powiązania zadań).
  - **Śledzone projekty**: lista z „Usuń” i „Dodaj projekt…” (podprojekty z wcięciem, wyszukiwanie).
  - **Przełączniki i statusy**:
    - „Pozwól aplikacji zmieniać zgłoszenia w Redmine” (`redmine_write`, domyślnie włączone; blokuje zapisy także w backendzie);
    - „Pytaj o przesunięcie zgłoszenia przy starcie pracy” (`redmine_ask_move`);
    - „Statusy początkowe” (`redmine_new_statuses`, domyślnie „New”, „Nowy”);
    - „Docelowy status przy starcie pracy” (`redmine_move_status`, domyślnie „In Progress”);
    - „Statusy oznaczające „Czekam na”” (`waiting_redmine_statuses`, domyślnie „Feedback”);
    - „Dodawaj nowe zgłoszenia przypisane do mnie” (`redmine_auto_import`);
    - „Prefiks nazwy gałęzi” (`redmine_branch_prefix`, domyślnie `feature/`).

    Przy połączonym koncie „Wstaw statusy z Redmine” pokazuje statusy z serwera do kliknięcia. Wszystko jest niezależne od ustawień Jiry. Pod sekcją krótki opis „Jak to działa”.
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

Wersja w repozytorium: 1.9.0 (Redmine obok Jiry: połączenie kluczem API, widok Redmine ze śledzonymi projektami, import przypisań, zgłoszenie `#1234` na zadaniu, pytanie o przesunięcie i zapisy po kliknięciu, Ustawienia → Redmine; nowe tabele `redmine_tasks`, `redmine_issues` i `redmine_statuses` tworzone przy starcie przez `IF NOT EXISTS`, tabele Jiry bez zmian, nowe ustawienia `redmine_auto_import`, `redmine_write`, `redmine_ask_move`, `redmine_move_status`, `redmine_new_statuses`, `waiting_redmine_statuses` i `redmine_branch_prefix` z wartościami domyślnymi, bez migracji. `reqwest` dostał feature `rustls-tls-native-roots` obok `rustls-tls`, więc cała aplikacja (Jira, Microsoft Graph, TS, Redmine, sprawdzanie aktualizacji) ufa wbudowanej liście urzędów i dodatkowo certyfikatom z magazynu Windows; nic nie ubyło, `Cargo.lock` zaktualizowany, `--locked` przechodzi. Numer wersji w manifestach podbija `npm run ship -- 1.9.0` przy wydaniu), wcześniej 1.8.0 (wysyłka Raportu TS do Ideo Timesheet: podgląd i zapis dnia, pole „Zadanie TS” w zadaniu i domyślne zadania TS projektów, Ustawienia → Timesheet (TS); nowe tabele `ts_project_map` i `ts_sent_days` tworzone przy starcie przez `IF NOT EXISTS`, nowe ustawienie `ts_send` z wartością domyślną, pola `tsTaskId` i `tsTaskName` w `task_details` bez migracji; numer wersji w manifestach podbija `npm run ship -- 1.8.0` przy wydaniu), wcześniej 1.7.4 (automatyczne aktualizacje: plugin updater z publicznego repozytorium wydań `matkrajnik/todo-track`, baner nowej wersji, nowe ustawienie `auto_update_check` z wartością domyślną, komenda `prepare_update`, skrypt `npm run release` i `RELEASE.md`; bez nowych tabel), wcześniej 1.7.3 (pola Jiry na kafelku, odświeżanie zgłoszeń przekazanych innym i priorytet z Jiry, logo kiwi; nowe tabele `jira_fields` i `jira_issue_fields` tworzone przy starcie przez `IF NOT EXISTS`, nowe ustawienie `jira_card_fields` z wartością domyślną), wcześniej 1.7.2 (przebudowa interfejsu według `UX_AUDIT.md`: lewy panel nawigacji, mierzone zadanie w pasku, banery, kontrast i klawiatura; bez nowych tabel i ustawień poza zapamiętanym zwinięciem panelu w pamięci WebView), wcześniej 1.7.1 (przypięte zadania w widgecie i menu traya; bez nowych tabel i ustawień) i 1.7.0 (kalendarz Microsoft i pojemność dnia, zakładka Ustawienia, „Rozłóż”, poranny przegląd, cykle miesięczne i „Czekam na”, czas agentów, przegląd tygodnia, podpowiedzi z okna i z rozszerzenia przeglądarki, przesuwanie i tablica zgłoszeń Jiry; nowe tabele `calendar_events`, `agent_sessions`, `week_notes`, `jira_board_columns`, `jira_statuses` i `jira_board_meta` tworzone przy starcie przez `IF NOT EXISTS`, nowe ustawienia dostają wartości domyślne). Instalator dołącza `todo-track-bridge.exe` i folder `browser-extension`; odinstalowanie usuwa manifest hosta rozszerzenia i oba wpisy w `HKCU`. Build NSIS tworzy `src-tauri/target/release/bundle/nsis/Todo Track_1.7.3_x64-setup.exe`; w repozytorium w `releases/` leży tylko najnowszy instalator (1.7.3), starsze usunięto przy wydaniu 1.7.3. Instalatory 1.2.0–1.7.3 nie były jeszcze instalowane, więc przyciski w powiadomieniach, szybkie dodawanie, szablony, propozycje agentów, eksport, tryb skupienia, kalendarz, rozszerzenie przeglądarki i zapisy do Jiry nie są sprawdzone natywnie. Od 1.6.0 tabele `focus_blocks` i `focus_queue` powstają przy starcie (`IF NOT EXISTS`). Od 1.5.0 tabela `agent_bindings` powstaje przy starcie (`IF NOT EXISTS`). Od 1.4.0 tabela `task_templates` powstaje przy starcie (`IF NOT EXISTS`). Od 1.1.0 nowa tabela `jira_issues` (statusy i ostatnie przypisania) powstaje przy starcie (`IF NOT EXISTS`). Otwarte z 1.0: aktualizacja przez instalator z działającego 0.3.0 przy aktywnym timerze (PLAN_1.0.md, etap 5 pkt 4) nie jest sprawdzona i blokuje wydanie 1.0.0.

Przy pierwszym uruchomieniu 1.0.0 na bazie z 0.3.0 powstaje kopia `before-1.0.0-<czas>.sqlite` (opis niżej). Nowe indeksy i tabela `notifications_sent` są tworzone przy starcie (`IF NOT EXISTS`) bez zmiany istniejących danych.

Od 0.2.1 minimalizacja pozostawia okno na pasku zadań; krzyżyk chowa do zasobnika. Zweryfikowano natywnie stan minimized/visible, zamknięcie do traya i przywracanie na osobnej bazie testowej.

Instalator ma polski interfejs, własne grafiki i instaluje dla bieżącego użytkownika. Gdy wykryje istniejącą instalację, wyświetla potwierdzenie wersji i aktualizuje program w dotychczasowym katalogu, bez odinstalowywania. Zachowuje skróty i dane, a po zwykłej aktualizacji uruchamia aplikację ponownie. Nowszej wersji nie zastąpi starszą.

Od 1.7.4 aplikacja sama sprawdza aktualizacje: wydania leżą w publicznym repozytorium `matkrajnik/todo-track` (instalator, podpis i `latest.json`), a aplikacja pyta o nową wersję przy starcie i raz dziennie, pokazuje baner z notatkami z wydania i po „Zainstaluj i uruchom ponownie” pobiera podpisany instalator oraz uruchamia go w trybie pasywnym. Wyłącznik i „Sprawdź teraz” są w Ustawieniach → Uruchamianie. Publikacja wydania: `npm run ship -- X.Y.Z` (wersja, testy, release w GitHubie, commit, tag i push) albo krok po kroku `npm run release`; opis w `RELEASE.md`.

Przy aktualizacji **z wersji starszej niż 0.2.0** zakończ aplikację z menu traya. Od 0.2.0 instalator wywołuje `--prepare-update`: aplikacja zapisuje koniec bieżącej sesji i kończy proces. Jeżeli nadal działa, instalator czeka na ręczne zamknięcie lub pozwala anulować; nie wymusza ubicia procesu w trybie aktualizacji.

Przed pierwszym otwarciem istniejącej bazy przez nową wersję powstaje spójna kopia SQLite (także danych WAL) w `%APPDATA%\pl.todotrack.desktop\backups\before-<wersja>-<czas>.sqlite`. Kopie nie są automatycznie usuwane. Nowe pola są w dodatkowej tabeli `task_details`; istniejące zadania i przedziały pozostają bez zmian. Usuwanie zadania usuwa także jego plan i notatki.

Szablon instalatora pochodzi z [Tauri CLI 2.12.2](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.2/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi), z lokalnymi zmianami w `src-tauri/installer/template.nsi`. Przy zmianie wersji Tauri należy porównać szablon z upstreamem. Grafiki można odtworzyć skryptem `src-tauri/installer/make-artwork.ps1`; tłumaczenia są w `Polish.nsh`. Logo aplikacji (kiwi) ma źródło w `app-icon.svg`; ikony w `src-tauri/icons/` generuje `npm run tauri icon app-icon.svg`, a ikonę traya z kropką pomiaru (`icon-active.ico`) ten sam `make-artwork.ps1`. Plan i warianty logo: `LOGO_PLAN.md`, `design/logo/`.

## Jira

Otwórz zakładkę **Jira** w aplikacji desktopowej. Adres `https://asistonwms.atlassian.net` jest wstępnie uzupełniony. Podaj e-mail konta Atlassian i token API **bez zakresów** (klasyczny), a następnie wybierz „Połącz i sprawdź”. Link „Utwórz token Atlassian” otwiera stronę zarządzania tokenami w przeglądarce. Token jest przechowywany w magazynie poświadczeń Windows; nie trafia do SQLite ani localStorage. „Rozłącz konto” usuwa zapisany token.

Wybierz tablice (maksymalnie 20) i zapisz wybór. Aplikacja pobiera wszystkie strony wyników dla `assignee = currentUser()`, łączy powtarzające się zgłoszenia i pokazuje informację, jeśli którejś tablicy nie udało się pobrać. Domyślnie widoczne są tylko otwarte zgłoszenia; filtr można wyłączyć. Wyszukiwanie działa po kluczu lub nazwie w pobranych wynikach. Odświeżenie następuje przy wejściu w zakładkę, zmianie tablic/filtra oraz po kliknięciu „Odśwież”.

„Rozpocznij” tworzy lokalne zadanie powiązane z kluczem Jiry albo wykorzystuje istniejące, przywraca je do zrobienia i uruchamia timer. Poprzedni pomiar zostaje zatrzymany. Link pozostaje dostępny również w rozwiniętym kafelku lokalnego zadania. Co 5 minut aplikacja dodaje też nowe zgłoszenia przypisane do Ciebie do „Do zaplanowania” i odświeża statusy na kafelkach (zob. „Nowości 1.1”; zapytanie `/rest/api/3/search/jql` z `assignee = currentUser() AND (statusCategory != Done OR updated >= -14d)`), a od 1.7.3 także status, pola i priorytet zgłoszeń otwartych zadań przekazanych innym osobom (`POST /rest/api/3/issue/bulkfetch` po kluczach). Do 1.6 integracja tylko odczytywała Jirę. Od 1.7 może przesuwać zgłoszenia, przypisywać je, dodawać komentarze i tworzyć nowe (zob. „Nowości 1.7”), zawsze dopiero po kliknięciu; wyłącznik „Pozwól aplikacji zmieniać zgłoszenia w Jirze” w Ustawieniach → Jira blokuje wszystkie zapisy także w backendzie. Worklogi nie są wysyłane. Działa z Jira Cloud, nie z Server/Data Center.

Dokumentacja API: [tablice i stronicowanie Jira Cloud](https://developer.atlassian.com/cloud/jira/software/rest/api-group-board/), [logowanie e-mailem i tokenem](https://developer.atlassian.com/cloud/jira/platform/basic-auth-for-rest-apis/).

## Redmine

Od 1.9 aplikacja współpracuje też z własnym (self-hosted) Redmine ≥ 4.2 z włączonym REST API (Administracja → Ustawienia → API). Redmine jest **niezależny od Jiry**: ma osobne połączenie, poświadczenie, ustawienia, widok, wątek obserwacji i tabele. Można połączyć jedno, oba albo żadne.

**Połączenie**: Ustawienia → Redmine. Wpisz pełny adres, pod którym otwierasz Redmine (np. `https://redmine.firma.pl` albo `https://firma.pl/redmine`; `http` też jest dozwolone, z ostrzeżeniem). Wklej klucz API z Redmine: **Moje konto → Klucz dostępu API → Pokaż** (link „Gdzie znaleźć klucz API” otwiera tę stronę), a potem kliknij „Połącz”. Klucz trafia do Menedżera poświadczeń Windows (usługa `pl.todotrack.desktop.redmine`, użytkownik `<adres>|<login>`). Nie trafia do SQLite, localStorage, logów ani komunikatów błędów. Konfiguracja (adres, login, identyfikator i nazwa użytkownika, śledzone projekty, opcja certyfikatu) jest w tabeli `settings` pod kluczem `redmine`. Zmiana adresu usuwa stare poświadczenie, a „Rozłącz” usuwa klucz i zostawia konfigurację oraz powiązania zadań. Klient ufa certyfikatom z magazynu Windows. Opcja „Akceptuj niezaufany certyfikat (tylko sieć wewnętrzna)” jest tylko dla serwerów, których certyfikatu nie da się zainstalować w systemie. Proxy systemowe działa jak w pozostałych klientach.

**Protokół** (backend `src-tauri/src/redmine.rs`, ścieżki względem adresu; każde żądanie ma nagłówek `X-Redmine-API-Key` i `Accept: application/json`, limit 30 s, bez podążania za przekierowaniami):

- `GET /users/current.json`: połączenie i „Testuj połączenie”. Identyfikator użytkownika zapisany w konfiguracji służy do „Przypisz do mnie”.
- `GET /issues.json`: lista zgłoszeń z `limit=100` i `offset`, najwyżej 5 stron (500 zgłoszeń) z ostrzeżeniem. Warianty:
  - projekt: `project_id=<id>&status_id=open&sort=updated_on:desc`, dla „Moje” z `assigned_to_id=me`;
  - moje ze wszystkich projektów: `assigned_to_id=me`;
  - obserwacja co 5 minut: otwarte oraz `status_id=*` z `updated_on>=` sprzed 14 dni;
  - zgłoszenia zadań przekazane innym: `issue_id=1,2,3&status_id=*`.
- `GET /issue_statuses.json`: lista statusów z flagą `is_closed`. Pamięć podręczna w tabeli `redmine_statuses`, odświeżana raz na dobę i przy „Odśwież”.
- `GET /projects.json`: lista do wyboru śledzonych projektów (do 10 stron), z `parent.id` dla podprojektów.
- `GET /issues/{id}.json?include=allowed_statuses`: dozwolone statusy przed przesunięciem. Bez pola `allowed_statuses` (Redmine < 5.0) aplikacja bierze wszystkie statusy.
- `PUT /issues/{id}.json`: jedyny zapis. Wysyła `{"issue":{"status_id":N}}` (przesunięcie, potem odczyt statusu i komunikat „Redmine nie zmienił statusu”, gdy się nie zmienił), `assigned_to_id` („Przypisz do mnie”) albo `notes` (komentarz).

**Co aplikacja zapisuje**: tylko zmianę statusu, przypisanie do siebie i komentarz, zawsze dopiero po kliknięciu. Przełącznik „Pozwól aplikacji zmieniać zgłoszenia w Redmine” blokuje wszystkie zapisy także w backendzie. Czas pracy nie jest wysyłany do Redmine; wpisy czasu (`time_entries`) są odłożone, a czas idzie do TS. Lokalnie powiązanie zadania ze zgłoszeniem leży w tabeli `redmine_tasks`, a ostatni znany stan zgłoszeń (temat, status, projekt, tracker, priorytet, termin) w `redmine_issues`. **Jedno zadanie ma co najwyżej jedno zgłoszenie**: z Jiry albo z Redmine; backend odrzuca drugie powiązanie. Komunikaty błędów są po polsku: 401 (zły klucz), 403, 404 (zły adres albo wyłączone API), 422 z treścią `errors`, 429, 5xx, przekierowanie, strona HTML zamiast JSON, brak sieci i osobny komunikat dla niezaufanego certyfikatu.

**Różnice wobec Jiry**:
- Redmine nie ma tablic. Zamiast wybranych tablic są **śledzone projekty**, a widok grupuje zgłoszenia projektu według statusu, bez kolumn i przeciągania (status zmienia „Przesuń do…”).
- Statusy nie mają kategorii („do zrobienia”, „w toku”, „zrobione”), tylko flagę „zamknięty”. Dlatego statusy początkowe i docelowy status przy starcie pracy są listami nazw w Ustawieniach. Przy polskich nazwach („Nowy”, „W toku”) trzeba je wpisać albo wstawić przyciskiem „Wstaw statusy z Redmine”.
- Zgłoszenie ma numer (`#1234`, adres `<adres>/issues/1234`), a nie klucz projektu. Dlatego numer nie jest rozpoznawany z nazwy gałęzi, tytułu okna ani sesji agenta; rozpoznaje go tylko rozszerzenie przeglądarki ze strony zgłoszenia.
- Nie ma „Zmień osobę…”, bo wyszukiwarka użytkowników wymaga uprawnień administratora. Nie ma też „Nowe zgłoszenie”, konfigurowalnych pól na kafelku (stały zestaw: tracker i termin), filtra po wersji docelowej, logowania hasłem ani wtyczki Agile.

Nie sprawdzono z prawdziwym Redmine: nic w trakcie prac nie łączyło się z żadnym serwerem Redmine. Testy Rust działają na fragmentach JSON i lokalnym serwerze `127.0.0.1`, a testy Playwright na symulowanym Redmine w localStorage (`todo-track-preview-redmine-v1`). Do sprawdzenia po instalacji:

- czy REST API jest włączone, jaka jest wersja (od 5.0 działa `include=allowed_statuses`) i czy adres z prefiksem ścieżki działa bez przekierowań, także w linkach „Otwórz w Redmine”;
- czy „Testuj połączenie” przechodzi z certyfikatem firmowego urzędu z magazynu Windows bez opcji „Akceptuj niezaufany certyfikat”, i czy przy self-signed certyfikacie pojawia się komunikat o certyfikacie, a nie ogólny błąd sieci;
- czy `assigned_to_id=me`, `status_id=*` z `updated_on` i `issue_id=…` działają w firmowej wersji; czy `/users/current.json` oddaje imię i nazwisko; czy `/projects.json` oddaje `parent.id`;
- rzeczywiste nazwy statusów, reakcja na `PUT` ze statusem spoza przepływu i na wymagane pola, komentarz z polskimi znakami i nowymi liniami, „Przypisz do mnie” z uprawnieniami zwykłego użytkownika;
- pierwsze sprawdzenie bez importu backlogu, a potem powiadomienie i zadanie po nowym przypisaniu;
- podpowiedź rozszerzenia na stronie zgłoszenia;
- zniknięcie klucza z Menedżera poświadczeń po „Rozłącz”.

Pełna lista: `NEXT_SESSION.md` („Stan po 1.9.0”) i `PLAN_1.9.md` („Do ręcznego sprawdzenia z prawdziwym Redmine”). Dokumentacja API: [Redmine REST API](https://www.redmine.org/projects/redmine/wiki/Rest_api).

## Asystent AI — ochrona danych osobowych

Każdy tekst i JSON, który trafia do modelu AI, przechodzi najpierw przez filtr `src-tauri/src/ai_privacy.rs`. Osoby zamieniane są na pseudonimy „Osoba 1”, „Osoba 2”… (ta sama osoba ma ten sam pseudonim w całym wywołaniu; czat i serwer MCP mają jedną wspólną mapę na czas działania aplikacji, więc „Osoba 1” z pytania i z wyniku narzędzia to zawsze ta sama osoba — także po „Nowej rozmowie”). Mapa pseudonimów jest tylko w pamięci i nie trafia do modelu; odpowiedź modelu wraca z prawdziwymi nazwiskami.

- **Pola osobowe** w danych Jiry, Redmine, spotkań i commitów (przypisany, zgłaszający, autor, użytkownik wpisu czasu, obserwujący, uczestnicy, pola Jiry typu osoba, zmiany przypisania w historii) są podmieniane w całości; adresy e-mail, loginy i identyfikatory kont znikają.
- **Zwykły tekst** (tytuły zadań, notatki, wiadomości czatu) jest filtrowany słownikiem: osoby z Jiry i Redmine, własne imię i nazwisko oraz lista „Ukrywaj te nazwy” (`ai_hidden_names`, jedna nazwa w wierszu). Dopasowanie nie rozróżnia wielkości liter ani polskich znaków i rozpoznaje typowe odmiany (Kowalski, Kowalskiego, Kowalską; Anna, Anny, Anią).

Ograniczenia:
- nazwisko spoza słownika wpisane ręcznie w tytuł albo notatkę **przejdzie** do modelu — dopisz je do „Ukrywaj te nazwy”;
- nazwisko będące zwykłym słowem (np. „Marzec”) jest podmieniane wszędzie, także w „raport za marzec”;
- nieregularne odmiany (np. „Marca” od nazwiska „Marzec”) nie są rozpoznawane;
- formy odmiany, które są popularnymi słowami (np. „ale” od imienia „Ala”, „idę” od „Ida”), są pomijane, żeby nie ukrywać zwykłych słów; samo imię i pozostałe formy działają.

## Asystent AI — serwer MCP dla agentów

`todo-track-mcp.exe` (instalowany obok aplikacji) to serwer MCP dla Claude Code i Codexa. Sam nie ma logiki: każde wywołanie narzędzia przekazuje potokiem `\\.\pipe\todo-track-mcp-<użytkownik>` do działającej aplikacji, a wynik przechodzi przez filtr danych osobowych (pseudonimy jak wyżej). Gdy aplikacja nie działa, narzędzie zwraca błąd „Todo Track nie działa”.

**Rejestracja** — przełącznik „Serwer MCP dla agentów” w Ustawieniach → Asystent AI (`ai_mcp`), działa tylko przy wybranym silniku (Claude Code albo Codex). Aplikacja rejestruje serwer przez CLI, bez edycji plików konfiguracji: `claude mcp add --scope user todo-track -- <ścieżka exe>` i `codex mcp add todo-track -- <ścieżka exe>`; po wyłączeniu przełącznika albo wyborze „Nie używaj” wywołuje `… mcp remove todo-track`. Przy starcie (przy wybranym silniku) sprawdza `claude mcp get todo-track` i `codex mcp list` i rejestruje ponownie, gdy ścieżka wskazuje stary exe (np. po aktualizacji). Przy „Nie używaj” start nie uruchamia żadnego procesu CLI. Stan rejestracji i „Zarejestruj ponownie” są w tej samej sekcji Ustawień.

**Narzędzia odczytu** (wykonywane od razu): `tasks_list` (filtry: otwarte, dzisiaj, zablokowane, projekt, szukaj), `task_get` (id, klucz Jiry, `#numer` Redmine albo tytuł), `day_summary`, `week_summary`, `current_measurement`, `ts_tasks` (lista zadań TS z pamięci, bez logowania do TS).

**Narzędzia zapisu tylko proponują**: `propose_task_create` (klucz tylko w postaci Jiry `ABC-12` albo Redmine `#12`), `propose_note_append`, `propose_link_folder` (folder roboczy sesji, która wywołała narzędzie), `propose_switch`, `propose_plan`. Propozycja pojawia się w aplikacji jako baner z „Zastosuj” / „Odrzuć”; bez kliknięcia baza się nie zmienia. Kolejka propozycji jest w pamięci i znika po zamknięciu aplikacji. Nie ma narzędzi do wysyłki TS, zapisów w Jirze ani Redmine, usuwania i zmiany ustawień.

Przykład wpisu do `CLAUDE.md` projektu, dzięki któremu opisy do TS powstają z pracy agenta:

```markdown
Po zakończeniu zadania wywołaj `propose_note_append` z jednym zdaniem podsumowania.
```

## Timesheet (Ideo TS)

Od 1.8 Raport TS dnia można wysłać do Ideo Timesheet (domyślnie `https://api.ts.ideo.pl`). Konto łączy się w Ustawieniach → Timesheet (TS): login i hasło konta TS, „Połącz”. Hasło trafia do Menedżera poświadczeń Windows (usługa `pl.todotrack.desktop.timesheet`, użytkownik `<adres API>|<login>`), konfiguracja (adres API, login, identyfikator urządzenia, nazwa urządzenia „Todo Track”) do tabeli `settings` pod kluczem `timesheet`. Token, refresh token i ciasteczko sesji są tylko w pamięci procesu; nie trafiają do SQLite, localStorage, logów ani komunikatów błędów. Kopia SQLite obejmuje tabele `ts_project_map` i `ts_sent_days`, hasła nie.

Backend (`src-tauri/src/timesheet.rs`) to port konektora Node `ideo-ts-api/connector` na Rust (`reqwest` z `multipart`, parser HTML `scraper`); konektor nie jest zależnością aplikacji. Protokół, ścieżki względem adresu API:

- **Logowanie**: `POST /api/login` z JSON `{login, password, deviceId, deviceName}` → `{token, refresh, expires}`. `deviceId` (32 znaki szesnastkowe) powstaje przy pierwszym połączeniu i zostaje w konfiguracji, żeby TS widział jedno urządzenie.
- **Identity**: `GET /api/user/identity` z `Authorization: Bearer <token>`; pierwszy nagłówek `Set-Cookie` (sam fragment `nazwa=wartość`) to ciasteczko stron timesheetu. Każde żądanie do `/timesheet/*` idzie z oboma nagłówkami: `Authorization` i `Cookie`.
- **Refresh**: ważność tokenu z `exp` w JWT, inaczej z `expires` (ms, sekundy epoki albo sekundy od teraz), inaczej 30 min. Gdy zostaje mniej niż 60 s, aplikacja wywołuje `GET /api/token/refresh?id=api-ts&name=api-ts&refresh=…` i ponownie identity po nowe ciasteczko; odrzucone odświeżenie oznacza pełne logowanie hasłem z magazynu. Odpowiedź 401 na żądanie timesheetu daje jedno odświeżenie i powtórkę.
- **Formularz dnia**: `GET /timesheet/wypelnij?dzien=RRRR-MM-DD` → HTML. Z pierwszego `select` w `.jq-wiersz-ts .jq-zadanie` aplikacja czyta listę zadań (`optgroup` = projekt TS, `option` = zadanie), a z wierszy `tr.jq-wiersz-ts` wpisy już zapisane (zadanie, godziny, nadgodziny, opis; wartości jako tekst, bez przeliczania). Strona bez formularza (np. strona logowania) daje błąd „Nie rozpoznano formularza timesheetu…”.
- **Zapis (`wypelnij`, multipart)**: `POST /timesheet/wypelnij?dzien=RRRR-MM-DD` jako `multipart/form-data` z polami `akcja=zapisz`, `dzien`, `status=0`, `submit=Zapisz Timesheet` oraz `zadanie[i]`, `zadanie_godziny[i]`, `zadanie_nadgodziny[i]`, `zadanie_opis[i]` dla każdego wpisu. Klient nie podąża za przekierowaniami, więc 2xx albo 3xx liczy się jako przyjęcie; potwierdzeniem jest ponowny odczyt formularza zaraz po zapisie.

**Zapis zastępuje cały dzień**: wpisy dnia pominięte w POST znikają z TS. Dlatego podgląd wysyłki zawsze pokazuje wpisy już zapisane w TS, a domyślny tryb „Zachowaj pozostałe wpisy TS” (`keep`) sam pobiera je w backendzie tuż przed zapisem i dokleja bez zmian te, których zadań nie ma w raporcie. „Zastąp wszystko” (`replace`) wysyła tylko wpisy z raportu. Backend sprawdza wpisy przed wysyłką (1–50 wpisów, id zadania do 20 znaków, godziny w formacie `1`, `1,5` albo `1.25`, opis do 2000 znaków bez znaków sterujących), a przełącznik „Wysyłaj raport TS” blokuje zapis także w backendzie. Błędy są po polsku (401, 403, 404, 429, 5xx, brak sieci albo limit 30 s).

Nie sprawdzono z prawdziwym TS (nic w trakcie prac nie łączyło się z serwerem; testy Rust działają na zanonimizowanym pliku `src-tauri/tests/fixtures/ts_wypelnij.html` i lokalnym serwerze na `127.0.0.1`, testy Playwright na symulowanym TS w localStorage):

- czy serwer przyjmuje godziny z przecinkiem, dwa miejsca po przecinku (bez kroku 0,25 h) i wpis „0,00”, oraz co oddaje po zapisie;
- znaczenie pola `status=0` i zachowanie dla dnia zatwierdzonego przez przełożonego;
- czy zapis bez części wpisów faktycznie je usuwa, czy odpowiedź na POST to przekierowanie, i czy błąd walidacji wraca jako strona 200;
- czy lista zadań różni się między dniami i czy identyfikatory zadań są stałe;
- logowanie z nowym `deviceId`, ponowne logowanie po restarcie, wystarczalność ciasteczka z identity, rzeczywisty czas życia tokenu i pierwsza wysyłka po dłuższej przerwie;
- usunięcie hasła z Menedżera poświadczeń po „Rozłącz”.

Pełna lista: `NEXT_SESSION.md` („Stan po 1.8.0”) i `PLAN_1.8.md` („Do ręcznego sprawdzenia z prawdziwym TS”). Pierwszą wysyłkę najlepiej zrobić na dniu testowym.

## Rozszerzenie przeglądarki (Chrome i Edge)

Rozszerzenie z folderu `browser-extension/` podpowiada zadanie ze zgłoszenia Jiry otwartego w aktywnej karcie, także gdy tytuł okna nie ma klucza (np. tablica z `?selectedIssue=WMS-123`): „Pracujesz teraz przy WMS-123? — Przełącz”. Od 1.9 rozpoznaje też stronę zgłoszenia Redmine (`<adres Redmine>/issues/1234`, także pod prefiksem ścieżki) jako `#1234`; adres Redmine z konfiguracji dochodzi do znanych domen jak hosty zadań Jiry. Timer nigdy nie przełącza się sam. Szczegóły: [browser-extension/README.md](browser-extension/README.md).

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

Weryfikacja 1.9.0 (20 października 2026, gałąź `release-1-9`, przed scaleniem do `master`):

- `npm run build`: przechodzi.
- Playwright: 366 testów z zrzutami wzorcowymi przechodzi (było 296 w 1.8.0). Nowe testy:
  - `tests/redmine.spec.ts` (44 scenariusze): widok, grupy statusów, „Moje” / „Wszystkie otwarte”, filtr trackera, wyszukiwanie, „Rozpocznij”, sekcja spoza śledzonych projektów, chipy na kafelkach, szybkie dodawanie `#1234`, pytanie o przesunięcie, „Przesuń w Redmine…”, propozycja po ukończeniu, „Przypisz do mnie”, komentarz, nazwa gałęzi, przełącznik zapisów, Ustawienia → Redmine i pełna ścieżka na zastępczym IPC;
  - `tests/redmine-store.spec.ts` (7 testów symulowanego Redmine);
  - `tests/issue.spec.ts` (5 testów `issueRef`);
  - nowe przypadki w `tests/quickadd.spec.ts` i `tests/browser-extension.spec.ts`.

  `tests/export.spec.ts` świadomie dostał kolumnę „Redmine”, a `tests/settings-tab.spec.ts` nową sekcję. Testy Jiry są bez zmian. Zrzuty wzorcowe zregenerowano świadomie: nowa pozycja „Redmine” w nawigacji zmieniła wszystkie widoki z panelem; doszły `redmine-*` i `ustawienia-redmine-*`.
- `cargo fmt --check` i 318 testów Rust (`cargo test --locked`): przechodzą (było 268 w 1.8.0). Nowe testy obejmują:
  - parsowanie fixture zgłoszeń, projektów, statusów i użytkownika, stronicowanie i limit 5 stron;
  - `normalize_site` z prefiksem ścieżki, `allowed_link`, `status_error` bez sekretów;
  - `redmine_start` (nowe zadanie, kosz, archiwum), `sync_assigned`, zapytania obserwacji, grupowanie według statusu;
  - wybór statusu docelowego, `allowed_statuses`, ciała `PUT`, blokadę `redmine_write`, nazwę gałęzi;
  - `quick_add` z `#1234`, `snapshot`, `clear_waiting`, eksport z kolumną, ustawienia Redmine i rozpoznanie strony zgłoszenia.
- `npm run tauri build` i instalator: wykonuje `npm run ship -- 1.9.0` przy wydaniu (wymaga klucza podpisu).
- Nie sprawdzono: połączenie z prawdziwym Redmine (żadne żądanie do żadnego serwera Redmine ani Jiry) ani natywna aplikacja; lista w „Redmine” i `NEXT_SESSION.md`.

Weryfikacja 1.8.0 (20 października 2026, gałąź `release-1-8`, przed scaleniem do `master`):

- `npm run build`: przechodzi.
- Playwright: 296 testów z zrzutami wzorcowymi przechodzi (nowe `tests/timesheet.spec.ts` — 22 scenariusze: Ustawienia → Timesheet, pole „Zadanie TS” i domyślne zadania projektów, podgląd i zapis z Zamknięcia dnia i Raportu dnia, „Zachowaj” / „Zastąp”, znacznik wysyłki, przełącznik, odświeżanie z drugiego okna — oraz `tests/timesheet-store.spec.ts` — 5 testów symulowanego TS). Zrzuty wzorcowe Ustawień i Zamknięcia dnia zregenerowano świadomie, doszły zrzuty sekcji Timesheet.
- `cargo fmt --check` i 268 testów Rust (`cargo test --locked`): przechodzą (nowe: parser formularza, multipart, ważność tokenu, ciasteczko, walidacja wpisów, scalanie `keep`, komunikaty bez sekretów, `ts_project_map`, `ts_sent_days`, `ts_send` w ustawieniach, sesja na lokalnym serwerze testowym).
- `npm run tauri build` i instalator: wykonuje `npm run ship -- 1.8.0` przy wydaniu (wymaga klucza podpisu).
- Nie sprawdzono: połączenie z prawdziwym TS (żadne żądanie do `api.ts.ideo.pl`) ani natywna aplikacja; lista w „Timesheet (Ideo TS)” i `NEXT_SESSION.md`.

Weryfikacja 1.7.4 (19 października 2026, gałąź `claude/upbeat-allen-2f6245`):

- `npm run build`: przechodzi.
- Playwright: 231 testów bez zrzutów wzorcowych przechodzi (nowy `tests/updater.spec.ts`: baner aktualizacji, instalacja z postępem i restartem, brak nowszej wersji, błąd sieci, wyłączone sprawdzanie). Zrzuty wzorcowe nie były uruchamiane; sekcja Ustawienia → Uruchamianie ma nowe pola, więc jej wzorce wymagają odświeżenia.
- `cargo fmt --check` i 244 testy Rust (`cargo test --locked`): przechodzą.
- `npm run tauri build -- -- --locked` uruchamia dopiero `npm run release` (wymaga klucza podpisu).
- Nie sprawdzono natywnie: cała ścieżka aktualizacji z własnym szablonem NSIS w trybie pasywnym (lista w `RELEASE.md`); 1.7.4 to pierwsza wersja z updaterem, więc trzeba ją zainstalować ręcznie.

Weryfikacja 1.7.3 (19 października 2026, gałąź `master`):

- `npm run build`: przechodzi.
- Playwright: 260 testów z zrzutami wzorcowymi przechodzi (nowy `tests/jira-fields.spec.ts`; wzorce bez zmian).
- `cargo fmt --check` i 244 testy Rust (`cargo test --locked`): przechodzą.
- `npm run tauri build -- -- --locked`: przechodzi.
- Nie sprawdzono z prawdziwą Jirą: lista pól (`GET /rest/api/3/field`), `POST /rest/api/3/issue/bulkfetch`, wartości pól typu sprint, opcja, użytkownik i data; nowe logo nie było oglądane w działającej aplikacji ani w instalatorze.

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
