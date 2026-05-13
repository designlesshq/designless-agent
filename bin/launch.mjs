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

import { spawn } from 'node:child_process'
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

const child = spawn(binaryPath, [], { stdio: 'inherit' })

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
