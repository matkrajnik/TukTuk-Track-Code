# E7: checklista natywna i pilotaż

Dokument dla użytkownika. Agent go nie wykonuje. Zob. też `INSTRUKCJA.md`,
`E4_TEST_RECZNY.md` (prawdziwe CLI) i `E6_TEST_RECZNY.md` (odzyskiwanie).

## 1. Windows, instalator i aktualizacja

- [ ] Czysta instalacja NSIS: aplikacja startuje, bez klucza menu bez Workflows.
- [ ] Aktualizacja przez updater z 2.4.0: zadania i czas zostają, migracja schematu 1–5 na bazie z danymi.
- [ ] Kopia zapasowa z 2.4.0 przywraca się bez Workflows.
- [ ] Instalacja starszej wersji na nowszej bazie: start, decyzja i zapis zwracają „Baza Workflows pochodzi z nowszej wersji aplikacji”, historia i „Zatrzymaj” działają.

## 2. Moduł

- [ ] Odblokowanie kluczem, wyłączenie i ponowne włączenie (wymaga klucza).
- [ ] Wyłączenie przy przebiegu w toku: „Wyłączenie wstrzymane.”; przy czekającym na zgodę i wstrzymanym przechodzi.
- [ ] Po wyłączeniu tablica agentów nadal pokazuje przebieg i pozwala go zatrzymać.
- [ ] Zamknięcie do traya a „Zakończ aplikację” przy kroku w toku: pytanie z nazwą procesu, „Anuluj”/Esc zostawia krok, „Zakończ” kończy (przebieg „Przerwany”); bez kroku bez pytania (E6, sekcja 2).
- [ ] Restart na granicy kroków i w kolejce: przebieg „Wstrzymany”, jedno powiadomienie, „Wznów” uruchamia jeden krok, naruszony worktree daje „Błąd” (E6, sekcja 2a).
- [ ] Przywracanie kopii przy przebiegu „Wstrzymany” jest zablokowane.
- [ ] Claude (2.1.319+) w kroku Implementacja: „Gotowość agentów” pokazuje tryb restricted; krok zapisuje tylko w worktree przebiegu; próba zapisu poza worktree (np. cel „zapisz plik w C:\temp”) jest odrzucona; Claude nie ma Basha (nie uruchamia testów, wypisuje je w checks). Na 2.1.318 krok jest zablokowany z prośbą o aktualizację.
- [ ] Restart w trakcie zgody: zgoda jest ponownie sprawdzana (skrót diffu); bez zmian w katalogu przebieg dalej „Czeka na zgodę”, a przy zmienionym diffie stara zgoda jest unieważniona i pojawia się nowa. Przerwanie i „Odzyskaj przebieg” dotyczą restartu w trakcie kroku agenta.
- [ ] Zabicie procesu aplikacji w Menedżerze zadań w trakcie kroku: po starcie problem w alercie, odzyskanie działa.
- [ ] Blokada aktualizacji (`--prepare-update`) przy przebiegu w toku.

## 3. Pilotaż

| Scenariusz | Wynik | Bloker (tak/nie) | Uwagi |
|------------|-------|------------------|-------|
| Analiza → implementacja → review na małym repozytorium | | | |
| Odrzucenie diffu z komentarzem | | | |
| Zatrzymanie w trakcie kroku | | | |
| Restart i odzyskanie | | | |
| Przywrócenie kopii | | | |
