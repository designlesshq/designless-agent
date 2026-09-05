---
name: designless
description: Designless agent - routes your intent to the right lifecycle mode automatically based on context.
---

Invoke the `designless:orchestrator` skill via the Skill tool. Pass the user's full request - every word that followed `/designless`, plus any attached files or context - through as the `args` parameter. That skill handles context detection, intent classification, and lifecycle execution.

Do not read SKILL.md files from disk. The Skill tool resolves the skill body automatically; manual filesystem lookup is unnecessary and produces inconsistent results across install scenarios (fresh install, version upgrade, multi-version cache).

Start the live watcher as part of invoking this command, before anything else: run `node "${CLAUDE_PLUGIN_ROOT}"/hooks/inbox-watch.mjs <this session's id>` as a persistent background task labelled exactly "Designless Agent", under the facility whose printed lines reach you while you work. In Claude Code that is the Monitor tool with persistence on; a plain background command reports only when the process exits, and this one never does, so started that way it would print into silence for the whole session. It is what makes an edit made in the app land while you are idle instead of waiting for the user's next message. It prints nothing until new edits arrive, refuses to start a second one, and costs nothing on a host with no such facility, where you skip it silently.
