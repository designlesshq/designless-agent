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
 * The same once-per-state-change gate, for the line that says the accelerator
 * could not be reached.
 *
 * THE DEFECT: that line `return`ed above the gate, so it printed on EVERY
 * prompt of every session while the known-state line printed once per change.
 * An idle canvas serves the probe over HTTP by design (v0.2.65's warmth poll is
 * edit-activity-gated), so "unreachable" is a NORMAL steady state, not an event
 * — and repeating a steady state every turn is how a message stops being read.
 * Two sessions reported it as noise before this was looked at.
 *
 * Keyed on the REASON, so a CHANGED reason still informs: a timeout and a
 * refused socket are different facts. Cleared whenever the probe succeeds, so a
 * later relapse informs again rather than staying silent because the same
 * reason was mentioned an hour ago. Fail-open, like its sibling: a state-file
 * error means print, because a repeated mention is noise and a dropped one is a
 * lost message — and this one specifically must never be swallowed, since its
 * whole job is to stop a timeout being read as "nothing waiting".
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
    // Steady state, not an event — see unknownGate.
    if (!unknownGate(hookSessionId, unknown)) return
    process.stdout.write(JSON.stringify({
      hookSpecificOutput: {
        hookEventName: 'UserPromptSubmit',
        additionalContext:
          `Designless canvas: could not reach the desktop inbox accelerator (${unknown}). ` +
          `This is NOT a signal that nothing is waiting. Call less_canvas_inbox to check for real.`,
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
