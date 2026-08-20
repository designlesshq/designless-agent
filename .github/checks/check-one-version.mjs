#!/usr/bin/env node
// One version, every door.
//
// Designless installs into more than one editor — Claude Code, the ChatGPT
// app (Codex), Cursor — and each door carries its own manifest with its own
// version field, plus the orchestrator skill that tells the agent which
// version it is running. A release is only a release when every one of those
// numbers is the same number.
//
// This check makes that a build-time guarantee instead of a habit:
//   1. Discover every version-bearing file STRUCTURALLY (any plugin.json /
//      marketplace.json under a *-plugin directory, plus the orchestrator
//      skill's frontmatter) — a future fourth door joins the parity set the
//      moment its directory exists, with no edit here.
//   2. Assert they all agree with .claude-plugin/plugin.json.
//   3. When CI runs on a version tag (vX.Y.Z), assert the tag names that
//      same version — a tag that disagrees with the tree it points at is a
//      release that never happened.
//
// Node built-ins only. Exit 1 with a table of disagreements; exit 0 quietly.

import fs from 'node:fs'
import path from 'node:path'

const root = process.cwd()
const canonicalPath = '.claude-plugin/plugin.json'
const canonical = JSON.parse(fs.readFileSync(path.join(root, canonicalPath), 'utf8')).version

if (typeof canonical !== 'string' || !/^\d+\.\d+\.\d+$/.test(canonical)) {
  console.error(`✗ ${canonicalPath} carries no well-formed version (got: ${JSON.stringify(canonical)})`)
  process.exit(1)
}

const findings = [] // { file, version }

// 1a. Every manifest under a *-plugin directory.
for (const entry of fs.readdirSync(root, { withFileTypes: true })) {
  if (!entry.isDirectory() || !entry.name.endsWith('-plugin')) continue
  for (const name of ['plugin.json', 'marketplace.json']) {
    const p = path.join(entry.name, name)
    if (!fs.existsSync(path.join(root, p))) continue
    const doc = JSON.parse(fs.readFileSync(path.join(root, p), 'utf8'))
    // A manifest may carry the version at the top level or on its plugin
    // entries (marketplace shape) — collect every occurrence.
    const versions = []
    if (typeof doc.version === 'string') versions.push(doc.version)
    for (const plugin of Array.isArray(doc.plugins) ? doc.plugins : []) {
      if (typeof plugin?.version === 'string') versions.push(plugin.version)
    }
    for (const v of versions) findings.push({ file: p, version: v })
  }
}

// 1b. The orchestrator skill's frontmatter — the number the agent believes.
const skillPath = 'skills/orchestrator/SKILL.md'
const skill = fs.readFileSync(path.join(root, skillPath), 'utf8')
const fm = skill.match(/^plugin_version:\s*"([^"]+)"/m)
findings.push({ file: `${skillPath} (frontmatter)`, version: fm ? fm[1] : '(missing)' })

// 2. Parity.
const off = findings.filter((f) => f.version !== canonical)
if (off.length > 0) {
  console.error(`✗ One version, every door: ${canonicalPath} says ${canonical}, but:`)
  for (const f of off) console.error(`    ${f.file} says ${f.version}`)
  console.error('  A release is only a release when every manifest ships the same number.')
  process.exit(1)
}

// 3. Tag parity, when CI is building a version tag.
const ref = process.env.GITHUB_REF ?? ''
const tagMatch = ref.match(/^refs\/tags\/v(\d+\.\d+\.\d+)$/)
if (tagMatch && tagMatch[1] !== canonical) {
  console.error(`✗ Tag v${tagMatch[1]} disagrees with the tree it points at (${canonical}).`)
  process.exit(1)
}

console.log(`✓ One version, every door: ${canonical} across ${findings.length + 1} manifests${tagMatch ? ` (tag v${tagMatch[1]} agrees)` : ''}`)
