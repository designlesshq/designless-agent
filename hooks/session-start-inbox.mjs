#!/usr/bin/env node
// SessionStart eager inbox probe - FAIL-OPEN.
//
// At the start of a session, enumerate every waiting canvas edit across ALL of
// the user's sessions (cwd-independent) so the agent knows up front what is
// pending - including the recoverable (expired-with-ops) backlog, annotations,
// and items needing the user. Any error / empty inbox exits 0 silently. Node
// built-ins only; one dependency: a reachable, signed-in desktop.

import { probeInbox, summarizeInbox } from './inbox-probe.mjs'
import fs from 'node:fs'
import path from 'node:path'

// The writing register, carried HERE because hooks are the only prose that
// reaches every session. The rules also live in the orchestrator skill and the
// sub-agent contract, but neither loads on a session that refuses before a
// workflow starts, and the audit showed refusals are the surface users see
// most when the desktop is unreachable. Ten emdashes and two quoted tool
// names came out of exactly that gap. ~100 tokens, paid once per session.
//
// SCOPED TO ITS DOMAIN (trust boundary, 2026-09-02). A plugin's voice rules
// govern the plugin's work, never the host: a person who installs a design
// plugin has not asked it to edit their unrelated writing. The register
// therefore names its own scope, and the scope is part of the text — the one
// channel that reaches every session now carries a rule that knows where it
// applies and says where it does not. The same audit's second finding is
// folded in: plain language is the DEFAULT, and exactness (commands, error
// text, identifiers) is the rule whenever recovery or an explicit technical
// ask needs it - a register that suppresses diagnostics is not plain dealing.
const REGISTER =
  'Designless register, scoped to Designless work only (canvas, compose, ' +
  'artefact, and brand flows, including refusals within them): default to ' +
  'plain product language; say what happened in words the user already has, ' +
  'and do not decorate prose with tool names, schema fields, or wire values. ' +
  'Exception, equally binding: when the user asks for technical detail, or an ' +
  'exact command, identifier, or error text is needed for recovery, support, ' +
  'or diagnosis, show it exactly - precision is part of plain dealing. No ' +
  'emdashes in those flows: colon, comma, or period. Internal scores are ' +
  'explained plainly or left out, never quoted as bare numbers. This register ' +
  'does not govern conversation unrelated to Designless.'

// The workspace's own memory of its last compose (written by the compose
// epilogue hook, zero requests). Served as one bounded line when fresh, so
// the next session knows which canvas already exists before it reads
// anything. Context, not an instruction: a Type-1 artefact may still compose
// fresh; the line only stops a session rediscovering what it already made.
const EPILOGUE_FRESH_DAYS = 14
export function epilogueLine(cwd) {
  try {
    const p = path.join(cwd, '.designless', 'compose-epilogue.json')
    const e = JSON.parse(fs.readFileSync(p, 'utf8'))
    const age = (Date.now() - new Date(e.composed_at).getTime()) / 86400000
    if (!(age >= 0 && age <= EPILOGUE_FRESH_DAYS)) return null
    if (!e.brand_slug || !e.template_id || !e.session_id) return null
    const when = String(e.composed_at).slice(0, 10)
    const title = e.title ? `"${String(e.title).slice(0, 60)}"` : 'an untitled piece'
    return `Designless memory for this workspace: on ${when} it composed ${title} with the ${e.template_id} template for the ${e.brand_slug} brand; that canvas (${e.session_id}) can be updated in place rather than minted again, and ` +
      (e.open_url ? `opens with ${e.open_url}. ` : 'is reachable through the canvas status tool. ') +
      'Use this when the ask continues that piece; compose fresh when it is a new one.'
  } catch { return null }
}

const emit = (context) => process.stdout.write(JSON.stringify({
  hookSpecificOutput: { hookEventName: 'SessionStart', additionalContext: context },
}))

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let cwd
  try { cwd = JSON.parse(raw).cwd } catch { cwd = process.cwd() }
  if (!cwd || typeof cwd !== 'string') cwd = process.cwd()

  const memory = epilogueLine(cwd)
  const REG = memory ? `${memory} ${REGISTER}` : REGISTER
  const { count, sessions, unknown } = await probeInbox()

  // An indeterminate probe must not read as "no waiting edits" — see canvas-wake
  // for the incident. At session start this matters most: it is the one moment the
  // agent forms its picture of what is outstanding, and a false all-clear here
  // persists for the whole session.
  if (unknown) {
    emit(
      `Designless canvas: could not reach the desktop inbox accelerator (${unknown}). ` +
      `This is NOT a signal that nothing is waiting. Check the real inbox with the ` +
      `canvas-inbox tool (less_canvas_inbox) before treating it as clear. ` + REG,
    )
    return
  }

  // The register rides even when the inbox has nothing to say — but since it
  // is scoped to Designless work by its own text, riding is dormancy, not
  // governance: a session that never touches Designless is never spoken for.
  if (!count) { emit(REG); return }
  const text = summarizeInbox(sessions, cwd)
  if (!text) { emit(REG); return }

  emit(`Designless canvas (waiting edits): ${text} ` + REG)
}

if (process.argv[1] && import.meta.url.endsWith(path.basename(process.argv[1]))) {
  main().then(() => process.exit(0)).catch(() => process.exit(0))
}
