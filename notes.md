TukTuk Track 2.0.0: aplikacja zmienia nazwę z Todo Track na TukTuk Track.

- Nowa nazwa, ten sam program. Instalator 2.0 sam wykrywa Todo Track, pyta o aktualizację i usuwa starą instalację bez kasowania danych. Przy pierwszym starcie zadania, historia i ustawienia są kopiowane do nowego katalogu, a logowania Jira, kalendarza Microsoft, TS i Redmine przenoszą się same. Stary katalog zostaje jako zapas. Rozszerzenie przeglądarki trzeba raz przeładować z folderu `browser-extension` nowej instalacji.
- Pomiary: akcje przy lukach w zapisie, „Przytnij i wznów” po przerwie, poprawa startu trwającego pomiaru. „Rozpocznij dzień”, „Zakończ dzień” i „Wznów dzień”. Plan dnia otwiera się rano przy pierwszym uruchomieniu.
- Wysyłka do TS z nadgodzinami (automatyczny podział ponad normę dnia), edycją godzin, przypisaniem zadania TS w podglądzie i wysyłką wielu dni naraz. Raporty i Dashboard pokazują nadgodziny.
- Ręczne spotkania spoza Outlooka, także bez kalendarza Microsoft. Pola wyboru z wyszukiwaniem i czytelniejsze przyciski.
- Agenci AI wykrywani przez hooki Claude Code i Codexa (Ustawienia → Agenci); w Codexie hooki trzeba raz zatwierdzić poleceniem `/hooks`. „Otwórz worktree w Orce” z kafelka zadania.
- Asystent AI (Ustawienia → Asystent AI): korzysta z subskrypcji Claude Code albo Codexa, bez klucza API; silnik wybierasz przy pierwszym uruchomieniu. Czat w panelu bocznym (Ctrl+J), serwer MCP dla agentów, podpowiedzi przypisań, analiza w Zamknięciu dnia i komentarz w Rano. Model tylko proponuje: nic nie zmienia się bez kliknięcia „Zastosuj”. Dane wychodzące do modelu przechodzą przez filtr nazwisk, który ma ograniczenia opisane w README.
- Szybsza praca aplikacji i drobne poprawki układu, nawigacji i kopii zapasowych.
- Przypomnienia (Ustawienia → Powiadomienia): własne na datę i godzinę, także powtarzalne i przypięte do zadania, oraz przypomnienia przed spotkaniami (domyślnie 5 minut). Toast ma „Odłóż 10 min” i „Pokaż”, nigdy nie startuje pomiaru.
- Opcjonalny malutki mini-widget (Ustawienia → Okna): sam czas, pełny po najechaniu myszą. W Ustawienia → TS stała ramka „Wersja niefinalna”.

Wersja 2.0 nie była jeszcze instalowana ani sprawdzana z prawdziwymi Claude Code, Codexem, Orką, TS, Jirą i Redmine. Lista rzeczy do sprawdzenia jest w `NEXT_SESSION.md`.
