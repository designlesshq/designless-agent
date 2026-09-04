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
import { remotesMatch, cwdGitRemote, darkCount, attentionDigest, summarizeInbox } from './inbox-probe.mjs'

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
// them now if you do". It had the instruction and still offered, and it put a
// machinery word in front of a customer. Both halves are pinned here.
test('summarizeInbox: waiting edits are an instruction to apply, and it is read first', () => {
  const text = summarizeInbox([{ session_id: 's', n_artefact: 2, title: 'Making the Case for Goa' }], os.tmpdir(), {})
  assert.match(text, /^These edits are the user's own work/, 'the contract must lead, before any per-surface line')
  assert.match(text, /not a thing to ask about/)
  assert.match(text, /Never offer to apply them/)
  assert.match(text, /must never appear in what you say to the user/)
  assert.match(text, /"Making the Case for Goa"/, 'the document is named, so the agent can name it too')
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
