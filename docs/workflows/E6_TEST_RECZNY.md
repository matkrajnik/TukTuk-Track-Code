# E6: checklista testu ręcznego (recovery, backup, aktualizacja)

Dokument dla użytkownika. Testy automatyczne używają atrapy procesów (`processes::fake`) i
`ScriptedRunner`; poniższe kroki sprawdzają zachowanie na prawdziwym Windows i prawdziwym CLI.
Przygotuj repozytorium testowe jak w `E4_TEST_RECZNY.md` (sekcja 1).

## 1. Awaria hosta w trakcie kroku

1. Uruchom przebieg; gdy krok agenta jest „W toku”, zakończ `tuktuk-track.exe` w Menedżerze zadań.
2. Sprawdź w Menedżerze zadań, że proces agenta (i jego potomkowie) też zniknęli (Job Object).
3. Uruchom aplikację. Przebieg ma stan „Przerwany”, krok „Przerwany”; agent nie startuje ponownie.
4. W monitorze kliknij „Odzyskaj przebieg”: komunikat o zwolnieniu katalogu roboczego, na osi
   zdarzenie `recovery`. Drugi przebieg w tym samym repozytorium startuje.

## 2. Zamknięcie okna a wyjście

1. W trakcie kroku zamknij okno (X): aplikacja chowa się do traya, krok trwa dalej.
2. Wybierz „Zakończ aplikację” z traya: pojawia się główne okno z pytaniem „Zakończyć aplikację?”
   z nazwą procesu. „Anuluj” (i Esc) zostawia aplikację i krok bez zmian.
3. Ponownie „Zakończ aplikację” → „Zakończ”: krok jest anulowany, aplikacja kończy się w ciągu ok. 8 s.
4. Po ponownym uruchomieniu przebieg jest „Przerwany” i nie startuje ponownie.
5. Bez kroku w toku (np. przebieg czeka na zgodę albo jest wstrzymany) „Zakończ aplikację” kończy
   od razu, bez pytania. Pytanie pada też po wyłączeniu modułu (bez klucza), gdy krok działa.

## 2a. Restart na granicy kroków i w kolejce

1. Zamknij aplikację („Zakończ” bez kroku w toku albo przez Menedżera zadań), gdy przebieg jest
   między krokami (np. tuż po wyniku analizy) albo czeka w kolejce za innym przebiegiem.
2. Po starcie przebieg ma stan „Wstrzymany”, żaden agent nie startuje; pojawia się powiadomienie
   „Przebieg wstrzymany po restarcie — wznów go w monitorze.” (jedno na przebieg).
3. W monitorze kliknij „Wznów” (także dwukrotnie szybko): startuje dokładnie jeden następny krok.
4. Powtórz z podmienionym plikiem `.git` w worktree przebiegu przed „Wznów”: przebieg kończy się
   „Błąd” (katalog roboczy niedostępny), agent nie startuje.
5. „Zatrzymaj” na wstrzymanym przebiegu kończy go jako „Zatrzymany” (także bez klucza).

## 3. Aktualizacja i przywracanie kopii

1. Przy przebiegu „W toku” kliknij instalację aktualizacji: baner pokazuje „Aktualizacja wstrzymana”.
2. Przy przebiegu „Czeka na zgodę” aktualizacja przechodzi.
3. Przy przebiegu „W toku”, „Czeka na zgodę” albo „Wstrzymany” przywracanie kopii pokazuje „Nie można
   teraz przywrócić kopii”, a podgląd kopii ostrzega o tym przed potwierdzeniem. Przy „Wstrzymany”
   aktualizacja i wyłączenie modułu przechodzą.
4. Wyeksportuj kopię z przebiegiem czekającym na zgodę, odrzuć tę zgodę w aplikacji, usuń katalog
   `workflows\runs` z danych aplikacji i przywróć kopię: przebieg z kopii jest „Przerwany”, a plik
   diffu wraca na dysk.
5. Usuń jeden plik `diff-*.patch` przed eksportem: komunikat eksportu podaje liczbę pominiętych diffów.
6. Gdy instalacja aktualizacji się nie uda (np. przerwane pobieranie), uruchom nowy przebieg bez
   restartu aplikacji: przebieg startuje (sterownik nie został zatrzymany).
7. Przy procesie agenta, którego nie da się zakończyć, „Odzyskaj przebieg” pokazuje alert z PID.

## 4. Retencja i miejsce na dysku

1. Ustaw w Ustawieniach → Workflows retencję 7 dni (najmniej), przestaw zegar o 8 dni i uruchom aplikację:
   diff zakończonego przebiegu pokazuje komunikat o okresie przechowywania.
2. Na dysku z mniej niż 512 MiB wolnego start przebiegu pokazuje „Za mało miejsca na dysku”.

## Wynik

Do wykonania przez użytkownika; do tego czasu zachowanie na prawdziwym Windows i CLI jest
niezweryfikowane.

Zob. też `INSTRUKCJA.md` (użytkowanie i rozwiązywanie problemów) oraz `E7_TEST_RECZNY.md` (checklista natywna i pilotaż).
