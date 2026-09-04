/**
 * Every hook this plugin ships is actually registered.
 *
 * THE DEFECT this closes: the compose epilogue was added with its event key
 * placed BESIDE the `hooks` object rather than inside it. The host reads events
 * from `hooks`, so a sibling key registers nothing: the hook shipped in every
 * release from 1.12.30 on and never once ran. It is the quietest kind of
 * broken — the file is present, the script is correct, and nothing calls it.
 *
 * So the test is not "does the file parse" but "is every shipped hook script
 * reachable from a registered event".
 */
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

const DIR = path.dirname(fileURLToPath(import.meta.url))
const config = JSON.parse(readFileSync(path.join(DIR, 'hooks.json'), 'utf8'))

test('every event is registered inside `hooks`, never beside it', () => {
  assert.deepEqual(Object.keys(config), ['hooks'], 'a key outside `hooks` registers nothing')
  for (const [event, entries] of Object.entries(config.hooks)) {
    assert.ok(Array.isArray(entries) && entries.length > 0, `${event} has no entries`)
  }
})

test('every hook script this plugin ships is reachable from an event', () => {
  const commands = JSON.stringify(config.hooks)
  const shipped = readdirSync(DIR)
    .filter((f) => f.endsWith('.mjs') && !f.endsWith('.test.mjs'))
    // inbox-probe is a library the others import, not an entry point.
    .filter((f) => f !== 'inbox-probe.mjs')
  for (const script of shipped) {
    assert.ok(commands.includes(script), `${script} ships but no event runs it`)
  }
})

test('the compose epilogue runs after a compose, and nothing else', () => {
  const post = config.hooks.PostToolUse
  assert.ok(Array.isArray(post), 'PostToolUse must be registered')
  assert.equal(post[0].matcher, 'less_canvas_compose$')
  assert.match(post[0].hooks[0].command, /compose-epilogue\.mjs/)
})
