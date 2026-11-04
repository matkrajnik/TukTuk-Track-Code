TukTuk Track 2.3.0: tablica sesji wie, gdzie działa agent, i otwiera tam konkretną sesję.

- Przy każdej sesji w „Sesjach agentów” widać, gdzie działa: Orca, Claude Desktop, Codex Desktop, VS Code albo terminal.
- „Przejdź” do konkretnej sesji: Orca przełącza się na terminal tej sesji, Claude Desktop otwiera tę sesję, a Codex Desktop jej wątek. Gdy terminal Orki ma nieaktualny uchwyt (np. po restarcie Orki), wychodzi samo okno Orki.
- Hook przekazuje tylko nazwę źródła i identyfikator sesji w tej aplikacji; aplikacja sprawdza oba, a identyfikator nie trafia do okna ani do serwera MCP.
- Sesje sprzed aktualizacji dostają oznaczenie źródła przy pierwszym nowym zdarzeniu.
