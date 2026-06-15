#!/usr/bin/env node
// UserPromptSubmit wake (Phase-5 v3, brain d8db8b78) - FAIL-OPEN, cwd-INDEPENDENT.
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

import { probeInbox, summarizeInbox } from './inbox-probe.mjs'

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let cwd
  try { cwd = JSON.parse(raw).cwd } catch { return }
  if (!cwd || typeof cwd !== 'string') return

  const { count, sessions } = await probeInbox()
  if (!count) return
  const text = summarizeInbox(sessions, cwd)
  if (!text) return

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'UserPromptSubmit', additionalContext: `Designless canvas: ${text}` },
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
