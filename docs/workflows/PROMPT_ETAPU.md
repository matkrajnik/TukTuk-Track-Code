# Workflows: uruchamianie kolejnych etapów

Etapy E3–E7 z [PLAN_WDROZENIA.md](PLAN_WDROZENIA.md) realizuje zapisany workflow
`.claude/workflows/workflows-stage.js` (nazwa: `workflows-stage`). Jedno uruchomienie przechodzi
przez kolejne etapy: po każdym etapie robi commit i od razu zaczyna następny.

## Jak działa workflow (każdy etap)

| Faza | Model | Co robi |
| --- | --- | --- |
| Plan | model sesji (agent `planner`) | Czyta etap w rozdz. 10, powiązane rozdziały, dziennik w rozdz. 15 i kod. Zwraca kroki, kryteria odbioru i testy. |
| Implementacja | **Sonnet** (1. pętla developera) | Realizuje plan: najpierw testy, potem kod. Uruchamia tylko testy ukierunkowane. |
| Review | model sesji | Dwa przeglądy równolegle: kod (`code-reviewer`) oraz zgodność z planem (każde kryterium odbioru ma test). |
| Poprawki | **Opus** (2. i kolejne pętle) | Najwyżej 2 rundy: poprawki CRITICAL, HIGH i MEDIUM z testem na każdy błąd, potem ponowny przegląd. |
| Check | model sesji (`effort: low`); naprawa na **Opusie** | Pełne `npm run check` uruchamiane bez nic równolegle. Najwyżej 2 próby naprawy. Same timeouty środowiskowe = ponowne uruchomienie bez zmian. |
| Dziennik | Sonnet | Wpis w rozdz. 15 i checkboxy etapu w rozdz. 10, tylko te z dowodem w testach. |
| Commit | Sonnet | `feat: Workflows E<n> — …`, tylko przy zielonym `npm run check`. Bez push i bez `--no-verify`. `.claude/launch.json` zostaje poza commitem. |

### Kiedy workflow się zatrzymuje

- `npm run check` jest czerwone po próbach naprawy. Zmiany etapu zostają niezacommitowane, a stan jest opisany w dzienniku.
- Commit się nie udał, np. hook odrzucił go dwa razy.
- Przed E4, jeśli w `notes.E4` nie ma Twojej decyzji o trybie „tylko do odczytu” dla Claude (bloker 1 w [E0_ANALIZA.md](E0_ANALIZA.md)).

Workflow nigdy nie pushuje i nie uruchamia `npm run ship`. Na jeden etap przypada od kilkunastu do
około 20 agentów, więc cały zakres E3–E7 to kilkadziesiąt do około stu agentów.

## Prompt do nowej sesji

Wklej w nowej sesji Claude Code w tym repozytorium.

```text
Uruchom zapisany workflow `workflows-stage` z args:
{
  "from": "E3",
  "to": "E7",
  "notes": {
    "E3": "Silnik jako czysta maszyna stanów w workflow-core (zdarzenia wejściowe → następny krok), stan odtwarzany z zapisanych zdarzeń (workflow_runs, workflow_node_executions, workflow_events, workflow_approvals). Runner jako trait z deterministycznym runnerem testowym; prawdziwe CLI dopiero w E4. Bez komendy startu w UI (formularz startu to E5). Po wyczerpaniu limitu rund przebieg kończy się Failed z kodem ReviewLimitExceeded. Restart na granicy kroków nie dubluje kolejnego uruchomienia."
  }
}
Przed uruchomieniem sprawdź `git status`: drzewo ma być czyste, poza .claude/launch.json.
Workflow realizuje kolejne etapy według docs/workflows/PLAN_WDROZENIA.md. Pierwsza pętla developera
idzie na Sonnecie, poprawki na Opusie. Po każdym etapie robi commit (bez push) i przechodzi dalej.
Bez notes.E4 zatrzyma się przed E4.
Po zakończeniu pokaż mi dla każdego etapu: commit, kroki planu, kryteria odbioru z testami, wynik
npm run check, niezałatwione uwagi z review i to, co etap pominął (notDone, outOfScope).
Nie pushuj.
```

Żeby od razu przejść też przez E4, dopisz w `notes` klucz `"E4"` z decyzją. Przykład:
„Akceptuję read-only dla Claude: --tools Read,Grep,Glob, --permission-mode dontAsk,
--permission-prompts none, --strict-mcp-config oraz wykrywanie zmian worktree przed krokiem i po nim;
UI nazywa to kontrolą CLI, nie izolacją systemu plików. Codex review: -s read-only, preflight blokuje
profil bez skonfigurowanego sandboxa.”

Jeden etap: `{ "stage": "E5" }` (opcjonalnie `"notes": "…"` jako tekst).

## Uwagi do etapów

- **E4.** Test na prawdziwych CLI robisz sam: workflow przygotuje checklistę (rozdz. 10, E4).
- **E7.** Wydanie podpisujesz i publikujesz sam (`RELEASE.md`). Workflow przygotuje dokumentację
  i notatki do wydania.
