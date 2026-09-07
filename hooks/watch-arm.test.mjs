/**
 * The watcher gets started, without anyone remembering to start it.
 *
 * THE DEFECT this closes: the watcher existed only as a line in the skill, so it
 * needed the skill loaded (which needs the command) AND the agent to remember
 * the rule at the right moment. In practice it was armed almost never, and edits
 * made while the agent sat idle waited for the user's next message. The canvas
 * promises live editing; a watcher nobody starts does not deliver it.
 *
 * A hook cannot start a background task, so the ask is still an ask. What is
 * tested here is the property that makes an ask reliable: it repeats until the
 * watcher itself records that it is running.
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { canvasInPlay, decide, armLine } from './canvas-arm-watch.mjs'
import { isArmed, arm, disarm, beat, STALE_MS } from './watch-marker.mjs'
import { drainDigest, step , shouldStandDown } from './inbox-watch.mjs'

const NEVER = () => false
const ALWAYS = () => true

test('any canvas tool means a canvas is in play', () => {
  for (const t of [
    'mcp__plugin_designless_less-mcp__less_canvas_compose',
    'mcp__plugin_designless_less-mcp__less_canvas_update',
    'mcp__plugin_designless_less-mcp__less_canvas_status',
    'mcp__plugin_designless_less-mcp__less_canvas_walkplan',
  ]) assert.equal(canvasInPlay(t, ''), true, t)
})

// Founder ruling 2026-09-08: ANY Designless tool, not the canvas family alone.
// Arming only at the first canvas call means the edits made before it are the
// ones that wait, which is the thing the watcher exists to stop.
test('any Designless tool is reason to want a watcher, not just the canvas ones', () => {
  for (const t of [
    'mcp__plugin_designless_less-mcp__less_artefact_open',
    'mcp__plugin_designless_less-mcp__less_list_templates',
    'mcp__plugin_designless_less-mcp__less_resolve_brand',
    'less_whoami',
  ]) assert.equal(canvasInPlay(t, ''), true, t)
})

// The boundary is an underscore, not a bare substring, so a name that merely
// ENDS in "less" is not a Designless tool.
test('a lookalike name is not a Designless tool', () => {
  assert.equal(canvasInPlay('harmless_thing', ''), false)
  assert.equal(canvasInPlay('mcp__other__stateless_probe', ''), false)
})

// The one exception, and the reason for it: this call runs at the start of every
// turn whether or not the user has ever opened a canvas. Treating it like the
// rest would ask for a watcher in sessions with nothing to watch.
test('the inbox read counts only when it comes back naming a session', () => {
  const name = 'mcp__plugin_designless_less-mcp__less_canvas_inbox'
  assert.equal(canvasInPlay(name, '# Designless: nothing waiting'), false)
  // The other empty answer. It contains the word "waiting" and its one item
  // belongs to the USER, so a positive-only test would arm a watcher that has
  // nothing it may ever act on.
  assert.equal(canvasInPlay(name, '# Designless: nothing to apply, but an edit still waits for the user'), false)
  assert.equal(canvasInPlay(name, '2 edit(s) are waiting on the Designless app for "Deck"'), true)
})

test('a tool that is not a canvas tool is never a reason to ask', () => {
  assert.equal(canvasInPlay('Bash', 'less_canvas_compose'), false)
  assert.equal(canvasInPlay(undefined, ''), false)
})

// THE PROPERTY THAT MAKES THIS WORK. The ask is not remembered and not retried
// by the agent; it is re-emitted by the next canvas call until the watcher
// itself has recorded that it is running.
test('the ask repeats while no watcher is running, and stops when one is', () => {
  const input = { tool_name: 'x_less_canvas_compose', session_id: 's1', tool_response: '' }
  assert.ok(decide(input, NEVER), 'no watcher: ask')
  assert.ok(decide(input, NEVER), 'still no watcher: ask again')
  assert.equal(decide(input, ALWAYS), null, 'watcher running: silence')
})

test('the ask carries the session it is for, and the command to run', () => {
  const line = decide({ tool_name: 'x_less_canvas_compose', session_id: 'abc-123', tool_response: '' }, NEVER)
  assert.match(line, /inbox-watch\.mjs abc-123/, 'the command must be runnable as written')
  assert.match(line, /"Designless Agent"/, 'the label is what the user sees in their task list')
  assert.match(line, /persistent/)
})

test('the ask names the facility whose lines are heard, and says why the other one is silent', () => {
  // A watcher speaks by printing. Under a facility that reports only at exit it
  // prints into silence for the whole session while looking alive, so the ask
  // must name the streaming facility and the reason, or "background task" is
  // read as the silent one.
  const line = armLine('s1')
  assert.match(line, /Monitor tool/, 'on Claude Code the watcher runs under the Monitor tool')
  assert.match(line, /reports only when the process exits/, 'the ask says why a plain background command will not do')
})

test('a hook input with no session id asks for nothing', () => {
  assert.equal(decide({ tool_name: 'x_less_canvas_compose' }, NEVER), null)
})

test('the ask never names the machinery it rides', () => {
  const line = armLine('s1')
  assert.doesNotMatch(line, /\bdrain\b/i, 'a word the user would not understand, in a line an agent may echo')
})

// ── The marker: the one-watcher rule, enforced rather than requested ────────

test('a second watcher cannot claim a session the first holds', () => {
  const id = `test-${process.pid}-a`
  try {
    assert.equal(arm(id), true)
    assert.equal(arm(id), false, 'a sub-agent starting its own must find it taken')
    assert.equal(isArmed(id), true)
  } finally { disarm(id) }
})

// A crashed watcher must not lock the session out of ever having another. It
// stops beating, the marker goes stale on its own, and the next canvas call asks
// again — no cleanup step that a crash is exactly the case that skips.
test('a watcher that stopped beating frees the session', () => {
  const id = `test-${process.pid}-b`
  try {
    arm(id)
    const later = Date.now() + STALE_MS + 1_000
    assert.equal(isArmed(id, later), false, 'a stale marker must not hold the session')
    assert.equal(arm(id, later), true, 'and the next watcher must be able to claim it')
    assert.equal(isArmed(id, later + 1_000), true)
    assert.equal(beat(id, later + 1_000), true)
    assert.equal(isArmed(id, later + STALE_MS), true, 'a beat keeps it alive')
  } finally { disarm(id) }
})

test('an unknown session is never armed', () => {
  assert.equal(isArmed(`test-${process.pid}-never`), false)
  assert.equal(isArmed(undefined), false)
  assert.equal(isArmed(''), false)
})

// ── The watcher's own voice: silence is what keeps it alive ────────────────
//
// A background task that chatters is throttled and then stopped by the host, so
// emitting only on change is not politeness. It is the difference between a
// watcher that lasts the session and one that is killed in the first minutes.

test('new work wakes the agent; the same work does not', () => {
  const cwd = process.cwd()
  const one = [{ session_id: 'a', n_artefact: 2, title: 'Deck' }]
  let st = { digest: '', blind: null }
  let r = step({ sessions: one }, st, cwd)
  assert.ok(r.line, 'the first sighting must wake')
  st = r.next
  assert.equal(step({ sessions: one }, st, cwd).line, null, 'unchanged work must stay silent')
})

test('a second edit on a canvas that already had one is new work', () => {
  const cwd = process.cwd()
  let st = step({ sessions: [{ session_id: 'a', n_artefact: 1, title: 'Deck' }] }, { digest: '', blind: null }, cwd).next
  assert.ok(step({ sessions: [{ session_id: 'a', n_artefact: 2, title: 'Deck' }] }, st, cwd).line,
    'counting sessions instead of edits would sleep through it')
})

test('work that clears, then returns, wakes again', () => {
  const cwd = process.cwd()
  const one = [{ session_id: 'a', n_artefact: 1, title: 'Deck' }]
  let st = step({ sessions: one }, { digest: '', blind: null }, cwd).next
  st = step({ sessions: [] }, st, cwd).next
  assert.ok(step({ sessions: one }, st, cwd).line, 'a cleared inbox must not latch the digest')
})

// The attention items belong to the USER and there is nothing an agent may do
// about them. Waking an idle agent for one wakes it for nothing.
test('items only the user can act on never wake the agent', () => {
  const r = step({ sessions: [{ session_id: 'a', n_needs_human: 3, brand_slug: 'acme' }] }, { digest: '', blind: null }, process.cwd())
  assert.equal(r.line, null)
})

// Cannot see is not all clear — the lesson the wake hook already carries. Said
// once per reason, because a line repeated every 15s for an hour stops being read.
test('a blind watcher says so once, and says it is not an all-clear', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const first = step(blind, { digest: '', blind: null }, cwd)
  assert.match(first.line, /NOT a signal that nothing is waiting/)
  assert.match(first.line, /less_canvas_inbox/, 'it must name the fallback that always works')
  assert.equal(step(blind, first.next, cwd).line, null, 'and not repeat it every poll')
})

// THE FLAP. Clearing the latch on ONE good poll is right for a relapse after a
// healthy stretch and exactly wrong for an accelerator that answers
// intermittently: every answer between two timeouts rearms the line, so an
// intermittent desktop produces one every cycle. Observed live eight times in a
// single session, within an hour of this shipping.
test('an accelerator that flaps is announced once, not once per cycle', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const good = { sessions: [] }
  let st = { digest: '', blind: null, healthy: 0 }
  let spoke = 0
  // bad, good, bad, good, bad ... the shape that produced the noise.
  for (const probe of [blind, good, blind, good, blind, good, blind]) {
    const r = step(probe, st, cwd)
    if (r.line) spoke += 1
    st = r.next
  }
  assert.equal(spoke, 1, 'a flap must not re-announce what it already said')
})

// A genuine recovery still resets it, so a relapse after the desktop has been
// steadily answering is news. That is what the latch was for.
test('sight returning STEADILY makes a later relapse news again', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const good = { sessions: [] }
  let st = step(blind, { digest: '', blind: null, healthy: 0 }, cwd).next
  for (let i = 0; i < 3; i++) st = step(good, st, cwd).next
  assert.ok(step(blind, st, cwd).line, 'a relapse after a healthy stretch must speak')
})

// THE REASON IS NOT THE FACT, and this test used to assert the opposite.
//
// It read as careful: a timeout and a refused socket are different facts with
// different fixes, so a changed reason should speak again. Against a real
// desktop it is useless, because an unreachable one does not fail the same way
// twice. A machine left alone alternated between a timeout and a stale-session
// reply on every poll, so the reason had ALWAYS changed, the latch never held,
// and the watcher spoke every cycle for as long as nobody was there. Fifty-odd
// lines reached one transcript that way.
//
// What the agent needs to know is that the accelerator cannot answer and the
// inbox must be read directly. That is one fact however many ways it is spelled,
// and the spelling still rides in the single line that does get sent.
test('a changed reason is the same blindness, and does not speak again', () => {
  const cwd = process.cwd()
  const reasons = ['timeout after 700ms', 'desktop replied no_session_stale']
  let st = { digest: '', blind: false, blindSince: null, healthy: 0 }
  let spoke = 0
  // The live shape: the reason alternates on every single poll.
  for (let i = 0; i < 12; i++) {
    const r = step({ unknown: reasons[i % 2], sessions: [] }, st, cwd)
    if (r.line) spoke += 1
    st = r.next
  }
  assert.equal(spoke, 1, 'an alternating reason must not defeat the latch')
})

// Half an hour with no desktop means the app is closed or the machine is asleep
// or locked. Nobody is editing a canvas in any of those states, so there is
// nothing between turns left to watch, and a watcher that polls on regardless
// spends the whole idle stretch producing lines the person reads later as spam.
test('a desktop gone for half an hour ends the watch', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const t0 = 1_000_000
  let st = step(blind, { digest: '', blind: false, blindSince: null, healthy: 0 }, cwd, t0).next

  assert.equal(shouldStandDown(st, t0), false, 'it must not stand down the moment it goes blind')
  assert.equal(shouldStandDown(st, t0 + 29 * 60_000), false, 'nor a minute early')
  assert.equal(shouldStandDown(st, t0 + 30 * 60_000), true, 'and must stand down at half an hour')
})

// The clock runs from when the blindness STARTED, not from the last poll, or a
// watcher that keeps failing would keep resetting its own deadline and never
// reach it.
test('the stand-down clock is not reset by continuing to fail', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const t0 = 1_000_000
  let st = step(blind, { digest: '', blind: false, blindSince: null, healthy: 0 }, cwd, t0).next
  for (let i = 1; i <= 20; i++) st = step(blind, st, cwd, t0 + i * 60_000).next
  assert.equal(st.blindSince, t0, 'the start of the blindness must not move')
  assert.equal(shouldStandDown(st, t0 + 30 * 60_000), true)
})

// A desktop that comes back clears the deadline, so a later relapse gets its
// own full half hour rather than standing down immediately.
test('a recovered desktop clears the stand-down clock', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  const good = { sessions: [] }
  const t0 = 1_000_000
  let st = step(blind, { digest: '', blind: false, blindSince: null, healthy: 0 }, cwd, t0).next
  for (let i = 1; i <= 3; i++) st = step(good, st, cwd, t0 + i * 1000).next
  assert.equal(st.blindSince, null, 'steady sight must forget when the blindness began')
  assert.equal(shouldStandDown(st, t0 + 60 * 60_000), false, 'a healthy watcher never stands down')
})

// A watcher that can see must never stand down, whatever else is true: the
// stretch between turns is exactly what it is for.
test('a watcher that can see keeps watching', () => {
  const now = Date.now()
  assert.equal(shouldStandDown({ digest: 'a:1:0:0', healthy: 9, blind: false, blindSince: null }, now), false)
  assert.equal(shouldStandDown({}, now), false)
  assert.equal(shouldStandDown(undefined, now), false)

  // Standing down asks whether it is blind NOW, not whether it ever was. The
  // pair below cannot occur today, because recovery clears both together, and
  // that is the point: if some later edit clears one and forgets the other, a
  // watcher that can see perfectly well must not quietly switch itself off. A
  // silent stand-down is the one failure nobody would report, because the
  // symptom is an absence.
  assert.equal(
    shouldStandDown({ blind: false, blindSince: now - 60 * 60_000 }, now),
    false,
    'having once been blind must never end a watch that can see',
  )
})

test('drainDigest ignores sessions with nothing drainable', () => {
  assert.equal(drainDigest([{ session_id: 'a', n_needs_human: 5 }]), '')
  assert.equal(drainDigest([]), '')
  assert.equal(drainDigest(undefined), '')
})
