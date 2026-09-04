#!/usr/bin/env node
// UserPromptSubmit wake - FAIL-OPEN, cwd-INDEPENDENT.
//
// Replaces page-session-nudge.mjs. Drops the existsSync(cwd/.designless/
// session.json) gate that missed the prism-vs-skilldesign case: discovery is now
// the server inbox (keyed on user identity), surfaced via the desktop IPC
// accelerator, so a waiting edit is found in ANY cwd. Routes per surface +
// checkout: page edits drainable here vs route-the-user, Type-1 informational,
// annotations as context, needs_human to the user.
//
// Never blocks the prompt: any error / empty inbox exits 0 with no output. Node
// built-ins only; the one dependency is a reachable, signed-in desktop (else the
// canvas "waiting" pill is the floor).

import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { probeInbox, summarizeInbox, attentionDigest } from './inbox-probe.mjs'

// Once-per-state-change gate for the INFORM-ONLY attention line. Keyed by the
// Claude session id from the hook input, stored under the user's home (never
// the repo). Drainable-edit lines are obligations and are never suppressed.
// Fail-open: any state-file error means "include the line" - a repeated
// mention is noise, a dropped one is a lost message.
function stateFile(sessionId) {
  const dir = path.join(os.homedir(), '.designless', 'nudge-state')
  fs.mkdirSync(dir, { recursive: true })
  return path.join(dir, `${String(sessionId).replace(/[^A-Za-z0-9_-]/g, '')}.json`)
}

/** Read the whole state object. Any error reads as empty (fail-open). */
function readState(sessionId) {
  try { return JSON.parse(fs.readFileSync(stateFile(sessionId), 'utf8')) || {} } catch { return {} }
}

/**
 * Merge, never replace. Two gates share this file now, and a wholesale write
 * would have one silently clear the other's memory every turn.
 */
function writeState(sessionId, patch) {
  const next = { ...readState(sessionId), ...patch, at: new Date().toISOString() }
  fs.writeFileSync(stateFile(sessionId), JSON.stringify(next))
}

function attentionGate(sessionId, digest) {
  if (!sessionId || digest === 'none') return digest !== 'none'
  try {
    if (readState(sessionId).attention === digest) return false
    writeState(sessionId, { attention: digest })
    return true
  } catch {
    return true
  }
}

/**
 * IS THIS THE FIRST TIME we could not reach the accelerator for this reason?
 *
 * Not a mute: an unanswerable accelerator is the one state where this hook
 * knows NOTHING, so the obligation to look stands on every prompt until it can
 * answer again. What the gate governs is LENGTH. The full sentence, repeated on
 * every turn of a session that is denied for hours, is how a message stops
 * being read — two sessions reported exactly that as noise. So it is said in
 * full once per reason, and thereafter in a clause.
 *
 * That distinction is what a real session got wrong: denied for a whole
 * afternoon, the line was spent early, and two of the user's edits sat waiting
 * with nothing left to mention them. A steady state deserves brevity; the duty
 * it carries does not expire with the sentence that announced it.
 *
 * Keyed on the REASON, so a CHANGED reason speaks in full again: a timeout and
 * a refused socket are different facts. Cleared whenever the probe succeeds, so
 * a later relapse is news. Fail-open: a state-file error means say it in full.
 */
function unknownGate(sessionId, reason) {
  if (!sessionId) return true
  try {
    if (readState(sessionId).unknown === reason) return false
    writeState(sessionId, { unknown: reason })
    return true
  } catch {
    return true
  }
}

/** Forget the last unknown, so the next one is news again. */
function clearUnknown(sessionId) {
  if (!sessionId) return
  try { if (readState(sessionId).unknown !== undefined) writeState(sessionId, { unknown: undefined }) } catch { /* best effort */ }
}

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let cwd, hookSessionId
  try {
    const input = JSON.parse(raw)
    cwd = input.cwd
    hookSessionId = input.session_id
  } catch { return }
  if (!cwd || typeof cwd !== 'string') return

  const { count, sessions, attnDark, unknown } = await probeInbox()

  // The probe could not determine anything (slow socket, denied, stale reply).
  // Say so — do NOT stay silent. Silence here is read as "no edits waiting", and
  // on 2026-07-20 that exact conflation hid a real pending artefact edit: the
  // socket answered correctly in 797-2002ms against a 700ms budget, so every
  // prompt saw "all clear" while the user's work sat undrained. The probe is only
  // an accelerator; less_canvas_inbox is the authority and is server-side, so the
  // honest fallback is to tell the agent to ask it.
  if (unknown) {
    // Said in full the first time for this reason, and in a clause after — but
    // said EVERY turn. This branch is the one where the hook can see nothing,
    // so silence here is indistinguishable from "nothing is waiting", and that
    // is precisely the conflation this line exists to prevent.
    const first = unknownGate(hookSessionId, unknown)
    process.stdout.write(JSON.stringify({
      hookSpecificOutput: {
        hookEventName: 'UserPromptSubmit',
        additionalContext: first
          ? `Designless canvas: could not reach the desktop inbox accelerator (${unknown}). ` +
            `This is NOT a signal that nothing is waiting. Call less_canvas_inbox to check for real, ` +
            `this turn and every turn while it stays unreachable.`
          : `Designless canvas: inbox accelerator still unreachable (${unknown}) — read less_canvas_inbox yourself.`,
      },
    }))
    return
  }

  // The probe answered, so a previous 'unreachable' is stale news; forget it and
  // let a later relapse speak again.
  clearUnknown(hookSessionId)

  // A dark count beside an empty listing is a real message (see summarizeInbox):
  // the waiting item's session may not be enumerated while the item still is.
  if (!count && !attnDark) return
  const includeAttention = attentionGate(hookSessionId, attentionDigest(sessions, attnDark))
  const text = summarizeInbox(sessions, cwd, { includeAttention, attnDark })
  if (!text) return

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'UserPromptSubmit', additionalContext: `Designless canvas: ${text}` },
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
