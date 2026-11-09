# E8b — test ręczny trybu dynamicznego (wykonuje użytkownik)

Testy automatyczne nie łączą się z prawdziwym Claude. Poniższą listę wykonaj w `npm run tauri dev`
na Claude Code 2.1.319 lub nowszym, w testowym repozytorium Git (czysty stan, np. mały projekt
z jednym plikiem). Zaznacz każdy punkt i dopisz uwagi do `NEXT_SESSION.md`.

- [ ] Definicje → „Dynamiczny (Claude)”: edytor pokazuje formularz, walidacja bez problemów, publikacja działa.
- [ ] Przy dostępie „Tylko odczyt” opcja „Dodaj review i poprawki” jest wyłączona i odznaczona.
- [ ] Formularz startu pokazuje ostrzeżenie o koszcie (tylko dla procesu dynamicznego).
- [ ] Przebieg tylko do odczytu („Opisz strukturę repozytorium”): drzewo pokazuje subagentów na żywo,
  wynik końcowy pochodzi od orkiestratora, repozytorium bez zmian.
- [ ] Przebieg z zapisem i review: subagenci zmieniają pliki tylko w katalogu roboczym przebiegu,
  przed zgodą widać diff, review i ewentualne poprawki działają (do 3 rund).
- [ ] Kilku subagentów naraz (np. „Przeczytaj równolegle trzy pliki, każdy osobnym subagentem”):
  w trakcie ich pracy licznik pokazuje ich jako aktywnych („3 aktywnych / 3 łącznie”), a każdy
  zmienia stan na „zakończony” dopiero po swoim zakończeniu, nie zaraz po uruchomieniu w tle.
- [ ] Drzewo odtwarza się po wyjściu z monitora i ponownym otwarciu przebiegu.
- [ ] „Zatrzymaj” w trakcie: przebieg kończy się jako zatrzymany, w Menedżerze zadań nie zostaje
  żaden `claude.exe`.
- [ ] Polecenie zapisu poza worktree (np. „zapisz plik w katalogu nadrzędnym”) jest odrzucone, plik nie powstaje.
- [ ] Polecenie uruchomienia powłoki (test, `git`) nie jest wykonane (agent nie ma Bash/PowerShell).
- [ ] Zbyt stary Claude (jeśli masz): start zablokowany komunikatem o wersji 2.1.319.
- [ ] Stara historia przebiegów i opublikowane definicje standardowe działają bez zmian (te same wersje
  i skróty); czat AI działa jak dotąd.
- [ ] Zwróć uwagę na koszt tokenów jednego przebiegu i zapisz go w uwagach.
