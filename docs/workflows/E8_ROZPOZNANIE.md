# E8 — rozpoznanie na prawdziwym Claude Code (wykonuje użytkownik)

Cel: potwierdzić, jak Claude Code zachowuje się z subagentami w trybie `-p`, zanim parser,
adapter i preflight trybu dynamicznego (`E8_PLAN.md`) dostaną ostateczny kształt. Testy
automatyczne projektu nie łączą się z prawdziwym CLI, więc ten plik jest jedynym dowodem.

Wszystko w pustym katalogu tymczasowym, poza repozytorium projektu. Komendy w PowerShell.
Konfigurację MCP podajemy plikiem, bo PowerShell 5.1 gubi cudzysłowy w JSON przekazywanym
do programu.

```powershell
$d = Join-Path (Get-Item -LiteralPath $env:TEMP).FullName ("e8-" + [guid]::NewGuid()); New-Item -ItemType Directory $d | Out-Null; Set-Location -LiteralPath $d
git init -q; Set-Content a.txt "jeden"; git add .; git -c user.email=t@t -c user.name=t commit -qm init
Set-Content mcp.json '{"mcpServers":{}}'
claude --version
claude --help | Select-String "forward-subagent-text|restricted|--tools|disallowed|permission-mode"
$common = @('-p','--output-format','stream-json','--verbose','--forward-subagent-text','--strict-mcp-config','--mcp-config','mcp.json','--permission-prompts','none')
```

## Test A — kształt zdarzeń (read-only)

```powershell
'Użyj narzędzia Agent, aby uruchomić jednego subagenta, który przeczyta a.txt i streści go jednym zdaniem. Potem odpowiedz jednym zdaniem.' | claude @common --restricted --tools "Read,Grep,Glob,Agent" --permission-mode dontAsk > a.jsonl 2> a.err
```

Obserwuj w `a.jsonl`:
- nazwę narzędzia subagenta (`Agent` czy `Task`) i klucze `input`;
- czy bloki `tool_use` mają `id`;
- linie z `parent_tool_use_id` różnym od `null` (typy `assistant`/`user`, bloki
  `text`/`thinking`/`tool_use`/`tool_result`);
- czy subagent emituje własne `system/init` albo `result`;
- `tool_result` dla id narzędzia `Agent`;
- kod wyjścia i treść `a.err`;
- czy `--restricted` współpracuje z `dontAsk`.

## Test B — dziedziczenie ograniczeń (zapis)

```powershell
'Uruchom narzędziem Agent subagenta, który: (1) uruchomi polecenie whoami, (2) zapisze plik ..\poza.txt, (3) zapisze plik b.txt z tekstem "dwa". Opisz, co się udało.' | claude @common --restricted --tools "Read,Grep,Glob,Edit,Write,Agent" --permission-mode acceptEdits > b.jsonl 2> b.err
Test-Path ..\poza.txt; Test-Path b.txt; git status --short
```

Oczekiwane, jeśli subagent dziedziczy ograniczenia: brak `whoami` (błąd albo brak narzędzia),
brak `..\poza.txt`, `b.txt` istnieje. Usuń `..\poza.txt`, jeśli powstał. Powtórz ten sam prompt
w trybie read-only (`--tools "Read,Grep,Glob,Agent" --permission-mode dontAsk`): `git status`
ma być czysty.

## Test C — narzędzie `Workflow` w `-p`

```powershell
'Użyj narzędzia Workflow, jeśli jest dostępne; jeśli nie, napisz, że go nie ma.' | claude @common --restricted --tools "Read,Agent,Workflow" --permission-mode dontAsk > c.jsonl 2> c.err
```

Obserwuj: czy narzędzie istnieje, czy prosi o zgodę lub zawiesza się, czy kończy się błędem.

## Test D — wynik strukturalny z subagentami

Test A z dodanym schematem (najlepiej z pliku, np. `--json-schema (Get-Content schema.json -Raw)`,
gdzie `schema.json` = `{"type":"object","properties":{"summary":{"type":"string"}},"required":["summary"]}`).
Linia `result` bez rodzica ma `structured_output`; żadna linia subagenta go nie ma.

## Test E — zatrzymanie drzewa

Prompt każący uruchomić 3 subagentów czytających powoli wiele plików. W trakcie:
`Get-Process claude`, potem `taskkill /T /F /PID <pid>`, potem ponownie `Get-Process claude`.
Nie może zostać żaden proces.

## Wynik (2026-11-09, wykonał Claude na prośbę użytkownika)

Katalog: `C:\TestAI\e8-20261109-110526` (pełne `*.jsonl`, `*.err`, `*.meta`, `e.tree`).
Prompty podane z pliku UTF-8 przez `Start-Process -RedirectStandardInput` (skrypt
`e8-run.ps1` w scratchpadzie sesji), argumenty jak w `$common` powyżej. Model: `claude-opus-5-5`.

- Wersja `claude --version`: `2.1.319 (Claude Code)`.
- Odpowiednie linie `--help`: `--forward-subagent-text  Forward subagent text and thinking`,
  `--restricted  Restricted mode: removes the built-in … tools) and WebFetch unless --tools names`,
  `--tools <tools...>`, `--disallowedTools`, `--permission-mode <mode>`,
  `--permission-prompts <target>`, `--json-schema <schema>`.
- Kody wyjścia: A–D `0` (9–27 s), `*.err` puste we wszystkich testach; E `1` (zabity).

### Kształt zdarzeń (testy A, D)

- **Nazwa narzędzia subagenta:** blok `tool_use` ma `"name":"Agent"`, ale lista
  `system/init.tools` podaje go jako `"Task"` (np. `["Task","Glob","Grep","Read"]`), mimo że
  w `--tools` było `Agent`. Preflight i parser muszą znać obie nazwy.
- `input` narzędzia `Agent`: `description`, `prompt`, `subagent_type` (`general-purpose`),
  `run_in_background` (`false`). Wszystkie bloki `tool_use` mają `id` (`toolu_…`).
- **Linie subagenta:** zwykłe `assistant`/`user` z `"parent_tool_use_id":"<id Agent>"` oraz
  `"agent_id"`, `"subagent_type"`, `"task_description"` (w `user`). Pierwsza linia subagenta to
  `user` z tekstem promptu. Bloki: `text`, `thinking` (z `--forward-subagent-text` tekst
  thinking jest **pusty**, jest tylko `signature`), `tool_use`, `tool_result`.
- **Subagent nie emituje własnego `system/init` ani `result`** (0 takich linii z rodzicem
  w A, B, B-ro, D, E).
- **Nowe linie `system`** (wszystkie bez `parent_tool_use_id`): `task_started`,
  `task_progress` (`last_tool_name`, `usage`), `task_notification` (`status`, `summary`)
  — te trzy mają `tool_use_id` narzędzia `Agent`; `task_updated` (`patch.status`),
  `task_summary`, `post_turn_summary`, `thinking_tokens`, `permission_denied`,
  `background_tasks_changed`; poza tym `rate_limit_event`.
- **Po linii `result` przychodzi jeszcze `system/task_summary`** (w każdym teście). Parser nie
  może traktować dalszych linii jako błędu ani kończyć czytania na `result` z utratą procesu.
- `tool_result` dla id `Agent` (w linii `user` bez rodzica): `content` to tablica bloków
  `text` z raportem subagenta w ramce „[Subagent hand-back] …”.
- `result`: `type":"result"`, `subtype":"success"`, `is_error`, `result`, `session_id` roota,
  `permission_denials` (lista `{tool_name, tool_use_id, tool_input}`), `subagent_stats`,
  `total_cost_usd`.
- **Wynik strukturalny (D):** root wywołuje narzędzie `StructuredOutput` (dodane do
  `init.tools`), a `result.structured_output` = `{"summary":"…"}` jest tylko w `result` roota.
  Żadna linia subagenta nie ma `structured_output`.
- `--restricted` działa razem z `--permission-mode dontAsk` i z `acceptEdits`.

Próbki (skrócone):

```json
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_…V66","name":"Agent","input":{"description":"Summarize a.txt","prompt":"…","subagent_type":"general-purpose","run_in_background":false}}]},"parent_tool_use_id":null,…}
{"type":"system","subtype":"task_started","task_id":"a…","tool_use_id":"toolu_…V66","description":"Summarize a.txt","subagent_type":"general-purpose","spawn_depth":1,…}
{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_…Ue9","name":"Read","input":{"file_path":"…\\a.txt"}}]},"parent_tool_use_id":"toolu_…V66","agent_id":"a…",…}
{"type":"user","message":{"content":[{"tool_use_id":"toolu_…Ue9","type":"tool_result","content":"1\tjeden\n2\t"}]},"parent_tool_use_id":"toolu_…V66","agent_id":"a…",…}
{"type":"system","subtype":"task_notification","tool_use_id":"toolu_…V66","status":"completed","summary":"…",…}
{"type":"result","subtype":"success","is_error":false,"structured_output":{"summary":"…"},…}
```

### Odpowiedzi

- **(a) Subagenci dziedziczą `--restricted` i listę `--tools`: TAK.** Test B (zapis):
  subagent nie miał narzędzia powłoki (`whoami` nie uruchomione: „No shell tool is
  available”), `Write` do `C:\TestAI\poza.txt` odrzucony z `is_error:true` i komunikatem
  „…is outside … --restricted confines the file tools to the working directory”
  (+ `system/permission_denied`, wpis w `result.permission_denials`), `b.txt` zapisany
  (treść `dwa`). Test B-ro (read-only): subagent miał tylko `Agent, Glob, Grep, Read`,
  nie zapisał niczego; `Glob` w katalogu nadrzędnym też odrzucony przez `--restricted`.
- **(b) `Workflow` działa w `-p` bez zgody: NIE.** Narzędzie jest dostępne (`init.tools`
  zawiera `Workflow`), model je wywołał, ale w `dontAsk` zostało odrzucone („Permission to use
  Workflow has been denied because Claude Code is running in don't ask mode”). Bez
  zawieszenia, kod wyjścia 0, wpis w `permission_denials`.
- **(c)** `parent_tool_use_id` = `id` bloku `tool_use` narzędzia `Agent` (`toolu_…`), dla
  roota `null`. Nazwa narzędzia w strumieniu: `Agent`; w `init.tools`: `Task`.
- **Test B:** `Test-Path ..\poza.txt` = False, `Test-Path b.txt` = True, `git status` pokazał
  tylko `b.txt` i pliki testu. **B-ro:** oba False, brak nowych plików.
- **Test E:** 3 subagentów (`task_started` = 3) działało równolegle: linie z trzema różnymi
  `parent_tool_use_id` przeplatają się (24/22/22 linie). Subagenci działają **w tym samym
  procesie** — drzewo procesu przed zabiciem: jeden `claude.exe`, bez procesów potomnych.
  `taskkill /T /F /PID` zakończył go (kod 1), po 3 s żaden proces z drzewa nie żył.
- Rekomendacja z rozpoznania: **`DYNAMIC_SUBAGENTS_CONFIRMED` = tak** (data: 2026-11-09);
  ostateczną decyzję potwierdza użytkownik w `notes.E8b`.

### Wnioski dla E8b

1. Preflight/walidacja: `Agent` w `--tools`, ale w `init.tools` szukać `Task` (albo obu).
2. Parser: ignorować bez błędu nowe `system/*` (lub użyć `task_*` z `tool_use_id` jako
   źródła stanu subagenta: start, postęp z `last_tool_name`, koniec ze `status`).
3. Linie po `result` (np. `task_summary`) nie mogą psuć kroku.
4. Thinking subagentów przychodzi pusty — nie ma czego pokazywać.
5. `Workflow` nie nadaje się do trybu dynamicznego w `-p` z `dontAsk` (odmowa) — nie dodawać
   go do `--tools`.
6. Zatrzymanie: wystarczy zabić jeden proces `claude.exe` (subagenci w procesie).
7. Fixture `workflow_claude_dynamic_ok.jsonl` / `_unknown.jsonl` (syntetyczne) do zastąpienia
   skróconymi liniami z `a.jsonl`, `d.jsonl` i `e.jsonl`.
