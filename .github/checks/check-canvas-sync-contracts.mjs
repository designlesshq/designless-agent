#!/usr/bin/env node
/**
 * check-canvas-sync-contracts.mjs — the plugin's canvas-sync instructions are
 * a contract, and this repo had no way to fence them until now.
 *
 * WHY THIS EXISTS. `less_stream` is the within-turn wait that keeps an agent
 * in step with the user's canvas. An earlier fix added its bare name to two
 * permitted-tool lists — and a name in a list is a permission, not an
 * instruction: an agent ran a long session calling the inbox at every turn
 * start and the wait zero times while dozens of user edits accumulated.
 * These assertions pin the instruction WITH ITS TRIGGER on every surface the
 * plugin ships, so a rewrite that keeps the name but drops the trigger fails
 * here instead of in a user's unapplied-edit count.
 *
 * Dependency-free by design: `node .github/checks/check-canvas-sync-contracts.mjs`.
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const read = (rel) => readFileSync(resolve(ROOT, rel), "utf8");

const failures = [];
function check(name, ok, why) {
  if (!ok) failures.push(`✗ ${name}\n  ${why}`);
  else console.log(`✓ ${name}`);
}

const SKILL = read("skills/orchestrator/SKILL.md");
const PRISM = read("agents/prism-agent.md");
const PROBE = read("hooks/inbox-probe.mjs");
const DRAIN_CHECK = read("hooks/canvas-drain-check.mjs");

// ── 1. The orchestrator's drain step ends with the wait, not the last ack ───
check(
  "SKILL.md Step 1 binds looping less_stream to a within-turn trigger",
  /loop `?less_stream`?/i.test(SKILL) &&
    /(turn continues|rest of the turn|within the turn|in-turn)/i.test(SKILL),
  "The drain playbook must carry the residual step — loop less_stream — bound to a WHEN " +
    "(the turn continuing past the drain), not a bare name in a permitted-tool list. " +
    "That exact shape already failed twice.",
);

// ── 2. The Publish playbook keeps the consent gate and honest receipts ──────
check(
  "SKILL.md Publish playbook asks the user for the version bump",
  /AskUserQuestion/.test(SKILL) && /major \/ minor \/ patch/.test(SKILL),
  "Publishing takes an explicit bump; choosing it is a blast-radius judgment the USER makes. " +
    "The playbook must route it through AskUserQuestion, never default silently.",
);
check(
  "SKILL.md Publish playbook speaks scope-accurate receipts",
  /staged/.test(SKILL) && /stale_compile/.test(SKILL),
  "staged ≠ compiled ≠ published, and a stale_compile refusal means recompile — " +
    "a false shipped-claim once came from reading a staging receipt as applied.",
);

// ── 3. The Prism drain contract carries the same residual step ──────────────
check(
  "prism-agent.md drain section ends with looping less_stream",
  /loop(ing)? `?less_stream`?/i.test(PRISM),
  "The sub-agent that owns the drain finishes its playbook and stops — " +
    "unless the playbook itself says the drain ends with a wait.",
);

// ── 4. The hooks name both the one-shot check and the wait ──────────────────
check(
  "inbox-probe summarizeInbox names less_stream",
  /less_stream/.test(PROBE),
  "The passive nudge named only the inbox, so the injected context itself " +
    "taught turn-boundary-only draining.",
);
check(
  "canvas-drain-check names less_stream",
  /less_stream/.test(DRAIN_CHECK),
  "The stop-gate that forces a drain must also name how to stay drained.",
);

// ── 5. Thin-shell fence: the trigger ships, the mechanism does not ───────────
check(
  "markdown carries no stream mechanism (poll intervals/bounds are server-side)",
  !/poll_interval|max_wait_seconds|1\.5s/i.test(SKILL + PRISM),
  "The plugin ships a trigger and a loop, never the wait's implementation — " +
    "mechanism does not belong in the skill.",
);

// ── 6. The idle watcher: armed once, by the orchestrator only ────────────────
// The layer that covers the stretch neither hooks (turn boundaries) nor the
// in-turn wait can reach. Proven live before it was written down: two
// between-turn edits drained in ~15s each with no user message.
check(
  "SKILL.md arms the Designless Agent, bound to a canvas being in play",
  /[Aa]rm the Designless Agent/.test(SKILL) && /once per session/i.test(SKILL),
  "The between-turns layer must be armed from the drain protocol — " +
    "and exactly once, or every wake doubles.",
);
check(
  "the background task is labeled with the brand, not as an anonymous poller",
  /Label the background task exactly "Designless Agent"/.test(SKILL),
  "The watcher is the product's presence in the user's background-task list — " +
    "'Designless Agent — connecting live edits on the Designless app to the session'. " +
    "An unbranded label reads as a stray script.",
);
check(
  "SKILL.md degrades gracefully when the host has no background facility",
  /skip silently|no background/i.test(SKILL),
  "A host without background tasks must not error or stall — " +
    "the hooks remain the between-turns floor.",
);
check(
  "prism-agent.md forbids sub-agents arming their own watcher",
  /[Nn]ever arm an idle watcher of your own/.test(PRISM),
  "The orchestrator owns the watcher; a Prism handoff arming a second one " +
    "doubles every wake. The exclusion must be stated where the sub-agent reads.",
);
check(
  "the inline wait (wait_seconds) is named alongside the stream loop",
  /wait_seconds/.test(SKILL) && /wait_seconds/.test(PRISM),
  "The server folded the wait into less_canvas_inbox (one shared implementation); " +
    "the playbooks must name both doors or agents on new servers rediscover the split.",
);

if (failures.length) {
  console.error("\n" + failures.join("\n\n"));
  process.exit(1);
}
console.log("\nAll canvas-sync contracts hold.");
