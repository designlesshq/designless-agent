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
    // Not every .mjs here is a hook. Each exclusion names what it IS, so the
    // list cannot quietly become the place a genuinely unregistered hook hides.
    //   inbox-probe   - a library the hooks import
    //   watch-marker  - a library the hooks and the watcher import
    //   inbox-watch   - the live watcher: a background task the AGENT starts, on
    //                   the ask canvas-arm-watch emits. No event can run it, and
    //                   an event that did would run it once per turn and exit.
    .filter((f) => !['inbox-probe.mjs', 'watch-marker.mjs', 'inbox-watch.mjs'].includes(f))
  for (const script of shipped) {
    assert.ok(commands.includes(script), `${script} ships but no event runs it`)
  }
})

// Two hooks share PostToolUse now and they must NOT share a matcher. The
// epilogue writes the workspace's compose memory and belongs to compose alone;
// the watcher ask belongs to every canvas tool, because a canvas coming into
// play is the moment to want a watcher whatever call brought it.
test('the compose epilogue runs after a compose, and nothing else', () => {
  const post = config.hooks.PostToolUse
  assert.ok(Array.isArray(post), 'PostToolUse must be registered')
  const epilogue = post.find((e) => /compose-epilogue\.mjs/.test(JSON.stringify(e.hooks)))
  assert.ok(epilogue, 'the compose epilogue must be registered')
  assert.equal(epilogue.matcher, 'less_canvas_compose$')
})

test('the watcher ask runs after every canvas tool, not only compose', () => {
  const post = config.hooks.PostToolUse
  const armer = post.find((e) => /canvas-arm-watch\.mjs/.test(JSON.stringify(e.hooks)))
  assert.ok(armer, 'the watcher ask must be registered')
  assert.equal(armer.matcher, 'less_canvas_')
  // A matcher anchored to one tool is the bug this entry exists to avoid: the
  // ask has to reach a session that composed, opened, updated or walked a
  // canvas, not just one that called the single tool someone thought of.
  assert.doesNotMatch(armer.matcher, /\$$/, 'the ask must not be anchored to one tool')
})
