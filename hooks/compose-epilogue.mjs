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
  const session = str(args.session_id) ?? firstMatch(text, /\\?"session_id\\?"\s*:\s*\\?"([0-9a-f-]{36})/i) ?? firstMatch(text, /session[=:\s"]+([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i)
  if (!session) return null
  if (/manifest_conflict|"isError"\s*:\s*true|Nothing was written/i.test(text)) return null
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

export function writeEpilogue(cwd, epilogue) {
  const dir = path.join(cwd, '.designless')
  fs.mkdirSync(dir, { recursive: true })
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
