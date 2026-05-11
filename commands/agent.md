---
name: designless
description: Designless agent — routes your intent to the right lifecycle mode automatically based on context.
---

Invoke the `designless:orchestrator` skill via the Skill tool. Pass the user's full request — every word that followed `/designless`, plus any attached files or context — through as the `args` parameter. That skill handles context detection, intent classification, and lifecycle execution.

Do not read SKILL.md files from disk. The Skill tool resolves the skill body automatically; manual filesystem lookup is unnecessary and produces inconsistent results across install scenarios (fresh install, version upgrade, multi-version cache).
