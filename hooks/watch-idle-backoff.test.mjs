// The watcher asks less when nothing is happening, and beats just as often.
//
// A machine with the app open and nobody at it asked the desktop 240 times an
// hour, every hour, to be told nothing had changed. The watcher was already
// silent while quiet — it prints only on news — so this was never transcript
// noise; it was a poll nobody needed and no one could see.
//
// The fix has one hazard, and it is the whole reason these two facts are tested
// together. The marker must be refreshed inside STALE_MS or the session frees
// itself and a SECOND watcher starts, doubling every wake for the rest of the
// session. So the beat and the question had to come apart: the loop keeps
// ticking at BEAT_MS and only the QUESTION slows down. Slowing the beat with it
// is the obvious next edit and the one that breaks the session.
import { strict as assert } from 'node:assert'
import test from 'node:test'
import { pollEveryBeats, shouldPoll } from './inbox-watch.mjs'
import { BEAT_MS, STALE_MS } from './watch-marker.mjs'

const minutes = (m) => m * 60_000

test('someone working is answered at full rate', () => {
  assert.equal(pollEveryBeats(0), 1)
  assert.equal(pollEveryBeats(minutes(4)), 1)
  for (let b = 0; b < 16; b++) assert.equal(shouldPoll(b), true, `beat ${b}`)
})

test('stepped away for five minutes, asked once a minute', () => {
  assert.equal(pollEveryBeats(minutes(5)), 4)
  assert.equal(pollEveryBeats(minutes(29)), 4)
})

test('idle past thirty minutes, asked every five', () => {
  assert.equal(pollEveryBeats(minutes(30)), 20)
  assert.equal(pollEveryBeats(minutes(90)), 20)
  assert.equal(pollEveryBeats(minutes(600)), 20)
})

test('the ladder only ever slows down', () => {
  let last = 1
  for (let m = 0; m <= 120; m++) {
    const every = pollEveryBeats(minutes(m))
    assert.ok(every >= last, `rate increased again at ${m}m`)
    last = every
  }
})

test('an idle hour costs a fraction of what it did', () => {
  const beatsPerHour = 3_600_000 / BEAT_MS
  assert.equal(beatsPerHour, 240, 'what every idle hour used to cost')

  // The FIRST idle hour still pays for the ramp: full rate for five minutes,
  // then a quarter to thirty. That is deliberate — someone who steps away for
  // two minutes is not idle, and answering them in fifteen seconds is the whole
  // reason the top rung exists.
  let first = 0
  for (let b = 0; b < beatsPerHour; b++) if (shouldPoll(b)) first++
  assert.equal(first, 51)

  // Every hour after that is the steady state, and it is the number that
  // matters for a machine left open overnight.
  let later = 0
  for (let b = beatsPerHour; b < beatsPerHour * 2; b++) if (shouldPoll(b)) later++
  assert.equal(later, 12)
  assert.ok(later < beatsPerHour / 15, 'the steady state is not quiet enough to bother')
})

test('THE BEAT NEVER SLOWS — a stale marker frees the session to a second watcher', () => {
  // The loop sleeps BEAT_MS regardless of which rung the question is on, so the
  // marker is refreshed on every tick. Several misses must still fit inside the
  // stale window, or one slow disk turns into two watchers.
  assert.ok(BEAT_MS * 3 <= STALE_MS, 'three missed beats no longer fit inside STALE_MS')
  // And the slowest rung must not be mistaken for a sleep interval: five
  // minutes between QUESTIONS, fifteen seconds between beats.
  const slowestQuestionMs = pollEveryBeats(minutes(60)) * BEAT_MS
  assert.equal(slowestQuestionMs, 300_000)
  assert.ok(slowestQuestionMs > STALE_MS, 'the ladder is pointless if it stays inside the stale window')
})
