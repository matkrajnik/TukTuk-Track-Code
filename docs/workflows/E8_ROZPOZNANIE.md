# E8 — rozpoznanie na prawdziwym Claude Code (wykonuje użytkownik)

Cel: potwierdzić, jak Claude Code zachowuje się z subagentami w trybie `-p`, zanim parser,
adapter i preflight trybu dynamicznego (`E8_PLAN.md`) dostaną ostateczny kształt. Testy
automatyczne projektu nie łączą się z prawdziwym CLI, więc ten plik jest jedynym dowodem.

Wszystko w pustym katalogu tymczasowym, poza repozytorium projektu. Komendy w PowerShell.
Konfigurację MCP podajemy plikiem, bo PowerShell 5.1 gubi cudzysłowy w JSON przekazywanym
do programu.

```powershell
$d = Join-Path $env:TEMP ("e8-" + [guid]::NewGuid()); New-Item -ItemType Directory $d | Out-Null; Set-Location $d
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

## Wynik (do wypełnienia)

- Wersja `claude --version`: …
- Odpowiednie linie `--help`: …
- Próbki (2–3 linie każdego rodzaju z A–D, identyfikatory sesji można ukryć): …
- (a) Subagenci dziedziczą `--restricted` i listę `--tools`: tak / nie — dowód: …
- (b) `Workflow` działa w `-p` bez zgody: tak / nie — …
- (c) Kształt `parent_tool_use_id` i nazwa narzędzia subagenta: …
- Wyniki `Test-Path` / `git status` z testu B: …
- Wynik testu E: …
- **Decyzja: `DYNAMIC_SUBAGENTS_CONFIRMED` = tak / nie** (data: …)
