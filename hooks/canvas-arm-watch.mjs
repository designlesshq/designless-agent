#!/usr/bin/env node
// PostToolUse - ask for a live watcher the moment a canvas is in play.
//
// The watcher used to be a line in the skill: arm one when a canvas is in play.
// That line is only read when the skill is loaded, which needs the command, and
// it is a rule the agent has to remember at the right moment. So in practice it
// was armed almost never, and edits made while the agent sat idle waited for the
// next thing the user typed - which is the opposite of what the canvas promises.
//
// The ask now arrives AT the moment instead: whenever a Designless tool returns,
// and whether or not this session ever ran the command. A hook cannot start a
// background task (it returns text and exits), so this is still an ask. What
// makes it reliable is that it repeats: the WATCHER writes the marker, so an ask
// that was ignored looks exactly like one never sent, and the next canvas call
// asks again.
//
// Silent whenever a watcher is already alive, which is nearly always. Fail-open:
// any error exits 0 with no output, and the turn is never blocked.

import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { isArmed } from './watch-marker.mjs'

const HERE = path.dirname(fileURLToPath(import.meta.url))
export const WATCHER = path.join(HERE, 'inbox-watch.mjs')

/**
 * Does this tool call mean a canvas is in the picture?
 *
 * ANY Designless tool does (founder ruling 2026-09-08). It used to be the
 * canvas tools alone, which reads the intent too narrowly: an agent resolving a
 * brand, listing templates or reading a capsule is working on something that
 * will be painted, and arming only once the first canvas call lands means the
 * edits made before it are the ones that wait. The watcher is cheap and refuses
 * to start twice; a session that never opens a canvas costs one quiet process.
 *
 * ONE exception survives, for a reason that is not about breadth.
 * `less_canvas_inbox` is the discovery read that runs at the start of every turn
 * whether or not the user has ever opened a canvas, so it is not evidence that
 * anyone chose to do anything — it counts only when it comes back naming a
 * canvas. Every other tool here was CALLED deliberately, which is the signal.
 *
 * Matched on `(^|_)less_`, not a bare substring. Names arrive MCP-qualified
 * (`mcp__plugin_designless_less-mcp__less_canvas_ops`) and also bare, so the
 * boundary has to admit both; requiring the underscore is what keeps an
 * unrelated `harmless_thing` out, since the character before its `less_` is
 * an `m`.
 */
export function canvasInPlay(toolName, responseText) {
  const name = String(toolName ?? '')
  if (!/(^|_)less_/.test(name)) return false
  if (/less_canvas_inbox$/.test(name)) {
    const t = String(responseText ?? '')
    // Read the NEGATIVE first. Both empty answers open "# Designless: nothing"
    // ("nothing waiting", and "nothing to apply, but an edit still waits for the
    // user"), and both contain the word "waiting" — so any positive-only test
    // arms a watcher for a session with nothing to watch, including one whose
    // only item belongs to the user and cannot be acted on at all.
    if (/#\s*Designless:\s*nothing/i.test(t)) return false
    return /edit\(s\)|annotation\(s\)|session_id/i.test(t)
  }
  return true
}

/**
 * The longest watch the host allows. A Claude Code monitor is capped at thirty
 * minutes whatever is asked for, and expires with one notice either way; an
 * ask that names no length got a ten-minute watch and three times the wakes.
 */
export const WATCH_MS = 30 * 60_000

/**
 * The ask, and the two things in it that decide whether the watcher works.
 *
 * THE FACILITY. The watcher speaks by printing a line. It only helps if that
 * line reaches the agent WHILE the process keeps running, so the facility it is
 * started under has to stream its output. In Claude Code that is the Monitor
 * tool with persistence on. A plain background command is the wrong facility:
 * it reports once, when the process exits, and a watcher never exits on its
 * own, so every line it prints goes unheard for the whole session while it
 * looks alive. The ask therefore names the right facility and says why the
 * other will not do, rather than leaving "background task" to be read either
 * way.
 *
 * THE EXPIRY. A host watch has a ceiling and ends with one notice that says,
 * in the host's words, to re-arm if the watch is still needed. That sentence
 * was a loop: the agent re-armed, read the inbox because a
 * turn had begun, this hook saw a canvas in play with no watcher and asked
 * again, and the three repeated every ten minutes for as long as the session
 * lived, six wakes in forty minutes with no edit in any of them, in a paying
 * customer's session as much as in the harness. So the ask says what the
 * notice cannot: an expiry that delivered no event is the end of the watch.
 * Nothing is lost by it ending. The turn-boundary hook reads the inbox the
 * moment the person types, and the next Designless tool call arms a fresh
 * watcher through this same ask. The one wake the host insists on is spent
 * saying nothing.
 */
export function armLine(sessionId, watcher = WATCHER) {
  return 'Designless canvas: this session has a canvas in play and no live watcher running, so edits made ' +
    'while you are idle will wait for the next thing the user types. Start one now as a persistent ' +
    'background task whose printed lines reach you while you work: in Claude Code that is the Monitor ' +
    'tool with persistence on, never a plain background command, which reports only when the process ' +
    `exits and would leave every line unheard. Label it exactly "Designless Agent", give it the longest ` +
    `watch the host allows (timeout_ms ${WATCH_MS}), and run: node ${watcher} ${sessionId}. It stays silent ` +
    'until new edits arrive and prints one line when they do; apply them then, the same as at a turn ' +
    'boundary. It refuses to start a second one, so starting it when one is already running is harmless. ' +
    'When the host later reports that this watch expired and delivered no event, that is the end of the ' +
    'watch and not an ask: do not start another, do not read the inbox, say nothing and end the turn. The ' +
    'hooks cover the next thing the user types, and the next Designless tool call asks for a fresh watcher ' +
    'through this line. If this host has no such facility, ignore this line: the turn-boundary hooks ' +
    'remain the floor.'
}

export function decide(input, armed = isArmed) {
  const toolName = input?.tool_name ?? input?.toolName
  const sessionId = input?.session_id ?? input?.sessionId
  if (!sessionId) return null
  const resp = input?.tool_response ?? input?.toolResponse ?? input?.result
  const text = typeof resp === 'string' ? resp : JSON.stringify(resp ?? '')
  if (!canvasInPlay(toolName, text)) return null
  if (armed(sessionId)) return null
  return armLine(sessionId)
}

async function main() {
  let raw = ''
  for await (const chunk of process.stdin) raw += chunk
  let input
  try { input = JSON.parse(raw) } catch { return }
  const context = decide(input)
  if (!context) return
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: { hookEventName: 'PostToolUse', additionalContext: context },
  }))
}

if (process.argv[1] && import.meta.url.endsWith(path.basename(process.argv[1]))) {
  main().then(() => process.exit(0)).catch(() => process.exit(0))
}
