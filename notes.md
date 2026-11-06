TukTuk Track 2.5.1: poprawki Workflows po pierwszym użyciu 2.5.0.

- Dziennik agentów w monitorze przebiegu pokazuje tekst komunikatów (w 2.5.0 pokazywał surowy JSON zdarzenia).
- Claude Code 2.1.319+ może wykonywać kroki z zapisem (Implementacja, Poprawki) w trybie `--restricted`: zmiany tylko w katalogu roboczym przebiegu, bez poleceń powłoki (agent nie uruchamia testów, tylko je wypisuje). Starsza wersja dostaje czytelny komunikat z wykrytą wersją.
- Monitor przebiegu ma zakładki „Przebieg” i „Graf”; graf w osobnej zakładce wypełnia szerokość okna z powiększeniem co najmniej 100% i czytelnymi statusami kroków.
- Nowa animacja odblokowania Workflows: świetlne smugi po krawędziach okna i podświetlony graf (Esc pomija; przy ograniczonym ruchu bez animacji).
- Przebieg na granicy kroków po restarcie czeka jako „Wstrzymany” do ręcznego „Wznów”; „Zakończ aplikację” z traya w trakcie kroku agenta pyta o potwierdzenie.
