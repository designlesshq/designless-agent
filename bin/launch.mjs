#!/usr/bin/env node
//
// bin/launch.mjs — OS / architecture dispatcher for `designless-mcp-bridge`.
//
// Claude Code's `.mcp.json` static config can't conditionally pick a binary
// per platform. This launcher does it at spawn time: it inspects
// `process.platform` and `process.arch`, then execs the matching binary as
// a stdio MCP server. Stdio is inherited so Claude Code talks to the bridge
// transparently — the launcher is a thin shim, not a real process boundary.
//
// Supported targets today: `darwin/arm64` (Apple Silicon Macs). Cowork uses
// the same binary because Cowork executes plugin MCP servers natively on
// the macOS host, not inside its Linux sandbox VM (Cowork desktop
// architecture overview, support.claude.com/en/articles/14479288).
//
// Everything else — Intel Mac, Windows, Linux desktops — gets a clear
// "platform not yet supported" message routed to stderr so Claude Code's
// `/mcp` panel surfaces it cleanly, plus a non-zero exit code so the host
// flags the MCP server as failed rather than silently hanging.
//
// Before spawning, the launcher also runs a desktop-app pre-flight (see
// resolveMode below) and hands the bridge a deterministic auth mode via
// DESIGNLESS_BRIDGE_MODE, so an installed desktop app is preferred over a
// silent browser-OAuth fallback.

import { spawn, execFile, execFileSync } from 'node:child_process'
import { connect } from 'node:net'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { existsSync } from 'node:fs'

const __dirname = dirname(fileURLToPath(import.meta.url))

const platform = process.platform // 'darwin' | 'linux' | 'win32' | …
const arch = process.arch         // 'arm64' | 'x64' | …

if (platform !== 'darwin' || arch !== 'arm64') {
  process.stderr.write(
    [
      `Designless MCP bridge: this platform (${platform}/${arch}) is not currently supported.`,
      'Apple Silicon Macs are the only shipped target today.',
      'Use the web app at https://designless.app instead, or watch the changelog for new platform support.',
    ].join('\n') + '\n'
  )
  process.exit(1)
}

const binaryPath = join(__dirname, 'designless-mcp-bridge-darwin-arm64')

if (!existsSync(binaryPath)) {
  process.stderr.write(
    [
      `Designless MCP bridge: binary not found at ${binaryPath}.`,
      'The plugin install may be incomplete. Try `claude plugin update designless@designless-plugins`.',
      'If the issue persists, reinstall with `claude plugin install designless@designless-plugins`.',
    ].join('\n') + '\n'
  )
  process.exit(1)
}

// ── Desktop-app pre-flight ──────────────────────────────────────────────
// The bridge can authenticate two ways: "anchored" (token owned by the
// Designless desktop app, read over local IPC, no browser) or "standalone"
// (the bridge runs its own browser OAuth and stores its own token). On a
// machine where the desktop app is installed, anchored is the correct path;
// silently doing browser OAuth there creates a second, divergent identity.
//
// Claude Code spawns this launcher with inherited stdio and no interactive
// channel, so we can't ask here. Instead: detect the app, bring it up if it
// is installed but not running, wait briefly for its IPC socket, and hand
// the bridge a deterministic mode. Escape hatches: set DESIGNLESS_BRIDGE_MODE
// explicitly to bypass detection, or DESIGNLESS_BRIDGE_NO_AUTOLAUNCH=1 to
// skip the auto-open (still prefers anchored, just won't launch the app).

const BUNDLE_ID = 'com.designless.canvas'
const APP_PATHS = [
  '/Applications/Designless.app',
  join(process.env.HOME ?? '', 'Applications', 'Designless.app'),
]

function appInstalled() {
  if (APP_PATHS.some((p) => p && existsSync(p))) return true
  try {
    const out = execFileSync(
      'mdfind',
      [`kMDItemCFBundleIdentifier == '${BUNDLE_ID}'`],
      { encoding: 'utf8', timeout: 3000 }
    )
    return out.trim().length > 0
  } catch {
    return false
  }
}

function socketPath() {
  return join(process.env.TMPDIR || '/tmp', 'Designless.sock')
}

function probeSocket(timeoutMs) {
  return new Promise((resolve) => {
    let settled = false
    const finish = (ok) => {
      if (settled) return
      settled = true
      clearTimeout(timer)
      sock.destroy()
      resolve(ok)
    }
    const sock = connect(socketPath())
    const timer = setTimeout(() => finish(false), timeoutMs)
    sock.once('connect', () => finish(true))
    sock.once('error', () => finish(false))
  })
}

function launchApp() {
  return new Promise((resolve) => {
    execFile('open', ['-b', BUNDLE_ID], (err) => {
      if (!err) return resolve(true)
      execFile('open', ['-a', 'Designless'], (err2) => resolve(!err2))
    })
  })
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function resolveMode() {
  // Respect an explicit choice (CI, power users, Cowork overrides).
  const explicit = process.env.DESIGNLESS_BRIDGE_MODE
  if (explicit) return explicit

  // No desktop app: genuine standalone environment (CI, sandbox, web-only).
  if (!appInstalled()) return 'standalone'

  // App already running.
  if (await probeSocket(400)) return 'anchored'

  // Installed but not running.
  if (process.env.DESIGNLESS_BRIDGE_NO_AUTOLAUNCH) return 'anchored'

  process.stderr.write(
    'Designless: desktop app is installed but not running — opening it to authenticate…\n'
  )
  await launchApp()

  const deadline = Date.now() + 20_000
  while (Date.now() < deadline) {
    if (await probeSocket(500)) return 'anchored'
    await sleep(500)
  }

  // App didn't come up in time: still require anchored so the bridge shows an
  // "open Designless" hint in /mcp instead of silently doing browser OAuth.
  process.stderr.write(
    'Designless: desktop app did not become ready in time. Open Designless and ' +
      'sign in, then reconnect this MCP server from the /mcp panel.\n'
  )
  return 'anchored'
}

const mode = await resolveMode()

const child = spawn(binaryPath, [], {
  stdio: 'inherit',
  env: { ...process.env, DESIGNLESS_BRIDGE_MODE: mode },
})

child.on('exit', (code, signal) => {
  if (signal != null) {
    process.kill(process.pid, signal)
  } else {
    process.exit(code ?? 1)
  }
})

child.on('error', (err) => {
  process.stderr.write(
    `Designless MCP bridge: failed to spawn ${binaryPath}: ${err.message}\n`
  )
  process.exit(1)
})
