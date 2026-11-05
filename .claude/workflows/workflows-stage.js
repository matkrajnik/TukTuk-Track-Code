export const meta = {
  name: 'workflows-stage',
  description: 'Realizuje kolejne etapy planu Workflows (E3–E7): plan, implementacja (Sonnet), review i poprawki (Opus), npm run check, dziennik i commit po każdym etapie',
  whenToUse: 'Etapy z docs/workflows/PLAN_WDROZENIA.md, np. args: { from: "E3", to: "E7", notes: { E3: "...", E4: "..." } }',
  phases: [
    { title: 'Plan', detail: 'rozbicie etapu na kroki i kryteria odbioru (agent planner)' },
    { title: 'Implementacja', detail: 'pierwsza pętla developera: Sonnet, testy najpierw', model: 'sonnet' },
    { title: 'Review', detail: 'kod + zgodność z planem; poprawki: Opus, najwyżej 2 rundy', model: 'opus' },
    { title: 'Check', detail: 'npm run check; naprawa: Opus, najwyżej 2 próby', model: 'opus' },
    { title: 'Dziennik', detail: 'wpis w PLAN_WDROZENIA.md, rozdz. 15' },
    { title: 'Commit', detail: 'commit etapu (bez push), tylko przy zielonym npm run check' },
  ],
}

// ---------------------------------------------------------------------------
// args = {
//   from: "E3", to: "E7",                 // zakres etapów (domyślnie E3–E7)
//   notes: { E3: "...", E4: "..." },      // wskazówki i decyzje dla etapów
// }
// Dla zgodności: { stage: "E3", notes: "..." } uruchamia jeden etap.
//
// Pętla developera: implementacja (runda 1) na Sonnecie, każda kolejna runda
// poprawek (po review i po npm run check) na Opusie.
// Po każdym etapie: wpis w dzienniku i commit (bez push), potem następny etap.
// Zatrzymanie: czerwone npm run check po próbach naprawy, brak commitu, albo
// E4 bez decyzji użytkownika o read-only (notes.E4 musi ją zawierać).
// ---------------------------------------------------------------------------
const ALL_STAGES = ['E3', 'E4', 'E5', 'E6', 'E7']
const single = args && args.stage
const from = (args && args.from) || single || 'E3'
const to = (args && args.to) || single || 'E7'
const stages = ALL_STAGES.slice(ALL_STAGES.indexOf(from), ALL_STAGES.indexOf(to) + 1)
const notesFor = (stage) => {
  const n = args && args.notes
  if (!n) return ''
  if (typeof n === 'string') return single === stage ? n : ''
  return n[stage] || ''
}

const FIRST_DEV_MODEL = 'sonnet'
const FIX_DEV_MODEL = 'opus'
const MAX_REVIEW_ROUNDS = 2
const MAX_CHECK_ATTEMPTS = 2

if (!stages.length) return { error: `Nieprawidłowy zakres etapów: ${from}–${to}.` }
log(`Etapy do realizacji: ${stages.join(', ')}. Commit po każdym etapie, bez push.`)

const context = (stage) => `Repozytorium: C:\\Projekty\\Private\\to-do-track (TukTuk Track, Tauri 2 + Rust + Vue 3).
Moduł Workflows: plan w docs/workflows/PLAN_WDROZENIA.md (etapy w rozdz. 10, dziennik realizacji w rozdz. 15),
decyzje E0 w docs/workflows/E0_ANALIZA.md. Kod: src-tauri/crates/workflow-core (domena bez Tauri),
src-tauri/src/workflows/ (host: store.rs, komendy), src/workflows.ts, src/api/workflow*.ts, src/preview/workflows.ts,
src/views/WorkflowsView.vue, src/components/workflows/. Etap do realizacji: ${stage}.
${notesFor(stage) ? 'Wskazówki i decyzje użytkownika dla tego etapu: ' + notesFor(stage) : ''}
Twarde zasady:
- Testy i prace NIE łączą się z prawdziwymi claude/codex/orca, Jirą, Redmine, TS ani kalendarzem; tylko procesy zastępcze i fixture.
- Klucz dostępu Workflows nie trafia do żadnego pliku (w kodzie jest tylko jego SHA-256).
- Logika przeniesiona 1:1 z Rust do TS (walidator, szablony, fixture tests/fixtures/tauri.ts) zmienia się razem z Rust.
- Długie operacje i odczyt strumieni nie trzymają blokady bazy (Database(Mutex<Connection>)); runtime na wątkach std i mpsc, bez tokio.
- Commity robi tylko krok Commit; nigdy push, nigdy --no-verify, nigdy npm run ship (wydanie podpisuje użytkownik).
- cargo: w Bash najpierw export PATH="$USERPROFILE/.cargo/bin:$PATH".
- Testy z zamrożonym zegarem (page.clock.setFixedTime): Vue pomija handler przodka z tym samym znacznikiem czasu — przy Esc w oknach przesuń zegar po otwarciu okna.`

const PLAN_SCHEMA = {
  type: 'object',
  properties: {
    summary: { type: 'string' },
    steps: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          title: { type: 'string' },
          files: { type: 'array', items: { type: 'string' } },
          details: { type: 'string' },
          tests: { type: 'string' },
        },
        required: ['title', 'files', 'details', 'tests'],
      },
    },
    acceptance: { type: 'array', items: { type: 'string' } },
    outOfScope: { type: 'array', items: { type: 'string' } },
    targetedTests: { type: 'array', items: { type: 'string' } },
  },
  required: ['summary', 'steps', 'acceptance', 'outOfScope', 'targetedTests'],
}

const DEV_SCHEMA = {
  type: 'object',
  properties: {
    summary: { type: 'string' },
    filesChanged: { type: 'array', items: { type: 'string' } },
    testsRun: { type: 'array', items: { type: 'string' } },
    testsPassing: { type: 'boolean' },
    notDone: { type: 'array', items: { type: 'string' } },
  },
  required: ['summary', 'filesChanged', 'testsRun', 'testsPassing', 'notDone'],
}

const REVIEW_SCHEMA = {
  type: 'object',
  properties: {
    findings: {
      type: 'array',
      items: {
        type: 'object',
        properties: {
          severity: { type: 'string', enum: ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] },
          file: { type: 'string' },
          line: { type: 'number' },
          issue: { type: 'string' },
          fix: { type: 'string' },
        },
        required: ['severity', 'file', 'issue', 'fix'],
      },
    },
  },
  required: ['findings'],
}

const CHECK_SCHEMA = {
  type: 'object',
  properties: {
    passed: { type: 'boolean' },
    summary: { type: 'string' },
    failures: { type: 'array', items: { type: 'string' } },
    environmental: { type: 'boolean' },
  },
  required: ['passed', 'summary', 'failures', 'environmental'],
}

const COMMIT_SCHEMA = {
  type: 'object',
  properties: {
    committed: { type: 'boolean' },
    hash: { type: 'string' },
    message: { type: 'string' },
    problem: { type: 'string' },
  },
  required: ['committed', 'hash', 'message', 'problem'],
}

async function runStage(stage) {
  const CONTEXT = context(stage)

  // --- Plan -----------------------------------------------------------------
  const plan = await agent(
    `${CONTEXT}

Przygotuj plan realizacji etapu ${stage}. Przeczytaj rozdział 10 (etap ${stage}: lista zadań i "Odbiór"),
rozdziały, do których się odnosi (np. 5 dla silnika, 7 dla danych, 8 dla runnerów, 11 dla testów),
cały dziennik w rozdz. 15 (co już zrobiono i jakie decyzje podjęto) oraz istniejący kod modułu.
Nie zmieniaj plików. Zwróć: kroki w kolejności wykonania (pliki, szczegóły, testy do napisania najpierw),
kryteria odbioru możliwe do sprawdzenia testem, rzeczy świadomie poza zakresem (np. wymagające prawdziwych CLI
albo decyzji użytkownika) i listę ukierunkowanych komend testowych (np. cargo test -p workflow-core,
cargo test --manifest-path src-tauri/Cargo.toml --bin tuktuk-track workflows, npx playwright test tests/<plik>.spec.ts).`,
    { label: `plan:${stage}`, phase: 'Plan', schema: PLAN_SCHEMA, agentType: 'planner' },
  )
  if (!plan) return { stage, stop: true, error: 'Planowanie nie zwróciło wyniku.' }
  log(`${stage} plan: ${plan.steps.length} kroków, ${plan.acceptance.length} kryteriów odbioru.`)

  const planText = [
    `Podsumowanie: ${plan.summary}`,
    'Kroki:',
    ...plan.steps.map((s, i) => `${i + 1}. ${s.title}\n   pliki: ${s.files.join(', ')}\n   ${s.details}\n   testy: ${s.tests}`),
    'Kryteria odbioru:',
    ...plan.acceptance.map((a) => `- ${a}`),
    'Poza zakresem:',
    ...plan.outOfScope.map((o) => `- ${o}`),
    `Ukierunkowane testy: ${plan.targetedTests.join(' ; ')}`,
  ].join('\n')

  // --- Implementacja: pierwsza pętla developera (Sonnet) ----------------------
  const dev = await agent(
    `${CONTEXT}

Zrealizuj etap ${stage} według planu poniżej. Pracuj w bieżącym katalogu (jesteś jedynym piszącym).
Dla każdego kroku: najpierw test (RED), potem implementacja (GREEN), potem porządek. Po każdym kroku uruchom
ukierunkowane testy; na końcu wszystkie z listy. Formatuj Rust: cargo fmt --manifest-path src-tauri/Cargo.toml --all.
Typecheck frontendu: npx vue-tsc --noEmit. NIE uruchamiaj pełnego npm run check ani całego Playwright (zrobi to osobny krok).
Jeśli czegoś nie da się zrobić bez prawdziwych CLI albo decyzji użytkownika — pomiń i wpisz do notDone.

PLAN:
${planText}`,
    { label: `dev:${stage}:sonnet`, phase: 'Implementacja', schema: DEV_SCHEMA, model: FIRST_DEV_MODEL },
  )
  if (!dev) return { stage, plan, stop: true, error: 'Implementacja nie zwróciła wyniku.' }
  log(`${stage} implementacja (Sonnet): ${dev.filesChanged.length} plików, testy ${dev.testsPassing ? 'zielone' : 'CZERWONE'}.`)

  // --- Review + kolejne pętle developera (Opus) -------------------------------
  const reviewers = [
    {
      key: 'kod',
      agentType: 'code-reviewer',
      prompt: `Zrób review niezacommitowanych zmian (git status, git diff; nowe pliki są nieśledzone) dla etapu ${stage}.
Uwzględnij język: Rust (własność, błędy Result, blokady bazy, wątki i kanały, timeouty, procesy), Vue/TS (reaktywność,
sprzątanie nasłuchów, dostępność), bezpieczeństwo (ścieżki, procesy, sekrety), ciche połykanie błędów.
Tylko konkretne problemy z plikiem i linią. Nie zmieniaj plików, nie uruchamiaj npm/Playwright.`,
    },
    {
      key: 'plan',
      agentType: 'general-purpose',
      prompt: `Sprawdź zgodność niezacommitowanych zmian z planem etapu ${stage} i kryteriami odbioru poniżej.
Dla każdego kryterium: czy jest test, który go dowodzi (wskaż plik i nazwę testu)? Brak testu dla kryterium = HIGH.
Sprzeczność z PLAN_WDROZENIA.md (rozdz. 5–9) albo E0_ANALIZA.md = HIGH. Nie zmieniaj plików, nie uruchamiaj npm/Playwright.

${planText}`,
    },
  ]

  let openFindings = []
  for (let round = 1; round <= MAX_REVIEW_ROUNDS + 1; round++) {
    const results = await parallel(
      reviewers.map((r) => () =>
        agent(`${CONTEXT}\n\n${r.prompt}`, {
          label: `review:${stage}:${r.key}:${round}`,
          phase: 'Review',
          schema: REVIEW_SCHEMA,
          agentType: r.agentType,
        }),
      ),
    )
    const findings = results.filter(Boolean).flatMap((r) => r.findings)
    openFindings = findings.filter((f) => f.severity === 'CRITICAL' || f.severity === 'HIGH')
    const medium = findings.filter((f) => f.severity === 'MEDIUM')
    log(`${stage} review runda ${round}: ${openFindings.length} CRITICAL/HIGH, ${medium.length} MEDIUM.`)
    if (!openFindings.length && !medium.length) break
    if (round > MAX_REVIEW_ROUNDS) {
      log(`${stage}: limit ${MAX_REVIEW_ROUNDS} rund poprawek wyczerpany; pozostałe uwagi idą do dziennika.`)
      break
    }
    const toFix = [...openFindings, ...medium]
    await agent(
      `${CONTEXT}

Popraw uwagi z review etapu ${stage} (runda ${round}). Każdą CRITICAL/HIGH popraw; MEDIUM popraw, jeśli to
nie zmienia zakresu etapu. Do każdej poprawki błędu dodaj albo zmień test, który by go złapał. Uruchom ukierunkowane
testy (${plan.targetedTests.join(' ; ')}) i cargo fmt --all. Nie uruchamiaj pełnego npm run check.

UWAGI:
${toFix.map((f, i) => `${i + 1}. [${f.severity}] ${f.file}${f.line ? ':' + f.line : ''} — ${f.issue}\n   Proponowana poprawka: ${f.fix}`).join('\n')}`,
      { label: `fix:${stage}:review:${round}:opus`, phase: 'Review', schema: DEV_SCHEMA, model: FIX_DEV_MODEL },
    )
  }

  // --- Check: pełne npm run check, naprawa na Opusie --------------------------
  let check = null
  for (let attempt = 1; attempt <= MAX_CHECK_ATTEMPTS + 1; attempt++) {
    check = await agent(
      `${CONTEXT}

Uruchom w katalogu repozytorium: npm run check (build, cały Playwright, cargo fmt --check, cargo test --locked).
To jedyny proces testowy w tej chwili — nie uruchamiaj niczego równolegle. Trwa 3–8 minut; użyj timeoutu 900000 ms.
Zwróć: passed, krótkie podsumowanie liczb, listę nieudanych testów z pierwszą linią błędu. environmental=true, gdy
wszystkie błędy to przekroczenia czasu przy otwieraniu stron (obciążenie maszyny), a nie asercje. Niczego nie poprawiaj.`,
      { label: `check:${stage}:${attempt}`, phase: 'Check', schema: CHECK_SCHEMA, effort: 'low' },
    )
    if (!check || check.passed) break
    if (attempt > MAX_CHECK_ATTEMPTS) break
    if (check.environmental) {
      log(`${stage}: błędy wyglądają na środowiskowe (timeouty) — ponowne uruchomienie bez poprawek.`)
      continue
    }
    await agent(
      `${CONTEXT}

npm run check nie przechodzi po etapie ${stage}. Napraw przyczyny (kod, nie testy — chyba że test sprawdza
zachowanie, które etap świadomie zmienił; wtedy zaktualizuj test i powiedz dlaczego). Uruchom ponownie tylko
nieudane pliki testów. Nie uruchamiaj pełnego npm run check.

BŁĘDY:
${check.failures.join('\n')}`,
      { label: `fix:${stage}:check:${attempt}:opus`, phase: 'Check', schema: DEV_SCHEMA, model: FIX_DEV_MODEL },
    )
  }
  const green = !!(check && check.passed)

  // --- Dziennik ---------------------------------------------------------------
  await agent(
    `${CONTEXT}

Dopisz na końcu docs/workflows/PLAN_WDROZENIA.md (rozdz. 15) wpis "### <data z git log -1 --format=%cs> — ${stage}"
oraz zaznacz w rozdz. 10 checkboxy etapu ${stage}, które mają dowód w testach (zgodnie z rozdz. 14: bez dowodu
nie zaznaczaj). Wpis: zmienione pliki (git status), decyzje, wynik testów (poniżej), pozostałe blokery.
Pisz po polsku, rzeczowo. Zmieniaj tylko ten dokument.

Wynik npm run check: ${check ? (check.passed ? 'zielone' : 'NIE przechodzi') + ' — ' + check.summary : 'brak wyniku'}
Niezałatwione uwagi review: ${openFindings.length ? openFindings.map((f) => `[${f.severity}] ${f.file}: ${f.issue}`).join('; ') : 'brak'}
Pominięte w implementacji: ${dev.notDone.length ? dev.notDone.join('; ') : 'nic'}`,
    { label: `dziennik:${stage}`, phase: 'Dziennik', model: FIRST_DEV_MODEL, effort: 'low' },
  )

  const result = {
    stage,
    plan: { summary: plan.summary, steps: plan.steps.map((s) => s.title), acceptance: plan.acceptance, outOfScope: plan.outOfScope },
    notDone: dev.notDone,
    openFindings,
    check,
  }
  if (!green) {
    log(`${stage}: npm run check nie przechodzi — bez commitu, workflow się zatrzymuje.`)
    return { ...result, committed: false, stop: true }
  }

  // --- Commit -----------------------------------------------------------------
  const commit = await agent(
    `${CONTEXT}

Zrób commit etapu ${stage} (npm run check właśnie przeszedł). Kroki:
1. git status — dodaj wszystkie zmiany etapu (zmienione i nowe pliki), ale NIE .claude/launch.json
   (lokalna konfiguracja podglądu) ani plików tymczasowych (test-results/, playwright-report/, *.log).
2. Sprawdź, że w zmianach nie ma prawdziwego klucza Workflows ani innych sekretów (git diff --cached).
3. git commit z wiadomością w formacie "feat: Workflows ${stage} — <krótki opis po polsku>" i treścią:
   3–6 punktów, co etap dodaje (na podstawie wpisu w dzienniku). Bez --no-verify. Bez push.
4. Jeśli hook odrzuci commit — popraw przyczynę (np. formatowanie) i spróbuj raz jeszcze; jeśli dalej nie
   przechodzi, nie obchodź hooka, tylko zwróć committed=false i opis problemu.
Zwróć hash (git log -1 --format=%h) i pierwszą linię wiadomości.`,
    { label: `commit:${stage}`, phase: 'Commit', schema: COMMIT_SCHEMA, model: FIRST_DEV_MODEL, effort: 'low' },
  )
  const committed = !!(commit && commit.committed)
  log(committed ? `${stage}: commit ${commit.hash} — ${commit.message}` : `${stage}: commit się nie udał — workflow się zatrzymuje.`)
  return { ...result, committed, commit, stop: !committed }
}

// --- Kolejne etapy ------------------------------------------------------------
const report = []
for (const stage of stages) {
  if (stage === 'E4' && !notesFor('E4')) {
    log('Zatrzymanie przed E4: brak decyzji użytkownika o read-only dla Claude (args.notes.E4).')
    report.push({ stage: 'E4', skipped: true, reason: 'Brak decyzji o read-only dla Claude w args.notes.E4 (bloker 1 w E0_ANALIZA.md).' })
    break
  }
  phase('Plan')
  log(`=== ${stage} ===`)
  const result = await runStage(stage)
  report.push(result)
  if (result.stop) break
}

return { stages: report }
