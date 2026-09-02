// The compose epilogue is written only for a landed compose, carries what the
// next session needs, and never for anything else. Run: node --test hooks/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { extractEpilogue, writeEpilogue } from './compose-epilogue.mjs'
import { epilogueLine } from './session-start-inbox.mjs'

const COMPOSE = 'mcp__plugin_designless_less-mcp__less_canvas_compose'
const landed = {
  tool_name: COMPOSE,
  tool_input: { brand_slug: 'acme', template_id: 'hot-take-acid', title: 'Why reviews get shorter', session_id: '11111111-2222-4333-8444-555555555555' },
  tool_response: [{ type: 'text', text: 'Composed. verified: {"brand_slug":"acme","template_id":"hot-take-acid","slide_count":5,"session_id":"11111111-2222-4333-8444-555555555555"} open designless://canvas?brand=acme&session=11111111-2222-4333-8444-555555555555&template=hot-take-acid' }],
}

test('a landed compose yields an epilogue with brand, template, canvas, title, slides, link', () => {
  const e = extractEpilogue(landed)
  assert.equal(e.brand_slug, 'acme')
  assert.equal(e.template_id, 'hot-take-acid')
  assert.equal(e.session_id, '11111111-2222-4333-8444-555555555555')
  assert.equal(e.title, 'Why reviews get shorter')
  assert.equal(e.slide_count, 5)
  assert.match(e.open_url, /^designless:\/\/canvas\?/)
})

test('another tool, a refused compose, and a compose with no canvas write nothing', () => {
  assert.equal(extractEpilogue({ ...landed, tool_name: 'mcp__x__less_canvas_status' }), null)
  assert.equal(extractEpilogue({ ...landed, tool_response: [{ type: 'text', text: 'manifest_conflict: the canvas moved' }] }), null)
  assert.equal(extractEpilogue({ ...landed, tool_input: { brand_slug: 'acme', template_id: 't' }, tool_response: 'ok' }), null)
})

test('the file lands in the workspace, the log is bounded, and session start serves one line', () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'epilogue-'))
  const e = extractEpilogue(landed)
  for (let i = 0; i < 25; i++) writeEpilogue(cwd, { ...e, title: `t${i}` })
  const log = fs.readFileSync(path.join(cwd, '.designless', 'compose-log.jsonl'), 'utf8').trim().split('\n')
  assert.equal(log.length, 20)
  const line = epilogueLine(cwd)
  assert.ok(line && line.includes('t24') && line.includes('hot-take-acid') && line.includes('acme'), line)
  assert.ok(!/[—]/.test(line), 'no emdashes in served text')
  // A stale epilogue is not served.
  const stale = { ...e, composed_at: new Date(Date.now() - 30 * 86400000).toISOString() }
  fs.writeFileSync(path.join(cwd, '.designless', 'compose-epilogue.json'), JSON.stringify(stale))
  assert.equal(epilogueLine(cwd), null)
  assert.equal(epilogueLine(fs.mkdtempSync(path.join(os.tmpdir(), 'empty-'))), null)
})
