TukTuk Track: poprawki po 2.0.0.

- Pomiar po wyłączeniu komputera: gdy aplikacja nie działała co najmniej 10 minut, trwający pomiar kończy się na ostatnim znaku życia aplikacji, a baner „Pomiar zatrzymany po przerwie” pozwala go zostawić, wznowić albo przywrócić cały czas.
- Ręczne spotkania cykliczne: „Powtarzaj” przy dodawaniu (dni tygodnia, co 1–8 tygodni, do wybranego dnia, najwyżej 200 spotkań i rok). Wystąpienia edytuje się pojedynczo, a usuwa jedno albo „to i kolejne”. Starsze bazy i kopie dostają kolumnę `series_id` przy starcie.
- Dodanie zadania uruchamia pomiar: „Dodaj i rozpocznij” (Enter) dodaje i od razu mierzy, poprzedni pomiar staje; „Zaplanuj” (Shift+Enter) dodaje bez pomiaru i otwiera plan zadania. Po zakończonym dniu start pyta „Wznowić dzień?”.
