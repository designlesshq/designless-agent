#!/usr/bin/env node
// The live canvas watcher - the stretch the hooks and the in-turn wait cannot reach.
//
// Hooks fire at turn boundaries. The in-turn wait (wait_seconds / less_stream)
// covers the inside of a turn. Neither covers the agent sitting idle between
// turns, which is exactly when someone is most likely to be editing in the app.
// This is the script that covers it: the host runs it as a persistent monitor
// that streams its output, and every line it prints wakes the agent. The
// facility is the whole point. Started as a plain background command it still
// runs and still prints, and nothing hears it, because that facility reports
// once, when the process exits, and this process never exits on its own. In
// Claude Code the right facility is the Monitor tool with persistence on; the
// ask that starts the watcher (canvas-arm-watch.mjs) says so.
//
// Usage: node inbox-watch.mjs <host-session-id>
//
// It prints a line ONLY when new edits arrive. A background task that chatters
// gets throttled and then stopped by the host, so silence while nothing changes
// is not politeness, it is what keeps the watcher alive long enough to matter.
//
// One per session, enforced: it claims the marker on start and exits at once if
// another holds it. A sub-agent that starts its own finds it taken.
//
// Node built-ins only. Any error ends the watch quietly rather than spraying.

import path from 'node:path'
import { probeInbox, summarizeInbox } from './inbox-probe.mjs'
import { arm, beat, disarm, isArmed, BEAT_MS } from './watch-marker.mjs'

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

/**
 * How often to ASK the desktop, once nothing has been happening.
 *
 * The beat and the poll are two different jobs, and they were one. The marker
 * has to be refreshed inside STALE_MS or the session frees itself and a second
 * watcher starts — so the loop must keep ticking at BEAT_MS. Asking the desktop
 * on that same tick is what nobody needs: a machine with the app open and
 * nobody at it answered 240 times an hour, every hour, to say nothing had
 * changed.
 *
 * So the beat stays and the QUESTION slows down. The ladder is in beats:
 *
 *   under 5 minutes quiet   every beat      15s   — someone is working
 *   under 30 minutes quiet  every 4th       60s   — stepped away
 *   beyond that             every 20th      5m    — the machine is idle
 *
 * Any news resets it to the top, so returning to the canvas is answered at
 * fifteen seconds again. The cost of the slowest rung is that the FIRST edit
 * after a long idle can wait up to five minutes for the watcher — and it does
 * not, because the turn-boundary hooks still fire the moment the person types.
 * The watcher covers the stretch between turns; it was never the only cover.
 */
const QUIET_LADDER = [
  { afterMs: 30 * 60_000, everyBeats: 20 },
  { afterMs: 5 * 60_000, everyBeats: 4 },
]

export function pollEveryBeats(quietMs) {
  for (const rung of QUIET_LADDER) if (quietMs >= rung.afterMs) return rung.everyBeats
  return 1
}

/** Ask this beat? Beats are counted since the last time anything was news. */
export function shouldPoll(quietBeats, beatMs = BEAT_MS) {
  const every = pollEveryBeats(quietBeats * beatMs)
  return quietBeats % every === 0
}

/**
 * How many polls in a row must answer before blindness counts as news again.
 *
 * ONE WAS NOT ENOUGH, and the failure was mine. Clearing the latch on a single
 * good poll is right for a relapse after a healthy stretch, and it is exactly
 * wrong for an accelerator that FLAPS: every answer between two timeouts rearms
 * the announcement, so an intermittent desktop produces a line every cycle. Seen
 * live within an hour of shipping it, eight times in one session.
 *
 * Three at the poll interval is roughly a minute of steady answering, which a
 * flap does not survive and a genuine recovery does.
 */
const HEALTHY_STREAK = 3

/**
 * What is drainable right now, as a value that changes only when the work does.
 *
 * Counts, not just session ids: a second edit landing on a canvas that already
 * had one is new work and has to wake the agent. Attention and day-old items are
 * deliberately absent - they are the user's to act on, not the agent's, and
 * waking an idle agent for them would be waking it for nothing it may do.
 */
export function drainDigest(sessions) {
  return (Array.isArray(sessions) ? sessions : [])
    .map((s) => [s.session_id, Number(s.n_page || 0), Number(s.n_artefact || 0), Number(s.n_annotation || 0)])
    .filter(([, p, a, n]) => p + a + n > 0)
    .map((r) => r.join(':'))
    .sort()
    .join('|')
}

/**
 * One poll's decision, pure so it can be tested without a desktop or a clock.
 *
 * Returns the line to print, or null for silence, plus the state to carry into
 * the next poll. Silence is the common case by design.
 */
export function step(probe, prev, cwd) {
  // A poll that answers does not by itself mean the desktop is back. Counted
  // rather than trusted, so a flap cannot rearm the line it already said.
  const healthy = probe.unknown ? 0 : Number(prev.healthy ?? 0) + 1
  // Cannot see. Say so ONCE per reason: an unreachable accelerator is not an
  // all-clear, and the agent's fallback is to read the inbox itself. Repeating
  // it every 15s for an hour is how a line stops being read.
  if (probe.unknown) {
    if (prev.blind === probe.unknown) return { line: null, next: { ...prev, healthy: 0 } }
    return {
      line: `Designless canvas: the live watcher cannot see the desktop (${probe.unknown}). ` +
        `This is NOT a signal that nothing is waiting: read less_canvas_inbox yourself while it stays unreachable.`,
      next: { ...prev, blind: probe.unknown, healthy: 0 },
    }
  }
  const digest = drainDigest(probe.sessions)
  // The latch clears only once the desktop has answered steadily. Until then the
  // line already said stands, and a relapse inside a flap says nothing new.
  const next = { digest, healthy, blind: healthy >= HEALTHY_STREAK ? null : (prev.blind ?? null) }
  if (!digest || digest === prev.digest) return { line: null, next }
  const text = summarizeInbox(probe.sessions, cwd, { includeAttention: false })
  if (!text) return { line: null, next }
  return { line: `Designless canvas: ${text}`, next }
}

async function main() {
  const sessionId = process.argv[2]
  if (!sessionId) {
    process.stdout.write('Designless watcher: no session id was passed, so it did not start.\n')
    return
  }
  if (!arm(sessionId)) {
    // Already covered. Exiting silently is right: the host reports the exit, and
    // a second watcher would double every wake for the rest of the session.
    process.stdout.write('Designless watcher: one is already running for this session.\n')
    return
  }
  const cwd = process.cwd()
  let state = { digest: '', blind: null }
  const stop = () => { disarm(sessionId); process.exit(0) }
  process.on('SIGINT', stop)
  process.on('SIGTERM', stop)

  // Beats since anything was news. Drives the ladder, and nothing else.
  let quietBeats = 0

  for (;;) {
    if (shouldPoll(quietBeats)) {
      let probe
      try { probe = await probeInbox() } catch { probe = { unknown: 'probe failed', sessions: [] } }
      const { line, next } = step(probe, state, cwd)
      state = next
      if (line) {
        process.stdout.write(line + '\n')
        quietBeats = 0 // someone is doing something; ask at full rate again
      } else {
        quietBeats += 1
      }
    } else {
      quietBeats += 1
    }
    // The beat is what keeps the marker fresh, and it does NOT slow down: the
    // marker goes stale in 90s and a stale marker frees the session to a second
    // watcher. Stop beating and a crash frees the session instead of locking
    // it, which is the property worth keeping at every rung of the ladder.
    beat(sessionId)
    await sleep(BEAT_MS)
  }
}

if (process.argv[1] && import.meta.url.endsWith(path.basename(process.argv[1]))) {
  main().catch(() => process.exit(0))
}

export { isArmed }
