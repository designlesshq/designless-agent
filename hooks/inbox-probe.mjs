// inbox-probe.mjs - shared cwd-independent canvas-edit probe for the wake hooks.
//
// Phase-5 v3 (brain d8db8b78, §5). The old wakes gated on
// existsSync(cwd/.designless/session.json), which missed every case where the
// canvas renders a different repo than the Claude Code session is rooted in (the
// live prism-vs-skilldesign miss). Discovery is now keyed on USER IDENTITY via
// the desktop IPC `list_inbox` accelerator, so it surfaces waiting edits in ANY
// cwd. One honest dependency: a reachable, signed-in desktop. When the socket is
// absent / denied / stale, the probe is silently empty and the canvas "waiting"
// pill remains the human floor.
//
// Node built-ins only, fail-open on any error or timeout. Never throws.

import net from 'node:net'
import fs from 'node:fs'
import path from 'node:path'

const TIMEOUT_MS = 700

// ── Server/IPC input validation (trust boundary) ─────────────────────────────
// safety_branch and repo_remote arrive from the desktop IPC / server and get
// embedded VERBATIM into git checkout/push instruction text handed to the agent.
// Validate them at the boundary so a malformed value can never reach that text.

/**
 * A safety branch must live under the server-owned `designless/` namespace. The
 * suffix is deliberately permissive — legitimate names are BOTH `designless/<hex>`
 * AND agent-authored names like `designless/skilldesign-edits` — but restricted to
 * safe branch characters only: no whitespace, no shell metacharacters (; & $ ( ) | < > etc.).
 */
export function isSafeBranchName(b) {
  return typeof b === 'string' && /^designless\/[A-Za-z0-9._/-]+$/.test(b)
}

/**
 * A repo remote must look like a URL or `owner/repo` and carry no shell
 * metacharacters — never an injectable string.
 */
export function isSafeRepoRemote(r) {
  if (typeof r !== 'string') return false
  const s = r.trim()
  if (!s || /[\s;&$()|<>`'"\\]/.test(s)) return false
  return /^(https?:\/\/|git@)[\w.@:/~-]+$/.test(s) || /^[\w.-]+\/[\w.-]+$/.test(s)
}

/**
 * Drop any inbox row carrying a PRESENT-but-malformed server/IPC identifier. A
 * null/absent safety_branch (un-stamped legacy) or repo_remote (unknown checkout)
 * is allowed through — only a present, malformed value is rejected. Fail closed:
 * a bad row is simply not surfaced (consistent with the probe's fail-open-to-empty).
 */
export function sanitizeInboxRows(rows) {
  if (!Array.isArray(rows)) return []
  return rows.filter((s) => {
    if (!s || typeof s !== 'object') return false
    if (s.safety_branch != null && !isSafeBranchName(s.safety_branch)) return false
    if (s.repo_remote != null && !isSafeRepoRemote(s.repo_remote)) return false
    return true
  })
}

/** The desktop IPC socket path + its parent dir, derived from getuid(). */
function socketPath() {
  const uid = typeof process.getuid === 'function' ? process.getuid() : null
  if (process.platform === 'darwin') {
    if (uid == null) return null
    const dir = `/tmp/designless-${uid}`
    return { dir, sock: path.join(dir, 'ipc.sock') }
  }
  // Linux: XDG_RUNTIME_DIR/Designless, else /tmp/designless-<uid>
  const xdg = process.env.XDG_RUNTIME_DIR
  if (xdg) { const dir = path.join(xdg, 'Designless'); return { dir, sock: path.join(dir, 'ipc.sock') } }
  if (uid != null) { const dir = `/tmp/designless-${uid}`; return { dir, sock: path.join(dir, 'ipc.sock') } }
  return null
}

/** Refuse a socket dir that isn't owner-only 0700 (a same-uid-malware guard). */
function dirIsSafe(dir) {
  try {
    const st = fs.statSync(dir)
    if (!st.isDirectory()) return false
    if (typeof process.getuid === 'function' && st.uid !== process.getuid()) return false
    if ((st.mode & 0o077) !== 0) return false   // any group/other bit → unsafe
    return true
  } catch { return false }
}

/**
 * Probe the desktop inbox over the IPC socket. Resolves { count, sessions } -
 * always {count:0,sessions:[]} on any non-inbox reply, missing socket, or error.
 */
export function probeInbox() {
  return new Promise((resolve) => {
    const empty = { count: 0, sessions: [] }
    const sp = socketPath()
    if (!sp || !dirIsSafe(sp.dir) || !fs.existsSync(sp.sock)) return resolve(empty)

    let buf = ''
    let done = false
    let sock
    const finish = (v) => {
      if (done) return
      done = true
      clearTimeout(timer)
      try { sock && sock.destroy() } catch { /* already closed */ }
      resolve(v)
    }
    const timer = setTimeout(() => finish(empty), TIMEOUT_MS)

    try {
      sock = net.connect(sp.sock)
    } catch { return finish(empty) }

    sock.on('connect', () => {
      try { sock.write(JSON.stringify({ op: 'list_inbox' }) + '\n') } catch { finish(empty) }
    })
    sock.on('data', (chunk) => {
      buf += chunk.toString('utf8')
      const idx = buf.indexOf('\n')
      if (idx < 0) return
      let frame
      try { frame = JSON.parse(buf.slice(0, idx)) } catch { return finish(empty) }
      if (frame && frame.op === 'inbox') {
        const sessions = sanitizeInboxRows(frame.sessions)
        return finish({ count: sessions.length, sessions })
      }
      // denied / no_session / no_session_stale / error → silently empty.
      return finish(empty)
    })
    sock.on('error', () => finish(empty))
  })
}

// ── Right-checkout helpers (Stop hook gate, §5.2) ────────────────────────────

function normalizeRemote(u) {
  if (!u || typeof u !== 'string') return null
  return u.trim()
    .replace(/\.git$/, '')
    .replace(/^git@([^:]+):/, 'https://$1/')   // git@host:owner/repo → https://host/owner/repo
    .replace(/\/+$/, '')
    .toLowerCase()
}

/** The origin remote of the repo at `cwd`, normalized - or null (no git / no origin). */
export function cwdGitRemote(cwd) {
  try {
    const cfg = fs.readFileSync(path.join(cwd, '.git', 'config'), 'utf8')
    const m = cfg.match(/\[remote "origin"\][^[]*?url\s*=\s*([^\n]+)/)
    return m ? normalizeRemote(m[1]) : null
  } catch { return null }
}

/** True when two git remotes name the same repo (normalized). */
export function remotesMatch(a, b) {
  const na = normalizeRemote(a)
  const nb = normalizeRemote(b)
  return !!na && !!nb && na === nb
}

const sum = (rows, key) => rows.reduce((a, s) => a + Number(s[key] || 0), 0)

/**
 * Surface the required safety-branch name(s) for the drainable page sessions.
 * The branch is a SERVER-OWNED contract: the inbox row now carries it verbatim as
 * `safety_branch` (manifest._safety.branch, the same value the gate returns as
 * required_branch on a rejected claim). We READ it — never derive
 * `designless/<session_id>` client-side, which would drift from the server if the
 * stamping convention ever changes. A row with a null safety_branch is an un-stamped
 * legacy session: no branch is required, so it is simply omitted. Returns '' when no
 * row carries a branch.
 */
function requiredBranchHint(rows) {
  const branches = [...new Set(
    rows
      .map((s) => (s && isSafeBranchName(s.safety_branch) ? s.safety_branch : null))
      .filter(Boolean),
  )]
  if (!branches.length) return ''
  const label = branches.length > 1 ? 'Required safety branches' : 'Required safety branch'
  return ` ${label}: ${branches.join(', ')} (server-owned; read from each row's safety_branch, do NOT derive).`
}

/** Whether a page session is drainable from `cwd` (right checkout, §5.2). */
function pageDrainableHere(s, origin) {
  // Unknown checkout identity (no repo_remote, or no git here) → let the agent
  // decide per-op; a known mismatch routes the user instead of a wrong apply.
  return s.repo_remote ? remotesMatch(origin, s.repo_remote) : true
}

/**
 * Build the agent-facing wake text from the inbox rows, routed by surface and
 * checkout (§5.1/§5.2). Returns '' when there is nothing actionable to say.
 */
export function summarizeInbox(sessions, cwd) {
  const origin = cwdGitRemote(cwd)
  const here = [], elsewhere = [], artefact = [], annotations = [], attention = [], recoverable = []
  for (const s of sessions) {
    if (Number(s.n_page || 0) > 0) (pageDrainableHere(s, origin) ? here : elsewhere).push(s)
    if (Number(s.n_artefact || 0) > 0) artefact.push(s)
    if (Number(s.n_annotation || 0) > 0) annotations.push(s)
    if (Number(s.n_needs_human || 0) > 0) attention.push(s)
    if (s.recoverable) recoverable.push(s)
  }
  const lines = []
  if (here.length) {
    lines.push(
      `${sum(here, 'n_page')} page edit(s) are drainable from this checkout. These are Type-2 SOURCE ops - work BRANCH-FIRST: ` +
      `READ the required branch from the session's safety_branch field (on the less_canvas_inbox row, also on less_canvas_status), then ` +
      `git checkout -b <safety_branch> (or git checkout it if it already exists) BEFORE you claim - the server ` +
      `withholds every source op unless you are on that safety branch. If a session's safety_branch is null it is un-stamped: no branch is required. ` +
      `On EVERY source claim AND ack pass repo_branch (= git rev-parse --abbrev-ref HEAD) and checkout_head (= git rev-parse HEAD). ` +
      `Then drain with less_canvas_ops (claim -> apply each on previous_value, bottom-up per file -> ack), ` +
      `then let the canvas re-capture.` +
      requiredBranchHint(here),
    )
  }
  for (const s of elsewhere) {
    lines.push(`${Number(s.n_page || 0)} page edit(s) target ${s.repo_remote || s.source_hint || 'another repo'}; this session is rooted elsewhere - tell the user to run /designless from that repo (do NOT claim here).`)
  }
  if (artefact.length) {
    lines.push(`${sum(artefact, 'n_artefact')} artefact edit(s) waiting - inspect via less_canvas_status (artefact apply is rolling out; do not claim them yet).`)
  }
  if (annotations.length) {
    lines.push(`${sum(annotations, 'n_annotation')} annotation(s) waiting - read as context with less_canvas_ops action=peek, form your judgment, then ack them applied. They are not mechanical edits.`)
  }
  if (attention.length) {
    lines.push(`${sum(attention, 'n_needs_human')} edit(s) need the user - an earlier edit could not be safely applied (the file moved since the canvas captured it); ask them to re-open the canvas and redo it.`)
  }
  if (recoverable.length) {
    lines.push(`${recoverable.length} expired session(s) still hold un-applied edits; they revive in place when you drain them (no work is lost).`)
  }
  return lines.join(' ')
}
