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
  // a round-trip. The branch is a SERVER-OWNED contract carried verbatim on the inbox
  // row as safety_branch (manifest._safety.branch) — READ it, never derive
  // designless/<session_id> client-side (derivation = drift risk). A null safety_branch
  // is an un-stamped legacy session: no branch required, so it is omitted.
  const branches = [...new Set(
    drainableHere
      .map((s) => (s && typeof s.safety_branch === 'string' && s.safety_branch ? s.safety_branch : null))
      .filter(Boolean),
  )]
  const branchHint = branches.length
    ? ` Required safety branch(es): ${branches.join(', ')} (server-owned; read from each row's safety_branch, do NOT derive).`
    : ''

  process.stdout.write(JSON.stringify({
    continue: false,
    stopReason:
      'Designless canvas: page (Type-2 SOURCE) edits are waiting and drainable from this checkout. ' +
      'These are source ops - work BRANCH-FIRST: READ the required branch from the session\'s safety_branch field ' +
      '(on the less_canvas_inbox row, also on less_canvas_status), then git checkout -b <safety_branch> (or git ' +
      'checkout it if it exists) BEFORE you claim - the server withholds every source op unless you are on that safety ' +
      'branch. If safety_branch is null the session is un-stamped: no branch is required. On EVERY source ' +
      'claim AND ack pass repo_branch (= git rev-parse --abbrev-ref HEAD) and checkout_head (= git rev-parse HEAD). ' +
      'Enumerate with less_canvas_inbox, then apply with less_canvas_ops (claim -> apply on previous_value -> ack). ' +
      'If none remain claimable from here, you are done.' +
      branchHint,
  }))
}

main().then(() => process.exit(0)).catch(() => process.exit(0))
