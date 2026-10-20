#!/usr/bin/env node
// Pomocnik skilla /backlog-run: ta sama lista zadań i te same prompty co workflow
// .claude/workflows/backlog.js (jedno źródło prawdy), ale bez narzędzia Workflow.
// Sesja prowadząca uruchamia subagentów (narzędzie Agent) z poleceniem „wykonaj
// prompt z `node backlog.mjs prompt …`”; subagent zapisuje wynik JSON do pliku,
// a pomocnik czyta te pliki, decyduje o następnym kroku i prowadzi stan.
//
// Polecenia (wszystkie wypisują krótki tekst albo JSON):
//   status                         — kolejność zadań, ukończone, następne
//   init [--from ID] [--until ID] [--only ID,ID] [--rounds N]
//                                  — nowe uruchomienie (nadpisuje kolejkę, zostawia wyniki)
//   prompt <etap> <ID> [argument]  — pełny prompt dla subagenta (etapy niżej)
//   next <ID>                      — co zrobić dalej z zadaniem (na podstawie plików wyników)
//   done <ID> | block <ID> <powód> — zapis w stanie
//
// Etapy promptu: impl, verify <runda-etapu> (np. impl-1), fix <runda-etapu>,
// review <soczewka>, apply, release.
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'node:fs'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const REPO = resolve(HERE, '..', '..', '..')
const WORKFLOW = join(REPO, '.claude', 'workflows', 'backlog.js')
const RUN = join(REPO, '.claude', 'backlog-run')
const STATE = join(RUN, 'state.json')

// Wczytanie definicji z workflow: wszystko przed główną pętlą, bez `export`.
function loadWorkflow() {
  const src = readFileSync(WORKFLOW, 'utf8')
  const cut = src.indexOf('\nconst opts = args || {}')
  if (cut < 0) throw new Error('Nie znaleziono głównej pętli w backlog.js (const opts = args || {}).')
  const body = src.slice(0, cut).replace(/^export const meta/m, 'const meta')
  const fn = new Function('args', 'agent', 'parallel', 'log', 'phase',
    `${body}\nreturn { meta, TASKS, IMPL, VERIFY, REVIEW, ROOT, BRANCH, PLAN, implementPrompt, releasePrompt, verifyPrompt, fixPrompt, reviewPrompt, applyReviewPrompt }`)
  return fn({}, null, null, () => {}, () => {})
}
const W = loadWorkflow()

const readJson = (p, fallback) => { try { return JSON.parse(readFileSync(p, 'utf8').replace(/^﻿/, '')) } catch { return fallback } }
const writeJson = (p, v) => { mkdirSync(dirname(p), { recursive: true }); writeFileSync(p, JSON.stringify(v, null, 2) + '\n', 'utf8') }
const state = () => readJson(STATE, null)
const task = id => {
  const t = W.TASKS.find(x => x.id === id)
  if (!t) throw new Error(`Nieznane zadanie ${id}. Zadania: ${W.TASKS.map(x => x.id).join(', ')}`)
  return t
}
const dir = id => join(RUN, id)
const file = (id, name) => join(dir(id), `${name}.json`)
const lenses = t => ['plan', 'bugs'].concat(t.ai ? ['privacy'] : [], t.migration ? ['migration'] : [])

function resultInstruction(path, schema, readOnly) {
  return `

WYNIK (obowiązkowe): ${readOnly ? 'Jedyny plik, który wolno Ci zapisać, to plik wyniku poniżej. ' : 'Plik wyniku leży poza worktree (to jedyny wyjątek od zasady pracy tylko w worktree) i nie jest częścią repozytorium — nie commituj go. '}Na końcu zapisz obiekt JSON zgodny ze schematem do pliku:
${path}
(sam JSON, UTF-8, bez komentarzy). Odpowiedz potem jednym zdaniem podsumowania; nie wklejaj JSON-a do odpowiedzi.
Schemat: ${JSON.stringify(schema)}`
}

// Uwagi do rundy poprawek, jak w verifyWithFixes workflow.
function failuresWithExtras(v) {
  const extra = [
    v.uncommitted ? 'UWAGA: są niezacommitowane zmiany, dokończ commit.' : '',
    v.noNetwork === false ? 'UWAGA: weryfikator znalazł ścieżkę łączącą się z prawdziwym serwerem — usuń ją; testy nie mogą wychodzić do sieci.' : '',
    v.noRealTools === false ? 'UWAGA: test uruchamia prawdziwe claude/codex/orca albo używa prawdziwych katalogów użytkownika — podstaw proces zastępczy i katalog tymczasowy.' : '',
    v.testsIntact === false ? 'UWAGA: istniejące testy zmieniły się poza dozwolonym zakresem — przywróć je i napraw kod.' : '',
  ].filter(Boolean).join('\n')
  return (v.failures || '') + (extra ? `\n\n${extra}` : '')
}

const findings = id => lenses(task(id))
  .map(l => readJson(file(id, `review-${l}`), null))
  .filter(Boolean)
  .flatMap(r => r.findings || [])
  .filter(f => f.severity !== 'nit')

function prompt(stage, id, arg) {
  const t = task(id)
  const s = state() || {}
  mkdirSync(dir(id), { recursive: true })
  switch (stage) {
    case 'impl':
      return W.implementPrompt(t) + resultInstruction(file(id, 'impl'), W.IMPL)
    case 'release':
      return W.releasePrompt(t, (s.pendingNotes || []).join('\n')) + resultInstruction(file(id, 'impl'), W.IMPL)
    case 'verify': {
      if (!arg) throw new Error('verify wymaga rundy, np. impl-1')
      return W.verifyPrompt(t) + resultInstruction(file(id, `verify-${arg}`), W.VERIFY, true)
    }
    case 'fix': {
      if (!arg) throw new Error('fix wymaga rundy weryfikacji, np. impl-1')
      const v = readJson(file(id, `verify-${arg}`), null)
      if (!v) throw new Error(`Brak pliku ${file(id, `verify-${arg}`)}`)
      const round = Number(arg.split('-').pop())
      return W.fixPrompt(t, failuresWithExtras(v), round) + resultInstruction(file(id, `fix-${arg}`), W.IMPL)
    }
    case 'review': {
      if (!lenses(t).includes(arg)) throw new Error(`Soczewki dla ${id}: ${lenses(t).join(', ')}`)
      const impl = readJson(file(id, 'impl'), {})
      return W.reviewPrompt(t, arg, impl) + resultInstruction(file(id, `review-${arg}`), W.REVIEW, true)
    }
    case 'apply':
      return W.applyReviewPrompt(t, findings(id)) + resultInstruction(file(id, 'apply'), W.IMPL)
    default:
      throw new Error(`Nieznany etap ${stage}`)
  }
}

// Następny krok dla zadania na podstawie istniejących plików wyników.
function next(id) {
  const t = task(id)
  const s = state()
  const max = (s && s.rounds) || 3
  const has = n => existsSync(file(id, n))
  const get = n => readJson(file(id, n), null)
  const verifyStep = stageLabel => {
    for (let r = 1; r <= max + 1; r++) {
      const vn = `verify-${stageLabel}-${r}`
      if (!has(vn)) return { action: 'verify', arg: `${stageLabel}-${r}` }
      const v = get(vn)
      if (v.ok) return null
      if (r > max) return { action: 'block', reason: `weryfikacja (${stageLabel}): wyczerpano rundy poprawek — ${String(v.failures || '').slice(0, 400)}` }
      const fn = `fix-${stageLabel}-${r}`
      if (!has(fn)) return { action: 'fix', arg: `${stageLabel}-${r}` }
      const f = get(fn)
      if (f.status === 'blocked') return { action: 'block', reason: `poprawki (${stageLabel}-${r}): ${f.blocker || 'blocked'}` }
    }
    return { action: 'block', reason: 'wyczerpano rundy poprawek' }
  }

  if (!has('impl')) return { action: t.release ? 'release' : 'impl' }
  const impl = get('impl')
  if (impl.status === 'blocked') return { action: 'block', reason: impl.blocker || 'implementator: blocked' }
  const first = verifyStep(t.release ? 'release' : 'impl')
  if (first) return first
  if (t.release) return { action: 'done' }

  const missing = lenses(t).filter(l => !has(`review-${l}`))
  if (missing.length) return { action: 'review', lenses: missing, note: 'uruchom wszystkie soczewki równolegle (jedna wiadomość, kilka wywołań Agent)' }
  if (!findings(id).length) return { action: 'done' }
  if (!has('apply')) return { action: 'apply', findings: findings(id).length }
  const second = verifyStep('review')
  if (second) return second
  return { action: 'done' }
}

function order(opts) {
  let q = W.TASKS.map(t => t.id)
  if (opts.only) q = q.filter(id => opts.only.includes(id))
  else if (opts.from) q = q.slice(Math.max(0, q.indexOf(opts.from)))
  if (opts.until && q.includes(opts.until)) q = q.slice(0, q.indexOf(opts.until) + 1)
  return q
}

const [cmd, ...rest] = process.argv.slice(2)
const flag = name => { const i = rest.indexOf(`--${name}`); return i >= 0 ? rest[i + 1] : undefined }
try {
  if (cmd === 'init') {
    const prev = state() || {}
    const opts = { from: flag('from'), until: flag('until'), only: flag('only') && flag('only').split(','), rounds: Number(flag('rounds') || prev.rounds || 3) }
    const queue = order(opts)
    writeJson(STATE, { root: W.ROOT, branch: W.BRANCH, rounds: opts.rounds, queue, done: (prev.done || []).filter(id => queue.includes(id)), blocked: null, pendingNotes: prev.pendingNotes || [], log: prev.log || [] })
    console.log(`Kolejka: ${queue.join(', ')} (rundy poprawek: ${opts.rounds}). Wyniki: ${RUN}`)
  } else if (cmd === 'status') {
    const s = state()
    if (!s) { console.log(`Brak stanu. Zadania: ${W.TASKS.map(t => t.id).join(', ')}. Zacznij od: node "${join(HERE, 'backlog.mjs')}" init`); process.exit(0) }
    const nextId = s.queue.find(id => !s.done.includes(id))
    console.log(JSON.stringify({ root: s.root, branch: s.branch, queue: s.queue, done: s.done, blocked: s.blocked, next: nextId || null, nextStep: nextId ? next(nextId) : null }, null, 2))
  } else if (cmd === 'prompt') {
    const [stage, id, arg] = rest
    process.stdout.write(prompt(stage, id, arg) + '\n')
  } else if (cmd === 'next') {
    console.log(JSON.stringify(next(rest[0])))
  } else if (cmd === 'lenses') {
    console.log(lenses(task(rest[0])).join(' '))
  } else if (cmd === 'done') {
    const id = rest[0]; const t = task(id); const s = state()
    const n = next(id)
    if (n.action !== 'done') throw new Error(`${id} nie jest gotowe: ${JSON.stringify(n)}`)
    const impl = readJson(file(id, 'impl'), {})
    if (t.release) s.pendingNotes = []
    else {
      const note = [impl.readme ? `README: ${impl.readme}` : '', impl.deviations ? `odstępstwa: ${impl.deviations}` : '', impl.manualChecks ? `do ręcznego sprawdzenia: ${impl.manualChecks}` : ''].filter(Boolean).join(' | ')
      if (note) s.pendingNotes.push(`- ${id}: ${note}`)
    }
    if (!s.done.includes(id)) s.done.push(id)
    s.blocked = null
    s.log.push(`✔ ${id}: ${impl.summary || ''}`.slice(0, 600))
    writeJson(STATE, s)
    const nextId = s.queue.find(x => !s.done.includes(x))
    console.log(`✔ ${id} zapisane. Następne: ${nextId || 'brak — kolejka ukończona'}`)
  } else if (cmd === 'block') {
    const [id, ...why] = rest; task(id); const s = state()
    s.blocked = { task: id, reason: why.join(' ') }
    s.log.push(`⛔ ${id}: ${why.join(' ')}`.slice(0, 600))
    writeJson(STATE, s)
    console.log(`⛔ ${id} zablokowane. Po naprawie usuń pliki wyników kroku, który chcesz powtórzyć (${dir(id)}), i wznów /backlog-run.`)
  } else if (cmd === 'files') {
    const id = rest[0]
    console.log(existsSync(dir(id)) ? readdirSync(dir(id)).join('\n') : '(brak)')
  } else {
    console.log('Użycie: node backlog.mjs status | init [--from ID] [--until ID] [--only A,B] [--rounds N] | prompt <etap> <ID> [arg] | next <ID> | lenses <ID> | files <ID> | done <ID> | block <ID> <powód>')
  }
} catch (e) {
  console.error(`Błąd: ${e.message}`)
  process.exit(1)
}
