// Whether a live canvas watcher is running for this host session.
//
// The watcher is a background task only the AGENT can start: a hook returns
// text and exits, so it can ask for one but never create one. This marker is
// what turns that ask into something reliable. The WATCHER writes it, not the
// agent, so a directive that was ignored is indistinguishable from one that was
// never sent, and the next canvas call asks again. It retries until it is
// actually obeyed rather than trusting that it was.
//
// It is also the one-watcher rule, enforced rather than requested. A sub-agent
// that starts its own finds the marker and exits.
//
// GLOBAL, beside nudge-state, because it is keyed on the HOST SESSION and a
// session moves between repos. The repo-local .designless/ holds facts bound to
// a checkout (its canvas session stamp, its compose memory); this is not one.
//
// Node built-ins only. Never throws.

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'

/** A watcher that stopped beating this long ago is gone, whatever the file says. */
export const STALE_MS = 90_000

/** How often the watcher refreshes its beat. Several misses fit inside STALE_MS. */
export const BEAT_MS = 15_000

export function watchDir() {
  return path.join(os.homedir(), '.designless', 'watch')
}

export function markerPath(sessionId) {
  return path.join(watchDir(), `${String(sessionId).replace(/[^A-Za-z0-9_-]/g, '')}.json`)
}

export function readMarker(sessionId) {
  if (!sessionId) return null
  try { return JSON.parse(fs.readFileSync(markerPath(sessionId), 'utf8')) } catch { return null }
}

/**
 * Is a watcher alive for this session?
 *
 * An unreadable or stale marker reads as NOT armed, deliberately. The cost of
 * being wrong that way is one redundant directive, and the watcher script
 * checks the same marker and exits immediately — so a duplicate costs a process
 * that lives for milliseconds. The cost of the other error is a session with no
 * watcher at all, which is the failure this whole thing exists to end.
 */
export function isArmed(sessionId, now = Date.now()) {
  const m = readMarker(sessionId)
  if (!m || typeof m.beat_at !== 'string') return false
  const beat = new Date(m.beat_at).getTime()
  if (!Number.isFinite(beat)) return false
  return now - beat < STALE_MS
}

/** Claim the session. Returns false when someone else already holds it. */
export function arm(sessionId, now = Date.now()) {
  if (!sessionId) return false
  if (isArmed(sessionId, now)) return false
  try {
    fs.mkdirSync(watchDir(), { recursive: true })
    fs.writeFileSync(markerPath(sessionId), JSON.stringify({
      pid: process.pid,
      started_at: new Date(now).toISOString(),
      beat_at: new Date(now).toISOString(),
    }))
    return true
  } catch { return false }
}

/** Still here. Called every poll; a missed beat is what makes a marker stale. */
export function beat(sessionId, now = Date.now()) {
  const m = readMarker(sessionId)
  if (!m) return false
  try {
    fs.writeFileSync(markerPath(sessionId), JSON.stringify({ ...m, beat_at: new Date(now).toISOString() }))
    return true
  } catch { return false }
}

export function disarm(sessionId) {
  try { fs.unlinkSync(markerPath(sessionId)); return true } catch { return false }
}
