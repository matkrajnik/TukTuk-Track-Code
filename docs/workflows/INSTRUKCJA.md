# Workflows: instrukcja (funkcja eksperymentalna)

Workflows to moduł TukTuk Track, który prowadzi agentów (Claude Code i Codex) przez
proces: analiza, implementacja i review zadania w osobnym katalogu roboczym, z Twoją
decyzją na końcu. Bez klucza dostępu nic w aplikacji się nie zmienia. Klucz wydaje
właściciel aplikacji; nie znajduje się w żadnym pliku repozytorium ani dokumentacji.

## Wymagania

- Git, a repozytorium docelowe bez podmodułów.
- Claude Code 2.1.318 i Codex 0.163.0 (szablon używa obu: Claude analizuje i robi
  review, Codex implementuje); nowsze wersje z tej samej serii działają z
  ostrzeżeniem „niezweryfikowana wersja”.
- Codex: w `config.toml` ustawienie `[windows] sandbox = "elevated"`.

## Krok po kroku

1. **Odblokowanie.** Ustawienia → Workflows → „Klucz dostępu” → „Odblokuj Workflows”. W menu
   pojawia się grupa Workflows.
2. **Definicja.** Definicje → „Z szablonu” (Analiza → implementacja → review) albo pusty
   proces. Edytor pokazuje walidację.
3. **Publikacja.** Opublikowana wersja jest niezmienna; uruchomienie używa wersji, nie szkicu.
4. **Gotowość.** Formularz startu sprawdza agentów (CLI, wersja, sandbox) i blokuje „Uruchom”,
   gdy czegoś brakuje.
5. **Uruchomienie.** Z listy definicji („Uruchom …”) albo z karty zadania („Uruchom proces: …”).
   Podajesz repozytorium i cel. Przebieg powstaje w worktree z commita; jeden agent naraz.
6. **Monitor.** Uruchomienia → przebieg: kroki, komunikaty, czas. Czas agenta nie trafia do
   raportu pracy.
7. **Zgoda.** Przebieg czeka na decyzję o konkretnym diffie (skrót zmian). „Akceptuj” albo
   „Odrzuć” (odrzucenie wymaga komentarza). Aplikacja nie robi merge automatycznie.
8. **Zatrzymanie.** „Zatrzymaj” w monitorze albo na tablicy agentów (działa też bez klucza).
   „Zakończ aplikację” z traya w trakcie kroku agenta najpierw pyta o potwierdzenie: zakończenie
   przerwie krok, a przebieg będzie „Przerwany”. Zamknięcie okna (X) tylko chowa je do traya.
9. **Wznowienie po restarcie.** Przebieg, który przy zamknięciu aplikacji czekał w kolejce albo
   był między krokami, po starcie ma stan „Wstrzymany” i nic samo nie rusza (powiadomienie: „Przebieg
   wstrzymany po restarcie — wznów go w monitorze.”). „Wznów” w monitorze sprawdza katalog roboczy
   i uruchamia następny krok; naruszony katalog kończy przebieg błędem. „Wznów” wymaga klucza;
   „Zatrzymaj” działa też bez niego.
10. **Odzyskanie.** Przerwany przebieg (np. po awarii) wymaga „Odzyskaj przebieg”, który
    sprawdza procesy i zwalnia katalog roboczy. Przebieg nie jest wznawiany.
11. **Wyłączenie.** Ustawienia → Workflows → „Wyłącz Workflows”. Historia i definicje
    zostają; ponowne włączenie wymaga klucza. Wyłączenie jest wstrzymane, gdy są przebiegi w
    toku (kolejka, praca, zatrzymywanie); przebieg czekający na zgodę albo wstrzymany nie blokuje.

## Dane, kopia i retencja

- Diffy leżą w `workflows/runs/<runId>/` w katalogu danych aplikacji. Ustawienia → Workflows →
  retencja 7–365 dni; starsze diffy znikają przy starcie, historia zostaje.
- Kopia zapasowa osadza diffy. Przywrócenie jest zablokowane, gdy są nieskończone przebiegi
  (także czekające na zgodę i wstrzymane); przywrócona kopia nie odblokowuje modułu i przerywa
  stare przebiegi.

## Rozwiązywanie problemów

| Komunikat | Przyczyna | Co zrobić |
|-----------|-----------|-----------|
| Moduł Workflows nie jest odblokowany. Wpisz klucz dostępu w Ustawieniach. | Brak klucza albo moduł wyłączony. | Wpisz klucz w Ustawieniach → Workflows. |
| Nieprawidłowy klucz dostępu. | Błędny klucz. | Poproś właściciela o klucz. |
| Silnik przebiegów jeszcze się uruchamia. | Tuż po starcie aplikacji. | Poczekaj kilka sekund i ponów. |
| Silnik przebiegów jest zatrzymany. Uruchom aplikację ponownie. | Aplikacja się zamyka. | Uruchom aplikację ponownie. |
| Claude nie ma w MVP egzekwowanego ograniczenia zapisu do katalogu roboczego. | Claude w kroku z zapisem. | Użyj Codexa w kroku implementacji. |
| Codex nie ma skonfigurowanego sandboxa Windows. Ustaw [windows] sandbox = "elevated" w config.toml Codexa. | Brak sandboxa Codexa. | Dopisz ustawienie w `config.toml`. |
| Plik config.toml Codexa jest uszkodzony (niepoprawny TOML). | Błąd składni. | Napraw plik. |
| Nie udało się przygotować katalogu roboczego przebiegu. | Kod `workspaceUnavailable`. | Sprawdź repozytorium i miejsce na dysku. |
| Agent zwrócił niezgodną z protokołem odpowiedź. | Kod `protocolError`. | Ponów przebieg; zgłoś, jeśli się powtarza. |
| Wynik agenta nie pasuje do schematu roli. | Kod `invalidOutput`. | Ponów przebieg. |
| Wyczerpano limit rund poprawek, a review nadal wskazuje problemy. | Kod `reviewLimitExceeded` (limit 3 poprawek). | Przejrzyj diff ręcznie albo zmień cel. |
| Przebieg został przerwany (zamknięcie aplikacji albo agent bez reakcji na anulowanie); koniec kroku nie jest potwierdzony. | Stan Przerwany (`Interrupted`). | Użyj „Odzyskaj przebieg”. |
| Za mało miejsca na dysku danych aplikacji | Mało wolnego miejsca (start wymaga 512 MiB). | Zwolnij miejsce i uruchom ponownie. |
| Brak miejsca na dysku: nie zapisano diffu. Zwolnij miejsce i uruchom przebieg ponownie. | Dysk pełny przy zapisie diffu. | Zwolnij miejsce. |
| Aktualizacja wstrzymana. | Przebieg z procesem w toku. | Poczekaj albo zatrzymaj przebieg. |
| Nieskończone przebiegi Workflows | Blokada przywracania kopii. | Zakończ, zatwierdź, odrzuć albo zatrzymaj przebiegi (wstrzymany: wznów albo zatrzymaj). |
| Przebieg wstrzymany po restarcie — wznów go w monitorze. | Stan Wstrzymany (`Paused`) po starcie aplikacji. | „Wznów” w monitorze albo „Zatrzymaj”. |
| Przebieg nie jest wstrzymany. | „Wznów” na przebiegu, który już ruszył albo się zakończył. | Odśwież monitor. |
| Wyłączenie wstrzymane. | Przebieg w toku przy wyłączaniu. | Poczekaj albo zatrzymaj przebieg. |
| Baza Workflows pochodzi z nowszej wersji aplikacji | Starsza aplikacja na nowszej bazie. | Zaktualizuj aplikację. Historia i zatrzymanie działają. |

## Znane ograniczenia

- Claude „tylko do odczytu” to kontrola przez CLI, nie izolacja procesu.
- Jeden aktywny agent naraz; brak automatycznego merge.
- Czas agenta nie jest liczony w raportach pracy.
- Funkcja jest eksperymentalna; sprawdzenie na prawdziwych CLI robi użytkownik
  (`E4_TEST_RECZNY.md`, `E6_TEST_RECZNY.md`, `E7_TEST_RECZNY.md`).
