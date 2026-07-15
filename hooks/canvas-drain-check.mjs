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
  const drainableHere = sessions.filter((s) =>
    Number(s.n_page || 0) > 0 && (s.repo_remote ? remotesMatch(origin, s.repo_remote) : true))
  if (!drainableHere.length) return        // nothing applies here - let the stop through

  // Name the required safety branch(es) so the drainer can go branch-first without
  // a round-trip. The gate withholds every Type-2 source op unless repo_branch ===
  // the session's _safety.branch (designless/<session_id> by construction).
  const branches = drainableHere
    .map((s) => (s && s.session_id ? `designless/${s.session_id}` : null))
    .filter(Boolean)
  const branchHint = branches.length
    ? ` Required safety branch(es): ${branches.join(', ')} (confirm against each session's _safety.branch).`
    : ''

  process.stdout.write(JSON.stringify({
    continue: false,
    stopReason:
      'Designless canvas: page (Type-2 SOURCE) edits are waiting and drainable from this checkout. ' +
      'These are source ops - work BRANCH-FIRST: read the session\'s _safety.branch (from less_canvas_inbox / ' +
      'less_canvas_status), then git checkout -b designless/<session> (or git checkout it if it exists) BEFORE ' +
      'you claim - the server withholds every source op unless you are on that safety branch. On EVERY source ' +
      'claim AND ack pass repo_branch (= git rev-parse --abbrev-ref HEAD) and checkout_head (= git rev-parse HEAD). ' +
      'Enumerate with less_canvas_inbox, then apply with less_canvas_ops (claim -> apply on previous_value -> ack). ' +
      'If none remain claimable from here, you are done.' +
      branchHint,
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
