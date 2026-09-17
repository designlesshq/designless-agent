// A missed quick check is not a finding.
//
// Three sessions read the 700ms miss as an outage and reported it to the
// founder as one (Brain 9d398889; 2026-09-17 twice). The lines that report a
// miss now say what it means, from one sentence, and no line reports a miss
// without it. This is a guard on the consumed surface: it renders each line
// the way its hook does and looks for the sentence in the output, not for an
// import.
//
// Run: node --test hooks/missed-hint.test.mjs
import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import { MISSED_HINT } from './inbox-probe.mjs'
import { step } from './inbox-watch.mjs'

const here = new URL('.', import.meta.url)
const read = (rel) => fs.readFileSync(new URL(rel, here), 'utf8')

test('the sentence says it is designed, and says there is nothing to report', () => {
  assert.match(MISSED_HINT, /By design, not a fault/)
  assert.match(MISSED_HINT, /nothing to fix or report/)
  assert.ok(MISSED_HINT.length <= 60, 'a clause, not a lesson: the watcher line has a byte ceiling')
  assert.doesNotMatch(MISSED_HINT, /—/, 'no em dash')
})

test('the watcher\'s blind line carries it', () => {
  const { line } = step({ unknown: 'timeout after 700ms', sessions: [] }, { digest: '', blind: false, blindSince: null }, '/x', 1000)
  assert.ok(line, 'the first miss speaks')
  assert.ok(line.includes(MISSED_HINT), line)
})

test('WIRING: every hook line that reports a miss carries it, first time and after', () => {
  const wake = read('./canvas-wake.mjs')
  const branch = wake.slice(wake.indexOf('if (unknown) {'), wake.indexOf('clearUnknown(hookSessionId)'))
  // Both arms of the ternary end in the clause: count the interpolations on
  // the branch, one per arm, rather than parsing template literals that a
  // long line splits in two.
  assert.equal((branch.match(/\$\{MISSED_HINT\}/g) ?? []).length, 2, 'the first-time line and the repeat line')
  const start = read('./session-start-inbox.mjs')
  const sb = start.slice(start.indexOf('if (unknown) {'), start.indexOf('return', start.indexOf('if (unknown) {')))
  assert.match(sb, /\$\{MISSED_HINT\}/, 'the session-start line')
})

test('no blind line calls the desktop unreachable: the word is what invited the diagnosis', () => {
  // The watcher line is rendered, because it is split across two template
  // literals and a source grep read only the first (a mutation slipped past
  // that way, 2026-09-17). The hook lines are read from their unknown branch.
  const { line } = step({ unknown: 'timeout after 700ms', sessions: [] }, { digest: '', blind: false, blindSince: null }, '/x', 1000)
  assert.doesNotMatch(line, /unreachable/, line)
  for (const f of ['./canvas-wake.mjs', './session-start-inbox.mjs']) {
    const src = read(f)
    const at = src.indexOf('if (unknown) {')
    const branch = src.slice(at, src.indexOf('return', at))
    assert.doesNotMatch(branch, /unreachable/, `${f}: the unknown branch`)
  }
})

test('WIRING: the skill carries the same ruling', () => {
  const skill = read('../skills/orchestrator/SKILL.md')
  assert.match(skill, /A missed quick check is the designed behaviour of a deliberately short budget/)
  assert.match(skill, /never a fault to diagnose, a finding to report, or a thing to mention to the user/)
})
