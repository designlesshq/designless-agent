#!/usr/bin/env node
// Stop drain-check (Phase-5 v3, brain d8db8b78) - FAIL-OPEN, bounded,
// right-checkout-aware.
//
// Replaces page-session-drain.mjs. Stalls the turn (continue:false) ONCE only
// when there is a PAGE edit drainable FROM THIS CHECKOUT - so it never
// false-stalls on a session the agent cannot apply (wrong checkout) nor on
// Type-1 edits (ledger apply rolling out) or annotations (context). The
// stop_hook_active guard prevents a stop loop; any error allows the stop.

import { probeInbox, cwdGitRemote, remotesMatch } from './inbox-probe.mjs'

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let data
  try { data = JSON.parse(raw) } catch { return }
  if (data.stop_hook_active) return        // already continuing - never loop
  const cwd = data.cwd
  if (!cwd || typeof cwd !== 'string') return

  const { count, sessions } = await probeInbox()
  if (!count) return

  const origin = cwdGitRemote(cwd)
  const stallWorthy = sessions.some((s) =>
    Number(s.n_page || 0) > 0 && (s.repo_remote ? remotesMatch(origin, s.repo_remote) : true))
  if (!stallWorthy) return                 // nothing applies here - let the stop through

  process.stdout.write(JSON.stringify({
    continue: false,
    stopReason:
      'Designless canvas: page edits are waiting and drainable from this checkout. Before finishing, ' +
      'enumerate with less_canvas_inbox and apply with less_canvas_ops (claim -> apply on previous_value ' +
      '-> ack). If none remain claimable from here, you are done.',
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
