#!/usr/bin/env node
// Plugin tree attestation — the tooling twin of bridge/src/integrity.rs.
//
// Computes the same root hash the bridge sends on every MCP frame
// (`x-designless-plugin-integrity`). The release tooling uses this to publish
// the expected hash for each release, and cross-checks it against the real
// binary's `--integrity` output before publishing, so the two implementations
// cannot drift apart unnoticed.
//
// Usage: node bin/tree-hash.mjs [plugin-root]   (defaults to this file's ../)

import { createHash } from 'node:crypto'
import { readdirSync, readFileSync, readlinkSync, lstatSync, existsSync } from 'node:fs'
import { join, dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Directories WE own, attested wholesale. A host's own namespace (.agents,
// .claude-plugin, .codex-plugin, .cursor-plugin) is deliberately absent: hosts
// write their own files there at install time, and attesting a directory we do
// not own would read that ordinary work as a modified tree. The files we ship
// inside those namespaces are attested by name below.
// Mirrors bridge/src/integrity.rs — the two must stay identical.
const INCLUDE_DIRS = [
  'agents', 'bin', 'capsules', 'commands', 'docs', 'hooks', 'skills',
]
const INCLUDE_FILES = [
  '.agents/plugins/marketplace.json',
  '.claude-plugin/marketplace.json', '.claude-plugin/plugin.json',
  '.codex-plugin/plugin.json',
  '.cursor-plugin/marketplace.json', '.cursor-plugin/plugin.json',
  '.gitignore', '.mcp.json', '.mcp.codex.json', '.mcp.cursor.json',
  'LICENSE', 'README.md', 'THIRD-PARTY-LICENSES.md', 'llms.txt',
]
const STAMPED_FILE = '.mcp.cursor.json'

const sha256hex = (data) => createHash('sha256').update(data).digest('hex')

function fileHash(root, path, rel) {
  if (rel === STAMPED_FILE) {
    // Normalize the Cursor installer's absolute-path stamp back to the
    // template form so stamped and shipped trees hash identically.
    const text = readFileSync(path, 'utf8').split(root).join('.')
    return sha256hex(text)
  }
  return sha256hex(readFileSync(path))
}

function walk(root, dir, out) {
  for (const name of readdirSync(dir)) {
    if (name === '.DS_Store') continue
    const path = join(dir, name)
    const rel = path.slice(root.length + 1).split('\\').join('/')
    const st = lstatSync(path)
    if (st.isSymbolicLink()) {
      out.push([rel, sha256hex('symlink:' + readlinkSync(path))])
    } else if (st.isDirectory()) {
      walk(root, path, out)
    } else {
      out.push([rel, fileHash(root, path, rel)])
    }
  }
}

export function computeTreeHash(root) {
  root = resolve(root)
  const entries = []
  for (const dir of INCLUDE_DIRS) {
    const base = join(root, dir)
    if (existsSync(base) && lstatSync(base).isDirectory()) walk(root, base, entries)
  }
  for (const file of INCLUDE_FILES) {
    const path = join(root, file)
    if (existsSync(path) && lstatSync(path).isFile()) {
      entries.push([file, fileHash(root, path, file)])
    }
  }
  entries.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
  const hasher = createHash('sha256')
  for (const [rel, hash] of entries) {
    hasher.update(rel)
    hasher.update(Buffer.from([0]))
    hasher.update(hash)
    hasher.update('\n')
  }
  return hasher.digest('hex')
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), '..')
  console.log(computeTreeHash(root))
}
