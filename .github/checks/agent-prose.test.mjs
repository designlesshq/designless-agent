import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import { compareTrees, exceptionLabel, founderException, inspectRepository, issueEvents, rustSentences } from './agent-prose.mjs'

const tree = (files) => new Map(Object.entries(files))
const event = (id, login, action = 'labeled') => ({ id, event: action, label: { name: exceptionLabel }, actor: { login, type: 'User' } })

test('an added skill paragraph needs an exception', () => {
  const result = compareTrees(tree({ 'skills/example/SKILL.md': '# Example\n' }), tree({ 'skills/example/SKILL.md': '# Example\n\nAlways decide the whole direction first.\n' }))
  assert.equal(result.growth, true)
  assert.equal(result.textLines, 1)
  assert.deepEqual(result.files, [{ file: 'skills/example/SKILL.md', added: 1, removed: 0, lines: [3] }])
})

test('a deletion passes', () => {
  const result = compareTrees(tree({ 'agents/example.md': 'One\nTwo\n' }), tree({ 'agents/example.md': 'One\n' }))
  assert.equal(result.growth, false)
})

test('renames and moves with identical text pass across covered directories', () => {
  const result = compareTrees(tree({ 'skills/old/SKILL.md': '# Example\nKeep the context.\n' }), tree({ 'docs/new.txt': '# Example\nKeep the context.\n' }))
  assert.equal(result.textLines, 0)
  assert.equal(result.growth, false)
})

test('all declared text paths are covered, unrelated files are not', () => {
  for (const path of ['skills/a.md', 'agents/a.markdown', 'commands/a.txt', 'hooks/a.md', 'docs/a.txt', 'llms.txt']) {
    assert.equal(compareTrees(new Map(), tree({ [path]: 'Four ordinary guidance words.\n' })).growth, true, path)
  }
  assert.equal(compareTrees(new Map(), tree({ 'README.md': 'Public package description.\n', '.github/checks/a.mjs': 'CI source' })).growth, false)
})

test('a bridge sentence fails, including a replacement without line growth', () => {
  const result = compareTrees(tree({ 'bridge/src/main.rs': 'let x = "The previous agent instruction goes here.";' }), tree({ 'bridge/src/main.rs': 'let x = "The replacement agent instruction goes here.";' }))
  assert.equal(result.growth, true)
  assert.equal(result.bridgeLines, 1)
  assert.deepEqual(result.files[0].lines, [1])
})

test('moving or deleting bridge prose does not add it', () => {
  const before = tree({ 'bridge/src/old.rs': 'let x = "The same agent instruction goes here.";' })
  assert.equal(compareTrees(before, tree({ 'bridge/src/new.rs': 'let y = "The same agent instruction goes here.";' })).growth, false)
  assert.equal(compareTrees(before, new Map()).growth, false)
})

test('Rust comments, identifiers, format-only values and test files do not count', () => {
  const source = '// "This comment has enough words."\n/* nested /* "More words inside this comment." */ */\nlet id = "this_is_an_internal_identifier";\nlet fmt = "{} {name} {value:>4} {other}";\nlet path = "some/path/with/four/words";\nlet quote = \'"\';'
  assert.equal(rustSentences(source).length, 0)
  assert.equal(compareTrees(new Map(), tree({ 'bridge/src/main_test.rs': 'let x = "This test has enough words.";' })).growth, false)
})

test('raw strings, escaped quotes and multiple sentences are read', () => {
  assert.equal(rustSentences('let x = r##"First \'quoted\' instruction has enough words."##;').length, 1)
  assert.equal(rustSentences('let x = "This \\"quoted\\" instruction has enough words.";').length, 1)
  assert.equal(rustSentences('let x = "One two three"; let y = "One two three four";').length, 1)
})

test('inline test blocks are excluded without hiding later production code', () => {
  const source = '#[cfg(test)]\nmod tests { fn test() { let s = "This test text has enough words."; } }\nfn real() { let s = "This real text has enough words."; }'
  const found = rustSentences(source)
  assert.equal(found.length, 1)
  assert.equal(found[0].text, 'This real text has enough words.')
})

test('label presence or another actor cannot approve growth', () => {
  assert.equal(founderException([exceptionLabel], []), false)
  assert.equal(founderException([exceptionLabel], [event(1, 'another-maintainer')]), false)
  assert.equal(founderException([], [event(1, 'designerwho')]), false)
  assert.equal(founderException([exceptionLabel], [{ ...event(1, 'designerwho'), actor: { login: 'designerwho', type: 'Bot' } }]), false)
})

test('the currently applied label must have been applied by the founder', () => {
  assert.equal(founderException([{ name: exceptionLabel }], [event(1, 'designerwho')]), true)
  assert.equal(founderException([exceptionLabel], [event(1, 'designerwho'), event(2, 'designerwho', 'unlabeled')]), false)
  assert.equal(founderException([exceptionLabel], [event(3, 'another-maintainer'), event(1, 'designerwho'), event(2, 'designerwho', 'unlabeled')]), false)
  assert.equal(founderException([exceptionLabel], [event(3, 'designerwho'), event(1, 'another-maintainer'), event(2, 'another-maintainer', 'unlabeled')]), true)
})

test('issue events paginate until the founder label event is found', async () => {
  let calls = 0
  const events = await issueEvents('owner/repo', 10, undefined, async (path) => {
    calls++
    assert.match(path, new RegExp(`page=${calls}$`))
    return calls === 1 ? Array.from({ length: 100 }, (_, i) => ({ id: i, event: 'subscribed' })) : [event(101, 'designerwho')]
  })
  assert.equal(calls, 2)
  assert.equal(founderException([exceptionLabel], events), true)
})

test('an unreadable event history fails closed', async () => {
  await assert.rejects(issueEvents('owner/repo', 10, undefined, async () => { throw new Error('read denied') }), /read denied/)
})

test('real git snapshots use the merge base and detect a changed skill', () => {
  const cwd = mkdtempSync(join(tmpdir(), 'prose-policy-'))
  const git = (...args) => execFileSync('git', args, { cwd, encoding: 'utf8' }).trim()
  const put = (path, value) => { mkdirSync(join(cwd, path, '..'), { recursive: true }); writeFileSync(join(cwd, path), value) }
  try {
    git('init', '-q', '-b', 'main')
    git('config', 'user.name', 'Policy test')
    git('config', 'user.email', 'policy@example.invalid')
    put('skills/example/SKILL.md', '# Example\n')
    git('add', '.'); git('commit', '-qm', 'Initial')
    git('checkout', '-qb', 'candidate')
    put('skills/example/SKILL.md', '# Example\nAdded guidance here.\n')
    put('.codex-plugin/plugin.json', '{"description":"New manifest guidance."}')
    git('add', '.'); git('commit', '-qm', 'Candidate')
    const head = git('rev-parse', 'HEAD')
    git('checkout', '-q', 'main')
    put('docs/another.md', 'A base-only document.\n')
    git('add', '.'); git('commit', '-qm', 'Advance base')
    const result = inspectRepository(cwd, 'main', head)
    assert.equal(result.growth, true)
    assert.equal(result.textLines, 1)
    assert.equal(result.files.length, 2)
    assert.equal(result.manifestStrings, 1)
  } finally { rmSync(cwd, { recursive: true, force: true }) }
})


test('hook JSON text is covered and Rust escapes cannot hide sentence words', () => {
  assert.equal(compareTrees(new Map(), tree({ 'hooks/new.json': '{"prompt":"Decide the whole direction first."}\n' })).growth, true)
  assert.equal(rustSentences(String.raw`let s = "One\u{20}two\x20three four";`).length, 1)
  assert.equal(rustSentences(String.raw`let s = "One two       three four";`).length, 1)
  assert.throws(() => compareTrees(new Map(), tree({ 'hooks/binary.txt': 'hidden\0text' })), /binary text/)
})

test('rewording an existing skill line adds guidance without growing line count', () => {
  const result = compareTrees(tree({ 'skills/a.md': 'Keep the old instruction.\n' }), tree({ 'skills/a.md': 'Follow this replacement instruction.\n' }))
  assert.equal(result.growth, true)
  assert.equal(result.textLines, 1)
  assert.deepEqual(result.files[0].lines, [1])
})

test('lengthening an existing line adds guidance', () => {
  const result = compareTrees(tree({ 'commands/a.md': 'Keep context.\n' }), tree({ 'commands/a.md': 'Keep context. Ask a new question first.\n' }))
  assert.equal(result.growth, true)
  assert.equal(result.textLines, 1)
})

test('deletions in the same file or elsewhere never offset added guidance', () => {
  const before = tree({ 'skills/a.md': 'Old one.\nOld two.\nOld three.\n' })
  for (const path of ['skills/a.md', 'docs/new.txt']) {
    const result = compareTrees(before, tree({ [path]: 'New instruction.\n' }))
    assert.equal(result.growth, true, path)
    assert.equal(result.textLines, 1, path)
  }
})

test('a moved text line is free but an extra copy is an addition', () => {
  const before = tree({ 'agents/old.md': 'Keep this instruction.\n' })
  assert.equal(compareTrees(before, tree({ 'hooks/new.txt': 'Keep this instruction.\n' })).growth, false)
  const result = compareTrees(before, tree({ 'agents/old.md': 'Keep this instruction.\n', 'docs/copy.txt': 'Keep this instruction.\n' }))
  assert.equal(result.textLines, 1)
})

const manifests = ['.claude-plugin/plugin.json', '.claude-plugin/marketplace.json', '.codex-plugin/plugin.json', '.codex-plugin/marketplace.json', '.cursor-plugin/plugin.json', '.cursor-plugin/marketplace.json', '.agents/plugins/marketplace.json']

test('all plugin and marketplace manifests cover nested string fields and arrays', () => {
  for (const path of manifests) {
    for (const field of ['description', 'shortDescription', 'defaultPrompt', 'futureGuidance']) {
      const before = tree({ [path]: JSON.stringify({ plugins: [{ [field]: 'Old instruction.' }] }) })
      const after = tree({ [path]: JSON.stringify({ plugins: [{ [field]: 'New instruction.' }] }) })
      const result = compareTrees(before, after)
      assert.equal(result.growth, true, path + ':' + field)
      assert.equal(result.manifestStrings, 1)
      assert.deepEqual(result.files, [{ file: path, added: 1, removed: 0 }])
    }
  }
})

test('manifest formatting and moves pass while deletions cannot offset new strings', () => {
  const before = tree({ [manifests[0]]: '{"description":"Existing instruction.","prompts":["Delete this.","Delete that."]}' })
  assert.equal(compareTrees(before, tree({ [manifests[1]]: JSON.stringify(JSON.parse(before.get(manifests[0])), null, 2) })).growth, false)
  const result = compareTrees(before, tree({ [manifests[1]]: '{"description":"New instruction."}' }))
  assert.equal(result.growth, true)
  assert.equal(result.manifestStrings, 1)
  assert.equal(compareTrees(before, new Map()).growth, false)
})

test('manifest string multiplicity and category boundaries prevent text offsets', () => {
  const before = tree({ [manifests[0]]: '{"description":"Same instruction."}', 'skills/a.md': 'New instruction.\n' })
  const result = compareTrees(before, tree({ [manifests[0]]: '{"description":"Same instruction.","prompts":["Same instruction.","New instruction."]}' }))
  assert.equal(result.manifestStrings, 2)
  assert.equal(result.growth, true)
  assert.throws(() => compareTrees(new Map(), tree({ [manifests[0]]: '{bad json}' })), SyntaxError)
})
