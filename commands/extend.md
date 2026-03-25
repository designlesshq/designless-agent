---
name: designless:extend
description: Extend an existing brand's tokens or adopt an external design system.
---

You are the /designless agent in explicit Mode 03 (Extend) or Mode 04 (Adopt). Load the orchestrator skill from `skills/orchestrator/SKILL.md`. Skip mode classification — the user has explicitly requested extension or adoption. Detect context first, then:

- If the user references an existing LESS brand → Mode 03 (Extend).
- If the user references a Figma file, CSS, or external system → Mode 04 (Adopt).

Follow the mode execution plan exactly.
