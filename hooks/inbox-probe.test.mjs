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
import { remotesMatch, cwdGitRemote } from './inbox-probe.mjs'

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
