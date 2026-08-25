#!/usr/bin/env node
/**
 * check-integrity-lists.mjs — the attested file set is a contract with itself.
 *
 * WHY THIS EXISTS. The integrity hasher is implemented TWICE: once in Rust
 * (bridge/src/integrity.rs, which every customer runs) and once in Node
 * (bin/tree-hash.mjs, which publishes the expected hash at release). If the
 * two lists drift apart, release.mjs registers a hash no shipped binary can
 * produce and every install is refused before auth — the failure mode where
 * the person does not see an error, they see the product's tools vanish.
 * release.mjs cross-checks the two at release time; that is the last possible
 * moment to find out. This finds out on the pull request.
 *
 * The second assertion is newer and guards the shape the lists took after the
 * Codex fix. Host-owned namespaces (.agents, .claude-plugin, .codex-plugin,
 * .cursor-plugin) are no longer attested WHOLESALE, because the host writes
 * its own files into its own namespace — Codex generates a migrated skill
 * under .codex-plugin/ at install time, formatted by ITS template, so the
 * bytes depend on the Codex version and no release of ours can pin them.
 * Attesting the directory turned that routine work into what looked like
 * tampering.
 *
 * The cost of an allow-list is that it can silently stop covering something:
 * add .claude-plugin/some-new.json, forget the list, and it ships unattested
 * with nothing failing. So: every TRACKED file in those namespaces must be
 * named. Tracked, deliberately — a host-generated file is untracked, which is
 * exactly the distinction the fix is built on.
 *
 * Dependency-free: `node .github/checks/check-integrity-lists.mjs`.
 */

import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (rel) => readFileSync(resolve(ROOT, rel), "utf8");

const failures = [];
function check(name, ok, why) {
  if (!ok) failures.push(`✗ ${name}\n  ${why}`);
  else console.log(`✓ ${name}`);
}

/**
 * Pull a string-literal list out of either language, given the exact opening
 * text of the ASSIGNMENT. Anchoring on the assignment matters: `const
 * INCLUDE_DIRS: &[&str] = &[` contains a `[` in its TYPE, and a naive
 * "first bracket after the name" reads the empty type slice instead of the
 * value — which parses as an empty list and makes every later assertion pass
 * vacuously. That is exactly what this function did on its first run.
 */
function listFrom(src, marker) {
  const at = src.indexOf(marker);
  if (at < 0) return null;
  const open = at + marker.length;
  const close = src.indexOf("]", open);
  if (close < 0) return null;
  const items = [...src.slice(open, close).matchAll(/['"]([^'"]+)['"]/g)].map((m) => m[1]).sort();
  // A list that parses to nothing is a parse failure, not an empty list. Fail
  // loudly rather than silently asserting nothing.
  return items.length ? items : null;
}

const RS = read("bridge/src/integrity.rs");
const JS = read("bin/tree-hash.mjs");

const HOST_NAMESPACES = [".agents", ".claude-plugin", ".codex-plugin", ".cursor-plugin"];

// ── 1. The two implementations attest the same set ─────────────────────────
for (const list of ["INCLUDE_DIRS", "INCLUDE_FILES"]) {
  const rs = listFrom(RS, `const ${list}: &[&str] = &[`);
  const js = listFrom(JS, `const ${list} = [`);
  check(
    `${list} is identical in Rust and Node`,
    rs !== null && js !== null && JSON.stringify(rs) === JSON.stringify(js),
    `bridge/src/integrity.rs has ${JSON.stringify(rs)}\n  bin/tree-hash.mjs has ${JSON.stringify(js)}\n` +
      `  A hash the shipped binary cannot reproduce refuses every install before auth.`,
  );
}

const DIRS = listFrom(RS, "const INCLUDE_DIRS: &[&str] = &[");
const FILES = listFrom(RS, "const INCLUDE_FILES: &[&str] = &[");
if (!DIRS || !FILES) {
  console.error("could not parse INCLUDE_DIRS / INCLUDE_FILES from bridge/src/integrity.rs");
  process.exit(1);
}

// ── 2. A host's namespace is never attested wholesale ──────────────────────
for (const ns of HOST_NAMESPACES) {
  check(
    `${ns} is not attested wholesale`,
    !DIRS.includes(ns),
    `${ns} is a directory the HOST writes into. Attesting it wholesale means a file ` +
      `the host generates at install time reads as tampering, which is what refused ` +
      `every Codex session. List the files we ship there in INCLUDE_FILES instead.`,
  );
}

// ── 3. …but every file we SHIP there is still named ────────────────────────
const tracked = execFileSync("git", ["ls-files", "-z", ...HOST_NAMESPACES], {
  cwd: ROOT,
  encoding: "utf8",
})
  .split("\0")
  .filter(Boolean);

const missing = tracked.filter((f) => !FILES.includes(f));
check(
  "every tracked file in a host namespace is attested by name",
  missing.length === 0,
  `unattested: ${missing.join(", ")}\n` +
    `  Add them to INCLUDE_FILES in BOTH bridge/src/integrity.rs and bin/tree-hash.mjs. ` +
    `An allow-list that forgets a file ships it unattested and nothing else will say so.`,
);

// A listed file that no longer exists is dead weight, and dead weight in an
// allow-list is how the list stops being read.
const stale = FILES.filter((f) => HOST_NAMESPACES.some((ns) => f.startsWith(`${ns}/`)) && !tracked.includes(f));
check(
  "no attested file in a host namespace has been deleted",
  stale.length === 0,
  `listed but not tracked: ${stale.join(", ")}`,
);

if (failures.length) {
  console.error(`\n${failures.length} integrity-list contract failure(s):\n`);
  for (const f of failures) console.error(f + "\n");
  process.exit(1);
}
console.log("\nintegrity lists agree, and cover everything we ship.");
