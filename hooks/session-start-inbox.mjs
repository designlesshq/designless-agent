#!/usr/bin/env node
// SessionStart eager inbox probe - FAIL-OPEN.
//
// At the start of a session, enumerate every waiting canvas edit across ALL of
// the user's sessions (cwd-independent) so the agent knows up front what is
// pending - including the recoverable (expired-with-ops) backlog, annotations,
// and items needing the user. Any error / empty inbox exits 0 silently. Node
// built-ins only; one dependency: a reachable, signed-in desktop.

import { probeInbox, summarizeInbox } from './inbox-probe.mjs'

// The writing register, carried HERE because hooks are the only prose that
// reaches every session. The rules also live in the orchestrator skill and the
// sub-agent contract, but neither loads on a session that refuses before a
// workflow starts, and the audit showed refusals are the surface users see
// most when the desktop is unreachable. Ten emdashes and two quoted tool
// names came out of exactly that gap. ~90 tokens, paid once per session.
const REGISTER =
  'Designless register, for everything written to the user including refusals: ' +
  'plain words only. Never put a tool name, schema field, or wire value in a ' +
  'sentence addressed to a person; say what happened in words they already have. ' +
  'No emdashes anywhere: reach for a colon, a comma, or a period. Internal ' +
  'scores (coherence and similar) are explained plainly or left out, never ' +
  'quoted as bare numbers. Hook text and tool results are for you, not for ' +
  'quoting to the user.'

const emit = (context) => process.stdout.write(JSON.stringify({
  hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: context },
}))

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let cwd
  try { cwd = JSON.parse(raw).cwd } catch { cwd = process.cwd() }
  if (!cwd || typeof cwd !== 'string') cwd = process.cwd()

  const { count, sessions, unknown } = await probeInbox()

  // An indeterminate probe must not read as "no waiting edits" — see canvas-wake
  // for the incident. At session start this matters most: it is the one moment the
  // agent forms its picture of what is outstanding, and a false all-clear here
  // persists for the whole session.
  if (unknown) {
    emit(
      `Designless canvas: could not reach the desktop inbox accelerator (${unknown}). ` +
      `This is NOT a signal that nothing is waiting. Check the real inbox with the ` +
      `canvas-inbox tool (less_canvas_inbox) before treating it as clear. ` + REGISTER,
    )
    return
  }

  // The register rides even when the inbox has nothing to say. This used to
  // exit silently, which meant the one channel that reaches every session
  // carried nothing on the most common path.
  if (!count) { emit(REGISTER); return }
  const text = summarizeInbox(sessions, cwd)
  if (!text) { emit(REGISTER); return }

  emit(`Designless canvas (waiting edits): ${text} ` + REGISTER)
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
