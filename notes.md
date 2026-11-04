TukTuk Track 2.3.1: sesje Codexa na tablicy, okno Orki na wierzchu i aktualizacja bez zamykania agentów.

- Sesje Codexa (CLI i Codex Desktop) trafiają na tablicę: hook Codexa wcześniej się nie uruchamiał, bo Codex wykonuje go przez PowerShell, a ścieżka w cudzysłowie nie była tam programem. Po aktualizacji zatwierdź hooki w Codexie jeszcze raz (/hooks albo ustawienia hooków w Codex Desktop).
- „Orca” przełącza na terminal sesji i wyciąga okno Orki na wierzch, także zminimalizowane albo przykryte innym programem.
- Aktualizacja przechodzi przy otwartych sesjach Claude Code i Codexa oraz otwartej przeglądarce: działające programy pomocnicze (serwer MCP, host rozszerzenia) nie blokują już instalatora.
