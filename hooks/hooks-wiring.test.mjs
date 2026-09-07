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
// A REAL tool name, as the host delivers it. Measured 2026-09-08: the matchers
// were written as if the name were bare (`less_canvas_`), the hooks never fired
// for a whole session of canvas work, and nothing said so — an un-armed watcher
// is indistinguishable from a quiet one. The assertions below are therefore
// BEHAVIOURAL: they compile the configured matcher and run it against the names
// that actually arrive. A string equality cannot see this class of bug at all,
// which is how it survived.
const QUALIFIED = (tool) => `mcp__plugin_designless_less-mcp__${tool}`

// Semantics are the host's, and we do not get to know them: some match a
// matcher as a substring search, some as a full-string match. A matcher that
// only works under one reading is half-wired, so both are asserted.
const matchesEitherWay = (matcher, name) =>
  new RegExp(matcher).test(name) && new RegExp(`^(?:${matcher})$`).test(name)

test('the compose epilogue runs after a compose, and nothing else', () => {
  const post = config.hooks.PostToolUse
  assert.ok(Array.isArray(post), 'PostToolUse must be registered')
  const epilogue = post.find((e) => /compose-epilogue\.mjs/.test(JSON.stringify(e.hooks)))
  assert.ok(epilogue, 'the compose epilogue must be registered')
  assert.ok(
    matchesEitherWay(epilogue.matcher, QUALIFIED('less_canvas_compose')),
    'the epilogue must match a real compose call under either matcher semantics',
  )
  // Compose alone, still: it writes the workspace's compose memory.
  assert.ok(!new RegExp(epilogue.matcher).test(QUALIFIED('less_canvas_status')), 'compose only')
  assert.match(epilogue.matcher, /\$$/, 'anchored to compose on purpose')
})

test('the watcher ask runs after every Designless tool, not only the canvas ones', () => {
  const post = config.hooks.PostToolUse
  const armer = post.find((e) => /canvas-arm-watch\.mjs/.test(JSON.stringify(e.hooks)))
  assert.ok(armer, 'the watcher ask must be registered')

  // Founder ruling 2026-09-08: ANY Designless tool is reason to want a watcher,
  // not the canvas family alone. Arming only at the first canvas call means the
  // edits made before it are the ones that wait.
  for (const tool of [
    'less_canvas_compose',
    'less_canvas_status',
    'less_artefact_open',
    'less_list_templates',
    'less_resolve_brand',
  ]) {
    assert.ok(
      matchesEitherWay(armer.matcher, QUALIFIED(tool)),
      `the ask must reach ${tool} under either matcher semantics`,
    )
  }

  // And must not fire on everything: a hook that runs after every Read and Bash
  // is a spawn per tool call for nothing.
  for (const other of ['Bash', 'Read', 'Edit']) {
    assert.ok(!new RegExp(`^(?:${armer.matcher})$`).test(other), `must not fire on ${other}`)
  }

  // A matcher anchored to one tool is the bug this entry exists to avoid.
  assert.doesNotMatch(armer.matcher, /\$$/, 'the ask must not be anchored to one tool')
})
