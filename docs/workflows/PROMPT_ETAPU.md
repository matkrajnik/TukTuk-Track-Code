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

## E8 — tryb „Dynamic Workflow” (po wydaniu 2.5.1)

Etap jest podzielony na dwie części skryptu: **E8a** (E8.0–E8.2: naprawa kształtu zdarzeń,
czytelne linie narzędzi w dzienniku, parser subagentów; nie zmienia UI trybu dynamicznego) i
**E8b** (E8.3–E8.8: tryb dynamiczny). Plan: [E8_PLAN.md](E8_PLAN.md); rozpoznanie na prawdziwym
CLI, które wykonuje użytkownik: [E8_ROZPOZNANIE.md](E8_ROZPOZNANIE.md). E8b zatrzymuje się, gdy
w `notes.E8b` nie ma wyniku rozpoznania (albo decyzji „nie wykonano — tryb zostaje zablokowany”).

### Prompt E8a (nowa sesja)

```text
Uruchom zapisany workflow `workflows-stage` z args:
{
  "stage": "E8a",
  "notes": {
    "E8a": "Zakres: E8.0–E8.2 z docs/workflows/E8_PLAN.md (rozdz. 2) — E8.0 to checklista dla użytkownika (E8_ROZPOZNANIE.md już istnieje; nie wykonuj jej, próbki JSONL subagentów są syntetyczne wg `claude --help` 2.1.319 i tak oznaczone). E8.1: czytelny dziennik narzędzi dla obu trybów (runners/activity.rs z allowlistą argumentów, src/api/workflowActivity.ts, WorkflowActivityLog.vue; oś zdarzeń bez zdarzeń narzędzi); podgląd i testy już używają prawdziwego kształtu zdarzeń (naprawa z 2.5.1, commit 09131f8) — nie cofaj tego. E8.2: parse_claude_line_traced z parent_tool_use_id i tool_use_id obok niezmienionego parse_claude_line_raw (czat AI bez regresji), parowanie narzędzi po id ze stosem zapasowym, linie subagentów nie kończą kroku ani nie podmieniają sesji. Bez roli orchestrator, bez zmian w preflight, edytorze i formularzu startu (to E8b). Decyzje użytkownika z E8_PLAN.md rozdz. 9 obowiązują (D2 = blokada obu profili, D12 = E8a osobno). Commit: feat: Workflows E8a — …"
  }
}
Przed uruchomieniem sprawdź `git status`: drzewo ma być czyste, poza .claude/launch.json.
Workflow robi commit (bez push). Nie pushuj, nie uruchamiaj npm run ship.
Po zakończeniu pokaż: commit, kroki planu, kryteria odbioru z testami, wynik npm run check,
niezałatwione uwagi z review i to, co etap pominął (notDone, outOfScope).
```

### Prompt E8b (po wypełnieniu E8_ROZPOZNANIE.md)

```text
Uruchom zapisany workflow `workflows-stage` z args:
{
  "stage": "E8b",
  "notes": {
    "E8b": "Wynik E8_ROZPOZNANIE.md: (a) subagenci dziedziczą --restricted i --tools: TAK/NIE; (b) Workflow w -p: TAK/NIE; (c) nazwa narzędzia subagenta: Agent/Task, kształt parent_tool_use_id: …; test E (zabicie drzewa): …; DYNAMIC_SUBAGENTS_CONFIRMED = TAK/NIE. Zakres: E8.3–E8.8 z E8_PLAN.md, decyzje D1–D12 z rozdz. 9 (D2 = oba profile zablokowane do potwierdzenia, D8 = sam tekst o koszcie). Próbki syntetyczne zastąp prawdziwymi z rozpoznania."
  }
}
Przed uruchomieniem sprawdź `git status`: drzewo ma być czyste, poza .claude/launch.json.
Nie pushuj. Po zakończeniu pokaż to samo zestawienie co dla E8a.
```

Jeśli rozpoznania nie wykonano, w `notes.E8b` wpisz „nie wykonano — tryb zostaje zablokowany”:
interfejs i silnik powstaną, a preflight będzie odrzucał start trybu dynamicznego do czasu
potwierdzenia.
