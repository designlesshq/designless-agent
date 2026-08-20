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
function attentionGate(sessionId, digest) {
  if (!sessionId || digest === 'none') return digest !== 'none'
  try {
    const dir = path.join(os.homedir(), '.designless', 'nudge-state')
    fs.mkdirSync(dir, { recursive: true })
    const file = path.join(dir, `${String(sessionId).replace(/[^A-Za-z0-9_-]/g, '')}.json`)
    let last = null
    try { last = JSON.parse(fs.readFileSync(file, 'utf8')).attention } catch { /* first time */ }
    if (last === digest) return false
    fs.writeFileSync(file, JSON.stringify({ attention: digest, at: new Date().toISOString() }))
    return true
  } catch {
    return true
  }
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

  const { count, sessions, unknown } = await probeInbox()

  // The probe could not determine anything (slow socket, denied, stale reply).
  // Say so — do NOT stay silent. Silence here is read as "no edits waiting", and
  // on 2026-07-20 that exact conflation hid a real pending artefact edit: the
  // socket answered correctly in 797-2002ms against a 700ms budget, so every
  // prompt saw "all clear" while the user's work sat undrained. The probe is only
  // an accelerator; less_canvas_inbox is the authority and is server-side, so the
  // honest fallback is to tell the agent to ask it.
  if (unknown) {
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

  if (!count) return
  const includeAttention = attentionGate(hookSessionId, attentionDigest(sessions))
  const text = summarizeInbox(sessions, cwd, { includeAttention })
  if (!text) return

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'UserPromptSubmit', additionalContext: `Designless canvas: ${text}` },
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
