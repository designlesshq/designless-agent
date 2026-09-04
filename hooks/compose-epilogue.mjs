#!/usr/bin/env node
// PostToolUse compose epilogue - FAIL-OPEN, zero requests.
//
// Every compose already tells the agent what it made: the brand, the template,
// the canvas it landed on, the title. Until now that knowledge died with the
// session, and the next session in the same workspace rediscovered all of it
// with fresh registry reads and, often, a fresh mint of a canvas that already
// existed. This hook writes the compose result into the workspace as a small
// local file the session-start hook serves back next time. No server is
// asked anything; the memory is the emission itself.
//
// Type-1 artefacts still compose fresh by default: this is context for the
// next session (which canvas exists, what it was), never an instruction to
// reuse it. Written only when a compose actually landed a canvas.
//
// Node built-ins only. Never throws, never blocks the turn, never writes on
// a tool that is not the compose tool or on a result that carries no canvas.

import fs from 'node:fs'
import path from 'node:path'

const KEEP_LOG = 20

export function extractEpilogue(input) {
  const toolName = String(input?.tool_name ?? input?.toolName ?? '')
  if (!/less_canvas_compose$/.test(toolName)) return null
  const args = input?.tool_input ?? input?.toolInput ?? input?.input ?? {}
  const resp = input?.tool_response ?? input?.toolResponse ?? input?.result ?? null
  // The response is a content array of text blocks; read the text itself.
  // Stringifying the array escapes every inner quote and hides the fields.
  const text = responseText(resp)
  const brand = str(args.brand_slug)
  const template = str(args.template_id)
  if (!brand || !template) return null
  // THE WRITE HAS TO HAVE LANDED, AND THE SERVER HAS TO SAY SO.
  //
  // This used to read: not an error, therefore a success. Two things made that
  // wrong at once. The error test searched `text` for the characters
  // `"isError": true`, and `text` is built by responseText(), which walks INTO
  // `content` and drops every sibling field — so the flag it was looking for had
  // already been thrown away and the guard could not fire on a real refusal. And
  // the success test was the absence of three phrases, so every failure nobody
  // had thought of read as a success.
  //
  // Order matters below: refuse on a stated error, then require a receipt. The
  // receipt is the server's own, re-read from the session row after the write.
  if (errored(resp, text)) return null
  if (!hasReceipt(resp, text)) return null
  const session = str(args.session_id) ?? firstMatch(text, /\\?"session_id\\?"\s*:\s*\\?"([0-9a-f-]{36})/i) ?? firstMatch(text, /session[=:\s"]+([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i)
  // The id can come from the REQUEST, so on a continuation compose it is present
  // whether or not anything landed. It is the value, never the evidence — the
  // receipt above is the evidence.
  if (!session) return null
  const open = firstMatch(text, /(designless:\/\/canvas\?[^\s"'`)]+)/)
  const slides = firstMatch(text, /\\?"slide_count\\?"\s*:\s*(\d+)/)
  return {
    composed_at: new Date().toISOString(),
    brand_slug: brand,
    template_id: template,
    session_id: session,
    title: str(args.title) ?? null,
    slide_count: slides ? Number(slides) : null,
    open_url: open ?? null,
  }
}

/**
 * Did the call state an error? Read as VALUES, wherever the envelope carries it.
 *
 * Hosts hand this hook the tool result in more than one shape: bare, wrapped in
 * `result`, sometimes only its `content`. So the flag is looked for at each of
 * those levels rather than in a string that may no longer contain it. The text
 * phrases stay as a last line for a server that refuses in prose without setting
 * the flag.
 */
export function errored(resp, text = '') {
  const flags = [resp, resp?.result, resp?.tool_response]
  for (const r of flags) {
    if (r && typeof r === 'object' && !Array.isArray(r)) {
      if (r.isError === true || r.is_error === true) return true
      if (r.error) return true
    }
  }
  return /manifest_conflict|Nothing was written/i.test(String(text))
}

/**
 * The server's own receipt that the write landed.
 *
 * `verified` is re-read from the session row AFTER the write, on every success
 * path in the compose route, so its presence is the one positive fact available
 * here. Three forms are accepted because three are in the field: the structured
 * block when the host passes `_meta` through, the rendered table the tool prints
 * for clients that ignore `_meta`, and the older inline JSON. Accepting all
 * three matters more than tidiness: a receipt this misses is a compose that
 * silently stops being remembered, which is the same class of quiet wrong the
 * old guard was.
 */
export function hasReceipt(resp, text = '') {
  const meta = resp?._meta?.verified ?? resp?.result?._meta?.verified
  if (meta && typeof meta === 'object') return true
  const t = String(text)
  if (/\*\*Verified\*\*/i.test(t)) return true
  if (/\bverified\b\s*:?\s*[{|]/i.test(t)) return true
  return false
}

function responseText(resp) {
  if (resp == null) return ''
  if (typeof resp === 'string') return resp
  if (Array.isArray(resp)) return resp.map((b) => (b && typeof b.text === 'string') ? b.text : JSON.stringify(b)).join('\n')
  if (typeof resp === 'object') {
    if (Array.isArray(resp.content)) return responseText(resp.content)
    return JSON.stringify(resp)
  }
  return String(resp)
}
function str(v) { return typeof v === 'string' && v.length > 0 && v.length < 300 ? v : null }
function firstMatch(text, re) { const m = re.exec(text); return m ? m[1] : null }

/**
 * Make the folder ignore itself, so nothing we write can reach a commit.
 *
 * This folder is ours, not the user's: a session id, a cached branch, a compose
 * log, document titles, deep links. None of it is a credential and all of it is
 * theirs, but none of it belongs in a diff a colleague reviews, and it is one
 * `git add -A` away from getting there. Our own repos proved it: three had a
 * rule for this, added by hand on one afternoon, and the fourth did not, so the
 * folder turned up untracked in a release check.
 *
 * A `.gitignore` INSIDE the folder is what git calls a nested ignore, and it is
 * the right instrument here for three reasons. It never touches a tracked file,
 * so we modify nothing the user has committed and nothing appears in their diff.
 * It travels with the checkout, so a teammate who runs this is covered too,
 * which `.git/info/exclude` cannot do. And it needs no question: there is no
 * version of this the user would answer differently, so asking would be a speed
 * bump with one right answer.
 *
 * Written only when absent. If someone has put their own rules here, they
 * outrank ours.
 */
export function ensureSelfIgnored(dir) {
  const p = path.join(dir, '.gitignore')
  try {
    if (fs.existsSync(p)) return false
    fs.writeFileSync(p, '# Designless keeps local session notes here. Not for committing.\n*\n')
    return true
  } catch { return false }
}

export function writeEpilogue(cwd, epilogue) {
  const dir = path.join(cwd, '.designless')
  fs.mkdirSync(dir, { recursive: true })
  ensureSelfIgnored(dir)
  fs.writeFileSync(path.join(dir, 'compose-epilogue.json'), JSON.stringify(epilogue, null, 2) + '\n')
  const logPath = path.join(dir, 'compose-log.jsonl')
  let lines = []
  try { lines = fs.readFileSync(logPath, 'utf8').split('\n').filter(Boolean) } catch { /* first entry */ }
  lines.push(JSON.stringify(epilogue))
  fs.writeFileSync(logPath, lines.slice(-KEEP_LOG).join('\n') + '\n')
}

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let input
  try { input = JSON.parse(raw) } catch { return }
  const cwd = typeof input?.cwd === 'string' && input.cwd ? input.cwd : process.cwd()
  const epilogue = extractEpilogue(input)
  if (!epilogue) return
  try { writeEpilogue(cwd, epilogue) } catch { /* fail-open */ }
}

if (process.argv[1] && import.meta.url.endsWith(path.basename(process.argv[1]))) {
  main().then(() => process.exit(0)).catch(() => process.exit(0))
}
