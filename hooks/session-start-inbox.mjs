#!/usr/bin/env node
// SessionStart eager inbox probe (Phase-5 v3, brain d8db8b78) - FAIL-OPEN.
//
// At the start of a session, enumerate every waiting canvas edit across ALL of
// the user's sessions (cwd-independent) so the agent knows up front what is
// pending - including the recoverable (expired-with-ops) backlog, annotations,
// and items needing the user. Any error / empty inbox exits 0 silently. Node
// built-ins only; one dependency: a reachable, signed-in desktop.

import { probeInbox, summarizeInbox } from './inbox-probe.mjs'

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let cwd
  try { cwd = JSON.parse(raw).cwd } catch { cwd = process.cwd() }
  if (!cwd || typeof cwd !== 'string') cwd = process.cwd()

  const { count, sessions } = await probeInbox()
  if (!count) return
  const text = summarizeInbox(sessions, cwd)
  if (!text) return

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: `Designless canvas (waiting edits): ${text}` },
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
