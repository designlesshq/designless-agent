import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { pathToFileURL } from 'node:url'

export const exceptionLabel = 'critical-prose-approved'
const founder = 'designerwho'

function git(args, cwd) {
  return execFileSync('git', args, { cwd, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 })
}

const textPath = (p) => p === 'llms.txt' || p.startsWith('hooks/') || /^(skills|agents|commands|docs)\/.*\.(md|mdx|markdown|txt|text|rst|adoc)$/i.test(p)
const bridgePath = (p) => /^bridge\/src\/.*\.rs$/.test(p) && !/(^|\/)(tests?|.*[_.]tests?)\.rs$/.test(p) && !/(^|\/)(tests?)\//.test(p)
const lines = (s) => s.split(/\r?\n/).filter((line) => line.trim()).length

// Read Rust literals without mistaking comments, raw delimiters or escapes for code.
export function rustSentences(source) {
  const literals = []
  let masked = source.split('')
  const hide = (from, to) => {
    for (let n = from; n < to; n++) if (masked[n] !== '\n') masked[n] = ' '
  }
  for (let i = 0; i < source.length;) {
    const start = i
    if (source.startsWith('//', i)) {
      i = source.indexOf('\n', i)
      if (i < 0) i = source.length
      hide(start, i)
    } else if (source.startsWith('/*', i)) {
      let depth = 1
      i += 2
      while (i < source.length && depth) {
        if (source.startsWith('/*', i)) { depth++; i += 2 }
        else if (source.startsWith('*/', i)) { depth--; i += 2 }
        else i++
      }
      if (depth) throw new Error('Unclosed Rust comment')
      hide(start, i)
    } else {
      const raw = source.slice(i).match(/^(?:br|cr|r)(#*)"/)
      if (raw && (i === 0 || !/[\w]/.test(source[i - 1]))) {
        const end = source.indexOf('"' + raw[1], i + raw[0].length)
        if (end < 0) throw new Error('Unclosed raw Rust string')
        literals.push({ text: source.slice(i + raw[0].length, end), at: start })
        i = end + raw[1].length + 1
        hide(start, i)
      } else if (source[i] === '"') {
        let text = ''
        i++
        while (i < source.length && source[i] !== '"') {
          if (source[i] === '\\') {
            const next = source[++i]
            const escapes = { n: '\n', r: '\r', t: '\t', '0': '\0', "'": "'", '"': '"', '\\': '\\' }
            if (next in escapes) { text += escapes[next]; i++ }
            else if (next === 'x') {
              const hex = source.slice(i + 1, i + 3)
              if (!/^[a-f0-9]{2}$/i.test(hex)) throw new Error('Invalid Rust byte escape')
              text += String.fromCodePoint(parseInt(hex, 16)); i += 3
            } else if (next === 'u') {
              const unicode = source.slice(i).match(/^u\{([a-f0-9_]{1,12})\}/i)
              if (!unicode) throw new Error('Invalid Rust Unicode escape')
              const digits = unicode[1].replaceAll('_', '')
              if (!digits || digits.length > 6) throw new Error('Invalid Rust Unicode escape')
              text += String.fromCodePoint(parseInt(digits, 16)); i += unicode[0].length
            } else if (next === '\n' || (next === '\r' && source[i + 1] === '\n')) {
              while (/\s/.test(source[i] ?? '') && i < source.length) i++
            } else throw new Error('Unknown Rust string escape')
          } else text += source[i++]
        }
        if (i === source.length) throw new Error('Unclosed Rust string')
        i++
        literals.push({ text, at: start })
        hide(start, i)
      } else if (source[i] === "'" && /^'(?:\\.|[^'\\\n])'/.test(source.slice(i))) {
        const char = source.slice(i).match(/^'(?:\\.|[^'\\\n])'/)[0]
        i += char.length
        hide(start, i)
      } else i++
    }
  }
  const code = masked.join('')
  const testRanges = []
  const attr = /#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|(?:tokio::)?test)\s*\]/g
  for (const match of code.matchAll(attr)) {
    const open = code.indexOf('{', match.index + match[0].length)
    if (open < 0) throw new Error('Test attribute without a block')
    let depth = 1, end = open + 1
    while (end < code.length && depth) {
      if (code[end] === '{') depth++
      else if (code[end] === '}') depth--
      end++
    }
    testRanges.push([match.index, end])
  }
  return literals.filter(({ text, at }) => {
    if (testRanges.some(([start, end]) => at >= start && at < end)) return false
    const plain = text.replace(/\{[^{}]*\}/g, ' ').replace(/\{\{|\}\}/g, ' ')
    const words = plain.split(/\s+/).filter((word) => /\p{L}/u.test(word) && !/[_/\\]/.test(word))
    return words.length >= 4
  }).map(({ text, at }) => ({ text, line: source.slice(0, at).split('\n').length, count: lines(text) }))
}

export function compareTrees(before, after) {
  const files = []
  const oldSentences = new Map()
  let textAdded = 0, textRemoved = 0
  for (const [path, source] of before) {
    if (bridgePath(path)) for (const { text } of rustSentences(source)) oldSentences.set(text, (oldSentences.get(text) ?? 0) + 1)
  }
  // Unchanged literals cancel globally, so moving a sentence or file is free.
  const newSentences = []
  for (const [path, source] of after) {
    if (bridgePath(path)) for (const literal of rustSentences(source)) newSentences.push({ path, ...literal })
  }
  const bridgeAdded = new Map()
  for (const literal of newSentences) {
    const remaining = oldSentences.get(literal.text) ?? 0
    if (remaining) oldSentences.set(literal.text, remaining - 1)
    else {
      const found = bridgeAdded.get(literal.path) ?? { file: literal.path, added: 0, removed: 0, lines: [] }
      found.added += literal.count
      found.lines.push(literal.line)
      bridgeAdded.set(literal.path, found)
    }
  }
  for (const path of new Set([...before.keys(), ...after.keys()])) {
    if (!textPath(path)) continue
    if ((before.get(path) ?? '').includes('\0') || (after.get(path) ?? '').includes('\0')) throw new Error(`Cannot review binary text at ${JSON.stringify(path)}`)
    const delta = lines(after.get(path) ?? '') - lines(before.get(path) ?? '')
    textAdded += Math.max(delta, 0)
    textRemoved += Math.max(-delta, 0)
    if (delta) files.push({ file: path, added: Math.max(delta, 0), removed: Math.max(-delta, 0) })
  }
  files.push(...bridgeAdded.values())
  const bridgeLines = [...bridgeAdded.values()].reduce((sum, file) => sum + file.added, 0)
  return { files, textNet: Math.max(0, textAdded - textRemoved), bridgeLines, growth: textAdded > textRemoved || bridgeLines > 0 }
}

export function founderException(labels, events) {
  if (!labels.some((label) => (typeof label === 'string' ? label : label.name) === exceptionLabel)) return false
  const relevant = events.filter((event) => event.label?.name === exceptionLabel && ['labeled', 'unlabeled'].includes(event.event))
  relevant.sort((a, b) => a.id - b.id)
  const last = relevant.at(-1)
  return last?.event === 'labeled' && last.actor?.login === founder && last.actor?.type === 'User'
}

export function inspectRepository(cwd, base, head) {
  const mergeBase = git(['merge-base', base, head], cwd).trim()
  const readTree = (revision) => {
    const result = new Map()
    for (const path of git(['ls-tree', '-r', '--name-only', '-z', revision], cwd).split('\0').filter(Boolean)) {
      if (textPath(path) || bridgePath(path)) result.set(path, git(['show', `${revision}:${path}`], cwd))
    }
    return result
  }
  return { mergeBase, ...compareTrees(readTree(mergeBase), readTree(head)) }
}

async function githubJson(endpoint, token) {
  const response = await fetch(`https://api.github.com${endpoint}`, { headers: {
    Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28',
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  } })
  if (!response.ok) throw new Error(`GitHub read failed (${response.status})`)
  return response.json()
}

export async function issueEvents(repo, number, token, read = githubJson) {
  const all = []
  for (let page = 1; page <= 100; page++) {
    const entries = await read(`/repos/${repo}/issues/${number}/events?per_page=100&page=${page}`, token)
    if (!Array.isArray(entries)) throw new Error('Issue events were not an array')
    all.push(...entries)
    if (entries.length < 100) return all
  }
  throw new Error('Issue event history exceeds the verification limit')
}

export async function main() {
  const event = JSON.parse(readFileSync(process.env.GITHUB_EVENT_PATH, 'utf8'))
  const repo = process.env.GITHUB_REPOSITORY
  const number = event.pull_request?.number ?? event.number
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo ?? '') || !Number.isSafeInteger(number) || number < 1) throw new Error('A pull request is required')
  const token = process.env.GH_TOKEN
  const pr = await githubJson(`/repos/${repo}/pulls/${number}`, token)
  const expected = event.pull_request?.head?.sha
  if (!expected || pr.head.sha !== expected) throw new Error('The pull request changed; run the check for its current head')
  for (const sha of [pr.base.sha, pr.head.sha]) if (!/^[a-f0-9]{40}$/.test(sha)) throw new Error('Invalid commit id')
  git(['fetch', '--no-tags', 'origin', pr.base.sha], process.cwd())
  git(['fetch', '--no-tags', 'origin', `refs/pull/${number}/head`], process.cwd())
  if (git(['rev-parse', 'FETCH_HEAD'], process.cwd()).trim() !== expected) throw new Error('The pull request changed during the check')
  const report = inspectRepository(process.cwd(), pr.base.sha, pr.head.sha)
  console.log(`Agent-facing text: ${report.textNet} net text lines, ${report.bridgeLines} new or changed bridge sentence lines.`)
  for (const file of report.files) console.log(`${JSON.stringify(file.file)}: +${file.added} -${file.removed}${file.lines ? ` (source lines ${file.lines.join(', ')})` : ''}`)
  if (!report.growth) { console.log('No agent-facing text growth.'); return }
  const events = await issueEvents(repo, number, token)
  if (founderException(pr.labels, events)) { console.log(`Critical exception: ${exceptionLabel} applied by ${founder}.`); return }
  throw new Error(`Agent guidance stays on the server. Remove the added text, or ask ${founder} to apply ${exceptionLabel} to this pull request.`)
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => { console.error(error.message); process.exitCode = 1 })
}
