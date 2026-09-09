// A quiet canvas and an empty room are different things.
//
// The idle ladder slowed the watcher down when no edits had arrived, and spent
// that answer as though it meant nobody was there. Those come apart in an
// ordinary way: someone working in one session with the canvas quiet looks
// exactly like a laptop left locked. Every armed watcher then drops to a
// five-minute beat while the person sits right there — measured live as six
// edits waiting two to three and a half minutes each, with three watchers on.
//
// These tests hold the two halves apart: presence is evidence a person supplied
// (a submitted prompt), quiet is only the absence of edits, and the ladder is
// allowed to read one as the other in exactly one direction — presence caps the
// backoff, and never the reverse.

import { strict as assert } from 'node:assert'
import test from 'node:test'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { ACTIVE_WINDOW_MS, activityPath, hostIsActive, msSinceActivity, noteActivity } from './host-activity.mjs'
import { pollEveryBeats } from './inbox-watch.mjs'

const minutes = (m) => m * 60_000

// The stamp is a real file in the user's home, so each test restores whatever
// was there. A test suite that ate the running watcher's presence signal would
// be a poor trade for the coverage.
function withSavedStamp(fn) {
  const p = activityPath()
  let saved = null
  try { saved = fs.readFileSync(p, 'utf8') } catch { saved = null }
  try {
    fn()
  } finally {
    try {
      fs.mkdirSync(path.dirname(p), { recursive: true })
      if (saved === null) fs.rmSync(p, { force: true })
      else fs.writeFileSync(p, saved)
    } catch { /* leave it as found is best-effort */ }
  }
}

test('a submitted prompt is presence, and it expires', () => {
  withSavedStamp(() => {
    const now = Date.now()
    assert.equal(noteActivity(now), true)
    assert.ok(msSinceActivity(now) < 50)
    assert.equal(hostIsActive(now), true)
    // Just inside the window is still someone there; just outside is not.
    assert.equal(hostIsActive(now + ACTIVE_WINDOW_MS - 1_000), true)
    assert.equal(hostIsActive(now + ACTIVE_WINDOW_MS + 1_000), false)
  })
})

test('NO stamp reads as nobody there, so the ladder behaves as it always did', () => {
  // The safe default and the honest one. An older plugin, a host with no such
  // hook, a fresh install: presence must be shown, never assumed.
  withSavedStamp(() => {
    fs.rmSync(activityPath(), { force: true })
    assert.equal(msSinceActivity(), null)
    assert.equal(hostIsActive(), false)
  })
})

test('a stamp from the future is treated as now, not as a negative age', () => {
  withSavedStamp(() => {
    const now = Date.now()
    noteActivity(now + minutes(60)) // a clock that moved
    assert.equal(msSinceActivity(now), 0)
    assert.equal(hostIsActive(now), true)
  })
})

test('an unreadable stamp is nobody there, and never throws', () => {
  withSavedStamp(() => {
    fs.mkdirSync(path.dirname(activityPath()), { recursive: true })
    fs.writeFileSync(activityPath(), 'not json at all')
    assert.equal(msSinceActivity(), null)
    assert.equal(hostIsActive(), false)
  })
})

// ── the ladder, which is where this has to actually pay off ─────────────────

test('THE FIX: a long-quiet canvas with someone present polls once a minute, not once every five', () => {
  // The founder's case exactly: three sessions armed, one in use, canvas quiet
  // for the better part of an hour.
  assert.equal(pollEveryBeats(minutes(45), false), 20, 'nobody there: the five-minute rung stands')
  assert.equal(pollEveryBeats(minutes(45), true), 4, 'someone there: capped at a minute')
})

test('presence NEVER speeds the ladder past what quiet already earned', () => {
  // Presence caps the backoff; it is not an accelerator. A busy canvas is
  // already at full rate and must not be pushed anywhere by this, and a
  // stepped-away rung must not be promoted to the top one.
  assert.equal(pollEveryBeats(0, true), 1)
  assert.equal(pollEveryBeats(minutes(4), true), 1)
  assert.equal(pollEveryBeats(minutes(10), true), 4, 'the middle rung is unchanged by presence')
  assert.equal(pollEveryBeats(minutes(10), false), 4)
})

test('the locked-laptop quiet the ladder was built for is untouched', () => {
  // No prompts anywhere means no stamp inside the window, so the cap never
  // applies and an overnight machine keeps the twelve-polls-an-hour steady
  // state that the backoff exists to produce.
  const beatsPerHour = 240
  let polls = 0
  for (let b = beatsPerHour; b < beatsPerHour * 2; b++) {
    const every = pollEveryBeats(b * 15_000, false)
    if (b % every === 0) polls++
  }
  assert.equal(polls, 12)
})

test('the same idle hour with someone present costs a minute-rate, and no more', () => {
  // The price of the fix, stated: 60 polls an hour instead of 12 while a person
  // is actually working. Not the 240 the backoff was built to stop.
  const beatsPerHour = 240
  let polls = 0
  for (let b = beatsPerHour; b < beatsPerHour * 2; b++) {
    const every = pollEveryBeats(b * 15_000, true)
    if (b % every === 0) polls++
  }
  assert.equal(polls, 60)
  assert.ok(polls < beatsPerHour / 2, 'still a fraction of asking every beat')
})
