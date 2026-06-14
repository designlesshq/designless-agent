#!/usr/bin/env node
// Stop hook (Phase-5 P4 wake) — FAIL-OPEN, bounded.
//
// Last-chance drain directive: when a Designless Type-2 page session is active in
// this project, nudge the agent ONCE before the turn ends to apply any pending
// canvas edits, so a human's edits aren't left stranded. Guards on
// `stop_hook_active` so it fires at most once per turn (no stop loop).
//
// Fail-open: no marker, malformed input, an already-continuing stop, or ANY error
// all allow the stop (exit 0, no output). Only Node built-ins; no install, no
// network. The marker is cleared by the page-mode flow when the user is done, so
// a stale directive cannot persist past the session.

import { existsSync } from 'node:fs';
import { join } from 'node:path';

async function main() {
  let raw = '';
  for await (const chunk of process.stdin) raw += chunk;

  let data;
  try { data = JSON.parse(raw); } catch { return; }
  if (data.stop_hook_active) return; // already in a stop continuation — never loop

  const cwd = data.cwd;
  if (!cwd || typeof cwd !== 'string') return;
  if (!existsSync(join(cwd, '.designless', 'session.json'))) return; // no active page session

  process.stdout.write(JSON.stringify({
    continue: false,
    stopReason:
      'A Designless page session is active — before finishing, check less_canvas_ops for pending ' +
      'canvas edits and apply them (claim → apply on previous_value → ack). If none are pending, you are done.',
  }));
}

main().then(() => process.exit(0)).catch(() => process.exit(0));
