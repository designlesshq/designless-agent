// The compose epilogue is written only for a landed compose, carries what the
// next session needs, and never for anything else. Run: node --test hooks/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { extractEpilogue, errored, writeEpilogue } from './compose-epilogue.mjs'
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

// ── The write has to have landed, and the server has to say so ─────────────
//
// THE DEFECT: this hook decided a compose succeeded by failing to find one of
// three phrases in the response text. The error flag it looked for had already
// been dropped — responseText() walks INTO `content` and discards its siblings,
// `isError` among them — so a real refusal read as a success, and the next
// session in that workspace was told a canvas existed and a continuation
// belonged there. Reproduced against 1.12.38 with the payload below.

test('a refusal is never remembered as a compose', () => {
  const refused = [
    { isError: true, content: [{ type: 'text', text: 'Upstream request timed out' }] },
    { is_error: true, content: [{ type: 'text', text: 'nope' }] },
    { error: 'boom' },
    { result: { isError: true, content: [{ type: 'text', text: 'nope' }] } },
  ]
  for (const tool_response of refused) {
    assert.equal(extractEpilogue({ ...landed, tool_response }), null, JSON.stringify(tool_response).slice(0, 60))
  }
})

// The flag is a VALUE. Searching a string for its printed form is what failed,
// and it failed silently, which is why this asserts on the function directly.
test('the error flag is read where it lives, not from a stringification', () => {
  assert.equal(errored({ isError: true, content: [{ type: 'text', text: 'ok' }] }), true)
  assert.equal(errored({ content: [{ type: 'text', text: 'ok' }] }), false)
})

// Absence of a known error phrase is not proof of anything. A compose is
// remembered only on the server's own receipt, re-read from the session row
// after the write.
test('a response with no receipt is not remembered, however calm it reads', () => {
  assert.equal(extractEpilogue({ ...landed, tool_response: [{ type: 'text', text: 'Composed.' }] }), null)
  assert.equal(extractEpilogue({ ...landed, tool_response: [{ type: 'text', text: 'All done, looks great.' }] }), null)
})

// Three receipt forms are in the field at once and all three must count. A
// receipt this missed would be a compose that quietly stopped being remembered,
// which is the same class of silent wrong as the bug it replaces.
test('every receipt the server actually emits is accepted', () => {
  const forms = [
    [{ type: 'text', text: '**Verified** (re-read from session row after write):\n| brand_slug | acme |' }],
    [{ type: 'text', text: 'Composed. verified: {"brand_slug":"acme"}' }],
    { _meta: { verified: { brand_slug: 'acme' } }, content: [{ type: 'text', text: 'ok' }] },
  ]
  for (const tool_response of forms) {
    assert.ok(extractEpilogue({ ...landed, tool_response }), JSON.stringify(tool_response).slice(0, 60))
  }
})

// A continuation compose passes the session id IN, so it is present whether or
// not anything landed. It is the value; it was never the evidence.
test('a session id in the request is not evidence the write happened', () => {
  const carried = { ...landed, tool_input: { ...landed.tool_input, session_id: '11111111-2222-4333-8444-555555555555' } }
  assert.equal(extractEpilogue({ ...carried, tool_response: { isError: true, content: [{ type: 'text', text: 'timed out' }] } }), null)
})

// ── The folder ignores itself ──────────────────────────────────────────────

test('the folder we create keeps itself out of the user\'s commits', () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'epilogue-ignore-'))
  writeEpilogue(cwd, { composed_at: new Date().toISOString(), brand_slug: 'acme', template_id: 'deck', session_id: 'x' })
  const rule = fs.readFileSync(path.join(cwd, '.designless', '.gitignore'), 'utf8')
  assert.match(rule, /^\*$/m, 'the nested ignore must cover everything in the folder')
})

// Rules someone put here themselves outrank ours.
test('an existing ignore file is left alone', () => {
  const cwd = fs.mkdtempSync(path.join(os.tmpdir(), 'epilogue-ignore-'))
  fs.mkdirSync(path.join(cwd, '.designless'), { recursive: true })
  fs.writeFileSync(path.join(cwd, '.designless', '.gitignore'), 'mine\n')
  writeEpilogue(cwd, { composed_at: new Date().toISOString(), brand_slug: 'acme', template_id: 'deck', session_id: 'x' })
  assert.equal(fs.readFileSync(path.join(cwd, '.designless', '.gitignore'), 'utf8'), 'mine\n')
})
