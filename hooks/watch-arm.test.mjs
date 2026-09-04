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
import { drainDigest, step } from './inbox-watch.mjs'

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

test('sight returning makes a later relapse news again', () => {
  const cwd = process.cwd()
  const blind = { unknown: 'timeout after 700ms', sessions: [] }
  let st = step(blind, { digest: '', blind: null }, cwd).next
  st = step({ sessions: [] }, st, cwd).next
  assert.ok(step(blind, st, cwd).line, 'a relapse after a good poll must speak')
})

test('drainDigest ignores sessions with nothing drainable', () => {
  assert.equal(drainDigest([{ session_id: 'a', n_needs_human: 5 }]), '')
  assert.equal(drainDigest([]), '')
  assert.equal(drainDigest(undefined), '')
})
