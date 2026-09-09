// Is anyone actually at this machine?
//
// The watcher's idle ladder slows its polling down when nothing has arrived, so
// a laptop left locked overnight stops asking the desktop 240 times an hour to
// hear that nothing changed. That part is right and it stays.
//
// What it got wrong is the question it was answering. It measured "have there
// been edits" and spent the answer as though it were "is anyone here", and those
// come apart in an ordinary way: someone working in one session, with the canvas
// quiet, looks exactly like an abandoned machine. Every watcher on the box then
// slows to a five-minute beat, and the next edit waits minutes for one of them
// to ask — while the person sits right there.
//
// This is the missing half of that question, and it needs no new machinery. The
// turn-boundary hook already runs every time the person submits a prompt. It
// stamps the time here; the watcher reads it. A prompt is the plainest evidence
// there is that a human is present and working.
//
// MACHINE-WIDE, not per session, and that is the point rather than an
// implementation shortcut. The case to protect is one session busy while the
// others idle: the person is at the keyboard, any armed session could take the
// next edit, so a prompt anywhere keeps every watcher awake. Keyed per session
// this would have left the idle ones asleep, which is the exact failure.
//
// Node built-ins only. Never throws: a missing or unreadable stamp reads as "no
// evidence of anyone", which is the pre-existing behaviour.

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

/**
 * How long a submitted prompt counts as someone being present.
 *
 * Long enough to span reading, thinking and a slow reply, so a person working
 * steadily never falls through it; short enough that a machine walked away from
 * returns to the quiet ladder without waiting for a whole session to end. Ten
 * minutes of no prompt from ANY session is a fair reading of "nobody is here".
 */
export const ACTIVE_WINDOW_MS = 10 * 60_000

export function activityPath() {
  return path.join(os.homedir(), '.designless', 'watch', 'host-activity.json')
}

/**
 * Record that a person just did something. Called from the turn-boundary hook.
 *
 * Deliberately a whole-file write of one small object: several sessions stamp
 * this concurrently, and the last writer winning is exactly the semantics wanted
 * (the newest prompt is the freshest evidence). Nothing accumulates, so there is
 * nothing to merge and nothing to grow.
 */
export function noteActivity(now = Date.now()) {
  try {
    const p = activityPath()
    fs.mkdirSync(path.dirname(p), { recursive: true })
    fs.writeFileSync(p, JSON.stringify({ at: now }))
    return true
  } catch {
    return false
  }
}

/** Milliseconds since the last prompt anywhere, or null when there is no stamp. */
export function msSinceActivity(now = Date.now()) {
  try {
    const raw = JSON.parse(fs.readFileSync(activityPath(), 'utf8'))
    const at = Number(raw?.at)
    if (!Number.isFinite(at)) return null
    // A stamp from the future is a clock that moved; treat it as "just now"
    // rather than as a negative age that would compare strangely.
    return Math.max(0, now - at)
  } catch {
    return null
  }
}

/**
 * Is a person present, on the evidence available?
 *
 * FALSE when there is no stamp at all. That is the honest reading and it is also
 * the safe one: an older plugin, a host with no such hook, or a fresh install
 * has no evidence of anyone, and the ladder behaves exactly as it did before
 * this existed. Presence has to be shown, never assumed.
 */
export function hostIsActive(now = Date.now(), windowMs = ACTIVE_WINDOW_MS) {
  const age = msSinceActivity(now)
  return age !== null && age < windowMs
}
