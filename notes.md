TukTuk Track 2.2.0: tablica sesji agentów — co czeka na Ciebie, co pracuje, co się skończyło.

- Widok „Sesje agentów”: sesje Claude Code i Codexa w trzech kolumnach. „Czeka na Ciebie” z powodem (zgoda na narzędzie, pytanie, plan do akceptacji) i czasem czekania, „Pracuje” oraz „Skończone, nieprzejrzane”. „Przejrzane” zdejmuje zakończoną sesję, a licznik przy pozycji w nawigacji pokazuje, ile czeka i ile się skończyło.
- „Przejdź” przy każdej sesji: VS Code, folder w Eksploratorze albo Orca (dla sesji w worktree Orki wyciąga jej okno na wierzch). Po otwarciu sesja jest przejrzana.
- Czas czekania na Ciebie: ile dziś sesje agentów czekały na Twoją odpowiedź i suma z 7 dni. Dane w nowej tabeli `agent_waits`, tworzonej przy starcie.
- Tray i powiadomienia: drugi wiersz dymka „Agenci: czeka 2 · skończone 1”, pozycja menu „Sesje agentów”. Powiadomienie, gdy sesja czeka (domyślnie włączone) albo skończy pracę (domyślnie wyłączone); Ustawienia → Powiadomienia.
- Pytanie od Claude i plan do akceptacji liczą się jako czekanie, nie praca. Czekanie kończy się samo po 2 godzinach bez zdarzeń.
- Serwer MCP: nowe narzędzie `agent_sessions` (tablica sesji i dzisiejsze czekanie); `day_summary` podaje czas czekania.
- Hook przekazuje teraz także nazwę narzędzia (bez argumentów) i komunikat powiadomienia agenta (do 200 znaków).
