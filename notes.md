TukTuk Track 2.6.0: podpowiedzi istniejących zadań i tryb dynamiczny Workflows.

- Pole „Nowe zadanie” podpowiada istniejące, nieukończone zadania (strzałki i Enter wybierają, Escape zamyka listę). Gdy wpisana nazwa jest taka sama jak nazwa istniejącego zadania (bez względu na wielkość liter i ogonki), przycisk zmienia się na „Kontynuuj”: Enter wznawia pomiar tego zadania, a „Zaplanuj” otwiera jego plan, zamiast tworzyć nowe zadanie.
- Workflows: tryb dynamiczny (Claude). Nowy szablon „Dynamiczny (Claude)”: jeden orkiestrator deleguje pracę do subagentów (narzędzie Agent), opcjonalnie review i poprawki; wymaga Claude Code 2.1.319+, subagenci bez powłoki i tylko w katalogu roboczym. Monitor pokazuje drzewo orkiestrator → subagenci z licznikiem; formularz startu ostrzega o koszcie tokenów.
- Workflows: dziennik narzędzi agenta i śledzenie subagentów Claude w monitorze przebiegu.
