#!/usr/bin/env node
// Stop drain-check (Phase-5 v3, brain d8db8b78) - FAIL-OPEN, bounded,
// right-checkout-aware.
//
// Replaces page-session-drain.mjs. Stalls the turn (continue:false) ONCE only
// when there is a PAGE edit drainable FROM THIS CHECKOUT - so it never
// false-stalls on a session the agent cannot apply (wrong checkout), nor on
// Type-1 artefact edits, nor on annotations (context). The stop_hook_active
// guard prevents a stop loop; any error allows the stop.
//
// WHY TYPE-1 IS EXCLUDED. The original reason given here was "ledger apply
// rolling out", which went stale on 2026-06-16 when designsystem c054c03 removed
// the TYPE1_APPLY flag and artefact apply went GA. That justification is corrected
// rather than deleted, because the exclusion still holds for a DIFFERENT and
// durable reason: stalling a turn is only warranted when the work is bound to
// THIS agent in THIS checkout. A page op is - its source files exist only here, so
// if the turn ends undrained nobody else can apply it. A Type-1 op is not:
// apply_type1 applies server-side against the manifest with no cwd dependency, so
// any session can drain it at any time and ending the turn loses nothing. The
// whole stopReason payload below is likewise branch-first Type-2 instruction that
// does not describe an artefact drain at all.
//
// Artefact edits are surfaced instead by the UserPromptSubmit wake
// (canvas-wake.mjs -> summarizeInbox) and backstopped by the server's
// pending_ops_conflict refusal on compose/set_image. Whether this hook SHOULD also
// stall on Type-1 is an open architectural question (brain 61e158c1, status
// proposed) - it is deliberately NOT decided here.

import { probeInbox, cwdGitRemote, remotesMatch, isSafeBranchName, isSafeRepoRemote } from './inbox-probe.mjs'

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
  // repo_remote / safety_branch are server/IPC-supplied and get embedded into git
  // instruction text below — validate before trusting. probeInbox already drops
  // malformed rows; this is the second, explicit guard at the point of embedding.
  const drainableHere = sessions.filter((s) =>
    Number(s.n_page || 0) > 0 &&
    (s.repo_remote == null || isSafeRepoRemote(s.repo_remote)) &&
    (s.repo_remote ? remotesMatch(origin, s.repo_remote) : true))
  if (!drainableHere.length) return        // nothing applies here - let the stop through

  // Name the required safety branch(es) so the drainer can go branch-first without
  // a round-trip. The branch is a SERVER-OWNED contract carried verbatim on the inbox
  // row as safety_branch (manifest._safety.branch) — READ it, never derive
  // designless/<session_id> client-side (derivation = drift risk). A null safety_branch
  // is an un-stamped legacy session: no branch required, so it is omitted.
  const branches = [...new Set(
    drainableHere
      .map((s) => (s && isSafeBranchName(s.safety_branch) ? s.safety_branch : null))
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
