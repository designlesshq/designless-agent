---
name: designless
description: Designless agent orchestrator. Connects your agents to your brand's design intelligence and an agent-native canvas, so what they build carries your context and your brand.
---

Invoke the `designless:orchestrator` skill via the Skill tool. Pass the user's full request - every word that followed `/designless`, plus any attached files or context - through as the `args` parameter. That skill handles context detection, intent classification, and lifecycle execution.

Do not read SKILL.md files from disk. The Skill tool resolves the skill body automatically; manual filesystem lookup is unnecessary and produces inconsistent results across install scenarios (fresh install, version upgrade, multi-version cache).

Start the live watcher as part of invoking this command, before anything else: run `/bin/sh "${CLAUDE_PLUGIN_ROOT}"/bin/designless inbox-watch --once <this session's id>` as a background command described exactly "Designless Agent", under a facility that tells you when the process exits. In Claude Code that is Bash with `run_in_background`, not the Monitor tool, whose thirty-minute cap woke the agent at every expiry. It is what makes an edit made in the app land while you are idle instead of waiting for the user's next message. It costs nothing while it waits, prints one line and exits when new edits arrive (apply them, then start it again the same way), refuses to start a second one, and is skipped silently on a host with no such facility.
