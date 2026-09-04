/**
 * A hook that cannot see the inbox must not go quiet about it.
 *
 * THE DEFECT this closes: when the accelerator is unreachable — the app closed,
 * signed out, or the access grant missing — this hook knows NOTHING. It said so
 * once per reason and then fell silent for the rest of the session, and silence
 * from a hook whose whole job is to mention waiting work reads as "nothing is
 * waiting". A real session ran a whole afternoon that way, denied by a missing
 * grant, with two of the user's edits sitting undrained behind it.
 *
 * The dedup was right about one thing: an unreachable accelerator is a steady
 * state, and the full sentence on every prompt is what made two people stop
 * reading it. So length is what the gate governs, not whether it speaks.
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const SRC = readFileSync(path.join(path.dirname(fileURLToPath(import.meta.url)), 'canvas-wake.mjs'), 'utf8')

test('the unreachable branch always writes, and only the wording is gated', () => {
  const branch = SRC.slice(SRC.indexOf('if (unknown) {'), SRC.indexOf('clearUnknown(hookSessionId)'))
  // The old shape: `if (!unknownGate(...)) return` — a mute.
  assert.doesNotMatch(branch, /if \(!unknownGate\([^)]*\)\) return/)
  // The new one: the gate chooses the wording, the write always happens.
  assert.match(branch, /const first = unknownGate\(/)
  assert.match(branch, /first\s*\?/)
  assert.match(branch, /process\.stdout\.write/)
})

test('it names the standing duty, not just the fact', () => {
  assert.match(SRC, /every turn while it stays unreachable/)
  assert.match(SRC, /still unreachable/)
})

test('a reachable accelerator still clears the state, so a relapse speaks in full', () => {
  assert.match(SRC, /clearUnknown\(hookSessionId\)/)
})
