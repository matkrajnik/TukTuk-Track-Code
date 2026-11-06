# E8 — tryb „Dynamic Workflow” (tylko Claude): plan szczegółowy

Uzupełnienie rozdz. 10 `PLAN_WDROZENIA.md`. Plan powstał 2026-11-06 na podstawie kodu po E7
i zmianie „Claude w krokach z zapisem”. Rozpoznanie na prawdziwym CLI: `E8_ROZPOZNANIE.md`.
Decyzje użytkownika przed startem: rozdz. 9.

## 0. Co znaleziono w kodzie (wpływa na plan)

1. **Błąd (dotyczy 2.5.0): „Dziennik agentów” w aplikacji pokazuje surowy JSON.**
   `StoredEvent.payload` to całe zserializowane `RunEvent`
   (`{"type":"message","executionId":"…","payload":{"text":"…"}}`), a monitor czyta
   `e.payload.text`. Podgląd i helper `ev()` w `tests/workflow-runs.spec.ts` budują płaski kształt,
   którego host nigdy nie wysyła, więc testy tego nie wykrywają. Naprawa idzie osobno, przed E8
   (poprawka w partii po 2.5.0); E8.1 przestawia podgląd i testy na prawdziwy kształt.
2. **Parowanie narzędzi przez stos.** `runners/mod.rs::Stream::apply` kojarzy `ToolDone`
   z `self.tools.pop()`. Przy przeplatających się subagentach nazwy się pomylą. Potrzebne
   parowanie po `tool_use_id` (dziś `Parsed::Tool` nie ma id, `tool_result.tool_use_id` jest pomijane).
3. **Ten sam parser używa czat AI** (`ai_chat.rs` woła `parse_claude_line_raw`). Zmiana wariantów
   `Parsed` zepsułaby czat, dlatego dochodzi równoległy `parse_claude_line_traced`.
4. **`input` narzędzia jest wyrzucany** w runnerze, więc czytelnych linii („Czyta <plik>”) nie ma
   z czego zbudować.
5. **Payload `RunEvent::Message`/`Tool` to dowolny `Value`**: nowe pola (`toolUseId`,
   `parentToolUseId`, `args`) mieszczą się bez nowego wariantu i bez migracji. Stara historia
   czyta się jak dotąd (`engine.rs::stream_type` patrzy tylko na `phase`).
6. **Limit 500 zdarzeń na krok** (`driver/messages.rs::MAX_RUNNER_EVENTS_PER_EXECUTION`).
   Orkiestrator z subagentami wyczerpie go szybko.
7. **Skrót wersji liczony z `serde_json::to_string(graph)`** (`store.rs::graph_hash`). Każde nowe
   pole w `AgentConfig` musi mieć `skip_serializing_if = "Option::is_none"`.
8. **`workflow_create(from_template: bool)`** zna dwa szablony; dochodzi trzeci.
9. **`WorkflowRunMonitor.vue` ma ~690 linii**; drzewo subagentów i dziennik idą do osobnych komponentów.
10. Miejsca 1:1 do aktualizacji: `preflight.rs::readiness` ↔ `computePreflight` (fixture);
    `roleNames` (`workflowEdit.ts`) i `outputFields` (`api/workflows.ts`) dostają rolę; lista ról
    w `WorkflowNodePanel.vue` jej NIE dostaje.

## 1. Decyzja projektowa: typ wynika z grafu

Proces jest dynamiczny, gdy ma węzeł Agent z rolą `AgentRole::Orchestrator`. Graf ma stały,
kanoniczny kształt generowany przez `workflow_core::dynamic::build(spec)`; edytor pokazuje dla niego
formularz zamiast canvasu. Nowe pojęcia w domenie: wariant `Orchestrator`, opcjonalne
`AgentConfig.max_subagents`, moduł `dynamic.rs` (`DynamicSpec`, `build`, `spec_of`, `is_dynamic`),
trzy kody walidacji.

Dlaczego nie osobne pole `WorkflowGraph.kind`: jedno źródło prawdy (brak sprzeczności pole ↔ graf),
brak zmiany formatu (`SCHEMA_VERSION` 1, bez migracji, działają eksport, kopie, skróty wersji,
zakładka „Graf”), silnik/sterownik/recovery/warunki/pętla poprawek/zgoda bez zmian, prosta walidacja
kształtu (`spec_of(graph)` i równość z `build(spec)` z pominięciem pozycji; inaczej `InvalidDynamicShape`).

**Kształt kanoniczny** (stałe identyfikatory): `start` → `orchestrator` (Claude, Orchestrator,
dostęp `readOnly`/`workspaceWrite`, model, limit czasu, instrukcja, `maxSubagents`) →
[gdy `review=true`: `review` (Claude, Review, readOnly) → `decision` (`verdict == approved`,
`repairLimit` 3), „nie” → `fix` (Claude, Fix, workspaceWrite) → `review`] → `approval` →
`done` / `rejected`. Review tylko przy `workspaceWrite` (przy readOnly nie ma diffu). Wynik
orkiestratora ma schemat implementacji (`summary`, `changedFiles`, `checks`). Cel przebiegu zostaje
w formularzu startu.

## 2. Kroki implementacji (testy najpierw)

Logika zmieniana w Rust idzie 1:1 do `tests/fixtures/tauri.ts` i `src/api/*.ts`.

### E8.0 — rozpoznanie na prawdziwym CLI (użytkownik)
`E8_ROZPOZNANIE.md`. Na jego podstawie: próbki JSONL
`src-tauri/tests/fixtures/workflow_claude_dynamic_*.jsonl`, nazwa narzędzia subagenta
(`Agent`/`Task`, obsługujemy oba), decyzja o `DYNAMIC_SUBAGENTS_CONFIRMED` i o `Workflow`.
Do czasu rozpoznania próbki są syntetyczne (wg `claude --help`) i tak oznaczone.

### E8.1 — czytelny dziennik narzędzi (oba tryby)
- `ports.rs`: `RunnerEvent::ToolStarted` dostaje `args: Option<Value>`; `driver/messages.rs`
  dopisuje `"args"` tylko gdy `Some`.
- nowy `runners/activity.rs::tool_args(name, &input, worktree)`: allowlista kluczy
  (Read/Edit/Write: `file_path`; Grep: `pattern`, `path`, `glob`; Glob: `pattern`, `path`;
  Agent/Task: `description`, `subagent_type`; inne: brak). Nigdy `prompt`, `content`, `old_string`,
  `new_string`. Ścieżki względne do worktree, spoza worktree → `outside: true`. Wartości ≤ 200 znaków,
  przez `redact`.
- nowy `src/api/workflowActivity.ts`: `runnerPayload(e)`, `describeTool(name, args, phase)` →
  „Czyta src/a.ts”, „Szuka „foo” w src”, „Edytuje …”, „Zapisuje …”, „Szuka plików „*.ts””,
  „Uruchamia subagenta: <opis>”, inne „Narzędzie <name>”; błąd dopisuje „— błąd”.
- nowy `WorkflowActivityLog.vue` (komunikaty + narzędzia chronologicznie, limit 500, „Pokaż
  wcześniejsze”); oś zdarzeń pomija `toolStarted`/`toolFinished`.
- Testy: `runners::activity::tests::{args_allowlist_drops_prompt_and_unknown_keys,
  paths_outside_worktree_are_marked, long_and_secret_like_args_are_cut_and_redacted}`;
  `workflow-core/tests/driver.rs::tool_payload_keeps_optional_args`; Playwright
  `tests/workflow-activity.spec.ts`: „dziennik pokazuje czytelne linie narzędzi z prawdziwym
  kształtem zdarzeń”, „oś zdarzeń nie pokazuje już zdarzeń narzędzi”, „stara historia bez
  identyfikatorów narzędzi wyświetla się bez błędu”.

### E8.2 — parser z rodzicem i parowanie po `tool_use_id`
- `agent-cli/src/parse.rs`: `pub struct Traced { parent: Option<String>, tool_use_id: Option<String>,
  item: Parsed }`, `parse_claude_line_traced(line) -> Vec<Traced>`; `parse_claude_line_raw` bez zmian.
- `ports.rs`: `Message { text, parent }`, `ToolStarted { name, id, parent, args }`,
  `ToolFinished { name, id, parent, ok }`; `messages.rs`: `toolUseId`, `parentToolUseId` tylko gdy `Some`.
- `runners/mod.rs`: `Stream.tools` jako mapa `id → (name, parent)` plus stos zapasowy dla linii bez id.
  `structured_output`, `Done`, `Session` liczą się tylko z linii bez rodzica. Bloki `thinking`
  ignorowane. Nieznane pola/typy pomijane.
- Fixture `workflow_claude_dynamic_ok.jsonl`, `workflow_claude_dynamic_unknown.jsonl`.
- Testy: `agent-cli/tests/parse.rs::{traced_parser_reads_parent_tool_use_id_of_subagent_lines,
  traced_parser_keeps_tool_use_ids_for_pairing, traced_parser_ignores_thinking_blocks,
  traced_parser_survives_unknown_subagent_shapes, raw_parser_output_is_unchanged_for_subagent_lines}`;
  `runners/tests.rs::{tool_finish_is_paired_by_tool_use_id_across_interleaved_subagents,
  subagent_result_line_does_not_complete_the_step, unknown_subagent_event_shape_does_not_fail_the_step,
  orchestrator_stream_completes_with_subagent_events}`; `old_tool_payload_without_ids_still_replays`.

### E8.3 — domena (Rust + TS 1:1)
- `model.rs`: `AgentRole::Orchestrator`; `allowed_access(self)` (Orchestrator: oba profile);
  `required_access` jako domyślny dla szablonów (Orchestrator → WorkspaceWrite); `output_fields`
  jak Implementation; `AgentConfig.max_subagents` z `#[serde(default, skip_serializing_if)]`;
  `MAX_SUBAGENTS_HINT = 8`, `DEFAULT_SUBAGENTS_HINT = 3`, `DYNAMIC_TIMEOUT_SECS = 3600`.
- nowy `dynamic.rs`: `DynamicSpec { instruction, model, timeout_secs, access, max_subagents, review }`,
  `build`, `spec_of`, `is_dynamic`, `DYNAMIC_TEMPLATE_NAME`; `template.rs::dynamic_default()`.
- `validation.rs`: dostęp przez `allowed_access`; kody `InvalidDynamicShape`, `ProviderNotAllowed`,
  `InvalidSubagentLimit`; `check_dynamic` po `check_node_config`. Komunikaty: „Orkiestrator działa
  tylko z Claude.”, „Proces dynamiczny ma stały układ kroków; utwórz go z szablonu „Dynamiczny
  (Claude)”.”, „Review w trybie dynamicznym wymaga dostępu z zapisem.”, „Liczba subagentów musi
  wynosić od 1 do 8 i dotyczy tylko orkiestratora.”
- `output.rs`, `prompt.rs::contract`: Orchestrator jak Implementation.
- TS: `api/workflows.ts`, nowy `api/workflowDynamic.ts`, `workflowValidate.ts`,
  `workflowTemplates.ts`, `workflowEdit.ts` (`roleNames.orchestrator = "Orkiestrator"`).
- Testy: `validation.rs::{dynamic_template_is_valid_with_and_without_review, orchestrator_must_use_claude,
  orchestrator_outside_dynamic_shape_is_rejected, orchestrator_accepts_read_only_and_write_access,
  review_in_dynamic_requires_write_access, subagent_limit_only_on_orchestrator_and_in_range,
  dynamic_spec_round_trips_through_graph, graph_without_max_subagents_serializes_unchanged}`;
  `output.rs::orchestrator_output_uses_implementation_schema`; `workflow-definitions.spec.ts` (TS 1:1).

### E8.4 — adapter, prompt, limit zdarzeń, preflight
- `claude.rs`: dla Orchestrator zawsze `--restricted`, `--tools Read,Grep,Glob,Agent` (readOnly,
  `dontAsk`) albo `Read,Grep,Glob,Edit,Write,Agent` (zapis, `acceptEdits`), `--forward-subagent-text`;
  `,Workflow` tylko gdy `ORCHESTRATOR_WORKFLOW_TOOL = true` (domyślnie `false`).
- `prompt.rs::ORCHESTRATOR_GUIDE` (deleguje przez `Agent`, najwyżej N subagentów naraz, subagenci bez
  powłoki, wynik końcowy tylko od orkiestratora).
- `messages.rs`/`driver.rs`: `MessageBuffer::new(execution_id, limit)`,
  `MAX_RUNNER_EVENTS_PER_ORCHESTRATOR = 3000`, 500 dla pozostałych.
- `preflight.rs`: `CLAUDE_DYNAMIC = [2,1,319]`, `pub const DYNAMIC_SUBAGENTS_CONFIRMED: bool = false`;
  Codex jako orkiestrator → blocked „Tryb dynamiczny działa tylko z Claude.”; Claude < 2.1.319 →
  `UnsupportedVersion`; zapis przy `!DYNAMIC_SUBAGENTS_CONFIRMED` → `PermissionViolation` „Tryb
  dynamiczny z zapisem jest zablokowany, dopóki nie potwierdzisz, że subagenci dziedziczą ograniczenia
  (docs/workflows/E8_ROZPOZNANIE.md).”; readOnly → `CliRestricted` z ostrzeżeniem „subagenci:
  kontrola CLI, niezweryfikowane”.
- `workflows/mod.rs::workflow_create`: `template: TemplateKind` (`blank | standard | dynamic`).
- Fixture `computePreflight` 1:1.
- Testy: `runners/tests.rs::{orchestrator_args_add_agent_tool_and_forward_subagent_text,
  orchestrator_read_only_uses_restricted_without_edit_tools, standard_roles_do_not_get_agent_tool_or_forward_flag,
  workflow_tool_is_added_only_when_enabled, orchestrator_prompt_mentions_subagent_limit_and_no_shell,
  cancel_of_orchestrator_kills_the_whole_tree}`; `preflight/tests.rs::{dynamic_requires_claude_2_1_319,
  dynamic_with_write_is_blocked_until_subagent_inheritance_confirmed, dynamic_read_only_is_ready_with_subagent_warning,
  codex_orchestrator_is_blocked}`; `driver.rs::orchestrator_step_has_a_larger_event_budget`;
  `service/tests.rs::start_of_dynamic_write_is_refused_by_preflight`;
  `create_from_dynamic_template_gives_a_valid_dynamic_graph`.

### E8.5 — edytor i formularz startu
- nowy `WorkflowDynamicForm.vue` (instrukcja, model, limit czasu, dostęp, liczba subagentów, „Dodaj
  review i poprawki” wyłączone przy readOnly); `WorkflowEditor.vue` pokazuje formularz gdy
  `isDynamic(draft.graph)`; `WorkflowsView.vue` przycisk „Dynamiczny (Claude)”;
  `createWorkflow(name, description, template)`; `WorkflowRunForm.vue` ostrzeżenie `role="note"`:
  „Orkiestrator uruchamia subagentów; każdy zużywa tokeny osobno, więc koszt może być wielokrotnie
  wyższy niż w zwykłym kroku.”
- Testy `tests/workflow-dynamic.spec.ts`: „nowy proces dynamiczny otwiera formularz zamiast canvasu”,
  „formularz dynamiczny zapisuje i publikuje poprawny graf”, „review w trybie dynamicznym wymaga
  dostępu z zapisem”, „formularz startu ostrzega o koszcie trybu dynamicznego”, „start dynamicznego
  z zapisem jest zablokowany do potwierdzenia rozpoznania”, „start dynamicznego read-only wysyła
  żądanie startu”.

### E8.6 — drzewo orkiestrator → subagenci w monitorze
- `workflowActivity.ts::buildActivityTree(events, executionId)` → `{ orchestrator: Lane; subagents:
  Lane[]; truncated }`, `Lane = { id, label, current, state, lines }`. Subagent = `toolStarted`
  `Agent`/`Task` bez rodzica; jego zdarzenia mają `parentToolUseId` = id tego narzędzia; koniec =
  `toolFinished` z tym id. Głębsze zagnieżdżenie spłaszczane do drugiego poziomu.
- nowy `WorkflowAgentTree.vue` (lista zagnieżdżona, stan tekstem i ikoną, licznik „Subagenci: 2
  aktywnych / 5 łącznie” z `aria-live="polite"`); po limicie zdarzeń „Osiągnięto limit zapisanych
  zdarzeń kroku; bieżące czynności nie są już aktualizowane.”
- Testy (`workflow-activity.spec.ts`): „monitor przebiegu dynamicznego pokazuje drzewo orkiestratora
  i subagentów z licznikiem”, „bieżąca czynność subagenta zmienia się po zdarzeniu na żywo”,
  „przekroczony limit zdarzeń kroku pokazuje uczciwą informację”, „drzewo po ponownym otwarciu
  odtwarza się ze zdarzeń”.

### E8.7 — podgląd i fixture
`src/preview/workflowRuns.ts`: przykład „Dynamiczny (przykład)” (`succeeded`, dwóch subagentów,
zdarzenia w prawdziwym kształcie); `preview/workflows.ts`: szablon dynamiczny; fixture
`computePreflight` z zasadami trybu dynamicznego. Test „podgląd pokazuje przykładowy przebieg
dynamiczny z subagentami”; przejrzeć testy liczące przebiegi (`Otwórz przebieg`).

### E8.8 — dokumentacja i weryfikacja
`INSTRUKCJA.md` (sekcja „Tryb dynamiczny (Claude)”), `E0_ANALIZA.md` (dopisek), nowy
`E8_TEST_RECZNY.md`, `PLAN_WDROZENIA.md` rozdz. 15, `notes.md`; `npm run check` z kodem 0.

## 3. Kryteria odbioru (z testem)

| Kryterium | Test |
|---|---|
| Dziennik pokazuje tekst, nie JSON (prawdziwy kształt) | „dziennik pokazuje czytelne linie narzędzi z prawdziwym kształtem zdarzeń” |
| Argumenty narzędzi bez treści plików i promptów, ścieżki względne, sekrety ukryte | `args_allowlist_drops_prompt_and_unknown_keys`, `long_and_secret_like_args_are_cut_and_redacted` |
| Stara historia bez nowych pól działa | `old_tool_payload_without_ids_still_replays`, „stara historia bez identyfikatorów…” |
| Parser czatu bez zmian | `raw_parser_output_is_unchanged_for_subagent_lines` + `ai_chat::tests` |
| Przeplecione narzędzia parowane poprawnie | `tool_finish_is_paired_by_tool_use_id_across_interleaved_subagents` |
| `result` subagenta nie kończy kroku | `subagent_result_line_does_not_complete_the_step` |
| Nieznany kształt nie wywraca kroku | `unknown_subagent_event_shape_does_not_fail_the_step`, `traced_parser_survives_unknown_subagent_shapes` |
| Skróty istniejących definicji bez zmian | `graph_without_max_subagents_serializes_unchanged` |
| Orkiestrator tylko z Claude, tylko w kształcie kanonicznym (także przez IPC) | `orchestrator_must_use_claude`, `orchestrator_outside_dynamic_shape_is_rejected` + TS |
| Flagi CLI orkiestratora | `orchestrator_args_add_agent_tool_and_forward_subagent_text`, `orchestrator_read_only_uses_restricted_without_edit_tools`, `standard_roles_do_not_get_agent_tool_or_forward_flag` |
| `Workflow` tylko za flagą | `workflow_tool_is_added_only_when_enabled` |
| Claude ≥ 2.1.319; zapis zablokowany do potwierdzenia | `dynamic_requires_claude_2_1_319`, `dynamic_with_write_is_blocked_until_subagent_inheritance_confirmed`, `start_of_dynamic_write_is_refused_by_preflight` |
| Zatrzymanie kończy całe drzewo | `cancel_of_orchestrator_kills_the_whole_tree` |
| Większy budżet zdarzeń orkiestratora | `orchestrator_step_has_a_larger_event_budget` |
| Edytor: formularz, publikacja działa | „nowy proces dynamiczny otwiera formularz zamiast canvasu”, „formularz dynamiczny zapisuje i publikuje poprawny graf” |
| Ostrzeżenie o koszcie | „formularz startu ostrzega o koszcie trybu dynamicznego” |
| Drzewo z licznikiem, na żywo i po ponownym otwarciu | „monitor przebiegu dynamicznego pokazuje drzewo…”, „bieżąca czynność subagenta zmienia się…”, „drzewo po ponownym otwarciu…” |
| Podgląd ma przykład | „podgląd pokazuje przykładowy przebieg dynamiczny z subagentami” |

## 4. Istniejące testy i pliki do zmiany

- `runners/tests.rs::correct_review_result_completes_with_the_session`: literały
  `RunnerEvent::ToolStarted/ToolFinished` dostają nowe pola (`matches!` po nazwie).
- `workflow-core/src/testing.rs::message`: `parent: None`.
- `driver.rs::runner_messages_are_stored_with_an_explicit_truncation_marker` i
  `runs/tests.rs` (limit 500): sprawdzić, że limit 500 dalej dotyczy roli Analysis.
- `tests/workflow-runs.spec.ts`: `ev()` na prawdziwy kształt; testy „5000 komunikatów…”,
  „monitor po ponownym otwarciu…”, `approvalFixture()`.
- `src/preview/workflowRuns.ts::eventList`: prawdziwy kształt; testy liczące przebiegi.
- `computePreflight` (fixture); `createWorkflow` z `template`; `workflow_commands.rs` (sprawdzić
  `fromTemplate`); `roleNames`, `outputFields` (kompilator wymusi); `npm run test:visual`.

## 5. Poza zakresem

Codex jako orkiestrator; równoległe przebiegi; Parallel/Join; egzekwowanie liczby subagentów (tylko
wskazówka); `Workflow` domyślnie; MCP dla subagentów; Bash/PowerShell w trybie dynamicznym; zapis
bloków `thinking`; liczenie tokenów/kosztu; edycja grafu dynamicznego na canvasie; konwersja
dynamiczny ↔ standardowy; subagenci na tablicy agentów; zmiana obcinania długiego tekstu; wydanie.

## 6. Testy ukierunkowane

```powershell
cargo test --manifest-path src-tauri/Cargo.toml -p agent-cli --test parse
cargo test --manifest-path src-tauri/Cargo.toml -p workflow-core --features testing --test validation --test output --test driver --test run_states
cargo test --manifest-path src-tauri/Cargo.toml --locked workflows::runners
cargo test --manifest-path src-tauri/Cargo.toml --locked workflows::preflight
cargo test --manifest-path src-tauri/Cargo.toml --locked workflows::service
cargo test --manifest-path src-tauri/Cargo.toml --locked ai_chat
npx playwright test tests/workflow-activity.spec.ts tests/workflow-dynamic.spec.ts tests/workflow-runs.spec.ts tests/workflow-definitions.spec.ts tests/workflows-ui.spec.ts
npm run test:visual
npm run check
```

(Nazwy pakietów `agent-cli`/`workflow-core` sprawdzić w `Cargo.toml` crate przed `-p`.)

## 7. Ryzyka

| Ryzyko | Działanie |
|---|---|
| Subagenci nie dziedziczą `--restricted`/`--tools` | Zapis zablokowany stałą do potwierdzenia; readOnly z `--restricted` i migawką worktree; test B rozpoznania; opcja: blokada obu profili (D2) |
| Inny kształt zdarzeń niż w `--help` | Parser odporny, zapasowe parowanie stosem, obie nazwy narzędzia; próbki z rozpoznania zastępują syntetyczne |
| `result`/`init` subagenta kończy krok lub podmienia sesję | Filtr „tylko linie bez rodzica”; test |
| Lawina zdarzeń | Budżet 3000, partie po 32, render 500 linii, komunikat po limicie |
| Wyciek treści plików/sekretów w argumentach | Allowlista, 200 znaków, `redact` |
| Zmiana skrótów opublikowanych wersji | `skip_serializing_if` + test skrótu |
| Regresja czatu AI | `_raw` bez zmian, osobny parser, testy `ai_chat::` |
| Koszt tokenów | Ostrzeżenie, wskazówka liczby subagentów, limit czasu 60 min |
| `--restricted` z `dontAsk` zachowuje się inaczej | Test A rozpoznania; awaryjnie readOnly bez `--restricted` (D4) |
| Zatrzymanie nie kończy subagentów | Subagenci w procesie `claude`; `taskkill /T`; test E |
| Monitor > 800 linii | Osobne komponenty i `workflowActivity.ts` |

## 8. Kolejność i wydania

E8.1 (naprawa dziennika + czytelne narzędzia) i E8.2 (parser) są niezależne od trybu dynamicznego
i mogą wyjść wcześniej jako poprawka. Interfejs trybu dynamicznego (E8.5) trafia do użytkownika
dopiero razem z adapterem (E8.4). Zależności: E8.1 → E8.0 (rozpoznanie) przed ostatecznym E8.2 →
E8.3 → E8.4 → E8.5/E8.6 → E8.7 → E8.8.

## 9. Decyzje użytkownika przed startem

| # | Decyzja | Rekomendacja |
|---|---|---|
| D1 | Typ wynika z grafu (rola `orchestrator`) zamiast pola `kind` | tak |
| D2 | Co blokować do potwierdzenia rozpoznania: (A) tylko zapis, (B) oba profile | B jest ostrożniejsze (subagent z Bash przy readOnly mógłby działać poza worktree) |
| D3 | Flaga potwierdzenia: stała w kodzie `DYNAMIC_SUBAGENTS_CONFIRMED` czy przełącznik w Ustawieniach | stała (zmiana świadoma, odnotowana w `E8_ROZPOZNANIE.md`) |
| D4 | `--restricted` także dla orkiestratora read-only | tak |
| D5 | Narzędzie `Workflow` | wyłączone do testu C |
| D6 | Poprawki w pętli review: Claude w roli Fix czy ponownie orkiestrator | Fix |
| D7 | Budżet zdarzeń orkiestratora | 3000 |
| D8 | Ostrzeżenie o koszcie: sam tekst czy checkbox „Rozumiem koszt” | tekst |
| D9 | Liczba subagentów | 1–8, domyślnie 3, tylko wskazówka |
| D10 | Zapis tekstu subagentów (`--forward-subagent-text`), bez `thinking` | tak |
| D11 | Obrona w głąb `--disallowed-tools Bash,PowerShell,WebFetch,WebSearch` | po teście B |
| D12 | E8.1–E8.2 jako osobne wydanie poprawek przed trybem dynamicznym | tak |

**Decyzje użytkownika (2026-11-06):** D2 = B (do potwierdzenia rozpoznania zablokowane są oba
profile; tryb dynamiczny w ogóle nie startuje), D8 = sam tekst, D12 = tak. Pozostałe (D1, D3–D7,
D9–D11) według rekomendacji. Skutki dla planu: w E8.4 preflight blokuje każdy węzeł `orchestrator`
przy `!DYNAMIC_SUBAGENTS_CONFIRMED` (komunikat „Tryb dynamiczny jest zablokowany, dopóki nie
potwierdzisz w docs/workflows/E8_ROZPOZNANIE.md, że subagenci dziedziczą ograniczenia.”); test
`dynamic_is_blocked_until_subagent_inheritance_confirmed` zastępuje wariant „z zapisem”; fixture
dostaje opcję `subagentsConfirmed` (odpowiednik stałej), żeby Playwright sprawdził start po
potwierdzeniu. Realizacja w dwóch częściach skryptu `workflows-stage`: **E8a** = E8.0–E8.2
(poprawka wydania, nie wymaga decyzji), **E8b** = E8.3–E8.8 (po rozpoznaniu; `notes.E8b` ma
zawierać wynik `E8_ROZPOZNANIE.md` albo „nie wykonano — tryb zostaje zablokowany”).
