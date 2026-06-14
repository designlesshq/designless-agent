#!/usr/bin/env node
// UserPromptSubmit hook (Phase-5 P4 wake) — FAIL-OPEN.
//
// When a Designless Type-2 page session is active in this project (the page-mode
// flow wrote `.designless/session.json` on compose), remind the agent to drain
// pending canvas edits before starting new work. This is the cheap local "wake"
// so a human's canvas edits don't sit un-applied; the agent does the actual drain
// over its authenticated MCP wire (less_canvas_ops).
//
// It NEVER blocks the prompt: missing marker, malformed input, or any error all
// exit 0 with no output. Only Node built-ins; no install, no network.

import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

async function main() {
  let raw = '';
  for await (const chunk of process.stdin) raw += chunk;

  let cwd;
  try { cwd = JSON.parse(raw).cwd; } catch { return; }
  if (!cwd || typeof cwd !== 'string') return;

  const marker = join(cwd, '.designless', 'session.json');
  if (!existsSync(marker)) return; // no active page session here — stay silent

  let session = '';
  try { session = JSON.parse(readFileSync(marker, 'utf8')).session_id || ''; } catch { /* generic nudge */ }

  const additionalContext =
    `A Designless page session${session ? ` (${session})` : ''} is active in this project. ` +
    `If the user has edited the canvas, drain pending edits with less_canvas_ops ` +
    `(claim → apply on previous_value → ack) before composing new work. ` +
    `If less_canvas_ops reports none pending, ignore this.`;

  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'UserPromptSubmit', additionalContext },
  }));
}

main().then(() => process.exit(0)).catch(() => process.exit(0));
